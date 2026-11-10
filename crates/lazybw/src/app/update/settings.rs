//! The settings overlay, `:set` values and the keymap editor.

use super::{
    Action, AppState, BTreeSet, CaptureState, Density, Effect, IconMode, Key, Keymap, MessageLevel,
    Overlay, PickerEntry, PickerKind, PickerState, Screen, SettingKind, SettingRow, SortOrder,
    Source, apply, display_keys, parse_keys,
};

pub(crate) fn resolve_setting_key(key: &str) -> String {
    if key.contains('.') {
        return key.to_owned();
    }
    for section in ["ui", "security", "vault", "generator", "health", "editor"] {
        if crate::app::commands::setting_keys().contains(&format!("{section}.{key}")) {
            return format!("{section}.{key}");
        }
    }
    String::new()
}

pub(crate) fn setting_value(state: &AppState, dotted: &str) -> String {
    config_setting_value(&state.config, dotted)
}

pub(crate) fn config_setting_value(config: &crate::config::Config, dotted: &str) -> String {
    match dotted {
        "ui.theme" => config.ui.theme.clone(),
        "ui.transparent" => config.ui.transparent.to_string(),
        "ui.icons" => config.ui.icons.label().to_owned(),
        "ui.animations" => config.ui.animations.to_string(),
        "ui.list_density" => config.ui.list_density.label().to_owned(),
        "ui.statusline_separator" => config.ui.statusline_separator.label().to_owned(),
        "ui.show_hints" => config.ui.show_hints.to_string(),
        "ui.date_format" => {
            if config.ui.date_format.is_iso() {
                "iso".to_owned()
            } else {
                "relative".to_owned()
            }
        }
        "ui.mouse" => config.ui.mouse.to_string(),
        "security.clipboard_timeout" => config.security.clipboard_timeout.to_string(),
        "security.clear_clipboard_on_exit" => config.security.clear_clipboard_on_exit.to_string(),
        "security.auto_lock" => config.security.auto_lock.to_string(),
        "security.reveal_timeout" => config.security.reveal_timeout.to_string(),
        "security.hide_on_focus_lost" => config.security.hide_on_focus_lost.to_string(),
        "security.reprompt_grace" => config.security.reprompt_grace.to_string(),
        "security.totp" => config.security.totp.to_string(),
        "vault.sync_interval" => config.vault.sync_interval.to_string(),
        "vault.sort" => config.vault.sort.label().to_owned(),
        "vault.search_notes" => config.vault.search_notes.to_string(),
        "vault.show_empty_types" => config.vault.show_empty_types.to_string(),
        "vault.confirm_trash" => config.vault.confirm_trash.to_string(),
        "generator.type" => config.generator.kind.label().to_owned(),
        "generator.length" => config.generator.length.to_string(),
        "generator.words" => config.generator.words.to_string(),
        "generator.uppercase" => config.generator.uppercase.to_string(),
        "generator.lowercase" => config.generator.lowercase.to_string(),
        "generator.numbers" => config.generator.numbers.to_string(),
        "generator.special" => config.generator.special.to_string(),
        "generator.min_numbers" => config.generator.min_numbers.to_string(),
        "generator.min_special" => config.generator.min_special.to_string(),
        "generator.avoid_ambiguous" => config.generator.avoid_ambiguous.to_string(),
        "generator.separator" => config.generator.separator.clone(),
        "generator.capitalize" => config.generator.capitalize.to_string(),
        "generator.include_number" => config.generator.include_number.to_string(),
        "health.check_exposed" => config.health.check_exposed.to_string(),
        "health.old_password_age" => config.health.old_password_age.to_string(),
        "editor.command" => config.editor.command.clone(),
        _ => String::new(),
    }
}

pub(crate) fn default_config_value(dotted: &str) -> String {
    config_setting_value(&crate::config::Config::default(), dotted)
}

/// The settings that are on or off.
const BOOL_SETTINGS: &[&str] = &[
    "ui.transparent",
    "ui.animations",
    "ui.show_hints",
    "ui.mouse",
    "security.clear_clipboard_on_exit",
    "security.hide_on_focus_lost",
    "security.totp",
    "vault.search_notes",
    "vault.show_empty_types",
    "vault.confirm_trash",
    "generator.uppercase",
    "generator.lowercase",
    "generator.numbers",
    "generator.special",
    "generator.avoid_ambiguous",
    "generator.capitalize",
    "generator.include_number",
    "health.check_exposed",
];

/// The settings that are numbers, with their range.
const NUMBER_SETTINGS: &[(&str, u8, u8)] = &[
    ("generator.length", 5, 128),
    ("generator.words", 3, 20),
    ("generator.min_numbers", 0, 9),
    ("generator.min_special", 0, 9),
];

/// The settings with a fixed set of values.
const OPTION_SETTINGS: &[(&str, &[&str])] = &[
    ("ui.icons", &["nerdfont", "unicode", "ascii"]),
    ("ui.list_density", &["comfortable", "compact"]),
    ("ui.statusline_separator", &["arrow", "round", "slant", "none"]),
    ("ui.date_format", &["relative", "iso"]),
    ("generator.type", &["password", "passphrase"]),
];

/// Reads an on/off value the way Vim users type it.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Checks a value before anything changes, so a typo is reported instead
/// of written to config.toml, where it would stop the config from loading.
fn invalid_setting(dotted: &str, value: &str) -> Option<String> {
    if BOOL_SETTINGS.contains(&dotted) && parse_bool(value).is_none() {
        return Some(format!("E474: {dotted} is true or false, not “{value}”"));
    }
    if let Some((_, min, max)) = NUMBER_SETTINGS.iter().find(|(key, _, _)| *key == dotted)
        && !value.trim().parse::<u8>().is_ok_and(|n| (*min..=*max).contains(&n))
    {
        return Some(format!("E474: {dotted} is a number from {min} to {max}, not “{value}”"));
    }
    if let Some((_, options)) = OPTION_SETTINGS.iter().find(|(key, _)| *key == dotted)
        && !options.contains(&value.trim())
    {
        return Some(format!("E474: {dotted} is one of {}, not “{value}”", options.join(", ")));
    }
    None
}

pub(crate) fn apply_setting(state: &mut AppState, dotted: &str, value: &str) -> Vec<Effect> {
    if let Some(error) = invalid_setting(dotted, value) {
        state.message(MessageLevel::Error, error);
        return Vec::new();
    }
    let normalized = parse_bool(value).filter(|_| BOOL_SETTINGS.contains(&dotted));
    let normalized = normalized.map(|on| on.to_string());
    let value = normalized.as_deref().unwrap_or(value.trim());
    let parsed_bool = |v: &str| v == "true";
    let ok = match dotted {
        "ui.theme" => {
            let _ = apply(state, Action::with("theme.set", &[value]));
            // A theme that does not load leaves the old one, and the config.
            state.config.ui.theme == value
        }
        "ui.transparent" => {
            state.config.ui.transparent = parsed_bool(value);
            state.theme.transparent = state.config.ui.transparent;
            state.theme.resolve(crate::theme::truecolor_available(), crate::theme::no_color());
            true
        }
        "ui.icons" => {
            state.config.ui.icons = match value {
                "unicode" => IconMode::Unicode,
                "ascii" => IconMode::Ascii,
                _ => IconMode::Nerdfont,
            };
            true
        }
        "ui.animations" => {
            state.config.ui.animations = parsed_bool(value);
            state
                .fx
                .borrow_mut()
                .set_enabled(state.config.ui.animations && !crate::theme::no_color());
            true
        }
        "ui.list_density" => {
            state.config.ui.list_density =
                if value == "compact" { Density::Compact } else { Density::Comfortable };
            true
        }
        "ui.statusline_separator" => {
            state.config.ui.statusline_separator = match value {
                "round" => crate::config::SeparatorStyle::Round,
                "slant" => crate::config::SeparatorStyle::Slant,
                "none" => crate::config::SeparatorStyle::None,
                _ => crate::config::SeparatorStyle::Arrow,
            };
            true
        }
        "ui.show_hints" => {
            state.config.ui.show_hints = parsed_bool(value);
            true
        }
        "ui.date_format" => {
            state.config.ui.date_format = if value == "iso" {
                crate::config::DateFormat::Iso
            } else {
                crate::config::DateFormat::Relative
            };
            true
        }
        "ui.mouse" => {
            state.config.ui.mouse = parsed_bool(value);
            true
        }
        "security.clipboard_timeout" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.security.clipboard_timeout = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "security.clear_clipboard_on_exit" => {
            state.config.security.clear_clipboard_on_exit = parsed_bool(value);
            true
        }
        "security.auto_lock" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.security.auto_lock = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "security.reveal_timeout" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.security.reveal_timeout = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "security.hide_on_focus_lost" => {
            state.config.security.hide_on_focus_lost = parsed_bool(value);
            true
        }
        "security.reprompt_grace" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.security.reprompt_grace = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "security.totp" => {
            state.config.security.totp = parsed_bool(value);
            true
        }
        "vault.sync_interval" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.vault.sync_interval = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "vault.sort" => match SortOrder::parse(value) {
            Some(order) => {
                state.config.vault.sort = order;
                true
            }
            None => {
                state.message(MessageLevel::Error, format!("Unknown sort order: {value}"));
                false
            }
        },
        "vault.search_notes" => {
            state.config.vault.search_notes = parsed_bool(value);
            true
        }
        "vault.show_empty_types" => {
            state.config.vault.show_empty_types = parsed_bool(value);
            true
        }
        "vault.confirm_trash" => {
            state.config.vault.confirm_trash = parsed_bool(value);
            true
        }
        "generator.type" => {
            state.config.generator.kind = if value == "passphrase" {
                crate::config::GeneratorType::Passphrase
            } else {
                crate::config::GeneratorType::Password
            };
            true
        }
        "generator.length" => {
            let length = value.parse::<u8>().unwrap_or(state.config.generator.length);
            state.config.generator.length = length;
            state.generator.request.length = length;
            true
        }
        "generator.words" => {
            let words = value.parse::<u8>().unwrap_or(state.config.generator.words);
            state.config.generator.words = words;
            state.generator.request.words = words;
            true
        }
        "generator.uppercase"
        | "generator.lowercase"
        | "generator.numbers"
        | "generator.special"
        | "generator.avoid_ambiguous"
        | "generator.capitalize"
        | "generator.include_number" => {
            let on = parsed_bool(value);
            let generator = &mut state.config.generator;
            let request = &mut state.generator.request;
            match dotted {
                "generator.uppercase" => (generator.uppercase, request.uppercase) = (on, on),
                "generator.lowercase" => (generator.lowercase, request.lowercase) = (on, on),
                "generator.numbers" => (generator.numbers, request.numbers) = (on, on),
                "generator.special" => (generator.special, request.special) = (on, on),
                "generator.avoid_ambiguous" => {
                    (generator.avoid_ambiguous, request.avoid_ambiguous) = (on, on);
                }
                "generator.capitalize" => (generator.capitalize, request.capitalize) = (on, on),
                _ => (generator.include_number, request.include_number) = (on, on),
            }
            true
        }
        "generator.min_numbers" | "generator.min_special" => match value.parse::<u8>() {
            Ok(n) if n <= 9 => {
                if dotted == "generator.min_numbers" {
                    state.config.generator.min_numbers = n;
                    state.generator.request.min_numbers = n;
                } else {
                    state.config.generator.min_special = n;
                    state.generator.request.min_special = n;
                }
                true
            }
            _ => false,
        },
        "generator.separator" => {
            state.config.generator.separator = value.to_owned();
            state.generator.request.separator = value.to_owned();
            true
        }
        "health.check_exposed" => {
            state.config.health.check_exposed = parsed_bool(value);
            true
        }
        "health.old_password_age" => match crate::config::duration::DurationSpec::parse(value) {
            Ok(duration) => {
                state.config.health.old_password_age = duration;
                true
            }
            Err(err) => {
                state.message(MessageLevel::Error, err.to_string());
                false
            }
        },
        "editor.command" => {
            state.config.editor.command = value.to_owned();
            true
        }
        _ => false,
    };
    if !ok {
        return Vec::new();
    }
    // Save the value as the config now holds it (`on` becomes `true`, a
    // duration its canonical form), typed as config.toml expects it.
    let saved = config_setting_value(&state.config, dotted);
    let toml_value = if BOOL_SETTINGS.contains(&dotted) {
        toml_edit::Value::from(saved == "true")
    } else if NUMBER_SETTINGS.iter().any(|(key, _, _)| *key == dotted) {
        toml_edit::Value::from(saved.parse::<i64>().unwrap_or_default())
    } else {
        toml_edit::Value::from(saved)
    };
    let parts: Vec<String> = dotted.split('.').map(str::to_owned).collect();
    vec![Effect::SaveConfig(vec![(parts, toml_value)])]
}

pub(crate) fn settings_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    let rows = settings_rows(state);
    if rows.is_empty() {
        return Vec::new();
    }
    state.settings.index = state.settings.index.min(rows.len() - 1);
    match action.id.as_str() {
        "settings.activate" | "settings.next_value" | "settings.prev_value" => {
            let row = rows[state.settings.index].clone();
            let forward = action.id != "settings.prev_value";
            change_setting(state, &row, forward)
        }
        "settings.reset" => {
            let row = rows[state.settings.index].clone();
            let default = default_config_value(row.key);
            apply_setting(state, row.key, &default)
        }
        _ => Vec::new(),
    }
}

pub(crate) fn change_setting(state: &mut AppState, row: &SettingRow, forward: bool) -> Vec<Effect> {
    let value = match &row.kind {
        SettingKind::Bool => {
            if row.value == "true" {
                "false".to_owned()
            } else {
                "true".to_owned()
            }
        }
        SettingKind::Option(options) => {
            if options.is_empty() {
                return Vec::new();
            }
            let index = options.iter().position(|o| o == &row.value).unwrap_or(0);
            let next = if forward {
                (index + 1) % options.len()
            } else {
                (index + options.len() - 1) % options.len()
            };
            options[next].clone()
        }
        SettingKind::Number => {
            let current: i64 = row.value.parse().unwrap_or(0);
            (current + if forward { 1 } else { -1 }).to_string()
        }
        SettingKind::Action => {
            if row.key == "keybindings" {
                open_keymap_editor(state);
            } else if row.key == "ui.theme" {
                open_theme_picker(state);
            }
            return Vec::new();
        }
        SettingKind::Text => {
            // Typed in a prompt; the answer comes back as `settings.set`.
            state.open(crate::app::state::PromptState {
                title: row.label.to_owned(),
                input: crate::input::line::Line::new(&row.value),
                on_accept: Action::with("settings.set", &[row.key, ""]),
                replace_first_arg: false,
                error: None,
            });
            return Vec::new();
        }
        SettingKind::Duration => {
            let current = crate::config::duration::DurationSpec::parse(&row.value)
                .ok()
                .and_then(|d| d.seconds());
            if forward { next_duration_up(current) } else { next_duration_down(current) }
        }
    };
    apply_setting(state, row.key, &value)
}

/// Steps for `h`/`l` on duration settings. Below the first step comes
/// `never`; a zero duration is never offered because it would lock or sync on
/// every tick.
pub(crate) const DURATION_STEPS: [i64; 13] =
    [5, 10, 20, 30, 60, 300, 600, 900, 1800, 3600, 86_400, 604_800, 31_536_000];

pub(crate) fn format_duration_step(seconds: i64) -> String {
    crate::config::duration::DurationSpec::from(std::time::Duration::from_secs(seconds as u64))
        .to_string()
}

pub(crate) fn next_duration_up(current: Option<i64>) -> String {
    let next = match current {
        None => DURATION_STEPS[0],
        Some(seconds) => DURATION_STEPS
            .iter()
            .find(|s| **s > seconds)
            .copied()
            .unwrap_or(DURATION_STEPS[DURATION_STEPS.len() - 1]),
    };
    format_duration_step(next)
}

pub(crate) fn next_duration_down(current: Option<i64>) -> String {
    match current.and_then(|seconds| DURATION_STEPS.iter().rev().find(|s| **s < seconds)) {
        Some(previous) => format_duration_step(*previous),
        None => "never".to_owned(),
    }
}

/// The rows shown in the settings overlay.
#[must_use]
pub fn settings_rows(state: &AppState) -> Vec<SettingRow> {
    let c = &state.config;
    let mut rows = Vec::new();
    let mut push = |section: Option<&'static str>,
                    label: &'static str,
                    key: &'static str,
                    kind: SettingKind,
                    value: String| {
        rows.push(SettingRow { section, label, key, kind, value });
    };
    push(Some("APPEARANCE"), "Theme", "ui.theme", SettingKind::Action, c.ui.theme.clone());
    push(
        None,
        "Transparent background",
        "ui.transparent",
        SettingKind::Bool,
        c.ui.transparent.to_string(),
    );
    push(
        None,
        "Icons",
        "ui.icons",
        SettingKind::Option(vec!["nerdfont".into(), "unicode".into(), "ascii".into()]),
        c.ui.icons.label().to_owned(),
    );
    push(None, "Animations", "ui.animations", SettingKind::Bool, c.ui.animations.to_string());
    push(
        None,
        "List density",
        "ui.list_density",
        SettingKind::Option(vec!["comfortable".into(), "compact".into()]),
        c.ui.list_density.label().to_owned(),
    );
    push(
        None,
        "Statusline separators",
        "ui.statusline_separator",
        SettingKind::Option(vec!["arrow".into(), "round".into(), "slant".into(), "none".into()]),
        c.ui.statusline_separator.label().to_owned(),
    );
    push(None, "Key hints", "ui.show_hints", SettingKind::Bool, c.ui.show_hints.to_string());
    push(
        None,
        "Date format",
        "ui.date_format",
        SettingKind::Option(vec!["relative".into(), "iso".into()]),
        if c.ui.date_format.is_iso() { "iso".into() } else { "relative".into() },
    );
    push(None, "Mouse", "ui.mouse", SettingKind::Bool, c.ui.mouse.to_string());
    push(
        Some("SECURITY"),
        "Clear clipboard after",
        "security.clipboard_timeout",
        SettingKind::Duration,
        c.security.clipboard_timeout.to_string(),
    );
    push(
        None,
        "Lock after inactivity",
        "security.auto_lock",
        SettingKind::Duration,
        c.security.auto_lock.to_string(),
    );
    push(
        None,
        "Hide revealed fields after",
        "security.reveal_timeout",
        SettingKind::Duration,
        c.security.reveal_timeout.to_string(),
    );
    push(
        None,
        "Hide on focus lost",
        "security.hide_on_focus_lost",
        SettingKind::Bool,
        c.security.hide_on_focus_lost.to_string(),
    );
    push(None, "Show TOTP codes", "security.totp", SettingKind::Bool, c.security.totp.to_string());
    push(
        None,
        "Clear clipboard on exit",
        "security.clear_clipboard_on_exit",
        SettingKind::Bool,
        c.security.clear_clipboard_on_exit.to_string(),
    );
    push(
        None,
        "Re-prompt stays valid for",
        "security.reprompt_grace",
        SettingKind::Duration,
        c.security.reprompt_grace.to_string(),
    );
    push(
        Some("VAULT"),
        "Sync interval",
        "vault.sync_interval",
        SettingKind::Duration,
        c.vault.sync_interval.to_string(),
    );
    push(
        None,
        "Sort",
        "vault.sort",
        SettingKind::Option(vec![
            "name".into(),
            "modified".into(),
            "created".into(),
            "type".into(),
        ]),
        c.vault.sort.label().to_owned(),
    );
    push(
        None,
        "Search notes",
        "vault.search_notes",
        SettingKind::Bool,
        c.vault.search_notes.to_string(),
    );
    push(
        None,
        "Show empty types",
        "vault.show_empty_types",
        SettingKind::Bool,
        c.vault.show_empty_types.to_string(),
    );
    push(
        None,
        "Confirm trash",
        "vault.confirm_trash",
        SettingKind::Bool,
        c.vault.confirm_trash.to_string(),
    );
    push(
        Some("GENERATOR"),
        "Type",
        "generator.type",
        SettingKind::Option(vec!["password".into(), "passphrase".into()]),
        c.generator.kind.label().to_owned(),
    );
    push(None, "Length", "generator.length", SettingKind::Number, c.generator.length.to_string());
    push(None, "Words", "generator.words", SettingKind::Number, c.generator.words.to_string());
    for (label, key, value) in [
        ("Uppercase (A-Z)", "generator.uppercase", c.generator.uppercase),
        ("Lowercase (a-z)", "generator.lowercase", c.generator.lowercase),
        ("Numbers (0-9)", "generator.numbers", c.generator.numbers),
        ("Special (!@#$%^&*)", "generator.special", c.generator.special),
        ("Avoid ambiguous", "generator.avoid_ambiguous", c.generator.avoid_ambiguous),
        ("Capitalize words", "generator.capitalize", c.generator.capitalize),
        ("Include a number", "generator.include_number", c.generator.include_number),
    ] {
        push(None, label, key, SettingKind::Bool, value.to_string());
    }
    push(
        None,
        "Minimum numbers",
        "generator.min_numbers",
        SettingKind::Number,
        c.generator.min_numbers.to_string(),
    );
    push(
        None,
        "Minimum special",
        "generator.min_special",
        SettingKind::Number,
        c.generator.min_special.to_string(),
    );
    push(
        None,
        "Word separator",
        "generator.separator",
        SettingKind::Option(vec!["-".into(), " ".into(), ".".into(), "_".into(), ",".into()]),
        c.generator.separator.clone(),
    );
    push(
        Some("HEALTH"),
        "Check exposed",
        "health.check_exposed",
        SettingKind::Bool,
        c.health.check_exposed.to_string(),
    );
    push(
        None,
        "Old password age",
        "health.old_password_age",
        SettingKind::Duration,
        c.health.old_password_age.to_string(),
    );
    push(
        Some("EDITOR"),
        "Editor command",
        "editor.command",
        SettingKind::Text,
        c.editor.command.clone(),
    );
    push(
        Some("KEYBINDINGS"),
        "Keybindings",
        "keybindings",
        SettingKind::Action,
        "Edit keybindings…".to_owned(),
    );
    rows
}

pub(crate) fn open_keymap_editor(state: &mut AppState) {
    state.screen = Screen::KeymapEditor;
    state.keymap_editor = crate::app::state::KeymapEditorState {
        context_index: 1,
        filter: String::new(),
        index: 0,
        offset: 0,
        capture: None,
        contexts: std::iter::once("All".to_owned())
            .chain(crate::config::keymap::KNOWN_CONTEXTS.iter().map(|c| (*c).to_owned()))
            .collect(),
        dirty: false,
    };
}

/// The rows of the keymap editor for the selected context and filter.
#[must_use]
pub fn keymap_rows(state: &AppState) -> Vec<(String, String, String, String)> {
    let editor = &state.keymap_editor;
    let context =
        editor.contexts.get(editor.context_index).cloned().unwrap_or_else(|| "All".to_owned());
    let mut rows = Vec::new();
    let contexts: Vec<String> = if context == "All" {
        crate::config::keymap::KNOWN_CONTEXTS.iter().map(|c| (*c).to_owned()).collect()
    } else {
        vec![context]
    };
    for context in contexts {
        for (keys, call) in state.keymap.bindings_in(&context) {
            let description = crate::config::keymap::action_description(&call);
            let actions = call.as_text();
            let keytext = display_keys(&keys);
            let filter = editor.filter.to_lowercase();
            if !filter.is_empty()
                && !description.to_lowercase().contains(&filter)
                && !actions.to_lowercase().contains(&filter)
                && !keytext.to_lowercase().contains(&filter)
            {
                continue;
            }
            rows.push((description, actions, keytext, context.clone()));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

pub(crate) fn keymap_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    let rows = keymap_rows(state);
    match action.id.as_str() {
        "keymap.next_context" => {
            state.keymap_editor.context_index =
                (state.keymap_editor.context_index + 1) % state.keymap_editor.contexts.len();
            state.keymap_editor.index = 0;
        }
        "keymap.prev_context" => {
            let len = state.keymap_editor.contexts.len();
            state.keymap_editor.context_index = (state.keymap_editor.context_index + len - 1) % len;
            state.keymap_editor.index = 0;
        }
        "keymap.rebind" | "keymap.add" => {
            if let Some((description, action_text, keys, context)) =
                rows.get(state.keymap_editor.index)
            {
                state.keymap_editor.capture = Some(CaptureState {
                    action: action_text.clone(),
                    context: context.clone(),
                    keys: if action.id == "keymap.rebind" {
                        keys.split_whitespace().map(str::to_owned).collect()
                    } else {
                        Vec::new()
                    },
                    conflict: None,
                    adding: action.id == "keymap.add",
                });
                state.message(MessageLevel::Info, format!("Rebind “{description}” ({context})."));
            }
        }
        "keymap.remove" => {
            if let Some((_, action_text, keys, context)) = rows.get(state.keymap_editor.index) {
                let first = keys.split_whitespace().next().unwrap_or("");
                if let Ok(parsed) = parse_keys(first) {
                    state.keymap.unbind(context, &parsed);
                    state.keymap_editor.dirty = true;
                    state.message(MessageLevel::Ok, format!("Removed {first} from {action_text}."));
                }
            }
        }
        "keymap.reset" => {
            if let Some((_, action_text, _, context)) = rows.get(state.keymap_editor.index) {
                reset_action_bindings(state, context, action_text);
                state.keymap_editor.dirty = true;
            }
        }
        "keymap.reset_all" => {
            let context = state
                .keymap_editor
                .contexts
                .get(state.keymap_editor.context_index)
                .cloned()
                .unwrap_or_default();
            if let Some(trie) = state.keymap.contexts.get_mut(&context) {
                *trie = Default::default();
            }
            state.keymap.flat.remove(&context);
            if let Ok((default, _)) = Keymap::build(&state.config.keys) {
                for (keys, call) in default.bindings_in(&context) {
                    state.keymap.bind(&context, &keys, call);
                }
            }
            state.keymap_editor.dirty = true;
        }
        "keymap.save" => {
            let updates = keymap_diff(state);
            state.keymap_editor.dirty = false;
            state.message(MessageLevel::Ok, "Keybindings saved.");
            return vec![Effect::SaveConfig(updates)];
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn reset_action_bindings(state: &mut AppState, context: &str, action_text: &str) {
    if let Ok((default, _)) = Keymap::build(&state.config.keys) {
        for (keys, call) in default.bindings_in(context) {
            if call.as_text() == action_text {
                state.keymap.bind(context, &keys, call);
            }
        }
    }
}

pub(crate) fn capture_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let Some(capture) = &mut state.keymap_editor.capture else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.keymap_editor.capture = None;
            return Vec::new();
        }
        Code::Enter => {
            let keys: Vec<Key> =
                capture.keys.iter().filter_map(|k| parse_keys(k).ok()).flatten().collect();
            if keys.is_empty() {
                state.keymap_editor.capture = None;
                return Vec::new();
            }
            let context = capture.context.clone();
            let action = capture.action.clone();
            let adding = capture.adding;
            state.keymap_editor.capture = None;
            if !adding {
                // Remove the old keys for this action in the context.
                let old: Vec<Vec<Key>> = state
                    .keymap
                    .bindings_in(&context)
                    .into_iter()
                    .filter(|(_, call)| call.as_text() == action)
                    .map(|(keys, _)| keys)
                    .collect();
                for keys in old {
                    state.keymap.unbind(&context, &keys);
                }
            }
            let call = crate::config::keymap::ActionCall::parse(&action, Source::Session);
            state.keymap.bind(&context, &keys, call);
            state.keymap_editor.dirty = true;
            state.message(MessageLevel::Ok, format!("Bound {} in {context}.", display_keys(&keys)));
            return Vec::new();
        }
        Code::Backspace => {
            capture.keys.pop();
            return Vec::new();
        }
        _ => {}
    }
    capture.keys.push(key.notation());
    Vec::new()
}

pub(crate) fn config_notation(keys: &[Key]) -> String {
    keys.iter().map(|key| key.notation()).collect()
}

pub(crate) fn keymap_diff(state: &AppState) -> Vec<(Vec<String>, toml_edit::Value)> {
    let Ok((default, _)) = Keymap::build(&state.config.keys) else { return Vec::new() };
    let mut updates = Vec::new();
    let mut contexts: BTreeSet<String> = state.keymap.flat.keys().cloned().collect();
    contexts.extend(default.flat.keys().cloned());
    for context in contexts {
        let effective = state.keymap.bindings_in(&context);
        let base = default.bindings_in(&context);
        for (keys, call) in &effective {
            let same = base.iter().any(|(k, c)| k == keys && c.as_text() == call.as_text());
            if !same {
                let mut path = vec!["keys".to_owned(), context.clone(), config_notation(keys)];
                path.retain(|p| !p.is_empty());
                updates.push((path, toml_edit::Value::from(call.as_text())));
            }
        }
        for (keys, call) in &base {
            let still = effective.iter().any(|(k, c)| k == keys && c.as_text() == call.as_text());
            if !still {
                let mut path = vec!["keys".to_owned(), context.clone(), config_notation(keys)];
                path.retain(|p| !p.is_empty());
                updates.push((path, toml_edit::Value::from("noop")));
            }
        }
    }
    updates
}

/// Opens the theme picker: built-in themes, then the user's, with the
/// current one marked. Moving through it previews each theme live.
pub(crate) fn open_theme_picker(state: &mut AppState) {
    let current = state.config.ui.theme.clone();
    let note = |appearance: &str, name: &str, yours: bool| {
        let mut note = appearance.to_owned();
        if yours {
            note.push_str(" · yours");
        }
        if name == current {
            note.push_str(" · current");
        }
        note
    };
    let mut entries: Vec<PickerEntry> = crate::theme::builtin::all()
        .into_iter()
        .map(|theme| PickerEntry {
            note: note(theme.appearance.label(), &theme.name, false),
            label: theme.name.clone(),
            filter: theme.name.clone(),
            value: theme.name,
            accelerator: None,
        })
        .collect();
    let (user, _) = crate::theme::loader::load_user_themes(&state.paths.themes_dir());
    for theme in user {
        if entries.iter().any(|e| e.value == theme.name) {
            continue;
        }
        entries.push(PickerEntry {
            note: note(theme.appearance.label(), &theme.name, true),
            label: theme.name.clone(),
            filter: theme.name.clone(),
            value: theme.name,
            accelerator: None,
        });
    }
    let mut picker = PickerState::new("Theme", PickerKind::Theme, entries);
    if let Some(position) = picker.entries.iter().position(|e| e.value == current) {
        picker.index = position;
    }
    state.open(picker);
    state.theme_preview_from = Some(current);
}

/// Applies a theme for display only (the config keeps its value).
pub(crate) fn preview_theme(state: &mut AppState, name: &str) {
    if state.theme.name == name {
        return;
    }
    let outcome = crate::theme::loader::load(name, &state.paths.themes_dir());
    if let Some(mut theme) = outcome.theme {
        theme.transparent = state.config.ui.transparent;
        theme.resolve(crate::theme::truecolor_available(), crate::theme::no_color());
        state.theme = theme;
    }
}

/// Live theme preview for the theme picker and `:theme <Tab>`: shows the
/// highlighted theme, and puts the previous one back when the picker or the
/// command line closes without choosing.
pub(crate) fn update_theme_preview(state: &mut AppState) {
    let Some(from) = state.theme_preview_from.clone() else {
        if let Some(cmdline) = state.cmdline()
            && cmdline.show_completions
            && cmdline.input.trim_start().starts_with("theme ")
        {
            state.theme_preview_from = Some(state.config.ui.theme.clone());
            return update_theme_preview(state);
        }
        return;
    };
    let highlighted = match &state.overlay {
        Some(Overlay::Picker(picker)) if picker.kind == PickerKind::Theme => {
            picker.selected().map(|e| e.value.clone())
        }
        Some(Overlay::Cmdline(cmdline)) if cmdline.input.trim_start().starts_with("theme ") => {
            if cmdline.show_completions {
                cmdline.completions.get(cmdline.completion_index).cloned()
            } else {
                Some(from.clone())
            }
        }
        _ => None,
    };
    match highlighted {
        Some(name) => preview_theme(state, &name),
        None => {
            // Closed: keep a chosen theme, otherwise restore the old one.
            state.theme_preview_from = None;
            let chosen = state.config.ui.theme.clone();
            if state.theme.name != chosen {
                preview_theme(state, &chosen);
            }
            let _ = from;
        }
    }
}
