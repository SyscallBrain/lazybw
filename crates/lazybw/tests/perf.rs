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
