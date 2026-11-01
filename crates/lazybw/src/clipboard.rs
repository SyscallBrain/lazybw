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
        use std::process::{Command, Stdio};
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err(format!("{program} exited with {status}")) }
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
            if let Ok(output) = std::process::Command::new(program).args(args).output()
                && output.status.success()
            {
                return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
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
}

impl Clipboard {
    /// The system clipboard.
    #[must_use]
    pub fn system() -> Self {
        Self::with_backend(Arc::new(SystemClipboard::default()))
    }

    /// An in-memory clipboard for tests.
    #[must_use]
    pub fn memory() -> Self {
        Self::with_backend(Arc::new(MemoryClipboard::new()))
    }

    /// A clipboard with a custom backend.
    #[must_use]
    pub fn with_backend(backend: Arc<dyn ClipboardBackend>) -> Self {
        Self { backend, pending: None, last_copied: None, enabled: true }
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
        match &self.pending {
            Some(pending) if now >= pending.deadline => self.clear_if_ours(now).unwrap_or(false),
            _ => false,
        }
    }

    /// Seconds left before the auto-clear.
    #[must_use]
    pub fn countdown(&self, now: Instant) -> Option<u64> {
        self.pending
            .as_ref()
            .map(|pending| pending.deadline.saturating_duration_since(now).as_secs())
    }

    /// True while a clear is scheduled.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Clears unconditionally, used on exit and on lock.
    pub fn clear_now(&mut self) {
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
