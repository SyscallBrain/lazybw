//! Theme loading: TOML files, `extends`, user overrides and validation.

use crate::theme::builtin;
use crate::theme::{Appearance, Colour, Palette, TOKEN_MAP, Theme};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// The raw theme file, exactly as written by a user.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ThemeFile {
    /// Theme name.
    pub name: String,
    /// Parent theme.
    pub extends: Option<String>,
    /// `dark` or `light`.
    pub appearance: Option<String>,
    /// Palette overrides.
    pub palette: BTreeMap<String, toml::Value>,
    /// Token overrides.
    pub tokens: BTreeMap<String, toml::Value>,
    /// Transparent background.
    pub transparent: Option<bool>,
}

/// Parses a theme file (without `extends`) into a fully mapped theme.
#[must_use]
pub fn parse_theme(name: &str, source: &str) -> Theme {
    let file: ThemeFile = toml::from_str(source).unwrap_or_default();
    let appearance = match file.appearance.as_deref() {
        Some("light") => Appearance::Light,
        _ => Appearance::Dark,
    };
    let mut palette = Palette::new();
    for (key, value) in &file.palette {
        if let Ok(colour) = Colour::parse(value) {
            palette.set(key, colour);
        }
    }
    let mut theme = Theme::from_palette(
        if file.name.is_empty() { name } else { &file.name },
        appearance,
        palette,
    );
    for (token, value) in &file.tokens {
        if let Ok(colour) = Colour::parse(value) {
            theme.tokens.insert(token.clone(), colour);
        }
    }
    if let Some(transparent) = file.transparent {
        theme.transparent = transparent;
    }
    theme
}

/// Errors and warnings from loading a theme.
#[derive(Debug, Clone, Default)]
pub struct LoadOutcome {
    /// The theme when loading succeeded.
    pub theme: Option<Theme>,
    /// Errors that prevented loading.
    pub errors: Vec<String>,
    /// Non-fatal findings.
    pub warnings: Vec<String>,
}

/// Resolves a theme by name, following `extends` through built-ins and the
/// user's theme directory. Cycles are reported, not followed.
#[must_use]
pub fn load(name: &str, user_dir: &Path) -> LoadOutcome {
    if name == "terminal" {
        let mut theme = builtin::terminal();
        let mut outcome = LoadOutcome { theme: Some(theme.clone()), ..Default::default() };
        theme.resolve(true, false);
        outcome.theme = Some(theme);
        return outcome;
    }
    let mut outcome = LoadOutcome::default();
    match resolve(name, user_dir, &mut Vec::new(), &mut outcome) {
        Some(theme) => {
            outcome.theme = Some(theme);
        }
        None => {
            if outcome.errors.is_empty() {
                outcome.errors.push(format!("theme \"{name}\" not found"));
            }
        }
    }
    outcome
}

fn resolve(
    name: &str,
    user_dir: &Path,
    seen: &mut Vec<String>,
    outcome: &mut LoadOutcome,
) -> Option<Theme> {
    if seen.iter().any(|n| n == name) {
        outcome.errors.push(format!("theme \"{name}\" extends itself (cycle)"));
        return None;
    }
    seen.push(name.to_owned());
    let (file, source_name) = match read_source(name, user_dir) {
        Some(found) => found,
        None => {
            outcome.errors.push(format!("theme \"{name}\" not found"));
            return None;
        }
    };
    let mut base = if let Some(parent) = &file.extends {
        resolve(parent, user_dir, seen, outcome)?
    } else {
        Theme::from_palette(name, Appearance::Dark, Palette::new())
    };
    // Apply overrides.
    for (key, value) in &file.palette {
        if !Palette::KEYS.contains(&key.as_str()) {
            outcome
                .warnings
                .push(format!("theme \"{source_name}\": unknown palette entry \"{key}\""));
            continue;
        }
        match Colour::parse(value) {
            Ok(colour) => base.palette.set(key, colour),
            Err(err) => outcome.errors.push(format!("theme \"{source_name}\": {err}")),
        }
    }
    let known_token = |token: &str| TOKEN_MAP.iter().any(|(t, _)| *t == token);
    for (token, value) in &file.tokens {
        if !known_token(token) {
            outcome.warnings.push(format!("theme \"{source_name}\": unknown token \"{token}\""));
            continue;
        }
        let parsed = match value {
            toml::Value::String(text) if base.palette.get(text).is_some() => {
                Ok(base.palette.get(text).expect("the match guard found this palette entry"))
            }
            other => Colour::parse(other),
        };
        match parsed {
            Ok(colour) => {
                base.tokens.insert(token.clone(), colour);
            }
            Err(err) => outcome.errors.push(format!("theme \"{source_name}\": {err}")),
        }
    }
    if let Some(appearance) = &file.appearance {
        base.appearance = match appearance.as_str() {
            "light" => Appearance::Light,
            "dark" => Appearance::Dark,
            other => {
                outcome.errors.push(format!(
                    "theme \"{source_name}\": appearance must be \"dark\" or \"light\", not \"{other}\""
                ));
                base.appearance
            }
        };
    }
    if let Some(transparent) = file.transparent {
        base.transparent = transparent;
    }
    base.name = if file.name.is_empty() { name.to_owned() } else { file.name.clone() };
    // Fill missing tokens from the standard mapping; report missing palette entries.
    for (token, palette_key) in TOKEN_MAP {
        match base.palette.get(palette_key) {
            Some(colour) => {
                base.tokens.entry((*token).to_owned()).or_insert(colour);
            }
            None => outcome.errors.push(format!(
                "theme \"{source_name}\": missing palette entry \"{palette_key}\" (needed by token \"{token}\")"
            )),
        }
    }
    if !outcome.errors.is_empty() {
        return None;
    }
    Some(base)
}

fn read_source(name: &str, user_dir: &Path) -> Option<(ThemeFile, String)> {
    if let Some((_, source)) = builtin::SOURCES.iter().find(|(n, _)| *n == name) {
        let file: ThemeFile = toml::from_str(source).ok()?;
        return Some((file, name.to_owned()));
    }
    let path = user_dir.join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path).ok()?;
    let file: ThemeFile = toml::from_str(&text).ok()?;
    Some((file, name.to_owned()))
}

/// Loads every theme from a directory, skipping broken files with warnings.
#[must_use]
pub fn load_user_themes(user_dir: &Path) -> (Vec<Theme>, Vec<String>) {
    let mut themes = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(user_dir) else {
        return (themes, warnings);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let outcome = load(stem, user_dir);
        warnings.extend(outcome.errors.into_iter().map(|e| format!("{}: {e}", path.display())));
        warnings.extend(outcome.warnings);
        if let Some(theme) = outcome.theme {
            themes.push(theme);
        }
    }
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    (themes, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn extends_overrides_palette_entry() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("my-night.toml"),
            "name = \"my-night\"\nextends = \"tokyonight-night\"\n\n[palette]\naccent = \"#bb9af7\"\n",
        )
        .unwrap();
        let outcome = load("my-night", tmp.path());
        let theme = outcome.theme.unwrap();
        assert_eq!(theme.palette.get("accent").unwrap().hex().as_deref(), Some("#bb9af7"));
        assert_eq!(theme.palette.get("bg").unwrap().hex().as_deref(), Some("#1a1b26"));
    }

    #[test]
    fn token_override_accepts_palette_name_and_hex() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("t.toml"),
            "extends = \"nord\"\n[tokens]\n\"border.focus\" = \"magenta\"\n\"match\" = \"#ff0000\"\n",
        )
        .unwrap();
        let theme = load("t", tmp.path()).theme.unwrap();
        assert_eq!(theme.tokens.get("match").unwrap().hex().as_deref(), Some("#ff0000"));
        assert_eq!(theme.tokens.get("border.focus").copied(), theme.palette.get("magenta"));
    }

    #[test]
    fn cycles_are_errors() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.toml"), "extends = \"b\"\n").unwrap();
        std::fs::write(tmp.path().join("b.toml"), "extends = \"a\"\n").unwrap();
        let outcome = load("a", tmp.path());
        assert!(outcome.theme.is_none());
        assert!(outcome.errors.iter().any(|e| e.contains("cycle")));
    }

    #[test]
    fn unknown_entries_are_warnings() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("w.toml"),
            "extends = \"nord\"\n[palette]\nnovel = \"#010203\"\n[tokens]\n\"not.a.token\" = \"red\"\n",
        )
        .unwrap();
        let outcome = load("w", tmp.path());
        assert!(outcome.theme.is_some());
        assert_eq!(outcome.warnings.len(), 2);
    }

    #[test]
    fn missing_parent_is_an_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("m.toml"), "extends = \"nope\"\n").unwrap();
        let outcome = load("m", tmp.path());
        assert!(outcome.theme.is_none());
        assert!(outcome.errors[0].contains("not found"));
    }

    #[test]
    fn every_user_theme_loads() {
        let tmp = TempDir::new().unwrap();
        for name in builtin::names() {
            let outcome = load(name, tmp.path());
            assert!(outcome.theme.is_some(), "{name}: {:?}", outcome.errors);
        }
    }
}
