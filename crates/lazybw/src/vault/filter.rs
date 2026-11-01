//! The sidebar model: the ordered list of filters, sections and folder rows.
//!
//! The UI renders this list; navigation and filtering operate on it, so the
//! sidebar, the list title and the statusline can never disagree.

use crate::bw::models::ItemType;
use crate::vault::cache::{Filter, VaultCache};
use crate::vault::folders::{self, FolderTree};
use std::collections::BTreeSet;

/// What a row represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRole {
    /// A section label (not selectable).
    Section(&'static str),
    /// A visual separator (not selectable).
    Separator,
    /// All items.
    All,
    /// Favourites.
    Favorites,
    /// One item type.
    Type(ItemType),
    /// A folder in the tree.
    Folder {
        /// Full path.
        path: String,
        /// Real folder (rename/delete allowed).
        real: bool,
        /// Has subfolders.
        has_children: bool,
        /// Children are visible.
        expanded: bool,
    },
    /// Items with no folder.
    NoFolder,
    /// Archived items.
    Archive,
    /// Trash.
    Trash,
}

/// One sidebar row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarEntry {
    /// What it is.
    pub role: SidebarRole,
    /// Display label.
    pub label: String,
    /// Count, right-aligned; `None` for sections.
    pub count: Option<usize>,
    /// The filter it applies; `None` for sections.
    pub filter: Option<Filter>,
}

impl SidebarEntry {
    /// Selectable rows only.
    #[must_use]
    pub fn selectable(&self) -> bool {
        self.filter.is_some()
    }
}

/// Builds the sidebar for the current cache.
#[must_use]
pub fn entries(
    cache: &VaultCache,
    collapsed: &BTreeSet<String>,
    show_empty_types: bool,
) -> Vec<SidebarEntry> {
    let mut out = Vec::new();
    out.push(SidebarEntry {
        role: SidebarRole::All,
        label: "All items".to_owned(),
        count: Some(cache.count(&Filter::All)),
        filter: Some(Filter::All),
    });
    out.push(SidebarEntry {
        role: SidebarRole::Favorites,
        label: "Favorites".to_owned(),
        count: Some(cache.count(&Filter::Favorites)),
        filter: Some(Filter::Favorites),
    });
    out.push(SidebarEntry {
        role: SidebarRole::Section("TYPES"),
        label: "TYPES".to_owned(),
        count: None,
        filter: None,
    });
    for kind in [
        ItemType::Login,
        ItemType::Card,
        ItemType::Identity,
        ItemType::SecureNote,
        ItemType::SshKey,
    ] {
        let count = cache.count(&Filter::Type(kind));
        if count == 0 && !show_empty_types {
            continue;
        }
        out.push(SidebarEntry {
            role: SidebarRole::Type(kind),
            label: kind.plural().to_owned(),
            count: Some(count),
            filter: Some(Filter::Type(kind)),
        });
    }
    let tree = folders::build(cache.folders(), &cache.counts().by_folder);
    if !tree.roots.is_empty() {
        out.push(SidebarEntry {
            role: SidebarRole::Section("FOLDERS"),
            label: "FOLDERS".to_owned(),
            count: None,
            filter: None,
        });
        for row in folders::flatten(&tree, collapsed) {
            out.push(SidebarEntry {
                role: SidebarRole::Folder {
                    path: row.path.clone(),
                    real: row.is_real,
                    has_children: row.has_children,
                    expanded: row.expanded,
                },
                label: row.label,
                count: Some(row.count),
                filter: Some(Filter::Folder(row.path)),
            });
        }
    }
    out.push(SidebarEntry {
        role: SidebarRole::NoFolder,
        label: "No folder".to_owned(),
        count: Some(cache.count(&Filter::NoFolder)),
        filter: Some(Filter::NoFolder),
    });
    if cache.counts().archived > 0 {
        out.push(SidebarEntry {
            role: SidebarRole::Archive,
            label: "Archive".to_owned(),
            count: Some(cache.counts().archived),
            filter: Some(Filter::Archive),
        });
    }
    out.push(SidebarEntry {
        role: SidebarRole::Separator,
        label: String::new(),
        count: None,
        filter: None,
    });
    out.push(SidebarEntry {
        role: SidebarRole::Trash,
        label: "Trash".to_owned(),
        count: Some(cache.count(&Filter::Trash)),
        filter: Some(Filter::Trash),
    });
    out
}

/// The folder tree used by the sidebar.
#[must_use]
pub fn folder_tree(cache: &VaultCache) -> FolderTree {
    folders::build(cache.folders(), &cache.counts().by_folder)
}

/// The index of the entry matching a filter.
#[must_use]
pub fn index_of(entries: &[SidebarEntry], filter: &Filter) -> Option<usize> {
    entries.iter().position(|entry| entry.filter.as_ref() == Some(filter))
}

/// The index of the first selectable entry at or after `from`.
#[must_use]
pub fn next_selectable(entries: &[SidebarEntry], from: usize, forward: bool) -> Option<usize> {
    if forward {
        (from + 1..entries.len()).find(|&i| entries[i].selectable())
    } else {
        (0..from).rev().find(|&i| entries[i].selectable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bw::models::Folder;
    use crate::vault::cache::synthetic_item;

    fn cache() -> VaultCache {
        let mut cache = VaultCache::new();
        let mut items: Vec<_> = (0..20).map(synthetic_item).collect();
        for (i, item) in items.iter_mut().enumerate() {
            item.favorite = i < 3;
        }
        cache.load(
            items,
            vec![Folder { id: Some("f1".into()), name: "Personal".into(), ..Default::default() }],
            vec![],
            vec![],
        );
        cache
    }

    #[test]
    fn builds_expected_sections() {
        let entries = entries(&cache(), &BTreeSet::new(), false);
        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels[0], "All items");
        assert!(labels.contains(&"TYPES"));
        assert!(labels.contains(&"FOLDERS"));
        assert!(labels.contains(&"Trash"));
        assert_eq!(entries[1].count, Some(3));
        assert!(entries.iter().any(|e| e.role
            == SidebarRole::Folder {
                path: "Personal".into(),
                real: true,
                has_children: false,
                expanded: true
            }));
    }

    #[test]
    fn empty_types_can_be_hidden_or_shown() {
        let hidden = entries(&cache(), &BTreeSet::new(), false);
        assert!(!hidden.iter().any(|e| e.role == SidebarRole::Type(ItemType::SshKey)));
        let shown = entries(&cache(), &BTreeSet::new(), true);
        assert!(shown.iter().any(|e| e.role == SidebarRole::Type(ItemType::SshKey)));
    }

    #[test]
    fn navigation_skips_sections() {
        let entries = entries(&cache(), &BTreeSet::new(), false);
        let all = index_of(&entries, &Filter::All).unwrap();
        let next = next_selectable(&entries, all, true).unwrap();
        assert_eq!(entries[next].filter, Some(Filter::Favorites));
        let up = next_selectable(&entries, next, false).unwrap();
        assert_eq!(entries[up].filter, Some(Filter::All));
    }

    #[test]
    fn folder_counts_include_children() {
        let mut cache = VaultCache::new();
        let mut child = synthetic_item(1);
        child.folder_id = Some("f2".into());
        let mut parent = synthetic_item(2);
        parent.folder_id = Some("f1".into());
        cache.load(
            vec![child, parent],
            vec![
                Folder { id: Some("f1".into()), name: "Work".into(), ..Default::default() },
                Folder {
                    id: Some("f2".into()),
                    name: "Work/Projects".into(),
                    ..Default::default()
                },
            ],
            vec![],
            vec![],
        );
        let entries = entries(&cache, &BTreeSet::new(), false);
        let work = entries.iter().find(|e| e.label == "Work").unwrap();
        assert_eq!(work.count, Some(2));
        let projects = entries.iter().find(|e| e.label == "Projects").unwrap();
        assert_eq!(projects.count, Some(1));
    }
}

/// One row of the medium-width icon rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailSlot {
    /// A sidebar entry shown as its own icon.
    Entry(usize),
    /// The single folder icon standing for every folder row (and "No
    /// folder"); it opens the folder picker. Holds the first folder index.
    Folders(usize),
    /// A section label or separator, drawn as a rule.
    Divider,
}

impl RailSlot {
    /// Can the cursor rest here?
    #[must_use]
    pub fn selectable(self) -> bool {
        !matches!(self, Self::Divider)
    }

    /// The sidebar index the cursor takes on this slot.
    #[must_use]
    pub fn entry(self) -> Option<usize> {
        match self {
            Self::Entry(index) | Self::Folders(index) => Some(index),
            Self::Divider => None,
        }
    }
}

/// Is this a row the rail folds into its folder icon?
#[must_use]
pub fn is_folder_row(role: &SidebarRole) -> bool {
    matches!(role, SidebarRole::Folder { .. } | SidebarRole::NoFolder)
}

/// The rail rows for a sidebar: folders collapse into one icon.
#[must_use]
pub fn rail_slots(entries: &[SidebarEntry]) -> Vec<RailSlot> {
    let mut out = Vec::new();
    let mut folders_shown = false;
    for (index, entry) in entries.iter().enumerate() {
        if is_folder_row(&entry.role) {
            if !folders_shown {
                folders_shown = true;
                out.push(RailSlot::Folders(index));
            }
        } else if entry.selectable() {
            out.push(RailSlot::Entry(index));
        } else {
            out.push(RailSlot::Divider);
        }
    }
    out
}

/// The rail row holding the sidebar cursor.
#[must_use]
pub fn rail_position(entries: &[SidebarEntry], slots: &[RailSlot], row: usize) -> Option<usize> {
    let on_folder = entries.get(row).is_some_and(|entry| is_folder_row(&entry.role));
    slots.iter().position(|slot| match slot {
        RailSlot::Entry(index) => *index == row,
        RailSlot::Folders(_) => on_folder,
        RailSlot::Divider => false,
    })
}
