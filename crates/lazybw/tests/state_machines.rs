//! The app's state machines, driven through `update`: which overlay owns
//! the keyboard, the wizards' steps, the login form, the master password
//! re-prompt and the session lifecycle. Each test names a transition and
//! checks where it lands, including the moves that must not happen.

use lazybw::app::state::{
    Action, ConfirmState, ExportStep, HistoryState, ImportStep, InfoState, LoginFocus, LoginMethod,
    LoginServer, Overlay, PickerEntry, PickerKind, PickerState, PromptState, QuickOpenState,
};
use lazybw::app::update::update;
use lazybw::app::{AppState, Effect, Msg, Screen, SessionPhase};
use lazybw::bw::models::{Item, ItemType, Login};
use lazybw::config::{Config, Paths};

fn state() -> AppState {
    let config = Config::default();
    let theme = lazybw::theme::builtin::by_name("tokyonight-night").unwrap();
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let dir = std::env::temp_dir().join("lazybw-state-machines-unused");
    let mut state =
        AppState::new(config, theme, keymap, Paths { config_dir: dir.clone(), state_dir: dir });
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.screen = Screen::Vault;
    state.session = SessionPhase::Ready;
    state.cache.load(vec![item("a"), item("b")], vec![], vec![], vec![]);
    state
}

fn item(id: &str) -> Item {
    let mut item = Item::new(ItemType::Login);
    item.id = Some(id.into());
    item.name = id.into();
    item.login = Some(Login::default());
    item
}

fn keys(state: &mut AppState, notation: &str) -> Vec<Effect> {
    Msg::key(notation).into_iter().flat_map(|msg| update(state, msg)).collect()
}

fn paste(state: &mut AppState, text: &str) -> Vec<Effect> {
    update(state, Msg::Paste(text.to_owned()))
}

fn confirm() -> ConfirmState {
    ConfirmState {
        title: "Delete".into(),
        message: "Sure?".into(),
        confirm_label: "Delete".into(),
        danger: true,
        requires_word: None,
        input: String::new(),
        on_confirm: Action::new("noop"),
        on_deny: None,
        also_subfolders: false,
        subfolder_count: 0,
    }
}

fn prompt() -> PromptState {
    PromptState { title: "New folder".into(), on_accept: Action::new("noop"), ..Default::default() }
}

fn picker() -> PickerState {
    let entry = |label: &str| PickerEntry {
        label: label.into(),
        note: String::new(),
        filter: label.into(),
        value: label.into(),
        accelerator: None,
    };
    PickerState::new("Theme", PickerKind::Theme, vec![entry("nord"), entry("rose-pine")])
}

/// One of each overlay that a test can open directly, with the keymap
/// context it should put first.
fn every_overlay() -> Vec<(Overlay, &'static str)> {
    vec![
        (Overlay::QuickOpen(QuickOpenState::default()), "quick_open"),
        (Overlay::Cmdline(Default::default()), "cmdline"),
        (Overlay::Picker(picker()), "picker"),
        (Overlay::Confirm(confirm()), "confirm"),
        (Overlay::Reprompt(Default::default()), "unlock"),
        (Overlay::Prompt(prompt()), "cmdline"),
        (Overlay::History(HistoryState::default()), "help"),
        (Overlay::Info(InfoState::default()), "help"),
    ]
}

// ── Overlays ────────────────────────────────────────────────────────────

#[test]
fn an_open_overlay_leads_the_keymap_context_chain() {
    for (overlay, context) in every_overlay() {
        let mut s = state();
        s.open(overlay.clone());
        let chain = s.contexts();
        assert_eq!(chain.first().map(String::as_str), Some(context), "{overlay:?}");
        assert_eq!(chain.last().map(String::as_str), Some("global"), "{overlay:?}");
    }
}

#[test]
fn esc_closes_every_overlay() {
    for (overlay, _) in every_overlay() {
        let mut s = state();
        s.open(overlay.clone());
        keys(&mut s, "<Esc>");
        assert!(s.overlay.is_none(), "Esc left {overlay:?} open");
    }
}

#[test]
fn opening_an_overlay_replaces_the_open_one() {
    let mut s = state();
    s.open(picker());
    s.open(confirm());
    assert!(s.picker().is_none());
    assert!(s.confirm().is_some());
}

#[test]
fn taking_an_overlay_leaves_other_kinds_open() {
    let mut s = state();
    s.open(picker());
    assert!(s.take_confirm().is_none());
    assert!(s.picker().is_some(), "closing a confirm closed the picker");
    assert!(s.take_picker().is_some());
    assert!(s.overlay.is_none());
}

#[test]
fn a_paste_goes_into_the_open_prompt() {
    let mut s = state();
    s.open(prompt());
    paste(&mut s, "Work\nignored second line");
    assert_eq!(s.prompt().unwrap().input.text(), "Work");
}

#[test]
fn a_paste_filters_the_open_picker() {
    let mut s = state();
    s.open(picker());
    paste(&mut s, "rose");
    let picker = s.picker().unwrap();
    assert_eq!(picker.filter, "rose");
    assert_eq!(picker.selected().map(|e| e.value.as_str()), Some("rose-pine"));
}

#[test]
fn a_paste_never_reaches_the_editor_behind_a_popup() {
    let mut s = state();
    keys(&mut s, "al");
    assert_eq!(s.screen, Screen::Editor);
    keys(&mut s, "i");
    assert!(s.editor.as_ref().unwrap().insert, "the editor is in INSERT");
    let before = s.editor.as_ref().unwrap().rows.clone();
    s.open(confirm());
    paste(&mut s, "pasted");
    assert_eq!(s.editor.as_ref().unwrap().rows, before, "the paste went behind the dialog");
    assert!(s.confirm().unwrap().input.is_empty(), "a confirmation word is typed, not pasted");
}

#[test]
fn locking_closes_any_overlay() {
    for (overlay, _) in every_overlay() {
        let mut s = state();
        s.open(overlay.clone());
        s.drop_secrets();
        assert!(s.overlay.is_none(), "{overlay:?} survived the lock");
    }
}

// ── Export wizard ───────────────────────────────────────────────────────

fn open_export(s: &mut AppState, format: &str) {
    keys(s, &format!(":export {format} /tmp/lazybw-never-written.json<CR>"));
    assert!(s.export().is_some(), "the export wizard opened");
}

fn backend_ids(effects: &[Effect]) -> Vec<u64> {
    effects
        .iter()
        .filter_map(|e| if let Effect::Backend { id, .. } = e { Some(*id) } else { None })
        .collect()
}

#[test]
fn export_steps_follow_the_format() {
    let mut s = state();
    open_export(&mut s, "encrypted");
    assert_eq!(s.export().unwrap().step, ExportStep::Format);
    keys(&mut s, "<CR>");
    assert_eq!(s.export().unwrap().step, ExportStep::FilePassword);
    keys(&mut s, "<Esc>");

    open_export(&mut s, "json");
    keys(&mut s, "<CR>");
    assert_eq!(s.export().unwrap().step, ExportStep::MasterPassword, "no file password for JSON");
}

#[test]
fn export_fields_take_j_and_k_as_text() {
    let mut s = state();
    open_export(&mut s, "encrypted");
    keys(&mut s, "<Tab>");
    assert_eq!(s.export().unwrap().step, ExportStep::Destination);
    keys(&mut s, "jk");
    assert!(s.export().unwrap().path.ends_with("jk"), "{}", s.export().unwrap().path);
    keys(&mut s, "<CR>jakek<Tab>jakek<CR>");
    assert_eq!(s.export().unwrap().error, None);
    assert_eq!(s.export().unwrap().step, ExportStep::MasterPassword);
    keys(&mut s, "kojak");
    assert_eq!(s.export().unwrap().master.expose(), "kojak");
}

#[test]
fn export_format_list_reads_j_and_k_as_moves() {
    let mut s = state();
    open_export(&mut s, "json");
    let before = s.export().unwrap().format;
    keys(&mut s, "j");
    assert_ne!(s.export().unwrap().format, before);
    keys(&mut s, "k");
    assert_eq!(s.export().unwrap().format, before);
}

#[test]
fn export_password_step_starts_on_the_password_field() {
    // Leaving step one from the destination field used to land on the
    // confirmation field.
    let mut s = state();
    open_export(&mut s, "encrypted");
    keys(&mut s, "<Tab><CR>");
    assert_eq!(s.export().unwrap().step, ExportStep::FilePassword);
}

#[test]
fn export_ignores_keys_while_the_cli_works() {
    let mut s = state();
    open_export(&mut s, "json");
    let effects = keys(&mut s, "<CR>master<CR>");
    assert_eq!(backend_ids(&effects).len(), 1, "one export request");
    assert!(s.exporting());
    let effects = keys(&mut s, "more<CR>");
    assert!(backend_ids(&effects).is_empty(), "a second export was sent");
    assert!(s.export().unwrap().master.is_empty(), "typing reached the busy wizard");
}

#[test]
fn a_late_export_answer_leaves_the_screen_alone() {
    let mut s = state();
    open_export(&mut s, "json");
    let id = backend_ids(&keys(&mut s, "<CR>master<CR>"))[0];
    keys(&mut s, "<Esc>");
    assert!(s.overlay.is_none());
    keys(&mut s, "al");
    assert_eq!(s.screen, Screen::Editor);
    let outcome = lazybw::bw::service::ExportOutcome {
        path: "/tmp/lazybw-never-written.json".into(),
        bytes: 1,
    };
    update(
        &mut s,
        Msg::Backend(lazybw::bw::service::BackendEvent {
            id,
            event: lazybw::bw::service::BwEvent::Exported(Ok(outcome)),
        }),
    );
    assert_eq!(s.screen, Screen::Editor, "the export answer closed the editor");
}

// ── Import wizard ───────────────────────────────────────────────────────

#[test]
fn esc_walks_the_import_wizard_back_one_step() {
    let mut s = state();
    keys(&mut s, ":import<CR>");
    assert!(matches!(s.import().unwrap().step, ImportStep::Format));
    keys(&mut s, "<CR>");
    assert!(matches!(s.import().unwrap().step, ImportStep::Path));
    keys(&mut s, "<Esc>");
    assert!(matches!(s.import().unwrap().step, ImportStep::Format));
    keys(&mut s, "<Esc>");
    assert!(s.import().is_none());
}

#[test]
fn import_password_is_a_step_only_for_protected_files() {
    let mut s = state();
    keys(&mut s, ":import bitwardenpasswordprotected /tmp/x.json<CR>");
    keys(&mut s, "<Tab>");
    assert!(matches!(s.import().unwrap().step, ImportStep::Password));
    paste(&mut s, "filepass\n");
    assert_eq!(s.import().unwrap().password.expose(), "filepass");
    keys(&mut s, "<Esc><Esc>");
    assert!(s.import().is_none());

    keys(&mut s, ":import bitwardencsv /tmp/x.csv<CR>");
    keys(&mut s, "<Tab>");
    assert!(matches!(s.import().unwrap().step, ImportStep::Path), "Tab completes the path");
}

#[test]
fn an_empty_protected_password_moves_focus_to_it() {
    let mut s = state();
    keys(&mut s, ":import bitwardenpasswordprotected /tmp/x.json<CR>");
    keys(&mut s, "<CR>");
    let import = s.import().unwrap();
    assert!(matches!(import.step, ImportStep::Password));
    assert!(import.error.is_some());
}

// ── Session lifecycle ───────────────────────────────────────────────────

fn status(kind: &str) -> lazybw::bw::models::Status {
    lazybw::bw::models::Status { status: kind.into(), ..Default::default() }
}

fn answer(s: &mut AppState, id: u64, event: lazybw::bw::service::BwEvent) -> Vec<Effect> {
    update(s, Msg::Backend(lazybw::bw::service::BackendEvent { id, event }))
}

fn starting() -> AppState {
    let mut s = state();
    s.cache.clear();
    s.session = SessionPhase::Starting;
    s.screen = Screen::Starting;
    s
}

fn error_screen(title: &str) -> lazybw::app::state::ErrorScreen {
    lazybw::app::state::ErrorScreen {
        title: title.into(),
        lines: vec![],
        detail: vec![],
        actions: "e edit config   r retry   q quit".into(),
        retry: true,
        open_file: None,
    }
}

#[test]
fn every_phase_shows_its_own_screen() {
    for (phase, screen) in [
        (SessionPhase::Starting, Screen::Starting),
        (SessionPhase::LoggedOut, Screen::Login),
        (SessionPhase::Locked, Screen::Unlock),
        (SessionPhase::Failed(Box::new(error_screen("x"))), Screen::Error),
    ] {
        let mut s = state();
        s.enter_session(phase.clone());
        assert_eq!(s.screen, screen, "{phase:?}");
    }
    // Opening the vault leaves a session screen for the tab, and keeps any
    // other screen.
    let mut s = starting();
    s.enter_session(SessionPhase::Loading);
    assert_eq!(s.screen, Screen::Vault);
    s.screen = Screen::Editor;
    s.enter_session(SessionPhase::Ready);
    assert_eq!(s.screen, Screen::Editor);
}

#[test]
fn leaving_the_open_vault_forgets_it() {
    for phase in [
        SessionPhase::Locked,
        SessionPhase::LoggedOut,
        SessionPhase::Failed(Box::new(error_screen("x"))),
    ] {
        let mut s = state();
        s.open(picker());
        s.enter_session(phase.clone());
        assert_eq!(s.cache.active_items().count(), 0, "{phase:?} kept the vault");
        assert!(s.overlay.is_none(), "{phase:?} kept an overlay");
    }
}

#[test]
fn starting_against_a_locked_vault_keeps_the_clipboard() {
    // Only leaving an open vault clears the clipboard; launching lazybw must
    // not wipe what the user copied elsewhere.
    let mut s = starting();
    s.clipboard.copy("copied elsewhere", None, std::time::Instant::now()).unwrap();
    s.pending.insert(1, lazybw::app::state::Pending::Starting);
    answer(&mut s, 1, lazybw::bw::service::BwEvent::Started(Ok(status("locked"))));
    assert_eq!(s.session, SessionPhase::Locked);
    assert_eq!(s.clipboard.peek().as_deref(), Some("copied elsewhere"));
}

#[test]
fn a_startup_error_is_not_replaced_by_the_cli_answer() {
    // A config error is shown before the CLI starts; the wake-up must not
    // start it behind the error screen.
    let mut s = starting();
    s.enter_session(SessionPhase::Failed(Box::new(error_screen("Configuration error"))));
    let effects = update(&mut s, Msg::Wake);
    assert!(!effects.iter().any(|e| matches!(e, Effect::StartBackend { .. })), "{effects:?}");
    assert_eq!(s.screen, Screen::Error);
    // `r` retries.
    let effects = keys(&mut s, "r");
    assert!(effects.iter().any(|e| matches!(e, Effect::StartBackend { .. })));
    assert_eq!(s.session, SessionPhase::Starting);
}

#[test]
fn a_cli_that_fails_to_start_shows_an_error_instead_of_spinning() {
    let mut s = starting();
    s.pending.insert(1, lazybw::app::state::Pending::Starting);
    answer(
        &mut s,
        1,
        lazybw::bw::service::BwEvent::Started(Err(lazybw::bw::BwError::Other(
            "bw serve exited".into(),
        ))),
    );
    assert_eq!(s.screen, Screen::Error);
    assert!(s.error().is_some_and(|e| e.retry && e.lines.join(" ").contains("bw serve exited")));
}

#[tokio::test]
async fn starting_with_no_cli_to_talk_to_says_so() {
    // `bw` was not found at launch: nothing can answer a start request, so
    // the start screen must not spin forever.
    let mut app = lazybw::app::App::new(starting());
    app.state.pending.insert(1, lazybw::app::state::Pending::Starting);
    app.run_effects(vec![Effect::StartBackend { id: 1 }]).await;
    assert_eq!(app.state.screen, Screen::Error);
    assert!(app.state.pending.is_empty());
    assert!(app.state.error().is_some_and(|e| !e.retry), "a retry could only fail again");
}

#[test]
fn not_being_logged_in_any_more_forgets_the_vault() {
    let mut s = state();
    s.pending.insert(7, lazybw::app::state::Pending::GetItem);
    answer(&mut s, 7, lazybw::bw::service::BwEvent::Item(Err(lazybw::bw::BwError::NotLoggedIn)));
    assert_eq!(s.session, SessionPhase::LoggedOut);
    assert_eq!(s.screen, Screen::Login);
    assert_eq!(s.cache.active_items().count(), 0, "the vault stayed in memory");
}

#[test]
fn a_fatal_cli_error_forgets_the_vault_and_stops_background_work() {
    let mut s = state();
    answer(&mut s, 0, lazybw::bw::service::BwEvent::Fatal("bw serve crashed".into()));
    assert_eq!(s.screen, Screen::Error);
    assert_eq!(s.cache.active_items().count(), 0);
    // No more syncs or locks against a CLI that is gone.
    s.config.vault.sync_interval =
        lazybw::config::duration::DurationSpec(Some(std::time::Duration::from_secs(60)));
    s.last_sync_at = None;
    let effects = update(&mut s, Msg::Tick);
    assert!(!effects.iter().any(|e| matches!(e, Effect::Backend { .. })), "{effects:?}");
}

#[test]
fn unlocking_opens_the_vault_while_it_loads() {
    let mut s = state();
    s.drop_secrets();
    assert_eq!(s.screen, Screen::Unlock);
    let id = backend_ids(&keys(&mut s, "secret<CR>"))[0];
    assert!(s.unlocking());
    let effects = keys(&mut s, "<CR>");
    assert!(backend_ids(&effects).is_empty(), "a second unlock was sent");
    answer(&mut s, id, lazybw::bw::service::BwEvent::Unlocked(Ok(status("unlocked"))));
    assert!(!s.unlocking());
    assert_eq!(s.session, SessionPhase::Loading);
    assert_eq!(s.screen, Screen::Vault);
}

#[test]
fn an_unlock_that_leaves_the_vault_locked_says_so() {
    let mut s = state();
    s.drop_secrets();
    let id = backend_ids(&keys(&mut s, "secret<CR>"))[0];
    answer(&mut s, id, lazybw::bw::service::BwEvent::Unlocked(Ok(status("locked"))));
    assert_eq!(s.session, SessionPhase::Locked, "stuck in Loading with a locked vault");
    assert!(s.unlock.error.is_some());
}

#[test]
fn busy_states_end_with_their_request_however_it_ends() {
    let mut s = state();
    s.drop_secrets();
    keys(&mut s, "secret<CR>");
    assert!(s.unlocking());
    // The request could not be sent: the app removes it, and the busy
    // state goes with it.
    s.pending.clear();
    assert!(!s.unlocking());
}

// ── Login ───────────────────────────────────────────────────────────────

fn login_screen() -> AppState {
    let mut s = state();
    s.enter_session(SessionPhase::LoggedOut);
    s
}

#[test]
fn the_api_key_method_can_be_left_again() {
    let mut s = login_screen();
    keys(&mut s, "<Tab>jj");
    assert_eq!(s.login.method, LoginMethod::ApiKey);
    assert_eq!(s.login.focus, LoginFocus::Method, "choosing a method does not enter its form");
    keys(&mut s, "<CR>");
    assert_eq!(s.login.focus, LoginFocus::ClientId);
    keys(&mut s, "user.id<Tab>s3cret");
    assert_eq!(s.login.client_id, "user.id");
    assert_eq!(s.login.client_secret.expose(), "s3cret");
    keys(&mut s, "<Esc>k");
    assert_eq!(s.login.focus, LoginFocus::Method);
    assert_eq!(s.login.method, LoginMethod::Sso, "j/k stayed in the API key form");
}

#[test]
fn q_quits_from_the_lists_and_is_text_in_the_fields() {
    let mut s = login_screen();
    assert!(keys(&mut s, "q").iter().any(|e| matches!(e, Effect::Quit)));
    let mut s = login_screen();
    keys(&mut s, "<Tab>jj<CR>");
    assert!(!keys(&mut s, "q").iter().any(|e| matches!(e, Effect::Quit)));
    assert_eq!(s.login.client_id, "q");
}

#[test]
fn an_api_key_login_is_sent_once() {
    let mut s = login_screen();
    keys(&mut s, "<Tab>jj<CR>id<Tab>secret");
    assert_eq!(backend_ids(&keys(&mut s, "<CR>")).len(), 1);
    assert!(s.logging_in());
    assert!(backend_ids(&keys(&mut s, "<CR>")).is_empty(), "a second login was sent");
}

#[test]
fn a_self_hosted_server_asks_for_its_url() {
    let mut s = login_screen();
    keys(&mut s, "jj");
    assert_eq!(s.login.server, LoginServer::SelfHosted);
    assert_eq!(s.login.focus, LoginFocus::ServerUrl);
    let effects = keys(&mut s, "https://vault.example.com<CR>");
    assert_eq!(backend_ids(&effects).len(), 1, "the server URL is saved");
    assert_eq!(s.login.focus, LoginFocus::Server);
    assert_eq!(s.login.server_url(), "https://vault.example.com");
}

// ── Master password re-prompt ───────────────────────────────────────────

fn protected_item() -> AppState {
    let mut s = state();
    let mut item = item("vault-key");
    item.reprompt = 1;
    item.login.as_mut().unwrap().password = Some(lazybw::bw::sensitive::Sensitive::new("hunter2"));
    s.cache.load(vec![item], vec![], vec![], vec![]);
    s.list_row = 0;
    s
}

#[test]
fn the_reprompt_runs_its_action_after_the_right_password() {
    let mut s = protected_item();
    keys(&mut s, "yp");
    assert!(s.reprompt().is_some(), "copying a protected password asks first");
    let id = backend_ids(&keys(&mut s, "master<CR>"))[0];
    assert!(s.verifying_reprompt());
    answer(&mut s, id, lazybw::bw::service::BwEvent::Unlocked(Ok(status("unlocked"))));
    assert!(s.reprompt().is_none());
    assert_eq!(s.clipboard.last_copied(), Some("hunter2"));
}

#[test]
fn a_reprompt_answer_after_cancelling_does_nothing() {
    let mut s = protected_item();
    keys(&mut s, "yp");
    let id = backend_ids(&keys(&mut s, "master<CR>"))[0];
    keys(&mut s, "<Esc>");
    answer(&mut s, id, lazybw::bw::service::BwEvent::Unlocked(Ok(status("unlocked"))));
    assert_eq!(s.clipboard.last_copied(), None, "the cancelled action ran anyway");
    assert!(s.reprompt().is_none(), "the dialog came back after Esc");
}

#[test]
fn a_reprompt_answer_only_counts_for_its_own_dialog() {
    let mut s = protected_item();
    keys(&mut s, "yp");
    let first = backend_ids(&keys(&mut s, "master<CR>"))[0];
    keys(&mut s, "<Esc>yp");
    assert!(s.reprompt().is_some(), "a new dialog");
    answer(&mut s, first, lazybw::bw::service::BwEvent::Unlocked(Ok(status("unlocked"))));
    assert!(s.reprompt().is_some(), "the old answer closed the new dialog");
    assert_eq!(s.clipboard.last_copied(), None);
}

#[test]
fn a_wrong_reprompt_password_keeps_the_dialog() {
    let mut s = protected_item();
    keys(&mut s, "yp");
    let id = backend_ids(&keys(&mut s, "wrong<CR>"))[0];
    answer(
        &mut s,
        id,
        lazybw::bw::service::BwEvent::Unlocked(Err(lazybw::bw::BwError::InvalidPassword)),
    );
    let reprompt = s.reprompt().expect("the dialog stays");
    assert!(reprompt.error.is_some());
    assert!(!s.verifying_reprompt());
    assert_eq!(backend_ids(&keys(&mut s, "master<CR>")).len(), 1, "a retry is sent");
}

// ── The rules above hold for new code too ───────────────────────────────

/// Runtime code changes the session only through `AppState::enter_session`,
/// which keeps the screen and the vault data in step with it.
#[test]
fn session_changes_go_through_enter_session() {
    fn visit(dir: &std::path::Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, found);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("app/state.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (number, line) in text.lines().enumerate() {
                let assigns_session = line.contains("state.session = ");
                let assigns_session_screen = ["Starting", "Error", "Login", "Unlock"]
                    .iter()
                    .any(|screen| line.contains(&format!(".screen = Screen::{screen}")));
                if assigns_session || assigns_session_screen {
                    found.push(format!("{}:{}: {}", path.display(), number + 1, line.trim()));
                }
            }
        }
    }
    let mut found = Vec::new();
    visit(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    assert!(
        found.is_empty(),
        "set the session with AppState::enter_session, not by hand:\n{}",
        found.join("\n")
    );
}
