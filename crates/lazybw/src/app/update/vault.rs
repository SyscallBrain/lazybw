//! Vault browsing: selection, detail fields, copy and reveal, re-prompt,
//! trash, favorites, moves, search, navigation, undo.

use super::{
    Action, AppState, BTreeMap, CommandKind, ConfirmState, Deferred, DetailState, Effect,
    FieldKind, Filter, Instant, Item, ItemType, Key, MessageLevel, Pending, PickerEntry,
    PickerKind, PickerState, Query, RepromptState, Screen, copy_value, keymap_rows, request,
    settings_rows, toast,
};
use crate::app::actions::ActionId;

pub(crate) fn sidebar_entries(state: &AppState) -> Vec<crate::vault::filter::SidebarEntry> {
    crate::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    )
}

/// Shown when a folder would be named like the "No folder" filter.
pub(crate) const RESERVED_FOLDER_MESSAGE: &str =
    "“No folder” is the filter for items without a folder; pick another name.";

/// Would this folder name be confused with the "No folder" filter (and the
/// `bw` pseudo folder "No Folder")?
pub(crate) fn reserved_folder_name(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("no folder")
}

/// Is the sidebar drawn as the medium-width icon rail?
pub(crate) fn rail_active(state: &AppState) -> bool {
    crate::ui::layout::LayoutPlan::compute_sized(
        ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1),
        state.sidebar_visible,
        state.sidebar_width,
        state.list_width,
    )
    .rail
    .width
        > 0
}

/// Moves the sidebar cursor over the rail's icons (folders are one icon).
fn move_rail(state: &mut AppState, action: ActionId, count: Option<usize>) {
    let entries = sidebar_entries(state);
    let slots = crate::vault::filter::rail_slots(&entries);
    let selectable: Vec<usize> = slots.iter().filter_map(|slot| slot.entry()).collect();
    if selectable.is_empty() {
        return;
    }
    let current = crate::vault::filter::rail_position(&entries, &slots, state.sidebar_row)
        .and_then(|position| slots[position].entry())
        .and_then(|entry| selectable.iter().position(|&index| index == entry))
        .unwrap_or(0);
    let all = View { top: 0, page: selectable.len() };
    let next = move_index(current, selectable.len(), action, count, all);
    state.sidebar_row = selectable[next];
}

pub(crate) fn fold_action(state: &mut AppState, id: ActionId) -> Vec<Effect> {
    let entries = sidebar_entries(state);
    let Some(entry) = entries.get(state.sidebar_row) else { return Vec::new() };
    let crate::vault::filter::SidebarRole::Folder { path, has_children, .. } = &entry.role else {
        return Vec::new();
    };
    let path = path.clone();
    let has_children = *has_children;
    match id {
        ActionId::FoldToggle if has_children => {
            if !state.collapsed.remove(&path) {
                state.collapsed.insert(path);
            }
        }
        ActionId::FoldOpen => {
            state.collapsed.remove(&path);
        }
        ActionId::FoldClose if has_children => {
            state.collapsed.insert(path);
        }
        ActionId::FoldOpenAll => state.collapsed.clear(),
        ActionId::FoldCloseAll => {
            let tree = crate::vault::filter::folder_tree(&state.cache);
            state.collapsed = crate::vault::folders::all_paths(&tree);
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn current_folder_path(state: &AppState) -> String {
    match &state.filter {
        Filter::Folder(path) => path.clone(),
        _ => {
            if let Some(entry) = sidebar_entries(state).get(state.sidebar_row)
                && let crate::vault::filter::SidebarRole::Folder { path, .. } = &entry.role
            {
                return path.clone();
            }
            String::new()
        }
    }
}

pub(crate) fn current_folder_id(state: &AppState) -> Option<String> {
    let path = current_folder_path(state);
    if path.is_empty() {
        return None;
    }
    state.cache.folders().iter().find(|f| f.name == path).and_then(|f| f.id.clone())
}

pub(crate) fn select_item_by_id(state: &mut AppState, id: &str) {
    if state.cache.item(id).is_some() {
        let filter = if state.cache.item(id).is_some_and(|i| i.is_deleted()) {
            Filter::Trash
        } else if let Some(folder) = state.cache.item(id).and_then(|i| i.folder_id.clone()) {
            state
                .cache
                .folder_path(&folder)
                .map(|path| Filter::Folder(path.to_owned()))
                .unwrap_or(Filter::All)
        } else {
            Filter::All
        };
        state.filter = filter;
        // A search that hides the item would leave the cursor on another one.
        if !state.visible_rows().iter().any(|row| row.id == id) {
            state.search = crate::app::state::SearchState::default();
        }
        let rows = state.visible_rows();
        if let Some(index) = rows.iter().position(|row| row.id == id) {
            state.list_row = index;
        }
        state.screen = Screen::Vault;
        state.focus = crate::app::state::Focus::Detail;
        state.detail = DetailState::default();
    }
}

pub(crate) fn selected_name(state: &AppState) -> String {
    state.selected_item().map(|i| i.name.clone()).unwrap_or_default()
}

/// True when `item` asks for the master password and no recent re-prompt
/// covers it. A successful re-prompt is consumed by the action it unlocked;
/// `security.reprompt_grace` keeps it valid for a while longer.
pub(crate) fn reprompt_required(state: &mut AppState, item: &Item) -> bool {
    if !item.wants_reprompt() {
        return false;
    }
    let id = item.id.clone().unwrap_or_default();
    if state.reprompt_once.as_deref() == Some(id.as_str()) {
        state.reprompt_once = None;
        return false;
    }
    !state
        .reprompt_passed
        .as_ref()
        .is_some_and(|(passed, until)| *passed == id && Instant::now() < *until)
}

/// Like [`reprompt_required`] for the selected item.
pub(crate) fn selected_reprompt_required(state: &mut AppState) -> bool {
    match state.selected_item().cloned() {
        Some(item) => reprompt_required(state, &item),
        None => false,
    }
}

/// Items that lazybw may change: not in the trash, not archived and of a type
/// it understands (unknown types are read-only).
pub(crate) fn editable(state: &mut AppState, item: &Item) -> bool {
    let reason = if item.is_deleted() {
        Some("Restore the item from the trash before editing it.")
    } else if item.is_archived() {
        Some("Archived items are read-only.")
    } else if matches!(item.kind, ItemType::Other(_)) {
        Some("This item type is shown read-only.")
    } else {
        None
    };
    if let Some(reason) = reason {
        state.message(MessageLevel::Warn, reason);
        return false;
    }
    true
}

/// Asks to replace the password of `item` with a freshly generated one.
pub(crate) fn confirm_generate(state: &mut AppState, item: &Item) -> Vec<Effect> {
    let id = item.id.clone().unwrap_or_default();
    state.open(ConfirmState::new(
"New password",
format!(
            "Replace the password saved in “{}” with a generated one? This changes the vault item only: change it on the website too. The current password moves to its history; u undoes it.",
            item.name
        ),
"Replace",
Deferred::GeneratePassword(id),
));
    Vec::new()
}

/// The text a detail-pane field copies.
pub(crate) fn detail_value(state: &AppState, item: &Item, key: &str) -> String {
    if let Some(index) = key.strip_prefix("uri:").and_then(|i| i.parse::<usize>().ok()) {
        return item
            .login
            .as_ref()
            .and_then(|l| l.uris.as_ref())
            .and_then(|uris| uris.get(index))
            .map(|u| u.uri_str().to_owned())
            .unwrap_or_default();
    }
    if let Some(index) = key.strip_prefix("field:").and_then(|i| i.parse::<usize>().ok()) {
        return item
            .fields
            .as_ref()
            .and_then(|fields| fields.get(index))
            .and_then(|f| f.value.as_ref())
            .map(|v| v.expose().to_owned())
            .unwrap_or_default();
    }
    if let Some(index) = key.strip_prefix("note:").and_then(|i| i.parse::<usize>().ok()) {
        return item
            .notes
            .as_ref()
            .and_then(|n| n.expose().lines().nth(index).map(str::to_owned))
            .unwrap_or_default();
    }
    if let Some(label) = key.strip_prefix("unknown:") {
        return item
            .unknown_fields()
            .into_iter()
            .find(|(l, _, _)| l == label)
            .map(|(_, value, _)| value)
            .unwrap_or_default();
    }
    let identity = item.identity.clone().unwrap_or_default();
    let reveal = |s: &Option<crate::bw::sensitive::Sensitive>| {
        s.as_ref().map(|v| v.expose().to_owned()).unwrap_or_default()
    };
    match key {
        "cardholder" => {
            item.card.as_ref().and_then(|c| c.cardholder_name.clone()).unwrap_or_default()
        }
        "expiry" => item
            .card
            .as_ref()
            .map(|c| {
                format!(
                    "{}/{}",
                    c.exp_month.clone().unwrap_or_default(),
                    c.exp_year.clone().unwrap_or_default()
                )
            })
            .unwrap_or_default(),
        "fingerprint" => {
            item.ssh_key.as_ref().map(|k| k.key_fingerprint.clone()).unwrap_or_default()
        }
        "id:name" => identity.full_name(),
        "id:contact" => identity.email.clone().unwrap_or_default(),
        "id:address" => [
            identity.address1.clone(),
            identity.address2.clone(),
            identity.address3.clone(),
            identity.city.clone(),
            identity.state.clone(),
            identity.postal_code.clone(),
            identity.country.clone(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", "),
        "id:ssn" => reveal(&identity.ssn),
        "id:passport" => reveal(&identity.passport_number),
        "id:license" => reveal(&identity.license_number),
        "passkey" => String::new(),
        other => field_value(item, other, None, state),
    }
}

pub(crate) fn reprompt(state: &mut AppState, action: impl Into<Deferred>) -> Vec<Effect> {
    match state.selected_item().cloned() {
        Some(item) => reprompt_for(state, &item, action),
        None => Vec::new(),
    }
}

pub(crate) fn reprompt_for(
    state: &mut AppState,
    item: &Item,
    action: impl Into<Deferred>,
) -> Vec<Effect> {
    let (id, name) = (item.id.clone().unwrap_or_default(), item.name.clone());
    state.open(RepromptState {
        item_id: id,
        item_name: name.clone(),
        password: crate::input::secret::SecretInput::new(),
        error: None,
        action: action.into(),
        request: None,
    });
    state.message(MessageLevel::Info, format!("“{name}” asks for your master password."));
    Vec::new()
}

pub(crate) fn copy_action(state: &mut AppState, args: &[String]) -> Vec<Effect> {
    let Some(item) = state.selected_item().cloned() else { return Vec::new() };
    let field = args.first().map(String::as_str).unwrap_or("primary");
    let label = field_label(field);
    let secret = matches!(
        field,
        "password" | "totp" | "number" | "code" | "primary" | "private_key" | "field" | "notes"
    );
    if secret && reprompt_required(state, &item) {
        let action = Action { id: ActionId::ItemCopy, args: args.to_vec(), count: None };
        return reprompt(state, action);
    }
    let value = field_value(&item, field, args.get(1).map(String::as_str), state);
    copy_value(state, &value, label, &item.name)
}

pub(crate) fn field_label(field: &str) -> &'static str {
    match field {
        "username" => "username",
        "password" | "primary" => "password",
        "totp" => "verification code",
        "uri" => "website",
        "notes" => "notes",
        "number" => "card number",
        "code" => "security code",
        "public_key" => "public key",
        "private_key" => "private key",
        _ => "value",
    }
}

pub(crate) fn field_value(item: &Item, field: &str, arg: Option<&str>, state: &AppState) -> String {
    let login = item.login.as_ref();
    match field {
        "username" => login
            .and_then(|l| l.username.as_ref())
            .map(|s| s.expose().to_owned())
            .unwrap_or_default(),
        "password" => login
            .and_then(|l| l.password.as_ref())
            .map(|s| s.expose().to_owned())
            .unwrap_or_default(),
        "primary" => match item.kind {
            ItemType::Login => login
                .and_then(|l| l.password.as_ref())
                .map(|s| s.expose().to_owned())
                .unwrap_or_default(),
            ItemType::Card => item
                .card
                .as_ref()
                .and_then(|c| c.number.as_ref())
                .map(|s| s.expose().to_owned())
                .unwrap_or_default(),
            ItemType::SecureNote => {
                item.notes.as_ref().map(|n| n.expose().to_owned()).unwrap_or_default()
            }
            ItemType::SshKey => {
                item.ssh_key.as_ref().map(|k| k.public_key.clone()).unwrap_or_default()
            }
            _ => String::new(),
        },
        "totp" => {
            if !state.config.security.totp {
                return String::new();
            }
            login
                .and_then(|l| l.totp.as_ref())
                .and_then(|s| crate::totp::current(s.expose(), state.now.timestamp() as u64).ok())
                .map(|code| code.code)
                .unwrap_or_default()
        }
        "uri" => login.and_then(|l| l.first_uri()).unwrap_or_default().to_owned(),
        "notes" => item.notes.as_ref().map(|n| n.expose().to_owned()).unwrap_or_default(),
        "number" => item
            .card
            .as_ref()
            .and_then(|c| c.number.as_ref())
            .map(|s| s.expose().to_owned())
            .unwrap_or_default(),
        "code" => item
            .card
            .as_ref()
            .and_then(|c| c.code.as_ref())
            .map(|s| s.expose().to_owned())
            .unwrap_or_default(),
        "public_key" => item.ssh_key.as_ref().map(|k| k.public_key.clone()).unwrap_or_default(),
        "private_key" => item
            .ssh_key
            .as_ref()
            .and_then(|k| k.private_key.as_ref())
            .map(|s| s.expose().to_owned())
            .unwrap_or_default(),
        "field" => {
            let name = arg.unwrap_or("");
            item.fields
                .as_ref()
                .and_then(|fields| fields.iter().find(|f| f.name_str() == name))
                .and_then(|f| f.value.as_ref())
                .map(|v| v.expose().to_owned())
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

pub(crate) fn reveal_action(state: &mut AppState, id: ActionId) -> Vec<Effect> {
    match id {
        ActionId::ItemRevealAll => {
            let rows = detail_fields(state);
            for (key, _, secret) in rows {
                if secret {
                    state.detail.revealed.insert(key);
                }
            }
            state.detail.hide_at =
                state.config.security.reveal_timeout.get().map(|d| Instant::now() + d);
        }
        ActionId::ItemHideAll => {
            state.detail.revealed.clear();
            state.detail.hide_at = None;
        }
        _ => {
            if let Some(key) = reveal_target(state)
                && !state.detail.revealed.remove(&key)
            {
                state.detail.revealed.insert(key);
                state.detail.hide_at =
                    state.config.security.reveal_timeout.get().map(|d| Instant::now() + d);
            }
        }
    }
    Vec::new()
}

/// The secret field `za` toggles: the one under the detail cursor, or, from
/// the list, the item's first secret (the password of a login).
pub(crate) fn reveal_target(state: &AppState) -> Option<String> {
    if state.focus == crate::app::state::Focus::Detail {
        return detail_field_at(state, state.detail.row)
            .and_then(|(key, _, secret)| secret.then_some(key));
    }
    detail_fields(state).into_iter().find(|(_, _, secret)| *secret).map(|(key, _, _)| key)
}

/// The fields of the detail pane: (key, label, secret).
#[must_use]
pub fn detail_fields(state: &AppState) -> Vec<(String, String, bool)> {
    let Some(item) = state.selected_item() else { return Vec::new() };
    let mut out = Vec::new();
    match item.kind {
        ItemType::Login => {
            if let Some(login) = &item.login {
                out.push(("username".to_owned(), "USERNAME".to_owned(), false));
                out.push(("password".to_owned(), "PASSWORD".to_owned(), true));
                if login.has_totp() && state.config.security.totp {
                    out.push(("totp".to_owned(), "VERIFICATION CODE".to_owned(), false));
                }
                if let Some(uris) = &login.uris {
                    for (i, uri) in uris.iter().enumerate() {
                        if !uri.uri_str().is_empty() {
                            out.push((format!("uri:{i}"), "WEBSITES".to_owned(), false));
                        }
                    }
                }
                if item.has_passkey() {
                    out.push(("passkey".to_owned(), "PASSKEY".to_owned(), false));
                }
            }
        }
        ItemType::Card => {
            out.push(("cardholder".to_owned(), "CARDHOLDER NAME".to_owned(), false));
            out.push(("number".to_owned(), "NUMBER".to_owned(), true));
            out.push(("expiry".to_owned(), "EXPIRATION".to_owned(), false));
            out.push(("code".to_owned(), "SECURITY CODE".to_owned(), true));
        }
        ItemType::Identity => {
            for (key, label) in [
                ("id:name", "NAME"),
                ("id:contact", "CONTACT"),
                ("id:address", "ADDRESS"),
                ("id:ssn", "SSN"),
                ("id:passport", "PASSPORT NUMBER"),
                ("id:license", "LICENCE NUMBER"),
            ] {
                out.push((
                    key.to_owned(),
                    label.to_owned(),
                    key.contains("ssn") || key.contains("passport") || key.contains("license"),
                ));
            }
        }
        ItemType::SecureNote => {
            // Counted in place: copying the note would leave another copy
            // of it in memory on every keystroke.
            let lines = item.notes.as_ref().map_or(0, |n| n.expose().lines().count());
            if (item.wants_reprompt() && !state.detail.revealed.contains("notes")) || lines == 0 {
                out.push(("notes".to_owned(), "NOTE".to_owned(), item.wants_reprompt()));
            } else {
                // One row per line: j/k move by line, yy copies the line.
                for index in 0..lines {
                    out.push((format!("note:{index}"), "NOTE LINE".to_owned(), false));
                }
            }
        }
        ItemType::SshKey => {
            out.push(("public_key".to_owned(), "PUBLIC KEY".to_owned(), false));
            out.push(("fingerprint".to_owned(), "FINGERPRINT".to_owned(), false));
            out.push(("private_key".to_owned(), "PRIVATE KEY".to_owned(), true));
        }
        ItemType::Other(_) => {
            for (label, _, secret) in item.unknown_fields() {
                out.push((format!("unknown:{label}"), label.to_uppercase(), secret));
            }
        }
    }
    // Custom fields and notes, in the order the detail pane draws them.
    if let Some(fields) = &item.fields {
        for (i, field) in fields.iter().enumerate() {
            out.push((
                format!("field:{i}"),
                field.name_str().to_uppercase(),
                field.kind() == FieldKind::Hidden,
            ));
        }
    }
    if item.kind != ItemType::SecureNote && item.notes.as_ref().is_some_and(|n| !n.is_empty()) {
        out.push(("notes".to_owned(), "NOTES".to_owned(), item.wants_reprompt()));
    }
    out
}

pub(crate) fn detail_field_at(state: &AppState, index: usize) -> Option<(String, String, bool)> {
    detail_fields(state).get(index).cloned()
}

pub(crate) fn trash_items(state: &mut AppState, ids: Vec<String>) -> Vec<Effect> {
    let mut effects = Vec::new();
    let mut trashed = Vec::new();
    for id in ids {
        if let Some(mut item) = state.cache.item(&id).cloned() {
            item.deleted_date = Some(state.now.to_rfc3339());
            state.cache.upsert(item);
            trashed.push(id.clone());
        }
        effects.push(request(
            state,
            CommandKind::DeleteItem { id: id.clone(), permanent: false },
            Pending::Delete { id, permanent: false },
        ));
    }
    if !trashed.is_empty() {
        state.undo.push(crate::app::state::UndoOp::Trash { ids: trashed.clone() });
    }
    if trashed.len() > 1 {
        toast(
            state,
            MessageLevel::Ok,
            format!("Moved {} items to Trash. Press u to undo.", trashed.len()),
        );
    } else if let Some(id) = trashed.first() {
        let name = state.cache.item(id).map(|i| i.name.clone()).unwrap_or_default();
        toast(state, MessageLevel::Ok, format!("Moved “{name}” to Trash. Press u to undo."));
    }
    state.clear_selection();
    state.clamp_selection();
    effects
}

pub(crate) fn favorite_action(state: &mut AppState, count: Option<u32>) -> Vec<Effect> {
    let ids = state.target_ids(count);
    let mut effects = Vec::new();
    let mut changed = Vec::new();
    let mut before = Vec::new();
    let make_favorite = ids.iter().filter_map(|id| state.cache.item(id)).any(|item| !item.favorite);
    for id in ids {
        if let Some(mut item) = state.cache.item(&id).cloned() {
            changed.push(id.clone());
            before.push(item.favorite);
            item.favorite = make_favorite;
            state.cache.upsert(item.clone());
            effects.push(request(
                state,
                CommandKind::PutItem { id: id.clone(), item: Box::new(item) },
                Pending::Save { id: Some(id.clone()), close: false, editor: None },
            ));
        }
    }
    if !before.is_empty() {
        let after = vec![make_favorite; before.len()];
        // The ids changed above, not a fresh selection: in the Favorites
        // view the un-favourited items have already left the list.
        state.undo.push(crate::app::state::UndoOp::Favorite { ids: changed, before, after });
    }
    state.clear_selection();
    state.clamp_selection();
    effects
}

pub(crate) fn open_folder_picker(state: &mut AppState) {
    let current = current_folder_path(state);
    let mut entries = vec![PickerEntry {
        label: "+ Create folder…".to_owned(),
        note: String::new(),
        filter: "create folder".to_owned(),
        value: "__create__".to_owned(),
        accelerator: None,
    }];
    entries.push(PickerEntry {
        label: "No folder".to_owned(),
        note: String::new(),
        filter: "no folder".to_owned(),
        value: String::new(),
        accelerator: None,
    });
    if let Some(entry) = entries.last_mut() {
        let unfiled = state.selected_item().is_some_and(|item| item.folder_id.is_none());
        entry.note = if unfiled { "current" } else { "" }.to_owned();
    }
    // Bitwarden allows two folders with the same name; the sidebar shows
    // them as one row, and so does the picker.
    let mut seen = std::collections::BTreeSet::new();
    for folder in state.cache.folders() {
        if !seen.insert(folder.name.as_str()) {
            continue;
        }
        let note = if folder.name == current { "current".to_owned() } else { String::new() };
        entries.push(PickerEntry {
            label: folder.name.clone(),
            note,
            filter: folder.name.clone(),
            value: folder.name.clone(),
            accelerator: None,
        });
    }
    state.open(PickerState::new("Move to folder", PickerKind::Folder, entries));
}

pub(crate) fn move_items(state: &mut AppState, folder: &str, count: Option<u32>) -> Vec<Effect> {
    let folder_id = if folder.is_empty() {
        None
    } else {
        // A name that matches no folder is a typo, not "No folder": moving
        // on would take the items out of the folder they are in.
        let found =
            state.cache.folders().iter().find(|f| f.name == folder).and_then(|f| f.id.clone());
        if found.is_none() {
            state.toast(
                MessageLevel::Error,
                format!("No folder named “{folder}”. Nothing was moved."),
            );
            return Vec::new();
        }
        found
    };
    let folder_path = folder.to_owned();
    let ids = state.target_ids(count);
    let mut effects = Vec::new();
    let mut moved = Vec::new();
    for id in ids {
        if let Some(mut item) = state.cache.item(&id).cloned() {
            moved.push((id.clone(), item.folder_id.clone(), folder_id.clone()));
            item.folder_id = folder_id.clone();
            state.cache.upsert(item.clone());
            effects.push(request(
                state,
                CommandKind::PutItem { id: id.clone(), item: Box::new(item) },
                Pending::Save { id: Some(id.clone()), close: false, editor: None },
            ));
        }
    }
    if !moved.is_empty() {
        state.undo.push(crate::app::state::UndoOp::Move { items: moved.clone() });
        let label = if folder_path.is_empty() { "No folder".to_owned() } else { folder_path };
        state.toast(MessageLevel::Ok, format!("Moved {} item(s) to {label}.", moved.len()));
    }
    state.clear_selection();
    state.clamp_selection();
    effects
}

pub(crate) fn search_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    match action.id {
        ActionId::SearchStart => {
            state.search.active = true;
            state.search.query = Some(Query::parse(&state.search.input));
            state.screen = Screen::Vault;
            state.focus = crate::app::state::Focus::List;
        }
        ActionId::SearchNext => {
            if state.search.query.is_some() {
                let len = state.visible_rows().len();
                if len > 0 {
                    state.list_row = (state.list_row + 1) % len;
                }
            }
        }
        ActionId::SearchPrev => {
            if state.search.query.is_some() {
                let len = state.visible_rows().len();
                if len > 0 {
                    state.list_row = (state.list_row + len - 1) % len;
                }
            }
        }
        ActionId::SearchClear => {
            state.search = crate::app::state::SearchState::default();
            state.list_row = 0;
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn search_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match key.code {
        Code::Esc => {
            state.search.active = false;
            state.search.input.clear();
            state.search.query = None;
        }
        Code::Enter => {
            state.search.active = false;
            state.focus = crate::app::state::Focus::List;
        }
        _ => {
            if crate::input::line::edit_at_end(&mut state.search.input, key) {
                state.search.query = Some(Query::parse(&state.search.input));
                state.list_row = 0;
            }
        }
    }
    Vec::new()
}

pub(crate) fn navigation(state: &mut AppState, action: &Action) -> Vec<Effect> {
    let count = action.count.map(|c| c as usize);
    let id = action.id;
    // Each list moves by its own height: the rows the renderer last showed
    // (`AppState::page`), so a page is a page at any terminal size.
    let view = |state: &AppState, key: &'static str| View::of(state, key);
    match state.screen {
        Screen::Settings => {
            let len = settings_rows(state).len();
            state.settings.index =
                move_index(state.settings.index, len, id, count, view(state, "settings"));
            return Vec::new();
        }
        Screen::KeymapEditor => {
            let len = keymap_rows(state).len();
            state.keymap_editor.index =
                move_index(state.keymap_editor.index, len, id, count, view(state, "keymap_editor"));
            return Vec::new();
        }
        Screen::Help | Screen::Messages => {
            scroll_text(state, id, count);
            return Vec::new();
        }
        _ if state.generator_popup_open() || state.screen == Screen::Generator => {
            let rows = if state.generator.request.passphrase { 4 } else { 8 };
            let all = View { top: 0, page: rows };
            state.generator.row =
                move_index(state.generator.row.min(rows - 1), rows, id, count, all);
            return Vec::new();
        }
        Screen::Health => {
            let len = state
                .health
                .report
                .as_ref()
                .map(|report| report.len_in(state.health.category))
                .unwrap_or(0);
            state.health.index =
                move_index(state.health.index, len, id, count, view(state, "health"));
            return Vec::new();
        }
        Screen::Editor => {
            let len = state.editor.as_ref().map(|e| e.rows.len()).unwrap_or(0);
            let page = View { top: 0, page: (state.size.1 as usize).saturating_sub(6).max(1) };
            if let Some(editor) = &mut state.editor {
                editor.row = move_index(editor.row, len, id, count, page);
            }
            return Vec::new();
        }
        _ => {}
    }
    let pane = match state.focus {
        crate::app::state::Focus::Sidebar => "sidebar",
        crate::app::state::Focus::List => "list",
        crate::app::state::Focus::Detail => "detail",
    };
    match state.focus {
        crate::app::state::Focus::Sidebar if rail_active(state) => {
            move_rail(state, id, count);
        }
        crate::app::state::Focus::Sidebar => {
            let entries = sidebar_entries(state);
            let next = move_index(state.sidebar_row, entries.len(), id, count, view(state, pane));
            if next != state.sidebar_row {
                // Skip section labels.
                let forward = next > state.sidebar_row;
                let mut candidate = next;
                while candidate < entries.len() && !entries[candidate].selectable() {
                    if forward {
                        if candidate + 1 >= entries.len() {
                            break;
                        }
                        candidate += 1;
                    } else if candidate == 0 {
                        break;
                    } else {
                        candidate -= 1;
                    }
                }
                if entries.get(candidate).is_some_and(|e| e.selectable()) {
                    state.sidebar_row = candidate;
                }
            }
        }
        crate::app::state::Focus::Detail => {
            let fields = detail_fields(state).len();
            state.detail.row = move_index(state.detail.row, fields, id, count, view(state, pane));
        }
        crate::app::state::Focus::List => {
            let len = state.visible_rows().len();
            state.list_row = move_index(state.list_row, len, id, count, view(state, pane));
        }
    }
    // zz, zt, zb scroll the pane around its cursor; the renderer keeps the
    // offset as long as the cursor stays in view.
    if matches!(id, ActionId::ViewCenter | ActionId::ViewTop | ActionId::ViewBottom) {
        let cursor = match state.focus {
            crate::app::state::Focus::Sidebar => state.sidebar_row,
            crate::app::state::Focus::List => state.list_row,
            crate::app::state::Focus::Detail => state.detail.row,
        };
        let page = state.page(pane);
        let offset = match id {
            ActionId::ViewCenter => cursor.saturating_sub(page / 2),
            ActionId::ViewTop => cursor,
            _ => (cursor + 1).saturating_sub(page),
        };
        state.ui_scroll.borrow_mut().insert(pane, offset);
    }
    state.clamp_selection();
    Vec::new()
}

/// What a list shows: its first visible row and how many rows fit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct View {
    /// The first row on screen.
    pub(crate) top: usize,
    /// Rows on screen.
    pub(crate) page: usize,
}

impl View {
    fn of(state: &AppState, key: &'static str) -> Self {
        Self { top: state.ui_scroll.borrow().get(key).copied().unwrap_or(0), page: state.page(key) }
    }
}

/// Scrolls the help and message screens, which have no cursor.
fn scroll_text(state: &mut AppState, id: ActionId, count: Option<usize>) {
    let key = if state.screen == Screen::Help { "help" } else { "messages" };
    let page = state.page(key);
    let max = state
        .ui_scroll
        .borrow()
        .get(if key == "help" { "help.max" } else { "messages.max" })
        .copied()
        .unwrap_or(usize::MAX);
    let offset = if key == "help" { state.help_offset } else { state.messages_offset };
    let step = count.unwrap_or(1);
    let next = match id {
        ActionId::NavDown => offset + step,
        ActionId::NavUp => offset.saturating_sub(step),
        ActionId::NavHalfDown => offset + (page / 2).max(1) * step,
        ActionId::NavHalfUp => offset.saturating_sub((page / 2).max(1) * step),
        ActionId::NavPageDown => offset + page.max(1) * step,
        ActionId::NavPageUp => offset.saturating_sub(page.max(1) * step),
        ActionId::NavTop => 0,
        ActionId::NavBottom => max,
        _ => offset,
    }
    .min(max);
    if key == "help" {
        state.help_offset = next;
    } else {
        state.messages_offset = next;
    }
}

/// The row a motion lands on, Vim style: a count repeats `j`/`k` and picks
/// the row for `gg`/`G`; `Ctrl-d`/`Ctrl-u` move half the view and
/// `Ctrl-f`/`Ctrl-b` a whole one; `H`, `M`, `L` go to the top, middle and
/// bottom of what is on screen.
pub(crate) fn move_index(
    current: usize,
    len: usize,
    action: ActionId,
    count: Option<usize>,
    view: View,
) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let page = view.page.max(1);
    let half = (page / 2).max(1);
    let step = count.unwrap_or(1);
    let top = view.top.min(last);
    let bottom = (top + page - 1).min(last);
    match action {
        ActionId::NavDown => (current + step).min(last),
        ActionId::NavUp => current.saturating_sub(step),
        ActionId::NavTop => count.unwrap_or(1).saturating_sub(1).min(last),
        ActionId::NavBottom => count.unwrap_or(len).saturating_sub(1).min(last),
        ActionId::NavHalfDown => (current + half * step).min(last),
        ActionId::NavHalfUp => current.saturating_sub(half * step),
        ActionId::NavPageDown => (current + page * step).min(last),
        ActionId::NavPageUp => current.saturating_sub(page * step),
        ActionId::NavScreenTop => (top + count.unwrap_or(1) - 1).min(bottom),
        ActionId::NavScreenMiddle => top + (bottom - top) / 2,
        ActionId::NavScreenBottom => bottom.saturating_sub(count.unwrap_or(1) - 1).max(top),
        _ => current,
    }
}

pub(crate) fn undo(state: &mut AppState, is_undo: bool) -> Vec<Effect> {
    let op = if is_undo { state.undo.pop_undo() } else { state.undo.pop_redo() };
    let Some(op) = op else {
        state.message(
            MessageLevel::Info,
            if is_undo { "Already at the oldest change." } else { "Already at the newest change." },
        );
        return Vec::new();
    };
    let description = op.description();
    // The same entry moves to the opposite stack. Folder creation on redo is
    // re-recorded by the create response, so it is not pushed twice.
    let folder_create = matches!(op, crate::app::state::UndoOp::FolderCreate { .. });
    if is_undo {
        state.undo.redo.push(op.clone());
    } else if !folder_create {
        state.undo.undo.push(op.clone());
    }
    let mut effects = Vec::new();
    use crate::app::state::UndoOp;
    match op.clone() {
        UndoOp::Trash { ids } | UndoOp::Restore { ids } => {
            let restoring = matches!(op, UndoOp::Trash { .. }) == is_undo;
            for id in ids {
                if let Some(mut item) = state.cache.item(&id).cloned() {
                    item.deleted_date = if restoring { None } else { Some(state.now.to_rfc3339()) };
                    state.cache.upsert(item);
                }
                if restoring {
                    effects.push(request(
                        state,
                        CommandKind::RestoreItem(id.clone()),
                        Pending::Restore { id },
                    ));
                } else {
                    effects.push(request(
                        state,
                        CommandKind::DeleteItem { id: id.clone(), permanent: false },
                        Pending::Delete { id, permanent: false },
                    ));
                }
            }
        }
        UndoOp::Edit { id, before, after } => {
            let target = if is_undo { *before } else { *after };
            state.cache.upsert(target.clone());
            effects.push(request(
                state,
                CommandKind::PutItem { id: id.clone(), item: Box::new(target) },
                Pending::Save { id: Some(id), close: false, editor: None },
            ));
        }
        UndoOp::Create { item } => {
            let id = item.id.clone().unwrap_or_default();
            if is_undo {
                if let Some(mut cached) = state.cache.item(&id).cloned() {
                    cached.deleted_date = Some(state.now.to_rfc3339());
                    state.cache.upsert(cached);
                }
                effects.push(request(
                    state,
                    CommandKind::DeleteItem { id: id.clone(), permanent: false },
                    Pending::Delete { id, permanent: false },
                ));
            } else if let Some(mut cached) = state.cache.item(&id).cloned() {
                cached.deleted_date = None;
                state.cache.upsert(cached);
                effects.push(request(
                    state,
                    CommandKind::RestoreItem(id.clone()),
                    Pending::Restore { id },
                ));
            }
        }
        UndoOp::Favorite { ids, before, after } => {
            let values = if is_undo { before } else { after };
            for (id, value) in ids.iter().zip(values.iter()) {
                if let Some(mut item) = state.cache.item(id).cloned() {
                    item.favorite = *value;
                    state.cache.upsert(item.clone());
                    effects.push(request(
                        state,
                        CommandKind::PutItem { id: id.clone(), item: Box::new(item) },
                        Pending::Save { id: Some(id.clone()), close: false, editor: None },
                    ));
                }
            }
        }
        UndoOp::Move { items } => {
            for (id, from, to) in items {
                let folder = if is_undo { from } else { to };
                if let Some(mut item) = state.cache.item(&id).cloned() {
                    item.folder_id = folder.clone();
                    state.cache.upsert(item.clone());
                    effects.push(request(
                        state,
                        CommandKind::PutItem { id: id.clone(), item: Box::new(item) },
                        Pending::Save { id: Some(id.clone()), close: false, editor: None },
                    ));
                }
            }
        }
        UndoOp::FolderCreate { name } => {
            if is_undo {
                if let Some(folder) = state.cache.folders().iter().find(|f| f.name == name) {
                    let empty = !state.cache.active_items().any(|item| item.folder_id == folder.id);
                    if empty {
                        if let Some(id) = folder.id.clone() {
                            effects.push(request(
                                state,
                                CommandKind::DeleteFolder(id),
                                Pending::Folder,
                            ));
                        }
                    } else {
                        state.message(
                            MessageLevel::Warn,
                            format!("Can't undo folder creation: “{name}” is no longer empty."),
                        );
                    }
                }
            } else {
                effects.push(request(state, CommandKind::CreateFolder(name), Pending::Folder));
            }
        }
        UndoOp::FolderRename { folders } => {
            // By id: a rename keeps the id, and two folders with the same
            // name stay apart.
            for (id, old, new) in folders {
                let name = if is_undo { old } else { new };
                if state.cache.folder(&id).is_some() {
                    effects.push(request(
                        state,
                        CommandKind::RenameFolder { id, name },
                        Pending::Folder,
                    ));
                }
            }
        }
        UndoOp::FolderDelete { folders } => {
            if is_undo {
                let mut parents_first = folders.clone();
                parents_first.sort_by_key(|folder| folder.name.matches('/').count());
                state.folder_restore = Some(crate::app::state::FolderRestore {
                    folders: parents_first,
                    created: BTreeMap::new(),
                    pending: None,
                });
                effects.extend(drive_folder_restore(state));
            } else {
                let mut targets = folders;
                targets.sort_by_key(|folder| std::cmp::Reverse(folder.name.len()));
                for folder in targets {
                    // Every folder the undo recreated under this name.
                    let ids: Vec<String> = state
                        .cache
                        .folders()
                        .iter()
                        .filter(|f| f.name == folder.name)
                        .filter_map(|f| f.id.clone())
                        .collect();
                    for id in ids {
                        effects.push(request(
                            state,
                            CommandKind::DeleteFolder(id),
                            Pending::Folder,
                        ));
                    }
                }
            }
        }
    }
    let message =
        if is_undo { format!("Undone: {description}.") } else { format!("Redone: {description}.") };
    state.toast(MessageLevel::Ok, message);
    effects
}

/// Creates the deleted folders one at a time, then moves their items back.
pub(crate) fn drive_folder_restore(state: &mut AppState) -> Vec<Effect> {
    let Some(restore) = &mut state.folder_restore else { return Vec::new() };
    if let Some(folder) = restore.next().cloned() {
        restore.pending = Some(folder.name.clone());
        return vec![request(state, CommandKind::CreateFolder(folder.name), Pending::Folder)];
    }
    let Some(restore) = state.folder_restore.take() else { return Vec::new() };
    let mut effects = Vec::new();
    for folder in &restore.folders {
        let Some(folder_id) = restore.created.get(&folder.name).cloned() else { continue };
        for item_id in &folder.items {
            if let Some(mut item) = state.cache.item(item_id).cloned() {
                item.folder_id = Some(folder_id.clone());
                state.cache.upsert(item.clone());
                effects.push(request(
                    state,
                    CommandKind::PutItem { id: item_id.clone(), item: Box::new(item) },
                    Pending::Save { id: Some(item_id.clone()), close: false, editor: None },
                ));
            }
        }
    }
    effects.push(request(state, CommandKind::LoadVault, Pending::Load));
    effects
}

pub(crate) fn doctor_lines(state: &AppState) -> Vec<(String, String)> {
    vec![
        ("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned()),
        (
            "bw".to_owned(),
            state
                .bw_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "not found".to_owned()),
        ),
        (
            "bw version".to_owned(),
            state
                .capabilities
                .as_ref()
                .map(|c| c.version.to_string())
                .unwrap_or_else(|| "?".to_owned()),
        ),
        (
            "capabilities".to_owned(),
            state
                .capabilities
                .as_ref()
                .map(|c| {
                    format!(
                        "archived={} new_types={} health={}",
                        c.archived_list, c.new_item_types, c.password_health
                    )
                })
                .unwrap_or_default(),
        ),
        ("transport".to_owned(), format!("{:?}", state.transport)),
        ("clipboard".to_owned(), state.clipboard.backend_name().to_owned()),
        ("terminal".to_owned(), format!("{}×{}", state.size.0, state.size.1)),
        ("truecolor".to_owned(), crate::theme::truecolor_available().to_string()),
        ("keyboard enhancement".to_owned(), state.keyboard_enhanced.to_string()),
        ("config".to_owned(), state.paths.config_file().display().to_string()),
        ("state".to_owned(), state.paths.state_dir.display().to_string()),
        ("log".to_owned(), state.paths.log_file().display().to_string()),
    ]
}
