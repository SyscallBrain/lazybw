//! The in-memory vault cache: items by id, precomputed row strings and counts.
//!
//! Rows precompute everything the list needs (display name, subtitle,
//! indicators, folder path, sort keys) so a frame never allocates per item.

use crate::bw::models::{Folder, Item, ItemType};
use crate::config::SortOrder;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Which items the list shows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Filter {
    /// Every active item.
    All,
    /// Favourites.
    Favorites,
    /// One item type.
    Type(ItemType),
    /// One folder, by full path (its subtree is included).
    Folder(String),
    /// Items without a folder.
    NoFolder,
    /// Trashed items.
    Trash,
    /// Archived items (read-only).
    Archive,
}

impl Filter {
    /// Title for the list pane.
    #[must_use]
    pub fn title(&self) -> &'static str {
        match self {
            Self::All => "All items",
            Self::Favorites => "Favorites",
            Self::Type(kind) => kind.plural(),
            Self::Folder(_) => "Folder",
            Self::NoFolder => "No folder",
            Self::Trash => "Trash",
            Self::Archive => "Archive",
        }
    }

    /// The list title: the folder path for a folder ("Finanças › Bancos"),
    /// otherwise the filter name.
    #[must_use]
    pub fn display_title(&self) -> String {
        match self {
            // One allocation instead of a `Vec` of segments plus a `join`:
            // this runs on every frame, in the list title.
            Self::Folder(path) => path.replace('/', " › "),
            other => other.title().to_owned(),
        }
    }

    /// True for the trash view.
    #[must_use]
    pub fn is_trash(&self) -> bool {
        matches!(self, Self::Trash)
    }

    /// True for the archive view.
    #[must_use]
    pub fn is_archive(&self) -> bool {
        matches!(self, Self::Archive)
    }

    /// The folder id, for a folder filter.
    #[must_use]
    pub fn folder_id(&self) -> Option<&str> {
        match self {
            Self::Folder(id) => Some(id),
            _ => None,
        }
    }
}

/// The precomputed data for one row.
#[derive(Debug, Clone)]
pub struct Row {
    /// Item id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Lowercase name, for sorting and search.
    pub name_lower: String,
    /// Secondary line.
    pub subtitle: String,
    /// Type.
    pub kind: ItemType,
    /// Favourite.
    pub favorite: bool,
    /// Has a TOTP secret.
    pub totp: bool,
    /// Has passkeys.
    pub passkey: bool,
    /// Number of attachments.
    pub attachments: usize,
    /// Master password re-prompt.
    pub reprompt: bool,
    /// Folder id.
    pub folder_id: Option<String>,
    /// Full folder path (`Work/Projects`).
    pub folder_path: String,
    /// Trashed.
    pub deleted: bool,
    /// Archived.
    pub archived: bool,
    /// Sort key for modified.
    pub modified: i64,
    /// Sort key for created.
    pub created: i64,
    /// Health flags from the latest run.
    pub exposed: bool,
    /// Reused password.
    pub reused: bool,
    /// Weak password.
    pub weak: bool,
    /// Old password.
    pub old: bool,
    /// Insecure URI.
    pub insecure: bool,
    /// The registrable domain of the first website, worked out the first
    /// time a frame needs it (the brand icon) and kept with the row. An
    /// edit builds a new row, so it never goes stale.
    pub domain: std::sync::OnceLock<Option<String>>,
}

impl Row {
    /// True when any health flag is set.
    #[must_use]
    pub fn has_health_issue(&self) -> bool {
        self.exposed || self.reused || self.weak || self.old || self.insecure
    }

    /// The worst health severity, used for the warning colour.
    #[must_use]
    pub fn health_severity(&self) -> Option<HealthSeverity> {
        if self.exposed {
            Some(HealthSeverity::Exposed)
        } else if self.reused || self.weak {
            Some(HealthSeverity::Warning)
        } else if self.old || self.insecure {
            Some(HealthSeverity::Notice)
        } else {
            None
        }
    }
}

/// Health severity of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthSeverity {
    /// Password seen in a breach.
    Exposed,
    /// Reused or weak.
    Warning,
    /// Old or insecure URI.
    Notice,
}

/// Per-filter counts, precomputed on load.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    /// All active items.
    pub all: usize,
    /// Favourites.
    pub favorites: usize,
    /// Per type.
    pub by_type: BTreeMap<ItemType, usize>,
    /// Per folder path (including descendants).
    pub by_folder: BTreeMap<String, usize>,
    /// Items without a folder.
    pub no_folder: usize,
    /// Trashed items.
    pub trash: usize,
    /// Archived items.
    pub archived: usize,
}

/// The vault cache.
#[derive(Debug, Default, Clone)]
pub struct VaultCache {
    active: BTreeMap<String, Item>,
    trash: BTreeMap<String, Item>,
    archived: BTreeMap<String, Item>,
    folders: Vec<Folder>,
    folder_paths: BTreeMap<String, String>,
    rows: BTreeMap<String, Arc<Row>>,
    counts: Counts,
    version: u64,
}

impl VaultCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A counter that changes whenever the contents change; used to
    /// invalidate derived views.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Replaces the cache contents, recomputing rows and counts.
    pub fn load(
        &mut self,
        items: Vec<Item>,
        folders: Vec<Folder>,
        trash: Vec<Item>,
        archived: Vec<Item>,
    ) {
        self.active = items.into_iter().filter_map(|i| i.id.clone().map(|id| (id, i))).collect();
        self.trash = trash.into_iter().filter_map(|i| i.id.clone().map(|id| (id, i))).collect();
        self.archived =
            archived.into_iter().filter_map(|i| i.id.clone().map(|id| (id, i))).collect();
        // `bw` lists a pseudo folder "No Folder" with a null id; lazybw has
        // its own No folder filter, so drop it.
        self.folders = folders.into_iter().filter(|f| f.id.is_some()).collect();
        self.reindex();
    }

    /// Drops everything (on lock).
    pub fn clear(&mut self) {
        self.version += 1;
        self.active.clear();
        self.trash.clear();
        self.archived.clear();
        self.folders.clear();
        self.folder_paths.clear();
        self.rows.clear();
        self.counts = Counts::default();
    }

    /// True when no items and no folders are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty() && self.trash.is_empty() && self.archived.is_empty()
    }

    /// Number of active items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// The counts.
    #[must_use]
    pub fn counts(&self) -> &Counts {
        &self.counts
    }

    /// The folders.
    #[must_use]
    pub fn folders(&self) -> &[Folder] {
        &self.folders
    }

    /// A folder by id.
    #[must_use]
    pub fn folder(&self, id: &str) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id.as_deref() == Some(id))
    }

    /// The full path of a folder id.
    #[must_use]
    pub fn folder_path(&self, id: &str) -> Option<&str> {
        self.folder_paths.get(id).map(String::as_str)
    }

    /// An item by id, from any of the three lists.
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.active.get(id).or_else(|| self.trash.get(id)).or_else(|| self.archived.get(id))
    }

    /// A mutable item by id, searching the active list first.
    pub fn item_mut(&mut self, id: &str) -> Option<&mut Item> {
        if self.active.contains_key(id) {
            return self.active.get_mut(id);
        }
        if self.trash.contains_key(id) {
            return self.trash.get_mut(id);
        }
        self.archived.get_mut(id)
    }

    /// A row by id.
    #[must_use]
    pub fn row(&self, id: &str) -> Option<&Row> {
        self.rows.get(id).map(Arc::as_ref)
    }

    /// Removes an item from every list.
    pub fn remove(&mut self, id: &str) -> Option<Item> {
        self.rows.remove(id);
        let removed = if let Some(item) = self.active.remove(id) {
            count_subtract(&mut self.counts, &self.folder_paths, &item);
            Some(item)
        } else {
            self.trash.remove(id).or_else(|| self.archived.remove(id))
        };
        if removed.is_some() {
            self.counts.trash = self.trash.len();
            self.counts.archived = self.archived.len();
            self.version += 1;
        }
        removed
    }

    /// Inserts or replaces an item, keeping it in the right list.
    ///
    /// Only the affected row and the counters change: editing one item in a
    /// 5,000-item vault must not rebuild the other 4,999 rows.
    pub fn upsert(&mut self, item: Item) {
        let Some(id) = item.id.clone() else { return };
        // The health marks of the previous row stay valid while the password
        // is the same (a favourite or a move must not hide a weak-password
        // warning until the next health run).
        fn password(item: &Item) -> Option<&str> {
            item.login.as_ref().and_then(|l| l.password.as_ref()).map(|p| p.expose())
        }
        let kept_health = self
            .active
            .get(&id)
            .or_else(|| self.trash.get(&id))
            .or_else(|| self.archived.get(&id))
            .filter(|old| password(old) == password(&item))
            .and_then(|_| self.rows.get(&id).cloned());
        // Remove the previous version. Only active items contribute to the
        // per-filter counters; trash and archive are counted by length.
        if let Some(old) = self.active.remove(&id) {
            count_subtract(&mut self.counts, &self.folder_paths, &old);
        }
        self.trash.remove(&id);
        self.archived.remove(&id);
        let deleted = item.is_deleted();
        let archived = item.is_archived();
        if deleted {
            self.trash.insert(id.clone(), item);
        } else if archived {
            self.archived.insert(id.clone(), item);
        } else {
            self.active.insert(id.clone(), item);
        }
        let stored = if let Some(stored) = self.active.get(&id) {
            count_add(&mut self.counts, &self.folder_paths, stored);
            Some(stored)
        } else {
            self.trash.get(&id).or_else(|| self.archived.get(&id))
        };
        if let Some(stored) = stored {
            let mut row = Row::from_item(stored, &self.folder_paths);
            if let Some(previous) = kept_health {
                row.exposed = previous.exposed;
                row.reused = previous.reused;
                row.weak = previous.weak;
                row.old = previous.old;
                row.insecure = previous.insecure;
            }
            self.rows.insert(id, Arc::new(row));
        }
        self.counts.trash = self.trash.len();
        self.counts.archived = self.archived.len();
        self.version += 1;
    }

    /// Replaces the folder list and recomputes paths and rows.
    pub fn set_folders(&mut self, folders: Vec<Folder>) {
        self.folders = folders.into_iter().filter(|f| f.id.is_some()).collect();
        self.reindex();
    }

    /// All active items.
    pub fn active_items(&self) -> impl Iterator<Item = &Item> {
        self.active.values()
    }

    /// All trashed items.
    pub fn trash_items(&self) -> impl Iterator<Item = &Item> {
        self.trash.values()
    }

    /// All archived items.
    pub fn archived_items(&self) -> impl Iterator<Item = &Item> {
        self.archived.values()
    }

    /// Items matching a filter, unsorted.
    #[must_use]
    pub fn filter<'a>(&'a self, filter: &Filter) -> Vec<&'a Item> {
        match filter {
            Filter::All => self.active.values().collect(),
            Filter::Favorites => self.active.values().filter(|i| i.favorite).collect(),
            Filter::Type(kind) => self.active.values().filter(|i| i.kind == *kind).collect(),
            Filter::Folder(path) => {
                let prefix = format!("{path}/");
                self.active
                    .values()
                    .filter(|i| {
                        i.folder_id
                            .as_deref()
                            .and_then(|id| self.folder_paths.get(id))
                            .is_some_and(|p| p == path || p.starts_with(&prefix))
                    })
                    .collect()
            }
            Filter::NoFolder => self.active.values().filter(|i| i.folder_id.is_none()).collect(),
            Filter::Trash => self.trash.values().collect(),
            Filter::Archive => self.archived.values().collect(),
        }
    }

    /// Rows matching a filter, in cache order.
    #[must_use]
    pub fn rows_matching(&self, filter: &Filter) -> Vec<Arc<Row>> {
        match filter {
            Filter::Trash => self.rows.values().filter(|r| r.deleted).cloned().collect(),
            Filter::Archive => self.rows.values().filter(|r| r.archived).cloned().collect(),
            other => self
                .rows
                .values()
                .filter(|r| !r.deleted && !r.archived)
                .filter(|r| match other {
                    Filter::All => true,
                    Filter::Favorites => r.favorite,
                    Filter::Type(kind) => r.kind == *kind,
                    // The folder or one of its subfolders, without building
                    // a "path/" string for every row.
                    Filter::Folder(path) => r
                        .folder_path
                        .strip_prefix(path.as_str())
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
                    Filter::NoFolder => r.folder_id.is_none(),
                    Filter::Trash | Filter::Archive => false,
                })
                .cloned()
                .collect(),
        }
    }

    /// Rows matching a filter, sorted.
    #[must_use]
    pub fn rows_filtered(&self, filter: &Filter, sort: SortOrder, reverse: bool) -> Vec<Arc<Row>> {
        let mut rows = self.rows_matching(filter);
        sort_rows(&mut rows, sort, reverse);
        rows
    }

    /// The number of items a filter would show.
    #[must_use]
    pub fn visible_ids_in_trash(&self) -> Vec<String> {
        self.trash.keys().cloned().collect()
    }

    /// The number of items a filter would show.
    #[must_use]
    pub fn count(&self, filter: &Filter) -> usize {
        match filter {
            Filter::All => self.counts.all,
            Filter::Favorites => self.counts.favorites,
            Filter::Type(kind) => self.counts.by_type.get(kind).copied().unwrap_or(0),
            Filter::Folder(path) => self.counts.by_folder.get(path).copied().unwrap_or(0),
            Filter::NoFolder => self.counts.no_folder,
            Filter::Trash => self.counts.trash,
            Filter::Archive => self.counts.archived,
        }
    }

    fn reindex(&mut self) {
        self.version += 1;
        self.folder_paths = self
            .folders
            .iter()
            .filter_map(|f| f.id.clone().map(|id| (id, f.name.clone())))
            .collect();
        // Rows and counters in one pass over the active items; the trash and
        // archive only need rows (their counts are their lengths).
        let mut rows = BTreeMap::new();
        let mut counts = Counts::default();
        for (id, item) in &self.active {
            rows.insert(id.clone(), Arc::new(Row::from_item(item, &self.folder_paths)));
            count_add(&mut counts, &self.folder_paths, item);
        }
        for (id, item) in self.trash.iter().chain(self.archived.iter()) {
            rows.insert(id.clone(), Arc::new(Row::from_item(item, &self.folder_paths)));
        }
        counts.trash = self.trash.len();
        counts.archived = self.archived.len();
        self.rows = rows;
        self.counts = counts;
    }

    /// Recomputes the health flags on rows from a health report.
    pub fn apply_health(&mut self, report: &crate::vault::health::HealthReport) {
        self.version += 1;
        for row in self.rows.values_mut() {
            let row = Arc::make_mut(row);
            let entry = report.entry(&row.id);
            row.exposed = entry.is_some_and(|e| e.flags.exposed);
            row.reused = entry.is_some_and(|e| e.flags.reused);
            row.weak = entry.is_some_and(|e| e.flags.weak);
            row.old = entry.is_some_and(|e| e.flags.old);
            row.insecure = entry.is_some_and(|e| e.flags.insecure);
        }
    }

    /// Folder ids that no longer exist in the folder list.
    #[must_use]
    pub fn dangling_folder_ids(&self) -> BTreeSet<String> {
        let known: BTreeSet<&str> = self.folders.iter().filter_map(|f| f.id.as_deref()).collect();
        self.active
            .values()
            .filter_map(|i| i.folder_id.as_deref())
            .filter(|id| !known.contains(id))
            .map(str::to_owned)
            .collect()
    }
}

impl Row {
    fn from_item(item: &Item, folder_paths: &BTreeMap<String, String>) -> Self {
        let folder_id = item.folder_id.clone();
        let folder_path =
            folder_id.as_ref().and_then(|id| folder_paths.get(id).cloned()).unwrap_or_default();
        Self {
            id: item.id.clone().unwrap_or_default(),
            name: item.name.clone(),
            // Most names are ASCII, where the simple lowering is the same
            // and several times faster than the Unicode tables.
            name_lower: if item.name.is_ascii() {
                item.name.to_ascii_lowercase()
            } else {
                item.name.to_lowercase()
            },
            subtitle: item.subtitle(),
            kind: item.kind,
            favorite: item.favorite,
            totp: item.has_totp(),
            passkey: item.has_passkey(),
            attachments: item.attachment_count(),
            reprompt: item.wants_reprompt(),
            folder_id,
            folder_path,
            deleted: item.is_deleted(),
            archived: item.is_archived(),
            modified: item
                .revision_date
                .as_deref()
                .and_then(crate::util::time::parse_iso)
                .map_or(0, |d| d.timestamp()),
            created: item
                .creation_date
                .as_deref()
                .and_then(crate::util::time::parse_iso)
                .map_or(0, |d| d.timestamp()),
            exposed: false,
            reused: false,
            weak: false,
            old: false,
            insecure: false,
            domain: std::sync::OnceLock::new(),
        }
    }
}

fn sort_rows(rows: &mut [Arc<Row>], sort: SortOrder, reverse: bool) {
    match sort {
        SortOrder::Name => rows.sort_by(|a, b| a.name_lower.cmp(&b.name_lower)),
        SortOrder::Modified => rows.sort_by(|a, b| {
            b.modified.cmp(&a.modified).then_with(|| a.name_lower.cmp(&b.name_lower))
        }),
        SortOrder::Created => rows.sort_by(|a, b| {
            b.created.cmp(&a.created).then_with(|| a.name_lower.cmp(&b.name_lower))
        }),
        SortOrder::Type => rows.sort_by(|a, b| {
            a.kind.code().cmp(&b.kind.code()).then_with(|| a.name_lower.cmp(&b.name_lower))
        }),
    }
    if reverse {
        rows.reverse();
    }
}

/// Adds one item's contribution to the counters.
fn count_add(counts: &mut Counts, folder_paths: &BTreeMap<String, String>, item: &Item) {
    counts.all += 1;
    if item.favorite {
        counts.favorites += 1;
    }
    *counts.by_type.entry(item.kind).or_insert(0) += 1;
    match &item.folder_id {
        Some(folder) => {
            if let Some(path) = folder_paths.get(folder) {
                // "A/B/C" counts for "A", "A/B" and "A/B/C". The prefixes are
                // slices of the path; a key is only allocated the first time
                // a folder is seen.
                let ends = path.match_indices('/').map(|(at, _)| at).chain([path.len()]);
                for end in ends {
                    let prefix = &path[..end];
                    match counts.by_folder.get_mut(prefix) {
                        Some(count) => *count += 1,
                        None => {
                            counts.by_folder.insert(prefix.to_owned(), 1);
                        }
                    }
                }
            }
        }
        None => counts.no_folder += 1,
    }
}

/// Subtracts one item's contribution from the counters.
fn count_subtract(counts: &mut Counts, folder_paths: &BTreeMap<String, String>, item: &Item) {
    counts.all = counts.all.saturating_sub(1);
    if item.favorite {
        counts.favorites = counts.favorites.saturating_sub(1);
    }
    if let Some(count) = counts.by_type.get_mut(&item.kind) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.by_type.remove(&item.kind);
        }
    }
    match &item.folder_id {
        Some(folder) => {
            if let Some(path) = folder_paths.get(folder) {
                let ends = path.match_indices('/').map(|(at, _)| at).chain([path.len()]);
                for end in ends {
                    let prefix = &path[..end];
                    if let Some(count) = counts.by_folder.get_mut(prefix) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            counts.by_folder.remove(prefix);
                        }
                    }
                }
            }
        }
        None => counts.no_folder = counts.no_folder.saturating_sub(1),
    }
}

/// A minimal fixture item for tests and the 5,000-item performance fixture.
#[must_use]
pub fn synthetic_item(index: usize) -> Item {
    use crate::bw::models::Login;
    let id = format!("item-{index:05}");
    let mut item = Item::new(ItemType::Login);
    item.id = Some(id);
    item.name = format!("Item {index:05}");
    item.favorite = index.is_multiple_of(7);
    item.folder_id =
        if index.is_multiple_of(3) { Some("folder-personal".to_owned()) } else { None };
    item.login = Some(Login {
        username: Some(crate::bw::sensitive::Sensitive::new(format!("user{index}@example.com"))),
        ..Login::default()
    });
    item.revision_date = Some("2026-10-01T12:00:00.000Z".to_owned());
    item.creation_date = Some("2026-01-01T00:00:00.000Z".to_owned());
    item
}

#[cfg(test)]
mod tests {

    #[test]
    fn removing_items_keeps_all_counts_equal_to_a_reload() {
        let mut cache = VaultCache::new();
        let folders: Vec<_> = ["Work/Deep/Folder", "/Leading", "A//B", "尾/部"]
            .into_iter()
            .enumerate()
            .map(|(n, name)| Folder {
                id: Some(format!("f{n}")),
                name: name.into(),
                ..Default::default()
            })
            .collect();
        let items: Vec<_> = (0..40)
            .map(|n| {
                let mut item = synthetic_item(n);
                item.folder_id = if n % 6 == 0 { None } else { Some(format!("f{}", n % 5)) };
                item.kind = if n % 4 == 0 { ItemType::SecureNote } else { ItemType::Login };
                item
            })
            .collect();
        let mut trash = synthetic_item(40);
        trash.deleted_date = Some("2026-10-01T00:00:00Z".into());
        let mut archive = synthetic_item(41);
        archive.archived_date = Some("2026-10-01T00:00:00Z".into());
        cache.load(items, folders.clone(), vec![trash], vec![archive]);
        for n in (0..42).rev() {
            let version = cache.version();
            assert!(cache.remove(&format!("item-{n:05}")).is_some());
            assert_eq!(cache.version(), version + 1);
            let mut reloaded = VaultCache::new();
            reloaded.load(
                cache.active_items().cloned().collect(),
                folders.clone(),
                cache.trash_items().cloned().collect(),
                cache.archived_items().cloned().collect(),
            );
            let a = cache.counts();
            let b = reloaded.counts();
            assert_eq!(
                (a.all, a.favorites, a.no_folder, a.trash, a.archived),
                (b.all, b.favorites, b.no_folder, b.trash, b.archived)
            );
            assert_eq!(a.by_type, b.by_type);
            assert_eq!(a.by_folder, b.by_folder);
        }
        let version = cache.version();
        assert!(cache.remove("missing").is_none());
        assert_eq!(cache.version(), version);
        assert!(cache.is_empty());
    }

    #[test]
    fn upsert_keeps_health_marks_unless_the_password_changes() {
        let mut cache = VaultCache::default();
        let mut item = synthetic_item(1);
        cache.load(vec![item.clone()], vec![], vec![], vec![]);
        let id = item.id.clone().unwrap();
        Arc::make_mut(cache.rows.get_mut(&id).unwrap()).weak = true;
        item.favorite = !item.favorite;
        cache.upsert(item.clone());
        assert!(cache.row(&id).unwrap().weak, "a favourite keeps the mark");
        if let Some(login) = item.login.as_mut() {
            login.password = Some(crate::bw::sensitive::Sensitive::new("a new password"));
        }
        cache.upsert(item);
        assert!(!cache.row(&id).unwrap().weak, "a new password drops the stale mark");
    }
    use super::*;

    fn item(id: &str, name: &str, kind: ItemType) -> Item {
        let mut item = Item::new(kind);
        item.id = Some(id.to_owned());
        item.name = name.to_owned();
        item
    }

    #[test]
    fn folder_titles_join_path_segments() {
        assert_eq!(
            Filter::Folder("Finanças/Bancos".to_owned()).display_title(),
            "Finanças › Bancos"
        );
        assert_eq!(Filter::Folder("A/B/C".to_owned()).display_title(), "A › B › C");
        assert_eq!(Filter::Folder("Solo".to_owned()).display_title(), "Solo");
        // The empty fold in "A//B" renders as a blank segment, as before.
        assert_eq!(Filter::Folder("A//B".to_owned()).display_title(), "A ›  › B");
        assert_eq!(Filter::All.display_title(), "All items");
    }

    #[test]
    fn loads_and_counts() {
        let mut cache = VaultCache::new();
        let mut favorite = item("a", "GitHub", ItemType::Login);
        favorite.favorite = true;
        let mut trash = item("b", "Old", ItemType::Login);
        trash.deleted_date = Some("2026-10-01T00:00:00Z".to_owned());
        cache.load(
            vec![favorite, item("c", "Note", ItemType::SecureNote)],
            vec![Folder { id: Some("f1".into()), name: "Work".into(), ..Default::default() }],
            vec![trash],
            vec![],
        );
        assert_eq!(cache.counts().all, 2);
        assert_eq!(cache.counts().favorites, 1);
        assert_eq!(cache.counts().trash, 1);
        assert_eq!(cache.count(&Filter::Type(ItemType::Login)), 1);
        assert_eq!(cache.rows_filtered(&Filter::Trash, SortOrder::Name, false).len(), 1);
        assert_eq!(cache.item("b").unwrap().name, "Old");
    }

    #[test]
    fn upsert_moves_between_lists() {
        let mut cache = VaultCache::new();
        cache.load(vec![item("a", "One", ItemType::Login)], vec![], vec![], vec![]);
        let mut trashed = cache.item("a").unwrap().clone();
        trashed.deleted_date = Some("2026-10-02T00:00:00Z".to_owned());
        cache.upsert(trashed);
        assert_eq!(cache.counts().all, 0);
        assert_eq!(cache.counts().trash, 1);
        assert_eq!(cache.rows_filtered(&Filter::Trash, SortOrder::Name, false)[0].name, "One");
    }

    #[test]
    fn sorts_by_name_modified_type() {
        let mut a = item("a", "Beta", ItemType::Card);
        a.revision_date = Some("2026-01-01T00:00:00Z".into());
        let mut b = item("b", "alpha", ItemType::Login);
        b.revision_date = Some("2026-05-01T00:00:00Z".into());
        let mut cache = VaultCache::new();
        cache.load(vec![a, b], vec![], vec![], vec![]);
        let names: Vec<_> = cache
            .rows_filtered(&Filter::All, SortOrder::Name, false)
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(names, vec!["alpha", "Beta"]);
        let modified: Vec<_> = cache
            .rows_filtered(&Filter::All, SortOrder::Modified, false)
            .iter()
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(modified, vec!["b", "a"]);
        let reversed: Vec<_> = cache
            .rows_filtered(&Filter::All, SortOrder::Name, true)
            .iter()
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(reversed, vec!["a", "b"]);
    }

    #[test]
    fn folder_paths_are_precomputed() {
        let mut cache = VaultCache::new();
        let mut child = item("a", "Child", ItemType::Login);
        child.folder_id = Some("f2".into());
        cache.load(
            vec![child],
            vec![
                Folder { id: Some("f1".into()), name: "Work".into(), ..Default::default() },
                Folder { id: Some("f2".into()), name: "Work/Clients".into(), ..Default::default() },
            ],
            vec![],
            vec![],
        );
        let row = cache.row("a").unwrap();
        assert_eq!(row.folder_path, "Work/Clients");
        assert_eq!(cache.count(&Filter::Folder("Work/Clients".into())), 1);
    }

    #[test]
    fn synthetic_vault_has_counts() {
        let items: Vec<_> = (0..250).map(synthetic_item).collect();
        let mut cache = VaultCache::new();
        cache.load(items, vec![], vec![], vec![]);
        assert_eq!(cache.counts().all, 250);
        assert_eq!(cache.count(&Filter::Favorites), 36);
        assert_eq!(cache.count(&Filter::NoFolder), 166);
    }
}
