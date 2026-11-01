//! Side effects returned by `update` and run by the event loop.

use crate::app::msg::SuspendTask;
use crate::bw::service::CommandKind;
use secrecy::SecretString;
use std::path::PathBuf;

/// An effect the executor must run.
#[derive(Debug)]
pub enum Effect {
    /// Write the collapsed sidebar folders to the state file.
    SaveFolds,
    /// Read the collapsed sidebar folders from the state file.
    LoadFolds,
    /// Send a command to the backend with a correlation id.
    Backend {
        /// Correlation id.
        id: u64,
        /// The command.
        kind: CommandKind,
    },
    /// Start `bw serve`.
    StartBackend {
        /// Correlation id.
        id: u64,
    },
    /// Store a session key and restart the backend with it.
    SetSession(SecretString),
    /// Release the terminal, run a task, restore it.
    Suspend(SuspendTask),
    /// Copy a value to the clipboard.
    Copy {
        /// The text (zeroized when dropped, redacted in `Debug`).
        text: crate::bw::sensitive::Sensitive,
        /// A label for the message ("password", "username", …).
        label: String,
    },
    /// Clear the clipboard now.
    ClearClipboard,
    /// Write configuration changes (`path`, value as TOML text).
    SaveConfig(Vec<(Vec<String>, toml_edit::Value)>),
    /// Reload the theme by name.
    ReloadTheme(String),
    /// Quit the application.
    Quit,
    /// Open a file with `xdg-open`.
    OpenFile(PathBuf),
    /// Open a URI with `xdg-open`.
    OpenUri(String),
    /// Re-run the health report.
    RunHealth,
    /// Persist the command history.
    SaveHistory,
    /// Delete an item's notes buffer and other secrets (lock).
    Lock,
    /// Refresh the vault list after a change (no network).
    RefreshList,
}
