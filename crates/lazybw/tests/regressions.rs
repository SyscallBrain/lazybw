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
    let mut s = state();
    keys(&mut s, "e");
    let id = request_id(&keys(&mut s, "Ifirst <Esc><C-s>"));
    keys(&mut s, "Asecond<Esc>");
    let mut written = item("a");
    written.name = "first a".into();
    event(&mut s, id, BwEvent::ItemWritten(Ok(written)));
    let editor = s.editor.as_ref().unwrap();
    assert_eq!(editor.item.name, "first asecond", "the later edit was overwritten");
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
    let mut s = state();
    keys(&mut s, "e");
    let editor = s.editor.as_mut().unwrap();
    editor.item.name = "renamed".into();
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
    let value = |key| lazybw::app::form::value(&editor.item, key, &s.cache).to_owned();
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

// ---------------------------------------------------------------------------
// Review of 2026-11-10: actions that hit the wrong items, a quit that lost
// changes, a login that ignored the chosen server.
// ---------------------------------------------------------------------------

fn deletes(effects: &[Effect]) -> Vec<(String, bool)> {
    use lazybw::bw::service::CommandKind;
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend { kind: CommandKind::DeleteItem { id, permanent }, .. } => {
                Some((id.clone(), *permanent))
            }
            _ => None,
        })
        .collect()
}

fn five_items() -> AppState {
    let mut state = state();
    state.cache.load(
        ["a", "b", "c", "d", "e"].into_iter().map(item).collect(),
        vec![lazybw::bw::models::Folder {
            id: Some("f-social".into()),
            name: "Social media".into(),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    state.list_row = 0;
    state
}

#[test]
fn a_trash_clears_the_marks_so_the_next_one_acts_on_the_cursor() {
    let mut state = five_items();
    keys(&mut state, "x");
    assert_eq!(deletes(&keys(&mut state, "dd")), vec![("a".to_owned(), false)]);
    assert!(state.marks.is_empty(), "the marks outlived the action that used them");
    let under_cursor = state.selected_id().expect("an item under the cursor");
    assert_ne!(under_cursor, "a");
    assert_eq!(deletes(&keys(&mut state, "dd")), vec![(under_cursor, false)]);
}

#[test]
fn an_action_on_a_visual_selection_ends_it_and_a_shorter_list_cannot_panic() {
    let mut state = five_items();
    keys(&mut state, "G");
    keys(&mut state, "V");
    keys(&mut state, "k");
    let trashed = deletes(&keys(&mut state, "dd"));
    assert_eq!(trashed, vec![("d".to_owned(), false), ("e".to_owned(), false)]);
    assert_eq!(state.visual_anchor, None, "visual mode outlived its operator");
    // One more: used to read rows 3..=4 of a three-row list and panic.
    assert_eq!(deletes(&keys(&mut state, "dd")), vec![("c".to_owned(), false)]);

    // A selection whose far end is past a list that shrank under it (a sync
    // that removed items) is pulled back in.
    let mut state = five_items();
    keys(&mut state, "G");
    keys(&mut state, "V");
    keys(&mut state, "gg");
    state.cache.load(vec![item("a"), item("b")], vec![], vec![], vec![]);
    state.clamp_selection();
    assert_eq!(state.visual_anchor, Some(1));
    assert_eq!(state.target_ids(None), vec!["a".to_owned(), "b".to_owned()]);
    state.cache.load(vec![], vec![], vec![], vec![]);
    assert!(state.target_ids(None).is_empty());
}

#[test]
fn the_delete_dialog_counts_its_items_and_deletes_exactly_those() {
    let mut state = five_items();
    keys(&mut state, "x");
    keys(&mut state, "j");
    keys(&mut state, "x");
    keys(&mut state, "D");
    let message = state.confirm().map(|c| c.message.clone()).unwrap_or_default();
    assert!(message.starts_with("2 items (“a” and 1 other)"), "{message}");

    // The selection changes while the dialog is open (an answer from bw, a
    // reload): the dialog still deletes what it named.
    state.marks.clear();
    state.list_row = 4;
    let effects = keys(&mut state, "y");
    assert_eq!(deletes(&effects), vec![("a".to_owned(), true), ("b".to_owned(), true)]);
}

#[test]
fn the_mouse_does_not_reach_behind_a_dialog_or_the_editor() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = five_items();
    state.size = (160, 45);
    let plan = lazybw::ui::layout::LayoutPlan::compute(
        ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1),
        state.sidebar_visible,
    );
    let click = |state: &mut AppState, column: u16, row: u16| {
        update(
            state,
            Msg::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
        )
    };
    // Rows are two lines tall at this size: the third starts four lines
    // below the pane's border.
    let third_row = (plan.list.x + 4, plan.list.y + 1 + 4);

    // In the vault a click selects the row.
    click(&mut state, third_row.0, third_row.1);
    assert_eq!(state.list_row, 2);

    // Behind "Delete permanently?" it does nothing.
    state.list_row = 0;
    keys(&mut state, "D");
    click(&mut state, third_row.0, third_row.1);
    assert_eq!(state.list_row, 0);
    assert_eq!(deletes(&keys(&mut state, "y")), vec![("a".to_owned(), true)]);

    // Nor under the editor: not on the list, and not on a tab, which would
    // walk away from unsaved changes.
    state.list_row = 0;
    keys(&mut state, "e");
    assert_eq!(state.screen, Screen::Editor);
    click(&mut state, third_row.0, third_row.1);
    assert_eq!(state.list_row, 0);
    let effects = click(&mut state, plan.topbar.x + 22, plan.topbar.y);
    assert_eq!(state.screen, Screen::Editor);
    assert!(effects.is_empty());
}

fn typed(state: &mut AppState, text: &str) -> Vec<Effect> {
    let mut out = Vec::new();
    for ch in text.chars() {
        let notation = if ch == ' ' { "<Space>".to_owned() } else { ch.to_string() };
        out.extend(keys(state, &notation));
    }
    out
}

/// Runs a `:` command; the effects, without the history it saves.
fn command(state: &mut AppState, line: &str) -> Vec<Effect> {
    keys(state, ":");
    typed(state, line);
    let mut effects = keys(state, "<CR>");
    effects.retain(|effect| !matches!(effect, Effect::SaveHistory));
    effects
}

#[test]
fn moving_to_a_folder_that_does_not_exist_moves_nothing() {
    let mut state = five_items();
    let effects = command(&mut state, "move Socail media");
    assert!(effects.is_empty(), "{effects:?}");
    let toast = state.toasts.last().map(|t| t.title.clone()).unwrap_or_default();
    assert_eq!(toast, "No folder named “Socail media”. Nothing was moved.");

    // The real name, spaces and all, does move it.
    let effects = command(&mut state, "move Social media");
    assert_eq!(effects.len(), 1);
    assert_eq!(state.cache.item("a").unwrap().folder_id.as_deref(), Some("f-social"));
}

#[test]
fn quit_waits_for_changes_still_on_their_way_to_bitwarden() {
    let mut state = five_items();
    keys(&mut state, "x");
    keys(&mut state, "j");
    keys(&mut state, "x");
    let sent = keys(&mut state, "dd");
    let ids: Vec<u64> = sent
        .iter()
        .filter_map(|e| if let Effect::Backend { id, .. } = e { Some(*id) } else { None })
        .collect();
    assert_eq!(ids.len(), 2);

    // `q` with two deletes unanswered: stay, and say why.
    let effects = keys(&mut state, "q");
    assert!(effects.is_empty() && !state.quit, "quit with changes still queued");
    let message = state.status_message.as_ref().map(|m| m.text.clone()).unwrap_or_default();
    assert_eq!(message, "Finishing 2 changes before quitting. :q! quits now.");

    // The first answer is not enough; the last one quits.
    let effects = event(&mut state, ids[0], BwEvent::ItemDeleted(Ok("a".into())));
    assert!(!state.quit && !effects.iter().any(|e| matches!(e, Effect::Quit)));
    let effects = event(&mut state, ids[1], BwEvent::ItemDeleted(Ok("b".into())));
    assert!(state.quit && effects.iter().any(|e| matches!(e, Effect::Quit)));
}

#[test]
fn quit_with_changes_queued_can_be_forced_or_cancelled_and_stays_on_a_failure() {
    // `:q!` does not wait.
    let mut state = five_items();
    keys(&mut state, "dd");
    let effects = command(&mut state, "q!");
    assert!(state.quit && effects.iter().any(|e| matches!(e, Effect::Quit)));

    // Esc takes the quit back.
    let mut state = five_items();
    let sent = keys(&mut state, "dd");
    keys(&mut state, "q");
    assert!(state.quit_when_saved);
    keys(&mut state, "<Esc>");
    assert!(!state.quit_when_saved);
    event(&mut state, request_id(&sent), BwEvent::ItemDeleted(Ok("a".into())));
    assert!(!state.quit);

    // A change that fails keeps lazybw open, so the error is seen.
    let mut state = five_items();
    let sent = keys(&mut state, "dd");
    keys(&mut state, "q");
    let effects = event(
        &mut state,
        request_id(&sent),
        BwEvent::ItemDeleted(Err(lazybw::bw::BwError::Other("server said no".into()))),
    );
    assert!(!state.quit && !effects.iter().any(|e| matches!(e, Effect::Quit)));

    // With nothing queued `q` quits at once, as before.
    let mut state = five_items();
    let effects = keys(&mut state, "q");
    assert!(state.quit && effects.iter().any(|e| matches!(e, Effect::Quit)));
}

fn server_request(effects: &[Effect]) -> Option<(u64, String)> {
    use lazybw::bw::service::CommandKind;
    effects.iter().find_map(|effect| match effect {
        Effect::Backend { id, kind: CommandKind::ConfigServer(url) } => Some((*id, url.clone())),
        _ => None,
    })
}

#[test]
fn login_sets_the_chosen_server_before_it_starts() {
    use lazybw::app::msg::SuspendTask;
    let mut state = state();
    state.enter_session(SessionPhase::LoggedOut);

    // bitwarden.eu, then Enter twice: on the server, then on "Email and
    // password". bw must hear about the server before `bw login` runs.
    keys(&mut state, "j");
    keys(&mut state, "<CR>");
    let effects = keys(&mut state, "<CR>");
    let (id, url) = server_request(&effects).expect("bw config server");
    assert_eq!(url, "https://vault.bitwarden.eu");
    assert!(!effects.iter().any(|e| matches!(e, Effect::Suspend(_))), "login before the server");

    let effects = event(&mut state, id, BwEvent::ServerConfigured(Ok(())));
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Suspend(SuspendTask::Login { sso: false }))),
        "{effects:?}"
    );

    // Again on the same server (a mistyped password): no second round trip.
    let effects = keys(&mut state, "<CR>");
    assert!(server_request(&effects).is_none());
    assert!(effects.iter().any(|e| matches!(e, Effect::Suspend(_))));

    // The default choice is set too: bw may still point at another server.
    let mut state = self::state();
    state.enter_session(SessionPhase::LoggedOut);
    keys(&mut state, "<Tab>");
    let effects = keys(&mut state, "<CR>");
    assert_eq!(
        server_request(&effects).map(|(_, url)| url).as_deref(),
        Some("https://vault.bitwarden.com")
    );
}

#[test]
fn a_server_bw_refuses_stops_the_login() {
    let mut state = state();
    state.enter_session(SessionPhase::LoggedOut);
    keys(&mut state, "<Tab>");
    let effects = keys(&mut state, "<CR>");
    let (id, _) = server_request(&effects).expect("bw config server");
    let effects =
        event(&mut state, id, BwEvent::ServerConfigured(Err(lazybw::bw::BwError::LogoutRequired)));
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(state.login.error.as_deref(), Some("Logout required before server config update."));
}

// ---------------------------------------------------------------------------
// Review of 2026-11-10, the rest: keys, editor, settings, security.
// ---------------------------------------------------------------------------

fn login_item(id: &str, password: &str) -> Item {
    let mut item = item(id);
    item.login.as_mut().unwrap().password = Some(lazybw::bw::sensitive::Sensitive::new(password));
    item
}

fn saved_settings(effects: &[Effect]) -> Vec<(String, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SaveConfig(updates) => Some(updates.clone()),
            _ => None,
        })
        .flatten()
        .map(|(path, value)| (path.join("."), value.to_string().trim().to_owned()))
        .collect()
}

fn puts(effects: &[Effect]) -> Vec<Item> {
    use lazybw::bw::service::CommandKind;
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend { kind: CommandKind::PutItem { item, .. }, .. } => {
                Some((**item).clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn d_in_visual_mode_acts_at_once() {
    let mut state = five_items();
    keys(&mut state, "V");
    keys(&mut state, "j");
    // `dd` in the list behind visual mode used to hold `d` for timeoutlen.
    let effects = keys(&mut state, "d");
    assert_eq!(deletes(&effects).len(), 2);
    assert!(!state.dispatcher.is_pending());
}

#[test]
fn the_command_line_edits_at_its_cursor_and_remembers_commands() {
    let mut state = five_items();
    keys(&mut state, ":");
    typed(&mut state, "abc");
    keys(&mut state, "<Left>");
    keys(&mut state, "<BS>");
    assert_eq!(state.cmdline().map(|c| (c.input.clone(), c.cursor)), Some(("ac".into(), 1)));
    keys(&mut state, "<End>");
    typed(&mut state, " def");
    keys(&mut state, "<C-w>");
    assert_eq!(state.cmdline().unwrap().input, "ac ");
    keys(&mut state, "<Esc>");

    let effects = command(&mut state, "sort modified");
    assert!(effects.is_empty());
    let mut effects = Vec::new();
    keys(&mut state, ":");
    effects.extend(typed(&mut state, "sort name"));
    effects.extend(keys(&mut state, "<CR>"));
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveHistory)));
    assert_eq!(state.command_history, vec!["sort modified".to_owned(), "sort name".to_owned()]);

    // Up goes back through what was run, Down returns to what was typed.
    keys(&mut state, ":");
    keys(&mut state, "<Up>");
    assert_eq!(state.cmdline().unwrap().input, "sort name");
    keys(&mut state, "<Up>");
    assert_eq!(state.cmdline().unwrap().input, "sort modified");
    keys(&mut state, "<Down><Down>");
    assert_eq!(state.cmdline().unwrap().input, "");
    // Only the entries that start with what is typed.
    typed(&mut state, "sort m");
    keys(&mut state, "<Up>");
    assert_eq!(state.cmdline().unwrap().input, "sort modified");
}

#[test]
fn rebinding_the_insert_and_confirm_contexts_works() {
    let text = "[keys.insert]\n\"<C-j>\" = \"editor.normal\"\n\"<Esc>\" = \"noop\"\n\
                [keys.confirm]\n\"o\" = \"confirm.yes\"\n";
    let (config, _) = lazybw::config::Config::from_toml(text).unwrap();
    let mut state = five_items();
    state.keymap = lazybw::config::keymap::Keymap::build(&config.keys).unwrap().0;
    keys(&mut state, "e");
    keys(&mut state, "i");
    keys(&mut state, "<Esc>");
    assert!(state.editor.as_ref().unwrap().insert, "Esc was unbound in insert");
    keys(&mut state, "<C-j>");
    assert!(!state.editor.as_ref().unwrap().insert, "<C-j> was bound to editor.normal");
    keys(&mut state, "ZQ");

    keys(&mut state, "D");
    assert!(state.confirm().is_some());
    let effects = keys(&mut state, "o");
    assert_eq!(deletes(&effects), vec![("a".to_owned(), true)]);
}

#[test]
fn page_motions_use_the_height_on_screen() {
    let mut state = state();
    state.cache.load((0..100).map(|n| item(&format!("i{n:03}"))).collect(), vec![], vec![], vec![]);
    state.size = (120, 36);
    // What the last frame showed.
    state.ui_pages.borrow_mut().insert("list", 30);
    state.ui_scroll.borrow_mut().insert("list", 10);
    state.list_row = 10;
    keys(&mut state, "<C-d>");
    assert_eq!(state.list_row, 25);
    keys(&mut state, "<C-f>");
    assert_eq!(state.list_row, 55);
    state.list_row = 12;
    keys(&mut state, "L");
    assert_eq!(state.list_row, 39);
    keys(&mut state, "M");
    assert_eq!(state.list_row, 24);
    keys(&mut state, "H");
    assert_eq!(state.list_row, 10);

    // Help scrolls by pages and stops at its end.
    keys(&mut state, "?");
    state.ui_pages.borrow_mut().insert("help", 20);
    state.ui_scroll.borrow_mut().insert("help.max", 45);
    keys(&mut state, "<C-f>");
    assert_eq!(state.help_offset, 20);
    keys(&mut state, "G");
    assert_eq!(state.help_offset, 45);
    keys(&mut state, "j");
    assert_eq!(state.help_offset, 45, "scrolled past the end");
    keys(&mut state, "k");
    assert_eq!(state.help_offset, 44);
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let mut state = five_items();
    state.size = (160, 45);
    let plan = lazybw::ui::layout::LayoutPlan::compute(
        ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1),
        true,
    );
    let wheel = |state: &mut AppState, column: u16, row: u16, kind: MouseEventKind| {
        update(state, Msg::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }))
    };
    wheel(&mut state, plan.list.x + 3, plan.list.y + 3, MouseEventKind::ScrollDown);
    assert_eq!((state.focus, state.list_row), (lazybw::app::state::Focus::List, 3));
    let sidebar = state.sidebar_row;
    wheel(&mut state, plan.sidebar.x + 3, plan.sidebar.y + 3, MouseEventKind::ScrollDown);
    assert_eq!(state.focus, lazybw::app::state::Focus::Sidebar);
    assert_ne!(state.sidebar_row, sidebar);
    assert_eq!(state.list_row, 3);

    // A pointer that only moves draws nothing.
    state.dirty = false;
    wheel(&mut state, 5, 5, MouseEventKind::Moved);
    assert!(!state.dirty);
}

#[test]
fn time_asleep_counts_toward_the_auto_lock() {
    let mut state = five_items();
    state.config.security.auto_lock = lazybw::config::duration::DurationSpec::parse("15m").unwrap();
    // The monotonic clock says just now; the wall clock says an hour ago,
    // as after a laptop slept with the lid closed.
    state.touch();
    state.last_input_wall = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    update(&mut state, Msg::Tick);
    assert_eq!(state.session, SessionPhase::Locked);
}

fn editing(state: &mut AppState) {
    keys(state, "e");
    assert_eq!(state.screen, Screen::Editor);
}

#[test]
fn the_notes_editor_saves_with_ctrl_s_and_takes_pastes() {
    let mut state = five_items();
    editing(&mut state);
    keys(&mut state, "G<CR>");
    assert!(state.notes.is_some(), "the notes editor did not open");
    keys(&mut state, "i");
    update(&mut state, Msg::Paste("line one\nline two".into()));
    let effects = keys(&mut state, "<C-s>");
    let saved = puts(&effects);
    assert_eq!(saved.len(), 1);
    assert_eq!(
        saved[0].notes.as_ref().map(|n| n.expose().to_owned()).as_deref(),
        Some("line one\nline two")
    );
    assert!(state.notes.is_some(), "Ctrl-s closed the notes editor");

    // Leaving the notes without a change leaves the item clean.
    let mut state = five_items();
    editing(&mut state);
    keys(&mut state, "G<CR>");
    keys(&mut state, "<Tab>");
    assert!(!state.editor.as_ref().unwrap().dirty);
}

#[test]
fn a_new_item_is_created_once_however_often_it_is_saved() {
    use lazybw::bw::service::CommandKind;
    let mut state = five_items();
    command(&mut state, "new login");
    keys(&mut state, "i");
    typed(&mut state, "Fresh");
    keys(&mut state, "<Esc>");
    let mut effects = keys(&mut state, "<C-s>");
    effects.extend(keys(&mut state, "<C-s>"));
    let creates = effects
        .iter()
        .filter(|e| matches!(e, Effect::Backend { kind: CommandKind::CreateItem(_), .. }))
        .count();
    assert_eq!(creates, 1);
}

#[test]
fn reloading_from_the_server_replaces_the_form() {
    let mut state = five_items();
    editing(&mut state);
    keys(&mut state, "A");
    typed(&mut state, "-local");
    keys(&mut state, "<Esc>");
    let effects = command(&mut state, "e!");
    let mut remote = item("a");
    remote.name = "a-from-the-server".into();
    event(&mut state, request_id(&effects), BwEvent::Item(Ok(remote)));
    let editor = state.editor.as_ref().unwrap();
    assert_eq!(editor.item.name, "a-from-the-server");
    assert!(!editor.dirty);
}

#[test]
fn json_edited_from_the_list_opens_in_the_editor() {
    use lazybw::app::msg::Suspended;
    let mut state = five_items();
    let mut edited = item("a");
    edited.name = "renamed in JSON".into();
    let json = serde_json::to_string(&edited).unwrap();
    update(
        &mut state,
        Msg::Suspended(Suspended::ItemJson(lazybw::bw::sensitive::Sensitive::new(json))),
    );
    let editor = state.editor.as_ref().expect("the editor opened");
    assert_eq!(state.screen, Screen::Editor);
    assert!(editor.dirty);
    assert_eq!(editor.item.name, "renamed in JSON");
    assert_eq!(editor.original_item.name, "a");
}

#[test]
fn quick_open_finds_its_item_through_an_active_search() {
    let mut state = five_items();
    keys(&mut state, "/");
    typed(&mut state, "a");
    keys(&mut state, "<CR>");
    keys(&mut state, "<C-p>");
    typed(&mut state, "c");
    let target = state.quick_open().and_then(|q| q.results.first().cloned());
    assert_eq!(target.as_deref(), Some("c"));
    keys(&mut state, "<CR>");
    assert_eq!(state.selected_id().as_deref(), Some("c"));
}

#[test]
fn a_tab_never_leaves_an_editor_with_changes_behind() {
    let mut state = five_items();
    editing(&mut state);
    keys(&mut state, "A");
    typed(&mut state, "-edited");
    keys(&mut state, "<Esc>");
    keys(&mut state, "gt");
    assert_eq!(state.screen, Screen::Editor, "gt left unsaved changes behind");
    // Help and back returns to the editor, not to the vault.
    keys(&mut state, "?");
    keys(&mut state, "<Esc>");
    assert_eq!(state.screen, Screen::Editor);
    // A clean editor just closes.
    keys(&mut state, "ZQ");
    editing(&mut state);
    keys(&mut state, "gt");
    assert_eq!(state.screen, Screen::Generator);
    assert!(state.editor.is_none());
}

#[test]
fn sigusr1_while_logged_out_changes_nothing() {
    let mut state = state();
    state.enter_session(SessionPhase::LoggedOut);
    let effects = update(&mut state, Msg::LockSignal);
    assert!(effects.is_empty());
    assert_eq!(state.screen, Screen::Login);
}

#[test]
fn editing_a_detail_field_opens_the_editor_on_that_field() {
    use lazybw::app::state::FormKey;
    let mut state = state();
    state.cache.load(vec![login_item("a", "old")], vec![], vec![], vec![]);
    keys(&mut state, "l");
    keys(&mut state, "j"); // PASSWORD
    keys(&mut state, "cc");
    let editor = state.editor.as_ref().unwrap();
    let row = &editor.rows[editor.row];
    assert_eq!(row.key, FormKey::Password);
    let value = lazybw::app::form::value(&editor.item, row.key, &state.cache);
    assert!(editor.insert && value.is_empty(), "cc replaces the value");
}

#[test]
fn set_checks_values_and_saves_them_as_config_toml_expects() {
    let mut state = five_items();
    let effects = command(&mut state, "set mouse=maybe");
    assert!(saved_settings(&effects).is_empty(), "a typo reached config.toml");
    assert!(state.status_message.as_ref().unwrap().text.starts_with("E474"));

    state.config.ui.mouse = false;
    let effects = command(&mut state, "set mouse=on");
    assert!(state.config.ui.mouse);
    assert_eq!(saved_settings(&effects), vec![("ui.mouse".to_owned(), "true".to_owned())]);

    let effects = command(&mut state, "set icons=emoji");
    assert!(saved_settings(&effects).is_empty());
    let effects = command(&mut state, "set length=500");
    assert!(saved_settings(&effects).is_empty());
    let effects = command(&mut state, "set length=40");
    assert_eq!(saved_settings(&effects), vec![("generator.length".to_owned(), "40".to_owned())]);
    let effects = command(&mut state, "set separator=1");
    assert_eq!(
        saved_settings(&effects),
        vec![("generator.separator".to_owned(), "\"1\"".to_owned())]
    );
}

#[test]
fn a_theme_picked_from_the_list_is_kept() {
    let mut state = five_items();
    keys(&mut state, "<Space>t");
    keys(&mut state, "<Down>");
    let effects = keys(&mut state, "<CR>");
    let theme = state.config.ui.theme.clone();
    assert_eq!(saved_settings(&effects), vec![("ui.theme".to_owned(), format!("\"{theme}\""))]);
}

#[test]
fn the_editor_command_setting_is_typed_in_a_prompt() {
    let mut state = five_items();
    keys(&mut state, "<Space>,");
    let index = lazybw::app::update::settings_rows(&state)
        .iter()
        .position(|row| row.key == "editor.command")
        .unwrap();
    state.settings.index = index;
    keys(&mut state, "<CR>");
    assert!(state.prompt().is_some());
    typed(&mut state, "nvim");
    let effects = keys(&mut state, "<CR>");
    assert_eq!(state.config.editor.command, "nvim");
    assert_eq!(
        saved_settings(&effects),
        vec![("editor.command".to_owned(), "\"nvim\"".to_owned())]
    );
}

#[test]
fn the_export_wizard_follows_the_format_and_asks_before_replacing_a_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let existing = dir.path().join("vault.json");
    std::fs::write(&existing, "{}").unwrap();
    let mut state = five_items();
    keys(&mut state, "<Space>e");
    let json = state.export().unwrap().path.clone();
    assert!(json.ends_with(".json"));
    keys(&mut state, "jj"); // CSV
    let csv = state.export().unwrap().path.clone();
    assert_eq!(csv, json.replace(".json", ".csv"));

    keys(&mut state, "k"); // back to JSON encrypted with the account
    if let Some(lazybw::app::state::Overlay::Export(export)) = &mut state.overlay {
        export.path = existing.display().to_string();
    }
    keys(&mut state, "<CR>");
    let export = state.export().unwrap();
    assert_eq!(export.step, lazybw::app::state::ExportStep::Format);
    assert!(export.error.as_deref().unwrap().contains("Enter again replaces it"));
    keys(&mut state, "<CR>");
    assert_eq!(state.export().unwrap().step, lazybw::app::state::ExportStep::MasterPassword);
}

#[test]
fn esc_closes_the_import_wizard_while_it_runs() {
    use lazybw::app::state::Pending;
    let mut state = five_items();
    command(&mut state, "import");
    state.pending.insert(99, Pending::Import);
    keys(&mut state, "<Esc>");
    assert!(state.import().is_none());
    assert!(state.importing(), "the import itself goes on");
}

#[test]
fn locking_leaves_a_clipboard_lazybw_did_not_fill() {
    use lazybw::clipboard::ClipboardBackend;
    let backend = std::sync::Arc::new(lazybw::clipboard::MemoryClipboard::new());
    let mut state = five_items();
    state.clipboard = lazybw::clipboard::Clipboard::with_backend(backend.clone());
    backend.set("copied in the browser").unwrap();
    update(&mut state, Msg::LockSignal);
    assert_eq!(backend.contents().as_deref(), Some("copied in the browser"));
}

#[test]
fn the_verification_code_of_a_protected_item_asks_first_from_the_detail_pane() {
    let mut state = state();
    let mut protected = login_item("p", "pw");
    protected.reprompt = 1;
    protected.login.as_mut().unwrap().totp =
        Some(lazybw::bw::sensitive::Sensitive::new("JBSWY3DPEHPK3PXP"));
    state.cache.load(vec![protected], vec![], vec![], vec![]);
    keys(&mut state, "l");
    keys(&mut state, "jj"); // VERIFICATION CODE
    keys(&mut state, "yy");
    assert!(state.reprompt().is_some());
    assert!(state.clipboard.last_copied().is_none());
}

#[test]
fn the_health_check_uses_the_setting_as_it_is_now() {
    use lazybw::bw::service::CommandKind;
    let mut state = five_items();
    command(&mut state, "set check_exposed=false");
    let effects = keys(&mut state, "<Space>h");
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Backend { kind: CommandKind::Health { check_exposed: false }, .. }
    )));
}

#[test]
fn gen_with_a_length_copies_a_password_and_sort_bang_reverses() {
    use lazybw::bw::service::CommandKind;
    let mut state = five_items();
    let effects = command(&mut state, "gen 32");
    let id = request_id(&effects);
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Backend { kind: CommandKind::Generate(request), .. } if request.length == 32 && !request.passphrase
    )));
    assert_eq!(state.screen, Screen::Vault, ":gen 32 stays where it is");
    event(&mut state, id, BwEvent::Generated(Ok("generated-32".into())));
    assert_eq!(state.clipboard.last_copied(), Some("generated-32"));

    let effects = command(&mut state, "gen phrase 6");
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Backend { kind: CommandKind::Generate(request), .. } if request.words == 6 && request.passphrase
    )));

    command(&mut state, "sort name!");
    assert!(state.sort_reversed);
    command(&mut state, "sort name");
    assert!(!state.sort_reversed);
}

#[test]
fn changing_a_generator_option_leaves_the_session_history_alone() {
    let mut state = five_items();
    let effects = keys(&mut state, "<Space>g");
    event(&mut state, request_id(&effects), BwEvent::Generated(Ok("first".into())));
    assert_eq!(state.generator.history.len(), 1);
    let effects = keys(&mut state, "l"); // length + 1
    event(&mut state, request_id(&effects), BwEvent::Generated(Ok("second".into())));
    assert_eq!(state.generator.output, "second");
    assert_eq!(state.generator.history.len(), 1);
    let effects = keys(&mut state, "r");
    event(&mut state, request_id(&effects), BwEvent::Generated(Ok("third".into())));
    assert_eq!(state.generator.history.len(), 2);
}

fn folder(id: &str, name: &str) -> lazybw::bw::models::Folder {
    lazybw::bw::models::Folder { id: Some(id.into()), name: name.into(), ..Default::default() }
}

/// Two folders named "Work" (Bitwarden allows it), one with a subfolder, and
/// a "Home" folder.
fn twin_folders() -> AppState {
    let mut state = state();
    let mut a = item("a");
    a.folder_id = Some("w1".into());
    let mut b = item("b");
    b.folder_id = Some("w2".into());
    state.cache.load(
        vec![a, b, item("c")],
        vec![
            folder("w1", "Work"),
            folder("w2", "Work"),
            folder("w3", "Work/Clients"),
            folder("h", "Home"),
        ],
        vec![],
        vec![],
    );
    state
}

fn folder_commands(effects: &[Effect]) -> Vec<String> {
    use lazybw::bw::service::CommandKind;
    let mut out: Vec<String> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend { kind: CommandKind::DeleteFolder(id), .. } => {
                Some(format!("delete {id}"))
            }
            Effect::Backend { kind: CommandKind::RenameFolder { id, name }, .. } => {
                Some(format!("rename {id} {name}"))
            }
            _ => None,
        })
        .collect();
    out.sort();
    out
}

#[test]
fn deleting_a_folder_deletes_every_folder_with_its_name() {
    let mut state = twin_folders();
    command(&mut state, "folder delete Work");
    let message = state.confirm().map(|c| c.message.clone()).unwrap_or_default();
    assert!(message.contains("Its 2 items move to No folder"), "{message}");
    // The dialog acts on the folders it named, wherever the sidebar is now.
    state.filter = Filter::Folder("Home".into());
    let effects = keys(&mut state, "y");
    assert_eq!(folder_commands(&effects), vec!["delete w1", "delete w2"]);
}

#[test]
fn deleting_a_folder_with_its_subfolders_uses_their_ids() {
    let mut state = twin_folders();
    command(&mut state, "folder delete Work");
    keys(&mut state, "<Space>");
    let effects = keys(&mut state, "y");
    assert_eq!(folder_commands(&effects), vec!["delete w1", "delete w2", "delete w3"]);
}

#[test]
fn renaming_a_folder_renames_every_folder_with_its_name_and_undo_puts_them_back() {
    let mut state = twin_folders();
    state.filter = Filter::Folder("Work".into());
    command(&mut state, "folder rename Jobs");
    state.filter = Filter::Folder("Home".into());
    let effects = keys(&mut state, "y");
    assert_eq!(
        folder_commands(&effects),
        vec!["rename w1 Jobs", "rename w2 Jobs", "rename w3 Jobs/Clients"]
    );
    state.cache.set_folders(vec![
        folder("w1", "Jobs"),
        folder("w2", "Jobs"),
        folder("w3", "Jobs/Clients"),
        folder("h", "Home"),
    ]);
    let effects = keys(&mut state, "u");
    assert_eq!(
        folder_commands(&effects),
        vec!["rename w1 Work", "rename w2 Work", "rename w3 Work/Clients"]
    );
}

#[test]
fn go_to_folder_lists_a_shared_name_once() {
    let mut state = twin_folders();
    keys(&mut state, "<Space>ff");
    let Some(lazybw::app::state::Overlay::Picker(picker)) = &state.overlay else {
        panic!("no picker");
    };
    let work = picker.entries.iter().filter(|entry| entry.label == "Work").count();
    assert_eq!(work, 1);
}

#[test]
fn the_editor_saves_what_it_shows() {
    use lazybw::app::state::FormKey;
    use lazybw::bw::service::CommandKind;
    let mut s = state();
    keys(&mut s, "e");
    let go = |s: &mut AppState, key: FormKey| {
        let editor = s.editor.as_mut().unwrap();
        editor.row = editor.rows.iter().position(|row| row.key == key).unwrap();
    };
    keys(&mut s, "A-2<Esc>");
    go(&mut s, FormKey::Username);
    keys(&mut s, "ime<Esc>");
    go(&mut s, FormKey::UriAdd);
    keys(&mut s, "<CR>");
    go(&mut s, FormKey::Uri(0));
    keys(&mut s, "iexample.com<Esc>");
    let shown = |s: &AppState, key| {
        lazybw::app::form::value(&s.editor.as_ref().unwrap().item, key, &s.cache).to_owned()
    };
    assert_eq!(shown(&s, FormKey::Uri(0)), "example.com");
    // Undo and redo move the item itself, so the form cannot drift from it.
    keys(&mut s, "u");
    assert_eq!(shown(&s, FormKey::Uri(0)), "");
    keys(&mut s, "<C-r>");
    assert_eq!(shown(&s, FormKey::Uri(0)), "example.com");

    let effects = keys(&mut s, "<C-s>");
    let saved = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Backend { kind: CommandKind::PutItem { item, .. }, .. } => Some(item.clone()),
            _ => None,
        })
        .expect("a save");
    assert_eq!(saved.name, "a-2");
    let login = saved.login.unwrap();
    assert_eq!(login.username.unwrap().expose(), "me");
    let uris: Vec<&str> = login.uris.as_deref().unwrap().iter().map(|u| u.uri_str()).collect();
    assert_eq!(uris, vec!["example.com"]);
}

#[test]
fn a_website_added_and_left_empty_is_not_saved() {
    use lazybw::app::state::FormKey;
    use lazybw::bw::service::CommandKind;
    let mut s = state();
    keys(&mut s, "e");
    let editor = s.editor.as_mut().unwrap();
    editor.row = editor.rows.iter().position(|row| row.key == FormKey::UriAdd).unwrap();
    keys(&mut s, "<CR>");
    let editor = s.editor.as_ref().unwrap();
    assert!(editor.rows.iter().any(|row| row.key == FormKey::Uri(0)), "no row for the website");
    let effects = keys(&mut s, "<C-s>");
    let saved = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Backend { kind: CommandKind::PutItem { item, .. }, .. } => Some(item.clone()),
            _ => None,
        })
        .expect("a save");
    assert!(saved.login.unwrap().uris.unwrap_or_default().is_empty());
}
