//! The item editor form and the notes editor.

use super::{
    Action, AppState, BTreeMap, CommandKind, ConfirmState, DetailState, EditorState, EditorTarget,
    Effect, Field, FieldKind, FormKey, FormKind, FormRow, Item, ItemType, Key, LoginUri,
    MessageLevel, Pending, PickerEntry, PickerKind, PickerState, PromptAction, PromptState, Screen,
    SshKey, SuspendTask, apply, current_folder_id, generate, request,
};
use crate::app::actions::ActionId;

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

/// The editor row of a detail-pane field (`"password"`, `"uri:1"`, …).
pub(crate) fn form_key_for_detail(key: &str) -> Option<FormKey> {
    let indexed = |prefix: &str| key.strip_prefix(prefix).and_then(|i| i.parse::<usize>().ok());
    if let Some(index) = indexed("uri:") {
        return Some(FormKey::Uri(index));
    }
    if let Some(index) = indexed("field:") {
        return Some(FormKey::FieldValue(index));
    }
    if key.starts_with("note:") {
        return Some(FormKey::Notes);
    }
    Some(match key {
        "username" => FormKey::Username,
        "password" => FormKey::Password,
        "totp" => FormKey::Totp,
        "notes" => FormKey::Notes,
        "cardholder" => FormKey::Cardholder,
        "number" => FormKey::CardNumber,
        "expiry" => FormKey::ExpMonth,
        "code" => FormKey::CardCode,
        "id:name" => FormKey::IdFirst,
        "id:contact" => FormKey::IdEmail,
        "id:address" => FormKey::IdAddress1,
        "id:ssn" => FormKey::IdSsn,
        "id:passport" => FormKey::IdPassport,
        "id:license" => FormKey::IdLicense,
        "public_key" => FormKey::SshPublic,
        "fingerprint" => FormKey::SshFingerprint,
        "private_key" => FormKey::SshPrivate,
        _ => return None,
    })
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
        state.open(
            ConfirmState::new(
                "Unsaved changes",
                format!("Save changes to {name}?"),
                "Save",
                Action::new(ActionId::EditorSaveClose),
            )
            .deny(Action::new(ActionId::EditorDiscard)),
        );
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
        kind: FormKind::Text,
        section: Some("CREDENTIALS"),
        hint: None,
    });
    let mut folders: Vec<String> = vec!["No folder".to_owned()];
    for folder in state.cache.folders() {
        if !folders.contains(&folder.name) {
            folders.push(folder.name.clone());
        }
    }
    rows.push(FormRow {
        key: FormKey::Folder,
        label: "Folder".to_owned(),
        kind: FormKind::Option(folders),
        section: None,
        hint: Some("Enter pick".to_owned()),
    });
    match item.kind {
        ItemType::Login => {
            rows.push(FormRow {
                key: FormKey::Username,
                label: "Username".to_owned(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::Password,
                label: "Password".to_owned(),
                kind: FormKind::Secret,
                section: None,
                hint: Some("Ctrl-g generate".to_owned()),
            });
            rows.push(FormRow {
                key: FormKey::Totp,
                label: "Authenticator key".to_owned(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
            let uris = item.login.as_ref().and_then(|l| l.uris.as_ref()).map_or(0, Vec::len);
            for index in 0..uris {
                rows.push(FormRow {
                    key: FormKey::Uri(index),
                    label: format!("Website {}", index + 1),
                    kind: FormKind::Text,
                    section: if index == 0 { Some("WEBSITES") } else { None },
                    hint: None,
                });
                rows.push(FormRow {
                    key: FormKey::UriMatch(index),
                    label: "  match".to_owned(),
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
                kind: FormKind::Action,
                section: if uris == 0 { Some("WEBSITES") } else { None },
                hint: None,
            });
        }
        ItemType::Card => {
            rows.push(FormRow {
                key: FormKey::Cardholder,
                label: "Cardholder name".to_owned(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::CardNumber,
                label: "Number".to_owned(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::CardBrand,
                label: "Brand".to_owned(),
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
                kind: FormKind::Option((1..=12).map(|m| format!("{m:02}")).collect()),
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::ExpYear,
                label: "Expiration year".to_owned(),
                kind: FormKind::Text,
                section: None,
                hint: Some("Ctrl-a/Ctrl-x".to_owned()),
            });
            rows.push(FormRow {
                key: FormKey::CardCode,
                label: "Security code".to_owned(),
                kind: FormKind::Secret,
                section: None,
                hint: None,
            });
        }
        ItemType::Identity => {
            let rows_data: Vec<(FormKey, &str, FormKind)> = vec![
                (
                    FormKey::IdTitle,
                    "Title",
                    FormKind::Option(vec![
                        "Mr".into(),
                        "Mrs".into(),
                        "Ms".into(),
                        "Mx".into(),
                        "Dr".into(),
                    ]),
                ),
                (FormKey::IdFirst, "First name", FormKind::Text),
                (FormKey::IdMiddle, "Middle name", FormKind::Text),
                (FormKey::IdLast, "Last name", FormKind::Text),
                (FormKey::IdUsername, "Username", FormKind::Text),
                (FormKey::IdCompany, "Company", FormKind::Text),
                (FormKey::IdEmail, "Email", FormKind::Text),
                (FormKey::IdPhone, "Phone", FormKind::Text),
                (FormKey::IdAddress1, "Address 1", FormKind::Text),
                (FormKey::IdAddress2, "Address 2", FormKind::Text),
                (FormKey::IdAddress3, "Address 3", FormKind::Text),
                (FormKey::IdCity, "City", FormKind::Text),
                (FormKey::IdState, "State", FormKind::Text),
                (FormKey::IdPostal, "Postal code", FormKind::Text),
                (FormKey::IdCountry, "Country", FormKind::Text),
                (FormKey::IdSsn, "SSN", FormKind::Secret),
                (FormKey::IdPassport, "Passport number", FormKind::Secret),
                (FormKey::IdLicense, "Licence number", FormKind::Secret),
            ];
            for (index, (key, label, kind)) in rows_data.into_iter().enumerate() {
                rows.push(FormRow {
                    key,
                    label: label.to_owned(),
                    kind,
                    section: if index == 0 { Some("IDENTITY") } else { None },
                    hint: None,
                });
            }
        }
        ItemType::SecureNote => {}
        ItemType::SshKey => {
            rows.push(FormRow {
                key: FormKey::SshGenerate,
                label: "Generate Ed25519 key".to_owned(),
                kind: FormKind::Action,
                section: Some("SSH KEY"),
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshImport,
                label: "Import from file…".to_owned(),
                kind: FormKind::Action,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshPublic,
                label: "Public key".to_owned(),
                kind: FormKind::Text,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshFingerprint,
                label: "Fingerprint".to_owned(),
                kind: FormKind::ReadOnly,
                section: None,
                hint: None,
            });
            rows.push(FormRow {
                key: FormKey::SshPrivate,
                label: "Private key".to_owned(),
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
        kind: FormKind::Bool,
        section: Some("OPTIONS"),
        hint: None,
    });
    rows.push(FormRow {
        key: FormKey::Reprompt,
        label: "Password re-prompt".to_owned(),
        kind: FormKind::Bool,
        section: None,
        hint: None,
    });
    // Custom fields.
    let fields = item.fields.as_deref().unwrap_or_default();
    for (index, field) in fields.iter().enumerate() {
        rows.push(FormRow {
            key: FormKey::FieldName(index),
            label: "Field name".to_owned(),
            kind: FormKind::Text,
            section: if index == 0 { Some("CUSTOM FIELDS") } else { None },
            hint: None,
        });
        rows.push(FormRow {
            key: FormKey::FieldType(index),
            label: "  type".to_owned(),
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
        kind: FormKind::Action,
        section: if fields.is_empty() { Some("CUSTOM FIELDS") } else { None },
        hint: None,
    });
    rows.push(FormRow {
        key: FormKey::Notes,
        label: "Notes".to_owned(),
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
    match action.id {
        ActionId::EditorSave => return editor_save(state, false),
        ActionId::EditorSaveClose => return editor_save(state, true),
        ActionId::EditorDiscard => {
            state.editor = None;
            state.notes = None;
            state.screen = Screen::Vault;
            return Vec::new();
        }
        ActionId::EditorColumnLeft | ActionId::EditorColumnRight => {
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
            let to_right = action.id == ActionId::EditorColumnRight;
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
        ActionId::EditorClose => {
            close_notes(state);
            close_editor(state);
            return Vec::new();
        }
        ActionId::EditorReload => {
            if let Some(id) = state.editor.as_ref().and_then(|e| e.item.id.clone()) {
                return vec![request(state, CommandKind::GetItem(id), Pending::GetItem)];
            }
            return Vec::new();
        }
        ActionId::EditorNormal => {
            if let Some(editor) = &mut state.editor {
                editor.insert = false;
                editor.cursor = editor.cursor.saturating_sub(1);
            }
            return Vec::new();
        }
        ActionId::EditorInsert
        | ActionId::EditorAppend
        | ActionId::EditorInsertStart
        | ActionId::EditorAppendEnd => {
            let len = current_len(state);
            let Some(editor) = &mut state.editor else { return Vec::new() };
            editor.insert = true;
            editor.cursor = match action.id {
                ActionId::EditorInsert => editor.cursor.min(len),
                ActionId::EditorAppend => (editor.cursor + 1).min(len),
                ActionId::EditorInsertStart => 0,
                _ => len,
            };
            return Vec::new();
        }
        ActionId::EditorChangeField => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            if let Some(row) = editor.rows.get(editor.row)
                && takes_text(&row.kind)
            {
                let key = row.key;
                crate::app::form::set(&mut editor.item, key, "", &state.cache);
                editor.cursor = 0;
                editor.insert = true;
            }
            return Vec::new();
        }
        ActionId::EditorNextField
        | ActionId::EditorPrevField
        | ActionId::EditorFirstField
        | ActionId::EditorLastField => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            if !editor.rows.is_empty() {
                // Tab and Shift-Tab keep INSERT, so a form can be filled in
                // one go; gg and G are NORMAL-mode moves.
                editor.insert &=
                    matches!(action.id, ActionId::EditorNextField | ActionId::EditorPrevField);
                editor.row = match action.id {
                    ActionId::EditorNextField => (editor.row + 1) % editor.rows.len(),
                    ActionId::EditorPrevField => {
                        (editor.row + editor.rows.len() - 1) % editor.rows.len()
                    }
                    ActionId::EditorFirstField => 0,
                    _ => editor.rows.len() - 1,
                };
            }
            state_cursor_to_end(state);
            return Vec::new();
        }
        ActionId::EditorRowAddBelow | ActionId::EditorRowAddAbove => {
            return editor_add_row(state);
        }
        ActionId::EditorRowDelete => {
            return editor_delete_row(state);
        }
        ActionId::EditorCycleNext | ActionId::EditorCyclePrev => {
            return editor_cycle(state, action.id == ActionId::EditorCycleNext);
        }
        ActionId::EditorActivate => {
            return editor_activate(state);
        }
        ActionId::EditorGenerate => {
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
            return vec![generate(state, true, false)];
        }
        ActionId::EditorExternal => {
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
        ActionId::EditorUndo | ActionId::EditorRedo => {
            let Some(editor) = &mut state.editor else { return Vec::new() };
            let undo = action.id == ActionId::EditorUndo;
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
    use crate::config::keymap::{Code, Mods};
    // Keys bound in the `insert` context come first (Esc, Tab, Ctrl-s and
    // the rest by default, and whatever the user rebinds there); printable
    // keys are always text.
    if !key.is_printable()
        && let Some(call) = state.keymap.action("insert", &[key])
    {
        return Action::from_call(call).map(|action| apply(state, action)).unwrap_or_default();
    }
    match key.code {
        Code::Enter if key.mods.contains(Mods::CTRL) => {
            editor_action(state, Action::new(ActionId::EditorActivate))
        }
        Code::Char(c) if key.is_printable() => {
            editor_value_insert(state, &c.to_string());
            Vec::new()
        }
        _ => {
            editor_value_edit(state, key);
            Vec::new()
        }
    }
}

/// The length, in characters, of the value under the editor's cursor.
fn current_len(state: &AppState) -> usize {
    let Some(editor) = &state.editor else { return 0 };
    editor.rows.get(editor.row).map_or(0, |row| {
        crate::app::form::value(&editor.item, row.key, &state.cache).chars().count()
    })
}

/// Puts the editor's cursor at the end of the value under it.
pub(crate) fn state_cursor_to_end(state: &mut AppState) {
    let len = current_len(state);
    if let Some(editor) = &mut state.editor {
        editor.cursor = len;
    }
}

/// Moves the cursor or deletes in the value under the cursor (arrows,
/// `Home`, `End`, `Backspace`, `Delete`, `Ctrl-w`, `Ctrl-u`). The value is
/// edited in the item, in place.
fn editor_value_edit(state: &mut AppState, key: Key) {
    use crate::config::keymap::Code;
    let Some(editor) = &mut state.editor else { return };
    let Some(row) = editor.rows.get(editor.row) else { return };
    let row_key = row.key;
    let moves = matches!(key.code, Code::Left | Code::Right | Code::Home | Code::End);
    if !takes_text(&row.kind) {
        return;
    }
    let Some(value) = crate::app::form::text_mut(&mut editor.item, row_key) else { return };
    let before = value.len();
    let mut cursor = editor.cursor;
    if !crate::input::line::edit_string(value, &mut cursor, key) {
        crate::app::form::settle(&mut editor.item, row_key);
        return;
    }
    let changed = !moves && value.len() != before;
    editor.cursor = cursor;
    crate::app::form::settle(&mut editor.item, row_key);
    if changed {
        editor.dirty = true;
    }
}

/// Whether typing in INSERT changes this row. Choices, checkboxes and
/// buttons keep INSERT while the cursor passes over them but take no text.
fn takes_text(kind: &FormKind) -> bool {
    matches!(kind, FormKind::Text | FormKind::Secret | FormKind::Notes)
}

pub(crate) fn editor_value_insert(state: &mut AppState, text: &str) {
    let Some(editor) = &mut state.editor else { return };
    let Some(row) = editor.rows.get(editor.row).filter(|row| takes_text(&row.kind)) else {
        return;
    };
    let key = row.key;
    let Some(value) = crate::app::form::text_mut(&mut editor.item, key) else { return };
    let at = editor.cursor.min(value.chars().count());
    crate::app::form::insert(value, at, text);
    editor.cursor = at + text.chars().count();
    crate::app::form::settle(&mut editor.item, key);
    editor.dirty = true;
}

pub(crate) fn editor_cycle(state: &mut AppState, forward: bool) -> Vec<Effect> {
    let Some(editor) = &mut state.editor else { return Vec::new() };
    let Some(row) = editor.rows.get(editor.row) else { return Vec::new() };
    let current = crate::app::form::value(&editor.item, row.key, &state.cache);
    let next = match &row.kind {
        FormKind::Option(options) if !options.is_empty() => {
            let index = options.iter().position(|o| o == current).unwrap_or(0);
            let next = if forward {
                (index + 1) % options.len()
            } else {
                (index + options.len() - 1) % options.len()
            };
            options[next].clone()
        }
        FormKind::Bool => if current == "true" { "false" } else { "true" }.to_owned(),
        _ => return Vec::new(),
    };
    let key = row.key;
    if crate::app::form::set(&mut editor.item, key, &next, &state.cache) {
        editor.dirty = true;
    }
    // A field that became a checkbox (or stopped being one) is drawn and
    // edited differently.
    if matches!(key, FormKey::FieldType(_)) {
        rebuild_rows(state);
    }
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
            state.open(picker);
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
            }
            state_cursor_to_end(state);
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
            state.open(picker);
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
            state.open(picker);
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
            state.open(PromptState::new(
                "Path to an OpenSSH private key",
                "",
                PromptAction::ImportSshKey,
            ));
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
            }
            state_cursor_to_end(state);
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
                && let Some(row) = editor.rows.get(editor.row)
            {
                let key = row.key;
                if crate::app::form::set(&mut editor.item, key, "", &state.cache) {
                    editor.dirty = true;
                }
            }
        }
    }
    rebuild_rows(state);
    Vec::new()
}

pub(crate) fn editor_save(state: &mut AppState, close: bool) -> Vec<Effect> {
    let Some(editor) = &state.editor else { return Vec::new() };
    // A new item has no id until the first save is answered: a second
    // `Ctrl-s` meanwhile would create it twice.
    let creating = state.awaiting(|p| matches!(p, Pending::Save { id: None, editor: Some(_), .. }));
    if editor.is_new && creating {
        state.message(MessageLevel::Info, "Still creating the item…");
        return Vec::new();
    }
    if editor.item.name.trim().is_empty() {
        state.message(MessageLevel::Error, "The name cannot be empty.");
        return Vec::new();
    }
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
    // A card number that fails the Luhn check is most likely mistyped; it is
    // still saved (test cards and some store cards fail it on purpose).
    let number = crate::app::form::value(&editor.item, FormKey::CardNumber, &state.cache);
    let mistyped_card = editor.item.kind == ItemType::Card
        && !number.trim().is_empty()
        && !crate::util::luhn_valid(number);
    let item = crate::app::form::for_saving(editor.item.clone());
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
    if mistyped_card {
        state.message(
            MessageLevel::Warn,
            "The card number looks mistyped (Luhn check); saved anyway.",
        );
    }
    vec![request_effect]
}

pub(crate) fn validate_editor(state: &mut AppState) -> Vec<(Option<usize>, String)> {
    let Some(editor) = &state.editor else { return Vec::new() };
    let mut errors = Vec::new();
    for (index, row) in editor.rows.iter().enumerate() {
        let value = crate::app::form::value(&editor.item, row.key, &state.cache);
        match row.key {
            FormKey::Totp if !value.trim().is_empty() && !crate::totp::is_valid(value) => {
                errors.push((Some(index), "Not a valid authenticator key".to_owned()));
            }
            FormKey::ExpMonth
                if !value.trim().is_empty()
                    && !(1..=12).contains(&value.parse::<u8>().unwrap_or(0)) =>
            {
                errors.push((Some(index), "Expiration month must be 1–12".to_owned()));
            }
            _ => {}
        }
    }
    errors
}

/// Copies the text of the notes editor into the form, which keeps it open.
/// The item only becomes dirty when the text changed.
pub(crate) fn sync_notes(state: &mut AppState) {
    let Some(notes) = &state.notes else { return };
    let text: String = notes
        .borrow()
        .editor
        .lines
        .iter_row()
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    let Some(editor) = &mut state.editor else { return };
    let current = editor.item.notes.as_ref().map(|n| n.expose()).unwrap_or("");
    if current == text {
        crate::app::form::wipe(text);
        return;
    }
    editor.item.notes =
        if text.is_empty() { None } else { Some(crate::bw::sensitive::Sensitive::new(text)) };
    editor.dirty = true;
}

pub(crate) fn close_notes(state: &mut AppState) {
    sync_notes(state);
    state.notes = None;
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
                return editor_action(state, Action::new(ActionId::EditorExternal));
            }
            Code::Char('s') => {
                // Save what is typed in the notes too; the notes stay open.
                drop(notes);
                sync_notes(state);
                return editor_action(state, Action::new(ActionId::EditorSave));
            }
            _ => {}
        }
    }
    if mode == edtui::EditorMode::Normal {
        match key.code {
            Code::Tab => {
                drop(notes);
                close_notes(state);
                return editor_action(state, Action::new(ActionId::EditorNextField));
            }
            Code::BackTab => {
                drop(notes);
                close_notes(state);
                return editor_action(state, Action::new(ActionId::EditorPrevField));
            }
            Code::Char(':') => {
                drop(notes);
                close_notes(state);
                return apply(state, Action::new(ActionId::CmdlineOpen));
            }
            _ => {}
        }
    }
    let event = crate::input::keys::to_event(key);
    let mut handler = edtui::EditorEventHandler::default();
    handler.on_key_event(event, &mut notes.editor);
    Vec::new()
}
