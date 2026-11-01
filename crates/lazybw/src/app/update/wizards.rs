//! Export and import wizards.

use super::{
    AppState, BTreeSet, CommandKind, Effect, ExportFormat, ImportRequest, Item, Key, PathBuf,
    Pending, Screen, ctrl_np_as_arrows, request,
};

pub(crate) fn open_export(state: &mut AppState, args: &[String]) -> Vec<Effect> {
    // Opened without a format, the wizard suggests the password-protected
    // file: the safe choice for a backup. A format given on the command
    // line (`:export json …`) is respected.
    let format = args
        .first()
        .and_then(|a| ExportFormat::parse(a))
        .unwrap_or(ExportFormat::PasswordProtectedJson);
    let path = args.get(1).cloned().unwrap_or_else(|| {
        let base = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(format!(
            "bitwarden_export_{}.{}",
            state.now.format("%Y%m%d%H%M%S"),
            format.extension()
        ))
        .display()
        .to_string()
    });
    state.export =
        Some(crate::app::state::ExportState { step: 0, format, path, ..Default::default() });
    state.screen = Screen::Vault;
    Vec::new()
}

/// The import formats in the wizard's order: the Bitwarden formats first,
/// then everything else alphabetically.
pub(crate) fn ordered_import_formats(all: &[String]) -> Vec<String> {
    const FEATURED: [&str; 3] = ["bitwardenjson", "bitwardenpasswordprotected", "bitwardencsv"];
    let mut rest: Vec<String> =
        all.iter().filter(|f| !FEATURED.contains(&f.as_str())).cloned().collect();
    rest.sort();
    rest.dedup();
    FEATURED.iter().map(|f| (*f).to_owned()).chain(rest).collect()
}

pub(crate) fn open_import(state: &mut AppState, args: &[String]) -> Vec<Effect> {
    let formats = ordered_import_formats(
        &crate::bw::subprocess::FALLBACK_IMPORT_FORMATS
            .iter()
            .map(|f| (*f).to_owned())
            .collect::<Vec<_>>(),
    );
    let format = args.first().cloned().unwrap_or_else(|| "bitwardenjson".to_owned());
    let path = args.get(1).cloned().unwrap_or_default();
    let format_index = formats.iter().position(|f| *f == format).unwrap_or(0);
    // With a format given on the command line, start at the file step.
    let step = usize::from(!args.is_empty());
    state.import = Some(crate::app::state::ImportState {
        step,
        format,
        path,
        formats,
        format_index,
        ..Default::default()
    });
    vec![request(state, CommandKind::ImportFormats, Pending::ImportFormats)]
}

pub(crate) fn export_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let key = ctrl_np_as_arrows(key);
    let Some(export) = &mut state.export else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.export = None;
            return Vec::new();
        }
        Code::Tab if export.step == 0 && export.focus == 1 => {
            export.path = complete_path(&export.path);
            return Vec::new();
        }
        Code::Tab | Code::BackTab => {
            export.focus = (export.focus + 1) % 2;
            return Vec::new();
        }
        Code::Char('j') | Code::Down => {
            if export.step == 0 && export.focus == 0 {
                export.format = next_format(export.format, true);
            }
            return Vec::new();
        }
        Code::Char('k') | Code::Up => {
            if export.step == 0 && export.focus == 0 {
                export.format = next_format(export.format, false);
            }
            return Vec::new();
        }
        Code::Enter => {
            let step = export.step;
            if step == 0 {
                export.step = if export.format.needs_password() { 1 } else { 2 };
            } else if step == 1 {
                if export.password != export.confirmation {
                    export.error = Some("The passwords do not match.".to_owned());
                } else if export.password.is_empty() {
                    export.error = Some("Enter a file password.".to_owned());
                } else {
                    export.error = None;
                    export.step = 2;
                }
            } else {
                if export.busy {
                    return Vec::new();
                }
                if export.master.is_empty() {
                    export.error = Some("Enter your master password.".to_owned());
                    return Vec::new();
                }
                let format = export.format;
                let destination = PathBuf::from(shellexpand_home(&export.path));
                let password = (format == ExportFormat::PasswordProtectedJson)
                    .then(|| export.password.to_secret());
                let master = export.master.take_secret();
                export.error = None;
                // The wizard stays open until the CLI answers, so a wrong
                // master password is shown in place.
                export.busy = true;
                return vec![request(
                    state,
                    CommandKind::Export { format, destination, password, master },
                    Pending::Export,
                )];
            }
            return Vec::new();
        }
        Code::Backspace => {
            match (export.step, export.focus) {
                (0, 0) => {}
                (1, 0) => {
                    export.password.pop();
                }
                (1, 1) => {
                    export.confirmation.pop();
                }
                (2, _) => {
                    export.master.pop();
                }
                _ => {
                    export.path.pop();
                }
            }
            return Vec::new();
        }
        Code::Char(c) if key.is_printable() => {
            match (export.step, export.focus) {
                (0, 0) => {}
                (0, 1) => export.path.push(c),
                (1, 0) => export.password.push(c),
                (1, 1) => export.confirmation.push(c),
                (2, _) => export.master.push(c),
                _ => {}
            }
            return Vec::new();
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn next_format(format: ExportFormat, forward: bool) -> ExportFormat {
    let all = [
        ExportFormat::Json,
        ExportFormat::PasswordProtectedJson,
        ExportFormat::AccountEncryptedJson,
        ExportFormat::Csv,
    ];
    let index = all.iter().position(|f| *f == format).unwrap_or(0);
    let next = if forward { (index + 1) % all.len() } else { (index + all.len() - 1) % all.len() };
    all[next]
}

pub(crate) fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
    {
        return home.join(rest).display().to_string();
    }
    path.to_owned()
}

pub(crate) fn import_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::{Code, Mods};
    let key = ctrl_np_as_arrows(key);
    let Some(import) = &mut state.import else { return Vec::new() };
    if import.busy {
        return Vec::new();
    }
    let ctrl = key.mods.contains(Mods::CTRL);
    match (import.step, key.code) {
        (_, Code::Esc) => {
            if import.step == 0 {
                state.import = None;
            } else {
                import.step -= 1;
                import.error = None;
            }
        }
        // Step 0: filterable format list.
        (0, Code::Down | Code::Tab) | (0, Code::Char('n'))
            if key.code != Code::Char('n') || ctrl =>
        {
            let len = import.filtered_formats().len();
            if len > 0 {
                import.format_index = (import.format_index + 1) % len;
            }
        }
        (0, Code::Up | Code::BackTab) | (0, Code::Char('p'))
            if key.code != Code::Char('p') || ctrl =>
        {
            let len = import.filtered_formats().len();
            if len > 0 {
                import.format_index = (import.format_index + len - 1) % len;
            }
        }
        (0, Code::Backspace) => {
            import.filter.pop();
            import.format_index = 0;
        }
        (0, Code::Char(c)) if key.is_printable() => {
            import.filter.push(c);
            import.format_index = 0;
        }
        (0, Code::Enter) => {
            let Some(format) =
                import.filtered_formats().get(import.format_index).map(|f| (*f).to_owned())
            else {
                import.error = Some("No format matches the filter.".to_owned());
                return Vec::new();
            };
            import.format = format;
            import.password_focus = false;
            import.error = None;
            import.step = 1;
        }
        // Step 1: file (and password for protected files).
        (1, Code::Tab | Code::BackTab) => {
            if import.format == "bitwardenpasswordprotected" {
                import.password_focus = !import.password_focus;
            } else {
                import.path = complete_path(&import.path);
            }
        }
        (1, Code::Backspace) => {
            if import.password_focus {
                import.password.pop();
            } else {
                import.path.pop();
            }
        }
        (1, Code::Char(c)) if key.is_printable() => {
            if import.password_focus {
                import.password.push(c);
            } else {
                import.path.push(c);
            }
        }
        (1, Code::Enter) => {
            if import.path.is_empty() {
                import.error = Some("Enter a file path.".to_owned());
                return Vec::new();
            }
            if import.format == "bitwardenpasswordprotected" && import.password.is_empty() {
                import.error = Some("Enter the file password.".to_owned());
                import.password_focus = true;
                return Vec::new();
            }
            if import.format == "bitwardenjson" {
                // Bitwarden JSON gets a preview with duplicate detection.
                match read_import_items(&import.path) {
                    Ok((items, folders)) => {
                        let known: BTreeSet<String> =
                            state.cache.folders().iter().map(|f| f.name.clone()).collect();
                        import.new_folders = folders
                            .iter()
                            .map(|f| f.name.clone())
                            .filter(|name| !known.contains(name))
                            .collect();
                        import.duplicates = crate::vault::dupes::find(&state.cache, &items);
                        import.skip = import.duplicates.iter().map(|d| d.index).collect();
                        import.skip_enabled = !import.duplicates.is_empty();
                        import.count = items.len();
                        import.preview = items;
                        import.folders = folders;
                        import.index = 0;
                        import.error = None;
                        import.step = 2;
                    }
                    Err(err) => import.error = Some(err),
                }
                return Vec::new();
            }
            return start_import(state);
        }
        // Step 2: preview.
        (2, Code::Char('j') | Code::Down) => {
            if import.index + 1 < import.duplicates.len() {
                import.index += 1;
            }
        }
        (2, Code::Char('k') | Code::Up) => import.index = import.index.saturating_sub(1),
        (2, Code::Char('x' | ' ')) => {
            if let Some(duplicate) = import.duplicates.get(import.index) {
                let index = duplicate.index;
                if !import.skip.remove(&index) {
                    import.skip.insert(index);
                }
            }
        }
        (2, Code::Char('a')) => {
            let all = import.duplicates.iter().all(|d| import.skip.contains(&d.index));
            import.skip = if all {
                BTreeSet::new()
            } else {
                import.duplicates.iter().map(|d| d.index).collect()
            };
        }
        (2, Code::Char('s')) => import.skip_enabled = !import.skip_enabled,
        (2, Code::Enter) => return start_import(state),
        _ => {}
    }
    Vec::new()
}

/// Runs `bw import` for the wizard's current choices; the wizard stays open
/// (busy) until the CLI answers.
pub(crate) fn start_import(state: &mut AppState) -> Vec<Effect> {
    let Some(import) = &mut state.import else { return Vec::new() };
    let format = import.format.clone();
    let mut path = PathBuf::from(shellexpand_home(&import.path));
    let mut temporary = false;
    let skipped = if import.step == 2 && import.skip_enabled { import.skip.len() } else { 0 };
    if skipped > 0 {
        match write_filtered_import(&import.preview, &import.folders, &import.skip) {
            Ok(filtered) => {
                path = filtered;
                temporary = true;
            }
            Err(err) => {
                import.error = Some(err);
                return Vec::new();
            }
        }
    }
    let password = (format == "bitwardenpasswordprotected").then(|| import.password.take_secret());
    let password_protected = password.is_some();
    import.busy = true;
    import.error = None;
    let imported = if import.step == 2 { import.count.saturating_sub(skipped) } else { 0 };
    state.last_import = (import.step == 2).then_some((imported, skipped));
    vec![request(
        state,
        CommandKind::Import(ImportRequest {
            format,
            path,
            password,
            password_protected,
            temporary,
        }),
        Pending::Import,
    )]
}

/// Completes a path to the longest common prefix of the matching entries
/// (directories get a trailing `/`).
pub(crate) fn complete_path(input: &str) -> String {
    let expanded = shellexpand_home(input);
    let (dir, prefix) = match expanded.rfind('/') {
        Some(index) => (expanded[..=index].to_owned(), expanded[index + 1..].to_owned()),
        None => ("./".to_owned(), expanded.clone()),
    };
    let Ok(entries) = std::fs::read_dir(&dir) else { return input.to_owned() };
    let mut matches: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            Some(if is_dir { format!("{name}/") } else { name })
        })
        .collect();
    matches.sort();
    let Some(first) = matches.first() else { return input.to_owned() };
    let mut common = first.clone();
    for other in &matches[1..] {
        while !other.starts_with(&common) {
            common.pop();
        }
    }
    if common.len() <= prefix.len() {
        return input.to_owned();
    }
    let completed =
        format!("{}{common}", if dir == "./" && !input.starts_with("./") { "" } else { &dir });
    // Keep the `~` the user typed.
    match directories::BaseDirs::new().map(|b| b.home_dir().display().to_string()) {
        Some(home) if input.starts_with('~') => completed.replacen(&home, "~", 1),
        _ => completed,
    }
}

pub(crate) fn read_import_items(
    path: &str,
) -> Result<(Vec<Item>, Vec<crate::bw::models::Folder>), String> {
    let path = PathBuf::from(shellexpand_home(path));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Not valid JSON: {e}"))?;
    if value.get("encrypted").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(
            "This file is password protected. Choose bitwardenpasswordprotected.".to_owned()
        );
    }
    let items: Vec<Item> = value
        .get("items")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("Not a Bitwarden export: {e}"))?
        .unwrap_or_default();
    let folders: Vec<crate::bw::models::Folder> = value
        .get("folders")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("Not a Bitwarden export: {e}"))?
        .unwrap_or_default();
    Ok((items, folders))
}

pub(crate) fn write_filtered_import(
    items: &[Item],
    folders: &[crate::bw::models::Folder],
    skip: &BTreeSet<usize>,
) -> Result<PathBuf, String> {
    let kept: Vec<&Item> = items
        .iter()
        .enumerate()
        .filter(|(index, _)| !skip.contains(index))
        .map(|(_, item)| item)
        .collect();
    let (path, mut file) = crate::security::secure_temp_file("import")
        .map_err(|e| format!("Couldn't create the filtered import: {e}"))?;
    use std::io::Write;
    // Keep the folders: items reference them by id.
    let body = serde_json::json!({ "encrypted": false, "folders": folders, "items": kept });
    file.write_all(serde_json::to_string(&body).unwrap_or_default().as_bytes())
        .map_err(|e| format!("Couldn't write the filtered import: {e}"))?;
    file.flush().map_err(|e| e.to_string())?;
    Ok(path)
}
