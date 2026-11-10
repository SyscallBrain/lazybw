//! Vim key notation, the binding trie and validation.
//!
//! Keys are kept in a terminal-independent form so the parser, the trie and
//! the tests never need a real terminal. `input/keys.rs` converts crossterm
//! events to and from [`Key`].
//!
//! Binding sources, in order of precedence: user config → in-session `:map` →
//! defaults. Each context owns a trie; resolution walks the context chain from
//! the most specific context to `global`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Modifier bit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Mods(pub u8);

impl Mods {
    /// Control.
    pub const CTRL: Self = Self(1);
    /// Alt / Meta.
    pub const ALT: Self = Self(2);
    /// Shift.
    pub const SHIFT: Self = Self(4);

    /// True when all bits of `other` are set.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Adds a bit.
    #[must_use]
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A key without terminal-specific data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    /// A printable character.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Function key.
    F(u8),
}

/// A key press: code plus modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    /// The key itself.
    pub code: Code,
    /// Modifiers.
    pub mods: Mods,
}

impl Key {
    /// A plain character.
    #[must_use]
    pub const fn ch(c: char) -> Self {
        Self { code: Code::Char(c), mods: Mods(0) }
    }

    /// A named key without modifiers.
    #[must_use]
    pub const fn named(code: Code) -> Self {
        Self { code, mods: Mods(0) }
    }

    /// Adds modifiers.
    #[must_use]
    pub fn with(self, mods: Mods) -> Self {
        Self { code: self.code, mods: self.mods.with(mods) }
    }

    /// True for printable characters that text-entry contexts should insert.
    #[must_use]
    pub fn is_printable(self) -> bool {
        matches!(self.code, Code::Char(c) if !c.is_control())
            && !self.mods.contains(Mods::CTRL)
            && !self.mods.contains(Mods::ALT)
    }

    /// The space key, as typed by the leader.
    #[must_use]
    pub fn space() -> Self {
        Self::ch(' ')
    }

    /// Alternative keys the terminal may send instead of this one when
    /// keyboard enhancement (`DISAMBIGUATE_ESCAPE_CODES`) is unavailable.
    #[must_use]
    pub fn legacy_alternatives(self) -> Vec<Self> {
        if !self.mods.contains(Mods::CTRL) {
            return Vec::new();
        }
        match self.code {
            Code::Char('i') => vec![Self::named(Code::Tab)],
            Code::Char('m') => vec![Self::named(Code::Enter)],
            Code::Char('[') => vec![Self::named(Code::Esc)],
            Code::Char('h') => vec![Self::named(Code::Backspace)],
            _ => Vec::new(),
        }
    }

    /// Canonical notation form, e.g. `dd`, `<C-w>`, `<Space>`.
    #[must_use]
    pub fn notation(self) -> String {
        if self.mods.is_empty() {
            return match self.code {
                Code::Char(' ') => "<Space>".to_owned(),
                Code::Char('<') => "<lt>".to_owned(),
                Code::Char('|') => "<Bar>".to_owned(),
                Code::Char(c) => c.to_string(),
                other => format!("<{}>", code_name(other)),
            };
        }
        let mut out = String::from("<");
        if self.mods.contains(Mods::CTRL) {
            out.push_str("C-");
        }
        if self.mods.contains(Mods::ALT) {
            out.push_str("A-");
        }
        if self.mods.contains(Mods::SHIFT) {
            out.push_str("S-");
        }
        match self.code {
            Code::Char(c) => out.push(c),
            other => out.push_str(code_name(other)),
        }
        out.push('>');
        out
    }
}

fn code_name(code: Code) -> &'static str {
    match code {
        Code::Char(_) => "",
        Code::Enter => "CR",
        Code::Esc => "Esc",
        Code::Tab => "Tab",
        Code::BackTab => "S-Tab",
        Code::Backspace => "BS",
        Code::Delete => "Del",
        Code::Insert => "Insert",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "PageUp",
        Code::PageDown => "PageDown",
        Code::Up => "Up",
        Code::Down => "Down",
        Code::Left => "Left",
        Code::Right => "Right",
        Code::F(1) => "F1",
        Code::F(2) => "F2",
        Code::F(3) => "F3",
        Code::F(4) => "F4",
        Code::F(5) => "F5",
        Code::F(6) => "F6",
        Code::F(7) => "F7",
        Code::F(8) => "F8",
        Code::F(9) => "F9",
        Code::F(10) => "F10",
        Code::F(11) => "F11",
        Code::F(12) => "F12",
        Code::F(_) => "F?",
    }
}

/// A notation error with the offending column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotationError {
    /// 1-based column in the input.
    pub column: usize,
    /// Human readable message.
    pub message: String,
}

impl fmt::Display for NotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at column {}", self.message, self.column)
    }
}

impl std::error::Error for NotationError {}

/// Parses a key sequence in Vim notation.
pub fn parse_keys(input: &str) -> Result<Vec<Key>, NotationError> {
    let mut keys = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '<' {
            keys.push(Key::ch(c));
            i += 1;
            continue;
        }
        let start = i;
        let Some(close) = chars[i..].iter().position(|&x| x == '>').map(|p| i + p) else {
            return Err(NotationError {
                column: start + 1,
                message: "unterminated \"<\"".to_owned(),
            });
        };
        let inner: String = chars[start + 1..close].iter().collect();
        if inner.is_empty() {
            return Err(NotationError { column: start + 1, message: "empty \"<>\"".to_owned() });
        }
        keys.push(parse_named_key(&inner, start + 1)?);
        i = close + 1;
    }
    if keys.is_empty() {
        return Err(NotationError { column: 1, message: "empty key sequence".to_owned() });
    }
    Ok(keys)
}

fn parse_named_key(inner: &str, column: usize) -> Result<Key, NotationError> {
    let mut parts: Vec<&str> = inner.split('-').collect();
    let name = parts.pop().unwrap_or_default();
    let mut mods = Mods(0);
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "c" | "ctrl" | "control" => mods = mods.with(Mods::CTRL),
            "a" | "m" | "alt" | "meta" => mods = mods.with(Mods::ALT),
            "s" | "shift" => mods = mods.with(Mods::SHIFT),
            other => {
                return Err(NotationError {
                    column,
                    message: format!("unknown modifier \"{other}\""),
                });
            }
        }
    }
    let lower = name.to_ascii_lowercase();
    let code = match lower.as_str() {
        "cr" | "enter" | "return" => Code::Enter,
        "esc" | "escape" => Code::Esc,
        "tab" => Code::Tab,
        "s-tab" | "backtab" => Code::BackTab,
        "bs" | "backspace" => Code::Backspace,
        "del" | "delete" => Code::Delete,
        "insert" | "ins" => Code::Insert,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" | "pgup" => Code::PageUp,
        "pagedown" | "pgdn" => Code::PageDown,
        "up" => Code::Up,
        "down" => Code::Down,
        "left" => Code::Left,
        "right" => Code::Right,
        "space" => Code::Char(' '),
        "lt" => Code::Char('<'),
        "bar" => Code::Char('|'),
        "leader" => Code::Char(' '), // replaced by the keymap after parsing
        // F1–F12; a lone `f` is the letter (`<C-f>`).
        name if name.starts_with('f') && (2..=3).contains(&name.len()) => {
            let n: u8 = name[1..].parse().map_err(|_| NotationError {
                column,
                message: format!("unknown key \"{name}\""),
            })?;
            if !(1..=12).contains(&n) {
                return Err(NotationError { column, message: format!("unknown key \"{name}\"") });
            }
            Code::F(n)
        }
        name if name.chars().count() == 1 => Code::Char(name.chars().next().unwrap_or('?')),
        _ => {
            return Err(NotationError { column, message: format!("unknown key \"{name}\"") });
        }
    };
    let mut key = Key { code, mods };
    key = normalize(key);
    Ok(key)
}

/// Normalises a key: `<S-a>` becomes `A`, `<S-Tab>` becomes `BackTab`.
fn normalize(mut key: Key) -> Key {
    if key.mods.contains(Mods::SHIFT) {
        if key.code == Code::Tab {
            key.code = Code::BackTab;
            key.mods = Mods(mods_without(key.mods, Mods::SHIFT));
        } else if !key.mods.contains(Mods::CTRL)
            && !key.mods.contains(Mods::ALT)
            && let Code::Char(c) = key.code
            && c.is_ascii_alphabetic()
        {
            key.code = Code::Char(c.to_ascii_uppercase());
            key.mods = Mods(mods_without(key.mods, Mods::SHIFT));
        }
    }
    key
}

fn mods_without(mods: Mods, remove: Mods) -> u8 {
    mods.0 & !remove.0
}

/// Formats a sequence for hints and the statusline.
#[must_use]
pub fn display_keys(keys: &[Key]) -> String {
    let pieces: Vec<String> = keys.iter().map(|key| hint(*key)).collect();
    let spaced = pieces.first().is_some_and(|p| p.chars().count() > 1);
    if spaced {
        return pieces.join(" ");
    }
    pieces.concat()
}

/// A human hint for a key, e.g. `Ctrl-d`, `Space`.
#[must_use]
pub fn hint(key: Key) -> String {
    match (key.code, key.mods.contains(Mods::CTRL), key.mods.contains(Mods::ALT)) {
        (Code::Char(' '), false, false) => "Space".to_owned(),
        (Code::Char(c), true, false) => format!("Ctrl-{c}"),
        (Code::Char(c), false, true) => format!("Alt-{c}"),
        (Code::Char(c), true, true) => format!("Ctrl-Alt-{c}"),
        (Code::Char(c), false, false) => c.to_string(),
        (Code::Enter, _, _) => "Enter".to_owned(),
        (Code::Esc, _, _) => "Esc".to_owned(),
        (Code::Tab, _, _) => "Tab".to_owned(),
        (Code::BackTab, _, _) => "Shift-Tab".to_owned(),
        (Code::Backspace, _, _) => "Backspace".to_owned(),
        (Code::Delete, _, _) => "Del".to_owned(),
        (Code::Insert, _, _) => "Insert".to_owned(),
        (Code::Up, _, _) => "Up".to_owned(),
        (Code::Down, _, _) => "Down".to_owned(),
        (Code::Left, _, _) => "Left".to_owned(),
        (Code::Right, _, _) => "Right".to_owned(),
        (Code::Home, _, _) => "Home".to_owned(),
        (Code::End, _, _) => "End".to_owned(),
        (Code::PageUp, _, _) => "PageUp".to_owned(),
        (Code::PageDown, _, _) => "PageDown".to_owned(),
        (Code::F(n), _, _) => format!("F{n}"),
    }
}

/// Where a binding came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Built-in default.
    Default,
    /// `config.toml`.
    User,
    /// `:map`, for this session only.
    Session,
}

/// One action invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCall {
    /// Action id, e.g. `item.copy`.
    pub id: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Where it came from.
    pub source: Source,
}

impl ActionCall {
    /// Parses `"item.copy password"` into an id and arguments.
    #[must_use]
    pub fn parse(text: &str, source: Source) -> Self {
        let mut parts = split_args(text);
        let id = if parts.is_empty() { String::new() } else { parts.remove(0) };
        Self { id, args: parts, source }
    }

    /// The canonical text form.
    #[must_use]
    pub fn as_text(&self) -> String {
        if self.args.is_empty() {
            self.id.clone()
        } else {
            format!("{} {}", self.id, self.args.join(" "))
        }
    }
}

/// Splits an action string respecting double quotes.
#[must_use]
pub fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in text.trim().chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// One node of the binding trie.
#[derive(Debug, Default, Clone)]
struct Node {
    children: BTreeMap<Key, Node>,
    action: Option<String>,
}

/// A trie of key sequences for one context.
#[derive(Debug, Default, Clone)]
pub struct Trie {
    root: Node,
}

impl Trie {
    /// Inserts a sequence, returning the previous action when replaced.
    pub fn insert(&mut self, keys: &[Key], action: &str) -> Option<String> {
        let mut node = &mut self.root;
        for key in keys {
            node = node.children.entry(*key).or_default();
        }
        node.action.replace(action.to_owned())
    }

    /// Removes a sequence, returning its action when present.
    pub fn remove(&mut self, keys: &[Key]) -> Option<String> {
        fn remove(node: &mut Node, keys: &[Key]) -> Option<String> {
            let (first, rest) = keys.split_first()?;
            let child = node.children.get_mut(first)?;
            let removed = if rest.is_empty() { child.action.take() } else { remove(child, rest) };
            if child.action.is_none() && child.children.is_empty() {
                node.children.remove(first);
            }
            removed
        }
        remove(&mut self.root, keys)
    }

    /// Looks up the exact action for a sequence.
    #[must_use]
    pub fn exact(&self, keys: &[Key]) -> Option<&str> {
        let mut node = &self.root;
        for key in keys {
            node = node.children.get(key)?;
        }
        node.action.as_deref()
    }

    /// True when the sequence is a strict prefix of a longer binding.
    #[must_use]
    pub fn has_children(&self, keys: &[Key]) -> bool {
        let mut node = &self.root;
        for key in keys {
            let Some(child) = node.children.get(key) else {
                return false;
            };
            node = child;
        }
        !node.children.is_empty()
    }

    /// The possible continuations of a prefix.
    #[must_use]
    pub fn children(&self, keys: &[Key]) -> Vec<(Key, bool, String)> {
        let mut node = &self.root;
        for key in keys {
            let Some(child) = node.children.get(key) else {
                return Vec::new();
            };
            node = child;
        }
        node.children
            .iter()
            .map(|(key, child)| {
                (*key, !child.children.is_empty(), child.action.clone().unwrap_or_default())
            })
            .collect()
    }

    /// True when the trie has no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.children.is_empty()
    }
}

/// Which-key group labels from `docs/keymap.md` §5.
const DEFAULT_LABELS: &[(&str, &str)] = &[
    ("<leader>f", "+folders"),
    ("<leader>s", "+sort"),
    ("g", "+go"),
    ("z", "+folds and reveal"),
    ("y", "+copy"),
    ("d", "+delete"),
    ("<C-w>", "+window"),
];

/// The effective keymap: a trie per context plus which-key labels.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    /// Bindings per context.
    pub contexts: BTreeMap<String, Trie>,
    /// Flat bindings per context, for listing and editing.
    pub flat: BTreeMap<String, BTreeMap<Vec<Key>, ActionCall>>,
    /// Which-key labels for prefixes (`"<leader>f" = "+folders"`).
    pub labels: BTreeMap<String, String>,
    /// The leader key sequence.
    pub leader: Vec<Key>,
    /// Keyboard enhancement is available.
    pub enhanced: bool,
}

impl Keymap {
    /// Builds the keymap from defaults plus configuration.
    pub fn build(
        config: &crate::config::KeysConfig,
    ) -> Result<(Self, Vec<Diagnostic>), Vec<Diagnostic>> {
        let leader = parse_keys(&config.leader)
            .map_err(|e| vec![Diagnostic::error("keys.leader", format!("invalid leader: {e}"))])?;
        let mut keymap = Self { leader: leader.clone(), enhanced: true, ..Self::default() };
        if config.defaults {
            for (context, notation, action) in default_bindings() {
                let notation = substitute_leader(notation, &leader);
                let Ok(keys) = parse_keys(&notation) else { continue };
                keymap.bind(context, &keys, ActionCall::parse(action, Source::Default));
            }
        }
        let mut errors = Vec::new();
        let label_key = |prefix: &str| -> Option<String> {
            parse_keys(&substitute_leader(prefix, &leader))
                .ok()
                .map(|keys| keys.iter().map(|k| k.notation()).collect())
        };
        for (prefix, label) in DEFAULT_LABELS {
            if let Some(key) = label_key(prefix) {
                keymap.labels.insert(key, (*label).to_owned());
            }
        }
        for (context, bindings) in &config.contexts {
            if context == "labels" {
                for (prefix, label) in bindings {
                    match label_key(prefix) {
                        Some(key) => {
                            keymap.labels.insert(key, label.clone());
                        }
                        None => errors.push(Diagnostic::error(
                            format!("keys.labels.\"{prefix}\""),
                            "invalid key sequence".to_owned(),
                        )),
                    }
                }
                continue;
            }
            if !KNOWN_CONTEXTS.contains(&context.as_str()) {
                errors.push(Diagnostic::error(
                    format!("keys.{context}"),
                    format!("unknown context \"{context}\""),
                ));
                continue;
            }
            for (notation, action_text) in bindings {
                let expanded = substitute_leader(notation, &leader);
                let keys = match parse_keys(&expanded) {
                    Ok(keys) => keys,
                    Err(e) => {
                        errors.push(Diagnostic::error(
                            format!("keys.{context}.\"{notation}\""),
                            e.to_string(),
                        ));
                        continue;
                    }
                };
                let action = ActionCall::parse(action_text, Source::User);
                if action.id == "noop" {
                    keymap.unbind(context, &keys);
                    continue;
                }
                if action.id.starts_with(':') {
                    errors.push(Diagnostic::error(
                        format!("keys.{context}.\"{notation}\""),
                        "command-line bindings are not supported in keys tables".to_owned(),
                    ));
                    continue;
                }
                keymap.bind(context, &keys, action);
            }
        }
        let diagnostics = keymap.validate();
        errors.extend(diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| {
            Diagnostic {
                location: d.location.clone(),
                message: d.message.clone(),
                severity: d.severity,
            }
        }));
        if errors.is_empty() { Ok((keymap, diagnostics)) } else { Err(errors) }
    }

    /// Inserts a binding and updates the flat map.
    pub fn bind(&mut self, context: &str, keys: &[Key], action: ActionCall) {
        self.contexts.entry(context.to_owned()).or_default().insert(keys, &action.as_text());
        self.flat.entry(context.to_owned()).or_default().insert(keys.to_vec(), action);
    }

    /// Removes a binding.
    pub fn unbind(&mut self, context: &str, keys: &[Key]) -> Option<ActionCall> {
        if let Some(trie) = self.contexts.get_mut(context) {
            trie.remove(keys);
        }
        self.flat.get_mut(context).and_then(|m| m.remove(keys))
    }

    /// The action bound exactly to a sequence in a context.
    #[must_use]
    pub fn action(&self, context: &str, keys: &[Key]) -> Option<&ActionCall> {
        self.flat.get(context).and_then(|m| m.get(keys))
    }

    /// Resolves a sequence against a context chain. The first context with
    /// any match (exact or prefix) decides.
    #[must_use]
    pub fn resolve(&self, chain: &[String], keys: &[Key]) -> Resolution {
        for context in chain {
            let Some(trie) = self.contexts.get(context) else {
                continue;
            };
            if let Some(action) = trie.exact(keys) {
                let source = self
                    .flat
                    .get(context)
                    .and_then(|m| m.get(keys))
                    .map_or(Source::Default, |c| c.source);
                return Resolution::Exact(ActionCall::parse(action, source));
            }
            if trie.has_children(keys) {
                return Resolution::Prefix;
            }
        }
        Resolution::None
    }

    /// True when `keys` is bound exactly and also starts a longer binding, so
    /// the dispatcher has to wait for `timeoutlen`. Like [`Keymap::resolve`],
    /// the first context that knows the keys decides: visual mode's `d` is
    /// not held back by the list's `dd` behind it.
    #[must_use]
    pub fn has_longer(&self, chain: &[String], keys: &[Key]) -> bool {
        for context in chain {
            let Some(trie) = self.contexts.get(context) else { continue };
            if trie.has_children(keys) {
                return true;
            }
            if trie.exact(keys).is_some() {
                return false;
            }
        }
        false
    }

    /// Continuations of a prefix for which-key, from the most specific context.
    #[must_use]
    pub fn continuations(&self, chain: &[String], keys: &[Key]) -> Vec<(Key, bool, String)> {
        for context in chain {
            if let Some(trie) = self.contexts.get(context) {
                let children = trie.children(keys);
                if !children.is_empty() {
                    return children;
                }
            }
        }
        Vec::new()
    }

    /// Runs all validation checks from `docs/keymap.md` §6.4.
    #[must_use]
    pub fn validate(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for (context, bindings) in &self.flat {
            for (keys, call) in bindings {
                if !crate::app::actions::is_known(&call.id) {
                    let suggestion = crate::app::actions::suggest(&call.id);
                    out.push(Diagnostic::error(
                        format!("keys.{context}.\"{}\"", display_keys(keys)),
                        match suggestion {
                            Some(s) => {
                                format!("unknown action \"{}\" (did you mean \"{s}\"?)", call.id)
                            }
                            None => format!("unknown action \"{}\"", call.id),
                        },
                    ));
                } else if !crate::app::actions::available_in(&call.id, context) {
                    out.push(Diagnostic::error(
                        format!("keys.{context}.\"{}\"", display_keys(keys)),
                        format!("\"{}\" is not available in {context}", call.id),
                    ));
                }
                if let Some(err) = crate::app::actions::check_args(&call.id, &call.args) {
                    out.push(Diagnostic::error(
                        format!("keys.{context}.\"{}\"", display_keys(keys)),
                        err,
                    ));
                }
                if TEXT_CONTEXTS.contains(&context.as_str())
                    && keys.len() == 1
                    && keys[0].is_printable()
                {
                    out.push(Diagnostic::error(
                        format!("keys.{context}.\"{}\"", display_keys(keys)),
                        format!("would make \"{}\" impossible to type", keys[0].notation()),
                    ));
                }
            }
            // A binding that is also a prefix of a longer one.
            for keys in bindings.keys() {
                if bindings
                    .keys()
                    .any(|other| other.len() > keys.len() && other.starts_with(keys.as_slice()))
                {
                    out.push(Diagnostic::warning(
                        format!("keys.{context}"),
                        format!(
                            "\"{}\" waits {} ms because longer bindings start with it",
                            display_keys(keys),
                            1000
                        ),
                    ));
                }
            }
            // Shadowing a global binding.
            if context != "global" {
                for (keys, call) in bindings {
                    if let Some(global) = self.action("global", keys)
                        && global.id != call.id
                    {
                        out.push(Diagnostic::info(
                            format!("keys.{context}.\"{}\"", display_keys(keys)),
                            format!("shadows global \"{}\"", global.id),
                        ));
                    }
                }
            }
            if MODAL_CONTEXTS.contains(&context.as_str()) {
                let has_cancel = bindings.values().any(|c| c.id == "app.cancel");
                if !has_cancel {
                    out.push(Diagnostic::error(
                        format!("keys.{context}"),
                        "nothing is bound to app.cancel".to_owned(),
                    ));
                }
            }
        }
        if !self.enhanced {
            out.push(Diagnostic::warning(
                "keys",
                "\"<C-i>\" and \"<Tab>\" are the same key without keyboard enhancement".to_owned(),
            ));
        }
        out
    }

    /// Every binding in a context, sorted by action description.
    #[must_use]
    pub fn bindings_in(&self, context: &str) -> Vec<(Vec<Key>, ActionCall)> {
        self.flat
            .get(context)
            .map(|m| m.iter().map(|(k, a)| (k.clone(), a.clone())).collect())
            .unwrap_or_default()
    }
}

/// Replaces `<leader>` in a notation string with the leader notation.
fn substitute_leader(notation: &str, leader: &[Key]) -> String {
    notation.replace("<leader>", &leader.iter().map(|k| k.notation()).collect::<String>())
}

/// Outcome of resolving a key sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A complete binding.
    Exact(ActionCall),
    /// A prefix of at least one binding.
    Prefix,
    /// Nothing matches.
    None,
}

/// Severity of a keymap diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Blocks startup.
    Error,
    /// Printed in `:messages`.
    Warning,
    /// Informational.
    Info,
}

/// A keymap validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Where the problem is.
    pub location: String,
    /// What is wrong.
    pub message: String,
    /// How serious it is.
    pub severity: Severity,
}

impl Diagnostic {
    fn error(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self { location: location.into(), message: message.into(), severity: Severity::Error }
    }

    fn warning(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self { location: location.into(), message: message.into(), severity: Severity::Warning }
    }

    fn info(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self { location: location.into(), message: message.into(), severity: Severity::Info }
    }

    /// The formatted line used in `:messages` and `lazybw keys --check`.
    #[must_use]
    pub fn line(&self) -> String {
        let prefix = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        format!("{prefix}: {}: {}", self.location, self.message)
    }
}

/// The contexts known to the application.
pub const KNOWN_CONTEXTS: &[&str] = &[
    "global",
    "sidebar",
    "list",
    "detail",
    "visual",
    "editor",
    "insert",
    "notes",
    "generator",
    "health",
    "quick_open",
    "cmdline",
    "picker",
    "confirm",
    "unlock",
    "login",
    "settings",
    "keymap_editor",
    "help",
];

/// Contexts where printable keys are text input.
pub const TEXT_CONTEXTS: &[&str] = &["insert", "quick_open", "cmdline", "picker", "unlock"];

/// Contexts that must always have a way out.
pub const MODAL_CONTEXTS: &[&str] =
    &["quick_open", "cmdline", "picker", "confirm", "unlock", "settings", "keymap_editor", "help"];

/// The context chain for a focused area, most specific first.
#[must_use]
pub fn chain(contexts: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = contexts.iter().map(|s| (*s).to_owned()).collect();
    if !out.iter().any(|c| c == "global") {
        out.push("global".to_owned());
    }
    out
}

/// The default bindings from `docs/keymap.md` §4.
#[must_use]
pub fn default_bindings() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut out = Vec::new();
    // global
    for (key, action) in [
        ("q", "app.quit"),
        ("<Esc>", "app.cancel"),
        ("<C-c>", "app.cancel"),
        ("?", "app.help"),
        (":", "cmdline.open"),
        ("<C-p>", "quick_open"),
        ("<C-l>", "app.redraw"),
        ("h", "focus.left"),
        ("<C-w>h", "focus.left"),
        ("<Left>", "focus.left"),
        ("l", "focus.right"),
        ("<C-w>l", "focus.right"),
        ("<Right>", "focus.right"),
        ("<Tab>", "focus.next"),
        ("<S-Tab>", "focus.prev"),
        ("gt", "tab.next"),
        ("gT", "tab.prev"),
        ("<leader>1", "tab.goto 1"),
        ("<leader>2", "tab.goto 2"),
        ("<leader>3", "tab.goto 3"),
        ("/", "search.start"),
        ("n", "search.next"),
        ("N", "search.prev"),
        ("R", "vault.sync"),
        ("<C-o>", "jump.back"),
        ("<C-i>", "jump.forward"),
        ("<leader>/", "quick_open"),
        ("<leader>n", "item.new"),
        ("<leader>g", "generator.open"),
        ("<leader>h", "health.open"),
        ("<leader>e", "vault.export"),
        ("<leader>i", "vault.import"),
        ("<leader>S", "vault.sync force"),
        ("<leader>l", "vault.lock"),
        ("<leader>t", "theme.pick"),
        ("<leader>b", "layout.toggle_sidebar"),
        ("<leader>d", "list.toggle_density"),
        ("<leader>m", "app.messages"),
        ("<leader>,", "app.settings"),
        ("<leader>k", "app.keymap"),
        ("<leader>q", "app.quit"),
        ("<leader>fn", "folder.new"),
        ("<leader>fr", "folder.rename"),
        ("<leader>fd", "folder.delete"),
        ("<leader>ff", "folder.jump"),
        ("<leader>sn", "sort.set name"),
        ("<leader>sm", "sort.set modified"),
        ("<leader>sc", "sort.set created"),
        ("<leader>st", "sort.set type"),
        ("<leader>sr", "sort.reverse"),
    ] {
        out.push(("global", key, action));
    }
    // movement, shared
    let movement: &[(&str, &str)] = &[
        ("j", "nav.down"),
        ("<Down>", "nav.down"),
        ("k", "nav.up"),
        ("<Up>", "nav.up"),
        ("gg", "nav.top"),
        ("G", "nav.bottom"),
        ("<C-d>", "nav.half_down"),
        ("<C-u>", "nav.half_up"),
        ("<C-f>", "nav.page_down"),
        ("<PageDown>", "nav.page_down"),
        ("<C-b>", "nav.page_up"),
        ("<PageUp>", "nav.page_up"),
        ("zz", "view.center"),
        ("zt", "view.top"),
        ("zb", "view.bottom"),
    ];
    for context in [
        "sidebar",
        "list",
        "detail",
        "visual",
        "editor",
        "generator",
        "health",
        "settings",
        "keymap_editor",
        "help",
    ] {
        for (key, action) in movement {
            out.push((context, key, action));
        }
    }
    for context in ["sidebar", "list", "visual", "settings", "keymap_editor", "help"] {
        for (key, action) in
            [("H", "nav.screen_top"), ("M", "nav.screen_middle"), ("L", "nav.screen_bottom")]
        {
            out.push((context, key, action));
        }
    }
    // sidebar
    for (key, action) in [
        ("<CR>", "filter.apply"),
        ("l", "fold.expand_or_apply"),
        ("<Right>", "fold.expand_or_apply"),
        ("h", "fold.collapse_or_parent"),
        ("<Left>", "fold.collapse_or_parent"),
        ("za", "fold.toggle"),
        ("zo", "fold.open"),
        ("zc", "fold.close"),
        ("zR", "fold.open_all"),
        ("zM", "fold.close_all"),
        ("a", "folder.new"),
        ("r", "folder.rename"),
        ("dd", "folder.delete"),
        ("u", "undo"),
        ("<C-r>", "redo"),
    ] {
        out.push(("sidebar", key, action));
    }
    // list
    for (key, action) in [
        ("<CR>", "item.open"),
        ("l", "item.open"),
        ("yy", "item.copy primary"),
        ("yu", "item.copy username"),
        ("yp", "item.copy password"),
        ("yt", "item.copy totp"),
        ("yU", "item.copy uri"),
        ("yn", "item.copy notes"),
        ("a", "item.new"),
        ("e", "item.edit"),
        ("i", "item.edit"),
        ("gE", "item.edit_json"),
        ("dd", "item.trash"),
        ("D", "item.delete"),
        ("r", "item.restore"),
        ("X", "trash.empty"),
        ("u", "undo"),
        ("<C-r>", "redo"),
        ("*", "item.favorite"),
        ("m", "item.move"),
        ("Y", "item.yank"),
        ("p", "item.paste_clone"),
        ("gx", "item.open_uri"),
        ("gh", "item.history"),
        ("gp", "item.generate_password"),
        ("gf", "item.goto_folder"),
        ("K", "item.info"),
        ("za", "item.reveal_toggle"),
        ("zR", "item.reveal_all"),
        ("zM", "item.hide_all"),
        ("V", "visual.start"),
        ("x", "mark.toggle"),
    ] {
        out.push(("list", key, action));
    }
    // detail
    for (key, action) in [
        ("yy", "item.copy_field"),
        ("yu", "item.copy username"),
        ("yp", "item.copy password"),
        ("yt", "item.copy totp"),
        ("yU", "item.copy uri"),
        ("yn", "item.copy notes"),
        ("za", "item.reveal_toggle"),
        ("zR", "item.reveal_all"),
        ("zM", "item.hide_all"),
        ("i", "item.edit_field"),
        ("cc", "item.change_field"),
        ("e", "item.edit"),
        ("dd", "item.remove_passkey"),
        ("gx", "item.open_uri"),
        ("gh", "item.history"),
        ("gp", "item.generate_password"),
        ("gf", "item.goto_folder"),
        ("K", "item.info"),
        ("*", "item.favorite"),
        ("m", "item.move"),
        ("u", "undo"),
        ("<C-r>", "redo"),
    ] {
        out.push(("detail", key, action));
    }
    // visual
    for (key, action) in [
        ("x", "mark.toggle"),
        ("d", "item.trash"),
        ("D", "item.delete"),
        ("r", "item.restore"),
        ("m", "item.move"),
        ("*", "item.favorite"),
        ("<Esc>", "visual.exit"),
        ("V", "visual.exit"),
    ] {
        out.push(("visual", key, action));
    }
    // editor
    for (key, action) in [
        ("j", "editor.next_field"),
        ("<Down>", "editor.next_field"),
        ("<Tab>", "editor.next_field"),
        ("k", "editor.prev_field"),
        ("<Up>", "editor.prev_field"),
        ("<S-Tab>", "editor.prev_field"),
        ("gg", "editor.first_field"),
        ("G", "editor.last_field"),
        ("i", "editor.insert"),
        ("a", "editor.append"),
        ("I", "editor.insert_start"),
        ("A", "editor.append_end"),
        ("cc", "editor.change_field"),
        ("S", "editor.change_field"),
        ("o", "editor.row_add_below"),
        ("O", "editor.row_add_above"),
        ("dd", "editor.row_delete"),
        ("<C-a>", "editor.cycle_next"),
        ("<C-x>", "editor.cycle_prev"),
        ("<CR>", "editor.activate"),
        ("za", "item.reveal_toggle"),
        ("<C-g>", "editor.generate"),
        ("<C-e>", "editor.external"),
        ("gE", "item.edit_json"),
        ("u", "editor.undo"),
        ("<C-r>", "editor.redo"),
        ("<C-s>", "editor.save"),
        ("ZZ", "editor.save_close"),
        ("ZQ", "editor.discard"),
        ("q", "editor.close"),
        ("h", "editor.column_left"),
        ("l", "editor.column_right"),
    ] {
        out.push(("editor", key, action));
    }
    // insert
    for (key, action) in [
        ("<Esc>", "editor.normal"),
        ("<C-[>", "editor.normal"),
        ("<C-c>", "editor.normal"),
        ("<Tab>", "editor.next_field"),
        ("<CR>", "editor.next_field"),
        ("<S-Tab>", "editor.prev_field"),
        ("<C-a>", "editor.cycle_next"),
        ("<C-x>", "editor.cycle_prev"),
        ("<C-g>", "editor.generate"),
        ("<C-e>", "editor.external"),
        ("<C-s>", "editor.save"),
    ] {
        out.push(("insert", key, action));
    }
    // notes
    for (key, action) in [
        ("<Tab>", "editor.next_field"),
        ("<S-Tab>", "editor.prev_field"),
        ("<C-e>", "editor.external"),
        ("<C-s>", "editor.save"),
        (":", "cmdline.open"),
    ] {
        out.push(("notes", key, action));
    }
    // generator
    for (key, action) in [
        ("r", "generator.regenerate"),
        ("yy", "generator.copy"),
        ("<Tab>", "generator.toggle_mode"),
        ("l", "generator.increase"),
        ("<C-a>", "generator.increase"),
        ("+", "generator.increase"),
        ("h", "generator.decrease"),
        ("<C-x>", "generator.decrease"),
        ("-", "generator.decrease"),
        ("x", "generator.toggle_option"),
        ("<CR>", "generator.use"),
    ] {
        out.push(("generator", key, action));
    }
    // health
    for (key, action) in [
        ("<Tab>", "health.next_category"),
        ("<S-Tab>", "health.prev_category"),
        ("<CR>", "health.open_item"),
        ("e", "item.edit"),
        ("gp", "item.generate_password"),
        ("r", "health.rerun"),
    ] {
        out.push(("health", key, action));
    }
    // quick_open
    for (key, action) in [
        ("<C-n>", "quick_open.next"),
        ("<C-j>", "quick_open.next"),
        ("<Down>", "quick_open.next"),
        ("<C-p>", "quick_open.prev"),
        ("<C-k>", "quick_open.prev"),
        ("<Up>", "quick_open.prev"),
        ("<CR>", "quick_open.accept"),
        ("<C-y>", "quick_open.copy_password"),
        ("<C-u>", "quick_open.copy_username"),
        ("<C-t>", "quick_open.copy_totp"),
        ("<C-e>", "quick_open.edit"),
        ("<Esc>", "app.cancel"),
        ("<C-c>", "app.cancel"),
    ] {
        out.push(("quick_open", key, action));
    }
    // cmdline
    for (key, action) in [
        ("<CR>", "cmdline.execute"),
        ("<Tab>", "cmdline.complete_next"),
        ("<S-Tab>", "cmdline.complete_prev"),
        ("<Up>", "cmdline.history_prev"),
        ("<C-p>", "cmdline.history_prev"),
        ("<Down>", "cmdline.history_next"),
        ("<C-n>", "cmdline.history_next"),
        ("<Esc>", "app.cancel"),
        ("<C-c>", "app.cancel"),
    ] {
        out.push(("cmdline", key, action));
    }
    // picker
    for (key, action) in [
        ("<C-n>", "picker.next"),
        ("<Down>", "picker.next"),
        ("<C-p>", "picker.prev"),
        ("<Up>", "picker.prev"),
        ("<CR>", "picker.accept"),
        ("<Esc>", "app.cancel"),
        ("<C-c>", "app.cancel"),
    ] {
        out.push(("picker", key, action));
    }
    // confirm
    for (key, action) in [
        ("y", "confirm.yes"),
        ("<CR>", "confirm.yes"),
        ("n", "confirm.no"),
        ("<Esc>", "app.cancel"),
        ("<C-c>", "app.cancel"),
    ] {
        out.push(("confirm", key, action));
    }
    // unlock
    for (key, action) in [
        ("<CR>", "unlock.submit"),
        ("<C-o>", "unlock.logout"),
        ("<C-c>", "app.quit"),
        ("<Esc>", "app.cancel"),
    ] {
        out.push(("unlock", key, action));
    }
    // settings
    for (key, action) in [
        ("<CR>", "settings.activate"),
        ("l", "settings.next_value"),
        ("<C-a>", "settings.next_value"),
        ("h", "settings.prev_value"),
        ("<C-x>", "settings.prev_value"),
        ("r", "settings.reset"),
        ("<C-n>", "nav.down"),
        ("<C-p>", "nav.up"),
        ("q", "app.cancel"),
        ("<Esc>", "app.cancel"),
    ] {
        out.push(("settings", key, action));
    }
    // keymap editor
    for (key, action) in [
        ("/", "keymap.filter"),
        ("<CR>", "keymap.rebind"),
        ("c", "keymap.rebind"),
        ("a", "keymap.add"),
        ("dd", "keymap.remove"),
        ("r", "keymap.reset"),
        ("R", "keymap.reset_all"),
        ("<Tab>", "keymap.next_context"),
        ("<S-Tab>", "keymap.prev_context"),
        ("<C-s>", "keymap.save"),
        ("<C-n>", "nav.down"),
        ("<C-p>", "nav.up"),
        ("q", "app.cancel"),
        ("<Esc>", "app.cancel"),
    ] {
        out.push(("keymap_editor", key, action));
    }
    // help
    for (key, action) in [
        ("/", "help.search"),
        ("<C-n>", "nav.down"),
        ("<C-p>", "nav.up"),
        ("q", "app.cancel"),
        ("<Esc>", "app.cancel"),
    ] {
        out.push(("help", key, action));
    }
    out.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    out
}

/// The displayed label of a binding for help, which-key and the keymap
/// editor: arguments complete a "…" description ("Copy …" + `uri` →
/// "Copy uri") or follow it in parentheses.
#[must_use]
pub fn action_description(call: &ActionCall) -> String {
    let description = crate::app::actions::description(&call.id);
    if call.args.is_empty() {
        return description.to_owned();
    }
    let args = call.args.join(" ");
    match description.strip_suffix('…') {
        Some(stem) => format!("{}{args}", stem),
        None => format!("{description} ({args})"),
    }
}

impl FromStr for Key {
    type Err = NotationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let keys = parse_keys(s)?;
        if keys.len() == 1 {
            Ok(keys[0])
        } else {
            Err(NotationError { column: 1, message: "expected a single key".to_owned() })
        }
    }
}

/// Parses a leader that may itself be a sequence, expanding `<leader>` in
/// default bindings.
#[must_use]
pub fn with_leader(
    mut bindings: Vec<(&'static str, &'static str, &'static str)>,
    leader: &str,
) -> Vec<(String, String, String)> {
    bindings
        .drain(..)
        .map(|(c, k, a)| {
            (
                c.to_owned(),
                substitute_leader(k, &parse_keys(leader).unwrap_or_else(|_| vec![Key::space()])),
                a.to_owned(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_default_binding_parses() {
        // A default that does not parse is dropped without a word: `<C-f>`
        // read as a function key without a number, and Ctrl-f did nothing.
        for (context, keys, action) in default_bindings() {
            assert!(parse_keys(keys).is_ok(), "{context}: {keys} → {action} does not parse");
        }
        let f = parse_keys("<C-f>").unwrap();
        assert_eq!(f, vec![Key { code: Code::Char('f'), mods: Mods::CTRL }]);
        assert_eq!(parse_keys("<F5>").unwrap()[0].code, Code::F(5));
    }

    use super::*;

    fn keys(s: &str) -> Vec<Key> {
        parse_keys(s).unwrap()
    }

    #[test]
    fn parses_printable_sequences() {
        assert_eq!(keys("gg"), vec![Key::ch('g'), Key::ch('g')]);
        assert_eq!(keys("G"), vec![Key::ch('G')]);
        assert_eq!(keys("*"), vec![Key::ch('*')]);
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(keys("<CR>"), vec![Key::named(Code::Enter)]);
        assert_eq!(keys("<Enter>"), vec![Key::named(Code::Enter)]);
        assert_eq!(keys("<Return>"), vec![Key::named(Code::Enter)]);
        assert_eq!(keys("<Esc>"), vec![Key::named(Code::Esc)]);
        assert_eq!(keys("<S-Tab>"), vec![Key::named(Code::BackTab)]);
        assert_eq!(keys("<Space>"), vec![Key::ch(' ')]);
        assert_eq!(keys("<lt>"), vec![Key::ch('<')]);
        assert_eq!(keys("<Bar>"), vec![Key::ch('|')]);
        assert_eq!(keys("<F12>"), vec![Key::named(Code::F(12))]);
    }

    #[test]
    fn parses_modifiers_and_equivalences() {
        assert_eq!(keys("<C-d>"), vec![Key::ch('d').with(Mods::CTRL)]);
        assert_eq!(keys("<C-D>"), vec![Key::ch('d').with(Mods::CTRL)]);
        assert_eq!(keys("<A-x>"), keys("<M-x>"));
        assert_eq!(keys("<S-a>"), vec![Key::ch('A')]);
        assert_eq!(keys("<C-S-x>"), vec![Key::ch('x').with(Mods::CTRL).with(Mods::SHIFT)]);
        assert_eq!(keys("<C-I>"), vec![Key::ch('i').with(Mods::CTRL)]);
        assert_eq!(keys("<C-S-Tab>"), vec![Key::named(Code::BackTab).with(Mods::CTRL)]);
    }

    #[test]
    fn rejects_invalid_notation() {
        assert!(parse_keys("<C-").is_err());
        assert!(parse_keys("<>").is_err());
        assert!(parse_keys("<Bogus>").is_err());
        assert!(parse_keys("<C-Unknown>").is_err());
        assert!(parse_keys("").is_err());
        assert_eq!(parse_keys("<C-").unwrap_err().column, 1);
    }

    #[test]
    fn trie_insert_lookup_prefix_remove() {
        let mut trie = Trie::default();
        assert!(trie.insert(&keys("gg"), "nav.top").is_none());
        assert!(trie.insert(&keys("gg"), "nav.bottom").as_deref() == Some("nav.top"));
        assert_eq!(trie.exact(&keys("gg")), Some("nav.bottom"));
        assert!(trie.has_children(&keys("g")));
        assert!(!trie.has_children(&keys("gx")));
        assert_eq!(trie.children(&keys("g")).len(), 1);
        assert_eq!(trie.remove(&keys("gg")).as_deref(), Some("nav.bottom"));
        assert!(trie.is_empty());
    }

    #[test]
    fn display_formats() {
        assert_eq!(display_keys(&keys("gg")), "gg");
        assert_eq!(display_keys(&keys("<Space>fn")), "Space f n");
        assert_eq!(display_keys(&keys("<C-w>h")), "Ctrl-w h");
        assert_eq!(display_keys(&keys("<S-Tab>")), "Shift-Tab");
        assert_eq!(display_keys(&keys("<CR>")), "Enter");
        assert_eq!(hint(keys("<C-d>")[0]), "Ctrl-d");
    }

    #[test]
    fn resolution_prefers_specific_context() {
        let mut keymap = Keymap::default();
        keymap.bind("global", &keys("x"), ActionCall::parse("app.quit", Source::Default));
        keymap.bind("list", &keys("x"), ActionCall::parse("mark.toggle", Source::Default));
        assert_eq!(
            keymap.resolve(&chain(&["list"]), &keys("x")),
            Resolution::Exact(ActionCall::parse("mark.toggle", Source::Default))
        );
        assert_eq!(
            keymap.resolve(&chain(&["detail"]), &keys("x")),
            Resolution::Exact(ActionCall::parse("app.quit", Source::Default))
        );
    }

    #[test]
    fn default_keymap_is_valid() {
        let (keymap, diagnostics) = Keymap::build(&crate::config::KeysConfig::default()).unwrap();
        let errors: Vec<_> = diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{errors:#?}");
        // The required behaviours from keymap.md §6.4.
        assert_eq!(
            keymap.resolve(&chain(&["list"]), &keys("dd")),
            Resolution::Exact(ActionCall::parse("item.trash", Source::Default))
        );
        assert_eq!(keymap.resolve(&chain(&["list"]), &keys("g")), Resolution::Prefix);
        assert_eq!(keymap.action("global", &keys("<Space>fn")).unwrap().id, "folder.new");
    }

    #[test]
    fn user_overrides_replace_and_unbind() {
        let text = r#"
[keys]
defaults = true
[keys.list]
"dd" = "noop"
"<Del>" = "item.trash"
"x" = "item.copy uri"
"#;
        let (config, _) = crate::config::Config::from_toml(text).unwrap();
        let (keymap, _) = Keymap::build(&config.keys).unwrap();
        assert!(keymap.action("list", &keys("dd")).is_none());
        assert_eq!(keymap.action("list", &keys("<Del>")).unwrap().id, "item.trash");
        assert_eq!(keymap.action("list", &keys("x")).unwrap().id, "item.copy");
        assert_eq!(keymap.action("list", &keys("x")).unwrap().args, vec!["uri"]);
    }

    #[test]
    fn defaults_false_starts_empty() {
        let config = crate::config::KeysConfig { defaults: false, ..Default::default() };
        let (keymap, _) = Keymap::build(&config).unwrap();
        assert!(keymap.action("list", &keys("dd")).is_none());
    }

    #[test]
    fn leader_change_reexpands() {
        let config = crate::config::KeysConfig { leader: ",".to_owned(), ..Default::default() };
        let (keymap, _) = Keymap::build(&config).unwrap();
        assert_eq!(keymap.action("global", &keys(",fn")).unwrap().id, "folder.new");
        assert!(keymap.action("global", &keys("<Space>fn")).is_none());
    }

    #[test]
    fn validation_rows() {
        let text = r#"
[keys.list]
"yl" = "item.cpy"
[keys.cmdline]
"j" = "cmdline.execute"
[keys.sidebar]
"zq" = "item.trash"
"#;
        let (config, _) = crate::config::Config::from_toml(text).unwrap();
        let err = Keymap::build(&config.keys).unwrap_err();
        let lines: Vec<String> = err.iter().map(|d| d.line()).collect();
        assert!(lines.iter().any(|l| l.contains("unknown action \"item.cpy\"")), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("is not available in sidebar")), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("impossible to type")), "{lines:#?}");

        // A modal context with no way out.
        let text = "[keys]\ndefaults = false\n[keys.picker]\n\"<CR>\" = \"picker.accept\"\n";
        let (config, _) = crate::config::Config::from_toml(text).unwrap();
        let err = Keymap::build(&config.keys).unwrap_err();
        assert!(err.iter().any(|d| d.message.contains("app.cancel")));

        // A sequence that is both a binding and a prefix is a warning.
        let mut keymap = Keymap::default();
        keymap.bind("list", &keys("g"), ActionCall::parse("nav.top", Source::User));
        keymap.bind("list", &keys("gg"), ActionCall::parse("nav.top", Source::Default));
        assert!(
            keymap
                .validate()
                .iter()
                .any(|d| d.severity == Severity::Warning && d.message.contains("waits"))
        );
    }

    #[test]
    fn ambiguous_prefix_is_a_warning() {
        let mut keymap = Keymap::default();
        keymap.bind("list", &keys("g"), ActionCall::parse("nav.top", Source::User));
        keymap.bind("list", &keys("gg"), ActionCall::parse("nav.top", Source::Default));
        let diagnostics = keymap.validate();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.severity == Severity::Warning && d.message.contains("waits"))
        );
    }

    #[test]
    fn shadows_global_is_info() {
        let mut keymap = Keymap::default();
        keymap.bind("global", &keys("R"), ActionCall::parse("vault.sync", Source::Default));
        keymap.bind("list", &keys("R"), ActionCall::parse("item.restore", Source::Default));
        let diagnostics = keymap.validate();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.severity == Severity::Info && d.message.contains("shadows global"))
        );
    }
}
