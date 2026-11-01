//! The `{success, message, data}` envelope returned by `bw serve` and by
//! `bw <command> --response`, and the mapping from `bw` messages to typed
//! errors.

use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use thiserror::Error;

/// The envelope shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Envelope<T> {
    /// True on success.
    pub success: bool,
    /// The `bw` message on failure.
    #[serde(default)]
    pub message: Option<String>,
    /// The payload.
    #[serde(default)]
    pub data: Option<T>,
}

/// A typed error from the backend.
#[derive(Debug, Error)]
pub enum BwError {
    /// The vault is locked.
    #[error("Vault is locked.")]
    Locked,
    /// No account is logged in.
    #[error("You are not logged in.")]
    NotLoggedIn,
    /// The master password is wrong.
    #[error("Invalid master password.")]
    InvalidPassword,
    /// The object does not exist.
    #[error("Not found.")]
    NotFound,
    /// A Premium-only feature was called (a lazybw bug).
    #[error("Premium status is required to use this feature.")]
    PremiumRequired,
    /// A server change needs a logout first.
    #[error("Logout required before server config update.")]
    LogoutRequired,
    /// A query matched more than one item (a lazybw bug).
    #[error("{0}")]
    Ambiguous(String),
    /// The Host/Origin check failed (a lazybw bug).
    #[error("Bitwarden CLI rejected the request (HTTP 403). This is a bug in lazybw; see the log.")]
    Rejected,
    /// The connection to `bw serve` ended.
    #[error("Lost the connection to the Bitwarden CLI.")]
    Disconnected,
    /// An endpoint timed out.
    #[error("Bitwarden CLI timed out on {endpoint}.")]
    Timeout {
        /// The endpoint that timed out.
        endpoint: String,
    },
    /// Any other message from `bw`, surfaced verbatim.
    #[error("{0}")]
    Other(String),
    /// The response could not be parsed.
    #[error("Could not understand the Bitwarden CLI response: {0}")]
    InvalidResponse(String),
}

impl BwError {
    /// Maps a `bw` message to a typed error, per bw-integration.md §9.
    #[must_use]
    pub fn from_message(message: &str) -> Self {
        let trimmed = message.trim();
        let lower = trimmed.to_ascii_lowercase();
        match lower.as_str() {
            "vault is locked." => Self::Locked,
            "you are not logged in." => Self::NotLoggedIn,
            "invalid master password." => Self::InvalidPassword,
            "not found." => Self::NotFound,
            "premium status is required to use this feature." => Self::PremiumRequired,
            "logout required before server config update." => Self::LogoutRequired,
            other if other.starts_with("more than one result was found") => {
                Self::Ambiguous(trimmed.to_owned())
            }
            _ => Self::Other(trimmed.to_owned()),
        }
    }

    /// True when the vault must be dropped and the Unlock screen shown.
    #[must_use]
    pub fn requires_lock(&self) -> bool {
        matches!(self, Self::Locked)
    }

    /// True when the session is gone and the Login screen must be shown.
    #[must_use]
    pub fn requires_login(&self) -> bool {
        matches!(self, Self::NotLoggedIn)
    }

    /// True for transport failures that justify restarting `bw serve`.
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected)
    }
}

/// Describes a body that did not parse, without any of its content.
///
/// The body is decrypted vault data, and serde's own message can quote the
/// offending value ("invalid type: string \"…\""), so only the error class,
/// its position and the body size go into the error (which reaches the
/// message line and the log).
fn invalid_response(error: &serde_json::Error, body: &[u8]) -> BwError {
    BwError::InvalidResponse(format!(
        "{:?} error at line {} column {} ({} bytes)",
        error.classify(),
        error.line(),
        error.column(),
        body.len()
    ))
}

/// Decodes an envelope body.
///
/// The body is parsed once, into `T`. Only when that fails is the envelope
/// read again without its payload: a failed call can carry a payload of
/// another shape, and the `bw` message is what the user needs to see.
pub fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, BwError> {
    match serde_json::from_slice::<Envelope<T>>(body) {
        Ok(envelope) if envelope.success => {
            envelope.data.ok_or_else(|| BwError::InvalidResponse("success without data".to_owned()))
        }
        Ok(envelope) => Err(failure(envelope.message.as_deref())),
        Err(error) => match serde_json::from_slice::<Envelope<IgnoredAny>>(body) {
            Ok(envelope) if !envelope.success => Err(failure(envelope.message.as_deref())),
            _ => Err(invalid_response(&error, body)),
        },
    }
}

/// Validates a response without requiring a payload (deletes return `null`).
pub fn check(status: u16, body: &[u8]) -> Result<(), BwError> {
    if status == 403 {
        return Err(BwError::Rejected);
    }
    let envelope: Envelope<IgnoredAny> =
        serde_json::from_slice(body).map_err(|e| invalid_response(&e, body))?;
    if envelope.success { Ok(()) } else { Err(failure(envelope.message.as_deref())) }
}

/// The error of a failed call: the `bw` message, or a generic one.
fn failure(message: Option<&str>) -> BwError {
    BwError::from_message(message.unwrap_or("Unknown error."))
}

/// Decodes a `bw serve` response, mapping HTTP 400 and 403 as specified.
pub fn decode_response<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, BwError> {
    if status == 403 {
        return Err(BwError::Rejected);
    }
    decode(body)
}

/// Extracts the `template` payload from a `/status` response.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusTemplate {
    /// The wrapped status object.
    pub template: crate::bw::models::Status,
}

#[cfg(test)]
mod tests {

    #[test]
    fn decode_errors_never_quote_the_body() {
        // A field with the wrong type: serde would quote the value.
        let body = br#"{"success":true,"data":{"n":"SYNTHETIC_SECRET"}}"#;
        #[derive(Debug, serde::Deserialize)]
        struct Shape {
            #[expect(dead_code, reason = "only the failed parse matters")]
            n: u32,
        }
        let err = decode::<Shape>(body).unwrap_err().to_string();
        assert!(!err.contains("SYNTHETIC_SECRET"), "{err}");
        let err = check(200, b"{\"success\": SYNTHETIC_SECRET").unwrap_err().to_string();
        assert!(!err.contains("SYNTHETIC_SECRET"), "{err}");
    }
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_success() {
        let body = serde_json::to_vec(&json!({"success": true, "data": {"x": 1}})).unwrap();
        let value: serde_json::Value = decode(&body).unwrap();
        assert_eq!(value["x"], 1);
    }

    #[test]
    fn maps_messages() {
        let body = serde_json::to_vec(&json!({
            "success": false, "message": "Vault is locked.", "data": null
        }))
        .unwrap();
        let err = decode::<serde_json::Value>(&body).unwrap_err();
        assert!(matches!(err, BwError::Locked));
        assert!(err.requires_lock());
    }

    #[test]
    fn message_mapping_is_case_insensitive() {
        assert!(matches!(BwError::from_message("VAULT IS LOCKED."), BwError::Locked));
        assert!(matches!(BwError::from_message("Not found."), BwError::NotFound));
        assert!(matches!(
            BwError::from_message("Invalid master password."),
            BwError::InvalidPassword
        ));
        assert!(matches!(
            BwError::from_message("More than one result was found."),
            BwError::Ambiguous(_)
        ));
        assert!(matches!(BwError::from_message("Some new error"), BwError::Other(_)));
    }

    #[test]
    fn forbidden_is_rejected() {
        let err = decode_response::<serde_json::Value>(403, b"{}").unwrap_err();
        assert!(matches!(err, BwError::Rejected));
    }

    #[test]
    fn invalid_json_is_an_invalid_response() {
        let err = decode::<serde_json::Value>(b"not json").unwrap_err();
        assert!(matches!(err, BwError::InvalidResponse(_)));
    }

    #[test]
    fn a_failed_call_reports_its_message_whatever_its_payload() {
        #[derive(Debug, serde::Deserialize)]
        struct List {
            #[expect(dead_code, reason = "only the failed parse matters")]
            data: Vec<u8>,
        }
        let body = br#"{"success":false,"message":"Vault is locked.","data":"not a list"}"#;
        assert!(matches!(decode::<List>(body), Err(BwError::Locked)));
    }

    #[test]
    fn check_reads_only_the_envelope() {
        let payload = br#"{"success":true,"data":{"deep":[1,[2,[3]]]}}"#;
        assert!(check(200, payload).is_ok());
        let failed = br#"{"success":false,"message":"Not found.","data":null}"#;
        assert!(matches!(check(200, failed), Err(BwError::NotFound)));
    }
}
