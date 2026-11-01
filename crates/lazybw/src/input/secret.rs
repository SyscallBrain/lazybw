//! A text buffer for passwords typed into the UI.
//!
//! The buffer is pre-allocated so typing does not leave reallocated copies
//! behind, its `Debug` output is redacted, and the bytes are zeroed when it
//! is cleared, taken or dropped.

use secrecy::SecretString;
use zeroize::Zeroize;

/// Capacity reserved up front; long enough for any realistic password.
const RESERVED: usize = 256;

/// A password being typed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretInput(String);

impl SecretInput {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self(String::with_capacity(RESERVED))
    }

    /// Appends a character.
    pub fn push(&mut self, ch: char) {
        if self.0.capacity() == 0 {
            self.0.reserve(RESERVED);
        } else if self.0.len() + ch.len_utf8() > self.0.capacity() {
            // Grow by hand so the old allocation is wiped, not just freed.
            let mut bigger = String::with_capacity(self.0.capacity() * 2);
            bigger.push_str(&self.0);
            self.0.zeroize();
            self.0 = bigger;
        }
        self.0.push(ch);
    }

    /// Appends text (paste).
    pub fn push_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.push(ch);
        }
    }

    /// Removes the last character.
    pub fn pop(&mut self) -> Option<char> {
        let ch = self.0.chars().last()?;
        // Copy the kept prefix into a fresh buffer and wipe the old one
        // (zeroize clears the full capacity), so the removed bytes do not
        // linger in spare capacity.
        let keep = self.0.len() - ch.len_utf8();
        let mut fresh = String::with_capacity(self.0.capacity().max(RESERVED));
        fresh.push_str(&self.0[..keep]);
        self.0.zeroize();
        self.0 = fresh;
        Some(ch)
    }

    /// Clears and wipes the buffer.
    pub fn clear(&mut self) {
        self.0.zeroize();
    }

    /// True when nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of characters (for masks).
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.0.chars().count()
    }

    /// The text, for comparisons and strength meters.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Moves the text into a [`SecretString`] and wipes the buffer.
    pub fn take_secret(&mut self) -> SecretString {
        let secret = SecretString::from(self.0.as_str());
        self.0.zeroize();
        secret
    }

    /// Copies the text into a [`SecretString`], keeping the buffer.
    #[must_use]
    pub fn to_secret(&self) -> SecretString {
        SecretString::from(self.0.as_str())
    }
}

impl std::fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretInput([redacted; {} chars])", self.char_count())
    }
}

impl Drop for SecretInput {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl From<&str> for SecretInput {
    fn from(text: &str) -> Self {
        let mut input = Self::new();
        input.push_str(text);
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn typing_and_taking() {
        let mut input = SecretInput::new();
        input.push_str("hunter2!");
        input.pop();
        assert_eq!(input.expose(), "hunter2");
        assert_eq!(input.char_count(), 7);
        assert!(!format!("{input:?}").contains("hunter"));
        let secret = input.take_secret();
        assert_eq!(secret.expose_secret(), "hunter2");
        assert!(input.is_empty());
    }

    #[test]
    fn grows_past_the_reservation() {
        let mut input = SecretInput::new();
        let long = "x".repeat(RESERVED * 3);
        input.push_str(&long);
        assert_eq!(input.expose(), long);
    }
}
