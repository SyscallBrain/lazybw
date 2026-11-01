//! Lifecycle of the `bw serve` child process.
//!
//! The default transport is a socketpair whose child end is fd 3
//! (`fd+connected://3`), so no TCP port and no filesystem socket exist. The
//! alternative `unix://` transport binds a socket in the private runtime
//! directory. Either way the child gets `PR_SET_PDEATHSIG` and
//! `kill_on_drop`, and its stderr is kept (last 20 lines, redacted) for the
//! error screen.

use crate::bw::client::BwClient;
use crate::bw::envelope::BwError;
use crate::bw::models::Status;
use crate::config::Transport;
use command_fds::{CommandFdExt, FdMapping};
use std::collections::VecDeque;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

/// How the child was attached.
#[derive(Debug, Clone)]
pub enum TransportState {
    /// A socketpair end inherited as fd 3.
    Fd,
    /// A socket file in the runtime directory.
    Unix(PathBuf),
}

/// A spawned `bw serve`, not yet connected.
#[derive(Debug)]
pub struct Serve {
    /// Child process.
    pub child: Child,
    /// Transport used.
    pub transport: TransportState,
    /// Our end of the socketpair, when using `fd`.
    stream: Option<UnixStream>,
    /// Last stderr lines, for the error screen.
    pub stderr: Arc<Mutex<VecDeque<String>>>,
    /// The resolved executable path, for logs.
    pub path: PathBuf,
}

/// Parameters for spawning the child.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Path to the `bw` executable.
    pub path: PathBuf,
    /// Session key passed as `BW_SESSION` in the child environment.
    pub session_key: Option<String>,
    /// Transport.
    pub transport: Transport,
    /// Directory for the `unix://` socket (already private).
    pub runtime_dir: PathBuf,
}

impl Serve {
    /// Spawns `bw serve`. Readiness is checked by [`Serve::connect`].
    pub fn spawn(options: &ServeOptions) -> Result<Self, BwError> {
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let (mut command, transport_state, stream, socket_path) = match options.transport {
            Transport::Fd => {
                let (ours, theirs) = std::os::unix::net::UnixStream::pair()
                    .map_err(|e| BwError::InvalidResponse(format!("socketpair failed: {e}")))?;
                ours.set_nonblocking(true).ok();
                let tokio_stream = UnixStream::from_std(ours)
                    .map_err(|e| BwError::InvalidResponse(format!("socket failed: {e}")))?;
                let mut command = base_command(options);
                command
                    .args(["serve", "--hostname", "fd+connected://3"])
                    .fd_mappings(vec![FdMapping { parent_fd: OwnedFd::from(theirs), child_fd: 3 }])
                    .map_err(|e| BwError::InvalidResponse(format!("fd mapping failed: {e}")))?;
                (command, TransportState::Fd, Some(tokio_stream), None)
            }
            Transport::Unix => {
                let path = options.runtime_dir.join(format!("bw-{}.sock", std::process::id()));
                let _ = std::fs::remove_file(&path);
                let mut command = base_command(options);
                command.args(["serve", "--hostname", &format!("unix://{}", path.display())]);
                (command, TransportState::Unix(path.clone()), None, Some(path))
            }
        };
        configure_child(&mut command, options)?;
        let mut child = command
            .spawn()
            .map_err(|e| BwError::InvalidResponse(format!("could not start bw serve: {e}")))?;
        if let Some(pipe) = child.stderr.take() {
            pipe_reader(pipe, Arc::clone(&stderr), "bw serve stderr");
        }
        if let Some(pipe) = child.stdout.take() {
            pipe_reader(pipe, Arc::clone(&stderr), "bw serve stdout");
        }
        let serve =
            Self { child, transport: transport_state, stream, stderr, path: options.path.clone() };
        if let Some(path) = socket_path {
            tracing::debug!(path = %path.display(), "spawned bw serve over unix socket");
        }
        Ok(serve)
    }

    /// Opens the HTTP connection and waits for `GET /status` to answer.
    pub async fn connect(&mut self) -> Result<BwClient, BwError> {
        let stream = match &self.transport {
            TransportState::Fd => self.stream.take().ok_or(BwError::Disconnected)?,
            TransportState::Unix(path) => {
                let mut delay = Duration::from_millis(100);
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    match UnixStream::connect(path).await {
                        Ok(stream) => break stream,
                        Err(err) if Instant::now() < deadline => {
                            if let Ok(Some(status)) = self.child.try_wait() {
                                return Err(BwError::Other(format!(
                                    "bw serve exited with {status}"
                                )));
                            }
                            let _ = err;
                            tokio::time::sleep(delay).await;
                            delay = delay.mul_f32(1.5).min(Duration::from_millis(1000));
                        }
                        Err(err) => {
                            return Err(BwError::Other(format!(
                                "could not connect to bw serve: {err}"
                            )));
                        }
                    }
                }
            }
        };
        let mut client = BwClient::handshake(stream).await?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut delay = Duration::from_millis(100);
        loop {
            match client.status().await {
                Ok(status) => {
                    if status.is_unauthenticated() {
                        return Err(BwError::NotLoggedIn);
                    }
                    return Ok(client);
                }
                Err(err) => {
                    if let Ok(Some(status)) = self.child.try_wait() {
                        return Err(BwError::Other(format!(
                            "bw serve exited with {status}: {}",
                            self.stderr_tail()
                        )));
                    }
                    if Instant::now() >= deadline {
                        return Err(err);
                    }
                    tokio::time::sleep(delay).await;
                    delay = delay.mul_f32(1.5).min(Duration::from_millis(1000));
                    // The client may have lost the connection while the server
                    // attached; reconnect is not possible on fd, so keep using it.
                }
            }
        }
    }

    /// The last 20 stderr lines, joined.
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        // The stderr tail is diagnostics only; a poisoned lock still holds
        // whole lines, so it is read anyway.
        let guard = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        guard.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    /// The child process id, for tests and logs.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// True when the child has exited.
    pub fn has_exited(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Stops the child: close the socket, wait 2 s, SIGTERM, wait 3 s, SIGKILL.
    pub async fn shutdown(mut self) {
        drop(self.stream.take());
        if let Some(path) = match &self.transport {
            TransportState::Unix(p) => Some(p.clone()),
            TransportState::Fd => None,
        } {
            let _ = std::fs::remove_file(path);
        }
        if tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await.is_ok() {
            return;
        }
        if let Some(pid) = self.child.id() {
            terminate_child(pid);
        }
        if tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await.is_err() {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        }
    }
}

impl Drop for Serve {
    fn drop(&mut self) {
        // The child is killed on drop through `kill_on_drop`; this is only a
        // second line of defence for normal early returns.
        if let Some(_stream) = self.stream.take() {
            tracing::trace!("closing the serve socket");
        }
    }
}

fn base_command(options: &ServeOptions) -> Command {
    let mut command = Command::new(&options.path);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    command
}

fn configure_child(command: &mut Command, options: &ServeOptions) -> Result<(), BwError> {
    command.env_clear();
    for (key, value) in crate::bw::subprocess::child_env_with(options.session_key.as_deref()) {
        command.env(key, value);
    }
    // SAFETY: the closure only calls prctl, which is async-signal-safe and
    // cannot allocate.
    #[expect(unsafe_code, reason = "pre_exec is unsafe by definition, see SAFETY above")]
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
            Ok(())
        });
    }
    Ok(())
}

#[expect(unsafe_code, reason = "libc::kill is the only way to signal the child")]
fn terminate_child(pid: u32) {
    // SAFETY: kill(2) with a pid we own and SIGTERM has no memory effects.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

fn pipe_reader<R>(pipe: R, sink: Arc<Mutex<VecDeque<String>>>, label: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(pipe).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Redact once, before the line reaches the log or the buffer.
            let line = redact(&line);
            tracing::debug!(target: "bw_serve_stderr", "{line}");
            // Poisoning is benign here: a panic mid-push leaves at worst one
            // missing diagnostic line.
            let mut guard = sink.lock().unwrap_or_else(|e| e.into_inner());
            guard.push_back(format!("{label}: {line}"));
            if guard.len() > 20 {
                guard.pop_front();
            }
        }
    });
}

/// Removes anything that could be a secret from a log line.
#[must_use]
pub fn redact(line: &str) -> String {
    let mut out = line.to_owned();
    for marker in ["password", "session", "secret", "token"] {
        if let Some(index) = out.to_ascii_lowercase().find(marker)
            && let Some(colon) = out[index..].find(':')
        {
            let cut = index + colon + 1;
            out.truncate(cut);
            out.push_str(" ***");
        }
    }
    out
}

/// Waits until the child's `/status` reports the given predicate, with the
/// readiness backoff used on startup.
pub async fn wait_for_status(
    client: &mut BwClient,
    predicate: impl Fn(&Status) -> bool,
    timeout: Duration,
) -> Result<Status, BwError> {
    let deadline = Instant::now() + timeout;
    let mut delay = Duration::from_millis(100);
    loop {
        match client.status().await {
            Ok(status) if predicate(&status) => return Ok(status),
            Ok(status) => {
                if Instant::now() >= deadline {
                    return Ok(status);
                }
            }
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(err);
                }
            }
        }
        tokio::time::sleep(delay).await;
        delay = delay.mul_f32(1.5).min(Duration::from_millis(1000));
    }
}

/// Removes a stale socket file, reporting errors that matter.
pub fn clean_socket(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_lines() {
        let line = "session key: abcdef123456";
        let redacted = redact(line);
        assert!(!redacted.contains("abcdef"));
        assert!(redacted.contains("***"));
        assert_eq!(redact("normal warning"), "normal warning");
    }
}
