//! The log file, `$XDG_STATE_HOME/lazybw/lazybw.log`.
//!
//! The terminal UI owns the terminal, so log events go to a file and never
//! to stderr. The level is `info` unless `LAZYBW_LOG` names another one
//! (`error`, `warn`, `debug`, `trace` or `off`); `LAZYBW_LOG=debug` adds the
//! redacted stderr of `bw serve`. The file is
//! readable only by its owner and is never opened through a symlink. A log
//! larger than 1 MiB is moved to `lazybw.log.1` at the next start, so the
//! two files stay around 2 MiB together.
//!
//! The log never holds requests, responses, item data, the master password
//! or the session key; `tests/logs.rs` drives a whole session at `debug` to
//! check it.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;

/// The environment variable that sets the log level.
pub const ENV: &str = "LAZYBW_LOG";

/// Size past which the log is moved aside when lazybw starts.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// Starts logging to `path`, at the level named by `LAZYBW_LOG`.
///
/// Returns the guard that flushes the log when it is dropped; keep it alive
/// until lazybw exits. Returns `None` when the file cannot be opened or a
/// logger is already installed: lazybw runs without a log rather than not
/// at all.
#[must_use]
pub fn init(path: &Path) -> Option<WorkerGuard> {
    init_with(path, level(&std::env::var(ENV).unwrap_or_default())).ok()
}

/// Starts logging to `path` at `level`.
///
/// # Errors
///
/// The file cannot be opened, or a global logger is already installed.
pub fn init_with(path: &Path, level: LevelFilter) -> io::Result<WorkerGuard> {
    let file = open(path)?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(io::Error::other)?;
    Ok(guard)
}

/// The level named by `spec`, in any case; `info` for anything else, so a
/// mistyped `LAZYBW_LOG` never leaves the log empty. (`tracing` itself reads
/// an empty string as `error`.)
#[must_use]
pub fn level(spec: &str) -> LevelFilter {
    match spec.trim() {
        "" => LevelFilter::INFO,
        name => name.parse().unwrap_or(LevelFilter::INFO),
    }
}

/// Opens the log for appending: private, never through a symlink, and
/// rotated first when it has grown past [`MAX_BYTES`].
fn open(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > MAX_BYTES) {
        fs::rename(path, rotated(path))?;
    }
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)?;
    // A file that already existed keeps its mode on open; make it private
    // through the descriptor, never through the path.
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

/// `lazybw.log` → `lazybw.log.1`.
fn rotated(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".1");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn the_log_is_private_and_its_directory_is_created() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("state/lazybw/lazybw.log");
        open(&path).unwrap();
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn an_existing_log_is_appended_to_and_made_private() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("lazybw.log");
        fs::write(&path, "earlier session\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        writeln!(open(&path).unwrap(), "this session").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "earlier session\nthis session\n");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_large_log_is_moved_aside_and_a_new_one_started() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("lazybw.log");
        let large = vec![b'x'; usize::try_from(MAX_BYTES).unwrap() + 1];
        fs::write(&path, &large).unwrap();
        open(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(fs::read(rotated(&path)).unwrap(), large);
    }

    #[test]
    fn a_log_at_the_limit_is_kept() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("lazybw.log");
        fs::write(&path, vec![b'x'; usize::try_from(MAX_BYTES).unwrap()]).unwrap();
        open(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), MAX_BYTES);
        assert!(!rotated(&path).exists());
    }

    #[test]
    fn a_symlink_is_never_followed() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("elsewhere");
        fs::write(&target, "not lazybw's\n").unwrap();
        let path = dir.path().join("lazybw.log");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(open(&path).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "not lazybw's\n");
    }

    #[test]
    fn the_level_is_info_unless_lazybw_log_names_another() {
        assert_eq!(level(""), LevelFilter::INFO);
        assert_eq!(level("verbose"), LevelFilter::INFO);
        assert_eq!(level("lazybw=debug"), LevelFilter::INFO);
        assert_eq!(level("debug"), LevelFilter::DEBUG);
        assert_eq!(level(" DEBUG\n"), LevelFilter::DEBUG);
        assert_eq!(level("warn"), LevelFilter::WARN);
        assert_eq!(level("off"), LevelFilter::OFF);
    }
}
