//! Configuration: structs, defaults, loading and validation.
//!
//! Every key in `assets/default-config.toml` has an equivalent field here.
//! Unknown keys are reported as warnings and kept out of the typed model;
//! invalid values are errors shown on the startup error screen.

pub mod duration;
pub mod keymap;
pub mod persist;
pub mod watch;

use crate::config::duration::DurationSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The embedded default configuration.
pub const DEFAULT_CONFIG: &str = include_str!("../../assets/default-config.toml");

/// Transport used to talk to `bw serve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Private socketpair on fd 3.
    #[default]
    Fd,
    /// Unix socket in `$XDG_RUNTIME_DIR/lazybw`.
    Unix,
}

/// Icon set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    /// Nerd Font glyphs.
    #[default]
    Nerdfont,
    /// Unicode symbols.
    Unicode,
    /// Plain ASCII.
    Ascii,
}

impl IconMode {
    /// Cycles to the next mode, used by `:set icons`.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Nerdfont => Self::Unicode,
            Self::Unicode => Self::Ascii,
            Self::Ascii => Self::Nerdfont,
        }
    }

    /// Label for settings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Nerdfont => "nerdfont",
            Self::Unicode => "unicode",
            Self::Ascii => "ascii",
        }
    }
}

/// List density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    /// Two rows per item.
    #[default]
    Comfortable,
    /// One row per item.
    Compact,
}

impl Density {
    /// The other density.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }

    /// Label for settings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
}

/// Statusline separator style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeparatorStyle {
    /// Powerline arrow.
    #[default]
    Arrow,
    /// Powerline round.
    Round,
    /// Powerline slant.
    Slant,
    /// A plain space.
    None,
}

impl SeparatorStyle {
    /// Label for settings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Arrow => "arrow",
            Self::Round => "round",
            Self::Slant => "slant",
            Self::None => "none",
        }
    }

    /// Cycles to the next style.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Arrow => Self::Round,
            Self::Round => Self::Slant,
            Self::Slant => Self::None,
            Self::None => Self::Arrow,
        }
    }
}

/// Date rendering style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    /// `3 days ago`, `12 Mar 2023`.
    #[default]
    Relative,
    /// `2026-03-12`.
    Iso,
}

impl DateFormat {
    /// The other style.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Relative => Self::Iso,
            Self::Iso => Self::Relative,
        }
    }

    /// True for ISO.
    #[must_use]
    pub fn is_iso(self) -> bool {
        matches!(self, Self::Iso)
    }
}

/// Sort order of the item list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Alphabetical.
    #[default]
    Name,
    /// Most recently modified first.
    Modified,
    /// Newest first.
    Created,
    /// Grouped by type, then name.
    Type,
}

impl SortOrder {
    /// Cycles to the next order.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Modified,
            Self::Modified => Self::Created,
            Self::Created => Self::Type,
            Self::Type => Self::Name,
        }
    }

    /// Parses a `:sort` argument.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "name" => Some(Self::Name),
            "modified" => Some(Self::Modified),
            "created" => Some(Self::Created),
            "type" => Some(Self::Type),
            _ => None,
        }
    }

    /// Label for messages.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Type => "type",
        }
    }
}

/// Generator mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GeneratorType {
    /// Random characters.
    #[default]
    Password,
    /// Random words.
    Passphrase,
}

impl GeneratorType {
    /// The other mode.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Password => Self::Passphrase,
            Self::Passphrase => Self::Password,
        }
    }

    /// Label for settings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Passphrase => "passphrase",
        }
    }
}

/// `[bw]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct BwConfig {
    /// Path to the CLI; empty means `$LAZYBW_BW`, then `$PATH`.
    pub path: String,
    /// Transport for `bw serve`.
    pub transport: Transport,
    /// Reuse `BW_SESSION` from the environment for the first serve start.
    pub inherit_session: bool,
}

impl Default for BwConfig {
    fn default() -> Self {
        Self { path: String::new(), transport: Transport::Fd, inherit_session: false }
    }
}

/// `[ui]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct UiConfig {
    /// Theme name.
    pub theme: String,
    /// Transparent background.
    pub transparent: bool,
    /// Icon mode.
    pub icons: IconMode,
    /// Enable animations.
    pub animations: bool,
    /// List density.
    pub list_density: Density,
    /// Statusline separator style.
    pub statusline_separator: SeparatorStyle,
    /// Show inline key hints.
    pub show_hints: bool,
    /// Date format.
    pub date_format: DateFormat,
    /// Enable mouse support.
    pub mouse: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "tokyonight-night".to_owned(),
            transparent: false,
            icons: IconMode::Nerdfont,
            animations: true,
            list_density: Density::Comfortable,
            statusline_separator: SeparatorStyle::Arrow,
            show_hints: true,
            date_format: DateFormat::Relative,
            mouse: true,
        }
    }
}

/// `[security]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SecurityConfig {
    /// Auto-clear delay for the clipboard.
    pub clipboard_timeout: DurationSpec,
    /// Clear the clipboard on exit.
    pub clear_clipboard_on_exit: bool,
    /// Lock after this much inactivity.
    pub auto_lock: DurationSpec,
    /// Re-hide revealed fields after this delay.
    pub reveal_timeout: DurationSpec,
    /// Re-hide when the terminal loses focus.
    pub hide_on_focus_lost: bool,
    /// How long a master password re-prompt stays valid.
    pub reprompt_grace: DurationSpec,
    /// Show TOTP codes.
    pub totp: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            clipboard_timeout: DurationSpec::from(Duration::from_secs(30)),
            clear_clipboard_on_exit: true,
            auto_lock: DurationSpec::from(Duration::from_secs(900)),
            reveal_timeout: DurationSpec::from(Duration::from_secs(20)),
            hide_on_focus_lost: true,
            reprompt_grace: DurationSpec::from(Duration::from_secs(0)),
            totp: true,
        }
    }
}

/// `[vault]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct VaultConfig {
    /// Background sync interval.
    pub sync_interval: DurationSpec,
    /// Default sort.
    pub sort: SortOrder,
    /// Include notes in fuzzy search.
    pub search_notes: bool,
    /// Show item types with zero items in the sidebar.
    pub show_empty_types: bool,
    /// Ask before moving an item to trash.
    pub confirm_trash: bool,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            sync_interval: DurationSpec::from(Duration::from_secs(600)),
            sort: SortOrder::Name,
            search_notes: false,
            show_empty_types: false,
            confirm_trash: false,
        }
    }
}

/// `[generator]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct GeneratorConfig {
    /// Password or passphrase.
    #[serde(rename = "type")]
    pub kind: GeneratorType,
    /// Password length.
    pub length: u8,
    /// Include uppercase letters.
    pub uppercase: bool,
    /// Include lowercase letters.
    pub lowercase: bool,
    /// Include digits.
    pub numbers: bool,
    /// Include symbols.
    pub special: bool,
    /// Minimum digits.
    pub min_numbers: u8,
    /// Minimum symbols.
    pub min_special: u8,
    /// Avoid ambiguous characters.
    pub avoid_ambiguous: bool,
    /// Passphrase words.
    pub words: u8,
    /// Passphrase separator.
    pub separator: String,
    /// Capitalise words.
    pub capitalize: bool,
    /// Include a number.
    pub include_number: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            kind: GeneratorType::Password,
            length: 24,
            uppercase: true,
            lowercase: true,
            numbers: true,
            special: true,
            min_numbers: 1,
            min_special: 1,
            avoid_ambiguous: false,
            words: 5,
            separator: "-".to_owned(),
            capitalize: true,
            include_number: true,
        }
    }
}

/// `[health]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HealthConfig {
    /// Check Have I Been Pwned through `bw report`.
    pub check_exposed: bool,
    /// Age after which a password is old.
    pub old_password_age: DurationSpec,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_exposed: true,
            old_password_age: DurationSpec::from(Duration::from_secs(365 * 86_400)),
        }
    }
}

/// `[editor]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct EditorConfig {
    /// Command used by `$EDITOR` features; empty means `$VISUAL`, `$EDITOR`, `vi`.
    pub command: String,
}

/// `[keys]` plus the per-context tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct KeysConfig {
    /// The leader key in notation.
    pub leader: String,
    /// Timeout for ambiguous sequences.
    pub timeoutlen: DurationSpec,
    /// Delay before which-key appears.
    pub which_key_delay: DurationSpec,
    /// Start from the built-in bindings.
    pub defaults: bool,
    /// `[keys.<context>]` tables; `labels` holds which-key group labels.
    #[serde(flatten)]
    pub contexts: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            leader: "<Space>".to_owned(),
            timeoutlen: DurationSpec::from(Duration::from_millis(1000)),
            which_key_delay: DurationSpec::from(Duration::from_millis(300)),
            defaults: true,
            contexts: BTreeMap::new(),
        }
    }
}

impl KeysConfig {
    /// User bindings for a context.
    #[must_use]
    pub fn context(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        self.contexts.get(name)
    }

    /// Which-key labels.
    #[must_use]
    pub fn labels(&self) -> Option<&BTreeMap<String, String>> {
        self.contexts.get("labels")
    }
}

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct Config {
    /// `[bw]`
    pub bw: BwConfig,
    /// `[ui]`
    pub ui: UiConfig,
    /// `[security]`
    pub security: SecurityConfig,
    /// `[vault]`
    pub vault: VaultConfig,
    /// `[generator]`
    pub generator: GeneratorConfig,
    /// `[health]`
    pub health: HealthConfig,
    /// `[editor]`
    pub editor: EditorConfig,
    /// `[keys]`
    pub keys: KeysConfig,
}

impl Config {
    /// Parses configuration text, returning the config and warnings for
    /// unknown keys.
    pub fn from_toml(text: &str) -> Result<(Self, Vec<String>), ConfigError> {
        let value: toml::Value =
            toml::from_str(text).map_err(|e| ConfigError::from_toml(e, text))?;
        let config: Self = value
            .clone()
            .try_into()
            .map_err(|e: toml::de::Error| ConfigError::from_toml(e, text))?;
        let mut warnings = Vec::new();
        let schema =
            toml::Value::try_from(&config).unwrap_or(toml::Value::Table(Default::default()));
        collect_unknown("", &value, &schema, &mut warnings);
        Ok((config, warnings))
    }

    /// Loads the configuration from disk; a missing file yields defaults.
    pub fn load(path: &Path) -> Result<(Self, Vec<String>), ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|e| e.with_path(path)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok((Self::default(), Vec::new()))
            }
            Err(err) => Err(ConfigError {
                path: Some(path.to_path_buf()),
                line: None,
                column: None,
                message: format!("couldn't read the config file: {err}"),
            }),
        }
    }

    /// Semantic validation; the returned strings are shown to the user.
    #[must_use]
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if !(5..=128).contains(&self.generator.length) {
            errors.push(format!(
                "generator.length must be between 5 and 128 (found {})",
                self.generator.length
            ));
        }
        if !(3..=20).contains(&self.generator.words) {
            errors.push(format!(
                "generator.words must be between 3 and 20 (found {})",
                self.generator.words
            ));
        }
        if self.generator.min_numbers > self.generator.length {
            errors.push("generator.min_numbers cannot exceed generator.length".to_owned());
        }
        if self.generator.min_special > self.generator.length {
            errors.push("generator.min_special cannot exceed generator.length".to_owned());
        }
        if self.ui.theme.trim().is_empty() {
            errors.push("ui.theme cannot be empty".to_owned());
        }
        if self.keys.leader.trim().is_empty() {
            errors.push("keys.leader cannot be empty".to_owned());
        }
        errors
    }

    /// The timeout used for ambiguous key sequences.
    #[must_use]
    pub fn timeoutlen(&self) -> Duration {
        self.keys.timeoutlen.get().unwrap_or(Duration::from_secs(1))
    }

    /// The which-key delay.
    #[must_use]
    pub fn which_key_delay(&self) -> Duration {
        self.keys.which_key_delay.get().unwrap_or(Duration::from_millis(300))
    }
}

/// A configuration problem with its source location when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The file, when loaded from disk.
    pub path: Option<PathBuf>,
    /// 1-based line.
    pub line: Option<usize>,
    /// 1-based column.
    pub column: Option<usize>,
    /// Human readable message.
    pub message: String,
}

impl ConfigError {
    fn from_toml(error: toml::de::Error, text: &str) -> Self {
        let span = error.span().unwrap_or(0..0);
        let upto = text.get(..span.start.min(text.len())).unwrap_or("");
        let line = upto.lines().count().max(1);
        let column = upto.rsplit('\n').next().map(|l| l.chars().count() + 1).unwrap_or(1);
        let message = error.message().to_owned();
        let mut line = line;
        let mut column = column;
        if span.start == 0 {
            // Some serde errors carry no useful span; locate the quoted
            // token from the message in the source instead.
            if let Some(token) = message.split('`').nth(1).or_else(|| message.split('"').nth(1))
                && let Some(index) = text.find(token)
            {
                let upto = &text[..index];
                line = upto.lines().count().max(1);
                column = upto.rsplit('\n').next().map(|l| l.chars().count() + 1).unwrap_or(1);
            }
        }
        Self { path: None, line: Some(line), column: Some(column), message }
    }

    fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }

    /// A one-line description for the status bar.
    #[must_use]
    pub fn summary(&self) -> String {
        let location = match (&self.path, self.line, self.column) {
            (Some(path), Some(line), Some(col)) => format!("{}:{line}:{col}: ", path.display()),
            (None, Some(line), Some(col)) => format!("{line}:{col}: "),
            (Some(path), _, _) => format!("{}: ", path.display()),
            (None, Some(line), None) => format!("{line}: "),
            _ => String::new(),
        };
        format!("{location}{}", self.message)
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

impl std::error::Error for ConfigError {}

fn collect_unknown(
    prefix: &str,
    source: &toml::Value,
    schema: &toml::Value,
    out: &mut Vec<String>,
) {
    match (source, schema) {
        (toml::Value::Table(source), toml::Value::Table(schema)) => {
            for (key, value) in source {
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                match schema.get(key) {
                    Some(expected) => collect_unknown(&path, value, expected, out),
                    None => out.push(format!("unknown configuration key \"{path}\"")),
                }
            }
        }
        (toml::Value::Array(source), toml::Value::Array(schema)) => {
            if let Some(expected) = schema.first() {
                for (i, value) in source.iter().enumerate() {
                    collect_unknown(&format!("{prefix}[{i}]"), value, expected, out);
                }
            }
        }
        _ => {}
    }
}

/// Resolved XDG paths used by lazybw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `$XDG_CONFIG_HOME/lazybw`.
    pub config_dir: PathBuf,
    /// `$XDG_STATE_HOME/lazybw`.
    pub state_dir: PathBuf,
}

impl Paths {
    /// Resolves the paths, falling back to `~/.config` and `~/.local/state`.
    #[must_use]
    pub fn resolve() -> Self {
        let base = directories::BaseDirs::new();
        let config_dir = base
            .as_ref()
            .map(|b| b.config_dir().to_path_buf())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("lazybw");
        let state_dir = base
            .as_ref()
            .map(|b| {
                b.state_dir()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| b.home_dir().join(".local/state"))
            })
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_else(|| PathBuf::from(".local/state"))
            .join("lazybw");
        Self { config_dir, state_dir }
    }

    /// `config.toml`.
    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// `themes/`.
    #[must_use]
    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    /// `lazybw.log`.
    #[must_use]
    pub fn log_file(&self) -> PathBuf {
        self.state_dir.join("lazybw.log")
    }

    /// The collapsed sidebar folders (ids only, see `vault::folds`).
    #[must_use]
    pub fn folds_file(&self) -> PathBuf {
        self.state_dir.join("folds.json")
    }

    /// The command-line history file.
    #[must_use]
    pub fn command_history(&self) -> PathBuf {
        self.state_dir.join("cmd_history")
    }

    /// The cached `bw --version` result, keyed on the executable's metadata.
    #[must_use]
    pub fn bw_version_file(&self) -> PathBuf {
        self.state_dir.join("bw-version.json")
    }
}

/// Formats a duration for a settings row.
#[must_use]
pub fn duration_label(spec: DurationSpec) -> String {
    spec.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_embedded_file() {
        let (parsed, warnings) = Config::from_toml(DEFAULT_CONFIG).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(parsed, Config::default());
        assert!(parsed.validate().is_empty());
    }

    #[test]
    fn unknown_keys_are_warnings_not_errors() {
        let text = "[ui]\ntheme = \"nord\"\nfuture_option = 1\n[mystery]\nx = 2\n";
        let (config, warnings) = Config::from_toml(text).unwrap();
        assert_eq!(config.ui.theme, "nord");
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("ui.future_option")));
        assert!(warnings.iter().any(|w| w.contains("mystery")));
    }

    #[test]
    fn bad_value_reports_line() {
        let err = Config::from_toml("[ui]\nlist_density = \"dense\"\n").unwrap_err();
        assert_eq!(err.line, Some(2));
        assert!(err.summary().contains("dense") || err.summary().contains("list_density"));
        assert!(err.summary().starts_with("2:"));
    }

    #[test]
    fn validation_catches_ranges() {
        let mut config = Config::default();
        config.generator.length = 3;
        config.generator.words = 99;
        let errors = config.validate();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn user_bindings_parse_into_contexts() {
        let text = "[keys.list]\n\"dd\" = \"noop\"\n\"<Del>\" = \"item.trash\"\n";
        let (config, warnings) = Config::from_toml(text).unwrap();
        assert!(warnings.is_empty());
        let list = config.keys.context("list").unwrap();
        assert_eq!(list.get("dd").map(String::as_str), Some("noop"));
        assert_eq!(list.get("<Del>").map(String::as_str), Some("item.trash"));
    }

    #[test]
    fn missing_file_is_defaults() {
        let (config, warnings) =
            Config::load(Path::new("/nonexistent/lazybw/config.toml")).unwrap();
        assert_eq!(config, Config::default());
        assert!(warnings.is_empty());
    }
}
