//! Writing configuration changes while preserving comments and formatting.

use std::io;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Value};

/// Sets a dotted path in a TOML document, creating intermediate tables.
pub fn set_value(document: &mut DocumentMut, path: &[&str], value: Value) {
    let Some((last, parents)) = path.split_last() else { return };
    let mut table = document.as_table_mut();
    for part in parents {
        if !table.contains_key(part) {
            table.insert(part, Item::Table(toml_edit::Table::new()));
        }
        let Some(next) = table.get_mut(part).and_then(Item::as_table_mut) else { return };
        table = next;
    }
    if let Some(existing) = table.get_mut(last) {
        if let Some(old) = existing.as_value() {
            let decor = old.decor().clone();
            let mut value = value;
            *value.decor_mut() = decor;
            *existing = Item::Value(value);
            return;
        }
        *existing = Item::Value(value);
    } else {
        table.insert(last, Item::Value(value));
    }
}

/// Removes a dotted path, leaving the rest of the document untouched.
pub fn remove_value(document: &mut DocumentMut, path: &[&str]) {
    let Some((last, parents)) = path.split_last() else { return };
    let mut table = document.as_table_mut();
    for part in parents {
        let Some(next) = table.get_mut(part).and_then(Item::as_table_mut) else { return };
        table = next;
    }
    table.remove(last);
}

/// Reads `path` from disk, edits it and writes it back atomically.
pub fn update_file(path: &Path, updates: &[(Vec<String>, Value)]) -> io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut document: DocumentMut =
        text.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e}")))?;
    for (dotted, value) in updates {
        let path: Vec<&str> = dotted.iter().map(String::as_str).collect();
        set_value(&mut document, &path, value.clone());
    }
    crate::util::atomic::write_atomic(path, document.to_string().as_bytes(), 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn keeps_comments_and_other_keys() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "# my comment\n[ui]\n# keep me\ntheme = \"nord\" # trailing\nlist_density = \"compact\"\n",
        )
        .unwrap();
        update_file(&path, &[(vec!["ui".into(), "theme".into()], Value::from("gruvbox-dark"))])
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my comment"));
        assert!(text.contains("# keep me"));
        assert!(text.contains("theme = \"gruvbox-dark\""));
        assert!(text.contains("list_density = \"compact\""));
    }

    #[test]
    fn creates_missing_tables() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[ui]\ntheme = \"nord\"\n").unwrap();
        update_file(
            &path,
            &[(vec!["keys".into(), "list".into(), "dd".into()], Value::from("noop"))],
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[keys.list]"));
        assert!(text.contains("dd = \"noop\""));
    }

    #[test]
    fn removes_keys() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[keys.list]\ndd = \"noop\"\n\"<Del>\" = \"item.trash\"\n").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut document: DocumentMut = text.parse().unwrap();
        remove_value(&mut document, &["keys", "list", "dd"]);
        assert!(!document.to_string().contains("noop"));
        assert!(document.to_string().contains("item.trash"));
    }
}
