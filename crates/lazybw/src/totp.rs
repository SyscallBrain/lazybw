//! TOTP codes computed locally (RFC 6238) from the authenticator key stored in
//! a login item.
//!
//! The official CLI refuses `bw get totp` without Premium, so lazybw never
//! calls it. Supported inputs: `otpauth://totp/…`, `otpauth://steam/…`, raw
//! Base32 secrets and `steam://` secrets.

use std::fmt;
use totp_rs::{Algorithm, Builder, Totp};

/// A code with its countdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpCode {
    /// The digits.
    pub code: String,
    /// Seconds left in the current period.
    pub remaining: u64,
    /// Period length.
    pub period: u64,
    /// Number of digits.
    pub digits: u8,
    /// Steam codes use a different alphabet.
    pub steam: bool,
}

impl TotpCode {
    /// Fraction of the period left, 0.0–1.0.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.period == 0 {
            return 0.0;
        }
        self.remaining as f32 / self.period as f32
    }

    /// Colour bucket: fresh, expiring (< 10 s) or critical (< 5 s).
    #[must_use]
    pub fn urgency(&self) -> Urgency {
        if self.remaining <= 5 {
            Urgency::Critical
        } else if self.remaining <= 10 {
            Urgency::Expiring
        } else {
            Urgency::Fresh
        }
    }
}

/// The countdown state used to colour the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// More than 10 seconds left.
    Fresh,
    /// 6–10 seconds left.
    Expiring,
    /// 5 seconds or less.
    Critical,
}

/// Why a secret could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TotpError {
    // The String payloads keep the library's detail for `Debug` in tests;
    // `Display`, which reaches the screen, never prints them.
    /// The key was empty.
    Empty,
    /// The Base32 text was invalid.
    InvalidBase32(String),
    /// The `otpauth://` URL was invalid.
    InvalidUrl(String),
    /// The TOTP parameters were rejected.
    Invalid(String),
}

impl fmt::Display for TotpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The library's own text can quote the key, which is a secret,
            // so only the kind of problem is shown.
            Self::Empty => f.write_str("Not a valid authenticator key: it is empty"),
            Self::InvalidBase32(_) => {
                f.write_str("Not a valid authenticator key: it is not Base32 text")
            }
            Self::InvalidUrl(_) => {
                f.write_str("Not a valid authenticator key: the otpauth:// link is malformed")
            }
            Self::Invalid(_) => f.write_str(
                "Not a valid authenticator key: it is too short or uses unsupported settings",
            ),
        }
    }
}

impl std::error::Error for TotpError {}

/// Parses a stored authenticator key into a TOTP generator.
pub fn parse(secret: &str) -> Result<Totp, TotpError> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(TotpError::Empty);
    }
    let lower = secret.to_ascii_lowercase();
    if lower.starts_with("otpauth://") {
        return Totp::from_url(secret).map_err(|e| TotpError::InvalidUrl(e.to_string()));
    }
    let (raw, steam) = if let Some(rest) = lower.strip_prefix("steam://") {
        (rest.to_owned(), true)
    } else {
        (secret.to_owned(), false)
    };
    let bytes = decode_base32(&raw).map_err(TotpError::InvalidBase32)?;
    if bytes.len() < 10 {
        return Err(TotpError::Invalid("secret too short".to_owned()));
    }
    let builder = if steam {
        Builder::new_steam().with_secret(bytes)
    } else {
        Builder::new().with_secret(bytes).with_algorithm(Algorithm::SHA1).with_digits(6)
    };
    builder
        .with_account_name(if steam { "steam" } else { "lazybw" })
        .with_step_duration(30)
        .with_skew(1)
        .build()
        .map_err(|e| TotpError::Invalid(e.to_string()))
}

/// Computes the code at `unix_time`.
pub fn code_at(totp: &Totp, unix_time: u64) -> TotpCode {
    let period = totp.step();
    let remaining = period - (unix_time % period);
    let code = totp.generate(unix_time).to_string();
    TotpCode { code, remaining, period, digits: totp.digits(), steam: totp.digits() == 5 }
}

/// Parses and computes in one step.
pub fn current(secret: &str, unix_time: u64) -> Result<TotpCode, TotpError> {
    let totp = parse(secret)?;
    Ok(code_at(&totp, unix_time))
}

/// True when the string can be used as an authenticator key.
#[must_use]
pub fn is_valid(secret: &str) -> bool {
    parse(secret).is_ok()
}

/// Decodes RFC 4648 Base32, ignoring spaces, dashes and padding.
pub fn decode_base32(input: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::new();
    for ch in input.chars() {
        if ch == ' ' || ch == '-' || ch == '=' || ch == '\n' || ch == '\r' {
            continue;
        }
        let upper = ch.to_ascii_uppercase();
        let Some(index) = ALPHABET.iter().position(|&c| char::from(c) == upper) else {
            return Err(format!("invalid Base32 character \"{ch}\""));
        };
        buffer = (buffer << 5) | index as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Encodes bytes as Base32 (used by tests and the generator).
#[must_use]
pub fn encode_base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = String::new();
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 0x1F) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 0x1F) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B: secret "12345678901234567890", SHA-1, 8 digits.
    fn rfc_totp() -> Totp {
        Builder::new()
            .with_secret(Vec::from("12345678901234567890".as_bytes()))
            .with_algorithm(Algorithm::SHA1)
            .with_digits(8)
            .with_step_duration(30)
            .with_skew(0)
            .with_account_name("test")
            .build()
            .unwrap()
    }

    #[test]
    fn rfc6238_vectors() {
        let totp = rfc_totp();
        for (time, expected) in [
            (59u64, "94287082"),
            (1_111_111_109, "07081804"),
            (1_111_111_111, "14050471"),
            (1_234_567_890, "89005924"),
            (2_000_000_000, "69279037"),
            (20_000_000_000, "65353130"),
        ] {
            assert_eq!(totp.generate(time).to_string(), expected, "t={time}");
        }
    }

    #[test]
    fn base32_round_trip() {
        let bytes = b"12345678901234567890";
        let encoded = encode_base32(bytes);
        assert_eq!(encoded, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(decode_base32(&encoded).unwrap(), bytes);
        assert_eq!(decode_base32("gezd gnbv-gy3").unwrap(), b"123456");
    }

    #[test]
    fn parses_raw_base32_and_counts_down() {
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let code = current(secret, 59).unwrap();
        assert_eq!(code.code.len(), 6);
        assert_eq!(code.remaining, 1);
        assert_eq!(code.urgency(), Urgency::Critical);
        assert_eq!(code.digits, 6);
    }

    #[test]
    fn parses_otpauth_url() {
        let url = "otpauth://totp/GitHub:dev?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=6&period=30&algorithm=SHA1";
        let totp = parse(url).unwrap();
        assert_eq!(totp.digits(), 6);
        assert_eq!(totp.step(), 30);
    }

    #[test]
    fn steam_secret_has_five_digits() {
        let totp = parse("steam://KRSXG5CTMVRXEZLUKN2XAZLSKNSWG4TFOQ").unwrap();
        assert_eq!(totp.digits(), 5);
    }

    #[test]
    fn rejects_invalid_keys() {
        assert_eq!(parse("").unwrap_err(), TotpError::Empty);
        assert!(matches!(parse("not base32!"), Err(TotpError::InvalidBase32(_))));
        assert!(!is_valid("abc"));
        assert!(is_valid("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
    }

    #[test]
    fn brackets_and_spaces_in_secret() {
        let code = current("gezd gnbv gy3t qojq gezd gnbv gy3t qojq", 59).unwrap();
        assert_eq!(code.code.len(), 6);
    }
}
