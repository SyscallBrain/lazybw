//! Generates the README screenshots as SVGs from the real renderer.
//!
//! Run with `cargo run -p lazybw --example screenshots`; the output lives in
//! `assets/screenshots/` (and a copy in the book). Deterministic: fixed
//! clock, animations off, Unicode icons so the images need no Nerd Font.
//! The vault is `examples/demo-vault.json`: invented items only.

use lazybw::app::msg::Msg;
use lazybw::app::state::{AppState, DetailState, FormKey, Screen, SessionPhase};
use lazybw::bw::models::{Folder, HealthEntry, Item};
use lazybw::config::{Config, Paths};
use lazybw::theme::builtin;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use std::fmt::Write as _;
use std::path::Path;

/// Sets up the state for one screenshot.
type Scene = fn(&mut AppState);

/// Every screenshot: file name, terminal size and how to set the scene.
const SHOTS: &[(&str, u16, u16, Scene)] = &[
    ("vault.svg", 140, 40, vault),
    ("editor.svg", 150, 38, editor),
    ("quick-open.svg", 140, 36, quick_open),
    ("which-key.svg", 140, 36, which_key),
    ("generator.svg", 140, 36, generator),
    ("health.svg", 140, 36, health),
    ("settings.svg", 140, 36, settings),
    ("light-theme.svg", 140, 36, light_theme),
];

fn main() {
    // Relative to the crate, whatever the current directory is.
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = crate_dir.join("assets/screenshots");
    let book = crate_dir.join("../../book/src/images/screenshots");
    std::fs::create_dir_all(&dir).expect("create screenshots dir");
    std::fs::create_dir_all(&book).expect("create book screenshots dir");
    for (name, width, height, scene) in SHOTS {
        let mut state = base_state();
        state.size = (*width, *height);
        scene(&mut state);
        render_svg(&state, *width, *height, &dir.join(name));
        // The book publishes only files under book/src, so it gets a copy.
        std::fs::copy(dir.join(name), book.join(name)).expect("copy screenshot to the book");
    }
    println!("wrote {} screenshots to {} and the book", SHOTS.len(), dir.display());
}

fn demo() -> serde_json::Value {
    serde_json::from_str(include_str!("demo-vault.json")).expect("demo vault")
}

fn base_state() -> AppState {
    let config = Config::default();
    let mut theme = builtin::by_name("tokyonight-night").expect("theme");
    theme.resolve(true, false);
    let (keymap, _) = lazybw::config::keymap::Keymap::build(&config.keys).expect("keymap");
    let mut state = AppState::new(
        config,
        theme,
        keymap,
        Paths { config_dir: "/tmp/lazybw/config".into(), state_dir: "/tmp/lazybw/state".into() },
    );
    state.clipboard = lazybw::clipboard::Clipboard::memory();
    state.config.ui.animations = false;
    state.config.ui.icons = lazybw::config::IconMode::Unicode;
    state.fx.borrow_mut().set_enabled(false);
    state.now = lazybw::util::time::test_now();
    state.screen = Screen::Vault;
    state.session = SessionPhase::Ready;
    state.email = Some("jane@example.com".to_owned());
    state.server = Some("vault.bitwarden.eu".to_owned());
    state.last_sync = Some(state.now - chrono::Duration::minutes(2));
    let demo = demo();
    let items: Vec<Item> = serde_json::from_value(demo["items"].clone()).expect("items");
    let folders: Vec<Folder> = serde_json::from_value(demo["folders"].clone()).expect("folders");
    state.cache.load(items, folders, vec![], vec![]);
    run_health(&mut state);
    select(&mut state, "GitHub");
    state
}

/// Sends keys through the real update function.
fn keys(state: &mut AppState, notation: &str) {
    for msg in Msg::key(notation) {
        let _ = lazybw::app::update::update(state, msg);
    }
}

/// Puts the cursor on an item and the focus on the detail pane.
fn select(state: &mut AppState, name: &str) {
    if let Some(index) = state.visible_rows().iter().position(|row| row.name == name) {
        state.list_row = index;
    }
    state.focus = lazybw::app::state::Focus::Detail;
    state.detail = DetailState::default();
}

/// A health report from the demo entries, as `bw report password-health`
/// would return them.
fn run_health(state: &mut AppState) {
    let entries: Vec<HealthEntry> =
        serde_json::from_value(demo()["health"].clone()).expect("health entries");
    let items: Vec<Item> = state.cache.active_items().cloned().collect();
    let report = lazybw::vault::health::build(&items, &entries, state.now, 365, false);
    state.cache.apply_health(&report);
    state.health.raw = entries;
    state.health.report = Some(report);
    state.health.exposure_available = true;
}

fn vault(_: &mut AppState) {}

fn editor(state: &mut AppState) {
    // Open the login in the editor the way a user would.
    keys(state, "e");
    assert_eq!(state.screen, Screen::Editor, "the editor opened");
    if let Some(editor) = &mut state.editor {
        if let Some(index) = editor.rows.iter().position(|row| row.key == FormKey::Password) {
            editor.row = index;
        }
        editor.dirty = true;
    }
}

fn quick_open(state: &mut AppState) {
    keys(state, "<C-p>mail");
}

fn which_key(state: &mut AppState) {
    // Press the leader and show the panel the way the delay would, without
    // a clock tick (which would also start timers and a sync).
    keys(state, "<Space>");
    let chain = state.contexts();
    state.which_key = Some(lazybw::app::state::WhichKeyState {
        prefix: state.dispatcher.pending_display(&state.keymap),
        continuations: state.dispatcher.continuations(&state.keymap, &chain),
        since: std::time::Instant::now(),
    });
}

fn generator(state: &mut AppState) {
    state.screen = Screen::Generator;
    state.tab = lazybw::app::state::Tab::Generator;
    state.generator.output = "Rv8%qTn!Ze3wLx&p6MuJ#c2y".to_owned();
    let now = std::time::Instant::now();
    for (value, minutes) in [
        ("Rv8%qTn!Ze3wLx&p6MuJ#c2y", 0),
        ("anchor-marble-crisp-velvet-7", 2),
        ("k7#Qm!vR2x@Lp9$Wn4eT", 6),
        ("orbit-tundra-plaza-quiver", 9),
    ] {
        let when = now.checked_sub(std::time::Duration::from_secs(minutes * 60)).unwrap_or(now);
        state.generator.history.push((value.to_owned(), when));
    }
}

fn health(state: &mut AppState) {
    state.screen = Screen::Health;
    state.tab = lazybw::app::state::Tab::Health;
}

fn settings(state: &mut AppState) {
    keys(state, "<Space>,");
}

fn light_theme(state: &mut AppState) {
    let mut theme = builtin::by_name("tokyonight-day").expect("theme");
    theme.resolve(true, false);
    state.theme = theme;
    state.config.ui.theme = "tokyonight-day".to_owned();
    select(state, "Proton Mail");
}

fn render_svg(state: &AppState, width: u16, height: u16, path: &Path) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| lazybw::ui::draw(frame, state)).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let cell_w = 8.4;
    let cell_h = 16.0;
    let base_bg = css(state.theme.colour("bg"));
    let base_fg = css(state.theme.colour("fg"));
    let colour = |c: Option<Color>, fallback: &str| match c {
        None | Some(Color::Reset) => fallback.to_owned(),
        Some(c) => css(c),
    };
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" font-family=\"JetBrains Mono, DejaVu Sans Mono, Menlo, \
         Consolas, monospace\" font-size=\"14\">",
        w = f64::from(width) * cell_w,
        h = f64::from(height) * cell_h,
    );
    let _ = writeln!(svg, "<rect width=\"100%\" height=\"100%\" rx=\"8\" fill=\"{base_bg}\"/>");
    // Backgrounds: one rectangle per run of cells sharing a colour.
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let bg = colour(buffer[(x, y)].style().bg, &base_bg);
            let start = x;
            while x < width && colour(buffer[(x, y)].style().bg, &base_bg) == bg {
                x += 1;
            }
            if bg != base_bg {
                let _ = writeln!(
                    svg,
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{cell_h}\" fill=\"{bg}\"/>",
                    f64::from(start) * cell_w,
                    f64::from(y) * cell_h,
                    f64::from(x - start) * cell_w,
                );
            }
        }
    }
    // Text: runs of one style, every character pinned to its column, so a
    // font with other glyph widths cannot shift the rest of the line.
    for y in 0..height {
        let baseline = f64::from(y) * cell_h + 12.0;
        let mut x = 0;
        while x < width {
            let style = buffer[(x, y)].style();
            let key = (colour(style.fg, &base_fg), style.add_modifier);
            let mut xs = Vec::new();
            let mut text = String::new();
            while x < width {
                let cell = &buffer[(x, y)];
                let style = cell.style();
                if (colour(style.fg, &base_fg), style.add_modifier) != key {
                    break;
                }
                let symbol = cell.symbol();
                if !symbol.is_empty() && symbol != " " {
                    xs.push(format!("{:.1}", f64::from(x) * cell_w));
                    for ch in symbol.chars().take(1) {
                        match ch {
                            '<' => text.push_str("&lt;"),
                            '>' => text.push_str("&gt;"),
                            '&' => text.push_str("&amp;"),
                            other => text.push(other),
                        }
                    }
                }
                x += 1;
            }
            if xs.is_empty() {
                continue;
            }
            let modifiers = key.1;
            let mut attrs = String::new();
            if modifiers.contains(Modifier::BOLD) {
                attrs.push_str(" font-weight=\"bold\"");
            }
            if modifiers.contains(Modifier::ITALIC) {
                attrs.push_str(" font-style=\"italic\"");
            }
            if modifiers.contains(Modifier::UNDERLINED) {
                attrs.push_str(" text-decoration=\"underline\"");
            }
            let _ = writeln!(
                svg,
                "<text x=\"{}\" y=\"{baseline:.1}\" fill=\"{}\"{attrs}>{text}</text>",
                xs.join(" "),
                key.0
            );
        }
    }
    svg.push_str("</svg>\n");
    std::fs::write(path, svg).expect("write svg");
}

fn css(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(index) => xterm(index),
        Color::Black => "#000000".to_owned(),
        Color::White => "#ffffff".to_owned(),
        Color::Red => "#ff0000".to_owned(),
        Color::Green => "#00ff00".to_owned(),
        Color::Blue => "#0000ff".to_owned(),
        Color::Yellow => "#ffff00".to_owned(),
        Color::Magenta => "#ff00ff".to_owned(),
        Color::Cyan => "#00ffff".to_owned(),
        Color::Gray => "#808080".to_owned(),
        Color::DarkGray => "#404040".to_owned(),
        Color::LightRed => "#ff8080".to_owned(),
        Color::LightGreen => "#80ff80".to_owned(),
        Color::LightBlue => "#8080ff".to_owned(),
        Color::LightYellow => "#ffff80".to_owned(),
        Color::LightMagenta => "#ff80ff".to_owned(),
        Color::LightCyan => "#80ffff".to_owned(),
        Color::Reset => "#000000".to_owned(),
    }
}

fn xterm(index: u8) -> String {
    let index = u32::from(index);
    let (r, g, b) = if index < 16 {
        [
            (0, 0, 0),
            (128, 0, 0),
            (0, 128, 0),
            (128, 128, 0),
            (0, 0, 128),
            (128, 0, 128),
            (0, 128, 128),
            (192, 192, 192),
            (128, 128, 128),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (0, 0, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ][index as usize]
    } else if index < 232 {
        let index = index - 16;
        let steps = [0, 95, 135, 175, 215, 255];
        (
            steps[(index / 36) as usize],
            steps[((index % 36) / 6) as usize],
            steps[(index % 6) as usize],
        )
    } else {
        let level = 8 + (index - 232) * 10;
        (level as u8, level as u8, level as u8)
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}
