//! The action dispatcher: every key, command, picker and menu entry ends here.

use super::{
    Action, AppState, BTreeSet, CmdlineState, CommandKind, ConfirmState, DetailState, EditorTarget,
    Effect, Filter, HistoryState, InfoState, ItemType, MessageLevel, Pending, PickerEntry,
    PickerKind, PickerState, PromptState, QuickOpenState, RESERVED_FOLDER_MESSAGE, Screen,
    SortOrder, SshKey, SuspendTask, Tab, cancel, close_editor, close_notes, cmdline_action,
    confirm_generate, confirm_yes, copy_action, copy_value, current_folder_id, current_folder_path,
    detail_field_at, detail_value, doctor_lines, editable, editor_action, favorite_action,
    fold_action, generator_action, health_action, keymap_action, login_action, move_items,
    navigation, open_editor, open_export, open_folder_picker, open_import, open_keymap_editor,
    open_new_editor, open_theme_picker, picker_action, quick_open_action, rail_active,
    rebuild_rows, refresh_quick_open, reprompt, reprompt_for, reprompt_required, request,
    reserved_folder_name, reveal_action, reveal_target, search_action, select_item_by_id,
    selected_name, selected_reprompt_required, settings_action, shellexpand_home, sidebar_entries,
    switch_tab, trash_items, undo,
};

pub(crate) fn apply(state: &mut AppState, action: Action) -> Vec<Effect> {
    let id = action.id.as_str();
    let args = action.args.clone();
    let count = action.count;
    match id {
        "noop" => Vec::new(),
        "app.quit" | "app.quit_force"
            if state.screen == Screen::Editor
                && state.editor.is_some()
                && args.first().map(String::as_str) != Some("all") =>
        {
            // In the editor `:q` closes the item (asking about unsaved
            // changes) and `:q!` discards them; neither quits lazybw.
            if id == "app.quit_force" {
                return apply(state, Action::new("editor.discard"));
            }
            close_notes(state);
            close_editor(state);
            Vec::new()
        }
        "app.quit" | "app.quit_force" => {
            if id == "app.quit" && state.editor.as_ref().is_some_and(|e| e.dirty) {
                state.open(ConfirmState {
                    title: "Unsaved changes".to_owned(),
                    message: "The item has unsaved changes. Quit and discard them?".to_owned(),
                    confirm_label: "Quit".to_owned(),
                    danger: true,
                    requires_word: None,
                    input: String::new(),
                    also_subfolders: false,
                    subfolder_count: 0,
                    on_deny: None,
                    on_confirm: Action::new("app.quit_force"),
                });
                return Vec::new();
            }
            state.quit = true;
            vec![Effect::Quit]
        }
        "app.cancel" => cancel(state),
        "app.help" => {
            if state.screen != Screen::Help {
                state.help_chain = state.contexts();
            }
            state.screen = Screen::Help;
            state.help_offset = 0;
            state.help_filter.clear();
            Vec::new()
        }
        "help.filter" => {
            state.help_filter = args.first().cloned().unwrap_or_default();
            state.help_offset = 0;
            Vec::new()
        }
        "app.redraw" => {
            state.full_redraw = true;
            state.dirty = true;
            Vec::new()
        }
        "app.messages" => {
            state.screen = Screen::Messages;
            state.messages_offset = 0;
            Vec::new()
        }
        "app.settings" => {
            state.screen = Screen::Settings;
            state.settings.index = 0;
            Vec::new()
        }
        "app.keymap" => {
            open_keymap_editor(state);
            Vec::new()
        }
        "app.doctor" => {
            let lines = doctor_lines(state);
            state.open(InfoState { item_id: String::new(), lines });
            Vec::new()
        }
        "cmdline.open" => {
            state.open(CmdlineState {
                input: String::new(),
                cursor: 0,
                completions: Vec::new(),
                completion_index: 0,
                show_completions: false,
            });
            Vec::new()
        }
        "quick_open" => {
            state.open(QuickOpenState::default());
            refresh_quick_open(state);
            Vec::new()
        }
        "focus.left" => {
            state.focus = state.focus.left();
            state.detail.row = 0;
            Vec::new()
        }
        "focus.right" => {
            state.focus = state.focus.right();
            state.detail.row = 0;
            Vec::new()
        }
        "focus.next" => {
            state.focus = match state.focus {
                crate::app::state::Focus::Sidebar => crate::app::state::Focus::List,
                crate::app::state::Focus::List => crate::app::state::Focus::Detail,
                crate::app::state::Focus::Detail => crate::app::state::Focus::Sidebar,
            };
            Vec::new()
        }
        "focus.prev" => {
            state.focus = match state.focus {
                crate::app::state::Focus::Sidebar => crate::app::state::Focus::Detail,
                crate::app::state::Focus::List => crate::app::state::Focus::Sidebar,
                crate::app::state::Focus::Detail => crate::app::state::Focus::List,
            };
            Vec::new()
        }
        "layout.toggle_sidebar" => {
            state.sidebar_visible = !state.sidebar_visible;
            Vec::new()
        }
        "list.toggle_density" => {
            state.config.ui.list_density = state.config.ui.list_density.toggled();
            let value = state.config.ui.list_density.label();
            state.message(MessageLevel::Info, format!("List density: {value}"));
            vec![Effect::SaveConfig(vec![(
                vec!["ui".into(), "list_density".into()],
                toml_edit::Value::from(value.to_owned()),
            )])]
        }
        "tab.next" | "tab.prev" | "tab.goto" => {
            let tab = match id {
                "tab.goto" => match args.first().map(String::as_str) {
                    Some("1") => Tab::Vault,
                    Some("2") => Tab::Generator,
                    Some("3") => Tab::Health,
                    _ => state.tab,
                },
                "tab.next" => match state.tab {
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
        "jump.back" => {
            if let Some(entry) = state.jump.back().map(|e| e.item_id.clone()) {
                select_item_by_id(state, &entry);
            }
            Vec::new()
        }
        "jump.forward" => {
            if let Some(entry) = state.jump.forward().map(|e| e.item_id.clone()) {
                select_item_by_id(state, &entry);
            }
            Vec::new()
        }
        "theme.pick" => {
            open_theme_picker(state);
            Vec::new()
        }
        "theme.set" => {
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
        "vault.sync" => {
            let force = args.first().is_some_and(|a| a == "force");
            vec![request(state, CommandKind::Sync { force }, Pending::Sync)]
        }
        "vault.lock" => {
            state.drop_secrets();
            state.message(MessageLevel::Info, "Vault locked.");
            vec![request(state, CommandKind::Lock, Pending::Lock)]
        }
        "vault.logout" => {
            state.open(ConfirmState {
                title: "Log out".to_owned(),
                message:
                    "Log out of Bitwarden? lazybw will stop the CLI and return to the login screen."
                        .to_owned(),
                confirm_label: "Log out".to_owned(),
                danger: true,
                requires_word: None,
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                on_confirm: Action::new("vault.logout.confirm"),
            });
            Vec::new()
        }
        "vault.logout.confirm" => {
            state.cache.clear();
            vec![request(state, CommandKind::Logout, Pending::Logout)]
        }
        "vault.export" => open_export(state, &args),
        "vault.import" => open_import(state, &args),
        "generator.open" => switch_tab(state, Tab::Generator),
        "health.open" => switch_tab(state, Tab::Health),
        "sort.set" => {
            if let Some(order) = args.first().and_then(|arg| SortOrder::parse(arg)) {
                state.config.vault.sort = order;
                state.sort_reversed = false;
                state.message(MessageLevel::Info, format!("Sorted by {}", order.label()));
            }
            Vec::new()
        }
        "sort.reverse" => {
            state.sort_reversed = !state.sort_reversed;
            state.message(
                MessageLevel::Info,
                if state.sort_reversed { "Sort reversed." } else { "Sort order restored." },
            );
            Vec::new()
        }
        "filter.apply" => {
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
                    return apply(state, Action::new("folder.jump"));
                }
                if let Some(entry) = entries.get(state.sidebar_row)
                    && let Some(filter) = &entry.filter
                {
                    state.filter = filter.clone();
                    state.focus = crate::app::state::Focus::List;
                    state.list_row = 0;
                    state.list_offset = 0;
                    state.search = crate::app::state::SearchState::default();
                }
            }
            Vec::new()
        }
        "fold.expand_or_apply" => {
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
            apply(state, Action::new("filter.apply"))
        }
        "fold.collapse_or_parent" => {
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
        "fold.toggle" | "fold.open" | "fold.close" | "fold.open_all" | "fold.close_all" => {
            fold_action(state, id)
        }
        "folder.new" => {
            if let Some(name) = args.first() {
                if reserved_folder_name(name) {
                    state.toast(MessageLevel::Error, RESERVED_FOLDER_MESSAGE);
                    return Vec::new();
                }
                vec![request(state, CommandKind::CreateFolder(name.clone()), Pending::Folder)]
            } else {
                state.open(PromptState {
                    title: "New folder".to_owned(),
                    input: crate::input::line::Line::new(""),
                    on_accept: Action::new("folder.new"),
                    replace_first_arg: true,
                    error: None,
                });
                Vec::new()
            }
        }
        "folder.rename" => {
            if let Some(new_name) = args.first() {
                if reserved_folder_name(new_name) {
                    state.toast(MessageLevel::Error, RESERVED_FOLDER_MESSAGE);
                    return Vec::new();
                }
                let old = current_folder_path(state);
                if old.is_empty() {
                    state.toast(MessageLevel::Error, "No folder selected.");
                    return Vec::new();
                }
                if !state.cache.folders().iter().any(|folder| folder.name == old) {
                    state.toast(MessageLevel::Error, "No such folder.");
                    return Vec::new();
                }
                let count = crate::vault::folders::subfolder_count(state.cache.folders(), &old);
                state.open(ConfirmState {
                    title: "Rename folder".to_owned(),
                    message: format!("Rename “{old}” to “{new_name}”?"),
                    confirm_label: "Rename".to_owned(),
                    danger: false,
                    requires_word: None,
                    input: String::new(),
                    also_subfolders: true,
                    subfolder_count: count,
                    on_deny: None,
                    on_confirm: Action {
                        id: "folder.rename.confirm".to_owned(),
                        args: vec![old, new_name.clone()],
                        count: None,
                    },
                });
                Vec::new()
            } else {
                let current = current_folder_path(state);
                state.open(PromptState {
                    title: "Rename folder".to_owned(),
                    input: crate::input::line::Line::new(&current),
                    on_accept: Action::new("folder.rename"),
                    replace_first_arg: true,
                    error: None,
                });
                Vec::new()
            }
        }
        "folder.delete" => {
            let selected = args
                .first()
                .cloned()
                .or_else(|| Some(current_folder_path(state)))
                .filter(|name| !name.is_empty());
            if let Some(name) = selected {
                if let Some(_folder) = state.cache.folders().iter().find(|f| f.name == name) {
                    let count = state.cache.count(&Filter::Folder(name.clone()));
                    let subfolders =
                        crate::vault::folders::subfolder_count(state.cache.folders(), &name);
                    let mut message = if count == 1 {
                        format!("Delete folder “{name}”? Its 1 item moves to No folder.")
                    } else {
                        format!("Delete folder “{name}”? Its {count} items move to No folder.")
                    };
                    if subfolders > 0 {
                        message.push_str(&format!(
                            " {subfolders} subfolder(s) will keep their names but their parent disappears."
                        ));
                    }
                    state.open(ConfirmState {
                        title: "Delete folder".to_owned(),
                        message,
                        confirm_label: "Delete".to_owned(),
                        danger: true,
                        requires_word: None,
                        input: String::new(),
                        also_subfolders: false,
                        subfolder_count: subfolders,
                        on_deny: None,
                        on_confirm: Action::with("folder.delete.confirm", &[&name]),
                    });
                }
                Vec::new()
            } else {
                Vec::new()
            }
        }
        "folder.rename.confirm" => {
            let (Some(old), Some(new_name)) = (args.first(), args.get(1)) else {
                return Vec::new();
            };
            let also = args.last().is_some_and(|arg| arg == "also");
            let mut effects = Vec::new();
            let mut undo_folders = Vec::new();
            if also {
                let subfolders =
                    crate::vault::folders::renamed_subfolders(state.cache.folders(), old, new_name);
                for (id, name) in &subfolders {
                    if let Some(folder) = state.cache.folder(id) {
                        undo_folders.push((folder.name.clone(), name.clone()));
                    }
                    effects.push(request(
                        state,
                        CommandKind::RenameFolder { id: id.clone(), name: name.clone() },
                        Pending::Folder,
                    ));
                }
            }
            if let Some(folder) = state.cache.folders().iter().find(|f| f.name == *old)
                && let Some(id) = folder.id.clone()
            {
                undo_folders.push((old.clone(), new_name.clone()));
                effects.push(request(
                    state,
                    CommandKind::RenameFolder { id, name: new_name.clone() },
                    Pending::Folder,
                ));
            }
            if !undo_folders.is_empty() {
                state.undo.push(crate::app::state::UndoOp::FolderRename { folders: undo_folders });
            }
            effects
        }
        "folder.delete.confirm" => {
            let Some(name) = args.first() else { return Vec::new() };
            let also = args.last().is_some_and(|arg| arg == "also");
            let mut deleted = Vec::new();
            let mut targets: Vec<&crate::bw::models::Folder> = state
                .cache
                .folders()
                .iter()
                .filter(|folder| also && folder.name.starts_with(&format!("{name}/")))
                .collect();
            targets.extend(state.cache.folders().iter().filter(|folder| folder.name == *name));
            for folder in targets {
                let items = folder
                    .id
                    .as_ref()
                    .map(|id| {
                        state
                            .cache
                            .active_items()
                            .filter(|item| item.folder_id.as_ref() == Some(id))
                            .filter_map(|item| item.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                deleted.push(crate::app::state::DeletedFolder { name: folder.name.clone(), items });
            }
            if !deleted.is_empty() {
                state.undo.push(crate::app::state::UndoOp::FolderDelete { folders: deleted });
            }
            let mut effects = Vec::new();
            if also {
                let prefix = format!("{name}/");
                let descendants: Vec<String> = state
                    .cache
                    .folders()
                    .iter()
                    .filter(|folder| folder.name.starts_with(&prefix))
                    .filter_map(|folder| folder.id.clone())
                    .collect();
                for id in descendants {
                    effects.push(request(state, CommandKind::DeleteFolder(id), Pending::Folder));
                }
            }
            if let Some(folder) = state.cache.folders().iter().find(|f| f.name == *name)
                && let Some(folder_id) = folder.id.clone()
            {
                effects.push(request(state, CommandKind::DeleteFolder(folder_id), Pending::Folder));
            }
            effects
        }
        "folder.jump" => {
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
            entries.extend(state.cache.folders().iter().map(|f| PickerEntry {
                label: f.name.clone(),
                note: note(&f.name),
                filter: f.name.clone(),
                value: f.name.clone(),
                accelerator: None,
            }));
            state.open(PickerState::new("Go to folder", PickerKind::FolderJump, entries));
            Vec::new()
        }
        "trash.empty" => {
            let count = state.cache.count(&Filter::Trash);
            if count == 0 {
                state.message(MessageLevel::Info, "The trash is empty.");
                return Vec::new();
            }
            state.open(ConfirmState {
                title: "Empty trash".to_owned(),
                message: format!(
                    "{} will be deleted for good. This cannot be undone. Type empty to confirm.",
                    crate::util::text::items(count)
                ),
                confirm_label: "Empty trash".to_owned(),
                danger: true,
                requires_word: Some("empty".to_owned()),
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                on_confirm: Action::new("trash.empty.confirm"),
            });
            Vec::new()
        }
        "trash.empty.confirm" => {
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
        "item.open" => {
            state.focus = crate::app::state::Focus::Detail;
            state.detail = DetailState::default();
            if let Some(id) = state.selected_id() {
                state.jump.push(id);
            }
            Vec::new()
        }
        "item.copy" => copy_action(state, &args),
        "item.copy_field" => {
            let field = detail_field_at(state, state.detail.row);
            let Some(item) = state.selected_item().cloned() else { return Vec::new() };
            if let Some((key, label, secret)) = field {
                if secret && reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                let value = detail_value(state, &item, &key);
                copy_value(state, &value, &label.to_lowercase(), &item.name)
            } else {
                Vec::new()
            }
        }
        "item.reveal_toggle" | "item.reveal_all" | "item.hide_all" => {
            if state.screen == Screen::Editor {
                if let Some(editor) = &mut state.editor {
                    editor.revealed = !editor.revealed;
                }
                Vec::new()
            } else {
                let reveals = match id {
                    "item.reveal_all" => true,
                    "item.hide_all" => false,
                    _ => reveal_target(state)
                        .is_some_and(|key| !state.detail.revealed.contains(&key)),
                };
                if reveals && selected_reprompt_required(state) {
                    return reprompt(state, action);
                }
                reveal_action(state, id)
            }
        }
        "item.new" => {
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
        "item.edit" => {
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
        "item.edit_field" | "item.change_field" => {
            if let Some(item) = state.selected_item().cloned() {
                if !editable(state, &item) {
                    return Vec::new();
                }
                if reprompt_required(state, &item) {
                    return reprompt(state, action);
                }
                open_editor(state, item, false);
                if let Some(editor) = &mut state.editor {
                    editor.insert = true;
                    editor.row = state.detail.row.min(editor.rows.len().saturating_sub(1));
                }
            }
            Vec::new()
        }
        "item.edit_json" => {
            if let Some(item) = state.selected_item().cloned() {
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
        "item.trash" => {
            let ids = state.target_ids(count);
            if ids.is_empty() {
                return Vec::new();
            }
            if state.config.vault.confirm_trash {
                state.open(ConfirmState {
                    title: "Move to trash".to_owned(),
                    message: format!("Move {} item(s) to Trash?", ids.len()),
                    confirm_label: "Move to trash".to_owned(),
                    danger: false,
                    requires_word: None,
                    input: String::new(),
                    also_subfolders: false,
                    subfolder_count: 0,
                    on_deny: None,
                    // The dialog acts on the items it counted, whatever the
                    // selection is by the time it is answered.
                    on_confirm: Action {
                        id: "item.trash.confirm".to_owned(),
                        args: ids,
                        count: None,
                    },
                });
                return Vec::new();
            }
            trash_items(state, ids)
        }
        "item.trash.confirm" => trash_items(state, args),
        "item.delete" => {
            if state.screen == Screen::Vault
                && state.focus == crate::app::state::Focus::Detail
                && detail_field_at(state, state.detail.row)
                    .is_some_and(|(key, _, _)| key == "passkey")
            {
                let name = selected_name(state);
                state.open(ConfirmState {
                    title: "Remove passkey".to_owned(),
                    message: format!(
                        "Remove the passkey from “{name}”? You can add it again from the Bitwarden web vault."
                    ),
                    confirm_label: "Remove".to_owned(),
                    danger: true,
                    requires_word: None,
                    input: String::new(),
                    also_subfolders: false,
                    subfolder_count: 0,
                    on_deny: None,
                    // Straight to the removal: `item.remove_passkey` would
                    // ask the same question a second time.
                    on_confirm: Action::with(
                        "item.remove_passkey.confirm",
                        &[&state.selected_id().unwrap_or_default()],
                    ),
                });
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
            state.open(ConfirmState {
                title: "Delete permanently?".to_owned(),
                message,
                confirm_label: "Delete".to_owned(),
                danger: true,
                requires_word: None,
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                // The dialog deletes the items it named and counted, whatever
                // the selection is by the time it is answered.
                on_confirm: Action { id: "item.delete.confirm".to_owned(), args: ids, count: None },
            });
            Vec::new()
        }
        "item.delete.confirm" => {
            let ids = args;
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
        "item.restore" => {
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
        "item.favorite" => favorite_action(state, count),
        "item.move" => {
            if let Some(folder) = args.first() {
                move_items(state, folder, count)
            } else {
                open_folder_picker(state);
                Vec::new()
            }
        }
        "item.yank" => {
            if let Some(item) = state.selected_item().cloned() {
                state.yanked_item = Some(item);
                state.message(MessageLevel::Info, "Yanked item. Press p to clone it here.");
            }
            Vec::new()
        }
        "item.paste_clone" => {
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
        "item.open_uri" => {
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
        "item.open_uri.run" => match args.first() {
            Some(uri) => {
                state.message(MessageLevel::Info, format!("Opening {uri}"));
                vec![Effect::OpenUri(uri.clone())]
            }
            None => Vec::new(),
        },
        "item.history" => {
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
        "item.info" => {
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
        "item.generate_password" => {
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
                    Action::with(
                        "item.generate_password.confirm",
                        &[item.id.as_deref().unwrap_or_default()],
                    ),
                );
            }
            confirm_generate(state, &item)
        }
        "item.generate_password.confirm" => {
            let Some(item) = args.first().and_then(|id| state.cache.item(id).cloned()) else {
                return Vec::new();
            };
            if reprompt_required(state, &item) {
                return reprompt_for(state, &item, action);
            }
            confirm_generate(state, &item)
        }
        "generator.use.confirm" => {
            let Some(item) = args.first().and_then(|id| state.cache.item(id).cloned()) else {
                return Vec::new();
            };
            if reprompt_required(state, &item) {
                return reprompt_for(state, &item, action);
            }
            let id = item.id.clone().unwrap_or_default();
            state.open(ConfirmState {
                title: "Use generated password".to_owned(),
                message: format!(
                    "Replace the password of “{}” with the generated one? The current password moves to its history; u undoes it.",
                    item.name
                ),
                confirm_label: "Replace".to_owned(),
                danger: false,
                requires_word: None,
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                on_confirm: Action::with("generator.use.run", &[&id]),
            });
            Vec::new()
        }
        "generator.use.run" => {
            let password = state.generator.output.clone();
            let Some(item) = args.first().and_then(|id| state.cache.item(id).cloned()) else {
                return Vec::new();
            };
            let Some(id) = item.id.clone() else { return Vec::new() };
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
        "item.generate_password.run" => {
            let Some(id) = args.first().cloned() else { return Vec::new() };
            state.pending_generate_target = Some(id);
            vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )]
        }
        "item.remove_passkey" => {
            if state.screen == Screen::Vault
                && state.focus == crate::app::state::Focus::Detail
                && detail_field_at(state, state.detail.row)
                    .is_some_and(|(key, _, _)| key == "passkey")
            {
                let name = selected_name(state);
                state.open(ConfirmState {
                    title: "Remove passkey".to_owned(),
                    message: format!(
                        "Remove the passkey from “{name}”? You can add it again from the Bitwarden web vault."
                    ),
                    confirm_label: "Remove".to_owned(),
                    danger: true,
                    requires_word: None,
                    input: String::new(),
                    also_subfolders: false,
                    subfolder_count: 0,
                    on_deny: None,
                    on_confirm: Action::with(
                        "item.remove_passkey.confirm",
                        &[&state.selected_id().unwrap_or_default()],
                    ),
                });
            } else {
                state.message(
                    MessageLevel::Info,
                    "Only passkeys can be removed from the detail pane; trash items from the list.",
                );
            }
            Vec::new()
        }
        "item.remove_passkey.confirm" => {
            // The item the dialog named, not whatever is selected by now.
            if let Some(item) = args.first().and_then(|id| state.cache.item(id).cloned())
                && let Some(id) = item.id.clone()
            {
                if reprompt_required(state, &item) {
                    return reprompt_for(state, &item, action);
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
                return vec![request(
                    state,
                    CommandKind::PutItem { id: id.clone(), item: Box::new(updated) },
                    Pending::Save { id: Some(id.clone()), close: false, editor: None },
                )];
            }
            Vec::new()
        }
        "item.goto_folder" => {
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
        "undo" => undo(state, true),
        "redo" => undo(state, false),
        "mark.toggle" => {
            if let Some(id) = state.selected_id()
                && !state.marks.remove(&id)
            {
                state.marks.insert(id);
            }
            Vec::new()
        }
        "visual.start" => {
            state.visual_anchor = Some(state.list_row);
            Vec::new()
        }
        "visual.exit" => {
            state.visual_anchor = None;
            Vec::new()
        }
        "editor.save"
        | "editor.save_close"
        | "editor.discard"
        | "editor.close"
        | "editor.column_left"
        | "editor.column_right"
        | "editor.reload"
        | "editor.normal"
        | "editor.insert"
        | "editor.append"
        | "editor.insert_start"
        | "editor.append_end"
        | "editor.change_field"
        | "editor.next_field"
        | "editor.prev_field"
        | "editor.first_field"
        | "editor.last_field"
        | "editor.row_add_below"
        | "editor.row_add_above"
        | "editor.row_delete"
        | "editor.cycle_next"
        | "editor.cycle_prev"
        | "editor.activate"
        | "editor.generate"
        | "editor.external"
        | "editor.undo"
        | "editor.redo" => editor_action(state, action),
        "generator.regenerate"
        | "generator.toggle_mode"
        | "generator.increase"
        | "generator.decrease"
        | "generator.toggle_option"
        | "generator.copy"
        | "generator.use" => generator_action(state, action),
        "health.rerun" | "health.next_category" | "health.prev_category" | "health.open_item" => {
            health_action(state, action)
        }
        "settings.activate" | "settings.next_value" | "settings.prev_value" | "settings.reset" => {
            settings_action(state, action)
        }
        "keymap.rebind"
        | "keymap.add"
        | "keymap.remove"
        | "keymap.reset"
        | "keymap.reset_all"
        | "keymap.save"
        | "keymap.next_context"
        | "keymap.prev_context" => keymap_action(state, action),
        "editor.ssh_import" => {
            if let Some(path) = args.first() {
                match std::fs::read_to_string(shellexpand_home(path)) {
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
                    Err(err) => state.toast_with(
                        MessageLevel::Error,
                        "Couldn't read the file.",
                        err.to_string(),
                    ),
                }
            }
            Vec::new()
        }
        "keymap.filter" => {
            state.open(PromptState {
                title: "Filter keybindings".to_owned(),
                input: crate::input::line::Line::new(&state.keymap_editor.filter),
                on_accept: Action::new("keymap.filter.apply"),
                replace_first_arg: true,
                error: None,
            });
            Vec::new()
        }
        "keymap.filter.apply" => {
            state.keymap_editor.filter = args.first().cloned().unwrap_or_default();
            state.keymap_editor.index = 0;
            Vec::new()
        }
        "help.search" => {
            if state.screen != Screen::Help {
                return apply(state, Action::new("app.help"));
            }
            state.open(PromptState {
                title: "Search help".to_owned(),
                input: crate::input::line::Line::new(&state.help_filter),
                on_accept: Action::new("help.filter"),
                replace_first_arg: true,
                error: None,
            });
            Vec::new()
        }
        "unlock.submit" => {
            if state.unlock.password.is_empty() || state.unlocking() {
                return Vec::new();
            }
            state.unlock.error = None;
            let password = state.unlock.password.take_secret();
            vec![request(state, CommandKind::Unlock(password), Pending::Unlock)]
        }
        "unlock.logout" => {
            state.open(ConfirmState {
                title: "Log out".to_owned(),
                message: "Log out of Bitwarden?".to_owned(),
                confirm_label: "Log out".to_owned(),
                danger: true,
                requires_word: None,
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                on_confirm: Action::new("vault.logout.confirm"),
            });
            Vec::new()
        }
        "confirm.yes" => confirm_yes(state),
        "confirm.no" => {
            state.take_confirm();
            Vec::new()
        }
        "login.next" | "login.prev" | "login.submit" | "login.back" => login_action(state, action),
        "nav.down" | "nav.up" | "nav.top" | "nav.bottom" | "nav.half_down" | "nav.half_up"
        | "nav.page_down" | "nav.page_up" | "nav.screen_top" | "nav.screen_middle"
        | "nav.screen_bottom" | "view.center" | "view.top" | "view.bottom" => {
            navigation(state, &action)
        }
        "search.start" | "search.next" | "search.prev" | "search.clear" => {
            search_action(state, &action)
        }
        "picker.next" | "picker.prev" | "picker.accept" => picker_action(state, &action),
        "quick_open.next"
        | "quick_open.prev"
        | "quick_open.accept"
        | "quick_open.copy_password"
        | "quick_open.copy_username"
        | "quick_open.copy_totp"
        | "quick_open.edit" => quick_open_action(state, &action),
        "cmdline.execute"
        | "cmdline.complete_next"
        | "cmdline.complete_prev"
        | "cmdline.history_prev"
        | "cmdline.history_next" => cmdline_action(state, &action),
        _ => Vec::new(),
    }
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
            state.open(ConfirmState {
                title: "Open a link that is not a website?".to_owned(),
                message: format!(
                    "“{trimmed}” is not a web address; it opens whatever program handles it. \
                     Open it?"
                ),
                confirm_label: "Open".to_owned(),
                danger: false,
                requires_word: None,
                input: String::new(),
                also_subfolders: false,
                subfolder_count: 0,
                on_deny: None,
                on_confirm: Action::with("item.open_uri.run", &[trimmed]),
            });
            return Vec::new();
        }
    };
    state.message(MessageLevel::Info, format!("Opening {target}"));
    vec![Effect::OpenUri(target)]
}
