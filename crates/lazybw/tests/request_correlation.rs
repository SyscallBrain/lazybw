//! Asynchronous responses belong to the editor instance/request that sent them.
use lazybw::app::update::update;
use lazybw::app::{AppState, Effect, Msg, Screen, SessionPhase};
use lazybw::bw::models::{Item, ItemType, Login};
use lazybw::bw::service::{BackendEvent, BwEvent, CommandKind};

fn state() -> AppState {
    let config = lazybw::config::Config::default();
    let theme = lazybw::theme::builtin::by_name("tokyonight-night").unwrap();
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let mut s = AppState::new(
        config,
        theme,
        keymap,
        lazybw::config::Paths {
            config_dir: "/tmp/lazybw-correlation-unused".into(),
            state_dir: "/tmp/lazybw-correlation-unused".into(),
        },
    );
    s.clipboard = lazybw::clipboard::Clipboard::memory();
    s.session = SessionPhase::Ready;
    s.screen = Screen::Vault;
    s.cache.load(
        ["a", "b"]
            .map(|id| {
                let mut item = Item::new(ItemType::Login);
                item.id = Some(id.into());
                item.name = id.into();
                item.login = Some(Login::default());
                item
            })
            .into(),
        vec![],
        vec![],
        vec![],
    );
    s
}
fn keys(s: &mut AppState, text: &str) -> Vec<Effect> {
    Msg::key(text).into_iter().flat_map(|m| update(s, m)).collect()
}
fn command(s: &mut AppState, text: &str) -> Vec<Effect> {
    keys(s, ":");
    update(s, Msg::Paste(text.into()));
    keys(s, "<CR>")
}
fn event(s: &mut AppState, id: u64, event: BwEvent) -> Vec<Effect> {
    update(s, Msg::Backend(BackendEvent { id, event }))
}
fn saved_request(effects: Vec<Effect>) -> (u64, Item) {
    effects
        .into_iter()
        .find_map(|e| match e {
            Effect::Backend {
                id,
                kind: CommandKind::CreateItem(item) | CommandKind::PutItem { item, .. },
            } => Some((id, *item)),
            _ => None,
        })
        .expect("save request")
}
fn generation_id(effects: Vec<Effect>) -> u64 {
    effects
        .into_iter()
        .find_map(|e| match e {
            Effect::Backend { id, kind: CommandKind::Generate(_) } => Some(id),
            _ => None,
        })
        .expect("generation request")
}
fn draft(s: &mut AppState, name: &str) {
    command(s, "new login");
    keys(s, "i");
    update(s, Msg::Paste(name.into()));
    keys(s, "<Esc>");
}
fn generate_for_selected(s: &mut AppState) -> u64 {
    keys(s, "gp");
    assert!(s.confirm().is_some());
    generation_id(keys(s, "y"))
}
#[test]
fn replacing_a_draft_preserves_the_new_editor() {
    let mut s = state();
    draft(&mut s, "Draft A");
    let (id, mut saved) = saved_request(keys(&mut s, "<C-s>"));
    keys(&mut s, "ZQ");
    draft(&mut s, "Draft B");
    saved.id = Some("saved-a".into());
    event(&mut s, id, BwEvent::ItemWritten(Ok(saved)));
    let editor = s.editor.as_ref().unwrap();
    assert_eq!(editor.item.name, "Draft B");
    assert!(editor.item.id.is_none());
    assert!(editor.is_new && editor.dirty);
    assert_eq!(s.cache.item("saved-a").unwrap().name, "Draft A");
}
#[test]
fn a_different_draft_can_save_while_the_previous_creation_is_pending() {
    let mut s = state();
    draft(&mut s, "Draft A");
    saved_request(keys(&mut s, "<C-s>"));
    assert!(keys(&mut s, "<C-s>").is_empty(), "same draft must not create twice");
    keys(&mut s, "ZQ");
    draft(&mut s, "Draft B");
    let (_, saved) = saved_request(keys(&mut s, "<C-s>"));
    assert_eq!(saved.name, "Draft B");
}
#[test]
fn reopening_the_same_item_does_not_reuse_the_old_save() {
    let mut s = state();
    keys(&mut s, "eIfirst <Esc>");
    let (id, saved) = saved_request(keys(&mut s, "ZZ"));
    let revision = s.editor.as_ref().unwrap().history.revision;
    keys(&mut s, "ZQeIlater <Esc>");
    assert_eq!(s.editor.as_ref().unwrap().history.revision, revision);
    event(&mut s, id, BwEvent::ItemWritten(Ok(saved)));
    let editor = s.editor.as_ref().expect("old save must not close reopened editor");
    assert_eq!(editor.item.name, "later a");
    assert_eq!(editor.original_item.name, "a");
    assert!(editor.dirty);
}
#[test]
fn creating_a_draft_keeps_newer_edits_and_accepts_its_server_id() {
    let mut s = state();
    draft(&mut s, "Draft A");
    let (id, mut saved) = saved_request(keys(&mut s, "ZZ"));
    keys(&mut s, "A newer<Esc>");
    saved.id = Some("saved-a".into());
    event(&mut s, id, BwEvent::ItemWritten(Ok(saved)));
    let editor = s.editor.as_ref().unwrap();
    assert_eq!(editor.item.name, "Draft A newer");
    assert_eq!(editor.item.id.as_deref(), Some("saved-a"));
    assert!(!editor.is_new);
    assert!(editor.dirty);
}
#[test]
fn ordinary_generation_does_not_consume_an_item_generation_target() {
    let mut s = state();
    let ordinary = generation_id(command(&mut s, "gen 12"));
    let targeted = generate_for_selected(&mut s);
    let effects = event(&mut s, ordinary, BwEvent::Generated(Ok("ordinary".into())));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Backend { kind: CommandKind::PutItem { .. }, .. }))
    );
    assert_eq!(s.pending_writes(), 1);
    let (_, item) = saved_request(event(&mut s, targeted, BwEvent::Generated(Ok("for-a".into()))));
    assert_eq!(item.id.as_deref(), Some("a"));
    assert_eq!(item.login.unwrap().password.unwrap().expose(), "for-a");
}
#[test]
fn concurrent_item_generations_keep_their_targets_when_answers_reverse() {
    let mut s = state();
    let first = generate_for_selected(&mut s);
    keys(&mut s, "j");
    let second = generate_for_selected(&mut s);
    assert_eq!(s.pending_writes(), 2);
    for (request, target, password) in [(second, "b", "for-b"), (first, "a", "for-a")] {
        let (save, item) =
            saved_request(event(&mut s, request, BwEvent::Generated(Ok(password.into()))));
        assert_eq!(item.id.as_deref(), Some(target));
        assert_eq!(item.login.as_ref().unwrap().password.as_ref().unwrap().expose(), password);
        event(&mut s, save, BwEvent::ItemWritten(Ok(item)));
    }
    assert_eq!(s.pending_writes(), 0);
}
#[test]
fn failed_generation_releases_its_target_and_cannot_poison_later_results() {
    let mut s = state();
    let targeted = generate_for_selected(&mut s);
    event(&mut s, targeted, BwEvent::Generated(Err(lazybw::bw::BwError::InvalidPassword)));
    assert_eq!(s.pending_writes(), 0);
    assert!(s.generator.error.is_some());
    let ordinary = generation_id(command(&mut s, "gen 12"));
    let effects = event(&mut s, ordinary, BwEvent::Generated(Ok("ordinary".into())));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Backend { kind: CommandKind::PutItem { .. }, .. }))
    );
}
#[test]
fn forgetting_a_failed_send_also_forgets_its_generation_target() {
    let mut s = state();
    let targeted = generate_for_selected(&mut s);
    // The effect runner removes the pending request if sending fails.
    s.pending.remove(&targeted);
    assert_eq!(s.pending_writes(), 0);
    assert!(event(&mut s, targeted, BwEvent::Generated(Ok("late".into()))).is_empty());
    assert!(s.generator.output.is_empty());
}
#[test]
fn lock_discards_generation_and_save_responses() {
    let mut s = state();
    let targeted = generate_for_selected(&mut s);
    keys(&mut s, "e");
    let (save, item) = saved_request(keys(&mut s, "<C-s>"));
    update(&mut s, Msg::LockSignal);
    assert_eq!(s.pending_writes(), 0);
    assert!(event(&mut s, targeted, BwEvent::Generated(Ok("late".into()))).is_empty());
    assert!(event(&mut s, save, BwEvent::ItemWritten(Ok(item))).is_empty());
    assert!(s.generator.output.is_empty());
    assert!(s.editor.is_none());
    assert!(s.cache.item("a").is_none());
}
