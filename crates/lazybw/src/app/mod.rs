//! The application: state, update function, effects and the backend bridge.
//!
//! The Elm Architecture: [`state::AppState`] holds everything the UI reads,
//! [`update::update`] turns messages into effects, and [`App`] executes them.

pub mod actions;
pub mod clock;
pub mod commands;
pub mod effect;
pub mod form;
pub mod msg;
pub mod state;
pub mod update;

pub use effect::Effect;
pub use msg::Msg;
pub use state::{Action, AppState, Focus, MessageLevel, Screen, SessionPhase, Tab};

use crate::bw::service::{Backend, BackendConfig, BackendEvent};
use tokio::sync::mpsc;

/// The application object that executes effects.
pub struct App {
    /// The whole UI state.
    pub state: AppState,
    /// Backend handle, once started.
    pub backend: Option<Backend>,
    /// Backend event receiver.
    pub backend_events: Option<mpsc::Receiver<BackendEvent>>,
    /// Backend configuration.
    pub backend_config: Option<BackendConfig>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App").field("state", &self.state).finish_non_exhaustive()
    }
}

impl App {
    /// Creates the app around a state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self { state, backend: None, backend_events: None, backend_config: None }
    }

    /// Connects a backend configuration and starts the task lazily.
    pub fn with_backend(mut self, config: BackendConfig) -> Self {
        self.backend_config = Some(config);
        self
    }

    /// Sends the initial wake message.
    pub fn start(&mut self) -> Vec<Effect> {
        update::update(&mut self.state, Msg::Wake)
    }

    /// Handles one message and runs the resulting effects.
    pub async fn handle(&mut self, msg: Msg) -> Vec<Effect> {
        let effects = update::update(&mut self.state, msg);
        self.run_effects(effects).await
    }

    /// Applies a key in Vim notation (used by the test harness).
    pub async fn keys(&mut self, notation: &str) {
        for msg in Msg::key(notation) {
            self.handle(msg).await;
        }
    }

    /// Executes effects, returning the ones that could not be run.
    pub async fn run_effects(&mut self, effects: Vec<Effect>) -> Vec<Effect> {
        let mut deferred = Vec::new();
        for effect in effects {
            match effect {
                Effect::Backend { id, kind } => {
                    if let Some(backend) = &self.backend {
                        if backend.send(id, kind).await.is_err() {
                            self.state.pending.remove(&id);
                            self.state.message(
                                state::MessageLevel::Error,
                                "The Bitwarden CLI backend is not running.",
                            );
                        }
                    } else {
                        self.state.pending.remove(&id);
                        deferred.push(Effect::Backend { id, kind });
                    }
                }
                Effect::StartBackend { id } => {
                    self.ensure_backend().await;
                    let sent = match &self.backend {
                        Some(backend) => {
                            backend.send(id, crate::bw::service::CommandKind::Start).await.is_ok()
                        }
                        None => false,
                    };
                    if !sent {
                        // Nothing will answer: say so rather than leave the
                        // start screen spinning.
                        self.state.pending.remove(&id);
                        self.state.enter_session(state::SessionPhase::Failed(Box::new(
                            state::ErrorScreen {
                                title: "Bitwarden CLI unavailable".to_owned(),
                                lines: vec![
                                    "lazybw has no Bitwarden CLI to start in this session."
                                        .to_owned(),
                                    "Check that bw is installed, then start lazybw again."
                                        .to_owned(),
                                ],
                                detail: Vec::new(),
                                actions: "q quit".to_owned(),
                                retry: false,
                                open_file: None,
                            },
                        )));
                    }
                }
                Effect::SetSession(key) => {
                    self.ensure_backend().await;
                    if let Some(backend) = &self.backend
                        && let Err(err) =
                            backend.send(0, crate::bw::service::CommandKind::SetSession(key)).await
                    {
                        self.state.toast(
                            state::MessageLevel::Error,
                            format!("Couldn't store the session key: {err}"),
                        );
                    }
                }
                Effect::Suspend(task) => {
                    // The runner releases the terminal and posts
                    // `Msg::Suspended` back; update never blocks on a child.
                    deferred.push(Effect::Suspend(task));
                }
                Effect::Copy { text, label } => {
                    let timeout = self.state.config.security.clipboard_timeout.get();
                    if let Err(err) =
                        self.state.clipboard.copy(text.expose(), timeout, std::time::Instant::now())
                    {
                        self.state.toast(state::MessageLevel::Error, err);
                    } else {
                        self.state.clipboard_label = Some(label);
                    }
                }
                Effect::ClearClipboard => {
                    self.state.clipboard.clear_ours();
                }
                Effect::SaveConfig(updates) => {
                    let path = self.state.paths.config_file();
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(err) = crate::config::persist::update_file(&path, &updates) {
                        self.state.toast_with(
                            state::MessageLevel::Error,
                            "Couldn't save the config.",
                            err.to_string(),
                        );
                    }
                }
                Effect::ReloadTheme(name) => {
                    let outcome = crate::theme::loader::load(&name, &self.state.paths.themes_dir());
                    if let Some(mut theme) = outcome.theme {
                        theme.transparent = self.state.config.ui.transparent;
                        theme
                            .resolve(crate::theme::truecolor_available(), crate::theme::no_color());
                        self.state.theme = theme;
                    }
                }
                Effect::Quit => {
                    self.state.quit = true;
                }
                Effect::OpenFile(path) => {
                    let _ = open::that_detached(path);
                }
                Effect::OpenUri(uri) => {
                    let _ = open::that_detached(uri);
                }
                Effect::RunHealth => {
                    let id = self.state.next_request_id;
                    self.state.next_request_id += 1;
                    self.state.pending.insert(id, state::Pending::Health);
                    if let Some(backend) = &self.backend {
                        let kind = crate::bw::service::CommandKind::Health {
                            check_exposed: self.state.config.health.check_exposed,
                        };
                        if backend.send(id, kind).await.is_err() {
                            self.state.pending.remove(&id);
                        }
                    } else {
                        self.state.pending.remove(&id);
                    }
                }
                Effect::SaveFolds => save_folds(&self.state),
                Effect::LoadFolds => {
                    let path = self.state.paths.folds_file();
                    if let Ok(text) = std::fs::read_to_string(&path)
                        && let Ok(file) =
                            serde_json::from_str::<crate::vault::folds::FoldFile>(&text)
                    {
                        self.state.collapsed = crate::vault::folds::from_ids(
                            &file.collapsed,
                            self.state.cache.folders(),
                        );
                    }
                    self.state.folds_restored = true;
                }
                Effect::SaveHistory => {
                    save_command_history(&self.state);
                }
                Effect::Lock => {
                    self.state.drop_secrets();
                }
            }
        }
        deferred
    }

    async fn ensure_backend(&mut self) {
        if self.backend.is_some() {
            return;
        }
        let Some(config) = self.backend_config.clone() else { return };
        let (backend, events) = crate::bw::service::spawn(config);
        self.backend = Some(backend);
        self.backend_events = Some(events);
    }
}

fn save_folds(state: &AppState) {
    let file = crate::vault::folds::FoldFile {
        version: 1,
        collapsed: crate::vault::folds::to_ids(&state.collapsed, state.cache.folders()),
    };
    if let Ok(json) = serde_json::to_vec_pretty(&file) {
        let _ = crate::util::atomic::write_atomic(&state.paths.folds_file(), &json, 0o600);
    }
}

fn save_command_history(state: &AppState) {
    let path = state.paths.command_history();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut text = state.command_history.join("\n");
    text.push('\n');
    let _ = crate::util::atomic::write_atomic(&path, text.as_bytes(), 0o600);
}

/// Reads the command-line history saved by an earlier session.
pub fn load_command_history(state: &mut AppState) {
    let Ok(text) = std::fs::read_to_string(state.paths.command_history()) else { return };
    let lines: Vec<String> =
        text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
    let skip = lines.len().saturating_sub(crate::app::update::HISTORY_LIMIT);
    state.command_history = lines.into_iter().skip(skip).collect();
}

/// Builds a state for tests: fixed clock, memory clipboard, no backend.
#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::config::{Config, KeysConfig, Paths};

    /// A state with the default config, theme and keymap.
    #[must_use]
    pub fn test_state() -> AppState {
        test_state_with_config(Config::default())
    }

    /// A state with a specific config.
    #[must_use]
    pub fn test_state_with_config(config: Config) -> AppState {
        let mut theme = crate::theme::builtin::by_name("tokyonight-night").unwrap();
        theme.resolve(true, false);
        let (keymap, _) = crate::config::keymap::Keymap::build(&config.keys).unwrap();
        let mut state = AppState::new(
            config,
            theme,
            keymap,
            Paths {
                config_dir: std::path::PathBuf::from("/tmp/lazybw-test/config"),
                state_dir: std::path::PathBuf::from("/tmp/lazybw-test/state"),
            },
        );
        state.clipboard = crate::clipboard::Clipboard::memory();
        state.now = crate::util::time::test_now();
        state.size = (120, 36);
        state.screen = Screen::Vault;
        state.enter_session(SessionPhase::Ready);
        state.email = Some("you@example.com".to_owned());
        state.server = Some("vault.bitwarden.eu".to_owned());
        state
    }

    /// A state with a small fixture vault.
    #[must_use]
    pub fn fixture_state() -> AppState {
        let mut state = test_state();
        let mut items = Vec::new();
        for (index, (name, kind, favorite)) in [
            ("GitHub", crate::bw::models::ItemType::Login, true),
            ("Gmail", crate::bw::models::ItemType::Login, false),
            ("Personal visa", crate::bw::models::ItemType::Card, false),
            ("Passport", crate::bw::models::ItemType::Identity, false),
            ("Router notes", crate::bw::models::ItemType::SecureNote, false),
            ("Deploy key", crate::bw::models::ItemType::SshKey, false),
        ]
        .into_iter()
        .enumerate()
        {
            let mut item = crate::bw::models::Item::new(kind);
            item.id = Some(format!("id-{index}"));
            item.name = name.to_owned();
            item.favorite = favorite;
            item.creation_date = Some("2026-01-01T00:00:00Z".to_owned());
            item.revision_date = Some("2026-10-01T12:00:00Z".to_owned());
            match kind {
                crate::bw::models::ItemType::Login => {
                    item.login = Some(crate::bw::models::Login {
                        username: Some(crate::bw::sensitive::Sensitive::new(format!(
                            "user{index}@example.com"
                        ))),
                        password: Some(crate::bw::sensitive::Sensitive::new("fixture-password")),
                        totp: Some(crate::bw::sensitive::Sensitive::new(
                            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
                        )),
                        uris: Some(vec![crate::bw::models::LoginUri::new("https://github.com")]),
                        ..Default::default()
                    });
                }
                crate::bw::models::ItemType::SecureNote => {
                    item.notes = Some(crate::bw::sensitive::Sensitive::new(
                        "Recovery codes are in the safe.",
                    ));
                }
                _ => {}
            }
            items.push(item);
        }
        state.cache.load(
            items,
            vec![crate::bw::models::Folder {
                id: Some("folder-work".to_owned()),
                name: "Work".to_owned(),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        state
    }

    /// A config with user bindings, used by keymap tests.
    #[must_use]
    pub fn config_with_bindings(text: &str) -> Config {
        let (config, _) = Config::from_toml(text).unwrap();
        let _ = KeysConfig::default();
        config
    }
}
