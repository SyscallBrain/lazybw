//! A single-line editor with the Vim motions and operators used by the
//! command line, search, unlock field, quick open and editor values.

use crate::config::keymap::{Code, Key, Mods};

/// Applies a text-editing key ([`Line::edit_key`]) to a plain string with
/// its cursor, in characters. Returns false when the key is not one.
pub fn edit_string(text: &mut String, cursor: &mut usize, key: Key) -> bool {
    let mut line = Line::new(text);
    line.cursor = (*cursor).min(line.len());
    if !line.edit_key(key) {
        return false;
    }
    *text = line.text();
    *cursor = line.cursor;
    true
}

/// Like [`edit_string`] for a field whose cursor is always at the end
/// (search, paths, filters): typing, `Backspace`, `Ctrl-w`, `Ctrl-u`.
pub fn edit_at_end(text: &mut String, key: Key) -> bool {
    if matches!(key.code, Code::Left | Code::Right | Code::Home | Code::End | Code::Delete) {
        return false;
    }
    let mut cursor = text.chars().count();
    edit_string(text, &mut cursor, key)
}

/// Editable text with a cursor (in characters, not bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    chars: Vec<char>,
    /// Insertion point, 0..=len.
    pub cursor: usize,
    undo: Vec<(Vec<char>, usize)>,
}

impl Line {
    /// Builds a line from text, cursor at the end.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor, undo: Vec::new() }
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Number of characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Replaces the contents, cursor at the end.
    pub fn set(&mut self, text: &str) {
        self.snapshot();
        self.chars = text.chars().collect();
        self.cursor = self.chars.len();
    }

    /// Clears the line.
    pub fn clear(&mut self) {
        self.set("");
    }

    /// The character under the cursor, when in normal mode.
    #[must_use]
    pub fn current(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    /// Moves the cursor to `index`, clamped.
    pub fn move_to(&mut self, index: usize) {
        self.cursor = index.min(self.chars.len());
    }

    /// Inserts a character at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        self.snapshot();
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }

    /// Inserts a string at the cursor.
    pub fn insert_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.snapshot();
        for ch in text.chars() {
            self.chars.insert(self.cursor, ch);
            self.cursor += 1;
        }
    }

    /// Backspace.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.snapshot();
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    /// Delete under the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.snapshot();
            self.chars.remove(self.cursor);
        }
    }

    /// Left.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Right.
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    /// Home / `0`.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// End / `$`.
    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// `w`: start of the next word.
    pub fn word_forward(&mut self) {
        let mut i = self.cursor;
        while i < self.chars.len() && !self.chars[i].is_alphanumeric() {
            i += 1;
        }
        while i < self.chars.len() && self.chars[i].is_alphanumeric() {
            i += 1;
        }
        while i < self.chars.len() && !self.chars[i].is_alphanumeric() {
            i += 1;
        }
        self.cursor = i;
    }

    /// `b`: start of the previous word.
    pub fn word_backward(&mut self) {
        let mut i = self.cursor;
        while i > 0 && !self.chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        while i > 0 && self.chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        self.cursor = i;
    }

    /// `e`: end of the current or next word.
    pub fn word_end(&mut self) {
        let mut i = self.cursor;
        while i < self.chars.len() && !self.chars[i].is_alphanumeric() {
            i += 1;
        }
        while i < self.chars.len() && self.chars[i].is_alphanumeric() {
            i += 1;
        }
        i = i.saturating_sub(1);
        self.cursor = i.min(self.chars.len().saturating_sub(1));
    }

    /// `^`: first non-blank character.
    pub fn first_non_blank(&mut self) {
        self.cursor = self.chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0);
    }

    /// `x`: delete the character under the cursor.
    pub fn delete_char(&mut self) {
        if self.cursor >= self.chars.len() && !self.chars.is_empty() {
            self.cursor = self.chars.len() - 1;
        }
        self.delete();
    }

    /// `dw`: delete to the start of the next word.
    pub fn delete_word_forward(&mut self) {
        let start = self.cursor;
        self.word_forward();
        self.delete_range(start, self.cursor);
        self.cursor = start;
    }

    /// `diw`: delete the word under the cursor.
    pub fn delete_inner_word(&mut self) {
        let mut start = self.cursor;
        let mut end = self.cursor;
        if start >= self.chars.len() {
            return;
        }
        while start > 0 && self.chars[start - 1].is_alphanumeric() {
            start -= 1;
        }
        while end < self.chars.len() && self.chars[end].is_alphanumeric() {
            end += 1;
        }
        self.delete_range(start, end);
        self.cursor = start;
    }

    /// `D`: delete to the end of the line.
    pub fn delete_to_end(&mut self) {
        let start = self.cursor;
        let len = self.chars.len();
        self.delete_range(start, len);
        self.cursor = start.min(self.chars.len());
    }

    /// `C` / `S` / `cc`: clear the line and leave the cursor at the start.
    pub fn change_line(&mut self) {
        self.set("");
        self.cursor = 0;
    }

    /// `u`: undo the last change.
    pub fn undo(&mut self) {
        if let Some((chars, cursor)) = self.undo.pop() {
            self.chars = chars;
            self.cursor = cursor.min(self.chars.len());
        }
    }

    /// True when undo has an entry.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// `Ctrl-w`: delete the word before the cursor.
    pub fn delete_word_backward(&mut self) {
        let end = self.cursor;
        self.word_backward();
        self.delete_range(self.cursor, end);
    }

    /// `Ctrl-u`: delete everything before the cursor.
    pub fn delete_to_start(&mut self) {
        let end = self.cursor;
        self.delete_range(0, end);
        self.cursor = 0;
    }

    /// Applies a text-editing key: the arrows, `Home`, `End`, `Backspace`,
    /// `Delete`, `Ctrl-w`, `Ctrl-u` and printable characters. Returns false
    /// for any other key, which the caller handles.
    pub fn edit_key(&mut self, key: Key) -> bool {
        match (key.code, key.mods == Mods::CTRL) {
            (Code::Backspace, _) | (Code::Char('h'), true) => self.backspace(),
            (Code::Delete, _) => self.delete(),
            (Code::Left, _) => self.left(),
            (Code::Right, _) => self.right(),
            (Code::Home, _) => self.home(),
            (Code::End, _) => self.end(),
            (Code::Char('w'), true) => self.delete_word_backward(),
            (Code::Char('u'), true) => self.delete_to_start(),
            (Code::Char(c), false) if key.is_printable() => self.insert_char(c),
            _ => return false,
        }
        true
    }

    /// Deletes `start..end` characters.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let start = start.min(self.chars.len());
        let end = end.min(self.chars.len());
        if start >= end {
            return;
        }
        self.snapshot();
        self.chars.drain(start..end);
        self.cursor = start.min(self.chars.len());
    }

    fn snapshot(&mut self) {
        self.undo.push((self.chars.clone(), self.cursor));
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::keymap::parse_keys;

    fn key(notation: &str) -> Key {
        parse_keys(notation).unwrap()[0]
    }

    #[test]
    fn readline_keys_edit_a_string_at_its_cursor() {
        let mut text = "move Social media".to_owned();
        let mut cursor = text.chars().count();
        assert!(edit_string(&mut text, &mut cursor, key("<C-w>")));
        assert_eq!((text.as_str(), cursor), ("move Social ", 12));
        assert!(edit_string(&mut text, &mut cursor, key("<Left>")));
        assert!(edit_string(&mut text, &mut cursor, key("<BS>")));
        assert_eq!((text.as_str(), cursor), ("move Socia ", 10));
        assert!(edit_string(&mut text, &mut cursor, key("<C-u>")));
        assert_eq!((text.as_str(), cursor), (" ", 0));
        assert!(!edit_string(&mut text, &mut cursor, key("<CR>")));
        let mut path = "~/backups/vault.json".to_owned();
        assert!(edit_at_end(&mut path, key("<C-w>")));
        assert_eq!(path, "~/backups/vault.");
        assert!(!edit_at_end(&mut path, key("<Left>")));
    }

    use super::*;

    #[test]
    fn inserts_and_deletes() {
        let mut line = Line::new("");
        line.insert_str("hello");
        assert_eq!(line.text(), "hello");
        assert_eq!(line.cursor, 5);
        line.backspace();
        assert_eq!(line.text(), "hell");
        line.home();
        line.delete();
        assert_eq!(line.text(), "ell");
    }

    #[test]
    fn motions() {
        let mut line = Line::new("git commit -m");
        line.home();
        line.word_forward();
        assert_eq!(line.cursor, 4);
        line.word_forward();
        assert_eq!(line.cursor, 12);
        line.word_backward();
        assert_eq!(line.cursor, 4);
        line.end();
        line.word_backward();
        assert_eq!(line.cursor, 12);
        line.first_non_blank();
        assert_eq!(line.cursor, 0);
        line.home();
        line.insert_str("  x");
        line.first_non_blank();
        assert_eq!(line.cursor, 2);
    }

    #[test]
    fn operators() {
        let mut line = Line::new("one two three");
        line.home();
        line.delete_word_forward();
        assert_eq!(line.text(), "two three");
        line.delete_inner_word();
        assert_eq!(line.text(), " three");
        line.end();
        line.insert_str(" four");
        line.home();
        line.delete_to_end();
        assert_eq!(line.text(), "");
    }

    #[test]
    fn undo_restores() {
        let mut line = Line::new("abc");
        line.insert_str("d");
        line.undo();
        assert_eq!(line.text(), "abc");
        line.delete_char();
        assert_eq!(line.text(), "ab");
    }

    #[test]
    fn cursor_is_char_based_for_unicode() {
        let mut line = Line::new("héllo");
        line.home();
        line.right();
        line.insert_char('!');
        assert_eq!(line.text(), "h!éllo");
        line.end();
        line.left();
        line.delete();
        assert_eq!(line.text(), "h!éll");
    }
}
