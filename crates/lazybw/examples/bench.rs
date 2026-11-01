//! Benchmark harness for the hot paths of lazybw.
//!
//! Run with `cargo run --release --example bench`. Each case prints the
//! median, minimum and 95th percentile time per call over `SAMPLES` samples.
//! A sample batches calls until it lasts about `TARGET`, so cheap operations
//! are not lost in the timer's resolution.
//!
//! Options: `--json` prints one JSON object per case, `--filter <text>` runs
//! only the cases whose name contains `<text>`, and `--calls <n>` runs each
//! case exactly `n` times without timing it (for instruction counts under
//! `valgrind --tool=callgrind`, where the timing-based batching would vary).

use lazybw::app::Msg;
use lazybw::app::state::{AppState, Screen, SessionPhase};
use lazybw::app::update::update;
use lazybw::bw::client::ListResponse;
use lazybw::bw::envelope;
use lazybw::bw::models::{Folder, Item, Login, LoginUri};
use lazybw::bw::sensitive::Sensitive;
use lazybw::config::Paths;
use lazybw::theme::builtin;
use lazybw::vault::cache::{Filter, VaultCache, synthetic_item};
use lazybw::vault::query::Query;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::hint::black_box;
use std::time::{Duration, Instant};

const ITEMS: usize = 5000;
const SAMPLES: usize = 30;
const WARMUP_SAMPLES: usize = 3;
const TARGET: Duration = Duration::from_millis(5);
const NOW: i64 = 1_795_000_000;

struct Harness {
    json: bool,
    filter: Option<String>,
    calls: Option<usize>,
}

impl Harness {
    fn wants(&self, name: &str) -> bool {
        self.filter.as_deref().is_none_or(|filter| name.contains(filter))
    }

    /// Times `f`, batching calls so that one sample lasts about `TARGET`.
    fn case<F: FnMut()>(&self, name: &str, mut f: F) {
        if !self.wants(name) {
            return;
        }
        if let Some(calls) = self.calls {
            for _ in 0..calls {
                f();
            }
            return;
        }
        let start = Instant::now();
        f();
        let single = start.elapsed().as_nanos().max(1);
        let batch = (TARGET.as_nanos() / single).clamp(1, 1_000_000) as usize;
        for _ in 0..WARMUP_SAMPLES {
            for _ in 0..batch {
                f();
            }
        }
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let start = Instant::now();
            for _ in 0..batch {
                f();
            }
            samples.push(start.elapsed().as_nanos() as f64 / batch as f64);
        }
        self.report(name, samples, batch);
    }

    /// Times `body` on a fresh input from `setup`. The setup is not timed,
    /// and the output is dropped after the clock stops.
    fn case_with_setup<I, O>(
        &self,
        name: &str,
        mut setup: impl FnMut() -> I,
        mut body: impl FnMut(I) -> O,
    ) {
        if !self.wants(name) {
            return;
        }
        if let Some(calls) = self.calls {
            for _ in 0..calls {
                drop(black_box(body(setup())));
            }
            return;
        }
        for _ in 0..WARMUP_SAMPLES {
            drop(black_box(body(setup())));
        }
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let input = setup();
            let start = Instant::now();
            let output = body(input);
            samples.push(start.elapsed().as_nanos() as f64);
            drop(output);
        }
        self.report(name, samples, 1);
    }

    fn report(&self, name: &str, mut samples: Vec<f64>, batch: usize) {
        samples.sort_by(f64::total_cmp);
        let last = samples.len() - 1;
        let median = samples[last / 2];
        let min = samples[0];
        let p95 = samples[(last * 95) / 100];
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        if self.json {
            println!(
                "{{\"name\":\"{name}\",\"median_ns\":{median:.1},\"min_ns\":{min:.1},\"p95_ns\":{p95:.1},\"mean_ns\":{mean:.1},\"samples\":{},\"batch\":{batch}}}",
                samples.len()
            );
        } else {
            println!(
                "{name:<52} median {:>11}   min {:>11}   p95 {:>11}",
                fmt_ns(median),
                fmt_ns(min),
                fmt_ns(p95)
            );
        }
    }
}

fn fmt_ns(ns: f64) -> String {
    if ns >= 1e6 {
        format!("{:.3} ms", ns / 1e6)
    } else if ns >= 1e3 {
        format!("{:.2} µs", ns / 1e3)
    } else {
        format!("{ns:.1} ns")
    }
}

fn fixture_items() -> Vec<Item> {
    (0..ITEMS)
        .map(|index| {
            let mut item = synthetic_item(index);
            // Real logins have URIs and a password; search, health and the
            // detail pane all read them.
            if let Some(login) = &mut item.login {
                login.uris =
                    Some(vec![LoginUri::new(format!("https://site{index}.example.com/login"))]);
                login.password = Some(Sensitive::new(format!("pw-{}-{index}", index % 997)));
            }
            item
        })
        .collect()
}

fn folders() -> Vec<Folder> {
    vec![
        Folder {
            id: Some("folder-personal".into()),
            name: "Personal".into(),
            ..Default::default()
        },
        Folder {
            id: Some("folder-banking".into()),
            name: "Personal/Banking".into(),
            ..Default::default()
        },
    ]
}

/// The `bw serve` list response for `items`, as the CLI sends it.
fn list_body(items: &[Item]) -> Vec<u8> {
    let data: Vec<serde_json::Value> =
        items.iter().map(|item| serde_json::to_value(item).expect("serialize")).collect();
    serde_json::to_vec(&serde_json::json!({
        "success": true,
        "data": { "object": "list", "data": data }
    }))
    .expect("serialize")
}

fn state_with(items: Vec<Item>) -> AppState {
    let config = lazybw::config::Config::default();
    let mut theme = builtin::by_name("tokyonight-night").expect("theme");
    theme.resolve(true, false);
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).expect("keymap");
    let mut state = AppState::new(
        config,
        theme,
        keymap,
        Paths { config_dir: "/tmp/bench/config".into(), state_dir: "/tmp/bench/state".into() },
    );
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.screen = Screen::Vault;
    state.session = SessionPhase::Ready;
    state.size = (160, 45);
    state.cache.load(items, folders(), vec![], vec![]);
    state
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value_of =
        |flag: &str| args.iter().position(|arg| arg == flag).and_then(|i| args.get(i + 1));
    let harness = Harness {
        json: args.iter().any(|arg| arg == "--json"),
        filter: value_of("--filter").cloned(),
        calls: value_of("--calls").and_then(|value| value.parse().ok()),
    };
    let h = &harness;
    let body = list_body(&fixture_items());
    if !h.json && h.calls.is_none() {
        println!(
            "lazybw bench: {ITEMS} items, {} byte list response, {SAMPLES} samples per case\n",
            body.len()
        );
    }

    h.case_with_setup("cache.load (5000 items)", fixture_items, |items| {
        let mut cache = VaultCache::new();
        cache.load(items, folders(), vec![], vec![]);
        black_box(cache.count(&Filter::All));
        cache
    });

    h.case_with_setup("AppState + load 5000 items", fixture_items, state_with);

    h.case_with_setup(
        "sync: check + decode + load into cache (5000 items)",
        || (),
        |()| {
            black_box(envelope::check(200, body.as_slice()).is_ok());
            let list: ListResponse<Item> = envelope::decode(body.as_slice()).expect("decode");
            let mut cache = VaultCache::new();
            cache.load(list.data, folders(), vec![], vec![]);
            cache
        },
    );

    h.case("envelope::check (5000-item body)", || {
        black_box(envelope::check(200, black_box(body.as_slice())).is_ok());
    });

    h.case("envelope::decode (5000-item body)", || {
        let list: ListResponse<Item> =
            envelope::decode(black_box(body.as_slice())).expect("decode");
        black_box(list.data.len());
    });

    h.case("visible_rows (cached, no query)", {
        let state = state_with(fixture_items());
        move || {
            black_box(state.visible_rows().len());
        }
    });

    {
        let queries = ["i", "it", "ite", "item", "item 1", "item 12", "item 123", "item 1234"];
        let mut state = state_with(fixture_items());
        let mut index = 0usize;
        h.case("search keystroke (recompute only)", || {
            let query = queries[index % queries.len()];
            index += 1;
            state.search.input = query.to_owned();
            state.search.query = Some(Query::parse(query));
            black_box(state.visible_rows().len());
        });
    }

    {
        let queries = ["i", "it", "ite", "item", "item 1", "item 12", "item 123", "item 1234"];
        let mut state = state_with(fixture_items());
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
        let mut index = 0usize;
        h.case("search keystroke (recompute+draw)", || {
            let query = queries[index % queries.len()];
            index += 1;
            state.search.input = query.to_owned();
            state.search.query = Some(Query::parse(query));
            terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
        });
    }

    {
        let items = fixture_items();
        let now = chrono::DateTime::from_timestamp(NOW, 0).expect("timestamp");
        let mut state = state_with(items.clone());
        state.health.report = Some(lazybw::vault::health::build(&items, &[], now, 365, false));
        let queries = ["i", "it", "ite", "item", "item 1", "item 12", "item 123", "item 1234"];
        let mut index = 0usize;
        h.case("search keystroke with health report (recompute only)", || {
            let query = queries[index % queries.len()];
            index += 1;
            state.search.input = query.to_owned();
            state.search.query = Some(Query::parse(query));
            black_box(state.visible_rows().len());
        });
    }

    {
        let mut state = state_with(fixture_items());
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
        let script: Vec<_> = ["/", "i", "t", "e", "m", "<Space>", "1", "2", "3", "4", "<Esc>"]
            .into_iter()
            .flat_map(Msg::key)
            .filter_map(|msg| msg.key_event())
            .collect();
        assert_eq!(script.len(), 11, "every key of the script must parse");
        h.case("search typing 'item 1234' (update+draw, 11 keys)", || {
            for event in &script {
                update(&mut state, Msg::Key(*event));
                terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
            }
        });
    }

    {
        let mut state = state_with(fixture_items());
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
        let keys: Vec<_> =
            ["j", "k"].into_iter().flat_map(Msg::key).filter_map(|msg| msg.key_event()).collect();
        let mut step = 0usize;
        h.case("list navigation j/k (update+draw)", || {
            update(&mut state, Msg::Key(keys[step % 2]));
            step += 1;
            terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
        });
    }

    for (width, height) in [(160u16, 45u16), (120, 35)] {
        let mut state = state_with(fixture_items());
        state.size = (width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        h.case(&format!("frame draw {width}x{height} (vault)"), || {
            terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
        });
    }

    {
        let mut state = state_with(fixture_items());
        let mut item = state.cache.item("item-00000").cloned().expect("item");
        item.login = Some(Login {
            username: Some(Sensitive::new("user@example.com")),
            password: Some(Sensitive::new("correct-horse-battery-staple-42")),
            totp: Some(Sensitive::new("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")),
            uris: Some(vec![LoginUri::new("https://github.com/login")]),
            ..Default::default()
        });
        state.cache.upsert(item);
        state.list_row = 0;
        assert_eq!(state.selected_id().as_deref(), Some("item-00000"));
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
        h.case("frame draw 160x45 (login detail: password, totp, uri)", || {
            terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
        });
    }

    {
        let items = fixture_items();
        let now = chrono::DateTime::from_timestamp(NOW, 0).expect("timestamp");
        let mut state = state_with(items.clone());
        state.health.report = Some(lazybw::vault::health::build(&items, &[], now, 365, false));
        state.screen = Screen::Health;
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
        h.case("frame draw 160x45 (health screen, 5000 items)", || {
            terminal.draw(|frame| lazybw::ui::draw(frame, &state)).expect("draw");
        });
    }

    {
        let mut state = state_with(fixture_items());
        let mut index = 0usize;
        h.case("cache.upsert (one edit)", || {
            index += 1;
            let mut item = state.cache.item("item-00000").cloned().expect("item");
            item.name = format!("Renamed {index}");
            state.cache.upsert(item);
            black_box(state.cache.count(&Filter::All));
        });
    }

    {
        let items = fixture_items();
        let now = chrono::DateTime::from_timestamp(NOW, 0).expect("timestamp");
        h.case("health::build (5000 items, no bw report)", || {
            black_box(
                lazybw::vault::health::build(black_box(items.as_slice()), &[], now, 365, false)
                    .checked,
            );
        });
    }

    h.case("totp::current (one code)", || {
        black_box(
            lazybw::totp::current(black_box("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"), NOW as u64)
                .is_ok(),
        );
    });

    h.case("zxcvbn local_strength (16 chars)", || {
        black_box(lazybw::vault::health::local_strength(black_box("q7$Lp!x2vR#mZ9kA")));
    });

    h.case("parse_iso x10000", || {
        for _ in 0..10_000 {
            black_box(lazybw::util::time::parse_iso(black_box("2026-10-01T12:00:00.000Z")));
        }
    });

    {
        let state = state_with(fixture_items());
        h.case("sidebar entries()", || {
            let entries = lazybw::vault::filter::entries(
                &state.cache,
                &state.collapsed,
                state.config.vault.show_empty_types,
            );
            black_box(entries.len());
        });
    }
}
