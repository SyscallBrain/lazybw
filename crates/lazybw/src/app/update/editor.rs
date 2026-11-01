//! The item editor form and the notes editor.

use super::{
    Action, AppState, BTreeMap, Card, CommandKind, ConfirmState, DetailState, EditorState,
    EditorTarget, Effect, Field, FieldKind, FormKey, FormKind, FormRow, Identity, Item, ItemType,
    Key, Login, LoginUri, MessageLevel, Pending, PickerEntry, PickerKind, PickerState, PromptState,
    Screen, SshKey, SuspendTask, UriMatch, apply, current_folder_id, request,
};

pub(crate) fn open_editor(state: &mut AppState, item: Item, is_new: bool) {
    let rows = build_rows(state, &item);
    state.editor = Some(EditorState {
        item: item.clone(),
        is_new,
        original_item: item,
        rows,
        row: 0,
        offset: 0,
        insert: false,
        cursor: 0,
        revealed: false,
        dirty: is_new,
        errors: BTreeMap::new(),
        notes_offset: 0,
        remote_change: false,
        history: crate::app::state::EditorHistory::default(),
    });
    state.screen = Screen::Editor;
    state.detail = DetailState::default();
}

pub(crate) fn open_new_editor(state: &mut AppState, kind: ItemType) {
    let mut item = Item::new(kind);
    item.folder_id = current_folder_id(state);
    open_editor(state, item, true);
}

pub(crate) fn close_editor(state: &mut AppState) {
    if let Some(editor) = state.editor.as_ref().filter(|e| e.dirty) {
        let name = if editor.item.name.trim().is_empty() {
            "the new item".to_owned()
        } else {
            format!("“{}”", editor.item.name)
        };
        state.confirm = Some(ConfirmState {
            title: "Unsaved changes".to_owned(),
            message: format!("Save changes to {name}?"),
            confirm_label: "Save".to_owned(),
            danger: false,
            requires_word: None,
            input: String::new(),
            also_subfolders: false,
            subfolder_count: 0,
            on_deny: Some(Action::new("editor.discard")),
            on_confirm: Action::new("editor.save_close"),
        });
        return;
    }
    state.editor = None;
    state.screen = Screen::Vault;
}

pub(crate) fn rebuild_rows(state: &mut AppState) {
    let Some(editor) = &state.editor else { return };
    let item = editor.item.clone();
    let selected_key = editor.rows.get(editor.row).map(|r| r.key);
    let rows = build_rows(state, &item);
    if let Some(editor) = &mut state.editor {
        editor.rows = rows;
        editor.row = selected_key
            .and_then(|key| editor.rows.iter().position(|r| r.key == key))
            .unwrap_or(editor.row)
            .min(editor.rows.len().saturating_sub(1));
    }
}

pub(crate) fn build_rows(state: &AppState, item: &Item) -> Vec<FormRow> {
    let mut rows = Vec::new();
    rows.push(FormRow {
        key: FormKey::Name,
        label: "Name".to_owned(),
        value: item.name.clone(),
        kind: FormKind::Text,
        section: Some("CREDENTIALS"),
        hint: None,
    });
    let folder_name =
        item.folder_id.as_ref().and_then(|id| state.cache.folder_path(id)).unwrap_or("").to_owned();
    rows.push(FormRow {
        key: FormKey::Folder,
        label: "Folder".to_owned(),
        value: folder_name,
        kind: FormKind::Option(
            std::iter::once("No folder".to_owned())
                .chain(state.cache.folders().iter().map(|f| f.name.clone()))
                .collect(),
        ),
        section: None,
        hint: Some("Enter pick".to_owned()),
    });
    match item.kind {
        ItemType::Login => {
            let login = item.login.clone().unwrap_or_default();
            rows.push(FormRow {
                key: FormKey::Username,
                label: "Username".to_owned(),
                value: login.username.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::Password,
                label: "Password".to_owned(),
                value: login.password.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Secret,
                section: None,
                hint: Some("Ctrl-g generate".to_owned()),
            });
            rows.push(FormRow {
                key: FormKey::Totp,
                label: "Authenticator key".to_owned(),
                value: login.totp.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
            let uris = login.uris.clone().unwrap_or_default();
            for (index, uri) in uris.iter().enumerate() {
                rows.push(FormRow {
                    key: FormKey::Uri(index),
                    label: format!("Website {}", index + 1),
                    value: uri.uri_str().to_owned(),
                    kind: FormKind::Text,
                    section: if index == 0 { Some("WEBSITES") } else { None },
                    hint: None,
                });
                rows.push(FormRow {
                    key: FormKey::UriMatch(index),
                    label: "  match".to_owned(),
                    value: uri.match_kind().label().to_owned(),
                    kind: FormKind::Option(
                        [
                            "Default",
                            "Base domain",
                            "Host",
                            "Starts with",
                            "Exact",
                            "Regular expression",
                            "Never",
                        ]
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                    ),
                    section: None,
                    hint: None,
                });
            }
            rows.push(FormRow {
                key: FormKey::UriAdd,
                label: "  + Add website".to_owned(),
                value: String::new(),
                kind: FormKind::Action,
                section: if uris.is_empty() { Some("WEBSITES") } else { None },
                hint: None,
            });
        }
        ItemType::Card => {
            let card = item.card.clone().unwrap_or_default();
            rows.push(FormRow {
                key: FormKey::Cardholder,
                label: "Cardholder name".to_owned(),
                value: card.cardholder_name.unwrap_or_default(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::CardNumber,
                label: "Number".to_owned(),
                value: card.number.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::CardBrand,
                label: "Brand".to_owned(),
                value: card.brand.unwrap_or_default(),
                kind: FormKind::Option(
                    [
                        "Visa",
                        "Mastercard",
                        "Amex",
                        "Discover",
                        "Diners Club",
                        "JCB",
                        "Maestro",
                        "UnionPay",
                        "RuPay",
                        "Other",
                    ]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
                ),
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::ExpMonth,
                label: "Expiration month".to_owned(),
                value: card.exp_month.unwrap_or_default(),
                kind: FormKind::Option((1..=12).map(|m| format!("{m:02}")).collect()),
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::ExpYear,
                label: "Expiration year".to_owned(),
                value: card.exp_year.unwrap_or_default(),
                kind: FormKind::Text,
                section: None,
                hint: Some("Ctrl-a/Ctrl-x".to_owned()),
            });
            rows.push(FormRow {
                key: FormKey::CardCode,
                label: "Security code".to_owned(),
                value: card.code.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
        }
        ItemType::Identity => {
            let identity = item.identity.clone().unwrap_or_default();
            let rows_data: Vec<(FormKey, &str, Option<String>, FormKind)> = vec![
                (
                    FormKey::IdTitle,
                    "Title",
                    identity.title,
                    FormKind::Option(vec![
                        "Mr".into(),
                        "Mrs".into(),
                        "Ms".into(),
                        "Mx".into(),
                        "Dr".into(),
                    ]),
                ),
                (FormKey::IdFirst, "First name", identity.first_name, FormKind::Text),
                (FormKey::IdMiddle, "Middle name", identity.middle_name, FormKind::Text),
                (FormKey::IdLast, "Last name", identity.last_name, FormKind::Text),
                (FormKey::IdUsername, "Username", identity.username, FormKind::Text),
                (FormKey::IdCompany, "Company", identity.company, FormKind::Text),
                (FormKey::IdEmail, "Email", identity.email, FormKind::Text),
                (FormKey::IdPhone, "Phone", identity.phone, FormKind::Text),
                (FormKey::IdAddress1, "Address 1", identity.address1, FormKind::Text),
                (FormKey::IdAddress2, "Address 2", identity.address2, FormKind::Text),
                (FormKey::IdAddress3, "Address 3", identity.address3, FormKind::Text),
                (FormKey::IdCity, "City", identity.city, FormKind::Text),
                (FormKey::IdState, "State", identity.state, FormKind::Text),
                (FormKey::IdPostal, "Postal code", identity.postal_code, FormKind::Text),
                (FormKey::IdCountry, "Country", identity.country, FormKind::Text),
                (
                    FormKey::IdSsn,
                    "SSN",
                    identity.ssn.map(|s| s.expose().to_owned()),
                    FormKind::Secret,
                ),
                (
                    FormKey::IdPassport,
                    "Passport number",
                    identity.passport_number.map(|s| s.expose().to_owned()),
                    FormKind::Secret,
                ),
                (
                    FormKey::IdLicense,
                    "Licence number",
                    identity.license_number.map(|s| s.expose().to_owned()),
                    FormKind::Secret,
                ),
            ];
            for (index, (key, label, value, kind)) in rows_data.into_iter().enumerate() {
                rows.push(FormRow {
                    key,
                    label: label.to_owned(),
                    value: value.unwrap_or_default(),
                    kind,
                    section: if index == 0 { Some("IDENTITY") } else { None },
                    hint: None,
                });
            }
        }
        ItemType::SecureNote => {}
        ItemType::SshKey => {
            let key = item.ssh_key.clone().unwrap_or_default();
            rows.push(FormRow {
                key: FormKey::SshGenerate,
                label: "Generate Ed25519 key".to_owned(),
                value: String::new(),
                kind: FormKind::Action,
                section: Some("SSH KEY"),
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshImport,
                label: "Import from file…".to_owned(),
                value: String::new(),
                kind: FormKind::Action,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshPublic,
                label: "Public key".to_owned(),
                value: key.public_key,
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshFingerprint,
                label: "Fingerprint".to_owned(),
                value: key.key_fingerprint,
                kind: FormKind::ReadOnly,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshPrivate,
                label: "Private key".to_owned(),
                value: key.private_key.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
        }
        ItemType::Other(_) => {}
    }
    // Options.
    rows.push(FormRow {
        key: FormKey::Favorite,
        label: "Favorite".to_owned(),
        value: item.favorite.to_string(),
        kind: FormKind::Bool,
        section: Some("OPTIONS"),
        hint: None,
    });
    rows.push(FormRow {
        key: FormKey::Reprompt,
        label: "Master password ask".to_owned(),
        value: (item.reprompt == 1).to_string(),
        kind: FormKind::Bool,
        section: None,
        hint: None,
    });
    // Custom fields.
    let fields = item.fields.clone().unwrap_or_default();
    for (index, field) in fields.iter().enumerate() {
        rows.push(FormRow {
            key: FormKey::FieldName(index),
            label: "Field name".to_owned(),
            value: field.name_str().to_owned(),
            kind: FormKind::Text,
            section: if index == 0 { Some("CUSTOM FIELDS") } else { None },
            hint: None,
        });
        rows.push(FormRow {
            key: FormKey::FieldType(index),
            label: "  type".to_owned(),
            value: field.kind().label().to_owned(),
            kind: FormKind::Option(vec![
                "text".into(),
                "hidden".into(),
                "boolean".into(),
                "linked".into(),
            ]),
            section: None,
            hint: None,
        });
        rows.push(FormRow {
            key: FormKey::FieldValue(index),
            label: "  value".to_owned(),
            value: field.value.as_ref().map(|v| v.expose().to_owned()).unwrap_or_default(),
            kind: match field.kind() {
                FieldKind::Hidden => FormKind::Secret,
                FieldKind::Boolean => FormKind::Bool,
                FieldKind::Linked => FormKind::ReadOnly,
                FieldKind::Text => FormKind::Text,
            },
            section: None,
            hint: None,
        });
    }
    rows.push(FormRow {
        key: FormKey::FieldAdd,
        label: "  + Add custom field".to_owned(),
        value: String::new(),
        kind: FormKind::Action,
        section: if fields.is_empty() { Some("CUSTOM FIELDS") } else { None },
        hint: None,
    });
    rows.push(FormRow {
        key: FormKey::Notes,
        label: "Notes".to_owned(),
        value: item.notes.as_ref().map(|n| n.expose().to_owned()).unwrap_or_default(),
        kind: FormKind::Notes,
        section: Some("NOTES"),
        hint: Some("Ctrl-e $EDITOR".to_owned()),
    });
    // Left column first, then the right one, so j/k walk each column top to
    // bottom instead of zig-zagging between them (the sort is stable).
    rows.sort_by_key(|row| row.key.in_right_column());
    rows
}

pub(crate) fn editor_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action.id.as_str() {
        "editor.save" => return editor_save(state, false),
        "editor.save_close" => return editor_save(state, true),
        "editor.discard" => {
            state.editor = None;
            state.notes = None;
            state.screen = Screen::Vault;
            return Vec::new();
        }
        "editor.column_left" | "editor.column_right" => {
            let width = crate::ui::layout::editor_content_width(
                state.size,
                state.sidebar_visible,
                state.sidebar_width,
                state.list_width,
            );
            if !crate::ui::layout::editor_two_columns(width) {
                return Vec::new();
            }
            let Some(editor) = &mut state.editor else { return Vec::new() };
            let Some(current) = editor.rows.get(editor.row) else { return Vec::new() };
            let right = current.key.in_right_column();
            let to_right = action.id == "editor.column_right";
            if right == to_right {
                return Vec::new();
            }
            // Same position down the other column, clamped to its length.
            let position = editor.rows[..editor.row]
                .iter()
                .filter(|row| row.key.in_right_column() == right)
                .count();
            let targets: Vec<usize> = editor
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.key.in_right_column() == to_right)
                .map(|(index, _)| index)
                .collect();
            if let Some(&target) = targets.get(position).or(targets.last()) {
                editor.row = target;
            }
            return Vec::new();
        }
        "editor.close" => {
            close_notes(state);
            close_editor(state);
            return Vec::new();
        }
        "editor.reload" => {
            if let Some(id) = state.editor.as_ref().and_then(|e| e.item.id.clone()) {
                return vec![request(state, CommandKind::GetItem(id), Pending::GetItem)];
            }
            return Vec::new();
        }
        "editor.normal" => {
            if let Some(editor) = &mut state.editor {
                editor.insert = false;
                editor.cursor = editor.cursor.saturating_sub(1);
            }
            return Vec::new();
        }
        "editor.insert" | "editor.append" | "editor.insert_start" | "editor.append_end" => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            let len = editor.rows.get(editor.row).map(|r| r.value.chars().count()).unwrap_or(0);
            editor.insert = true;
            editor.cursor = match action.id.as_str() {
                "editor.insert" => editor.cursor.min(len),
                "editor.append" => (editor.cursor + 1).min(len),
                "editor.insert_start" => 0,
                _ => len,
            };
            return Vec::new();
        }
        "editor.change_field" => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            if let Some(row) = editor.rows.get_mut(editor.row)
                && matches!(row.kind, FormKind::Text | FormKind::Secret | FormKind::Notes)
            {
                row.value.clear();
                editor.cursor = 0;
                editor.insert = true;
            }
            return Vec::new();
        }
        "editor.next_field" | "editor.prev_field" | "editor.first_field" | "editor.last_field" => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            if !editor.rows.is_empty() {
                editor.insert = false;
                editor.row = match action.id.as_str() {
                    "editor.next_field" => (editor.row + 1) % editor.rows.len(),
                    "editor.prev_field" => (editor.row + editor.rows.len() - 1) % editor.rows.len(),
                    "editor.first_field" => 0,
                    _ => editor.rows.len() - 1,
                };
                editor.cursor = editor.rows[editor.row].value.chars().count();
            }
            return Vec::new();
        }
        "editor.row_add_below" | "editor.row_add_above" => {
            return editor_add_row(state);
        }
        "editor.row_delete" => {
            return editor_delete_row(state);
        }
        "editor.cycle_next" | "editor.cycle_prev" => {
            return editor_cycle(state, action.id == "editor.cycle_next");
        }
        "editor.activate" => {
            return editor_activate(state);
        }
        "editor.generate" => {
            // Ctrl-g fills a password: aim at the row under the cursor when
            // it is a secret, otherwise at the item's password field.
            if let Some(editor) = &mut state.editor {
                let on_secret = editor
                    .rows
                    .get(editor.row)
                    .is_some_and(|row| matches!(row.kind, FormKind::Secret));
                if !on_secret {
                    match editor.rows.iter().position(|r| r.key == FormKey::Password) {
                        Some(index) => editor.row = index,
                        None => {
                            state.message(
                                MessageLevel::Info,
                                "Move to a hidden field to generate a value for it.",
                            );
                            return Vec::new();
                        }
                    }
                }
                editor.insert = false;
            }
            state.generator.popup = true;
            return vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )];
        }
        "editor.external" => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            let key = editor.rows.get(editor.row).map(|r| r.key);
            if key == Some(FormKey::SshPrivate) {
                let pem = editor
                    .item
                    .ssh_key
                    .as_ref()
                    .and_then(|k| k.private_key.as_ref())
                    .cloned()
                    .unwrap_or_default();
                return vec![Effect::Suspend(SuspendTask::Editor {
                    target: EditorTarget::SshPrivateKey(pem),
                })];
            }
            // Everywhere else Ctrl-e edits the notes (the only multi-line
            // field); move the cursor there so the result is in view.
            if let Some(index) = editor.rows.iter().position(|r| r.key == FormKey::Notes) {
                editor.row = index;
            }
            let text = editor.item.notes.clone().unwrap_or_default();
            return vec![Effect::Suspend(SuspendTask::Editor {
                target: EditorTarget::Notes(text),
            })];
        }
        "editor.undo" | "editor.redo" => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            let undo = action.id == "editor.undo";
            let step = if undo { editor.history.undo.pop() } else { editor.history.redo.pop() };
            let Some(step) = step else {
                state.message(
                    MessageLevel::Info,
                    if undo { "Already at oldest change." } else { "Already at newest change." },
                );
                return Vec::new();
            };
            let current = editor.snapshot();
            if undo {
                editor.history.redo.push(current);
            } else {
                editor.history.undo.push(current);
            }
            editor.history.restoring = true;
            editor.history.revision += 1;
            editor.insert = false;
            editor.restore(step);
            return Vec::new();
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn editor_insert_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    // Control keys leave INSERT or open tools; everything else inserts.
    match key.code {
        Code::Esc => {
            if let Some(editor) = &mut state.editor {
                editor.insert = false;
                sync_editor(state);
            }
            Vec::new()
        }
        Code::Tab | Code::Enter => editor_action(
            state,
            Action::new(if key.code == Code::Tab {
                "editor.next_field"
            } else if key.mods.contains(crate::config::keymap::Mods::CTRL) {
                "editor.activate"
            } else {
                "editor.next_field"
            }),
        ),
        Code::BackTab => editor_action(state, Action::new("editor.prev_field")),
        Code::Backspace => {
            editor_value_backspace(state);
            Vec::new()
        }
        Code::Delete => {
            editor_value_delete(state);
            Vec::new()
        }
        Code::Left => {
            if let Some(editor) = &mut state.editor {
                editor.cursor = editor.cursor.saturating_sub(1);
            }
            Vec::new()
        }
        Code::Right => {
            if let Some(editor) = &mut state.editor {
                let len = editor.rows.get(editor.row).map(|r| r.value.chars().count()).unwrap_or(0);
                editor.cursor = (editor.cursor + 1).min(len);
            }
            Vec::new()
        }
        Code::Home => {
            if let Some(editor) = &mut state.editor {
                editor.cursor = 0;
            }
            Vec::new()
        }
        Code::End => {
            if let Some(editor) = &mut state.editor {
                editor.cursor =
                    editor.rows.get(editor.row).map(|r| r.value.chars().count()).unwrap_or(0);
            }
            Vec::new()
        }
        Code::Char('g') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            editor_action(state, Action::new("editor.generate"))
        }
        Code::Char('e') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            editor_action(state, Action::new("editor.external"))
        }
        Code::Char('s') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            editor_action(state, Action::new("editor.save"))
        }
        Code::Char('a') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            editor_action(state, Action::new("editor.cycle_next"))
        }
        Code::Char('x') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            editor_action(state, Action::new("editor.cycle_prev"))
        }
        Code::Char(c) if key.is_printable() => {
            editor_value_insert(state, &c.to_string());
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn editor_value_insert(state: &mut AppState, text: &str) {
    let Some(editor) = &mut state.editor else { return };
    let Some(row) = editor.rows.get_mut(editor.row) else { return };
    let mut chars: Vec<char> = row.value.chars().collect();
    let at = editor.cursor.min(chars.len());
    for (offset, ch) in text.chars().enumerate() {
        chars.insert(at + offset, ch);
    }
    editor.cursor = at + text.chars().count();
    row.value = chars.into_iter().collect();
    editor.dirty = true;
    sync_editor(state);
}

pub(crate) fn editor_value_backspace(state: &mut AppState) {
    let Some(editor) = &mut state.editor else { return };
    let Some(row) = editor.rows.get_mut(editor.row) else { return };
    let mut chars: Vec<char> = row.value.chars().collect();
    if editor.cursor > 0 && editor.cursor <= chars.len() {
        let at = editor.cursor - 1;
        chars.remove(at);
        editor.cursor = at;
        row.value = chars.into_iter().collect();
        editor.dirty = true;
        sync_editor(state);
    }
}

pub(crate) fn editor_value_delete(state: &mut AppState) {
    let Some(editor) = &mut state.editor else { return };
    let Some(row) = editor.rows.get_mut(editor.row) else { return };
    let mut chars: Vec<char> = row.value.chars().collect();
    if editor.cursor < chars.len() {
        chars.remove(editor.cursor);
        row.value = chars.into_iter().collect();
        editor.dirty = true;
        sync_editor(state);
    }
}

pub(crate) fn editor_cycle(state: &mut AppState, forward: bool) -> Vec<Effect> {
    let Some(editor) = &mut state.editor else { return Vec::new() };
    let Some(row) = editor.rows.get_mut(editor.row) else { return Vec::new() };
    match &row.kind {
        FormKind::Option(options) => {
            if options.is_empty() {
                return Vec::new();
            }
            let index = options.iter().position(|o| o == &row.value).unwrap_or(0);
            let next = if forward {
                (index + 1) % options.len()
            } else {
                (index + options.len() - 1) % options.len()
            };
            row.value = options[next].clone();
            editor.dirty = true;
        }
        FormKind::Bool => {
            row.value = if row.value == "true" { "false".to_owned() } else { "true".to_owned() };
            editor.dirty = true;
        }
        _ => {}
    }
    sync_editor(state);
    Vec::new()
}

pub(crate) fn editor_activate(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.editor else { return Vec::new() };
    let Some(row) = editor.rows.get(editor.row).cloned() else { return Vec::new() };
    match row.key {
        FormKey::Folder => {
            let mut entries = vec![PickerEntry {
                label: "No folder".to_owned(),
                note: String::new(),
                filter: "no folder".to_owned(),
                value: String::new(),
                accelerator: None,
            }];
            for folder in state.cache.folders() {
                entries.push(PickerEntry {
                    label: folder.name.clone(),
                    note: String::new(),
                    filter: folder.name.clone(),
                    value: folder.name.clone(),
                    accelerator: None,
                });
            }
            let mut picker = PickerState::new("Folder", PickerKind::Folder, entries);
            picker.from_editor = true;
            state.picker = Some(picker);
            Vec::new()
        }
        FormKey::UriAdd => {
            if let Some(editor) = &mut state.editor
                && let Some(login) = editor.item.login.as_mut()
            {
                let uris = login.uris.get_or_insert_with(Vec::new);
                uris.push(LoginUri::new(""));
                editor.dirty = true;
            }
            rebuild_rows(state);
            Vec::new()
        }
        FormKey::Uri(index) => {
            // Open the URI editor in the browser-ish way: focus insert.
            if let Some(editor) = &mut state.editor {
                editor.row = editor
                    .rows
                    .iter()
                    .position(|r| r.key == FormKey::Uri(index))
                    .unwrap_or(editor.row);
                editor.insert = true;
                editor.cursor = editor.rows[editor.row].value.chars().count();
            }
            Vec::new()
        }
        FormKey::UriMatch(index) => {
            let mut picker = PickerState::new(
                "Match detection",
                PickerKind::UriMatch,
                [
                    "Default",
                    "Base domain",
                    "Host",
                    "Starts with",
                    "Exact",
                    "Regular expression",
                    "Never",
                ]
                .iter()
                .map(|v| PickerEntry {
                    label: (*v).to_owned(),
                    note: String::new(),
                    filter: (*v).to_owned(),
                    value: (*v).to_owned(),
                    accelerator: None,
                })
                .collect(),
            );
            picker.from_editor = true;
            let _ = index;
            state.picker = Some(picker);
            Vec::new()
        }
        FormKey::FieldAdd => {
            if let Some(editor) = &mut state.editor {
                let fields = editor.item.fields.get_or_insert_with(Vec::new);
                fields.push(Field::default());
                editor.dirty = true;
            }
            rebuild_rows(state);
            Vec::new()
        }
        FormKey::FieldType(_) => {
            let mut picker = PickerState::new(
                "Field type",
                PickerKind::FieldType,
                ["text", "hidden", "boolean", "linked"]
                    .iter()
                    .map(|v| PickerEntry {
                        label: (*v).to_owned(),
                        note: String::new(),
                        filter: (*v).to_owned(),
                        value: (*v).to_owned(),
                        accelerator: None,
                    })
                    .collect(),
            );
            picker.from_editor = true;
            state.picker = Some(picker);
            Vec::new()
        }
        FormKey::SshGenerate => {
            match crate::util::ssh::generate_ed25519("lazybw") {
                Ok(key) => {
                    if let Some(editor) = &mut state.editor {
                        editor.item.ssh_key = Some(SshKey {
                            private_key: Some(crate::bw::sensitive::Sensitive::new(
                                key.private_key,
                            )),
                            public_key: key.public_key,
                            key_fingerprint: key.fingerprint,
                            ..Default::default()
                        });
                        editor.dirty = true;
                    }
                    rebuild_rows(state);
                    state.message(MessageLevel::Ok, "Generated an Ed25519 key.");
                }
                Err(err) => state.toast_with(
                    MessageLevel::Error,
                    "Couldn't generate a key.",
                    err.to_string(),
                ),
            }
            Vec::new()
        }
        FormKey::SshImport => {
            state.prompt = Some(PromptState {
                title: "Path to an OpenSSH private key".to_owned(),
                input: crate::input::line::Line::new(""),
                on_accept: Action::new("editor.ssh_import"),
                replace_first_arg: true,
                error: None,
            });
            Vec::new()
        }
        FormKey::Notes => {
            let text = state
                .editor
                .as_ref()
                .and_then(|editor| editor.item.notes.as_ref())
                .map(|notes| notes.expose().to_owned())
                .unwrap_or_default();
            state.notes = Some(std::cell::RefCell::new(crate::app::state::NotesState {
                editor: edtui::EditorState::new(edtui::Lines::from(text.as_str())),
            }));
            Vec::new()
        }
        FormKey::Favorite | FormKey::Reprompt => {
            let effects = editor_cycle(state, true);
            let _ = effects;
            Vec::new()
        }
        FormKey::Name | FormKey::Username | FormKey::Password | FormKey::Totp => {
            if let Some(editor) = &mut state.editor {
                editor.insert = true;
                editor.cursor = editor.rows[editor.row].value.chars().count();
            }
            Vec::new()
        }
        _ => {
            if matches!(row.kind, FormKind::Bool) {
                editor_cycle(state, true);
            }
            Vec::new()
        }
    }
}

pub(crate) fn editor_add_row(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.editor else { return Vec::new() };
    let key = editor.rows.get(editor.row).map(|r| r.key);
    match key {
        Some(
            FormKey::FieldName(index) | FormKey::FieldValue(index) | FormKey::FieldType(index),
        ) => {
            if let Some(editor) = &mut state.editor {
                let fields = editor.item.fields.get_or_insert_with(Vec::new);
                fields.insert(index + 1, Field::default());
                editor.dirty = true;
            }
        }
        Some(FormKey::Uri(index) | FormKey::UriMatch(index)) => {
            if let Some(editor) = &mut state.editor
                && let Some(login) = editor.item.login.as_mut()
            {
                let uris = login.uris.get_or_insert_with(Vec::new);
                uris.insert(index + 1, LoginUri::new(""));
                editor.dirty = true;
            }
        }
        _ => {}
    }
    rebuild_rows(state);
    Vec::new()
}

pub(crate) fn editor_delete_row(state: &mut AppState) -> Vec<Effect> {
    let Some(editor) = &state.editor else { return Vec::new() };
    let key = editor.rows.get(editor.row).map(|r| r.key);
    match key {
        Some(
            FormKey::FieldName(index) | FormKey::FieldValue(index) | FormKey::FieldType(index),
        ) => {
            if let Some(editor) = &mut state.editor
                && let Some(fields) = editor.item.fields.as_mut()
                && index < fields.len()
            {
                fields.remove(index);
                editor.dirty = true;
            }
        }
        Some(FormKey::Uri(index) | FormKey::UriMatch(index)) => {
            if let Some(editor) = &mut state.editor
                && let Some(login) = editor.item.login.as_mut()
                && let Some(uris) = login.uris.as_mut()
                && index < uris.len()
            {
                uris.remove(index);
                editor.dirty = true;
            }
        }
        _ => {
            // Clear the field.
            if let Some(editor) = &mut state.editor
                && let Some(row) = editor.rows.get_mut(editor.row)
            {
                row.value.clear();
                editor.dirty = true;
            }
            sync_editor(state);
        }
    }
    rebuild_rows(state);
    Vec::new()
}

pub(crate) fn editor_save(state: &mut AppState, close: bool) -> Vec<Effect> {
    let Some(editor) = &state.editor else { return Vec::new() };
    if editor.rows.iter().any(|r| r.key == FormKey::Name && r.value.trim().is_empty()) {
        state.message(MessageLevel::Error, "The name cannot be empty.");
        return Vec::new();
    }
    sync_editor(state);
    let errors = validate_editor(state);
    if !errors.is_empty() {
        if let Some(editor) = &mut state.editor {
            for error in &errors {
                if let Some(index) = error.0 {
                    editor.errors.insert(index, error.1.clone());
                }
            }
        }
        let message = errors.first().map(|e| e.1.clone()).unwrap_or_default();
        state.message(MessageLevel::Error, message);
        return Vec::new();
    }
    let Some(editor) = &state.editor else { return Vec::new() };
    let item = editor.item.clone();
    let id = item.id.clone();
    let revision = editor.history.revision;
    let request_effect = request(
        state,
        if id.is_some() {
            CommandKind::PutItem { id: id.clone().unwrap_or_default(), item: Box::new(item) }
        } else {
            CommandKind::CreateItem(Box::new(item))
        },
        Pending::Save { id, close, editor: Some(revision) },
    );
    vec![request_effect]
}

pub(crate) fn validate_editor(state: &mut AppState) -> Vec<(Option<usize>, String)> {
    let Some(editor) = &state.editor else { return Vec::new() };
    let mut errors = Vec::new();
    for (index, row) in editor.rows.iter().enumerate() {
        match row.key {
            FormKey::Totp if !row.value.trim().is_empty() && !crate::totp::is_valid(&row.value) => {
                errors.push((Some(index), "Not a valid authenticator key".to_owned()));
            }
            FormKey::CardNumber if !row.value.trim().is_empty() => {
                if !crate::util::luhn_valid(&row.value) {
                    // Warning only: store it in errors but not blocking.
                    // Luhn failures are common for test cards; we warn.
                }
            }
            FormKey::ExpMonth
                if !row.value.trim().is_empty()
                    && !(1..=12).contains(&row.value.parse::<u8>().unwrap_or(0)) =>
            {
                errors.push((Some(index), "Expiration month must be 1–12".to_owned()));
            }
            _ => {}
        }
    }
    errors
}

pub(crate) fn sync_editor(state: &mut AppState) {
    let Some(editor) = &mut state.editor else { return };
    // Name and simple fields.
    for row in &editor.rows {
        let value = row.value.clone();
        match row.key {
            FormKey::Name => editor.item.name = value,
            FormKey::Notes => {
                editor.item.notes = if value.is_empty() {
                    None
                } else {
                    Some(crate::bw::sensitive::Sensitive::new(value))
                };
            }
            FormKey::Folder => {
                let path =
                    if value == "No folder" || value.is_empty() { String::new() } else { value };
                editor.item.folder_id = if path.is_empty() {
                    None
                } else {
                    state.cache.folders().iter().find(|f| f.name == path).and_then(|f| f.id.clone())
                };
            }
            FormKey::Favorite => editor.item.favorite = value == "true",
            FormKey::Reprompt => editor.item.reprompt = u8::from(value == "true"),
            _ => {}
        }
    }
    // Type-specific.
    match editor.item.kind {
        ItemType::Login => {
            if editor.item.login.is_none() {
                editor.item.login = Some(Login::default());
            }
            let mut username = None;
            let mut password = None;
            let mut totp = None;
            for row in &editor.rows {
                match row.key {
                    FormKey::Username => username = Some(row.value.clone()),
                    FormKey::Password => password = Some(row.value.clone()),
                    FormKey::Totp => totp = Some(row.value.clone()),
                    _ => {}
                }
            }
            if let Some(login) = editor.item.login.as_mut() {
                login.username =
                    username.and_then(non_empty).map(crate::bw::sensitive::Sensitive::new);
                login.password =
                    password.and_then(non_empty).map(crate::bw::sensitive::Sensitive::new);
                login.totp = totp.and_then(non_empty).map(crate::bw::sensitive::Sensitive::new);
                let uris = login.uris.get_or_insert_with(Vec::new);
                for row in &editor.rows {
                    match row.key {
                        FormKey::Uri(index) => {
                            while uris.len() <= index {
                                uris.push(LoginUri::new(""));
                            }
                            if row.value.is_empty() {
                                uris[index].uri = None;
                            } else {
                                uris[index].uri = Some(row.value.clone());
                            }
                        }
                        FormKey::UriMatch(index) => {
                            while uris.len() <= index {
                                uris.push(LoginUri::new(""));
                            }
                            let kind = match row.value.as_str() {
                                "Base domain" => UriMatch::BaseDomain,
                                "Host" => UriMatch::Host,
                                "Starts with" => UriMatch::StartsWith,
                                "Exact" => UriMatch::Exact,
                                "Regular expression" => UriMatch::Regex,
                                "Never" => UriMatch::Never,
                                _ => UriMatch::Default,
                            };
                            uris[index].set_match_kind(kind);
                        }
                        _ => {}
                    }
                }
                uris.retain(|uri| uri.uri.is_some());
            }
        }
        ItemType::Card => {
            let card = editor.item.card.get_or_insert_with(Card::default);
            for row in &editor.rows {
                let v = row.value.clone();
                match row.key {
                    FormKey::Cardholder => card.cardholder_name = non_empty(v),
                    FormKey::CardNumber => {
                        card.number =
                            non_empty(v.clone()).map(crate::bw::sensitive::Sensitive::new);
                        if card.brand.is_none() || card.brand.as_deref() == Some("") {
                            card.brand = crate::util::card_brand(&v).map(str::to_owned);
                        }
                    }
                    FormKey::CardBrand => card.brand = non_empty(v),
                    FormKey::ExpMonth => card.exp_month = non_empty(v),
                    FormKey::ExpYear => card.exp_year = non_empty(v),
                    FormKey::CardCode => {
                        card.code = non_empty(v).map(crate::bw::sensitive::Sensitive::new)
                    }
                    _ => {}
                }
            }
        }
        ItemType::Identity => {
            let identity = editor.item.identity.get_or_insert_with(Identity::default);
            for row in &editor.rows {
                let v = row.value.clone();
                match row.key {
                    FormKey::IdTitle => identity.title = non_empty(v),
                    FormKey::IdFirst => identity.first_name = non_empty(v),
                    FormKey::IdMiddle => identity.middle_name = non_empty(v),
                    FormKey::IdLast => identity.last_name = non_empty(v),
                    FormKey::IdUsername => identity.username = non_empty(v),
                    FormKey::IdCompany => identity.company = non_empty(v),
                    FormKey::IdEmail => identity.email = non_empty(v),
                    FormKey::IdPhone => identity.phone = non_empty(v),
                    FormKey::IdAddress1 => identity.address1 = non_empty(v),
                    FormKey::IdAddress2 => identity.address2 = non_empty(v),
                    FormKey::IdAddress3 => identity.address3 = non_empty(v),
                    FormKey::IdCity => identity.city = non_empty(v),
                    FormKey::IdState => identity.state = non_empty(v),
                    FormKey::IdPostal => identity.postal_code = non_empty(v),
                    FormKey::IdCountry => identity.country = non_empty(v),
                    FormKey::IdSsn => {
                        identity.ssn = non_empty(v).map(crate::bw::sensitive::Sensitive::new)
                    }
                    FormKey::IdPassport => {
                        identity.passport_number =
                            non_empty(v).map(crate::bw::sensitive::Sensitive::new)
                    }
                    FormKey::IdLicense => {
                        identity.license_number =
                            non_empty(v).map(crate::bw::sensitive::Sensitive::new)
                    }
                    _ => {}
                }
            }
        }
        ItemType::SshKey => {
            let key = editor.item.ssh_key.get_or_insert_with(SshKey::default);
            for row in &editor.rows {
                match row.key {
                    FormKey::SshPublic => key.public_key = row.value.clone(),
                    FormKey::SshFingerprint => key.key_fingerprint = row.value.clone(),
                    FormKey::SshPrivate => {
                        key.private_key =
                            non_empty(row.value.clone()).map(crate::bw::sensitive::Sensitive::new)
                    }
                    _ => {}
                }
            }
        }
        ItemType::SecureNote => {
            if editor.item.secure_note.is_none() {
                editor.item.secure_note = Some(crate::bw::models::SecureNote::default());
            }
        }
        ItemType::Other(_) => {}
    }
    // Custom fields.
    let field_rows: Vec<(usize, FormKey, String)> = editor
        .rows
        .iter()
        .filter_map(|row| match row.key {
            FormKey::FieldName(i) | FormKey::FieldValue(i) | FormKey::FieldType(i) => {
                Some((i, row.key, row.value.clone()))
            }
            _ => None,
        })
        .collect();
    if !field_rows.is_empty() {
        let mut fields = editor.item.fields.take().unwrap_or_default();
        for (index, key, value) in field_rows {
            while fields.len() <= index {
                fields.push(Field::default());
            }
            let field = &mut fields[index];
            match key {
                FormKey::FieldName(_) => field.name = non_empty(value),
                FormKey::FieldType(_) => {
                    field.kind = FieldKind::from_code(match value.as_str() {
                        "hidden" => 1,
                        "boolean" => 2,
                        "linked" => 3,
                        _ => 0,
                    })
                    .code();
                }
                FormKey::FieldValue(_) => {
                    field.value = if value.is_empty() {
                        None
                    } else {
                        Some(crate::bw::sensitive::Sensitive::new(value))
                    };
                }
                _ => {}
            }
        }
        editor.item.fields = Some(fields);
    }
}

pub(crate) fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() { None } else { Some(value) }
}

pub(crate) fn close_notes(state: &mut AppState) {
    let Some(notes) = state.notes.take() else { return };
    let notes = notes.into_inner();
    let text: String = notes
        .editor
        .lines
        .iter_row()
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(editor) = &mut state.editor {
        if let Some(row) = editor.rows.iter_mut().find(|row| row.key == FormKey::Notes) {
            row.value = text.clone();
        }
        editor.item.notes =
            if text.is_empty() { None } else { Some(crate::bw::sensitive::Sensitive::new(text)) };
        editor.dirty = true;
    }
}

pub(crate) fn notes_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let Some(notes) = &state.notes else { return Vec::new() };
    let mut notes = notes.borrow_mut();
    let mode = notes.editor.mode;
    if key.mods.contains(crate::config::keymap::Mods::CTRL) {
        match key.code {
            Code::Char('e') => {
                drop(notes);
                return editor_action(state, Action::new("editor.external"));
            }
            Code::Char('s') => {
                drop(notes);
                return editor_action(state, Action::new("editor.save"));
            }
            _ => {}
        }
    }
    if mode == edtui::EditorMode::Normal {
        match key.code {
            Code::Tab => {
                drop(notes);
                close_notes(state);
                return editor_action(state, Action::new("editor.next_field"));
            }
            Code::BackTab => {
                drop(notes);
                close_notes(state);
                return editor_action(state, Action::new("editor.prev_field"));
            }
            Code::Char(':') => {
                drop(notes);
                close_notes(state);
                return apply(state, Action::new("cmdline.open"));
            }
            _ => {}
        }
    }
    let event = crate::input::keys::to_event(key);
    let mut handler = edtui::EditorEventHandler::default();
    handler.on_key_event(event, &mut notes.editor);
    Vec::new()
}
