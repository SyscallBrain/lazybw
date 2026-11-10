//! A PTY driver for `bw` commands that prompt for a password.
//!
//! `bw export --format encrypted_json --password` asks `Export file password:`
//! on the terminal. Driving it through a PTY keeps the password out of argv
//! and out of the process environment.

use crate::bw::envelope::BwError;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Runs `command` in a PTY, waits for a prompt matching `prompt_hint`
/// (case-insensitive substring), writes the password, and waits for exit.
///
/// Returns the transcript produced *before* the password was sent, which is
/// safe to log. Setting `cancel` kills the command within a quarter of a
/// second (a lock while an export runs).
pub fn run_with_password_prompt(
    command: &Path,
    args: &[String],
    env: &[(OsString, OsString)],
    password: &str,
    prompt_hint: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, BwError> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| BwError::Other(format!("could not open a PTY: {e}")))?;
    let mut builder = CommandBuilder::new(command);
    for arg in args {
        builder.arg(arg);
    }
    for (key, value) in env {
        builder.env(key, value);
    }
    let mut child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| BwError::Other(format!("could not start bw in a PTY: {e}")))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| BwError::Other(format!("could not read the PTY: {e}")))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| BwError::Other(format!("could not write to the PTY: {e}")))?;

    let (tx, rx): (_, Receiver<Vec<u8>>) = channel();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut transcript = String::new();
    let mut sent = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BwError::Other("stopped: the vault was locked".to_owned()));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(BwError::Other(
                "bw did not ask for the export password in time".to_owned(),
            ));
        }
        let remaining =
            deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(250));
        match rx.recv_timeout(remaining) {
            Ok(chunk) => {
                let text = String::from_utf8_lossy(&chunk).to_string();
                if !sent {
                    transcript.push_str(&text);
                    if transcript.to_ascii_lowercase().contains(&prompt_hint.to_ascii_lowercase()) {
                        writer
                            .write_all(password.as_bytes())
                            .and_then(|()| writer.write_all(b"\r"))
                            .and_then(|()| writer.flush())
                            .map_err(|e| {
                                BwError::Other(format!("could not answer the PTY prompt: {e}"))
                            })?;
                        sent = true;
                        transcript.clear();
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if sent && let Ok(Some(status)) = child.try_wait() {
            if !status.success() {
                return Err(BwError::Other(format!(
                    "bw exited with {status} while writing the export"
                )));
            }
            break;
        }
    }
    let status = child.wait().map_err(|e| BwError::Other(format!("could not wait for bw: {e}")))?;
    if !status.success() {
        return Err(BwError::Other(format!("bw exited with {status}")));
    }
    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn answers_a_password_prompt() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("prompt.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'Export file password: '\nread -r password\ntest \"$password\" = \"s3cret\" || exit 7\necho ok\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = run_with_password_prompt(
            &script,
            &[],
            &[],
            "s3cret",
            "password",
            Duration::from_secs(10),
            &AtomicBool::new(false),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn wrong_password_fails_the_script() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("prompt.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'Export file password: '\nread -r password\ntest \"$password\" = \"right\" || exit 7\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = run_with_password_prompt(
            &script,
            &[],
            &[],
            "wrong",
            "password",
            Duration::from_secs(10),
            &AtomicBool::new(false),
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_cancelled_command_is_killed() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("slow.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'Export file password: '\nread -r p\nsleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let result = run_with_password_prompt(
            &script,
            &[],
            &[],
            "x",
            "password",
            Duration::from_secs(60),
            &cancel,
        );
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    }
}
