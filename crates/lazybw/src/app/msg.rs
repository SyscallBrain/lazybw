//! Messages that drive the update function.

use crate::bw::service::BackendEvent;
use crate::config::watch::ReloadEvent;
use crate::input::keys;
use crossterm::event::{KeyEvent, MouseEvent};
use secrecy::SecretString;
use std::path::PathBuf;

/// The result of a task that ran with the terminal released.
#[derive(Debug)]
pub enum Suspended {
    /// `bw login` finished; the session key is set on success.
    Login(Option<SecretString>),
    /// `bw login` failed or was cancelled.
    LoginFailed,
    /// `$EDITOR` returned new text for the item notes.
    Notes(crate::bw::sensitive::Sensitive),
    /// `$EDITOR` returned new JSON for the whole item.
    ItemJson(crate::bw::sensitive::Sensitive),
    /// `$EDITOR` returned a new SSH private key.
    SshPrivateKey(crate::bw::sensitive::Sensitive),
    /// The external task failed with a message.
    Failed(String),
    /// The editor was closed without changes.
    Cancelled,
}

/// Something that happened.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "Msg is moved once through a channel; boxing the payload would allocate per message"
)]
pub enum Msg {
    /// A key press.
    Key(KeyEvent),
    /// Pasted text (bracketed paste).
    Paste(String),
    /// A mouse event.
    Mouse(MouseEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// One-second tick.
    Tick,
    /// A pending key sequence reached its deadline: which-key opens, or an
    /// ambiguous sequence resolves after `timeoutlen`.
    KeyTimer,
    /// A result from the backend.
    Backend(BackendEvent),
    /// The config or a theme changed on disk.
    Reload(ReloadEvent),
    /// The terminal lost focus.
    FocusLost,
    /// The terminal regained focus.
    FocusGained,
    /// A suspended task finished.
    Suspended(Suspended),
    /// The starting screen should move on (used by the test harness).
    Wake,
    /// `SIGUSR1`: lock the vault (for a window-manager hotkey).
    LockSignal,
}

impl Msg {
    /// Builds a key message from Vim notation (`<CR>`, `gg` yields two msgs).
    #[must_use]
    pub fn key(notation: &str) -> Vec<Self> {
        match crate::config::keymap::parse_keys(notation) {
            Ok(keys) => keys.into_iter().map(|key| Self::Key(keys::to_event(key))).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The key event, if any.
    #[must_use]
    pub fn key_event(&self) -> Option<KeyEvent> {
        match self {
            Self::Key(event) => Some(*event),
            _ => None,
        }
    }
}

/// A file to open in the user's editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorTarget {
    /// The notes of an item.
    Notes(crate::bw::sensitive::Sensitive),
    /// The whole item as JSON.
    Json(crate::bw::sensitive::Sensitive),
    /// An SSH private key (PEM/OpenSSH) for the item being edited.
    SshPrivateKey(crate::bw::sensitive::Sensitive),
    /// The config file.
    Config(PathBuf),
}

/// What a suspended task should do when it resumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuspendTask {
    /// Interactive login.
    Login {
        /// SSO instead of email and password.
        sso: bool,
    },
    /// Run `$EDITOR`.
    Editor {
        /// What is being edited.
        target: EditorTarget,
    },
    /// Show the terminal error message and wait for Enter.
    LoginFailed,
}
