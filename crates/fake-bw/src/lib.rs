//! `fake-bw`: a test double for the Bitwarden CLI.
//!
//! The binary in `main.rs` is the interface lazybw uses
//! (`LAZYBW_BW=<path>`). This library exposes the same modules so integration
//! tests can call the HTTP handler and state helpers directly.

#![deny(unsafe_code)]

pub mod cli;
pub mod http;
pub mod import_export;
pub mod serve;
pub mod state;

/// Runs one command and returns the process exit code.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    cli::run(args)
}
