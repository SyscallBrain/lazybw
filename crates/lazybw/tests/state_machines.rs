//! The app's state machines, driven through `update`: which overlay owns
//! the keyboard, the wizards' steps, the login form, the master password
//! re-prompt and the session lifecycle. Each test names a transition and
//! checks where it lands, including the moves that must not happen.

use lazybw::app::state::{
    Action, ConfirmState, ExportStep, HistoryState, ImportStep, InfoState, Overlay, PickerEntry,
    PickerKind, PickerState, PromptState, QuickOpenState,
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
    assert!(s.export().unwrap().busy);
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
