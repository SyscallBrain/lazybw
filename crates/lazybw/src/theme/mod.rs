//! The visual theme: a palette of colours and the semantic tokens widgets use.
//!
//! Widgets never name raw colours; they ask for a token (`fg.muted`,
//! `border.focus`, `type.login`). A theme maps tokens onto palette entries.
//! User themes can override palette entries and token mappings, extend a
//! built-in theme, and turn on a transparent background.

pub mod builtin;
pub mod loader;

use ratatui::style::{Color, Modifier, Style};
use std::collections::BTreeMap;
use std::fmt;

/// Whether a theme is designed for a dark or light terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    /// Dark background.
    #[default]
    Dark,
    /// Light background.
    Light,
}

impl Appearance {
    /// Label used by the theme picker.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// A palette entry before truecolor is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    /// `#rrggbb`.
    Rgb(u8, u8, u8),
    /// A terminal palette index (0–255).
    Ansi(u8),
    /// The terminal default.
    Reset,
}

impl Colour {
    /// Resolves to a ratatui colour, optionally downgrading RGB to 256.
    #[must_use]
    pub fn to_color(self, truecolor: bool) -> Color {
        match self {
            Self::Rgb(r, g, b) if truecolor => Color::Rgb(r, g, b),
            Self::Rgb(r, g, b) => Color::Indexed(rgb_to_ansi256(r, g, b)),
            Self::Ansi(index) => Color::Indexed(index),
            Self::Reset => Color::Reset,
        }
    }

    /// Parses `#rrggbb`, an integer index, or `reset`.
    pub fn parse(value: &toml::Value) -> Result<Self, String> {
        match value {
            toml::Value::Integer(index) if (0..=255).contains(index) => {
                Ok(Self::Ansi(*index as u8))
            }
            toml::Value::String(text) => {
                let text = text.trim();
                if text.eq_ignore_ascii_case("reset") || text.eq_ignore_ascii_case("default") {
                    return Ok(Self::Reset);
                }
                if let Some(hex) = text.strip_prefix('#') {
                    if hex.len() == 6 {
                        let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
                        let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
                        let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
                        return Ok(Self::Rgb(r, g, b));
                    }
                    return Err(format!("invalid colour \"{text}\": expected #rrggbb"));
                }
                if let Ok(index) = text.parse::<u8>() {
                    return Ok(Self::Ansi(index));
                }
                Err(format!("invalid colour \"{text}\": expected #rrggbb, 0-255 or reset"))
            }
            _ => Err("invalid colour: expected a string or an ANSI index".to_owned()),
        }
    }

    /// Hex form for tests and mockups.
    #[must_use]
    pub fn hex(self) -> Option<String> {
        match self {
            Self::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
            _ => None,
        }
    }
}

/// The 20 palette entries every theme defines.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Palette {
    entries: BTreeMap<String, Colour>,
}

impl Palette {
    /// All palette keys in the canonical order.
    pub const KEYS: &'static [&'static str] = &[
        "bg",
        "bg_dark",
        "bg_highlight",
        "bg_visual",
        "border",
        "fg",
        "fg_dark",
        "comment",
        "dark5",
        "accent",
        "blue",
        "cyan",
        "magenta",
        "purple",
        "orange",
        "yellow",
        "green",
        "teal",
        "red",
        "shadow",
    ];

    /// An empty palette.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an entry.
    pub fn set(&mut self, key: &str, colour: Colour) {
        self.entries.insert(key.to_owned(), colour);
    }

    /// Gets an entry.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Colour> {
        self.entries.get(key).copied()
    }

    /// The entries in canonical order.
    #[must_use]
    pub fn entries(&self) -> Vec<(&str, Colour)> {
        Self::KEYS.iter().filter_map(|k| self.entries.get(*k).map(|c| (*k, *c))).collect()
    }

    /// Missing keys after `extends` resolution.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        Self::KEYS.iter().copied().filter(|k| !self.entries.contains_key(*k)).collect()
    }
}

/// The default token → palette mapping from design.md §6.1.
pub const TOKEN_MAP: &[(&str, &str)] = &[
    ("bg", "bg"),
    ("bg.bar", "bg_dark"),
    ("bg.float", "bg_dark"),
    ("bg.highlight", "bg_highlight"),
    ("selection", "bg_visual"),
    ("border", "border"),
    ("border.focus", "accent"),
    ("fg", "fg"),
    ("fg.alt", "fg_dark"),
    ("fg.muted", "comment"),
    ("fg.subtle", "dark5"),
    ("accent", "accent"),
    ("mode.normal", "blue"),
    ("mode.insert", "green"),
    ("mode.visual", "magenta"),
    ("mode.command", "yellow"),
    ("mode.search", "orange"),
    ("mode.locked", "red"),
    ("type.login", "blue"),
    ("type.card", "orange"),
    ("type.identity", "green"),
    ("type.note", "magenta"),
    ("type.ssh", "cyan"),
    ("type.other", "fg_dark"),
    ("fav", "yellow"),
    ("ok", "green"),
    ("info", "cyan"),
    ("caution", "yellow"),
    ("warn", "orange"),
    ("error", "red"),
    ("match", "orange"),
    ("link", "cyan"),
    ("hint.key", "blue"),
    ("totp.code", "teal"),
    ("totp.fresh", "teal"),
    ("totp.expiring", "yellow"),
    ("totp.critical", "red"),
    ("pw.digit", "orange"),
    ("pw.symbol", "magenta"),
    ("dirty", "orange"),
    ("shadow", "shadow"),
];

/// Tokens that carry extra modifiers.
const TOKEN_MODIFIERS: &[(&str, Modifier)] = &[
    ("match", Modifier::BOLD),
    ("link", Modifier::UNDERLINED),
    ("accent", Modifier::BOLD),
    ("dirty", Modifier::BOLD),
];

/// A resolved theme.
#[derive(Clone)]
pub struct Theme {
    /// Theme name.
    pub name: String,
    /// Dark or light.
    pub appearance: Appearance,
    /// The terminal background shows through.
    pub transparent: bool,
    /// Palette.
    pub palette: Palette,
    /// Token → colour.
    pub tokens: BTreeMap<String, Colour>,
    /// Resolved styles, after truecolor/monochrome decisions.
    styles: BTreeMap<String, Style>,
    /// Colours are RGB (truecolor) rather than indexed.
    pub truecolor: bool,
    /// `NO_COLOR` mode: colours reset, meaning carried by modifiers.
    pub monochrome: bool,
}

impl fmt::Debug for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Theme")
            .field("name", &self.name)
            .field("appearance", &self.appearance)
            .field("transparent", &self.transparent)
            .field("truecolor", &self.truecolor)
            .finish_non_exhaustive()
    }
}

impl Theme {
    /// Builds a theme from a palette using the default token mapping.
    #[must_use]
    pub fn from_palette(name: &str, appearance: Appearance, palette: Palette) -> Self {
        let tokens = TOKEN_MAP
            .iter()
            .filter_map(|(token, key)| palette.get(key).map(|c| ((*token).to_owned(), c)))
            .collect();
        Self {
            name: name.to_owned(),
            appearance,
            transparent: false,
            palette,
            tokens,
            styles: BTreeMap::new(),
            truecolor: true,
            monochrome: false,
        }
    }

    /// Finalises styles for the current terminal capabilities. Call once
    /// after loading and after changing `transparent` or `NO_COLOR`.
    pub fn resolve(&mut self, truecolor: bool, monochrome: bool) {
        self.truecolor = truecolor;
        self.monochrome = monochrome;
        let mut styles = BTreeMap::new();
        for (token, colour) in &self.tokens {
            let mut style = if monochrome {
                Style::default().fg(Color::Reset)
            } else {
                let colour = if self.transparent && matches!(token.as_str(), "bg" | "bg.bar") {
                    Colour::Reset
                } else {
                    *colour
                };
                Style::default().fg(colour.to_color(truecolor))
            };
            if let Some((_, modifier)) = TOKEN_MODIFIERS.iter().find(|(t, _)| t == token) {
                style = style.add_modifier(*modifier);
            }
            styles.insert(token.clone(), style);
        }
        if monochrome {
            for token in ["accent", "match", "dirty", "fg"] {
                if let Some(style) = styles.get_mut(token) {
                    *style = style.add_modifier(Modifier::BOLD);
                }
            }
            for token in ["border.focus", "selection"] {
                if let Some(style) = styles.get_mut(token) {
                    *style = style.add_modifier(Modifier::REVERSED);
                }
            }
        }
        self.styles = styles;
    }

    /// The colour of a token.
    #[must_use]
    pub fn colour(&self, token: &str) -> Color {
        if self.monochrome {
            return Color::Reset;
        }
        self.tokens.get(token).copied().unwrap_or(Colour::Reset).to_color(self.truecolor)
    }

    /// The foreground style of a token.
    #[must_use]
    pub fn style(&self, token: &str) -> Style {
        self.styles.get(token).copied().unwrap_or_else(|| Style::default().fg(self.colour(token)))
    }

    /// The foreground style of a token with a background from another token.
    #[must_use]
    pub fn on(&self, foreground: &str, background: &str) -> Style {
        self.style(foreground).bg(self.colour(background))
    }

    /// Bold variant.
    #[must_use]
    pub fn bold(&self, token: &str) -> Style {
        self.style(token).add_modifier(Modifier::BOLD)
    }

    /// The item type token (`type.login`, …).
    #[must_use]
    pub fn type_token(kind: crate::bw::models::ItemType) -> &'static str {
        use crate::bw::models::ItemType;
        match kind {
            ItemType::Login => "type.login",
            ItemType::Card => "type.card",
            ItemType::Identity => "type.identity",
            ItemType::SecureNote => "type.note",
            ItemType::SshKey => "type.ssh",
            ItemType::Other(_) => "type.other",
        }
    }

    /// Background for the editor's active row.
    #[must_use]
    pub fn active_row(&self) -> Style {
        self.on("fg", "bg.highlight")
    }

    /// Selected row in a focused list.
    #[must_use]
    pub fn selected_row(&self) -> Style {
        self.on("fg", "selection").add_modifier(Modifier::BOLD)
    }

    /// Selected row in an unfocused list.
    #[must_use]
    pub fn inactive_row(&self) -> Style {
        self.on("fg", "bg.highlight")
    }
}

/// Converts RGB to the nearest xterm-256 index (6×6×6 cube or grey ramp).
#[must_use]
pub fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    let cube = |v: u8| -> u8 {
        (if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (u16::from(v) - 35) / 40
        }) as u8
    };
    let qr = cube(r);
    let qg = cube(g);
    let qb = cube(b);
    let cube_index = 16 + 36 * qr + 6 * qg + qb;
    let cube_r = if qr == 0 { 0 } else { 55 + 40 * qr };
    let cube_g = if qg == 0 { 0 } else { 55 + 40 * qg };
    let cube_b = if qb == 0 { 0 } else { 55 + 40 * qb };
    let cube_distance = distance((r, g, b), (cube_r, cube_g, cube_b));
    let grey_index = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3).saturating_sub(8) / 10;
    let grey_index = grey_index.min(23) as u8;
    let grey = 8 + 10 * grey_index;
    let grey_distance = distance((r, g, b), (grey, grey, grey));
    if grey_distance < cube_distance { 232 + grey_index } else { cube_index }
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let dr = i32::from(a.0) - i32::from(b.0);
    let dg = i32::from(a.1) - i32::from(b.1);
    let db = i32::from(a.2) - i32::from(b.2);
    (dr * dr + dg * dg + db * db) as u32
}

/// True when the terminal advertises 24-bit colour.
#[must_use]
pub fn truecolor_available() -> bool {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default().to_ascii_lowercase();
    if matches!(colorterm.as_str(), "truecolor" | "24bit") {
        return true;
    }
    // Terminals that do not export COLORTERM (common inside multiplexers and
    // over SSH) still advertise direct colour through TERM.
    let term = std::env::var("TERM").unwrap_or_default().to_ascii_lowercase();
    term.ends_with("-direct") || term.contains("truecolor") || term.contains("24bit")
}

/// True when `NO_COLOR` is set to a non-empty value.
#[must_use]
pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_theme_defines_every_token() {
        for theme in builtin::all() {
            for (token, _) in TOKEN_MAP {
                assert!(theme.tokens.contains_key(*token), "{} misses token {token}", theme.name);
            }
            assert!(theme.palette.missing().is_empty(), "{} misses palette entries", theme.name);
        }
    }

    #[test]
    fn every_theme_has_a_distinct_name() {
        let mut names: Vec<_> = builtin::all().iter().map(|t| t.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), builtin::all().len());
    }

    #[test]
    fn default_theme_is_tokyonight_night() {
        let theme = builtin::by_name("tokyonight-night").unwrap();
        assert_eq!(theme.palette.get("bg").unwrap().hex().as_deref(), Some("#1a1b26"));
        assert_eq!(theme.palette.get("accent").unwrap().hex().as_deref(), Some("#7aa2f7"));
    }

    #[test]
    fn transparent_uses_terminal_background() {
        let mut theme = builtin::by_name("tokyonight-night").unwrap();
        theme.transparent = true;
        theme.resolve(true, false);
        assert_eq!(theme.style("bg").fg, Some(Color::Reset));
        assert_eq!(theme.style("bg.bar").fg, Some(Color::Reset));
        assert_ne!(theme.style("bg.float").fg, Some(Color::Reset));
    }

    #[test]
    fn monochrome_resets_colours() {
        let mut theme = builtin::by_name("nord").unwrap();
        theme.resolve(true, true);
        assert_eq!(theme.style("fg").fg, Some(Color::Reset));
        assert!(theme.style("accent").add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn ansi256_conversion_is_stable() {
        assert_eq!(rgb_to_ansi256(0, 0, 0), 16);
        assert_eq!(rgb_to_ansi256(255, 255, 255), 231);
        assert!(rgb_to_ansi256(26, 27, 38) >= 232);
    }

    #[test]
    fn parses_colours() {
        assert_eq!(
            Colour::parse(&toml::Value::String("#ff0000".into())).unwrap(),
            Colour::Rgb(255, 0, 0)
        );
        assert_eq!(Colour::parse(&toml::Value::Integer(4)).unwrap(), Colour::Ansi(4));
        assert_eq!(Colour::parse(&toml::Value::String("reset".into())).unwrap(), Colour::Reset);
        assert!(Colour::parse(&toml::Value::String("red".into())).is_err());
    }
}
