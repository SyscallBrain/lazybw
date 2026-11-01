//! Conversion between crossterm key events and the terminal-independent
//! [`Key`] used by the keymap.

use crate::config::keymap::{Code, Key, Mods};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Converts a crossterm event. Returns `None` for key releases.
#[must_use]
pub fn from_event(event: KeyEvent) -> Option<Key> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let code = match event.code {
        KeyCode::Char(c) => Code::Char(c),
        KeyCode::Enter => Code::Enter,
        KeyCode::Esc => Code::Esc,
        KeyCode::Tab => Code::Tab,
        KeyCode::BackTab => Code::BackTab,
        KeyCode::Backspace => Code::Backspace,
        KeyCode::Delete => Code::Delete,
        KeyCode::Insert => Code::Insert,
        KeyCode::Home => Code::Home,
        KeyCode::End => Code::End,
        KeyCode::PageUp => Code::PageUp,
        KeyCode::PageDown => Code::PageDown,
        KeyCode::Up => Code::Up,
        KeyCode::Down => Code::Down,
        KeyCode::Left => Code::Left,
        KeyCode::Right => Code::Right,
        KeyCode::F(n) => Code::F(n),
        KeyCode::Null => return None,
        _ => return None,
    };
    let mut mods = Mods(0);
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        mods = mods.with(Mods::CTRL);
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        mods = mods.with(Mods::ALT);
    }
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        mods = mods.with(Mods::SHIFT);
    }
    let mut key = Key { code, mods };
    // crossterm reports Shift for uppercase letters; canonicalise so `G`
    // matches a plain `G` binding.
    if let Code::Char(c) = key.code
        && c.is_uppercase()
        && !key.mods.contains(Mods::CTRL)
    {
        key.mods = Mods(key.mods.0 & !Mods::SHIFT.0);
    }
    Some(key)
}

/// Converts a canonical key back to crossterm, for tests and injected input.
#[must_use]
pub fn to_event(key: Key) -> KeyEvent {
    let code = match key.code {
        Code::Char(c) => KeyCode::Char(c),
        Code::Enter => KeyCode::Enter,
        Code::Esc => KeyCode::Esc,
        Code::Tab => KeyCode::Tab,
        Code::BackTab => KeyCode::BackTab,
        Code::Backspace => KeyCode::Backspace,
        Code::Delete => KeyCode::Delete,
        Code::Insert => KeyCode::Insert,
        Code::Home => KeyCode::Home,
        Code::End => KeyCode::End,
        Code::PageUp => KeyCode::PageUp,
        Code::PageDown => KeyCode::PageDown,
        Code::Up => KeyCode::Up,
        Code::Down => KeyCode::Down,
        Code::Left => KeyCode::Left,
        Code::Right => KeyCode::Right,
        Code::F(n) => KeyCode::F(n),
    };
    let mut modifiers = KeyModifiers::NONE;
    if key.mods.contains(Mods::CTRL) {
        modifiers |= KeyModifiers::CONTROL;
    }
    if key.mods.contains(Mods::ALT) {
        modifiers |= KeyModifiers::ALT;
    }
    if key.mods.contains(Mods::SHIFT) {
        modifiers |= KeyModifiers::SHIFT;
    }
    KeyEvent::new(code, modifiers)
}

/// A `KeyEvent` from a Vim-notation string like `<C-d>`.
#[must_use]
pub fn event_from_notation(notation: &str) -> Option<KeyEvent> {
    crate::config::keymap::parse_keys(notation)
        .ok()
        .and_then(|keys| keys.first().copied())
        .map(to_event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keymap::parse_keys;

    #[test]
    fn round_trips_named_keys() {
        for notation in ["a", "G", "<C-d>", "<S-Tab>", "<CR>", "<Esc>", "<F5>", "<Space>"] {
            let key = parse_keys(notation).unwrap()[0];
            let event = to_event(key);
            assert_eq!(from_event(event), Some(key), "{notation}");
        }
    }

    #[test]
    fn uppercase_shift_is_removed() {
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        let key = from_event(event).unwrap();
        assert_eq!(key, Key::ch('G'));
    }

    #[test]
    fn control_shift_is_kept() {
        let event = KeyEvent::new(KeyCode::Char('X'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let key = from_event(event).unwrap();
        assert!(key.mods.contains(Mods::CTRL) && key.mods.contains(Mods::SHIFT));
    }

    #[test]
    fn releases_are_ignored() {
        let mut event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        event.kind = KeyEventKind::Release;
        assert_eq!(from_event(event), None);
    }
}
