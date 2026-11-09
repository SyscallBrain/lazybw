//! Vault health: merge `bw report password-health` with the checks lazybw can
//! do locally (reuse, weakness, age, insecure URIs) and compute the score from
//! design.md §4.13.

use crate::bw::models::{HealthEntry, Item, ItemType};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};

/// The health flags of one item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealthFlags {
    /// Seen in a data breach.
    pub exposed: bool,
    /// Password used by another item.
    pub reused: bool,
    /// Weak password.
    pub weak: bool,
    /// Password older than `health.old_password_age`.
    pub old: bool,
    /// At least one `http://` URI.
    pub insecure: bool,
}

impl HealthFlags {
    /// True when any flag is set.
    #[must_use]
    pub fn any(self) -> bool {
        self.exposed || self.reused || self.weak || self.old || self.insecure
    }

    /// The penalty contribution, capped at 1.0.
    #[must_use]
    pub fn penalty(self, strength: u8) -> f64 {
        let mut sum: f64 = 0.0;
        if self.exposed {
            sum += 1.0;
        }
        if self.reused {
            sum += 0.6;
        }
        if self.weak {
            sum += if strength <= 1 { 0.5 } else { 0.25 };
        }
        if self.old {
            sum += 0.15;
        }
        if self.insecure {
            sum += 0.2;
        }
        sum.min(1.0)
    }
}

/// One item's health data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRecord {
    /// Item id.
    pub id: String,
    /// Item name.
    pub name: String,
    /// Flags.
    pub flags: HealthFlags,
    /// Strength 0–4.
    pub strength: u8,
    /// Times seen in breaches.
    pub exposed_count: Option<u64>,
    /// Number of items sharing the password.
    pub reuse_count: Option<u32>,
    /// Error from the HIBP lookup.
    pub exposed_error: Option<String>,
}

/// The whole report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HealthReport {
    /// Records by item id.
    pub records: BTreeMap<String, HealthRecord>,
    /// Items with each problem.
    pub exposed: usize,
    /// Items with each problem.
    pub reused: usize,
    /// Items with each problem.
    pub weak: usize,
    /// Items with each problem.
    pub old: usize,
    /// Items with each problem.
    pub insecure: usize,
    /// Number of logins checked (those with a password).
    pub checked: usize,
    /// Score 0–100.
    pub score: u8,
    /// Score without exposed and reused passwords.
    pub potential: u8,
    /// True when `bw report` supplied exposure data.
    pub has_exposed_data: bool,
    /// The ids of each category, worst first, in `HealthCategory::ALL`
    /// order. Worked out once by `build`: the health screen and the
    /// statusline ask for a category on every frame.
    ordered: [Vec<String>; 5],
    /// Display width for each category's name column, computed once.
    name_widths: [usize; 5],
}

impl HealthReport {
    /// The record for an item.
    #[must_use]
    pub fn entry(&self, id: &str) -> Option<&HealthRecord> {
        self.records.get(id)
    }

    /// The flags for an item.
    #[must_use]
    pub fn flags(&self, id: &str) -> HealthFlags {
        self.records.get(id).map(|r| r.flags).unwrap_or_default()
    }

    /// Items in a category, worst first.
    #[must_use]
    pub fn items_in(&self, category: HealthCategory) -> Vec<&HealthRecord> {
        self.items_from(category, 0).collect()
    }

    /// Records starting at a category offset. Skips ids before looking up
    /// records, so drawing a scrolled window only visits visible items.
    pub fn items_from(
        &self,
        category: HealthCategory,
        offset: usize,
    ) -> impl Iterator<Item = &HealthRecord> {
        self.ordered[category as usize].iter().skip(offset).filter_map(|id| self.records.get(id))
    }

    /// The record at an index in a category, without allocating its list.
    #[must_use]
    pub fn item_in(&self, category: HealthCategory, index: usize) -> Option<&HealthRecord> {
        self.ordered[category as usize].get(index).and_then(|id| self.records.get(id))
    }

    /// Width of the category's name column, in terminal columns (12–36).
    #[must_use]
    pub fn name_width_in(&self, category: HealthCategory) -> usize {
        self.name_widths[category as usize].clamp(12, 36)
    }

    /// The number of items in a category.
    #[must_use]
    pub fn len_in(&self, category: HealthCategory) -> usize {
        self.ordered[category as usize].len()
    }
}

/// True when a record belongs to a category.
fn in_category(record: &HealthRecord, category: HealthCategory) -> bool {
    match category {
        HealthCategory::Exposed => record.flags.exposed,
        HealthCategory::Reused => record.flags.reused,
        HealthCategory::Weak => record.flags.weak,
        HealthCategory::Old => record.flags.old,
        HealthCategory::Insecure => record.flags.insecure,
    }
}

/// The ids of each category, most exposures first, then by name.
fn order_by_category(records: &BTreeMap<String, HealthRecord>) -> ([Vec<String>; 5], [usize; 5]) {
    let mut widths = [0; 5];
    let mut sorted: Vec<(&HealthRecord, String)> = records
        .values()
        .filter(|record| record.flags.any())
        .map(|record| {
            let width = crate::util::text::width(&record.name);
            for category in HealthCategory::ALL {
                if in_category(record, category) {
                    widths[category as usize] = widths[category as usize].max(width);
                }
            }
            (record, record.name.to_lowercase())
        })
        .collect();
    sorted.sort_by(|(a, a_name), (b, b_name)| {
        b.exposed_count
            .unwrap_or(0)
            .cmp(&a.exposed_count.unwrap_or(0))
            .then_with(|| a_name.cmp(b_name))
    });
    let ordered = HealthCategory::ALL.map(|category| {
        sorted
            .iter()
            .filter(|(record, _)| in_category(record, category))
            .map(|(record, _)| record.id.clone())
            .collect()
    });
    (ordered, widths)
}

/// The five categories shown as cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum HealthCategory {
    /// Breached passwords.
    #[default]
    Exposed,
    /// Reused passwords.
    Reused,
    /// Weak passwords.
    Weak,
    /// Old passwords.
    Old,
    /// Insecure URIs.
    Insecure,
}

impl HealthCategory {
    /// All categories in card order.
    pub const ALL: [Self; 5] = [Self::Exposed, Self::Reused, Self::Weak, Self::Old, Self::Insecure];

    /// Label on the card.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Exposed => "exposed",
            Self::Reused => "reused",
            Self::Weak => "weak",
            Self::Old => "over 1 year",
            Self::Insecure => "no HTTPS",
        }
    }

    /// The count in a report.
    #[must_use]
    pub fn count(self, report: &HealthReport) -> usize {
        match self {
            Self::Exposed => report.exposed,
            Self::Reused => report.reused,
            Self::Weak => report.weak,
            Self::Old => report.old,
            Self::Insecure => report.insecure,
        }
    }
}

/// Builds the report. When `bw_entries` is empty and `local_fallback` is true,
/// weakness and reuse are computed locally with zxcvbn.
#[must_use]
pub fn build<T: std::borrow::Borrow<Item>>(
    items: &[T],
    bw_entries: &[HealthEntry],
    now: DateTime<Utc>,
    old_age_days: i64,
    local_fallback: bool,
) -> HealthReport {
    let logins: Vec<&Item> = items
        .iter()
        .map(std::borrow::Borrow::borrow)
        .filter(|item| item.kind == ItemType::Login)
        .filter(|item| {
            item.login.as_ref().and_then(|l| l.password.as_ref()).is_some_and(|p| !p.is_empty())
        })
        .collect();

    // Reuse counts by password.
    let mut password_counts: HashMap<&str, usize> = HashMap::new();
    for item in &logins {
        if let Some(password) = item.login.as_ref().and_then(|l| l.password.as_ref()) {
            *password_counts.entry(password.expose()).or_insert(0) += 1;
        }
    }

    let bw_by_id: BTreeMap<&str, &HealthEntry> =
        bw_entries.iter().map(|entry| (entry.id.as_str(), entry)).collect();

    let mut records = BTreeMap::new();
    let mut totals = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut penalty_sum = 0.0;
    let mut potential_sum = 0.0;
    let mut has_exposed_data = false;
    // Reused passwords need one zxcvbn evaluation per report. Keys borrow
    // the existing Sensitive values; nothing is copied or retained after
    // this call, and server scores still override each item's local score.
    let mut local_strengths = HashMap::new();

    for item in &logins {
        let Some(id) = &item.id else { continue };
        // `logins` only holds items with a password, but this runs on data
        // from the server: skip rather than panic if that ever changes.
        let Some(login) = item.login.as_ref() else { continue };
        let Some(password) = login.password.as_ref() else { continue };
        let bw = bw_by_id.get(id.as_str()).copied();

        let strength = bw.map_or_else(
            || {
                if local_fallback {
                    *local_strengths
                        .entry(password.expose())
                        .or_insert_with(|| local_strength(password.expose()))
                } else {
                    3
                }
            },
            |entry| entry.password_strength,
        );
        let reuse_count = bw.and_then(|e| e.reuse_count).or_else(|| {
            let count = *password_counts.get(password.expose()).unwrap_or(&1);
            (count > 1).then_some(count as u32)
        });
        let exposed = bw.and_then(|e| e.exposed).unwrap_or(false);
        if bw.and_then(|e| e.exposed).is_some() {
            has_exposed_data = true;
        }
        let weak = if bw.is_some() {
            strength <= 1
        } else if local_fallback {
            strength <= 1 || password.expose().chars().count() < 8
        } else {
            false
        };
        let old = is_old(item, now, old_age_days);
        let insecure = login
            .uris
            .as_ref()
            .is_some_and(|uris| uris.iter().any(|uri| uri.uri_str().starts_with("http://")));
        let flags = HealthFlags {
            exposed,
            reused: reuse_count.is_some_and(|count| count > 1),
            weak,
            old,
            insecure,
        };
        if flags.exposed {
            totals.0 += 1;
        }
        if flags.reused {
            totals.1 += 1;
        }
        if flags.weak {
            totals.2 += 1;
        }
        if flags.old {
            totals.3 += 1;
        }
        if flags.insecure {
            totals.4 += 1;
        }
        penalty_sum += flags.penalty(strength);
        let relaxed = HealthFlags { exposed: false, reused: false, ..flags };
        potential_sum += relaxed.penalty(strength);
        records.insert(
            id.clone(),
            HealthRecord {
                id: id.clone(),
                name: item.name.clone(),
                flags,
                strength,
                exposed_count: bw.and_then(|e| e.exposed_count),
                reuse_count,
                exposed_error: bw.and_then(|e| e.exposed_error.clone()),
            },
        );
    }

    let checked = logins.len();
    let score = score_from(penalty_sum, checked);
    let potential = score_from(potential_sum, checked);
    let (ordered, name_widths) = order_by_category(&records);
    HealthReport {
        records,
        ordered,
        name_widths,
        exposed: totals.0,
        reused: totals.1,
        weak: totals.2,
        old: totals.3,
        insecure: totals.4,
        checked,
        score,
        potential,
        has_exposed_data,
    }
}

fn score_from(penalty_sum: f64, checked: usize) -> u8 {
    if checked == 0 {
        return 100;
    }
    let score = (100.0 * (1.0 - penalty_sum / checked as f64)).round();
    score.clamp(0.0, 100.0) as u8
}

fn is_old(item: &Item, now: DateTime<Utc>, old_age_days: i64) -> bool {
    let date = item
        .login
        .as_ref()
        .and_then(|l| l.password_revision_date.as_deref())
        .or(item.creation_date.as_deref());
    let Some(date) = date.and_then(crate::util::time::parse_iso) else {
        return false;
    };
    (now - date).num_days() > old_age_days
}

/// Local strength estimate with zxcvbn, mapped to the server's 0–4 scale.
#[must_use]
pub fn local_strength(password: &str) -> u8 {
    if password.is_empty() {
        return 0;
    }
    // The detail pane, the editor and the generator ask for the strength of
    // the same password on every frame, and zxcvbn takes tens of
    // microseconds. Keep the last answer, keyed by a hash with a random
    // per-process key (never the password, never a hash anyone could
    // precompute), and forget it on lock (`forget_strength_cache`).
    let key = strength_key(password);
    if let Some((cached, score)) = LAST_STRENGTH.with(std::cell::Cell::get)
        && cached == key
    {
        return score;
    }
    let score = u8::from(zxcvbn::zxcvbn(password, &[]).score());
    LAST_STRENGTH.with(|cell| cell.set(Some((key, score))));
    score
}

thread_local! {
    /// The last `(keyed hash, strength)` pair computed on this thread.
    static LAST_STRENGTH: std::cell::Cell<Option<(u64, u8)>> =
        const { std::cell::Cell::new(None) };
}

fn strength_key(password: &str) -> u64 {
    use std::hash::BuildHasher;
    static KEY: std::sync::OnceLock<std::collections::hash_map::RandomState> =
        std::sync::OnceLock::new();
    KEY.get_or_init(std::collections::hash_map::RandomState::new).hash_one(password)
}

/// Drops the cached strength of the last password measured on this thread.
pub fn forget_strength_cache() {
    LAST_STRENGTH.with(|cell| cell.set(None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bw::models::Login;
    use crate::bw::sensitive::Sensitive;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, 14, 0, 0).unwrap()
    }

    fn login(id: &str, password: &str) -> Item {
        let mut item = Item::new(ItemType::Login);
        item.id = Some(id.to_owned());
        item.name = format!("Item {id}");
        item.creation_date = Some("2024-01-01T00:00:00Z".into());
        item.login = Some(Login { password: Some(Sensitive::new(password)), ..Login::default() });
        item
    }

    #[test]
    fn reused_password_strengths_stay_local_and_server_overrides_stay_per_item() {
        let passwords = ["password", "Xk9#mQ2!vR7$pL4z", "abc", "another distinct phrase !42"];
        let items: Vec<_> =
            (0..20).map(|n| login(&n.to_string(), passwords[n % passwords.len()])).collect();
        let entries = [HealthEntry { id: "0".into(), password_strength: 4, ..Default::default() }];
        let report = build(&items, &entries, now(), 365, true);
        for (n, item) in items.iter().enumerate() {
            let record = &report.records[item.id.as_ref().unwrap()];
            let expected =
                if n == 0 { 4 } else { u8::from(zxcvbn::zxcvbn(passwords[n % 4], &[]).score()) };
            assert_eq!(record.strength, expected, "item {n}");
            assert_eq!(record.reuse_count, Some(5));
        }
        // A later analysis must recompute for the current password.
        let changed = [login("0", "123")];
        let fresh = build(&changed, &[], now(), 365, true);
        assert_eq!(fresh.records["0"].strength, 0);
        assert_eq!(fresh.records["0"].reuse_count, None);
    }

    #[test]
    fn category_windows_and_widths_match_the_full_list() {
        let mut items = vec![login("a", "123456"), login("b", "123456"), login("c", "password")];
        items[0].name = "界界界界界界界".into(); // 14 terminal columns
        items[1].name = "e\u{301}".repeat(18); // combining marks take no extra columns
        let report = build(&items, &[], now(), 365, true);
        assert_eq!(report.name_width_in(HealthCategory::Weak), 18);
        assert_eq!(report.name_width_in(HealthCategory::Reused), 18);
        assert_eq!(report.name_width_in(HealthCategory::Exposed), 12);
        for category in HealthCategory::ALL {
            let full = report.items_in(category);
            for offset in 0..=full.len() + 1 {
                let actual: Vec<_> = report.items_from(category, offset).take(2).collect();
                let expected: Vec<_> = full.iter().copied().skip(offset).take(2).collect();
                assert_eq!(actual, expected);
                assert_eq!(report.item_in(category, offset), full.get(offset).copied());
            }
        }
    }

    #[test]
    fn local_reuse_and_weakness() {
        let items =
            vec![login("a", "password"), login("b", "password"), login("c", "Xk9#mQ2!vR7$pL4z")];
        let report = build(&items, &[], now(), 365, true);
        assert_eq!(report.checked, 3);
        assert_eq!(report.reused, 2);
        assert!(report.weak >= 2);
        assert_eq!(report.old, 3);
        assert!(report.score < 100);
    }

    #[test]
    fn categories_index_the_ordered_lists() {
        for (index, category) in HealthCategory::ALL.into_iter().enumerate() {
            assert_eq!(category as usize, index);
        }
    }

    #[test]
    fn categories_list_most_exposed_first_then_by_name() {
        let items = vec![login("b", "123456"), login("a", "123456"), login("c", "password")];
        let entries: Vec<HealthEntry> = [("a", 2), ("b", 9), ("c", 2)]
            .into_iter()
            .map(|(id, count)| HealthEntry {
                id: id.into(),
                exposed: Some(true),
                exposed_count: Some(count),
                ..HealthEntry::default()
            })
            .collect();
        let report = build(&items, &entries, now(), 365, false);
        let names =
            |category| report.items_in(category).iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        assert_eq!(names(HealthCategory::Exposed), ["b", "a", "c"]);
        assert_eq!(report.len_in(HealthCategory::Exposed), 3);
    }

    #[test]
    fn strength_cache_agrees_with_a_fresh_measurement() {
        let weak = local_strength("123456");
        let strong = local_strength("Xk9#mQ2!vR7$pL4z");
        assert_eq!(local_strength("123456"), weak);
        assert_eq!(local_strength("Xk9#mQ2!vR7$pL4z"), strong);
        assert!(weak < strong);
        forget_strength_cache();
        assert_eq!(LAST_STRENGTH.with(std::cell::Cell::get), None);
        assert_eq!(local_strength("Xk9#mQ2!vR7$pL4z"), strong);
        assert_eq!(local_strength(""), 0);
    }

    #[test]
    fn bw_entries_win_over_local_strength() {
        let items = vec![login("a", "abc")];
        let entries = vec![HealthEntry {
            id: "a".into(),
            name: "Item a".into(),
            password_strength: 4,
            exposed: Some(true),
            exposed_count: Some(3812),
            ..HealthEntry::default()
        }];
        let report = build(&items, &entries, now(), 365, false);
        assert_eq!(report.exposed, 1);
        assert!(!report.records["a"].flags.weak);
        assert_eq!(report.records["a"].exposed_count, Some(3812));
        assert_eq!(report.score, 0);
        assert!(report.has_exposed_data);
    }

    #[test]
    fn potential_ignores_exposed_and_reused() {
        let mut item = login("a", "shared");
        item.login.as_mut().unwrap().password = Some(Sensitive::new("shared"));
        item.creation_date = Some("2026-09-01T00:00:00Z".into());
        item.favorite = false;
        let items = vec![item];
        let entries = vec![HealthEntry {
            id: "a".into(),
            exposed: Some(true),
            password_strength: 4,
            ..HealthEntry::default()
        }];
        let report = build(&items, &entries, now(), 365, false);
        assert_eq!(report.score, 0);
        assert_eq!(report.potential, 100);
    }

    #[test]
    fn insecure_uri_is_flagged() {
        let mut item = login("a", "Xk9#mQ2!vR7$pL4z");
        item.login.as_mut().unwrap().uris =
            Some(vec![crate::bw::models::LoginUri::new("http://x.example")]);
        let report = build(&[item], &[], now(), 365, false);
        assert_eq!(report.insecure, 1);
    }

    #[test]
    fn categories_list_items() {
        let items = vec![login("a", "short")];
        let report = build(&items, &[], now(), 365, true);
        assert!(!report.items_in(HealthCategory::Weak).is_empty());
        assert!(report.items_in(HealthCategory::Exposed).is_empty());
        for category in HealthCategory::ALL {
            assert!(!category.label().is_empty());
        }
    }

    #[test]
    fn empty_vault_scores_100() {
        let report = build(&[] as &[Item], &[], now(), 365, true);
        assert_eq!(report.score, 100);
        assert_eq!(report.checked, 0);
    }
}
