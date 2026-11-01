//! Hot reload of `config.toml` and the user theme directory with `notify`.
//!
//! Events are debounced (150 ms) and delivered on a channel as
//! [`ReloadEvent`]; the application decides whether to apply or keep the
//! previous configuration.

use notify::{Event, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// What changed on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadEvent {
    /// `config.toml` changed.
    Config,
    /// A theme file changed.
    Themes,
    /// The watcher itself failed.
    Error(String),
}

/// A running watcher; dropping it stops watching.
#[derive(Debug)]
pub struct ConfigWatcher {
    _watcher: notify::RecommendedWatcher,
    receiver: Receiver<ReloadEvent>,
}

impl ConfigWatcher {
    /// Watches the config file and the themes directory.
    pub fn start(config_file: &Path, themes_dir: &Path) -> notify::Result<Self> {
        let (tx, rx) = channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    let paths: Vec<PathBuf> = event.paths.clone();
                    if paths.iter().any(|p| p.file_name().is_some_and(|n| n == "config.toml")) {
                        let _ = tx.send(ReloadEvent::Config);
                    } else if paths.iter().any(|p| p.extension().is_some_and(|e| e == "toml")) {
                        let _ = tx.send(ReloadEvent::Themes);
                    }
                }
                Err(err) => {
                    let _ = tx.send(ReloadEvent::Error(err.to_string()));
                }
            })?;
        if let Some(parent) = config_file.parent() {
            let _ = std::fs::create_dir_all(parent);
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
        if themes_dir.exists() {
            watcher.watch(themes_dir, RecursiveMode::NonRecursive)?;
        }
        Ok(Self { _watcher: watcher, receiver: rx })
    }

    /// Blocks until an event arrives or the timeout expires, coalescing
    /// events that arrive within 150 ms of the first one.
    pub fn next_event(&self, timeout: Duration) -> Option<ReloadEvent> {
        let first = self.receiver.recv_timeout(timeout).ok()?;
        let deadline = Instant::now() + Duration::from_millis(150);
        let mut last = first;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.receiver.recv_timeout(remaining) {
                Ok(event) => last = event,
                Err(_) => break,
            }
        }
        Some(last)
    }

    /// Non-blocking check, used from the event loop.
    pub fn try_event(&self) -> Option<ReloadEvent> {
        self.receiver.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn detects_config_changes() {
        let tmp = TempDir::new().unwrap();
        let config = tmp.path().join("config.toml");
        std::fs::write(&config, "[ui]\ntheme = \"nord\"\n").unwrap();
        let themes = tmp.path().join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        let watcher = ConfigWatcher::start(&config, &themes).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(&config, "[ui]\ntheme = \"gruvbox-dark\"\n").unwrap();
        let event = watcher.next_event(Duration::from_secs(3));
        assert_eq!(event, Some(ReloadEvent::Config));
    }
}
