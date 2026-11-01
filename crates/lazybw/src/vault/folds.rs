//! Which sidebar folders are collapsed, kept between sessions.
//!
//! Folder names are encrypted in the vault, so they are never written to
//! disk: a collapsed folder is stored as its id (a random UUID that `bw`
//! keeps in clear anyway), which also survives renames. A virtual parent
//! (the "A" of a folder "A/B" when no folder "A" exists) has no id; it is
//! stored as `under:<id>:<depth>`, the first real folder below it and the
//! number of path segments to keep.

use crate::bw::models::Folder;
use std::collections::BTreeSet;

/// The on-disk form.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FoldFile {
    /// Format version.
    pub version: u32,
    /// Collapsed folders as ids (see the module docs).
    pub collapsed: Vec<String>,
}

/// Collapsed paths → storable ids. Paths that match no folder are dropped.
#[must_use]
pub fn to_ids(collapsed: &BTreeSet<String>, folders: &[Folder]) -> Vec<String> {
    let mut out = Vec::new();
    for path in collapsed {
        if let Some(id) = folders.iter().find(|f| &f.name == path).and_then(|f| f.id.clone()) {
            out.push(id);
            continue;
        }
        let prefix = format!("{path}/");
        let below = folders
            .iter()
            .filter(|f| f.name.starts_with(&prefix))
            .filter_map(|f| f.id.as_ref().map(|id| (f.name.as_str(), id)))
            .min();
        if let Some((_, id)) = below {
            out.push(format!("under:{id}:{}", path.split('/').count()));
        }
    }
    out.sort();
    out
}

/// Stored ids → collapsed paths for the current folders. Unknown ids (a
/// deleted folder, another account) are ignored.
#[must_use]
pub fn from_ids(ids: &[String], folders: &[Folder]) -> BTreeSet<String> {
    let name_of = |id: &str| folders.iter().find(|f| f.id.as_deref() == Some(id)).map(|f| &f.name);
    let mut out = BTreeSet::new();
    for stored in ids {
        if let Some(rest) = stored.strip_prefix("under:") {
            let Some((id, depth)) = rest.rsplit_once(':') else { continue };
            let Ok(depth) = depth.parse::<usize>() else { continue };
            if let Some(name) = name_of(id) {
                let segments: Vec<&str> = name.split('/').collect();
                if depth >= 1 && depth < segments.len() {
                    out.insert(segments[..depth].join("/"));
                }
            }
        } else if let Some(name) = name_of(stored) {
            out.insert(name.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str) -> Folder {
        Folder { id: Some(id.to_owned()), name: name.to_owned(), ..Default::default() }
    }

    #[test]
    fn real_and_virtual_folders_round_trip_without_names() {
        let folders = vec![
            folder("f-1", "Finanças"),
            folder("f-2", "Finanças/Bancos"),
            folder("f-3", "Hermes/API Keys"),
            folder("f-4", "Hermes/Bots"),
        ];
        let collapsed: BTreeSet<String> =
            ["Finanças".to_owned(), "Hermes".to_owned()].into_iter().collect();
        let ids = to_ids(&collapsed, &folders);
        assert_eq!(ids, vec!["f-1".to_owned(), "under:f-3:1".to_owned()]);
        assert!(ids.iter().all(|id| !id.contains("Finan") && !id.contains("Hermes")));
        assert_eq!(from_ids(&ids, &folders), collapsed);
    }

    #[test]
    fn a_renamed_folder_stays_collapsed_and_unknown_ids_are_ignored() {
        let ids = vec!["f-1".to_owned(), "gone".to_owned(), "under:gone:1".to_owned()];
        let renamed = vec![folder("f-1", "Money"), folder("f-2", "Money/Banks")];
        let expected: BTreeSet<String> = ["Money".to_owned()].into_iter().collect();
        assert_eq!(from_ids(&ids, &renamed), expected);
    }
}
