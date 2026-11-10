//! The editor form's values, read from and written to the item itself.
//!
//! The item being edited is the only copy of what the form shows: a row is a
//! [`FormKey`] and a way to edit it, and its value is looked up in the item
//! when the row is drawn, typed into or saved. Text fields are edited in
//! place, so typing in a password leaves no second copy of it behind.

use crate::app::state::FormKey;
use crate::bw::models::{
    Card, FieldKind, Identity, Item, ItemType, Login, LoginUri, SecureNote, SshKey, UriMatch,
};
use crate::bw::sensitive::Sensitive;
use crate::vault::cache::VaultCache;

/// The labels of the match-detection choices, in the order they cycle.
pub const URI_MATCHES: &[&str] =
    &["Default", "Base domain", "Host", "Starts with", "Exact", "Regular expression", "Never"];

/// The match detection a label stands for.
#[must_use]
pub fn uri_match(label: &str) -> UriMatch {
    match label {
        "Base domain" => UriMatch::BaseDomain,
        "Host" => UriMatch::Host,
        "Starts with" => UriMatch::StartsWith,
        "Exact" => UriMatch::Exact,
        "Regular expression" => UriMatch::Regex,
        "Never" => UriMatch::Never,
        _ => UriMatch::Default,
    }
}

/// The field kind a label (`text`, `hidden`, …) stands for.
#[must_use]
pub fn field_kind(label: &str) -> FieldKind {
    FieldKind::from_code(match label {
        "hidden" => 1,
        "boolean" => 2,
        "linked" => 3,
        _ => 0,
    })
}

fn bool_text(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn opt(value: Option<&String>) -> &str {
    value.map_or("", String::as_str)
}

fn secret(value: Option<&Sensitive>) -> &str {
    value.map_or("", Sensitive::expose)
}

/// The value a row shows: text, a choice label, or `true`/`false`.
#[must_use]
pub fn value<'a>(item: &'a Item, key: FormKey, cache: &'a VaultCache) -> &'a str {
    let login = item.login.as_ref();
    let card = item.card.as_ref();
    let identity = item.identity.as_ref();
    let ssh = item.ssh_key.as_ref();
    let field = |index: usize| item.fields.as_ref().and_then(|fields| fields.get(index));
    let uri = |index: usize| login.and_then(|l| l.uris.as_ref()).and_then(|uris| uris.get(index));
    match key {
        FormKey::Name => &item.name,
        FormKey::Folder => {
            item.folder_id.as_ref().and_then(|id| cache.folder_path(id)).unwrap_or("")
        }
        FormKey::Notes => secret(item.notes.as_ref()),
        FormKey::Favorite => bool_text(item.favorite),
        FormKey::Reprompt => bool_text(item.reprompt == 1),
        FormKey::Username => secret(login.and_then(|l| l.username.as_ref())),
        FormKey::Password => secret(login.and_then(|l| l.password.as_ref())),
        FormKey::Totp => secret(login.and_then(|l| l.totp.as_ref())),
        FormKey::Uri(index) => uri(index).map_or("", LoginUri::uri_str),
        FormKey::UriMatch(index) => uri(index).map_or("Default", |u| u.match_kind().label()),
        FormKey::FieldName(index) => field(index).map_or("", |f| f.name_str()),
        FormKey::FieldType(index) => field(index).map_or("text", |f| f.kind().label()),
        FormKey::FieldValue(index) => secret(field(index).and_then(|f| f.value.as_ref())),
        FormKey::Cardholder => opt(card.and_then(|c| c.cardholder_name.as_ref())),
        FormKey::CardNumber => secret(card.and_then(|c| c.number.as_ref())),
        FormKey::CardBrand => opt(card.and_then(|c| c.brand.as_ref())),
        FormKey::ExpMonth => opt(card.and_then(|c| c.exp_month.as_ref())),
        FormKey::ExpYear => opt(card.and_then(|c| c.exp_year.as_ref())),
        FormKey::CardCode => secret(card.and_then(|c| c.code.as_ref())),
        FormKey::IdTitle => opt(identity.and_then(|i| i.title.as_ref())),
        FormKey::IdFirst => opt(identity.and_then(|i| i.first_name.as_ref())),
        FormKey::IdMiddle => opt(identity.and_then(|i| i.middle_name.as_ref())),
        FormKey::IdLast => opt(identity.and_then(|i| i.last_name.as_ref())),
        FormKey::IdUsername => opt(identity.and_then(|i| i.username.as_ref())),
        FormKey::IdCompany => opt(identity.and_then(|i| i.company.as_ref())),
        FormKey::IdEmail => opt(identity.and_then(|i| i.email.as_ref())),
        FormKey::IdPhone => opt(identity.and_then(|i| i.phone.as_ref())),
        FormKey::IdAddress1 => opt(identity.and_then(|i| i.address1.as_ref())),
        FormKey::IdAddress2 => opt(identity.and_then(|i| i.address2.as_ref())),
        FormKey::IdAddress3 => opt(identity.and_then(|i| i.address3.as_ref())),
        FormKey::IdCity => opt(identity.and_then(|i| i.city.as_ref())),
        FormKey::IdState => opt(identity.and_then(|i| i.state.as_ref())),
        FormKey::IdPostal => opt(identity.and_then(|i| i.postal_code.as_ref())),
        FormKey::IdCountry => opt(identity.and_then(|i| i.country.as_ref())),
        FormKey::IdSsn => secret(identity.and_then(|i| i.ssn.as_ref())),
        FormKey::IdPassport => secret(identity.and_then(|i| i.passport_number.as_ref())),
        FormKey::IdLicense => secret(identity.and_then(|i| i.license_number.as_ref())),
        FormKey::SshPublic => ssh.map_or("", |k| k.public_key.as_str()),
        FormKey::SshFingerprint => ssh.map_or("", |k| k.key_fingerprint.as_str()),
        FormKey::SshPrivate => secret(ssh.and_then(|k| k.private_key.as_ref())),
        FormKey::UriAdd | FormKey::FieldAdd | FormKey::SshGenerate | FormKey::SshImport => "",
    }
}

fn login(item: &mut Item) -> &mut Login {
    item.login.get_or_insert_with(Login::default)
}

fn card(item: &mut Item) -> &mut Card {
    item.card.get_or_insert_with(Card::default)
}

fn identity(item: &mut Item) -> &mut Identity {
    item.identity.get_or_insert_with(Identity::default)
}

fn ssh(item: &mut Item) -> &mut SshKey {
    item.ssh_key.get_or_insert_with(SshKey::default)
}

fn text(value: &mut Option<String>) -> &mut String {
    value.get_or_insert_with(String::new)
}

fn sensitive(value: &mut Option<Sensitive>) -> &mut String {
    value.get_or_insert_with(Sensitive::default).expose_mut()
}

/// The text a row edits, in the item, for rows that take typed text. A
/// missing value starts empty; [`settle`] turns an empty one back into
/// "absent" once the edit is done.
pub fn text_mut(item: &mut Item, key: FormKey) -> Option<&mut String> {
    Some(match key {
        FormKey::Name => &mut item.name,
        FormKey::Notes => sensitive(&mut item.notes),
        FormKey::Username => sensitive(&mut login(item).username),
        FormKey::Password => sensitive(&mut login(item).password),
        FormKey::Totp => sensitive(&mut login(item).totp),
        FormKey::Uri(index) => {
            let uri = login(item).uris.as_mut()?.get_mut(index)?;
            text(&mut uri.uri)
        }
        FormKey::FieldName(index) => text(&mut item.fields.as_mut()?.get_mut(index)?.name),
        FormKey::FieldValue(index) => {
            let field = item.fields.as_mut()?.get_mut(index)?;
            if matches!(field.kind(), FieldKind::Boolean | FieldKind::Linked) {
                return None;
            }
            sensitive(&mut field.value)
        }
        FormKey::Cardholder => text(&mut card(item).cardholder_name),
        FormKey::CardNumber => sensitive(&mut card(item).number),
        FormKey::ExpYear => text(&mut card(item).exp_year),
        FormKey::CardCode => sensitive(&mut card(item).code),
        FormKey::IdFirst => text(&mut identity(item).first_name),
        FormKey::IdMiddle => text(&mut identity(item).middle_name),
        FormKey::IdLast => text(&mut identity(item).last_name),
        FormKey::IdUsername => text(&mut identity(item).username),
        FormKey::IdCompany => text(&mut identity(item).company),
        FormKey::IdEmail => text(&mut identity(item).email),
        FormKey::IdPhone => text(&mut identity(item).phone),
        FormKey::IdAddress1 => text(&mut identity(item).address1),
        FormKey::IdAddress2 => text(&mut identity(item).address2),
        FormKey::IdAddress3 => text(&mut identity(item).address3),
        FormKey::IdCity => text(&mut identity(item).city),
        FormKey::IdState => text(&mut identity(item).state),
        FormKey::IdPostal => text(&mut identity(item).postal_code),
        FormKey::IdCountry => text(&mut identity(item).country),
        FormKey::IdSsn => sensitive(&mut identity(item).ssn),
        FormKey::IdPassport => sensitive(&mut identity(item).passport_number),
        FormKey::IdLicense => sensitive(&mut identity(item).license_number),
        FormKey::SshPublic => &mut ssh(item).public_key,
        FormKey::SshPrivate => sensitive(&mut ssh(item).private_key),
        _ => return None,
    })
}

fn settle_text(value: &mut Option<String>) {
    if value.as_ref().is_some_and(String::is_empty) {
        *value = None;
    }
}

fn settle_secret(value: &mut Option<Sensitive>) {
    if value.as_ref().is_some_and(Sensitive::is_empty) {
        *value = None;
    }
}

/// Tidies the item after a row changed: an emptied value is absent again,
/// and a card number names its brand when none is set.
pub fn settle(item: &mut Item, key: FormKey) {
    match key {
        FormKey::Notes => settle_secret(&mut item.notes),
        FormKey::Username | FormKey::Password | FormKey::Totp => {
            if let Some(login) = item.login.as_mut() {
                settle_secret(&mut login.username);
                settle_secret(&mut login.password);
                settle_secret(&mut login.totp);
            }
        }
        FormKey::FieldName(index) | FormKey::FieldValue(index) => {
            if let Some(field) = item.fields.as_mut().and_then(|f| f.get_mut(index)) {
                settle_text(&mut field.name);
                settle_secret(&mut field.value);
            }
        }
        FormKey::Cardholder | FormKey::CardNumber | FormKey::ExpYear | FormKey::CardCode => {
            if let Some(card) = item.card.as_mut() {
                settle_text(&mut card.cardholder_name);
                settle_secret(&mut card.number);
                settle_text(&mut card.exp_year);
                settle_secret(&mut card.code);
                if key == FormKey::CardNumber && card.brand.as_deref().unwrap_or("").is_empty() {
                    card.brand = card
                        .number
                        .as_ref()
                        .and_then(|n| crate::util::card_brand(n.expose()))
                        .map(str::to_owned);
                }
            }
        }
        FormKey::SshPrivate => {
            if let Some(key) = item.ssh_key.as_mut() {
                settle_secret(&mut key.private_key);
            }
        }
        _ => {
            if let Some(identity) = item.identity.as_mut() {
                for value in [
                    &mut identity.first_name,
                    &mut identity.middle_name,
                    &mut identity.last_name,
                    &mut identity.username,
                    &mut identity.company,
                    &mut identity.email,
                    &mut identity.phone,
                    &mut identity.address1,
                    &mut identity.address2,
                    &mut identity.address3,
                    &mut identity.city,
                    &mut identity.state,
                    &mut identity.postal_code,
                    &mut identity.country,
                ] {
                    settle_text(value);
                }
                settle_secret(&mut identity.ssn);
                settle_secret(&mut identity.passport_number);
                settle_secret(&mut identity.license_number);
            }
        }
    }
}

/// Sets a row to `value`: typed text, or the label of a choice. Returns
/// false when the row takes no value (a button, a read-only field).
pub fn set(item: &mut Item, key: FormKey, value: &str, cache: &VaultCache) -> bool {
    if let Some(text) = text_mut(item, key) {
        let old = std::mem::replace(text, value.to_owned());
        wipe(old);
        settle(item, key);
        return true;
    }
    let none_if_empty = |value: &str| (!value.is_empty()).then(|| value.to_owned());
    match key {
        FormKey::Folder => {
            // The sidebar shows folders with the same name as one row: the
            // first of them stands for it.
            item.folder_id = if value.is_empty() || value == "No folder" {
                None
            } else {
                cache.folders().iter().find(|f| f.name == value).and_then(|f| f.id.clone())
            };
        }
        FormKey::Favorite => item.favorite = value == "true",
        FormKey::Reprompt => item.reprompt = u8::from(value == "true"),
        FormKey::UriMatch(index) => {
            let Some(uri) = item
                .login
                .as_mut()
                .and_then(|l| l.uris.as_mut())
                .and_then(|uris| uris.get_mut(index))
            else {
                return false;
            };
            uri.set_match_kind(uri_match(value));
        }
        FormKey::FieldType(index) => {
            let Some(field) = item.fields.as_mut().and_then(|f| f.get_mut(index)) else {
                return false;
            };
            field.kind = field_kind(value).code();
        }
        FormKey::FieldValue(index) => {
            // A checkbox field: "true" or "false".
            let Some(field) = item.fields.as_mut().and_then(|f| f.get_mut(index)) else {
                return false;
            };
            if field.kind() != FieldKind::Boolean {
                return false;
            }
            field.value = Some(Sensitive::new(value));
        }
        FormKey::CardBrand => card(item).brand = none_if_empty(value),
        FormKey::ExpMonth => card(item).exp_month = none_if_empty(value),
        FormKey::IdTitle => identity(item).title = none_if_empty(value),
        _ => return false,
    }
    true
}

/// The item as it is saved: websites left empty are dropped, whitespace-only
/// values are absent, and the type's own section exists.
#[must_use]
pub fn for_saving(mut item: Item) -> Item {
    fn blank_text(value: &mut Option<String>) {
        if value.as_ref().is_some_and(|v| v.trim().is_empty()) {
            *value = None;
        }
    }
    fn blank_secret(value: &mut Option<Sensitive>) {
        if value.as_ref().is_some_and(|v| v.expose().trim().is_empty()) {
            *value = None;
        }
    }
    blank_secret(&mut item.notes);
    match item.kind {
        ItemType::Login => {
            let login = login(&mut item);
            blank_secret(&mut login.username);
            blank_secret(&mut login.password);
            blank_secret(&mut login.totp);
            let uris = login.uris.get_or_insert_with(Vec::new);
            uris.retain(|uri| uri.uri.as_deref().is_some_and(|u| !u.is_empty()));
        }
        ItemType::Card => {
            let card = card(&mut item);
            blank_text(&mut card.cardholder_name);
            blank_secret(&mut card.number);
            blank_text(&mut card.brand);
            blank_text(&mut card.exp_month);
            blank_text(&mut card.exp_year);
            blank_secret(&mut card.code);
        }
        ItemType::Identity => {
            let identity = identity(&mut item);
            for value in [
                &mut identity.title,
                &mut identity.first_name,
                &mut identity.middle_name,
                &mut identity.last_name,
                &mut identity.username,
                &mut identity.company,
                &mut identity.email,
                &mut identity.phone,
                &mut identity.address1,
                &mut identity.address2,
                &mut identity.address3,
                &mut identity.city,
                &mut identity.state,
                &mut identity.postal_code,
                &mut identity.country,
            ] {
                blank_text(value);
            }
            blank_secret(&mut identity.ssn);
            blank_secret(&mut identity.passport_number);
            blank_secret(&mut identity.license_number);
        }
        ItemType::SshKey => blank_secret(&mut ssh(&mut item).private_key),
        ItemType::SecureNote => {
            item.secure_note.get_or_insert_with(SecureNote::default);
        }
        ItemType::Other(_) => {}
    }
    if let Some(fields) = item.fields.as_mut() {
        for field in fields {
            blank_text(&mut field.name);
            blank_secret(&mut field.value);
        }
    }
    item
}

/// Wipes a value the form no longer uses (it may be a password).
pub fn wipe(mut text: String) {
    use zeroize::Zeroize;
    text.zeroize();
}

/// Inserts `text` at the character index `at`, wiping the old buffer when
/// the string has to grow, so a password typed one key at a time leaves no
/// shorter copies of itself in freed memory.
pub fn insert(value: &mut String, at: usize, text: &str) {
    let byte = value.char_indices().nth(at).map_or(value.len(), |(index, _)| index);
    if value.capacity() < value.len() + text.len() {
        let mut grown = String::with_capacity((value.len() + text.len()).max(32) * 2);
        grown.push_str(value);
        wipe(std::mem::replace(value, grown));
    }
    value.insert_str(byte, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bw::models::Field;

    fn cache() -> VaultCache {
        let mut cache = VaultCache::default();
        let folder = crate::bw::models::Folder {
            id: Some("f1".into()),
            name: "Work".into(),
            ..Default::default()
        };
        cache.load(vec![], vec![folder], vec![], vec![]);
        cache
    }

    #[test]
    fn rows_read_and_write_the_item() {
        let cache = cache();
        let mut item = Item::new(ItemType::Login);
        assert_eq!(value(&item, FormKey::Password, &cache), "");
        text_mut(&mut item, FormKey::Password).unwrap().push_str("hunter2");
        assert_eq!(item.login.as_ref().unwrap().password.as_ref().unwrap().expose(), "hunter2");
        assert_eq!(value(&item, FormKey::Password, &cache), "hunter2");

        assert!(set(&mut item, FormKey::Folder, "Work", &cache));
        assert_eq!(item.folder_id.as_deref(), Some("f1"));
        assert_eq!(value(&item, FormKey::Folder, &cache), "Work");
        assert!(set(&mut item, FormKey::Favorite, "true", &cache));
        assert!(item.favorite);
        assert!(!set(&mut item, FormKey::UriAdd, "x", &cache));
    }

    #[test]
    fn an_emptied_value_is_absent_again() {
        let cache = cache();
        let mut item = Item::new(ItemType::Login);
        set(&mut item, FormKey::Username, "me", &cache);
        set(&mut item, FormKey::Username, "", &cache);
        assert_eq!(item.login.as_ref().unwrap().username, None);
    }

    #[test]
    fn a_card_number_names_its_brand() {
        let cache = cache();
        let mut item = Item::new(ItemType::Card);
        set(&mut item, FormKey::CardNumber, "4111111111111111", &cache);
        assert_eq!(item.card.as_ref().unwrap().brand.as_deref(), Some("Visa"));
    }

    #[test]
    fn saving_drops_empty_websites_and_blank_values() {
        let mut item = Item::new(ItemType::Login);
        let login = item.login.get_or_insert_with(Login::default);
        login.uris = Some(vec![LoginUri::new(""), LoginUri::new("a.com")]);
        login.username = Some(Sensitive::new("  "));
        item.fields = Some(vec![Field { name: Some(" ".into()), ..Default::default() }]);
        let saved = for_saving(item);
        let login = saved.login.unwrap();
        assert_eq!(login.uris.unwrap().len(), 1);
        assert_eq!(login.username, None);
        assert_eq!(saved.fields.unwrap()[0].name, None);
    }

    #[test]
    fn insert_counts_characters() {
        let mut value = "çé".to_owned();
        insert(&mut value, 1, "x");
        assert_eq!(value, "çxé");
        insert(&mut value, 9, "!");
        assert_eq!(value, "çxé!");
    }
}
