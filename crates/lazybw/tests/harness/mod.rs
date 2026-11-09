#![allow(dead_code, reason = "each test binary uses a different part of this harness")]

//! A `TestApp` that boots the real application against `fake-bw`.
//!
//! Tests drive the app with Vim notation and inspect both the UI state and the
//! fake CLI's state file (which records exactly what was sent).

use lazybw::app::state::MessageLevel;
use lazybw::app::{App, AppState, Effect, Msg, Screen, SessionPhase};
use lazybw::bw::locate::Capabilities;
use lazybw::bw::service::{self, BackendConfig, BackendEvent};
use lazybw::clipboard::Clipboard;
use lazybw::config::{Config, IconMode, Paths};
use lazybw::theme::builtin;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// The workspace `target/debug` directory.
fn target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir).join("debug");
    }
    // Test binaries live in `target/debug/deps`; resolve from the running
    // executable so a moved checkout never points at a stale absolute path
    // baked in at compile time.
    if let Ok(exe) = std::env::current_exe()
        && let Some(debug) = exe.parent().and_then(Path::parent)
    {
        return debug.to_path_buf();
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug")
}

/// Locates (building if necessary) the `fake-bw` binary.
pub fn fake_bw_path() -> PathBuf {
    let path = target_dir().join("fake-bw");
    if !path.exists() {
        let status =
            std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
                .args(["build", "-p", "fake-bw"])
                .status()
                .expect("cargo build -p fake-bw");
        assert!(status.success(), "failed to build fake-bw");
    }
    path
}

/// The fixture vault from `tests/fixtures/vault.json`.
pub fn fixture_vault() -> Value {
    let text = include_str!("../fixtures/vault.json");
    serde_json::from_str(text).expect("fixture JSON")
}

/// A booted application with a fake backend.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct TestApp {
    /// The application.
    pub app: App,
    /// Serialises tests that share process environment variables.
    _env_lock: std::sync::MutexGuard<'static, ()>,
    /// The temp directory holding the state file.
    pub dir: TempDir,
    /// The fake state file.
    pub state_path: PathBuf,
    /// Backend events.
    pub events: mpsc::Receiver<BackendEvent>,
    /// The fixture JSON, for assertions.
    pub fixture: Value,
    /// The scripted `$EDITOR` used by suspension tests.
    pub editor: Option<PathBuf>,
    /// The in-memory clipboard the app copies to.
    pub clipboard: lazybw::clipboard::MemoryClipboard,
}

impl TestApp {
    /// Boots the app with a locked or unlocked fake account.
    pub async fn boot(locked: bool) -> Self {
        Self::boot_with_status(if locked { "locked" } else { "unlocked" }).await
    }

    /// Boots the app with the fake account in `status` (`unauthenticated`,
    /// `locked` or `unlocked`).
    pub async fn boot_with_status(status: &str) -> Self {
        let env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new().expect("tempdir");
        let state_path = dir.path().join("state.json");
        let items = fixture_vault()["items"].clone();
        let folders = fixture_vault()["folders"].clone();
        // The fixture ids are stable; keep them.
        let state = json!({
            "version": "2026.10.1",
            "status": status,
            "serverUrl": null,
            "lastSync": "2026-10-30T13:00:00.000Z",
            "email": "you@example.com",
            "masterPassword": "correct horse",
            "sessionKey": "fake-session",
            "folders": folders,
            "items": items,
            "log": []
        });
        std::fs::write(&state_path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();

        // fake-bw reads its state from the environment; tests run serially.
        #[expect(unsafe_code, reason = "the test sets the environment the child inherits")]
        unsafe {
            std::env::set_var("FAKE_BW_STATE", &state_path);
            std::env::set_var("LAZYBW_BW", fake_bw_path());
            std::env::set_var("BW_SESSION", "fake-session");
        }
        let mut config = Config::default();
        config.bw.path = fake_bw_path().display().to_string();
        config.bw.inherit_session = true;
        config.ui.animations = false;
        config.ui.icons = IconMode::Unicode;
        config.security.clipboard_timeout =
            lazybw::config::duration::DurationSpec::parse("30s").unwrap();
        config.vault.sort = lazybw::config::SortOrder::Name;
        let mut theme = builtin::by_name("tokyonight-night").unwrap();
        theme.resolve(true, false);
        let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
        let paths =
            Paths { config_dir: dir.path().join("config"), state_dir: dir.path().join("state") };
        let mut state = AppState::new(config.clone(), theme, keymap, paths);
        let clipboard = lazybw::clipboard::MemoryClipboard::new();
        state.clipboard = Clipboard::with_backend(std::sync::Arc::new(clipboard.clone()));
        state.now = lazybw::util::time::test_now();
        state.size = (140, 40);
        state.screen = Screen::Starting;
        state.session = SessionPhase::Starting;
        let location = Caps::caps();
        state.bw_path = Some(fake_bw_path());
        state.capabilities = Some(location);
        let mut app = App::new(state);
        let (backend, events) = service::spawn(BackendConfig {
            bw_path: fake_bw_path(),
            transport: config.bw.transport,
            runtime_dir: dir.path().to_path_buf(),
            inherit_session: true,
            capabilities: Caps::caps(),
            check_exposed: false,
        });
        app.backend = Some(backend);
        let mut test = Self {
            app,
            _env_lock: env_lock,
            dir,
            state_path,
            events,
            fixture: fixture_vault(),
            editor: None,
            clipboard,
        };
        let effects = test.app.start();
        test.run(effects).await;
        test
    }

    /// Sets the scripted editor used for `$EDITOR` effects.
    pub fn with_editor(mut self, editor: impl Into<PathBuf>) -> Self {
        self.editor = Some(editor.into());
        self
    }

    /// Runs effects, executing suspended `$EDITOR` tasks with the scripted
    /// editor and cancelling anything that needs a real terminal login.
    pub async fn run(&mut self, effects: Vec<Effect>) {
        let remaining = self.app.run_effects(effects).await;
        for effect in remaining {
            if let Effect::Suspend(task) = effect {
                use lazybw::app::msg::{EditorTarget, SuspendTask, Suspended};
                let result = match (&self.editor, task) {
                    (Some(editor), SuspendTask::Editor { target }) => match target {
                        EditorTarget::Notes(text) => {
                            match lazybw::external_editor::edit_text(
                                text.expose(),
                                &editor.display().to_string(),
                            ) {
                                Ok(edited) => {
                                    Suspended::Notes(lazybw::bw::sensitive::Sensitive::new(edited))
                                }
                                Err(err) => Suspended::Failed(err.to_string()),
                            }
                        }
                        EditorTarget::Json(text) => {
                            match lazybw::external_editor::edit_text(
                                text.expose(),
                                &editor.display().to_string(),
                            ) {
                                Ok(edited) => Suspended::ItemJson(
                                    lazybw::bw::sensitive::Sensitive::new(edited),
                                ),
                                Err(err) => Suspended::Failed(err.to_string()),
                            }
                        }
                        EditorTarget::SshPrivateKey(text) => {
                            match lazybw::external_editor::edit_text(
                                text.expose(),
                                &editor.display().to_string(),
                            ) {
                                Ok(edited) => Suspended::SshPrivateKey(
                                    lazybw::bw::sensitive::Sensitive::new(edited),
                                ),
                                Err(err) => Suspended::Failed(err.to_string()),
                            }
                        }
                        EditorTarget::Config(_) => Suspended::Cancelled,
                    },
                    _ => Suspended::Cancelled,
                };
                let effects = self.app.handle(Msg::Suspended(result)).await;
                Box::pin(self.run(effects)).await;
            }
        }
    }

    /// Feeds one message and runs its effects.
    pub async fn handle(&mut self, msg: Msg) {
        let effects = self.app.handle(msg).await;
        self.run(effects).await;
    }

    /// Feeds a key sequence in Vim notation.
    pub async fn keys(&mut self, notation: &str) {
        for msg in Msg::key(notation) {
            self.handle(msg).await;
        }
    }

    /// Drains backend events without blocking.
    pub async fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            let effects = self.app.handle(Msg::Backend(event)).await;
            self.run(effects).await;
        }
    }

    /// Waits until there are no pending backend requests.
    pub async fn wait_idle(&mut self) {
        for _ in 0..200 {
            self.pump().await;
            if self.app.state.pending.is_empty() {
                // One more pump for the final response.
                tokio::time::sleep(Duration::from_millis(20)).await;
                self.pump().await;
                if self.app.state.pending.is_empty() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("backend did not become idle: {:?}", self.app.state.pending);
    }

    /// Waits until the app is on a screen.
    pub async fn wait_screen(&mut self, screen: Screen) {
        for _ in 0..200 {
            self.pump().await;
            if self.app.state.screen == screen {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("screen did not become {screen:?}: {:?}", self.app.state.screen);
    }

    /// Waits until the vault is loaded.
    pub async fn wait_ready(&mut self) {
        for _ in 0..200 {
            self.pump().await;
            if self.app.state.session.is_ready() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "vault did not load: screen={:?} session={:?} pending={:?} messages={:?}",
            self.app.state.screen,
            self.app.state.session,
            self.app.state.pending,
            self.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>()
        );
    }

    /// One item from the fixture vault, by id.
    pub fn fixture_vault_item(&self, id: &str) -> Option<serde_json::Value> {
        self.fixture["items"].as_array()?.iter().find(|item| item["id"] == id).cloned()
    }

    /// The fake CLI state file.
    pub fn fake_state(&self) -> Value {
        let text = std::fs::read_to_string(&self.state_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    }

    /// The item names in the fake vault.
    pub fn fake_names(&self) -> Vec<String> {
        self.fake_state()["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The requests recorded by fake-bw.
    pub fn requests(&self) -> Vec<Value> {
        self.fake_state()["log"].as_array().cloned().unwrap_or_default()
    }

    /// Unlocks the vault.
    pub async fn unlock(&mut self) {
        self.wait_screen(Screen::Unlock).await;
        self.keys("correct horse<CR>").await;
        self.wait_ready().await;
    }

    /// The message line text.
    pub fn status_text(&self) -> String {
        self.app.state.status_message.as_ref().map(|m| m.text.clone()).unwrap_or_default()
    }
}

struct Caps;

impl Caps {
    fn caps() -> Capabilities {
        Capabilities::from_version(semver::Version::new(2026, 10, 1))
    }
}

/// Convenience: build a test app and load the vault.
pub async fn ready_app() -> TestApp {
    let mut app = TestApp::boot(false).await;
    app.wait_ready().await;
    app
}

/// Assert a message was logged at a level.
pub fn assert_message(app: &TestApp, needle: &str) {
    let found = app.app.state.messages.iter().any(|m| {
        m.text.contains(needle)
            && matches!(
                m.level,
                MessageLevel::Info | MessageLevel::Ok | MessageLevel::Warn | MessageLevel::Error
            )
    });
    assert!(
        found,
        "expected a message containing {needle:?}; got: {:?}",
        app.app.state.messages.iter().map(|m| m.text.clone()).collect::<Vec<_>>()
    );
}

/// The path of a file in the workspace.
pub fn workspace_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}
