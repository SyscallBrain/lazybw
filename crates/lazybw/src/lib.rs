//! lazybw — a keyboard-first terminal UI for the Bitwarden vault.
//!
//! The crate is a library plus a binary so that the application can be
//! exercised by integration tests without a terminal. The public surface is
//! the module tree; the binary in `main.rs` only wires the terminal, the
//! event loop and the CLI.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

pub mod app;
pub mod bw;
pub mod cli;
pub mod clipboard;
pub mod config;
pub mod external_editor;
pub mod input;
pub mod logging;
pub mod security;
pub mod theme;
pub mod totp;
pub mod ui;
pub mod util;
pub mod vault;
