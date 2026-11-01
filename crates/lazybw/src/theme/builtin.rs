//! Built-in themes, embedded from `assets/themes/*.toml`.
//!
//! The files use the same format as user themes so `extends` works uniformly
//! and the docs stay honest.

use crate::theme::loader::parse_theme;
use crate::theme::{Colour, Palette, Theme};

const TOKYONIGHT_NIGHT: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/tokyonight-night.toml"));
const TOKYONIGHT_STORM: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/tokyonight-storm.toml"));
const TOKYONIGHT_MOON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/tokyonight-moon.toml"));
const TOKYONIGHT_DAY: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/tokyonight-day.toml"));
const CATPPUCCIN_MOCHA: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/catppuccin-mocha.toml"));
const GRUVBOX_DARK: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/gruvbox-dark.toml"));
const NORD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/nord.toml"));
const ROSE_PINE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/rose-pine.toml"));
const KANAGAWA_WAVE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes/kanagawa-wave.toml"));

/// The embedded theme sources, in picker order.
pub const SOURCES: &[(&str, &str)] = &[
    ("tokyonight-night", TOKYONIGHT_NIGHT),
    ("tokyonight-storm", TOKYONIGHT_STORM),
    ("tokyonight-moon", TOKYONIGHT_MOON),
    ("tokyonight-day", TOKYONIGHT_DAY),
    ("catppuccin-mocha", CATPPUCCIN_MOCHA),
    ("gruvbox-dark", GRUVBOX_DARK),
    ("nord", NORD),
    ("rose-pine", ROSE_PINE),
    ("kanagawa-wave", KANAGAWA_WAVE),
];

/// The name of the default theme.
pub const DEFAULT: &str = "tokyonight-night";

/// Builds every built-in theme, in picker order.
#[must_use]
pub fn all() -> Vec<Theme> {
    let mut themes: Vec<Theme> =
        SOURCES.iter().map(|(name, source)| parse_theme(name, source)).collect();
    themes.push(terminal());
    themes
}

/// Looks up a built-in theme by name (including `terminal`).
#[must_use]
pub fn by_name(name: &str) -> Option<Theme> {
    if name == "terminal" {
        return Some(terminal());
    }
    SOURCES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, source)| parse_theme(name, source))
}

/// The `terminal` theme: palette entries map to ANSI colours so the user's
/// terminal palette applies.
#[must_use]
pub fn terminal() -> Theme {
    let mut palette = Palette::new();
    for (key, colour) in [
        ("bg", Colour::Reset),
        ("bg_dark", Colour::Reset),
        ("bg_highlight", Colour::Ansi(0)),
        ("bg_visual", Colour::Ansi(8)),
        ("border", Colour::Ansi(8)),
        ("fg", Colour::Reset),
        ("fg_dark", Colour::Ansi(7)),
        ("comment", Colour::Ansi(8)),
        ("dark5", Colour::Ansi(7)),
        ("accent", Colour::Ansi(4)),
        ("blue", Colour::Ansi(4)),
        ("cyan", Colour::Ansi(6)),
        ("magenta", Colour::Ansi(5)),
        ("purple", Colour::Ansi(5)),
        ("orange", Colour::Ansi(9)),
        ("yellow", Colour::Ansi(3)),
        ("green", Colour::Ansi(2)),
        ("teal", Colour::Ansi(14)),
        ("red", Colour::Ansi(1)),
        ("shadow", Colour::Ansi(0)),
    ] {
        palette.set(key, colour);
    }
    Theme::from_palette("terminal", crate::theme::Appearance::Dark, palette)
}

/// Names of every built-in theme.
#[must_use]
pub fn names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = SOURCES.iter().map(|(name, _)| *name).collect();
    names.push("terminal");
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sources_parse() {
        for (name, source) in SOURCES {
            let theme = parse_theme(name, source);
            assert_eq!(theme.name, *name);
            let expected = if *name == "tokyonight-day" {
                crate::theme::Appearance::Light
            } else {
                crate::theme::Appearance::Dark
            };
            assert_eq!(theme.appearance, expected, "{name}");
        }
        assert_eq!(by_name("tokyonight-day").unwrap().appearance, crate::theme::Appearance::Light);
    }

    #[test]
    fn terminal_theme_uses_reset_and_indices() {
        let theme = terminal();
        assert_eq!(theme.palette.get("bg").unwrap(), Colour::Reset);
        assert_eq!(theme.palette.get("accent").unwrap(), Colour::Ansi(4));
    }

    #[test]
    fn unknown_theme_is_none() {
        assert!(by_name("nope").is_none());
        assert_eq!(names().len(), SOURCES.len() + 1);
    }
}
