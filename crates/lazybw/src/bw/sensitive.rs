//! A `String` wrapper for decrypted secrets.
//!
//! Values are zeroized on drop, `Debug` is redacted and there is no
//! `Display` implementation, so a secret can only leave the process through
//! the explicit [`Sensitive::expose`] accessor. Serialisation is transparent
//! because the wire format of the `bw` CLI is plain JSON.

use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A secret string that never prints itself.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Sensitive(String);

impl Sensitive {
    /// Wraps a secret.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Returns a mutable reference to the secret, e.g. to edit a field in place.
    pub fn expose_mut(&mut self) -> &mut String {
        &mut self.0
    }

    /// True when the secret is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of bytes in the secret.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<String> for Sensitive {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Sensitive {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for Sensitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sensitive(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Sensitive::new("hunter2");
        assert_eq!(format!("{s:?}"), "Sensitive(***)");
    }

    #[test]
    fn serde_is_transparent() {
        let s = Sensitive::new("abc");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"abc\"");
        let back: Sensitive = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(back.expose(), "abc");
    }

    #[test]
    fn exposes_value() {
        let mut s = Sensitive::new("abc");
        s.expose_mut().push('d');
        assert_eq!(s.expose(), "abcd");
        assert_eq!(s.len(), 4);
        assert!(!s.is_empty());
    }
}
