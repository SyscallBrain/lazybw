//! Regression tests for the issues found in the pre-release reviews. They
//! drive `update` directly and inspect the effects, without a backend, so a
//! test can also check what must *not* happen (a browser opening, a stale
//! response being applied).

use lazybw::app::state::{Pending, UndoOp};
use lazybw::app::update::update;
use lazybw::app::{AppState, Effect, Msg, Screen, SessionPhase};
use lazybw::bw::models::{Item, ItemType, Login, LoginUri};
use lazybw::bw::service::{BackendEvent, BwEvent, VaultSnapshot};
use lazybw::config::{Config, Paths};
use lazybw::vault::cache::Filter;

fn state() -> AppState {
    let config = Config::default();
    let theme = lazybw::theme::builtin::by_name("tokyonight-night").unwrap();
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let dir = std::env::temp_dir().join("lazybw-regressions-unused");
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
    item.favorite = true;
    item.login = Some(Login::default());
    item
}

fn keys(state: &mut AppState, notation: &str) -> Vec<Effect> {
    Msg::key(notation).into_iter().flat_map(|msg| update(state, msg)).collect()
}

fn event(state: &mut AppState, id: u64, event: BwEvent) -> Vec<Effect> {
    update(state, Msg::Backend(BackendEvent { id, event }))
}

fn request_id(effects: &[Effect]) -> u64 {
    effects
        .iter()
        .find_map(|e| if let Effect::Backend { id, .. } = e { Some(*id) } else { None })
        .expect("a backend request")
}

fn snapshot(items: Vec<Item>) -> VaultSnapshot {
    VaultSnapshot { items, folders: vec![], trash: vec![], archived: vec![] }
}

fn with_uri(state: &mut AppState, uri: &str) {
    let mut item = state.cache.item("a").unwrap().clone();
    item.login.as_mut().unwrap().uris = Some(vec![LoginUri::new(uri)]);
    state.cache.upsert(item);
    state.list_row = 0;
}

fn opened(effects: &[Effect]) -> Option<String> {
    effects.iter().find_map(|e| if let Effect::OpenUri(uri) = e { Some(uri.clone()) } else { None })
}

#[test]
fn web_links_open_and_other_schemes_ask_first() {
    let mut s = state();
    with_uri(&mut s, "https://github.com/login");
    assert_eq!(opened(&keys(&mut s, "gx")).as_deref(), Some("https://github.com/login"));

    with_uri(&mut s, "github.com");
    assert_eq!(opened(&keys(&mut s, "gx")).as_deref(), Some("https://github.com"));

    for risky in ["file:///etc/passwd", "vscode://open?x=1", "androidapp://com.example.app"] {
        with_uri(&mut s, risky);
        let effects = keys(&mut s, "gx");
        assert_eq!(opened(&effects), None, "{risky} opened without asking");
        assert!(s.confirm().is_some(), "{risky}: no confirmation");
        assert_eq!(opened(&keys(&mut s, "y")).as_deref(), Some(risky), "{risky} after yes");
        with_uri(&mut s, risky);
        keys(&mut s, "gx");
        assert_eq!(opened(&keys(&mut s, "n")), None, "{risky} after no");
    }
}

/// A clipboard lazybw can write but not read back (OSC 52 over SSH).
#[derive(Debug, Default)]
struct WriteOnlyClipboard {
    contents: std::sync::Mutex<String>,
    failures_left: std::sync::Mutex<u8>,
}

impl lazybw::clipboard::ClipboardBackend for WriteOnlyClipboard {
    fn set(&self, text: &str) -> Result<(), String> {
        *self.contents.lock().unwrap() = text.into();
        Ok(())
    }
    fn read(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn clear(&self) -> Result<(), String> {
        let mut failures = self.failures_left.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return Err("transient".into());
        }
        self.contents.lock().unwrap().clear();
        Ok(())
    }
    fn name(&self) -> &'static str {
        "write-only"
    }
}

#[test]
fn an_unreadable_clipboard_is_still_cleared_on_time() {
    use std::time::{Duration, Instant};
    let backend = std::sync::Arc::new(WriteOnlyClipboard::default());
    let mut clipboard = lazybw::clipboard::Clipboard::with_backend(backend.clone());
    let now = Instant::now();
    clipboard.copy("SYNTHETIC_SECRET", Some(Duration::from_secs(30)), now).unwrap();
    assert!(!clipboard.tick(now + Duration::from_secs(29)));
    assert!(clipboard.tick(now + Duration::from_secs(31)));
    assert!(backend.contents.lock().unwrap().is_empty());
    assert!(!clipboard.has_pending());
}

#[test]
fn a_failed_clear_is_retried() {
    use std::time::{Duration, Instant};
    let backend = std::sync::Arc::new(WriteOnlyClipboard {
        failures_left: std::sync::Mutex::new(2),
        ..Default::default()
    });
    let mut clipboard = lazybw::clipboard::Clipboard::with_backend(backend.clone());
    let now = Instant::now();
    clipboard.copy("SYNTHETIC_SECRET", Some(Duration::from_secs(30)), now).unwrap();
    let mut at = now + Duration::from_secs(31);
    let mut cleared = false;
    for _ in 0..4 {
        if clipboard.tick(at) {
            cleared = true;
            break;
        }
        assert!(clipboard.has_pending(), "a failed clear must stay scheduled");
        at += Duration::from_secs(3);
    }
    assert!(cleared);
    assert!(backend.contents.lock().unwrap().is_empty());
}

#[test]
fn a_vault_answer_arriving_after_a_lock_is_dropped() {
    let mut s = state();
    s.pending.insert(500, Pending::Load);
    update(&mut s, Msg::LockSignal);
    assert_eq!(s.session, SessionPhase::Locked);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a")]))));
    assert_eq!(s.session, SessionPhase::Locked, "the old answer reopened the vault");
    assert_eq!(s.screen, Screen::Unlock);
    assert!(s.cache.item("a").is_none(), "vault data came back after the lock");
}

#[test]
fn a_save_for_another_item_leaves_the_open_editor_alone() {
    let mut s = state();
    let id = request_id(&keys(&mut s, "*")); // favourite of "a"
    keys(&mut s, "je"); // open "b"
    assert_eq!(s.editor.as_ref().unwrap().item.id.as_deref(), Some("b"));
    let mut written = item("a");
    written.favorite = false;
    event(&mut s, id, BwEvent::ItemWritten(Ok(written)));
    assert_eq!(s.editor.as_ref().unwrap().item.id.as_deref(), Some("b"));
    assert_eq!(s.screen, Screen::Editor);
}

#[test]
fn edits_made_after_sending_a_save_survive_its_answer() {
    use lazybw::app::state::FormKey;
    let mut s = state();
    keys(&mut s, "e");
    let id = request_id(&keys(&mut s, "Ifirst <Esc><C-s>"));
    keys(&mut s, "Asecond<Esc>");
    let mut written = item("a");
    written.name = "first a".into();
    event(&mut s, id, BwEvent::ItemWritten(Ok(written)));
    let editor = s.editor.as_ref().unwrap();
    let name = editor.rows.iter().find(|r| r.key == FormKey::Name).unwrap();
    assert_eq!(name.value, "first asecond", "the later edit was overwritten");
    assert!(editor.dirty);
}

#[test]
fn undoing_a_favourite_in_the_favourites_view_restores_that_item() {
    let mut s = state();
    s.filter = Filter::Favorites;
    keys(&mut s, "*"); // un-favourite "a", which leaves the view
    match s.undo.undo.last().unwrap() {
        UndoOp::Favorite { ids, .. } => assert_eq!(ids, &vec!["a".to_owned()]),
        other => panic!("{other:?}"),
    }
    assert!(!s.cache.item("a").unwrap().favorite);
    keys(&mut s, "u");
    assert!(s.cache.item("a").unwrap().favorite);
    assert!(s.cache.item("b").unwrap().favorite, "undo touched the wrong item");
}

#[test]
fn saving_from_the_editor_can_be_undone_in_the_vault() {
    use lazybw::app::state::FormKey;
    let mut s = state();
    keys(&mut s, "e");
    let editor = s.editor.as_mut().unwrap();
    editor.rows.iter_mut().find(|r| r.key == FormKey::Name).unwrap().value = "renamed".into();
    editor.dirty = true;
    let id = request_id(&keys(&mut s, "<C-s>"));
    let mut written = item("a");
    written.name = "renamed".into();
    event(&mut s, id, BwEvent::ItemWritten(Ok(written)));
    match s.undo.undo.last() {
        Some(UndoOp::Edit { id, before, after }) => {
            assert_eq!(id, "a");
            assert_eq!(before.name, "a");
            assert_eq!(after.name, "renamed");
        }
        other => panic!("expected an edit entry, got {other:?}"),
    }
}

#[test]
fn a_background_refresh_keeps_a_dirty_editor_on_screen() {
    let mut s = state();
    keys(&mut s, "e");
    s.editor.as_mut().unwrap().dirty = true;
    s.pending.insert(500, Pending::Load);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a"), item("b")]))));
    assert_eq!(s.screen, Screen::Editor);
    assert!(s.editor.as_ref().unwrap().dirty);
}

#[test]
fn a_failed_trash_drops_its_undo_and_reloads() {
    let mut s = state();
    let id = request_id(&keys(&mut s, "dd"));
    assert!(!s.undo.undo.is_empty());
    let effects = event(
        &mut s,
        id,
        BwEvent::ItemDeleted(Err(lazybw::bw::BwError::Other("synthetic network error".into()))),
    );
    assert!(s.undo.undo.is_empty(), "the undo entry describes a change that failed");
    assert!(
        s.pending.values().any(|p| matches!(p, Pending::Load)),
        "the vault should be reloaded to show its real state"
    );
    assert!(effects.iter().any(|e| matches!(e, Effect::Backend { .. })));
}

#[test]
fn the_first_load_does_not_pull_the_user_out_of_a_new_item() {
    let mut s = state();
    s.session = SessionPhase::Loading;
    keys(&mut s, "al"); // new login while the vault is still loading
    assert_eq!(s.screen, Screen::Editor);
    s.pending.insert(500, Pending::Load);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a")]))));
    assert_eq!(s.session, SessionPhase::Ready);
    assert_eq!(s.screen, Screen::Editor, "the load closed the form");
}

#[test]
fn ctrl_c_quits_from_the_unlock_login_and_error_screens() {
    for screen in [Screen::Unlock, Screen::Login, Screen::Error, Screen::Starting] {
        let mut s = state();
        s.screen = screen;
        s.session = SessionPhase::Locked;
        let effects = keys(&mut s, "<C-c>");
        assert!(
            effects.iter().any(|e| matches!(e, Effect::Quit)),
            "Ctrl-c did nothing on {screen:?}: {effects:?}"
        );
    }
}

#[test]
fn ctrl_o_logs_out_from_the_unlock_screen() {
    let mut s = state();
    s.screen = Screen::Unlock;
    s.session = SessionPhase::Locked;
    keys(&mut s, "secret");
    keys(&mut s, "<C-o>");
    assert!(s.confirm().is_some(), "Ctrl-o did not ask to log out");
}

#[test]
fn cancelling_a_key_sequence_clears_the_statusline() {
    let mut s = state();
    s.screen = Screen::Vault;
    s.session = SessionPhase::Ready;
    keys(&mut s, " ");
    assert_eq!(s.showcmd, "Space");
    keys(&mut s, "<Esc>");
    assert!(s.showcmd.is_empty(), "still showing {:?}", s.showcmd);
}

/// Draws frames 50 ms apart, as the main loop does while `animating()` holds,
/// and returns how long the screen took to settle.
fn settle(s: &mut AppState, width: u16, height: u16) -> std::time::Duration {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    let start = std::time::Instant::now();
    while s.animating() {
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "the animation never ends");
        terminal.draw(|frame| lazybw::ui::draw(frame, s)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    start.elapsed()
}

/// The main loop only redraws a moving screen while `animating()` says so.
/// It used to rely on the 1 s tick instead, so the 250 ms sweep after
/// unlocking took about two seconds and the right pane filled in late.
#[test]
fn the_sweep_after_unlocking_keeps_animating_until_it_has_run() {
    let mut s = state();
    s.fx = std::cell::RefCell::new(lazybw::ui::fx::Fx::new(true));
    s.size = (120, 36);
    s.enter_session(SessionPhase::Loading);
    s.pending.insert(500, Pending::Load);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a"), item("b")]))));
    assert!(s.animating(), "the sweep did not start");
    let took = settle(&mut s, 120, 36);
    assert!(took < std::time::Duration::from_millis(600), "the sweep took {took:?}");
}

/// The sweep belongs to opening the vault. A reload while it is open (the
/// background sync every 10 minutes, a folder change) used to sweep the
/// whole screen in again.
#[test]
fn a_reload_of_an_open_vault_does_not_sweep() {
    let mut s = state();
    s.fx = std::cell::RefCell::new(lazybw::ui::fx::Fx::new(true));
    s.pending.insert(500, Pending::Load);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a"), item("b")]))));
    assert_eq!(s.session, SessionPhase::Ready);
    assert!(!s.animating(), "a reload swept the screen");
}

#[test]
fn a_new_toast_animates_until_it_has_slid_in() {
    let mut s = state();
    s.fx = std::cell::RefCell::new(lazybw::ui::fx::Fx::new(true));
    assert!(!s.animating());
    s.toast(lazybw::app::state::MessageLevel::Ok, "Vault unlocked.");
    assert!(s.animating());
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!s.animating());
}

/// An effect only advances when a frame is drawn, and the "too small" screen
/// draws none; the effect is dropped so the loop does not draw forever.
#[test]
fn a_too_small_terminal_drops_running_effects() {
    let mut s = state();
    s.fx = std::cell::RefCell::new(lazybw::ui::fx::Fx::new(true));
    s.size = (40, 10);
    s.enter_session(SessionPhase::Loading);
    s.pending.insert(500, Pending::Load);
    event(&mut s, 500, BwEvent::Vault(Ok(snapshot(vec![item("a")]))));
    assert!(s.animating());
    settle(&mut s, 40, 10);
}

#[test]
fn tab_in_insert_moves_to_the_next_field_and_keeps_typing() {
    use lazybw::app::state::FormKey;
    let mut s = state();
    keys(&mut s, "al");
    keys(&mut s, "iLinear<Tab>");
    let editor = s.editor.as_ref().unwrap();
    assert!(editor.insert, "Tab left INSERT");
    assert_eq!(editor.rows[editor.row].key, FormKey::Folder);
    // The folder is a choice: typing on it changes nothing.
    keys(&mut s, "x<BS>");
    keys(&mut s, "<Tab>jane@example.com<S-Tab><Tab>");
    let editor = s.editor.as_ref().unwrap();
    let value = |key| editor.rows.iter().find(|row| row.key == key).unwrap().value.clone();
    assert_eq!(value(FormKey::Name), "Linear");
    assert_eq!(value(FormKey::Folder), "");
    assert_eq!(value(FormKey::Username), "jane@example.com");
    assert!(editor.insert);
    assert_eq!(editor.rows[editor.row].key, FormKey::Username);
    // gg is a NORMAL-mode move and leaves INSERT behind.
    keys(&mut s, "<Esc>gg");
    assert!(!s.editor.as_ref().unwrap().insert);
}

#[test]
fn which_key_opens_at_its_delay_not_at_the_next_tick() {
    use std::time::{Duration, Instant};
    let mut s = state();
    keys(&mut s, "<Space>");
    let since = s.dispatcher.since.expect("Space is pending");
    // The main loop sleeps until this deadline instead of the 1 s tick.
    assert_eq!(s.key_deadline(), Some(since + s.config.which_key_delay()));
    s.dispatcher.since = Some(Instant::now() - Duration::from_millis(350));
    update(&mut s, Msg::KeyTimer);
    assert!(s.which_key.is_some(), "which-key did not open at its deadline");
    // The leader is only a prefix: nothing else is due until the next key.
    assert_eq!(s.key_deadline(), None);
    keys(&mut s, "<Esc>");
    assert!(s.which_key.is_none());
    assert_eq!(s.key_deadline(), None);
}
