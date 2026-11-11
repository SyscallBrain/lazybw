//! Clipboard access with auto-clear.
//!
//! Order of preference: `arboard` (Wayland data-control or X11), then
//! `wl-copy`, `xclip`, `xsel`, then OSC 52 written to the terminal when there
//! is no display or the session runs over SSH. A copied value is cleared
//! after `security.clipboard_timeout` only if the clipboard still holds what
//! lazybw put there.

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
mod async_tests;
mod worker;

/// A clipboard implementation.
pub trait ClipboardBackend: Send + Sync + std::fmt::Debug {
    /// Writes text.
    fn set(&self, text: &str) -> Result<(), String>;
    /// Reads the current text.
    fn read(&self) -> Result<Option<String>, String>;
    /// Clears the clipboard.
    fn clear(&self) -> Result<(), String>;
    /// Backend name for `lazybw doctor`.
    fn name(&self) -> &'static str;
}

/// The real system clipboard with fallbacks.
///
/// One `arboard` handle lives for the whole session. On X11 the handle owns
/// the selection: dropping it right after a write loses the contents unless
/// a clipboard manager grabs them first, and makes `arboard` print a warning
/// straight onto the terminal, over the TUI.
#[derive(Default)]
pub struct SystemClipboard {
    arboard: Mutex<Option<arboard::Clipboard>>,
}

impl std::fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemClipboard").finish_non_exhaustive()
    }
}

impl SystemClipboard {
    /// Runs `op` on the shared `arboard` handle, opening it on first use.
    fn with_arboard<T>(
        &self,
        op: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    ) -> Option<T> {
        // Poisoning policy: every lock in this module guards a single value that
        // is replaced whole, never left half-written, so recovering is safe.
        let mut handle = self.arboard.lock().unwrap_or_else(|e| e.into_inner());
        if handle.is_none() {
            *handle = arboard::Clipboard::new().ok();
        }
        op(handle.as_mut()?).ok()
    }

    fn tool(&self, program: &str, args: &[&str], text: &str) -> Result<(), String> {
        run_tool(program, args, Some(text.as_bytes()), false).map(|_| ())
    }
}

impl ClipboardBackend for SystemClipboard {
    fn set(&self, text: &str) -> Result<(), String> {
        if self.with_arboard(|clipboard| clipboard.set_text(text.to_owned())).is_some() {
            return Ok(());
        }
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        if wayland && self.tool("wl-copy", &[], text).is_ok() {
            return Ok(());
        }
        if self.tool("xclip", &["-selection", "clipboard"], text).is_ok() {
            return Ok(());
        }
        if self.tool("xsel", &["--clipboard", "--input"], text).is_ok() {
            return Ok(());
        }
        if let Some(tty) = terminal_tty() {
            return osc52(&tty, text);
        }
        Err("no clipboard backend available".to_owned())
    }

    fn read(&self) -> Result<Option<String>, String> {
        if let Some(text) = self.with_arboard(arboard::Clipboard::get_text) {
            return Ok(Some(text));
        }
        for (program, args) in [
            ("wl-paste", &["--no-newline"][..]),
            ("xclip", &["-selection", "clipboard", "-o"][..]),
            ("xsel", &["--clipboard", "--output"][..]),
        ] {
            if let Ok(output) = run_tool(program, args, None, true) {
                return Ok(Some(String::from_utf8_lossy(&output).into_owned()));
            }
        }
        Ok(None)
    }

    fn clear(&self) -> Result<(), String> {
        if self.with_arboard(arboard::Clipboard::clear).is_some() {
            return Ok(());
        }
        match self.set("") {
            Ok(()) => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn name(&self) -> &'static str {
        "arboard/wl-copy/xclip/osc52"
    }
}

/// Bounded pipe I/O as well as a bounded wait: a clipboard owner may never
/// answer a read, and a hung copy helper may never read its standard input.
fn run_tool(
    program: &str,
    args: &[&str],
    input: Option<&[u8]>,
    capture: bool,
) -> Result<Vec<u8>, String> {
    use std::io::{ErrorKind, Read};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
        .process_group(0)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(if capture { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<Vec<u8>, String> {
        fn nonblocking(fd: &impl std::os::fd::AsFd) -> Result<(), String> {
            let flags = rustix::fs::fcntl_getfl(fd).map_err(|e| e.to_string())?;
            rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)
                .map_err(|e| e.to_string())
        }
        if let Some(stdin) = &child.stdin {
            nonblocking(stdin)?;
        }
        if let Some(stdout) = &child.stdout {
            nonblocking(stdout)?;
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut remaining = input.unwrap_or_default();
        let mut output = Vec::new();
        let mut eof = !capture;
        loop {
            if Instant::now() >= deadline {
                return Err(format!("{program} timed out"));
            }
            if let Some(stdin) = &mut child.stdin {
                if !remaining.is_empty() {
                    match stdin.write(remaining) {
                        Ok(0) => return Err(format!("{program} stopped reading input")),
                        Ok(n) => remaining = &remaining[n..],
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::WouldBlock | ErrorKind::Interrupted
                            ) => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                if remaining.is_empty() {
                    child.stdin.take();
                }
            }
            if let Some(stdout) = &mut child.stdout {
                let mut buffer = [0u8; 8192];
                match stdout.read(&mut buffer) {
                    Ok(0) => eof = true,
                    Ok(n) => output.extend_from_slice(&buffer[..n]),
                    Err(e)
                        if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("{program} exited with {status}"));
                }
                if eof && child.stdin.is_none() {
                    return Ok(output);
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        // Kill the helper's process group too: a shell or forked helper must
        // not complete a stale clipboard write after its parent times out.
        #[expect(unsafe_code, reason = "kill the private process group created for this child")]
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

/// An in-memory clipboard for tests.
#[derive(Debug, Clone, Default)]
pub struct MemoryClipboard(Arc<Mutex<Option<String>>>);

impl MemoryClipboard {
    /// Empty clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current contents.
    #[must_use]
    pub fn contents(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl ClipboardBackend for MemoryClipboard {
    fn set(&self, text: &str) -> Result<(), String> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
        Ok(())
    }

    fn read(&self) -> Result<Option<String>, String> {
        Ok(self.contents())
    }

    fn clear(&self) -> Result<(), String> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

/// A pending auto-clear.
#[derive(Debug, Clone)]
pub struct PendingClear {
    /// Hash of the value lazybw wrote.
    pub hash: u64,
    /// When to clear.
    pub deadline: Instant,
    /// Failed clear attempts so far.
    pub attempts: u8,
}

/// The clipboard facade the application uses.
#[derive(Debug)]
pub struct Clipboard {
    backend: Arc<dyn ClipboardBackend>,
    pending: Option<PendingClear>,
    last_copied: Option<zeroize::Zeroizing<String>>,
    enabled: bool,
    worker: Option<worker::Worker>,
}

impl Clipboard {
    /// The system clipboard.
    #[must_use]
    pub fn system() -> Self {
        Self::with_background_backend(Arc::new(SystemClipboard::default()))
    }

    /// An in-memory clipboard for tests.
    #[must_use]
    pub fn memory() -> Self {
        Self::with_backend(Arc::new(MemoryClipboard::new()))
    }

    /// A clipboard with a custom backend.
    #[must_use]
    pub fn with_backend(backend: Arc<dyn ClipboardBackend>) -> Self {
        Self { backend, pending: None, last_copied: None, enabled: true, worker: None }
    }

    /// Runs a blocking backend on an ordered worker, keeping the caller responsive.
    /// Copies report their completion through [`Self::take_copy_result`] after a tick.
    #[must_use]
    pub fn with_background_backend(backend: Arc<dyn ClipboardBackend>) -> Self {
        Self { worker: Some(worker::Worker::new()), ..Self::with_backend(backend) }
    }

    /// Whether an asynchronous copy is still in progress.
    #[must_use]
    pub fn copying(&self) -> bool {
        self.worker.as_ref().is_some_and(worker::Worker::copying)
    }

    /// Takes the latest asynchronous copy result; cancelled requests are discarded.
    pub fn take_copy_result(&mut self) -> Option<Result<(), String>> {
        self.worker.as_mut().and_then(worker::Worker::take_result)
    }

    /// Finishes queued clipboard cleanup at process exit, with a bounded wait.
    pub fn finish(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.finish();
        }
    }

    /// Disables writes (used when `NO_COLOR`-style restrictions apply).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Backend name.
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Copies a value and schedules the auto-clear.
    pub fn copy(
        &mut self,
        text: &str,
        timeout: Option<Duration>,
        now: Instant,
    ) -> Result<(), String> {
        if !self.enabled {
            return Err("the clipboard is disabled".to_owned());
        }
        if let Some(worker) = &mut self.worker {
            return worker.copy(Arc::clone(&self.backend), text, timeout);
        }
        self.backend.set(text)?;
        self.last_copied = Some(zeroize::Zeroizing::new(text.to_owned()));
        self.pending = timeout.map(|timeout| PendingClear {
            hash: hash_text(text),
            deadline: now + timeout,
            attempts: 0,
        });
        Ok(())
    }

    /// Clears a previously copied value unless something else replaced it.
    ///
    /// When the clipboard cannot be read back (OSC 52 over SSH, a write-only
    /// backend), there is no way to tell whether the value is still there, so
    /// it is cleared anyway: losing an unrelated copy is the lesser risk.
    /// A failed clear is retried a few times instead of being forgotten.
    pub fn clear_if_ours(&mut self, now: Instant) -> Result<bool, String> {
        if let Some(worker) = &mut self.worker {
            worker.clear(false);
            return Ok(false);
        }
        let Some(mut expected) = self.pending.take() else {
            return Ok(false);
        };
        let ours = match self.backend.read() {
            Ok(Some(current)) => hash_text(&current) == expected.hash,
            // Unreadable or empty: cannot prove it changed, so clear.
            Ok(None) | Err(_) => true,
        };
        if !ours {
            // The user copied something else since; leave it alone.
            self.last_copied = None;
            return Ok(false);
        }
        match self.backend.clear() {
            Ok(()) => {
                self.last_copied = None;
                Ok(true)
            }
            Err(err) => {
                expected.attempts += 1;
                if expected.attempts < 5 {
                    expected.deadline = now + Duration::from_secs(2);
                    self.pending = Some(expected);
                }
                Err(err)
            }
        }
    }

    /// Called once per tick; returns true when the clipboard was cleared.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(worker) = &mut self.worker {
            return worker.tick();
        }
        match &self.pending {
            Some(pending) if now >= pending.deadline => self.clear_if_ours(now).unwrap_or(false),
            _ => false,
        }
    }

    /// Seconds left before the auto-clear.
    #[must_use]
    pub fn countdown(&self, now: Instant) -> Option<u64> {
        if let Some(worker) = &self.worker {
            return worker
                .deadline()
                .map(|deadline| deadline.saturating_duration_since(now).as_secs());
        }
        self.pending
            .as_ref()
            .map(|pending| pending.deadline.saturating_duration_since(now).as_secs())
    }

    /// True while a clear is scheduled.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.worker.as_ref().is_some_and(|worker| worker.deadline().is_some())
    }

    /// Clears what lazybw copied, on lock and on exit, unless the clipboard
    /// has changed since: a lock must not wipe a link the user copied in
    /// the browser. A clipboard that cannot be read back is cleared anyway,
    /// as [`Clipboard::clear_if_ours`] does.
    pub fn clear_ours(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.clear(false);
            return;
        }
        let Some(copied) = self.last_copied.take() else {
            self.pending = None;
            return;
        };
        self.pending = None;
        let ours = match self.backend.read() {
            Ok(Some(current)) => hash_text(&current) == hash_text(&copied),
            Ok(None) | Err(_) => true,
        };
        if ours {
            let _ = self.backend.clear();
        }
    }

    /// Clears unconditionally (`:clear`-style requests and tests).
    pub fn clear_now(&mut self) {
        if let Some(worker) = &mut self.worker {
            // Start even before the first copy for an explicit unconditional clear.
            // Usually the worker already exists; a normal lock never starts one.
            if worker.start(Arc::clone(&self.backend)).is_ok() {
                worker.clear(true);
            }
            return;
        }
        let _ = self.backend.clear();
        self.pending = None;
        self.last_copied = None;
    }

    /// The last value copied by lazybw (used by tests).
    #[must_use]
    pub fn last_copied(&self) -> Option<&str> {
        self.last_copied.as_ref().map(|text| text.as_str())
    }

    /// Reads the current clipboard contents (used by tests).
    #[must_use]
    pub fn peek(&self) -> Option<String> {
        self.backend.read().ok().flatten()
    }
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// The terminal device used for OSC 52.
fn terminal_tty() -> Option<PathBuf> {
    std::env::var_os("SSH_TTY")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("/dev/tty")))
}

fn osc52(tty: &std::path::Path, text: &str) -> Result<(), String> {
    let encoded = base64(text.as_bytes());
    let sequence = format!("\x1b]52;c;{encoded}\x07");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(tty)
        .map_err(|e| format!("could not open {}: {e}", tty.display()))?;
    file.write_all(sequence.as_bytes()).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}

/// Minimal Base64 (standard alphabet, padded).
#[must_use]
pub fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(ALPHABET[((triple >> 18) & 0x3F) as usize]));
        out.push(char::from(ALPHABET[((triple >> 12) & 0x3F) as usize]));
        if chunk.len() > 1 {
            out.push(char::from(ALPHABET[((triple >> 6) & 0x3F) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(ALPHABET[(triple & 0x3F) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stalled_clipboard_tool_is_terminated_within_the_deadline() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("clipboard");
        std::fs::write(&script, "#!/bin/sh\nsleep 2\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let start = Instant::now();
        let result = SystemClipboard::default().tool(script.to_str().unwrap(), &[], "secret");
        assert!(start.elapsed() < Duration::from_millis(1500), "clipboard child was not bounded");
        assert!(result.is_err(), "a timed-out copy must not report success");
    }

    #[test]
    fn memory_clipboard_round_trip() {
        let backend = Arc::new(MemoryClipboard::new());
        let mut clipboard = Clipboard::with_backend(backend.clone());
        clipboard.copy("s3cret", Some(Duration::from_secs(30)), Instant::now()).unwrap();
        assert_eq!(backend.contents().as_deref(), Some("s3cret"));
        assert!(clipboard.has_pending());
        assert!(clipboard.countdown(Instant::now()).is_some());
    }

    #[test]
    fn clears_only_our_value() {
        let backend = Arc::new(MemoryClipboard::new());
        let mut clipboard = Clipboard::with_backend(backend.clone());
        let now = Instant::now();
        clipboard.copy("ours", Some(Duration::from_secs(1)), now).unwrap();
        assert!(!clipboard.tick(now));
        assert!(clipboard.tick(now + Duration::from_secs(2)));
        assert_eq!(backend.contents(), None);

        clipboard.copy("ours", Some(Duration::from_secs(1)), now).unwrap();
        backend.set("something else").unwrap();
        assert!(!clipboard.tick(now + Duration::from_secs(2)));
        assert_eq!(backend.contents().as_deref(), Some("something else"));
    }

    #[test]
    fn clearing_on_lock_leaves_what_the_user_copied_elsewhere() {
        let backend = Arc::new(MemoryClipboard::new());
        let mut clipboard = Clipboard::with_backend(backend.clone());
        // Something copied in another program: lazybw never touched it.
        backend.set("a link from the browser").unwrap();
        clipboard.clear_ours();
        assert_eq!(backend.contents().as_deref(), Some("a link from the browser"));
        // A password lazybw copied is cleared, also without a timeout.
        clipboard.copy("s3cret", None, Instant::now()).unwrap();
        clipboard.clear_ours();
        assert_eq!(backend.contents(), None);
        // Copied by lazybw, then replaced by the user: left alone.
        clipboard.copy("s3cret", None, Instant::now()).unwrap();
        backend.set("something newer").unwrap();
        clipboard.clear_ours();
        assert_eq!(backend.contents().as_deref(), Some("something newer"));
    }

    #[test]
    fn never_timeout_keeps_value() {
        let backend = Arc::new(MemoryClipboard::new());
        let mut clipboard = Clipboard::with_backend(backend);
        clipboard.copy("x", None, Instant::now()).unwrap();
        assert!(!clipboard.has_pending());
    }

    #[test]
    fn base64_encode() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"hello world"), "aGVsbG8gd29ybGQ=");
    }
}
