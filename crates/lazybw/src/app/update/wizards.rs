//! Export and import wizards.

use super::{
    AppState, BTreeSet, CommandKind, Effect, ExportFormat, ImportPreview, ImportRequest,
    ImportStep, Item, Key, Overlay, PathBuf, Pending, Screen, ctrl_np_as_arrows, request,
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
    state.open(crate::app::state::ExportState { format, path, ..Default::default() });
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
    let step = if args.is_empty() { ImportStep::Format } else { ImportStep::Path };
    state.open(crate::app::state::ImportState {
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
    use crate::app::state::ExportStep::{
        ConfirmPassword, Destination, FilePassword, Format, MasterPassword,
    };
    use crate::config::keymap::Code;
    let key = ctrl_np_as_arrows(key);
    if key.code == Code::Esc {
        state.take_export();
        return Vec::new();
    }
    // While the CLI works the wizard only shows progress; Esc still closes it.
    if state.exporting() {
        return Vec::new();
    }
    let Some(Overlay::Export(export)) = &mut state.overlay else { return Vec::new() };
    match (export.step, key.code) {
        (Format, Code::Char('j') | Code::Down) => {
            export.path =
                with_extension(&export.path, export.format, next_format(export.format, true));
            export.format = next_format(export.format, true);
        }
        (Format, Code::Char('k') | Code::Up) => {
            export.path =
                with_extension(&export.path, export.format, next_format(export.format, false));
            export.format = next_format(export.format, false);
        }
        (Format, Code::Tab | Code::BackTab) => export.step = Destination,
        (Destination, Code::Tab) => export.path = complete_path(&export.path),
        (Destination, Code::BackTab) => export.step = Format,
        (FilePassword, Code::Tab | Code::BackTab) => export.step = ConfirmPassword,
        (ConfirmPassword, Code::Tab | Code::BackTab) => export.step = FilePassword,
        (Format | Destination, Code::Enter) => {
            // An existing file is replaced only on a second Enter.
            let target = shellexpand_home(&export.path);
            let exists = std::path::Path::new(&target).exists();
            if exists && export.overwrite.as_deref() != Some(target.as_str()) {
                export.error = Some(format!("{} exists. Enter again replaces it.", export.path));
                export.overwrite = Some(target);
                return Vec::new();
            }
            export.error = None;
            export.step =
                if export.format.needs_password() { FilePassword } else { MasterPassword };
        }
        (FilePassword | ConfirmPassword, Code::Enter) => {
            if export.password != export.confirmation {
                export.error = Some("The passwords do not match.".to_owned());
            } else if export.password.is_empty() {
                export.error = Some("Enter a file password.".to_owned());
            } else {
                export.error = None;
                export.step = MasterPassword;
            }
        }
        (MasterPassword, Code::Enter) => {
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
            return vec![request(
                state,
                CommandKind::Export { format, destination, password, master },
                Pending::Export,
            )];
        }
        (Destination, _) if crate::input::line::edit_at_end(&mut export.path, key) => {}
        (FilePassword | ConfirmPassword | MasterPassword, _) if super::is_ctrl(key, 'u') => {
            match export.step {
                FilePassword => export.password.clear(),
                ConfirmPassword => export.confirmation.clear(),
                _ => export.master.clear(),
            }
        }
        (_, Code::Backspace) => export.backspace(),
        // Printable keys are text in every field, `j` and `k` included;
        // only the format list reads them as moves.
        (_, Code::Char(c)) if key.is_printable() => export.insert(c.encode_utf8(&mut [0; 4])),
        _ => {}
    }
    Vec::new()
}

/// The destination with the extension of the new format, when it still
/// has the old one (a path the user typed with another name is kept).
pub(crate) fn with_extension(path: &str, from: ExportFormat, to: ExportFormat) -> String {
    match path.strip_suffix(&format!(".{}", from.extension())) {
        Some(stem) if from.extension() != to.extension() => format!("{stem}.{}", to.extension()),
        _ => path.to_owned(),
    }
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
    if state.importing() {
        // The CLI keeps going; its answer arrives as a message either way.
        if key.code == crate::config::keymap::Code::Esc {
            state.overlay = None;
            state.message(
                crate::app::state::MessageLevel::Info,
                "The import goes on; lazybw says when it is done.",
            );
        }
        return Vec::new();
    }
    let Some(Overlay::Import(import)) = &mut state.overlay else { return Vec::new() };
    let ctrl = key.mods.contains(Mods::CTRL);
    match (&mut import.step, key.code) {
        // Esc goes back a step, and closes the wizard from the first.
        (ImportStep::Format, Code::Esc) => state.overlay = None,
        (ImportStep::Path | ImportStep::Password, Code::Esc) => {
            import.step = ImportStep::Format;
            import.error = None;
        }
        (ImportStep::Preview(_), Code::Esc) => {
            import.step = ImportStep::Path;
            import.error = None;
        }
        // Step one: the filterable format list.
        (ImportStep::Format, Code::Down | Code::Tab) | (ImportStep::Format, Code::Char('n'))
            if key.code != Code::Char('n') || ctrl =>
        {
            let len = import.filtered_formats().len();
            if len > 0 {
                import.format_index = (import.format_index + 1) % len;
            }
        }
        (ImportStep::Format, Code::Up | Code::BackTab) | (ImportStep::Format, Code::Char('p'))
            if key.code != Code::Char('p') || ctrl =>
        {
            let len = import.filtered_formats().len();
            if len > 0 {
                import.format_index = (import.format_index + len - 1) % len;
            }
        }
        (ImportStep::Format, _) if crate::input::line::edit_at_end(&mut import.filter, key) => {
            import.format_index = 0;
        }
        (ImportStep::Format, Code::Enter) => {
            let Some(format) =
                import.filtered_formats().get(import.format_index).map(|f| (*f).to_owned())
            else {
                import.error = Some("No format matches the filter.".to_owned());
                return Vec::new();
            };
            import.format = format;
            import.error = None;
            import.step = ImportStep::Path;
        }
        // Step two: the file, and the password for protected files.
        (ImportStep::Path, Code::Tab | Code::BackTab) => {
            if import.needs_password() {
                import.step = ImportStep::Password;
            } else {
                import.path = complete_path(&import.path);
            }
        }
        (ImportStep::Password, Code::Tab | Code::BackTab) => import.step = ImportStep::Path,
        (ImportStep::Path, _) if crate::input::line::edit_at_end(&mut import.path, key) => {}
        (ImportStep::Password, Code::Backspace) => {
            import.password.pop();
        }
        (ImportStep::Password, _) if super::is_ctrl(key, 'u') => import.password.clear(),
        (ImportStep::Password, Code::Char(c)) if key.is_printable() => import.password.push(c),
        (ImportStep::Path | ImportStep::Password, Code::Enter) => {
            if import.path.is_empty() {
                import.error = Some("Enter a file path.".to_owned());
                import.step = ImportStep::Path;
                return Vec::new();
            }
            if import.needs_password() && import.password.is_empty() {
                import.error = Some("Enter the file password.".to_owned());
                import.step = ImportStep::Password;
                return Vec::new();
            }
            if import.format == "bitwardenjson" {
                // Bitwarden JSON gets a preview with duplicate detection.
                match read_import_items(&import.path) {
                    Ok((items, folders)) => {
                        let known: BTreeSet<String> =
                            state.cache.folders().iter().map(|f| f.name.clone()).collect();
                        let duplicates = crate::vault::dupes::find(&state.cache, &items);
                        import.step = ImportStep::Preview(ImportPreview {
                            new_folders: folders
                                .iter()
                                .map(|f| f.name.clone())
                                .filter(|name| !known.contains(name))
                                .collect(),
                            skip: duplicates.iter().map(|d| d.index).collect(),
                            skip_enabled: !duplicates.is_empty(),
                            duplicates,
                            items,
                            folders,
                            index: 0,
                        });
                        import.error = None;
                    }
                    Err(err) => import.error = Some(err),
                }
                return Vec::new();
            }
            return start_import(state);
        }
        // Step three: the preview.
        (ImportStep::Preview(preview), Code::Char('j') | Code::Down) => {
            if preview.index + 1 < preview.duplicates.len() {
                preview.index += 1;
            }
        }
        (ImportStep::Preview(preview), Code::Char('k') | Code::Up) => {
            preview.index = preview.index.saturating_sub(1);
        }
        (ImportStep::Preview(preview), Code::Char('x' | ' ')) => {
            if let Some(duplicate) = preview.duplicates.get(preview.index) {
                let index = duplicate.index;
                if !preview.skip.remove(&index) {
                    preview.skip.insert(index);
                }
            }
        }
        (ImportStep::Preview(preview), Code::Char('a')) => {
            let all = preview.duplicates.iter().all(|d| preview.skip.contains(&d.index));
            preview.skip = if all {
                BTreeSet::new()
            } else {
                preview.duplicates.iter().map(|d| d.index).collect()
            };
        }
        (ImportStep::Preview(preview), Code::Char('s')) => {
            preview.skip_enabled = !preview.skip_enabled;
        }
        (ImportStep::Preview(_), Code::Enter) => return start_import(state),
        _ => {}
    }
    Vec::new()
}

/// Runs `bw import` for the wizard's current choices; the wizard stays open
/// (and ignores keys) until the CLI answers.
pub(crate) fn start_import(state: &mut AppState) -> Vec<Effect> {
    let Some(Overlay::Import(import)) = &mut state.overlay else { return Vec::new() };
    let format = import.format.clone();
    let mut path = PathBuf::from(shellexpand_home(&import.path));
    let mut temporary = false;
    let preview = import.preview();
    let skipped = preview.map_or(0, ImportPreview::skipped);
    // Only the preview knows what the file holds; other formats report the
    // CLI's own message.
    let counts = preview.map(|preview| (preview.items.len().saturating_sub(skipped), skipped));
    let filtered = preview
        .filter(|_| skipped > 0)
        .map(|preview| write_filtered_import(&preview.items, &preview.folders, &preview.skip));
    match filtered {
        Some(Ok(filtered)) => {
            path = filtered;
            temporary = true;
        }
        Some(Err(err)) => {
            import.error = Some(err);
            return Vec::new();
        }
        None => {}
    }
    state.last_import = counts;
    let password = import.needs_password().then(|| import.password.take_secret());
    let password_protected = password.is_some();
    import.error = None;
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
