//! A single-line editor with the Vim motions and operators used by the
//! command line, search, unlock field, quick open and editor values.

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
