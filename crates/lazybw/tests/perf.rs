//! Performance guard-rails with the 5,000-item fixture.
//!
//! The thresholds are the performance targets with a 3× allowance for debug
//! builds; a regression that makes a frame or a keystroke scale badly fails.

use lazybw::app::state::{AppState, Screen, SessionPhase};
use lazybw::bw::models::{Folder, ItemType};
use lazybw::config::Paths;
use lazybw::theme::builtin;
use lazybw::vault::cache::{Filter, VaultCache, synthetic_item};
use std::time::Instant;

fn big_state() -> AppState {
    let config = lazybw::config::Config::default();
    let mut theme = builtin::by_name("tokyonight-night").unwrap();
    theme.resolve(true, false);
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).unwrap();
    let mut state = AppState::new(
        config,
        theme,
        keymap,
        Paths { config_dir: "/tmp/perf/config".into(), state_dir: "/tmp/perf/state".into() },
    );
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.screen = Screen::Vault;
    state.session = SessionPhase::Ready;
    state.size = (160, 45);
    let items: Vec<_> = (0..5000).map(synthetic_item).collect();
    let folders = vec![Folder {
        id: Some("folder-personal".into()),
        name: "Personal".into(),
        ..Default::default()
    }];
    state.cache.load(items, folders, vec![], vec![]);
    state
}

#[test]
fn loading_and_sorting_5000_items_is_fast() {
    let start = Instant::now();
    let state = big_state();
    let elapsed = start.elapsed();
    assert_eq!(state.cache.count(&Filter::All), 5000);
    assert!(elapsed.as_millis() < 600, "load took {elapsed:?}");
}

#[test]
fn list_view_builds_under_the_budget() {
    let state = big_state();
    let start = Instant::now();
    for _ in 0..10 {
        let _ = state.visible_rows();
    }
    let elapsed = start.elapsed() / 10;
    // Target 8 ms per frame, 3× allowance = 24 ms.
    assert!(elapsed.as_millis() < 24, "list view took {elapsed:?}");
}

#[test]
fn search_keystroke_under_the_budget() {
    let mut state = big_state();
    let start = Instant::now();
    for query in ["i", "it", "ite", "item", "item 12", "item 1234"] {
        state.search.input = query.to_owned();
        state.search.query = Some(lazybw::vault::query::Query::parse(query));
        let rows = state.visible_rows();
        assert!(!rows.is_empty(), "{query}");
    }
    let elapsed = start.elapsed() / 6;
    // Target 16 ms per keystroke, 3× allowance = 48 ms.
    assert!(elapsed.as_millis() < 48, "search took {elapsed:?}");
}

#[test]
fn memory_stays_reasonable() {
    // A coarse guard: 5,000 rows must not duplicate entire items.
    let state = big_state();
    let rows = state.cache.count(&Filter::All);
    assert_eq!(rows, 5000);
    let cache_size = std::mem::size_of::<VaultCache>();
    assert!(cache_size < 1024, "cache struct is {cache_size} bytes");
    // The row struct itself is small.
    assert!(std::mem::size_of::<lazybw::vault::cache::Row>() < 512);
    let _ = ItemType::Login;
}

fn assert_search_matches_full_scan(state: &AppState) {
    let query = state.search.query.as_ref().unwrap();
    let mut matcher = lazybw::vault::query::QueryMatcher::new(query);
    let hits = state
        .cache
        .rows_matching(&state.filter)
        .into_iter()
        .filter_map(|row| {
            let item = state.cache.item(&row.id).unwrap();
            let flags = state.health.report.as_ref().map(|r| r.flags(&row.id));
            matcher
                .matches(item, &row.folder_path, flags, state.config.vault.search_notes)
                .map(|result| (row, result))
        })
        .collect();
    let expected: Vec<_> =
        lazybw::vault::query::rank(hits).into_iter().map(|(r, _)| r.id.clone()).collect();
    let actual: Vec<_> = state.visible_rows().iter().map(|r| r.id.clone()).collect();
    assert_eq!(actual, expected, "query: {}", query.raw);
}

#[test]
fn incremental_search_preserves_hits_and_order_through_edits_and_syntax_changes() {
    use lazybw::bw::models::{Field, LoginUri};
    use lazybw::bw::sensitive::Sensitive;
    use lazybw::vault::query::Query;
    let mut state = big_state();
    for (index, name) in ["Água", "aBc", "GitHub", "Item 01234", "Item 01234", "İstanbul", "ＡＢＣ"]
        .into_iter()
        .enumerate()
    {
        let mut item = synthetic_item(index);
        item.name = name.into();
        item.notes = Some(Sensitive::new("unique notes value"));
        item.fields =
            Some(vec![Field { name: Some("Recovery codes".into()), ..Default::default() }]);
        item.login.as_mut().unwrap().uris = Some(vec![LoginUri::new("https://github.com/login")]);
        state.cache.upsert(item);
    }
    for text in [
        "i",
        "it",
        "ite",
        "item",
        "item 1",
        "item 12",
        "item 123",
        "item 1234",
        "item 123",
        "item",
        "",
        "a",
        "aB",
        "aBc",
        "abc",
        "!a",
        "!ab",
        "-a",
        "-ab",
        "a$",
        "a$b",
        "^a",
        "^ab",
        "'a",
        "'ab",
        "name:i",
        "name:it",
        "type:login i",
        "type:login it",
        "type:note ite",
        "ág",
        "água",
        "\"item 0\"",
        "\"item 01\"",
        "rec",
        "reco",
        "is:weak i",
        "is:weak it",
    ] {
        state.search.query = Some(Query::parse(text));
        assert_search_matches_full_scan(&state);
    }
}

#[test]
fn incremental_search_invalidates_candidates_when_view_inputs_change() {
    use lazybw::vault::query::Query;
    let mut state = big_state();
    state.search.query = Some(Query::parse("nothing"));
    assert_search_matches_full_scan(&state);
    assert!(state.visible_rows().is_empty());
    let mut item = synthetic_item(1);
    item.name = "nothingmore".into();
    state.cache.upsert(item);
    state.search.query = Some(Query::parse("nothingm"));
    assert_search_matches_full_scan(&state);
    assert_eq!(state.visible_rows().len(), 1);
    state.cache.remove("item-00001");
    state.search.query = Some(Query::parse("nothingmo"));
    assert_search_matches_full_scan(&state);
    assert!(state.visible_rows().is_empty());

    state.filter = Filter::Favorites;
    state.search.query = Some(Query::parse("item 12"));
    assert_search_matches_full_scan(&state);
    state.filter = Filter::All;
    state.search.query = Some(Query::parse("item 123"));
    assert_search_matches_full_scan(&state);
    state.config.vault.search_notes = true;
    state.search.query = Some(Query::parse("item 1234"));
    assert_search_matches_full_scan(&state);
    state.sort_reversed = true;
    assert_search_matches_full_scan(&state);
    let items: Vec<_> = state.cache.active_items().collect();
    state.health.report = Some(lazybw::vault::health::build(&items, &[], state.now, 365, false));
    state.health_epoch += 1;
    assert_search_matches_full_scan(&state);
    state.forget_vault();
    state.search.query = Some(Query::parse("item 12345"));
    assert_search_matches_full_scan(&state);
    assert!(state.visible_rows().is_empty());
}

#[test]
fn unicode_health_qualifiers_filter_search_and_quick_open() {
    use lazybw::app::Msg;
    use lazybw::app::update::update;
    use lazybw::bw::models::HealthEntry;
    use lazybw::bw::sensitive::Sensitive;
    use lazybw::vault::query::Query;
    let mut state = big_state();
    let items: Vec<_> = (0..2)
        .map(|n| {
            let mut item = synthetic_item(n);
            item.login.as_mut().unwrap().password = Some(Sensitive::new(format!("pw-{n}")));
            item
        })
        .collect();
    let entries: Vec<_> = (0..2)
        .map(|n| HealthEntry {
            id: format!("item-{n:05}"),
            password_strength: if n == 0 { 0 } else { 4 },
            ..Default::default()
        })
        .collect();
    state.health.report =
        Some(lazybw::vault::health::build(&items, &entries, state.now, 365, false));
    state.health_epoch += 1;
    state.cache.load(items, vec![], vec![], vec![]);
    // U+212A lowercases to ASCII k, as in QueryMatcher's keyword handling.
    for (text, expected) in [("is:weaK", "item-00000"), ("-is:weaK", "item-00001")] {
        state.search.query = Some(Query::parse(text));
        let ids: Vec<_> = state.visible_rows().iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, [expected], "search: {text}");
        for key in Msg::key("<Esc><C-p>") {
            update(&mut state, key);
        }
        update(&mut state, Msg::Paste(text.into()));
        assert_eq!(state.quick_open().unwrap().results, [expected], "quick open: {text}");
        for key in Msg::key("<Esc>") {
            update(&mut state, key);
        }
    }
}

#[test]
fn enabling_note_search_restores_candidates_during_a_text_extension() {
    use lazybw::bw::sensitive::Sensitive;
    use lazybw::vault::query::Query;
    let mut state = big_state();
    let mut item = synthetic_item(0);
    item.name = "Other".into();
    item.login = None;
    item.notes = Some(Sensitive::new("needleabc"));
    state.cache.load(vec![item], vec![], vec![], vec![]);
    state.config.vault.search_notes = false;
    state.search.query = Some(Query::parse("needle"));
    assert!(state.visible_rows().is_empty());
    state.config.vault.search_notes = true;
    state.search.query = Some(Query::parse("needlea"));
    assert_eq!(
        state.visible_rows().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["item-00000"]
    );
}

#[test]
fn a_health_report_change_restores_candidates_during_a_text_extension() {
    use lazybw::bw::models::HealthEntry;
    use lazybw::bw::sensitive::Sensitive;
    use lazybw::vault::query::Query;
    let mut state = big_state();
    let mut item = synthetic_item(0);
    item.name = "needleabc".into();
    item.login.as_mut().unwrap().password = Some(Sensitive::new("password"));
    state.cache.load(vec![item], vec![], vec![], vec![]);
    let items: Vec<_> = state.cache.active_items().collect();
    let mut entries =
        vec![HealthEntry { id: "item-00000".into(), password_strength: 4, ..Default::default() }];
    state.health.report =
        Some(lazybw::vault::health::build(&items, &entries, state.now, 365, false));
    state.health_epoch += 1;
    state.search.query = Some(Query::parse("is:weak n"));
    assert!(state.visible_rows().is_empty());
    entries[0].password_strength = 0;
    state.health.report =
        Some(lazybw::vault::health::build(&items, &entries, state.now, 365, false));
    state.health_epoch += 1;
    state.search.query = Some(Query::parse("is:weak ne"));
    assert_eq!(
        state.visible_rows().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["item-00000"]
    );
}
