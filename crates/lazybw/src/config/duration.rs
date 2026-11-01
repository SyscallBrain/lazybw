//! Durations written as human strings: `500ms`, `30s`, `15m`, `2h`, `365d`
//! and the special value `never`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::time::Duration;

/// A configuration duration; `None` means "never" (feature disabled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct DurationSpec(pub Option<Duration>);

impl DurationSpec {
    /// Never.
    pub const NEVER: Self = Self(None);

    /// The duration, if not `never`.
    #[must_use]
    pub fn get(self) -> Option<Duration> {
        self.0
    }

    /// Seconds as `i64`, or `None` for `never`.
    #[must_use]
    pub fn seconds(self) -> Option<i64> {
        self.0.map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
    }

    /// True when the feature is disabled.
    #[must_use]
    pub fn is_never(self) -> bool {
        self.0.is_none()
    }

    /// Parses the textual form: a number and a unit (`ms`, `s`, `m`, `h`,
    /// `d`), or `never`.
    pub fn parse(text: &str) -> Result<Self, DurationError> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("never") {
            return Ok(Self::NEVER);
        }
        let split = text.find(|c: char| !c.is_ascii_digit() && c != '.');
        let (number, unit) = match split {
            Some(i) => (&text[..i], &text[i..]),
            None => (text, "s"),
        };
        let value: f64 =
            number.parse().map_err(|_| DurationError::NotANumber { input: text.to_owned() })?;
        if value < 0.0 || !value.is_finite() {
            return Err(DurationError::Negative { input: text.to_owned() });
        }
        let millis = match unit.trim() {
            "ms" => value,
            "s" | "" => value * 1_000.0,
            "m" => value * 60_000.0,
            "h" => value * 3_600_000.0,
            "d" => value * 86_400_000.0,
            other => return Err(DurationError::Unit { unit: other.to_owned() }),
        };
        // A hundred years is far beyond any timeout lazybw has; anything
        // larger is a typo, not a choice (and `as u64` would saturate it).
        const MAX_MILLIS: f64 = 100.0 * 365.0 * 86_400_000.0;
        if millis > MAX_MILLIS {
            return Err(DurationError::TooLarge { input: text.to_owned() });
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked above: finite, non-negative and at most a hundred years"
        )]
        let millis = millis.round() as u64;
        Ok(Self(Some(Duration::from_millis(millis))))
    }
}

/// Why a duration did not parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DurationError {
    /// No number where one was expected.
    #[error("invalid duration \"{input}\": expected a number and a unit")]
    NotANumber {
        /// The text given.
        input: String,
    },
    /// A negative or non-finite number.
    #[error("invalid duration \"{input}\": must be positive")]
    Negative {
        /// The text given.
        input: String,
    },
    /// More than a hundred years.
    #[error("invalid duration \"{input}\": too large")]
    TooLarge {
        /// The text given.
        input: String,
    },
    /// An unknown unit.
    #[error("invalid duration unit \"{unit}\": use ms, s, m, h or d")]
    Unit {
        /// The unit given.
        unit: String,
    },
}

impl fmt::Display for DurationSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(duration) = self.0 else {
            return f.write_str("never");
        };
        let ms = duration.as_millis();
        if ms == 0 {
            return f.write_str("0s");
        }
        if ms % 86_400_000 == 0 && ms >= 86_400_000 {
            write!(f, "{}d", ms / 86_400_000)
        } else if ms % 3_600_000 == 0 && ms >= 3_600_000 {
            write!(f, "{}h", ms / 3_600_000)
        } else if ms % 60_000 == 0 && ms >= 60_000 {
            write!(f, "{}m", ms / 60_000)
        } else if ms % 1_000 == 0 && ms >= 1_000 {
            write!(f, "{}s", ms / 1_000)
        } else {
            write!(f, "{ms}ms")
        }
    }
}

impl Serialize for DurationSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DurationSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl From<Duration> for DurationSpec {
    fn from(value: Duration) -> Self {
        Self(Some(value))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn absurd_durations_are_rejected_not_saturated() {
        assert!(matches!(DurationSpec::parse("1e30d"), Err(DurationError::Unit { .. })));
        assert!(matches!(DurationSpec::parse("99999999999d"), Err(DurationError::TooLarge { .. })));
        assert_eq!(DurationSpec::parse("365d").unwrap().seconds(), Some(365 * 86_400));
        assert!(matches!(DurationSpec::parse("5y"), Err(DurationError::Unit { .. })));
    }
    use super::*;

    #[test]
    fn parses_all_units() {
        assert_eq!(DurationSpec::parse("500ms").unwrap().0, Some(Duration::from_millis(500)));
        assert_eq!(DurationSpec::parse("30s").unwrap().0, Some(Duration::from_secs(30)));
        assert_eq!(DurationSpec::parse("15m").unwrap().0, Some(Duration::from_secs(900)));
        assert_eq!(DurationSpec::parse("2h").unwrap().0, Some(Duration::from_secs(7200)));
        assert_eq!(DurationSpec::parse("365d").unwrap().0, Some(Duration::from_secs(31_536_000)));
    }

    #[test]
    fn parses_never_and_case() {
        assert!(DurationSpec::parse("never").unwrap().is_never());
        assert!(DurationSpec::parse("NEVER").unwrap().is_never());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(DurationSpec::parse("soon").is_err());
        assert!(DurationSpec::parse("5w").is_err());
        assert!(DurationSpec::parse("-1s").is_err());
    }

    #[test]
    fn displays_canonically() {
        assert_eq!(DurationSpec::parse("500ms").unwrap().to_string(), "500ms");
        assert_eq!(DurationSpec::parse("60s").unwrap().to_string(), "1m");
        assert_eq!(DurationSpec::parse("365d").unwrap().to_string(), "365d");
        assert_eq!(DurationSpec::NEVER.to_string(), "never");
    }

    #[test]
    fn serde_round_trip() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
        struct T {
            d: DurationSpec,
        }
        let t: T = toml::from_str("d = \"15m\"").unwrap();
        assert_eq!(t.d.seconds(), Some(900));
        assert_eq!(toml::to_string(&t).unwrap().trim(), "d = \"15m\"");
    }
}
