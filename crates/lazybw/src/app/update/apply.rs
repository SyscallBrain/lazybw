//! The action dispatcher: every key, command, picker and menu entry ends here.

use super::{
    Action, AppState, BTreeSet, CmdlineState, CommandKind, ConfirmState, Deferred, DetailState,
    EditorTarget, Effect, Filter, FormKind, HistoryState, InfoState, ItemType, MessageLevel,
    Pending, PickerEntry, PickerKind, PickerState, PromptAction, PromptState, QuickOpenState,
    RESERVED_FOLDER_MESSAGE, Screen, SortOrder, SshKey, SuspendTask, Tab, cancel, close_editor,
    close_notes, cmdline_action, confirm_generate, confirm_yes, copy_action, copy_value,
    current_folder_id, current_folder_path, detail_field_at, detail_value, doctor_lines, editable,
    editor_action, favorite_action, fold_action, generator_action, health_action, keymap_action,
    login_action, move_items, navigation, open_editor, open_export, open_folder_picker,
    open_import, open_keymap_editor, open_new_editor, open_theme_picker, picker_action,
    quick_open_action, rail_active, rebuild_rows, refresh_quick_open, reprompt, reprompt_for,
    reprompt_required, request, reserved_folder_name, reveal_action, reveal_target, search_action,
    select_item_by_id, selected_name, selected_reprompt_required, settings_action,
    shellexpand_home, sidebar_entries, switch_tab, trash_items, undo,
};
use crate::app::actions::ActionId;

pub(crate) fn apply(state: &mut AppState, action: Action) -> Vec<Effect> {
    let id = action.id;
    let args = action.args.clone();
    let count = action.count;
    match id {
        ActionId::Noop => Vec::new(),
        ActionId::AppQuit | ActionId::AppQuitForce
            if state.screen == Screen::Editor
                && state.editor.is_some()
                && args.first().map(String::as_str) != Some("all") =>
        {
            // In the editor `:q` closes the item (asking about unsaved
            // changes) and `:q!` discards them; neither quits lazybw.
            if id == ActionId::AppQuitForce {
                return apply(state, Action::new(ActionId::EditorDiscard));
            }
            close_notes(state);
            close_editor(state);
            Vec::new()
        }
        ActionId::AppQuit if state.editor.as_ref().is_some_and(|e| e.dirty) => {
            state.open(
                ConfirmState::new(
                    "Unsaved changes",
                    "The item has unsaved changes. Quit and discard them?",
                    "Quit",
                    Deferred::QuitDiscarding,
                )
                .danger(),
            );
            Vec::new()
        }
        // The form's changes are given up; changes already sent are not.
        ActionId::AppQuit => quit(state, false),
        ActionId::AppQuitForce => quit(state, true),
        ActionId::AppCancel => cancel(state),
        ActionId::AppHelp => {
            if state.screen != Screen::Help {
                state.help_chain = state.contexts();
            }
            state.screen = Screen::Help;
            state.help_offset = 0;
            state.help_filter.clear();
            Vec::new()
        }
        ActionId::AppRedraw => {
            state.full_redraw = true;
            state.dirty = true;
            Vec::new()
        }
        ActionId::AppMessages => {
            state.screen = Screen::Messages;
            state.messages_offset = 0;
            Vec::new()
        }
        ActionId::AppSettings => {
            state.screen = Screen::Settings;
            state.settings.index = 0;
            Vec::new()
        }
        ActionId::AppKeymap => {
            open_keymap_editor(state);
            Vec::new()
        }
        ActionId::AppDoctor => {
            let lines = doctor_lines(state);
            state.open(InfoState { item_id: String::new(), lines });
            Vec::new()
        }
        ActionId::CmdlineOpen => {
            state.open(CmdlineState::default());
            Vec::new()
        }
        ActionId::QuickOpen => {
            state.open(QuickOpenState::default());
            refresh_quick_open(state);
            Vec::new()
        }
        ActionId::FocusLeft => {
            state.focus = state.focus.left();
            state.detail.row = 0;
            Vec::new()
        }
        ActionId::FocusRight => {
            state.focus = state.focus.right();
            state.detail.row = 0;
            Vec::new()
        }
        ActionId::FocusNext => {
            state.focus = match state.focus {
                crate::app::state::Focus::Sidebar => crate::app::state::Focus::List,
                crate::app::state::Focus::List => crate::app::state::Focus::Detail,
                crate::app::state::Focus::Detail => crate::app::state::Focus::Sidebar,
            };
            Vec::new()
        }
        ActionId::FocusPrev => {
            state.focus = match state.focus {
                crate::app::state::Focus::Sidebar => crate::app::state::Focus::Detail,
                crate::app::state::Focus::List => crate::app::state::Focus::Sidebar,
                crate::app::state::Focus::Detail => crate::app::state::Focus::List,
            };
            Vec::new()
        }
        ActionId::LayoutToggleSidebar => {
            state.sidebar_visible = !state.sidebar_visible;
            Vec::new()
        }
        ActionId::ListToggleDensity => {
            state.config.ui.list_density = state.config.ui.list_density.toggled();
            let value = state.config.ui.list_density.label();
            state.message(MessageLevel::Info, format!("List density: {value}"));
            vec![Effect::SaveConfig(vec![(
                vec!["ui".into(), "list_density".into()],
                toml_edit::Value::from(value.to_owned()),
            )])]
        }
        ActionId::TabNext | ActionId::TabPrev | ActionId::TabGoto => {
            let tab = match id {
                ActionId::TabGoto => match args.first().map(String::as_str) {
                    Some("1") => Tab::Vault,
                    Some("2") => Tab::Generator,
                    Some("3") => Tab::Health,
                    _ => state.tab,
                },
                ActionId::TabNext => match state.tab {
                    Tab::Vault => Tab::Generator,
                    Tab::Generator => Tab::Health,
                    Tab::Health => Tab::Vault,
                },
                _ => match state.tab {
                    Tab::Vault => Tab::Health,
                    Tab::Generator => Tab::Vault,
                    Tab::Health => Tab::Generator,
                },
            };
            switch_tab(state, tab)
        }
        ActionId::JumpBack => {
            if let Some(entry) = state.jump.back().map(|e| e.item_id.clone()) {
                select_item_by_id(state, &entry);
            }
            Vec::new()
        }
        ActionId::JumpForward => {
            if let Some(entry) = state.jump.forward().map(|e| e.item_id.clone()) {
                select_item_by_id(state, &entry);
            }
            Vec::new()
        }
        ActionId::ThemePick => {
            open_theme_picker(state);
            Vec::new()
        }
        ActionId::ThemeSet => {
            if let Some(name) = args.first() {
                let outcome = crate::theme::loader::load(name, &state.paths.themes_dir());
                if let Some(mut theme) = outcome.theme {
                    theme.transparent = state.config.ui.transparent;
                    theme.resolve(crate::theme::truecolor_available(), crate::theme::no_color());
                    state.theme = theme;
                    state.config.ui.theme = name.clone();
                    for warning in outcome.warnings {
                        state.message(MessageLevel::Warn, warning);
                    }
                    state.message(MessageLevel::Ok, format!("Theme: {name}"));
                } else {
                    let detail = outcome.errors.join("; ");
                    state.toast_with(MessageLevel::Error, "Couldn't load the theme.", detail);
                }
            }
            Vec::new()
        }
        ActionId::VaultSync => {
            let force = args.first().is_some_and(|a| a == "force");
            vec![request(state, CommandKind::Sync { force }, Pending::Sync)]
        }
        // Nothing to lock while logged out, starting or on an error (a
        // SIGUSR1 then must not show the unlock screen).
        ActionId::VaultLock if !state.session.holds_vault_data() => Vec::new(),
        ActionId::VaultLock => {
            state.drop_secrets();
            state.message(MessageLevel::Info, "Vault locked.");
            vec![request(state, CommandKind::Lock, Pending::Lock)]
        }
        ActionId::VaultLogout => {
            state.open(ConfirmState::new(
"Log out",
"Log out of Bitwarden? lazybw will stop the CLI and return to the login screen."
                        .to_owned(),
"Log out",
Deferred::Logout,
).danger());
            Vec::new()
        }
        ActionId::VaultExport => open_export(state, &args),
        ActionId::VaultImport => open_import(state, &args),
        ActionId::GeneratorOpen => switch_tab(state, Tab::Generator),
        ActionId::HealthOpen => switch_tab(state, Tab::Health),
        ActionId::SortSet => {
            if let Some(order) = args.first().and_then(|arg| SortOrder::parse(arg)) {
                state.config.vault.sort = order;
                state.sort_reversed = args.get(1).is_some_and(|arg| arg == "reverse");
                let reversed = if state.sort_reversed { ", reversed" } else { "" };
                state.message(MessageLevel::Info, format!("Sorted by {}{reversed}", order.label()));
            }
            Vec::new()
        }
        ActionId::SortReverse => {
            state.sort_reversed = !state.sort_reversed;
            state.message(
                MessageLevel::Info,
                if state.sort_reversed { "Sort reversed." } else { "Sort order restored." },
            );
            Vec::new()
        }
        ActionId::FilterApply => {
            if state.focus == crate::app::state::Focus::Sidebar {
                let entries = sidebar_entries(state);
                // The rail shows every folder as one icon: it opens the
                // folder picker instead of applying whichever folder the
                // hidden cursor happens to sit on.
                if rail_active(state)
                    && entries
                        .get(state.sidebar_row)
                        .is_some_and(|entry| crate::vault::filter::is_folder_row(&entry.role))
                {
                    return apply(state, Action::new(ActionId::FolderJump));
                }
                if let Some(entry) = entries.get(state.sidebar_row)
                    && let Some(filter) = &entry.filter
                {
                    state.filter = filter.clone();
                    state.focus = crate::app::state::Focus::List;
                    state.list_row = 0;
                    state.search = crate::app::state::SearchState::default();
                }
            }
            Vec::new()
        }
        ActionId::FoldExpandOrApply => {
            let entries = sidebar_entries(state);
            if let Some(crate::vault::filter::SidebarRole::Folder {
                path,
                has_children: true,
                expanded: false,
                ..
            }) = entries.get(state.sidebar_row).map(|e| &e.role)
            {
                state.collapsed.remove(path);
                return Vec::new();
            }
            apply(state, Action::new(ActionId::FilterApply))
        }
        ActionId::FoldCollapseOrParent => {
            let entries = sidebar_entries(state);
            let Some(crate::vault::filter::SidebarRole::Folder {
                path,
                has_children,
                expanded,
                ..
            }) = entries.get(state.sidebar_row).map(|e| e.role.clone())
            else {
                return Vec::new();
            };
            if has_children && expanded {
                state.collapsed.insert(path);
            } else if let Some((parent, _)) = path.rsplit_once('/')
                && let Some(index) = entries.iter().position(|e| {
                    matches!(&e.role, crate::vault::filter::SidebarRole::Folder { path, .. } if path == parent)
                })
            {
                state.sidebar_row = index;
            }
            Vec::new()
        }
        ActionId::FoldToggle
        | ActionId::FoldOpen
        | ActionId::FoldClose
        | ActionId::FoldOpenAll
        | ActionId::FoldCloseAll => fold_action(state, id),
        ActionId::FolderNew => {
            if let Some(name) = args.first() {
                if reserved_folder_name(name) {
                    state.toast(MessageLevel::Error, RESERVED_FOLDER_MESSAGE);
                    return Vec::new();
                }
                vec![request(state, CommandKind::CreateFolder(name.clone()), Pending::Folder)]
            } else {
                state.open(PromptState::new("New folder", "", PromptAction::NewFolder));
                Vec::new()
            }
        }
        ActionId::FolderRename => {
            let Some(new_name) = args.first() else {
                let current = current_folder_path(state);
                state.open(PromptState::new("Rename folder", &current, PromptAction::RenameFolder));
                return Vec::new();
            };
            if reserved_folder_name(new_name) {
                state.toast(MessageLevel::Error, RESERVED_FOLDER_MESSAGE);
                return Vec::new();
            }
            let old = current_folder_path(state);
            if old.is_empty() {
                state.toast(MessageLevel::Error, "No folder selected.");
                return Vec::new();
            }
            let ids = folder_ids(state, &old);
            if ids.is_empty() {
                state.toast(MessageLevel::Error, "No such folder.");
                return Vec::new();
            }
            // The dialog renames the folders it named, by id, whatever the
            // sidebar shows by the time it is answered.
            let subfolders =
                crate::vault::folders::renamed_subfolders(state.cache.folders(), &old, new_name);
            let count = subfolders.len();
            let folders = ids.into_iter().map(|id| (id, new_name.clone())).collect();
            state.open(
                ConfirmState::new(
                    "Rename folder",
                    format!("Rename “{old}” to “{new_name}”?"),
                    "Rename",
                    Deferred::RenameFolder { folders, subfolders, also: true },
                )
                .subfolders(count, true),
            );
            Vec::new()
        }
        ActionId::FolderDelete => {
            let selected = args
                .first()
                .cloned()
                .or_else(|| Some(current_folder_path(state)))
                .filter(|name| !name.is_empty());
            let Some(name) = selected else { return Vec::new() };
            let ids = folder_ids(state, &name);
            if ids.is_empty() {
                state.toast(MessageLevel::Error, format!("No folder named “{name}”."));
                return Vec::new();
            }
            let count = state
                .cache
                .active_items()
                .filter(|item| item.folder_id.as_ref().is_some_and(|id| ids.contains(id)))
                .count();
            let prefix = format!("{name}/");
            let subfolders: Vec<String> = state
                .cache
                .folders()
                .iter()
                .filter(|folder| folder.name.starts_with(&prefix))
                .filter_map(|folder| folder.id.clone())
                .collect();
            let mut message = if count == 1 {
                format!("Delete folder “{name}”? Its 1 item moves to No folder.")
            } else {
                format!("Delete folder “{name}”? Its {count} items move to No folder.")
            };
            if !subfolders.is_empty() {
                message.push_str(&format!(
                    " {} subfolder(s) will keep their names but their parent disappears.",
                    subfolders.len()
                ));
            }
            let total = subfolders.len();
            state.open(
                ConfirmState::new(
                    "Delete folder",
                    message,
                    "Delete",
                    Deferred::DeleteFolder { ids, subfolders, also: false },
                )
                .danger()
                .subfolders(total, false),
            );
            Vec::new()
        }
        ActionId::FolderJump => {
            let current = match &state.filter {
                Filter::Folder(path) => Some(path.as_str()),
                Filter::NoFolder => Some(""),
                _ => None,
            };
            let note = |value: &str| {
                if current == Some(value) { "current".to_owned() } else { String::new() }
            };
            // lazybw's "items without a folder" filter comes first, labelled,
            // so it is never confused with a real folder of a similar name.
            let mut entries = vec![PickerEntry {
                label: "No folder".to_owned(),
                note: note(""),
                filter: "no folder".to_owned(),
                value: String::new(),
                accelerator: None,
            }];
            let mut seen = BTreeSet::new();
            let folders = state.cache.folders().iter().filter(|f| seen.insert(f.name.as_str()));
            entries.extend(folders.map(|f| PickerEntry {
                label: f.name.clone(),
                note: note(&f.name),
                filter: f.name.clone(),
                value: f.name.clone(),
                accelerator: None,
            }));
            state.open(PickerState::new("Go to folder", PickerKind::FolderJump, entries));
            Vec::new()
        }
        ActionId::TrashEmpty => {
            let count = state.cache.count(&Filter::Trash);
            if count == 0 {
                state.message(MessageLevel::Info, "The trash is empty.");
                return Vec::new();
            }
            state.open(ConfirmState::new(
"Empty trash",
format!(
                    "{} will be deleted for good. This cannot be undone. Type empty to confirm.",
                    crate::util::text::items(count)
                ),
"Empty trash",
Deferred::EmptyTrash,
).danger().word("empty"));
            Vec::new()
        }
        ActionId::ItemOpen => {
            state.focus = crate::app::state::Focus::Detail;
            state.detail = DetailState::default();
            if let Some(id) = state.selected_id() {
                state.jump.push(id);
            }
            Vec::new()
        }
        ActionId::ItemCopy => copy_action(state, &args),
        ActionId::ItemCopyField => {
            let field = detail_field_at(state, state.detail.row);
            let Some(item) = state.selected_item().cloned() else { return Vec::new() };
            if let Some((key, label, secret)) = field {
                // The code is not masked, but it opens the account like the
                // password does: `yt` asks for the master password, and so
                // does `yy` on it.
                if (secret || key == "totp") && reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                let value = detail_value(state, &item, &key);
                copy_value(state, &value, &label.to_lowercase(), &item.name)
            } else {
                Vec::new()
            }
        }
        ActionId::ItemRevealToggle | ActionId::ItemRevealAll | ActionId::ItemHideAll => {
            if state.screen == Screen::Editor {
                if let Some(editor) = &mut state.editor {
                    editor.revealed = !editor.revealed;
                }
                Vec::new()
            } else {
                let reveals = match id {
                    ActionId::ItemRevealAll => true,
                    ActionId::ItemHideAll => false,
                    _ => reveal_target(state)
                        .is_some_and(|key| !state.detail.revealed.contains(&key)),
                };
                if reveals && selected_reprompt_required(state) {
                    return reprompt(state, action);
                }
                reveal_action(state, id)
            }
        }
        ActionId::ItemNew => {
            if let Some(kind) = args.first().and_then(|arg| ItemType::parse(arg)) {
                open_new_editor(state, kind);
            } else {
                let entries = [
                    (ItemType::Login, 'l'),
                    (ItemType::SecureNote, 'n'),
                    (ItemType::Card, 'c'),
                    (ItemType::Identity, 'i'),
                    (ItemType::SshKey, 's'),
                ]
                .into_iter()
                .map(|(kind, accelerator)| PickerEntry {
                    label: kind.label().to_owned(),
                    note: String::new(),
                    filter: kind.label().to_owned(),
                    value: kind.label().to_owned(),
                    accelerator: Some(accelerator),
                })
                .collect();
                state.open(PickerState::new("New item", PickerKind::ItemType, entries));
            }
            Vec::new()
        }
        ActionId::ItemEdit => {
            if let Some(item) = state.selected_item().cloned() {
                if !editable(state, &item) {
                    return Vec::new();
                }
                if reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                open_editor(state, item, false);
            }
            Vec::new()
        }
        ActionId::ItemEditField | ActionId::ItemChangeField => {
            if let Some(item) = state.selected_item().cloned() {
                if !editable(state, &item) {
                    return Vec::new();
                }
                if reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                // The detail pane and the form list fields differently:
                // find the form row of the field under the detail cursor.
                let target = detail_field_at(state, state.detail.row)
                    .and_then(|(key, _, _)| super::form_key_for_detail(&key));
                open_editor(state, item, false);
                if let Some(editor) = &mut state.editor {
                    if let Some(index) =
                        target.and_then(|key| editor.rows.iter().position(|row| row.key == key))
                    {
                        editor.row = index;
                    }
                    let row = &mut editor.rows[editor.row];
                    if matches!(row.kind, FormKind::Text | FormKind::Secret) {
                        if id == ActionId::ItemChangeField {
                            row.value.clear();
                        }
                        editor.insert = true;
                        editor.cursor = row.value.chars().count();
                    }
                }
            }
            Vec::new()
        }
        ActionId::ItemEditJson => {
            // In the editor, the form as it is now; in the list, the item.
            let current = if state.screen == Screen::Editor && state.editor.is_some() {
                super::sync_editor(state);
                state.editor.as_ref().map(|editor| editor.item.clone())
            } else {
                state.selected_item().cloned()
            };
            if let Some(item) = current {
                if !editable(state, &item) {
                    return Vec::new();
                }
                if reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                let json = crate::bw::sensitive::Sensitive::new(
                    serde_json::to_string_pretty(&item).unwrap_or_default(),
                );
                vec![Effect::Suspend(SuspendTask::Editor { target: EditorTarget::Json(json) })]
            } else {
                Vec::new()
            }
        }
        ActionId::ItemTrash => {
            let ids = state.target_ids(count);
            if ids.is_empty() {
                return Vec::new();
            }
            if state.config.vault.confirm_trash {
                // The dialog acts on the items it counted, whatever the
                // selection is by the time it is answered.
                state.open(ConfirmState::new(
                    "Move to trash",
                    format!("Move {} item(s) to Trash?", ids.len()),
                    "Move to trash",
                    Deferred::TrashItems(ids),
                ));
                return Vec::new();
            }
            trash_items(state, ids)
        }
        ActionId::ItemDelete => {
            if state.screen == Screen::Vault
                && state.focus == crate::app::state::Focus::Detail
                && detail_field_at(state, state.detail.row)
                    .is_some_and(|(key, _, _)| key == "passkey")
            {
                let name = selected_name(state);
                // Straight to the removal: `item.remove_passkey` would
                // ask the same question a second time.
                state.open(ConfirmState::new(
"Remove passkey",
format!(
                        "Remove the passkey from “{name}”? You can add it again from the Bitwarden web vault."
                    ),
"Remove",
Deferred::RemovePasskey(state.selected_id().unwrap_or_default()),
).danger());
                return Vec::new();
            }
            if state.focus == crate::app::state::Focus::Detail {
                state.message(
                    MessageLevel::Info,
                    "Only passkeys are removed from the detail pane; trash items from the list.",
                );
                return Vec::new();
            }
            let ids = state.target_ids(count);
            if ids.is_empty() {
                return Vec::new();
            }
            let name = ids
                .first()
                .and_then(|id| state.cache.item(id))
                .map(|i| i.name.clone())
                .unwrap_or_default();
            let message = match ids.len() {
                1 => format!("“{name}” will be deleted for good. This cannot be undone."),
                count => format!(
                    "{count} items (“{name}” and {}) will be deleted for good. This cannot be undone.",
                    if count == 2 { "1 other".to_owned() } else { format!("{} others", count - 1) }
                ),
            };
            // The dialog deletes the items it named and counted, whatever
            // the selection is by the time it is answered.
            state.open(
                ConfirmState::new(
                    "Delete permanently?",
                    message,
                    "Delete",
                    Deferred::DeleteItems(ids),
                )
                .danger(),
            );
            Vec::new()
        }
        ActionId::ItemRestore => {
            let ids = state.target_ids(count);
            if !ids.is_empty() {
                state.undo.push(crate::app::state::UndoOp::Restore { ids: ids.clone() });
            }
            state.clear_selection();
            let mut effects = Vec::new();
            for id in ids {
                effects.push(request(
                    state,
                    CommandKind::RestoreItem(id.clone()),
                    Pending::Restore { id },
                ));
            }
            effects
        }
        ActionId::ItemFavorite => favorite_action(state, count),
        ActionId::ItemMove => {
            if let Some(folder) = args.first() {
                move_items(state, folder, count)
            } else {
                open_folder_picker(state);
                Vec::new()
            }
        }
        ActionId::ItemYank => {
            if let Some(item) = state.selected_item().cloned() {
                state.yanked_item = Some(item);
                state.message(MessageLevel::Info, "Yanked item. Press p to clone it here.");
            }
            Vec::new()
        }
        ActionId::ItemPasteClone => {
            if let Some(mut item) = state.yanked_item.clone() {
                item.id = None;
                item.name = format!("{} (copy)", item.name);
                if let Some(folder) = current_folder_id(state) {
                    item.folder_id = Some(folder);
                }
                vec![request(
                    state,
                    CommandKind::CreateItem(Box::new(item)),
                    Pending::Save { id: None, close: true, editor: None },
                )]
            } else {
                state.message(MessageLevel::Warn, "Yank an item first with Y.");
                Vec::new()
            }
        }
        ActionId::ItemOpenUri => {
            if let Some(item) = state.selected_item() {
                let uris: Vec<String> = item
                    .login
                    .as_ref()
                    .and_then(|l| l.uris.as_ref())
                    .map(|uris| {
                        uris.iter()
                            .map(|u| u.uri_str().to_owned())
                            .filter(|u| !u.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let index = args
                    .first()
                    .and_then(|a| a.parse::<usize>().ok())
                    .map(|n| n.saturating_sub(1))
                    .unwrap_or(0);
                if let Some(uri) = uris.get(index).cloned() {
                    return open_uri(state, &uri);
                }
                state.message(MessageLevel::Warn, "This item has no website.");
            }
            Vec::new()
        }
        ActionId::ItemHistory => {
            if let Some(item) = state.selected_item().cloned() {
                if item.password_history.as_ref().is_some_and(|h| !h.is_empty())
                    && reprompt_required(state, &item)
                {
                    return reprompt(state, action);
                }
                let entries = item.password_history.clone().unwrap_or_default();
                if entries.is_empty() {
                    state.message(MessageLevel::Info, "No password history.");
                } else {
                    state.open(HistoryState {
                        item_id: item.id.clone().unwrap_or_default(),
                        entries,
                        index: 0,
                        revealed: BTreeSet::new(),
                        current: item
                            .login
                            .as_ref()
                            .and_then(|l| l.password.as_ref())
                            .map(|p| p.expose().to_owned()),
                    });
                }
            }
            Vec::new()
        }
        ActionId::ItemInfo => {
            if let Some(item) = state.selected_item().cloned() {
                let mut lines = Vec::new();
                lines.push(("Name".to_owned(), item.name.clone()));
                lines.push(("Type".to_owned(), item.kind.label().to_owned()));
                if let Some(folder) = &item.folder_id {
                    lines.push((
                        "Folder".to_owned(),
                        state.cache.folder_path(folder).unwrap_or("").to_owned(),
                    ));
                }
                if let Some(date) = &item.creation_date {
                    lines.push(("Created".to_owned(), date.clone()));
                }
                if let Some(date) = &item.revision_date {
                    lines.push(("Edited".to_owned(), date.clone()));
                }
                if let Some(login) = &item.login
                    && let Some(date) = &login.password_revision_date
                {
                    lines.push(("Password changed".to_owned(), date.clone()));
                }
                lines.push((
                    "Master password".to_owned(),
                    if item.wants_reprompt() {
                        "required".to_owned()
                    } else {
                        "not required".to_owned()
                    },
                ));
                lines.push(("Id".to_owned(), item.id.clone().unwrap_or_default()));
                state.open(InfoState { item_id: item.id.clone().unwrap_or_default(), lines });
            }
            Vec::new()
        }
        ActionId::ItemGeneratePassword => {
            // On the Health tab the target is the highlighted report row,
            // never whatever happens to be selected in the vault list.
            let target = if state.screen == Screen::Health {
                state.health.report.as_ref().and_then(|r| {
                    r.item_in(state.health.category, state.health.index)
                        .map(|record| record.id.clone())
                })
            } else {
                state.selected_id()
            };
            let Some(item) = target.and_then(|id| state.cache.item(&id).cloned()) else {
                return Vec::new();
            };
            if item.kind != ItemType::Login || !editable(state, &item) {
                state.message(MessageLevel::Warn, "Only logins have a password to replace.");
                return Vec::new();
            }
            if reprompt_required(state, &item) {
                if state.screen == Screen::Health {
                    select_item_by_id(state, item.id.as_deref().unwrap_or_default());
                    state.screen = Screen::Health;
                }
                return reprompt_for(
                    state,
                    &item,
                    Deferred::ConfirmGeneratePassword(item.id.clone().unwrap_or_default()),
                );
            }
            confirm_generate(state, &item)
        }
        ActionId::ItemRemovePasskey => {
            if state.screen == Screen::Vault
                && state.focus == crate::app::state::Focus::Detail
                && detail_field_at(state, state.detail.row)
                    .is_some_and(|(key, _, _)| key == "passkey")
            {
                let name = selected_name(state);
                state.open(ConfirmState::new(
"Remove passkey",
format!(
                        "Remove the passkey from “{name}”? You can add it again from the Bitwarden web vault."
                    ),
"Remove",
Deferred::RemovePasskey(state.selected_id().unwrap_or_default()),
).danger());
            } else {
                state.message(
                    MessageLevel::Info,
                    "Only passkeys can be removed from the detail pane; trash items from the list.",
                );
            }
            Vec::new()
        }
        ActionId::ItemGotoFolder => {
            if let Some(item) = state.selected_item() {
                let path = item
                    .folder_id
                    .as_ref()
                    .and_then(|id| state.cache.folder_path(id))
                    .map(str::to_owned);
                if let Some(path) = path {
                    state.filter = Filter::Folder(path.clone());
                    state.list_row = 0;
                    if let Some(index) = sidebar_entries(state)
                        .iter()
                        .position(|e| e.filter.as_ref() == Some(&Filter::Folder(path.clone())))
                    {
                        state.sidebar_row = index;
                    }
                } else {
                    state.message(MessageLevel::Info, "This item has no folder.");
                }
            }
            Vec::new()
        }
        ActionId::Undo => undo(state, true),
        ActionId::Redo => undo(state, false),
        ActionId::MarkToggle => {
            if let Some(id) = state.selected_id()
                && !state.marks.remove(&id)
            {
                state.marks.insert(id);
            }
            Vec::new()
        }
        ActionId::VisualStart => {
            state.visual_anchor = Some(state.list_row);
            Vec::new()
        }
        ActionId::VisualExit => {
            state.visual_anchor = None;
            Vec::new()
        }
        ActionId::EditorSave
        | ActionId::EditorSaveClose
        | ActionId::EditorDiscard
        | ActionId::EditorClose
        | ActionId::EditorColumnLeft
        | ActionId::EditorColumnRight
        | ActionId::EditorReload
        | ActionId::EditorNormal
        | ActionId::EditorInsert
        | ActionId::EditorAppend
        | ActionId::EditorInsertStart
        | ActionId::EditorAppendEnd
        | ActionId::EditorChangeField
        | ActionId::EditorNextField
        | ActionId::EditorPrevField
        | ActionId::EditorFirstField
        | ActionId::EditorLastField
        | ActionId::EditorRowAddBelow
        | ActionId::EditorRowAddAbove
        | ActionId::EditorRowDelete
        | ActionId::EditorCycleNext
        | ActionId::EditorCyclePrev
        | ActionId::EditorActivate
        | ActionId::EditorGenerate
        | ActionId::EditorExternal
        | ActionId::EditorUndo
        | ActionId::EditorRedo => editor_action(state, action),
        ActionId::GeneratorRegenerate
        | ActionId::GeneratorToggleMode
        | ActionId::GeneratorIncrease
        | ActionId::GeneratorDecrease
        | ActionId::GeneratorToggleOption
        | ActionId::GeneratorCopy
        | ActionId::GeneratorUse => generator_action(state, action),
        ActionId::HealthRerun
        | ActionId::HealthNextCategory
        | ActionId::HealthPrevCategory
        | ActionId::HealthOpenItem => health_action(state, action),
        ActionId::SettingsActivate
        | ActionId::SettingsNextValue
        | ActionId::SettingsPrevValue
        | ActionId::SettingsReset => settings_action(state, action),
        ActionId::KeymapRebind
        | ActionId::KeymapAdd
        | ActionId::KeymapRemove
        | ActionId::KeymapReset
        | ActionId::KeymapResetAll
        | ActionId::KeymapSave
        | ActionId::KeymapNextContext
        | ActionId::KeymapPrevContext => keymap_action(state, action),
        ActionId::KeymapFilter => {
            let filter = state.keymap_editor.filter.clone();
            state.open(PromptState::new("Filter keybindings", &filter, PromptAction::FilterKeymap));
            Vec::new()
        }
        ActionId::HelpSearch => {
            if state.screen != Screen::Help {
                return apply(state, Action::new(ActionId::AppHelp));
            }
            let filter = state.help_filter.clone();
            state.open(PromptState::new("Search help", &filter, PromptAction::FilterHelp));
            Vec::new()
        }
        ActionId::UnlockSubmit => {
            if state.unlock.password.is_empty() || state.unlocking() {
                return Vec::new();
            }
            state.unlock.error = None;
            let password = state.unlock.password.take_secret();
            vec![request(state, CommandKind::Unlock(password), Pending::Unlock)]
        }
        ActionId::UnlockLogout => {
            state.open(
                ConfirmState::new("Log out", "Log out of Bitwarden?", "Log out", Deferred::Logout)
                    .danger(),
            );
            Vec::new()
        }
        ActionId::ConfirmYes => confirm_yes(state),
        // "n": the three-way dialogs ("y save · n discard · c cancel") run
        // their second choice; the others just close.
        ActionId::ConfirmNo => match state.take_confirm().and_then(|confirm| confirm.on_deny) {
            Some(deferred) => run(state, deferred),
            None => Vec::new(),
        },
        ActionId::LoginNext | ActionId::LoginPrev | ActionId::LoginSubmit | ActionId::LoginBack => {
            login_action(state, action)
        }
        ActionId::NavDown
        | ActionId::NavUp
        | ActionId::NavTop
        | ActionId::NavBottom
        | ActionId::NavHalfDown
        | ActionId::NavHalfUp
        | ActionId::NavPageDown
        | ActionId::NavPageUp
        | ActionId::NavScreenTop
        | ActionId::NavScreenMiddle
        | ActionId::NavScreenBottom
        | ActionId::ViewCenter
        | ActionId::ViewTop
        | ActionId::ViewBottom => navigation(state, &action),
        ActionId::SearchStart
        | ActionId::SearchNext
        | ActionId::SearchPrev
        | ActionId::SearchClear => search_action(state, &action),
        ActionId::PickerNext | ActionId::PickerPrev | ActionId::PickerAccept => {
            picker_action(state, &action)
        }
        ActionId::QuickOpenNext
        | ActionId::QuickOpenPrev
        | ActionId::QuickOpenAccept
        | ActionId::QuickOpenCopyPassword
        | ActionId::QuickOpenCopyUsername
        | ActionId::QuickOpenCopyTotp
        | ActionId::QuickOpenEdit => quick_open_action(state, &action),
        ActionId::CmdlineExecute
        | ActionId::CmdlineCompleteNext
        | ActionId::CmdlineCompletePrev
        | ActionId::CmdlineHistoryPrev
        | ActionId::CmdlineHistoryNext => cmdline_action(state, &action),
    }
}

/// Runs what a dialog was opened for, once it is answered.
pub(crate) fn run(state: &mut AppState, deferred: Deferred) -> Vec<Effect> {
    match deferred {
        Deferred::Run(action) => apply(state, action),
        // The form's changes are given up; changes already sent are not.
        Deferred::QuitDiscarding => quit(state, false),
        Deferred::Logout => {
            state.cache.clear();
            vec![request(state, CommandKind::Logout, Pending::Logout)]
        }
        Deferred::RenameFolder { folders, subfolders, also } => {
            let mut renames = if also { subfolders } else { Vec::new() };
            renames.extend(folders);
            let mut effects = Vec::new();
            let mut undo_folders = Vec::new();
            for (id, name) in renames {
                let Some(old) = state.cache.folder(&id).map(|folder| folder.name.clone()) else {
                    continue;
                };
                undo_folders.push((id.clone(), old, name.clone()));
                effects.push(request(
                    state,
                    CommandKind::RenameFolder { id, name },
                    Pending::Folder,
                ));
            }
            if !undo_folders.is_empty() {
                state.undo.push(crate::app::state::UndoOp::FolderRename { folders: undo_folders });
            }
            effects
        }
        Deferred::DeleteFolder { ids, subfolders, also } => {
            let mut targets = if also { subfolders } else { Vec::new() };
            targets.extend(ids);
            let mut deleted = Vec::new();
            let mut effects = Vec::new();
            for id in targets {
                let Some(name) = state.cache.folder(&id).map(|folder| folder.name.clone()) else {
                    continue;
                };
                let items = state
                    .cache
                    .active_items()
                    .filter(|item| item.folder_id.as_ref() == Some(&id))
                    .filter_map(|item| item.id.clone())
                    .collect();
                deleted.push(crate::app::state::DeletedFolder { name, items });
                effects.push(request(state, CommandKind::DeleteFolder(id), Pending::Folder));
            }
            if !deleted.is_empty() {
                state.undo.push(crate::app::state::UndoOp::FolderDelete { folders: deleted });
            }
            effects
        }
        Deferred::EmptyTrash => {
            let ids: Vec<String> = state.cache.visible_ids_in_trash();
            let mut effects = Vec::new();
            for id in ids {
                effects.push(request(
                    state,
                    CommandKind::DeleteItem { id: id.clone(), permanent: true },
                    Pending::Delete { id, permanent: true },
                ));
            }
            state.toast(MessageLevel::Ok, "Emptying the trash…");
            effects
        }
        Deferred::TrashItems(ids) => trash_items(state, ids),
        Deferred::DeleteItems(ids) => {
            state.clear_selection();
            let mut effects = Vec::new();
            for id in ids {
                effects.push(request(
                    state,
                    CommandKind::DeleteItem { id: id.clone(), permanent: true },
                    Pending::Delete { id, permanent: true },
                ));
            }
            effects
        }
        Deferred::OpenUri(uri) => {
            state.message(MessageLevel::Info, format!("Opening {uri}"));
            vec![Effect::OpenUri(uri)]
        }
        Deferred::ConfirmGeneratePassword(id) => {
            let Some(item) = state.cache.item(&id).cloned() else { return Vec::new() };
            if reprompt_required(state, &item) {
                return reprompt_for(state, &item, Deferred::ConfirmGeneratePassword(id));
            }
            confirm_generate(state, &item)
        }
        Deferred::GeneratePassword(id) => {
            state.pending_generate_target = Some(id);
            vec![super::generate(state, true, false)]
        }
        Deferred::ConfirmUseGenerated(id) => {
            let Some(item) = state.cache.item(&id).cloned() else { return Vec::new() };
            if reprompt_required(state, &item) {
                return reprompt_for(state, &item, Deferred::ConfirmUseGenerated(id));
            }
            state.open(ConfirmState::new(
                "Use generated password",
                format!(
                    "Replace the password of “{}” with the generated one? The current password \
                     moves to its history; u undoes it.",
                    item.name
                ),
                "Replace",
                Deferred::UseGenerated(id),
            ));
            Vec::new()
        }
        Deferred::UseGenerated(id) => {
            let password = state.generator.output.clone();
            let Some(item) = state.cache.item(&id).cloned() else { return Vec::new() };
            if password.is_empty() {
                return Vec::new();
            }
            let mut updated = item.clone();
            if let Some(login) = &mut updated.login {
                login.password = Some(crate::bw::sensitive::Sensitive::new(password));
            }
            state.undo.push(crate::app::state::UndoOp::Edit {
                id: id.clone(),
                before: Box::new(item),
                after: Box::new(updated.clone()),
            });
            vec![request(
                state,
                CommandKind::PutItem { id: id.clone(), item: Box::new(updated) },
                Pending::Save { id: Some(id), close: false, editor: None },
            )]
        }
        // The item the dialog named, not whatever is selected by now.
        Deferred::RemovePasskey(id) => {
            let Some(item) = state.cache.item(&id).cloned() else { return Vec::new() };
            if reprompt_required(state, &item) {
                return reprompt_for(state, &item, Deferred::RemovePasskey(id));
            }
            let mut updated = item.clone();
            if let Some(login) = updated.login.as_mut()
                && let Some(credentials) = login.fido2_credentials.as_mut()
                && !credentials.is_empty()
            {
                credentials.remove(0);
            }
            state.undo.push(crate::app::state::UndoOp::Edit {
                id: id.clone(),
                before: Box::new(item),
                after: Box::new(updated.clone()),
            });
            state.toast(MessageLevel::Ok, "Removed the passkey.");
            vec![request(
                state,
                CommandKind::PutItem { id: id.clone(), item: Box::new(updated) },
                Pending::Save { id: Some(id), close: false, editor: None },
            )]
        }
    }
}

/// Does what a one-line prompt was opened for with the text typed into it.
pub(crate) fn accept_prompt(
    state: &mut AppState,
    on_accept: PromptAction,
    text: String,
) -> Vec<Effect> {
    match on_accept {
        PromptAction::None => Vec::new(),
        PromptAction::NewFolder => apply(state, Action::with(ActionId::FolderNew, &[&text])),
        PromptAction::RenameFolder => apply(state, Action::with(ActionId::FolderRename, &[&text])),
        PromptAction::FilterHelp => {
            state.help_filter = text;
            state.help_offset = 0;
            Vec::new()
        }
        PromptAction::FilterKeymap => {
            state.keymap_editor.filter = text;
            state.keymap_editor.index = 0;
            Vec::new()
        }
        PromptAction::Setting(key) => super::apply_setting(state, key, &text),
        PromptAction::ImportSshKey => {
            match std::fs::read_to_string(shellexpand_home(&text)) {
                Ok(pem) => match crate::util::ssh::parse_private_key(&pem) {
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
                        state.message(MessageLevel::Ok, "Imported the SSH key.");
                    }
                    Err(err) => state.toast_with(
                        MessageLevel::Error,
                        "Couldn't read the SSH key.",
                        err.to_string(),
                    ),
                },
                Err(err) => {
                    state.toast_with(
                        MessageLevel::Error,
                        "Couldn't read the file.",
                        err.to_string(),
                    );
                }
            }
            Vec::new()
        }
    }
}

/// The ids of the folders a sidebar path stands for: usually one, more when
/// Bitwarden holds several folders with the same name.
pub(crate) fn folder_ids(state: &AppState, path: &str) -> Vec<String> {
    state
        .cache
        .folders()
        .iter()
        .filter(|folder| folder.name == path)
        .filter_map(|folder| folder.id.clone())
        .collect()
}

/// Quits, unless changes are still on their way to Bitwarden: the list
/// already shows them as done, and leaving now would stop the CLI before it
/// sent them. `q` then waits for the answers (`update` quits once the last
/// one arrives); `:q!` leaves at once.
fn quit(state: &mut AppState, force: bool) -> Vec<Effect> {
    let writes = state.pending_writes();
    if !force && writes > 0 {
        state.quit_when_saved = true;
        state.message(
            MessageLevel::Info,
            format!(
                "Finishing {writes} change{} before quitting. :q! quits now.",
                if writes == 1 { "" } else { "s" }
            ),
        );
        return Vec::new();
    }
    state.quit = true;
    vec![Effect::Quit]
}

/// Opens a website from the vault. Web addresses open directly (a bare
/// `example.com` as `https://example.com`); any other scheme (`file:`,
/// `androidapp:`, a registered handler such as `vscode:`) asks first, because
/// the item may come from a shared collection and the link starts whatever
/// program handles that scheme.
pub(crate) fn open_uri(state: &mut AppState, uri: &str) -> Vec<Effect> {
    let trimmed = uri.trim();
    let scheme = trimmed.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase());
    let target = match scheme.as_deref() {
        Some("http" | "https") => trimmed.to_owned(),
        // No scheme at all: Bitwarden often stores just the host.
        None if !trimmed.contains(':') && !trimmed.contains(char::is_whitespace) => {
            format!("https://{trimmed}")
        }
        _ => {
            state.open(ConfirmState::new(
                "Open a link that is not a website?",
                format!(
                    "“{trimmed}” is not a web address; it opens whatever program handles it. \
                     Open it?"
                ),
                "Open",
                Deferred::OpenUri(trimmed.to_owned()),
            ));
            return Vec::new();
        }
    };
    state.message(MessageLevel::Info, format!("Opening {target}"));
    vec![Effect::OpenUri(target)]
}
