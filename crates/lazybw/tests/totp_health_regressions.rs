//! Protected TOTP rendering and capability-gated local health reports.
use lazybw::app::state::{Focus, Pending};
use lazybw::app::update::update;
use lazybw::app::{AppState, Effect, Msg, Screen, SessionPhase};
use lazybw::bw::models::{Item, ItemType, Login};
use lazybw::bw::sensitive::Sensitive;
use lazybw::bw::service::{BackendEvent, BwEvent, CommandKind};

const TOTP_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

fn state() -> AppState {
    let config = lazybw::config::Config::default();
    let theme = lazybw::theme::builtin::by_name("tokyonight-night").unwrap();
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let mut state = AppState::new(
        config,
        theme,
        keymap,
        lazybw::config::Paths {
            config_dir: "/tmp/lazybw-totp-health-unused".into(),
            state_dir: "/tmp/lazybw-totp-health-unused".into(),
        },
    );
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.session = SessionPhase::Ready;
    state.screen = Screen::Vault;
    state.size = (160, 45);
    state.now = chrono::DateTime::from_timestamp(59, 0).unwrap();
    state.fx.borrow_mut().set_enabled(false);
    let items = ["a", "b"].map(|id| {
        let mut item = Item::new(ItemType::Login);
        item.id = Some(id.into());
        item.name = id.into();
        item.reprompt = 1;
        item.login = Some(Login {
            password: Some(Sensitive::new("password")),
            totp: Some(Sensitive::new(TOTP_SECRET)),
            ..Default::default()
        });
        item
    });
    state.cache.load(items.into(), vec![], vec![], vec![]);
    update(&mut state, Msg::FocusGained);
    state
}

fn keys(state: &mut AppState, text: &str) -> Vec<Effect> {
    Msg::key(text).into_iter().flat_map(|msg| update(state, msg)).collect()
}

fn render(state: &AppState) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 45)).unwrap();
    terminal.draw(|frame| lazybw::ui::draw(frame, state)).unwrap();
    terminal.backend().buffer().content.iter().map(|cell| cell.symbol()).collect()
}

fn authorize(state: &mut AppState) {
    assert!(state.reprompt().is_some(), "protected action must ask for the master password");
    let effects = keys(state, "master<CR>");
    let id = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Backend { id, kind: CommandKind::Unlock(_) } => Some(id),
            _ => None,
        })
        .expect("reprompt must verify the master password");
    update(state, Msg::Backend(BackendEvent { id, event: BwEvent::Unlocked(Ok(())) }));
    assert!(state.reprompt().is_none());
}

fn select_totp(state: &mut AppState) {
    state.focus = Focus::Detail;
    state.detail.row = 2;
}

#[test]
fn protected_totp_is_masked_until_master_authorizes_reveal() {
    let mut state = state();
    assert!(!render(&state).contains("287 082"));
    select_totp(&mut state);
    keys(&mut state, "za");
    assert!(!render(&state).contains("287 082"));
    authorize(&mut state);
    assert!(render(&state).contains("287 082"));
    keys(&mut state, "za");
    assert!(!render(&state).contains("287 082"));
}

#[test]
fn protected_totp_rehides_on_expiry_focus_loss_and_selection_change() {
    for reset in ["expiry", "focus", "selection"] {
        let mut state = state();
        select_totp(&mut state);
        keys(&mut state, "za");
        authorize(&mut state);
        assert!(render(&state).contains("287 082"));
        match reset {
            "expiry" => {
                state.detail.hide_at = Some(std::time::Instant::now());
                update(&mut state, Msg::Tick);
                state.now = chrono::DateTime::from_timestamp(59, 0).unwrap();
            }
            "focus" => {
                update(&mut state, Msg::FocusLost);
            }
            _ => {
                state.focus = Focus::List;
                keys(&mut state, "j");
                assert_eq!(state.selected_id().as_deref(), Some("b"));
            }
        }
        assert!(!render(&state).contains("287 082"), "TOTP remained visible after {reset}");
    }
}

#[test]
fn protected_totp_copy_requires_authorization_and_does_not_reveal() {
    for key in ["yt", "yy"] {
        let mut state = state();
        select_totp(&mut state);
        keys(&mut state, key);
        assert!(state.clipboard.last_copied().is_none());
        authorize(&mut state);
        assert_eq!(state.clipboard.last_copied(), Some("287082"));
        assert!(!render(&state).contains("287 082"));
    }
}

#[test]
fn unprotected_totp_remains_visible() {
    let mut state = state();
    let mut item = state.cache.item("a").unwrap().clone();
    item.reprompt = 0;
    state.cache.upsert(item);
    assert!(render(&state).contains("287 082"));
}

async fn health_response(supported: bool, session: bool) -> (BackendEvent, bool) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let bw = dir.path().join("bw");
    // A marker records every invocation, including attempts that would fail.
    std::fs::write(
        &bw,
        "#!/bin/sh\ntouch \"$0.invoked\"\nprintf '%s\\n' 'Unknown command: report' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&bw, std::fs::Permissions::from_mode(0o700)).unwrap();
    let version = if supported { (2026, 10) } else { (2025, 9) };
    let capabilities = lazybw::bw::locate::Capabilities::from_version(semver::Version::new(
        version.0, version.1, 0,
    ));
    let (backend, mut events) = lazybw::bw::service::spawn(lazybw::bw::service::BackendConfig {
        bw_path: bw.clone(),
        transport: lazybw::config::Transport::Fd,
        runtime_dir: dir.path().into(),
        inherit_session: false,
        capabilities,
    });
    if session {
        backend
            .send(0, CommandKind::SetSession(secrecy::SecretString::from("synthetic")))
            .await
            .unwrap();
        events.recv().await.unwrap();
    }
    backend.send(1, CommandKind::Health { check_exposed: true }).await.unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(3), events.recv())
        .await
        .unwrap()
        .unwrap();
    backend.send(99, CommandKind::Shutdown).await.unwrap();
    (response, bw.with_extension("invoked").exists())
}

#[tokio::test]
async fn unsupported_health_report_runs_local_checks_without_launching_bw() {
    let (response, invoked) = health_response(false, true).await;
    assert!(!invoked, "unsupported report command must not run");
    let mut state = state();
    state.capabilities =
        Some(lazybw::bw::locate::Capabilities::from_version(semver::Version::new(2025, 9, 0)));
    state.pending.insert(response.id, Pending::Health);
    update(&mut state, Msg::Backend(response));
    let report = state.health.report.expect("older CLI must still produce a local report");
    assert_eq!(report.checked, 2);
    assert_eq!(report.reused, 2);
    assert_eq!(report.weak, 2);
    assert!(!report.has_exposed_data);
    assert!(report.flags("a").weak && report.flags("b").reused);
}

#[tokio::test]
async fn supported_health_report_preserves_cli_errors() {
    let (response, invoked) = health_response(true, true).await;
    assert!(invoked);
    assert!(matches!(response.event, BwEvent::Health(Err(_))));
    let mut state = state();
    state.pending.insert(response.id, Pending::Health);
    update(&mut state, Msg::Backend(response));
    assert!(state.health.report.is_none());
    assert!(state.messages.iter().any(|message| message.text.contains("Health check failed")));
}

#[tokio::test]
async fn unsupported_health_report_still_requires_a_session() {
    let (response, invoked) = health_response(false, false).await;
    assert!(!invoked);
    assert!(matches!(response.event, BwEvent::Health(Err(_))));
}
