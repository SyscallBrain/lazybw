//! The nested folder tree derived from `Parent/Child` folder names.
//!
//! A parent that is not itself a folder is a **virtual node**: it groups the
//! children for display and filtering, but is not a move target and cannot be
//! renamed or deleted.

use crate::bw::models::Folder;
use std::collections::{BTreeMap, BTreeSet};

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderNode {
    /// Full path from the root (`Work/Projects`).
    pub path: String,
    /// Last segment, shown in the sidebar.
    pub label: String,
    /// Indentation level, 0 for top-level folders.
    pub depth: usize,
    /// Real folder ids whose name is exactly this path.
    pub folder_ids: Vec<String>,
    /// Number of items in this subtree.
    pub count: usize,
    /// True when no real folder has exactly this path.
    pub virtual_node: bool,
    /// Children, sorted by label.
    pub children: Vec<FolderNode>,
}

/// The forest of folder roots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderTree {
    /// Top-level nodes.
    pub roots: Vec<FolderNode>,
}

/// A row of the flattened tree, for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    /// Full path.
    pub path: String,
    /// Display label.
    pub label: String,
    /// Indentation.
    pub depth: usize,
    /// Items in the subtree.
    pub count: usize,
    /// Real folder (rename/delete allowed).
    pub is_real: bool,
    /// Has children.
    pub has_children: bool,
    /// Expanded (children are shown).
    pub expanded: bool,
}

/// Builds the tree from the folder list and per-path item counts (counts
/// include descendants).
#[must_use]
pub fn build(folders: &[Folder], counts: &BTreeMap<String, usize>) -> FolderTree {
    #[derive(Default)]
    struct Builder {
        real: bool,
        children: BTreeMap<String, Builder>,
    }
    let mut root = Builder::default();
    for folder in folders {
        if folder.id.is_none() {
            continue;
        }
        let segments = folder.segments();
        if segments.is_empty() {
            continue;
        }
        let mut node = &mut root;
        for segment in &segments {
            node = node.children.entry((*segment).to_owned()).or_default();
        }
        node.real = true;
    }
    fn convert(
        name: &str,
        path: &str,
        builder: Builder,
        counts: &BTreeMap<String, usize>,
    ) -> FolderNode {
        let full = if path.is_empty() { name.to_owned() } else { format!("{path}/{name}") };
        let mut children: Vec<FolderNode> = builder
            .children
            .into_iter()
            .map(|(label, child)| convert(&label, &full, child, counts))
            .collect();
        children.sort_by_key(|a| a.label.to_lowercase());
        let count = counts
            .get(&full)
            .copied()
            .unwrap_or_else(|| children.iter().map(|child| child.count).sum());
        FolderNode {
            count,
            path: full,
            label: name.to_owned(),
            depth: 0,
            folder_ids: Vec::new(),
            virtual_node: !builder.real,
            children,
        }
    }
    let mut roots: Vec<FolderNode> = root
        .children
        .into_iter()
        .map(|(label, child)| convert(&label, "", child, counts))
        .collect();
    roots.sort_by_key(|a| a.label.to_lowercase());
    mark_depth(&mut roots, 0);
    fill_ids(&mut roots, folders);
    FolderTree { roots }
}

fn fill_ids(nodes: &mut [FolderNode], folders: &[Folder]) {
    for node in nodes {
        node.folder_ids =
            folders.iter().filter(|f| f.name == node.path).filter_map(|f| f.id.clone()).collect();
        fill_ids(&mut node.children, folders);
    }
}

fn mark_depth(nodes: &mut [FolderNode], depth: usize) {
    for node in nodes {
        node.depth = depth;
        mark_depth(&mut node.children, depth + 1);
    }
}

/// Flattens the tree, hiding the children of collapsed paths.
#[must_use]
pub fn flatten(tree: &FolderTree, collapsed: &BTreeSet<String>) -> Vec<FolderRow> {
    fn walk(nodes: &[FolderNode], collapsed: &BTreeSet<String>, out: &mut Vec<FolderRow>) {
        for node in nodes {
            let expanded = !collapsed.contains(&node.path);
            out.push(FolderRow {
                path: node.path.clone(),
                label: node.label.clone(),
                depth: node.depth,
                count: node.count,
                is_real: !node.virtual_node,
                has_children: !node.children.is_empty(),
                expanded,
            });
            if expanded {
                walk(&node.children, collapsed, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&tree.roots, collapsed, &mut out);
    out
}

/// All paths in the tree, for `zR`.
#[must_use]
pub fn all_paths(tree: &FolderTree) -> BTreeSet<String> {
    fn walk(nodes: &[FolderNode], out: &mut BTreeSet<String>) {
        for node in nodes {
            if !node.children.is_empty() {
                out.insert(node.path.clone());
            }
            walk(&node.children, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&tree.roots, &mut out);
    out
}

/// The real folder ids under a path (the node itself and its descendants).
#[must_use]
pub fn ids_under(tree: &FolderTree, path: &str) -> Vec<String> {
    fn collect(node: &FolderNode, out: &mut Vec<String>) {
        out.extend(node.folder_ids.iter().cloned());
        for child in &node.children {
            collect(child, out);
        }
    }
    fn find<'a>(nodes: &'a [FolderNode], path: &str) -> Option<&'a FolderNode> {
        for node in nodes {
            if node.path == path {
                return Some(node);
            }
            if let Some(found) = find(&node.children, path) {
                return Some(found);
            }
        }
        None
    }
    let mut out = Vec::new();
    if let Some(node) = find(&tree.roots, path) {
        collect(node, &mut out);
    }
    out
}

/// Every folder whose name starts with `old/`, as (id, new name).
#[must_use]
pub fn renamed_subfolders(folders: &[Folder], old: &str, new: &str) -> Vec<(String, String)> {
    let prefix = format!("{old}/");
    folders
        .iter()
        .filter_map(|f| {
            let id = f.id.clone()?;
            let rest = f.name.strip_prefix(&prefix)?;
            Some((id, format!("{new}/{rest}")))
        })
        .collect()
}

/// Number of folders whose name starts with `path/`.
#[must_use]
pub fn subfolder_count(folders: &[Folder], path: &str) -> usize {
    let prefix = format!("{path}/");
    folders.iter().filter(|f| f.name.starts_with(&prefix)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str) -> Folder {
        Folder { id: Some(id.to_owned()), name: name.to_owned(), ..Default::default() }
    }

    fn counts() -> BTreeMap<String, usize> {
        BTreeMap::from([
            ("Work".to_owned(), 15),
            ("Work/Projects".to_owned(), 5),
            ("Personal".to_owned(), 2),
        ])
    }

    #[test]
    fn builds_nested_tree_with_counts() {
        let folders =
            vec![folder("f1", "Work"), folder("f2", "Work/Projects"), folder("f3", "Personal")];
        let tree = build(&folders, &counts());
        assert_eq!(tree.roots.len(), 2);
        let work = tree.roots.iter().find(|n| n.label == "Work").unwrap();
        assert_eq!(work.count, 15);
        assert_eq!(work.children[0].path, "Work/Projects");
        assert_eq!(work.children[0].count, 5);
        assert!(!work.virtual_node);
    }

    #[test]
    fn missing_parents_are_virtual() {
        let folders = vec![folder("f2", "Work/Projects")];
        let tree = build(&folders, &BTreeMap::from([("Work/Projects".to_owned(), 5usize)]));
        let work = &tree.roots[0];
        assert_eq!(work.label, "Work");
        assert!(work.virtual_node);
        assert!(!work.children[0].virtual_node);
        assert_eq!(work.count, 5);
    }

    #[test]
    fn flatten_respects_collapse() {
        let folders = vec![folder("f1", "Work"), folder("f2", "Work/Projects")];
        let tree = build(&folders, &counts());
        let rows = flatten(&tree, &BTreeSet::new());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].depth, 1);
        let collapsed = BTreeSet::from(["Work".to_owned()]);
        let rows = flatten(&tree, &collapsed);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].expanded);
    }

    #[test]
    fn paths_and_ids() {
        let folders = vec![folder("f1", "Work"), folder("f2", "Work/Projects")];
        let tree = build(&folders, &counts());
        assert_eq!(all_paths(&tree).len(), 1);
        let ids = ids_under(&tree, "Work");
        assert_eq!(ids, vec!["f1", "f2"]);
    }

    #[test]
    fn renames_and_counts_subfolders() {
        let folders = vec![folder("f1", "Work"), folder("f2", "Work/Projects")];
        assert_eq!(subfolder_count(&folders, "Work"), 1);
        assert_eq!(
            renamed_subfolders(&folders, "Work", "Job"),
            vec![("f2".to_owned(), "Job/Projects".to_owned())]
        );
    }
}
