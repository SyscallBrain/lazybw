//! Data models mirroring the JSON exchanged with the Bitwarden CLI.
//!
//! Field names and enums follow `docs/bw-integration.md` §7. Every struct
//! keeps a flattened `extra` map so unknown fields — including fields added
//! by future CLI versions — survive a read-modify-write cycle unchanged.
//! Dates stay `String`: they are parsed lazily for display and never
//! re-serialised in a different format.

use crate::bw::sensitive::Sensitive;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// The kind of a vault item. Unknown integers are preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ItemType {
    /// Login (1).
    #[default]
    Login,
    /// Secure note (2).
    SecureNote,
    /// Card (3).
    Card,
    /// Identity (4).
    Identity,
    /// SSH key (5).
    SshKey,
    /// Types 6, 7, 8 and anything the CLI adds later. Read-only.
    Other(u8),
}

impl ItemType {
    /// The integer used by the API.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Login => 1,
            Self::SecureNote => 2,
            Self::Card => 3,
            Self::Identity => 4,
            Self::SshKey => 5,
            Self::Other(n) => n,
        }
    }

    /// Human readable singular, used in titles and pickers.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::SecureNote => "secure note",
            Self::Card => "card",
            Self::Identity => "identity",
            Self::SshKey => "SSH key",
            Self::Other(6) => "bank account",
            Self::Other(7) => "driver's licence",
            Self::Other(8) => "passport",
            Self::Other(_) => "unknown item",
        }
    }

    /// Plural used in filter titles.
    #[must_use]
    pub fn plural(self) -> &'static str {
        match self {
            Self::Login => "Logins",
            Self::SecureNote => "Secure notes",
            Self::Card => "Cards",
            Self::Identity => "Identities",
            Self::SshKey => "SSH keys",
            Self::Other(_) => "Other",
        }
    }

    /// True for the five types lazybw can edit.
    #[must_use]
    pub fn is_editable(self) -> bool {
        matches!(self, Self::Login | Self::SecureNote | Self::Card | Self::Identity | Self::SshKey)
    }

    /// Parses the short names accepted by the command line and `item.new`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "login" | "l" | "logins" => Some(Self::Login),
            "note" | "n" | "notes" | "secure-note" | "secure_note" => Some(Self::SecureNote),
            "card" | "c" | "cards" => Some(Self::Card),
            "identity" | "i" | "identities" => Some(Self::Identity),
            "ssh" | "s" | "ssh-key" | "ssh_key" | "sshkey" => Some(Self::SshKey),
            _ => None,
        }
    }
}

impl Serialize for ItemType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for ItemType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let n = u8::deserialize(deserializer)?;
        Ok(match n {
            1 => Self::Login,
            2 => Self::SecureNote,
            3 => Self::Card,
            4 => Self::Identity,
            5 => Self::SshKey,
            other => Self::Other(other),
        })
    }
}

/// How a login URI is matched by the browser extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "Option<u8>", into = "Option<u8>")]
pub enum UriMatch {
    /// Default (null).
    #[default]
    Default,
    /// Base domain (0).
    BaseDomain,
    /// Host (1).
    Host,
    /// Starts with (2).
    StartsWith,
    /// Exact (3).
    Exact,
    /// Regular expression (4).
    Regex,
    /// Never (5).
    Never,
}

impl From<Option<u8>> for UriMatch {
    fn from(value: Option<u8>) -> Self {
        match value {
            None => Self::Default,
            Some(0) => Self::BaseDomain,
            Some(1) => Self::Host,
            Some(2) => Self::StartsWith,
            Some(3) => Self::Exact,
            Some(4) => Self::Regex,
            Some(5) => Self::Never,
            Some(_) => Self::Default,
        }
    }
}

impl From<UriMatch> for Option<u8> {
    fn from(value: UriMatch) -> Self {
        match value {
            UriMatch::Default => None,
            UriMatch::BaseDomain => Some(0),
            UriMatch::Host => Some(1),
            UriMatch::StartsWith => Some(2),
            UriMatch::Exact => Some(3),
            UriMatch::Regex => Some(4),
            UriMatch::Never => Some(5),
        }
    }
}

impl UriMatch {
    /// The label shown in the editor chip.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::BaseDomain => "Base domain",
            Self::Host => "Host",
            Self::StartsWith => "Starts with",
            Self::Exact => "Exact",
            Self::Regex => "Regular expression",
            Self::Never => "Never",
        }
    }

    /// Cycles to the next option, as `Ctrl-a` does in the editor.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::BaseDomain,
            Self::BaseDomain => Self::Host,
            Self::Host => Self::StartsWith,
            Self::StartsWith => Self::Exact,
            Self::Exact => Self::Regex,
            Self::Regex => Self::Never,
            Self::Never => Self::Default,
        }
    }

    /// Cycles to the previous option.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Default => Self::Never,
            Self::BaseDomain => Self::Default,
            Self::Host => Self::BaseDomain,
            Self::StartsWith => Self::Host,
            Self::Exact => Self::StartsWith,
            Self::Regex => Self::Exact,
            Self::Never => Self::Regex,
        }
    }
}

/// One URI attached to a login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoginUri {
    /// The URI itself; `None` for an empty row being edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Match detection.
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    pub match_type: Option<u8>,
    /// Unknown fields (`uriChecksum`, …).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl LoginUri {
    /// Creates a URI row with default match detection.
    #[must_use]
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: Some(uri.into()), match_type: None, extra: Map::new() }
    }

    /// Typed match detection.
    #[must_use]
    pub fn match_kind(&self) -> UriMatch {
        UriMatch::from(self.match_type)
    }

    /// Sets the match detection, clearing the field for `Default`.
    pub fn set_match_kind(&mut self, kind: UriMatch) {
        self.match_type = kind.into();
    }

    /// The URI as a string, empty when absent.
    #[must_use]
    pub fn uri_str(&self) -> &str {
        self.uri.as_deref().unwrap_or("")
    }
}

/// Authentication data of a login item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    /// Websites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uris: Option<Vec<LoginUri>>,
    /// Username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<Sensitive>,
    /// Password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Sensitive>,
    /// Authenticator key (`otpauth://` or Base32 secret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<Sensitive>,
    /// When the password last changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_revision_date: Option<String>,
    /// Opaque passkey credentials, shown read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fido2_credentials: Option<Vec<Value>>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Login {
    /// The first URI, if any.
    #[must_use]
    pub fn first_uri(&self) -> Option<&str> {
        self.uris.as_ref()?.iter().find_map(|u| u.uri.as_deref().filter(|s| !s.is_empty()))
    }

    /// True when a TOTP secret is stored.
    #[must_use]
    pub fn has_totp(&self) -> bool {
        self.totp.as_ref().is_some_and(|t| !t.is_empty())
    }
}

/// A custom field attached to any item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Field label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Field value; `None` for linked and boolean fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Sensitive>,
    /// 0 text, 1 hidden, 2 boolean, 3 linked.
    #[serde(rename = "type", default)]
    pub kind: u8,
    /// Target of a linked field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_id: Option<u16>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The kind of a custom field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldKind {
    /// Plain text.
    #[default]
    Text,
    /// Hidden, masked like a password.
    Hidden,
    /// Boolean, stored as the string `"true"` or `"false"`.
    Boolean,
    /// Linked to a built-in field.
    Linked,
}

impl FieldKind {
    /// From the wire integer.
    #[must_use]
    pub fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Hidden,
            2 => Self::Boolean,
            3 => Self::Linked,
            _ => Self::Text,
        }
    }

    /// To the wire integer.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Hidden => 1,
            Self::Boolean => 2,
            Self::Linked => 3,
        }
    }

    /// Label for the chip in the editor.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Hidden => "hidden",
            Self::Boolean => "boolean",
            Self::Linked => "linked",
        }
    }

    /// Cycles forward.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Text => Self::Hidden,
            Self::Hidden => Self::Boolean,
            Self::Boolean => Self::Linked,
            Self::Linked => Self::Text,
        }
    }
}

impl Field {
    /// Typed kind.
    #[must_use]
    pub fn kind(&self) -> FieldKind {
        FieldKind::from_code(self.kind)
    }

    /// True when the field is false, for a boolean field.
    #[must_use]
    pub fn boolean(&self) -> bool {
        self.value.as_ref().is_some_and(|v| v.expose().eq_ignore_ascii_case("true"))
    }

    /// Name, empty when unset.
    #[must_use]
    pub fn name_str(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }
}

/// Secure note payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecureNote {
    /// 0 = generic.
    #[serde(rename = "type", default)]
    pub kind: u8,
}

/// Card payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Cardholder name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardholder_name: Option<String>,
    /// Brand name (`Visa`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// Card number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<Sensitive>,
    /// Expiry month, `"1"`–`"12"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_month: Option<String>,
    /// Four digit expiry year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_year: Option<String>,
    /// Security code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<Sensitive>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Identity payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// Title (`Mr`, `Ms`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// First name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    /// Middle name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    /// Last name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    /// Address line 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address1: Option<String>,
    /// Address line 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address2: Option<String>,
    /// Address line 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address3: Option<String>,
    /// City.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// State or region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    /// Country.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    /// Email address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Phone number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Social security number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssn: Option<Sensitive>,
    /// Username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Passport number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passport_number: Option<Sensitive>,
    /// Driver's licence number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_number: Option<Sensitive>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Identity {
    /// Full name, trimmed.
    #[must_use]
    pub fn full_name(&self) -> String {
        [self.first_name.as_deref(), self.middle_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// SSH key payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    /// OpenSSH private key (PEM).
    #[serde(default)]
    pub private_key: Option<Sensitive>,
    /// OpenSSH public key line.
    #[serde(default)]
    pub public_key: String,
    /// `SHA256:…` fingerprint.
    #[serde(default)]
    pub key_fingerprint: String,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A previous password kept by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PasswordHistory {
    /// When the password was replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_date: Option<String>,
    /// The old password.
    #[serde(default)]
    pub password: Sensitive,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A read-only attachment entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// Attachment id.
    #[serde(default)]
    pub id: String,
    /// File name.
    #[serde(default)]
    pub file_name: String,
    /// Size in bytes, as a string.
    #[serde(default)]
    pub size: String,
    /// Human readable size.
    #[serde(default)]
    pub size_name: String,
    /// Download URL (not used: no attachments download).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A vault item of any type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// Item id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Organization id (always `None` for a personal vault; preserved anyway).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    /// Folder id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    /// Item type.
    #[serde(rename = "type", default = "default_item_type")]
    pub kind: ItemType,
    /// Item name.
    #[serde(default)]
    pub name: String,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Sensitive>,
    /// Favourite flag.
    #[serde(default)]
    pub favorite: bool,
    /// 0 none, 1 master password re-prompt.
    #[serde(default)]
    pub reprompt: u8,
    /// Custom fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<Field>>,
    /// Login payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<Login>,
    /// Secure note payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_note: Option<SecureNote>,
    /// Card payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<Card>,
    /// Identity payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    /// SSH key payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<SshKey>,
    /// Previous passwords.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_history: Option<Vec<PasswordHistory>>,
    /// Attachments, listed read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
    /// Collection ids (empty for a personal vault; preserved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_ids: Option<Vec<String>>,
    /// Last modification, exactly as received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_date: Option<String>,
    /// Creation, exactly as received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_date: Option<String>,
    /// Trash timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_date: Option<String>,
    /// Archive timestamp (Premium; read-only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_date: Option<String>,
    /// `"item"` in responses; omitted when sending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Unknown fields at the item level.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_item_type() -> ItemType {
    ItemType::Login
}

impl Item {
    /// A new empty item of the given type, ready for the editor.
    #[must_use]
    pub fn new(kind: ItemType) -> Self {
        let mut item = Self { kind, ..Self::default() };
        match kind {
            ItemType::Login => item.login = Some(Login::default()),
            ItemType::SecureNote => item.secure_note = Some(SecureNote::default()),
            ItemType::Card => item.card = Some(Card::default()),
            ItemType::Identity => item.identity = Some(Identity::default()),
            ItemType::SshKey => item.ssh_key = Some(SshKey::default()),
            ItemType::Other(_) => {}
        }
        item.fields = Some(Vec::new());
        item
    }

    /// True when the item is in the trash.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_date.is_some()
    }

    /// True when the item is archived (Premium read-only view).
    #[must_use]
    pub fn is_archived(&self) -> bool {
        self.archived_date.is_some()
    }

    /// True when the item asks for the master password before revealing secrets.
    #[must_use]
    pub fn wants_reprompt(&self) -> bool {
        self.reprompt == 1
    }

    /// A short subtitle for the list row.
    #[must_use]
    pub fn subtitle(&self) -> String {
        match self.kind {
            ItemType::Login => self
                .login
                .as_ref()
                .and_then(|l| {
                    l.username
                        .as_ref()
                        .filter(|u| !u.is_empty())
                        .map(|u| u.expose().to_owned())
                        .or_else(|| l.first_uri().map(str::to_owned))
                })
                .unwrap_or_default(),
            ItemType::Card => {
                // Borrowed: cloning the card would copy its number and
                // security code for every row built.
                let Some(card) = self.card.as_ref() else { return String::new() };
                let brand = card.brand.as_deref().unwrap_or("");
                let last4 = card
                    .number
                    .as_ref()
                    .map(|n| n.expose().chars().filter(char::is_ascii_digit).collect::<String>())
                    .map(|digits| {
                        let n = digits.len();
                        digits.chars().skip(n.saturating_sub(4)).collect::<String>()
                    })
                    .unwrap_or_default();
                match (brand.is_empty(), last4.is_empty()) {
                    (false, false) => format!("{brand} •••• {last4}"),
                    (false, true) => brand.to_owned(),
                    (true, false) => format!("•••• {last4}"),
                    (true, true) => String::new(),
                }
            }
            ItemType::Identity => {
                self.identity.as_ref().map(Identity::full_name).unwrap_or_default()
            }
            // The note itself stays out of the list, where anyone looking at
            // the screen reads it: it is a secret like the others (people keep
            // recovery codes and PINs in notes), shown only in the detail pane.
            ItemType::SecureNote => {
                let lines = self.notes.as_ref().map_or(0, |n| n.expose().lines().count());
                match lines {
                    0 => String::new(),
                    1 => "1 line".to_owned(),
                    n => format!("{n} lines"),
                }
            }
            ItemType::SshKey => self
                .ssh_key
                .as_ref()
                .map(|k| {
                    let fp = k.key_fingerprint.clone();
                    if fp.chars().count() > 16 {
                        let start: String = fp.chars().take(11).collect();
                        let end: String = fp.chars().rev().take(3).collect::<String>();
                        format!("{start}…{}", end.chars().rev().collect::<String>())
                    } else {
                        fp
                    }
                })
                .unwrap_or_default(),
            ItemType::Other(_) => String::new(),
        }
    }

    /// True when the item has a TOTP secret.
    #[must_use]
    pub fn has_totp(&self) -> bool {
        self.login.as_ref().is_some_and(Login::has_totp)
    }

    /// True when the item has passkeys.
    #[must_use]
    pub fn has_passkey(&self) -> bool {
        self.login
            .as_ref()
            .and_then(|l| l.fido2_credentials.as_ref())
            .is_some_and(|c| !c.is_empty())
    }

    /// Number of attachments.
    #[must_use]
    pub fn attachment_count(&self) -> usize {
        self.attachments.as_ref().map_or(0, Vec::len)
    }

    /// Returns the item body that should be sent on create (no id, object or dates).
    #[must_use]
    pub fn for_create(&self) -> Self {
        let mut item = self.clone();
        item.id = None;
        item.object = None;
        item.revision_date = None;
        item.creation_date = None;
        item.deleted_date = None;
        item.archived_date = None;
        item.collection_ids = None;
        item.organization_id = None;
        item
    }

    /// The item's value objects as (label, plain value, secret) rows for
    /// read-only display of unknown types.
    #[must_use]
    pub fn unknown_fields(&self) -> Vec<(String, String, bool)> {
        let key = match self.kind {
            ItemType::Other(n) => match n {
                6 => "bankAccount",
                7 => "driversLicense",
                8 => "passport",
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let Some(Value::Object(map)) = self.extra.get(key) else {
            return Vec::new();
        };
        map.iter()
            .filter_map(|(k, v)| {
                let text = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => return None,
                    other => other.to_string(),
                };
                let secret = ["number", "code", "pin", "ssn", "iban", "account", "password"]
                    .iter()
                    .any(|needle| k.to_ascii_lowercase().contains(needle));
                Some((humanize_camel(k), text, secret))
            })
            .collect()
    }
}

/// `expMonth` → `Exp month`.
#[must_use]
pub fn humanize_camel(key: &str) -> String {
    let mut out = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// Folder id; the pseudo entry "No Folder" has `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Folder name, possibly nested with `/`.
    #[serde(default)]
    pub name: String,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Folder {
    /// The path segments of a nested name.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        self.name.split('/').map(str::trim).filter(|s| !s.is_empty()).collect()
    }
}

/// The session status returned by `bw status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `unauthenticated`, `locked` or `unlocked`.
    pub status: String,
    /// Server URL; `None` for the default US cloud.
    #[serde(default)]
    pub server_url: Option<String>,
    /// Account email.
    #[serde(default)]
    pub user_email: Option<String>,
    /// Last sync timestamp.
    #[serde(default)]
    pub last_sync: Option<String>,
    /// User id.
    #[serde(default)]
    pub user_id: Option<String>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Status {
    /// True when the vault is unlocked.
    #[must_use]
    pub fn is_unlocked(&self) -> bool {
        self.status.eq_ignore_ascii_case("unlocked")
    }

    /// True when the account is logged in but locked.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.status.eq_ignore_ascii_case("locked")
    }

    /// True when no account is logged in.
    #[must_use]
    pub fn is_unauthenticated(&self) -> bool {
        self.status.eq_ignore_ascii_case("unauthenticated")
    }
}

/// One entry of `bw report password-health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HealthEntry {
    /// Payload kind (`password-health`).
    #[serde(default)]
    pub object: Option<String>,
    /// Item id.
    #[serde(default)]
    pub id: String,
    /// Item name.
    #[serde(default)]
    pub name: String,
    /// 0–4 strength score computed by the server.
    #[serde(default)]
    pub password_strength: u8,
    /// Number of times the password is reused; `None` unless > 1.
    #[serde(default)]
    pub reuse_count: Option<u32>,
    /// True when the password appears in a breach.
    #[serde(default)]
    pub exposed: Option<bool>,
    /// Times seen in breaches.
    #[serde(default)]
    pub exposed_count: Option<u64>,
    /// Error from the HIBP lookup.
    #[serde(default)]
    pub exposed_error: Option<String>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A generator request, mirroring the `/generate` query parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateRequest {
    /// Passphrase instead of a password.
    pub passphrase: bool,
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
    /// Number of words in a passphrase.
    pub words: u8,
    /// Word separator (`space` and `empty` are special).
    pub separator: String,
    /// Capitalise passphrase words.
    pub capitalize: bool,
    /// Include a number in a passphrase.
    pub include_number: bool,
}

impl Default for GenerateRequest {
    fn default() -> Self {
        Self {
            passphrase: false,
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

impl GenerateRequest {
    /// The query pairs for `GET /generate`.
    #[must_use]
    pub fn query(&self) -> Vec<(&'static str, String)> {
        vec![
            ("passphrase", self.passphrase.to_string()),
            ("length", self.length.to_string()),
            ("uppercase", self.uppercase.to_string()),
            ("lowercase", self.lowercase.to_string()),
            ("number", self.numbers.to_string()),
            ("special", self.special.to_string()),
            ("minNumber", self.min_numbers.to_string()),
            ("minSpecial", self.min_special.to_string()),
            ("ambiguous", self.avoid_ambiguous.to_string()),
            ("words", self.words.to_string()),
            ("separator", self.separator.clone()),
            ("capitalize", self.capitalize.to_string()),
            ("includeNumber", self.include_number.to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn sample_item_json() -> Value {
        json!({
            "object": "item",
            "id": "9f8c1b2a",
            "organizationId": null,
            "folderId": "f1",
            "type": 1,
            "name": "GitHub",
            "notes": "2FA in the vault",
            "favorite": true,
            "reprompt": 1,
            "fields": [
                {"name": "Recovery", "value": "abc", "type": 1},
                {"name": "Linked user", "value": null, "type": 3, "linkedId": 100}
            ],
            "login": {
                "uris": [{"uri": "https://github.com", "match": null}],
                "username": "dev",
                "password": "s3cret",
                "totp": "JBSWY3DPEHPK3PXP",
                "passwordRevisionDate": "2026-03-01T10:00:00.000Z",
                "fido2Credentials": [{"rpId": "github.com", "userName": "dev"}],
                "futureLoginField": 7
            },
            "passwordHistory": [{"lastUsedDate": "2025-01-01T00:00:00.000Z", "password": "old"}],
            "attachments": [{"id": "a", "fileName": "x.txt", "size": "10", "sizeName": "10 B", "url": "https://x"}],
            "collectionIds": null,
            "revisionDate": "2026-10-01T12:00:00.000Z",
            "creationDate": "2026-01-01T00:00:00.000Z",
            "deletedDate": null,
            "archivedDate": null,
            "futureTopLevel": {"a": [1, 2, 3]}
        })
    }

    #[test]
    fn round_trips_unknown_fields() {
        let original = sample_item_json();
        let item: Item = serde_json::from_value(original.clone()).unwrap();
        let back = serde_json::to_value(&item).unwrap();
        assert_eq!(back["futureTopLevel"], original["futureTopLevel"]);
        assert_eq!(back["login"]["futureLoginField"], original["login"]["futureLoginField"]);
        assert_eq!(back["login"]["fido2Credentials"], original["login"]["fido2Credentials"]);
        assert_eq!(back["passwordHistory"], original["passwordHistory"]);
        assert_eq!(back["attachments"], original["attachments"]);
        assert_eq!(back["type"], 1);
    }

    #[test]
    fn item_debug_does_not_leak_secrets() {
        let item: Item = serde_json::from_value(sample_item_json()).unwrap();
        let debug = format!("{item:?}");
        assert!(!debug.contains("s3cret"));
        assert!(!debug.contains("JBSWY3DPEHPK3PXP"));
    }

    #[test]
    fn unknown_item_type_is_preserved() {
        let mut value = sample_item_json();
        value["type"] = json!(7);
        let item: Item = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(item.kind, ItemType::Other(7));
        assert_eq!(serde_json::to_value(&item).unwrap()["type"], 7);
    }

    #[test]
    fn uri_match_round_trip() {
        let u =
            LoginUri { uri: Some("https://x".into()), match_type: Some(1), ..Default::default() };
        let v = serde_json::to_value(&u).unwrap();
        assert_eq!(v["match"], 1);
        let back: LoginUri = serde_json::from_value(v).unwrap();
        assert_eq!(back.match_kind(), UriMatch::Host);
        let mut d = LoginUri::new("https://x");
        d.set_match_kind(UriMatch::Default);
        assert!(serde_json::to_value(&d).unwrap().get("match").is_none());
    }

    #[test]
    fn labels_do_not_panic() {
        for n in 0..=255u8 {
            let t = ItemType::Other(n);
            let _ = t.label();
            let _ = t.plural();
        }
    }

    #[test]
    fn item_for_create_drops_server_fields() {
        let item: Item = serde_json::from_value(sample_item_json()).unwrap();
        let create = item.for_create();
        assert!(create.id.is_none());
        assert!(create.object.is_none());
        assert!(create.revision_date.is_none());
        assert_eq!(create.name, "GitHub");
    }
}
