//! Talking to the official Bitwarden CLI.
//!
//! lazybw never implements Bitwarden cryptography or the Bitwarden HTTP API.
//! It starts `bw serve` as a private child process over a socketpair and
//! drives a small subset of its local HTTP API, plus short-lived `bw`
//! subprocesses for login, logout, server configuration, export, import and
//! the password health report.

pub mod client;
pub mod envelope;
pub mod locate;
pub mod models;
pub mod pty;
pub mod sensitive;
pub mod serve;
pub mod service;
pub mod subprocess;

pub use envelope::BwError;
pub use service::{Backend, BackendConfig, BwEvent, Command, CommandKind, ExportFormat};
