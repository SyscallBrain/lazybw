//! Duplicate detection for imports (design.md §4.12).
//!
//! Two items are duplicates when they have the same type and normalized name
//! and, by type, the same username and registrable domain (login), note text
//! hash (note), last four digits (card) or full name and email (identity).

use crate::bw::models::{Item, ItemType};
use crate::vault::cache::VaultCache;
use std::collections::HashSet;

/// One possible duplicate in the imported file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    /// Index in the imported item list.
    pub index: usize,
    /// Item name.
    pub name: String,
    /// Item type.
    pub kind: ItemType,
    /// A short description (username, domain, …).
    pub detail: String,
}

/// Finds imported items that already exist in the vault.
#[must_use]
pub fn find(existing: &VaultCache, imported: &[Item]) -> Vec<Duplicate> {
    let existing_keys: HashSet<String> = existing.active_items().map(identity_key).collect();
    imported
        .iter()
        .enumerate()
        .filter(|(_, item)| existing_keys.contains(&identity_key(item)))
        .map(|(index, item)| Duplicate {
            index,
            name: item.name.clone(),
            kind: item.kind,
            detail: detail(item),
        })
        .collect()
}

/// The normalised identity used to compare items.
#[must_use]
pub fn identity_key(item: &Item) -> String {
    let name = normalize(&item.name);
    match item.kind {
        ItemType::Login => {
            let login = item.login.as_ref();
            let username = login
                .and_then(|l| l.username.as_ref())
                .map(|u| normalize(u.expose()))
                .unwrap_or_default();
            let domain =
                login.and_then(|l| l.first_uri()).and_then(registrable_domain).unwrap_or_default();
            format!("login|{name}|{username}|{domain}")
        }
        ItemType::SecureNote => {
            let notes = item.notes.as_ref().map(|n| normalize(n.expose())).unwrap_or_default();
            format!("note|{name}|{notes}")
        }
        ItemType::Card => {
            let digits = item
                .card
                .as_ref()
                .and_then(|c| c.number.as_ref())
                .map(|n| n.expose().chars().filter(char::is_ascii_digit).collect::<String>())
                .unwrap_or_default();
            let last4: String = digits.chars().skip(digits.len().saturating_sub(4)).collect();
            format!("card|{name}|{last4}")
        }
        ItemType::Identity => {
            let identity = item.identity.as_ref();
            let full = identity.map(|i| normalize(&i.full_name())).unwrap_or_default();
            let email =
                identity.and_then(|i| i.email.as_deref()).map(normalize).unwrap_or_default();
            format!("identity|{name}|{full}|{email}")
        }
        ItemType::SshKey => {
            let public =
                item.ssh_key.as_ref().map(|k| k.public_key.trim().to_owned()).unwrap_or_default();
            format!("ssh|{name}|{public}")
        }
        ItemType::Other(code) => format!("other{code}|{name}"),
    }
}

fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}

/// The registrable domain of a URI (`https://gist.github.com` → `github.com`).
#[must_use]
pub fn registrable_domain(uri: &str) -> Option<String> {
    let parsed = url::Url::parse(uri).ok();
    let host = parsed.as_ref().and_then(url::Url::host_str).map(str::to_owned).or_else(|| {
        url::Url::parse(&format!("https://{uri}"))
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
    })?;
    Some(psl::domain_str(&host).unwrap_or(&host).to_ascii_lowercase())
}

fn detail(item: &Item) -> String {
    match item.kind {
        ItemType::Login => item
            .login
            .as_ref()
            .and_then(|l| l.username.as_ref())
            .map(|u| u.expose().to_owned())
            .or_else(|| {
                item.login.as_ref().and_then(|l| l.first_uri()).and_then(registrable_domain)
            })
            .unwrap_or_default(),
        ItemType::Card => item.subtitle(),
        ItemType::Identity => item.identity.as_ref().map(|i| i.full_name()).unwrap_or_default(),
        ItemType::SecureNote => {
            item.notes.as_ref().map(|n| n.expose().to_owned()).unwrap_or_default()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bw::models::{Card, Identity, Login, LoginUri};
    use crate::bw::sensitive::Sensitive;

    fn login(name: &str, user: &str, uri: &str) -> Item {
        let mut item = Item::new(ItemType::Login);
        item.id = Some(name.to_owned());
        item.name = name.to_owned();
        item.login = Some(Login {
            username: Some(Sensitive::new(user)),
            uris: Some(vec![LoginUri::new(uri)]),
            ..Login::default()
        });
        item
    }

    #[test]
    fn detects_login_duplicates_by_domain() {
        let existing = login("GitHub", "dev", "https://gist.github.com/x");
        let mut cache = VaultCache::new();
        cache.load(vec![existing], vec![], vec![], vec![]);
        let imported = vec![
            login("github", "DEV", "https://github.com"),
            login("GitHub", "other", "https://github.com"),
            login("GitLab", "dev", "https://gitlab.com"),
        ];
        let duplicates = find(&cache, &imported);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].index, 0);
    }

    #[test]
    fn detects_note_duplicates_by_text() {
        let mut a = Item::new(ItemType::SecureNote);
        a.id = Some("a".into());
        a.name = "Router".into();
        a.notes = Some(Sensitive::new("admin / hunter2"));
        let mut cache = VaultCache::new();
        cache.load(vec![a], vec![], vec![], vec![]);

        let mut b = Item::new(ItemType::SecureNote);
        b.name = "router ".into();
        b.notes = Some(Sensitive::new("admin / hunter2"));
        assert_eq!(find(&cache, &[b]).len(), 1);
    }

    #[test]
    fn detects_card_last_four() {
        let mut a = Item::new(ItemType::Card);
        a.id = Some("a".into());
        a.name = "Visa".into();
        a.card = Some(Card { number: Some(Sensitive::new("4242424242424242")), ..Card::default() });
        let mut cache = VaultCache::new();
        cache.load(vec![a], vec![], vec![], vec![]);
        let mut b = Item::new(ItemType::Card);
        b.name = "Visa".into();
        b.card =
            Some(Card { number: Some(Sensitive::new("5555 5555 5555 4242")), ..Card::default() });
        assert_eq!(find(&cache, &[b]).len(), 1);
    }

    #[test]
    fn detects_identity_by_name_and_email() {
        let mut a = Item::new(ItemType::Identity);
        a.id = Some("a".into());
        a.name = "Me".into();
        a.identity = Some(Identity {
            first_name: Some("Jane".into()),
            last_name: Some("Example".into()),
            email: Some("jane@example.com".into()),
            ..Identity::default()
        });
        let mut cache = VaultCache::new();
        cache.load(vec![a], vec![], vec![], vec![]);
        let mut b = Item::new(ItemType::Identity);
        b.name = "Me".into();
        b.identity = Some(Identity {
            first_name: Some("jane".into()),
            last_name: Some("example".into()),
            email: Some("JANE@example.com".into()),
            ..Identity::default()
        });
        assert_eq!(find(&cache, &[b]).len(), 1);
    }

    #[test]
    fn registrable_domains() {
        assert_eq!(registrable_domain("https://gist.github.com/x").as_deref(), Some("github.com"));
        assert_eq!(
            registrable_domain("https://www.example.co.uk").as_deref(),
            Some("example.co.uk")
        );
        assert_eq!(registrable_domain("example.com").as_deref(), Some("example.com"));
        assert_eq!(registrable_domain("not a url"), None);
        assert_eq!(registrable_domain("localhost:8080"), Some("localhost".into()));
    }
}
