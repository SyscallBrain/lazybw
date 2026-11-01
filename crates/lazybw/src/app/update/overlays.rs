//! Keys for pickers, dialogs, prompts, quick open and the login/unlock
//! screens.

use super::{
    Action, AppState, CommandKind, Effect, ExportFormat, FieldKind, Filter, FormKey, ItemType, Key,
    MessageLevel, Pending, PickerKind, PromptState, Query, Screen, SecretString, SessionPhase,
    SuspendTask, UriMatch, apply, cancel, copy_value, ctrl_np_as_arrows, field_label, field_value,
    move_items, next_id, open_new_editor, overlay_binding, rebuild_rows, reprompt_for,
    reprompt_required, request, select_item_by_id, sidebar_entries, sync_editor,
};

pub(crate) fn picker_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match overlay_binding(state, "picker", key) {
        Some(Some(action)) => return picker_action(state, &action),
        Some(None) => {
            state.picker = None;
            return Vec::new();
        }
        None => {}
    }
    match key.code {
        Code::Esc => {
            state.picker = None;
            return Vec::new();
        }
        Code::Up => return picker_action(state, &Action::new("picker.prev")),
        Code::Down => return picker_action(state, &Action::new("picker.next")),
        Code::Enter => return picker_action(state, &Action::new("picker.accept")),
        Code::Backspace => {
            if let Some(picker) = &mut state.picker {
                if !picker.filter.is_empty() {
                    picker.filter.pop();
                } else {
                    state.picker = None;
                    return Vec::new();
                }
                picker.refilter();
            }
            return Vec::new();
        }
        Code::Char(c) if key.is_printable() => {
            if let Some(picker) = &mut state.picker {
                if let Some(entry) = picker.entries.iter().find(|e| e.accelerator == Some(c)) {
                    let value = entry.value.clone();
                    return picker_accept_value(state, &value);
                }
                picker.filter.push(c);
                picker.refilter();
            }
            return Vec::new();
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn picker_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    let Some(picker) = &mut state.picker else { return Vec::new() };
    match action.id.as_str() {
        "picker.next" => {
            if !picker.filtered.is_empty() {
                picker.index = (picker.index + 1) % picker.filtered.len();
            }
        }
        "picker.prev" => {
            if !picker.filtered.is_empty() {
                picker.index = (picker.index + picker.filtered.len() - 1) % picker.filtered.len();
            }
        }
        "picker.accept" => {
            if let Some(value) = picker.selected().map(|e| e.value.clone()) {
                return picker_accept_value(state, &value);
            }
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn picker_accept_value(state: &mut AppState, value: &str) -> Vec<Effect> {
    let Some(picker) = state.picker.take() else { return Vec::new() };
    state.dirty = true;
    match picker.kind {
        PickerKind::FolderJump => {
            let filter =
                if value.is_empty() { Filter::NoFolder } else { Filter::Folder(value.to_owned()) };
            if let Some(index) = crate::vault::filter::index_of(&sidebar_entries(state), &filter) {
                state.sidebar_row = index;
            }
            state.filter = filter;
            state.focus = crate::app::state::Focus::List;
            state.list_row = 0;
            state.list_offset = 0;
            state.search = crate::app::state::SearchState::default();
        }
        PickerKind::ItemType => {
            let kind = ItemType::parse(value).unwrap_or(ItemType::Login);
            open_new_editor(state, kind);
        }
        PickerKind::Folder => {
            if value == "__create__" {
                state.prompt = Some(PromptState {
                    title: "New folder".to_owned(),
                    input: crate::input::line::Line::new(""),
                    on_accept: Action::new("folder.new"),
                    replace_first_arg: true,
                    error: None,
                });
            } else if picker.from_editor {
                if let Some(editor) = &mut state.editor
                    && let Some(row) = editor.rows.get_mut(editor.row)
                    && row.key == FormKey::Folder
                {
                    row.value = value.to_owned();
                    editor.dirty = true;
                }
            } else {
                let count = state.dispatcher.count;
                return move_items(state, value, count);
            }
        }
        PickerKind::Theme => {
            return apply(state, Action::with("theme.set", &[value]));
        }
        PickerKind::ExportFormat => {
            if let Some(export) = &mut state.export {
                export.format = ExportFormat::parse(value).unwrap_or(ExportFormat::Json);
                export.step = 1;
            }
        }
        PickerKind::ImportFormat => {
            if let Some(import) = &mut state.import {
                import.format = value.to_owned();
                import.step = 1;
            }
        }
        PickerKind::FieldType => {
            if let Some(editor) = &mut state.editor
                && let Some(row) = editor.rows.get_mut(editor.row)
                && let FormKey::FieldType(index) = row.key
            {
                if let Some(fields) = editor.item.fields.as_mut()
                    && let Some(field) = fields.get_mut(index)
                {
                    field.kind = FieldKind::from_code(match value {
                        "hidden" => 1,
                        "boolean" => 2,
                        "linked" => 3,
                        _ => 0,
                    })
                    .code();
                }
                row.value = value.to_owned();
                editor.dirty = true;
            }
            rebuild_rows(state);
        }
        PickerKind::UriMatch => {
            if let Some(editor) = &mut state.editor
                && let Some(row) = editor.rows.get_mut(editor.row)
                && let FormKey::UriMatch(index) = row.key
            {
                if let Some(uris) = editor.item.login.as_mut().and_then(|l| l.uris.as_mut())
                    && let Some(uri) = uris.get_mut(index)
                {
                    let kind = match value {
                        "Base domain" => UriMatch::BaseDomain,
                        "Host" => UriMatch::Host,
                        "Starts with" => UriMatch::StartsWith,
                        "Exact" => UriMatch::Exact,
                        "Regular expression" => UriMatch::Regex,
                        "Never" => UriMatch::Never,
                        _ => UriMatch::Default,
                    };
                    uri.set_match_kind(kind);
                }
                row.value = value.to_owned();
                editor.dirty = true;
            }
            rebuild_rows(state);
        }
        PickerKind::IdentityTitle | PickerKind::CardBrand | PickerKind::ExpMonth => {
            if let Some(editor) = &mut state.editor
                && let Some(row) = editor.rows.get_mut(editor.row)
            {
                row.value = value.to_owned();
                editor.dirty = true;
            }
            sync_editor(state);
        }
    }
    Vec::new()
}

pub(crate) fn confirm_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match key.code {
        Code::Esc => {
            state.confirm = None;
            Vec::new()
        }
        Code::Char('y') | Code::Char('n')
            if state.confirm.as_ref().is_some_and(|c| c.requires_word.is_some()) =>
        {
            // Typing the required word instead of y/n.
            if let Some(confirm) = &mut state.confirm
                && let Code::Char(c) = key.code
            {
                confirm.input.push(c);
            }
            Vec::new()
        }
        Code::Char('y') | Code::Enter => confirm_yes(state),
        Code::Char('n') => {
            let deny = state.confirm.take().and_then(|confirm| confirm.on_deny);
            match deny {
                Some(action) => apply(state, action),
                None => Vec::new(),
            }
        }
        Code::Char('c') if state.confirm.as_ref().is_some_and(|c| c.on_deny.is_some()) => {
            state.confirm = None;
            Vec::new()
        }
        Code::Char(' ') | Code::Char('x')
            if state.confirm.as_ref().is_some_and(|confirm| confirm.subfolder_count > 0) =>
        {
            if let Some(confirm) = &mut state.confirm {
                confirm.also_subfolders = !confirm.also_subfolders;
            }
            Vec::new()
        }
        Code::Backspace => {
            if let Some(confirm) = &mut state.confirm {
                confirm.input.pop();
            }
            Vec::new()
        }
        Code::Char(c) if key.is_printable() => {
            if let Some(confirm) = &mut state.confirm {
                confirm.input.push(c);
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn confirm_yes(state: &mut AppState) -> Vec<Effect> {
    let Some(confirm) = state.confirm.take() else { return Vec::new() };
    let mut on_confirm = confirm.on_confirm.clone();
    if on_confirm.id.starts_with("folder.") && on_confirm.id.ends_with(".confirm") {
        on_confirm.args.push(if confirm.also_subfolders { "also" } else { "no" }.to_owned());
    }
    if let Some(word) = confirm.requires_word.clone()
        && confirm.input.trim() != word
    {
        state.message(MessageLevel::Error, format!("Type {word} to confirm."));
        state.confirm = Some(confirm);
        return Vec::new();
    }
    apply(state, on_confirm)
}

pub(crate) fn prompt_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let Some(prompt) = &mut state.prompt else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.prompt = None;
            return Vec::new();
        }
        Code::Enter => {
            let value = prompt.input.text();
            let mut action = prompt.on_accept.clone();
            if action.args.is_empty() {
                action.args.push(value);
            } else if prompt.replace_first_arg {
                action.args[0] = value;
            } else {
                let last = action.args.len() - 1;
                action.args[last] = value;
            }
            state.prompt = None;
            return apply(state, action);
        }
        Code::Backspace => prompt.input.backspace(),
        Code::Left => prompt.input.left(),
        Code::Right => prompt.input.right(),
        Code::Home => prompt.input.home(),
        Code::End => prompt.input.end(),
        Code::Delete => prompt.input.delete(),
        Code::Char(c) if key.is_printable() => prompt.input.insert_char(c),
        _ => {}
    }
    Vec::new()
}

pub(crate) fn reprompt_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let Some(reprompt) = &mut state.remprompt else { return Vec::new() };
    if is_ctrl_c(key) {
        state.remprompt = None;
        state.reprompt_action = None;
        return Vec::new();
    }
    match key.code {
        Code::Esc => {
            state.remprompt = None;
            state.reprompt_action = None;
            return Vec::new();
        }
        Code::Enter => {
            if reprompt.busy || reprompt.password.is_empty() {
                return Vec::new();
            }
            reprompt.busy = true;
            reprompt.error = None;
            let password = reprompt.password.take_secret();
            let action = state.reprompt_action.take();
            let Some(action) = action else {
                state.remprompt = None;
                return Vec::new();
            };
            return vec![request(
                state,
                CommandKind::Unlock(password),
                Pending::Reprompt(Box::new(action)),
            )];
        }
        Code::Backspace => {
            reprompt.password.pop();
        }
        Code::Char(c) if key.is_printable() => reprompt.password.push(c),
        _ => {}
    }
    Vec::new()
}

pub(crate) fn popup_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let key = ctrl_np_as_arrows(key);
    match key.code {
        Code::Esc | Code::Char('q') => {
            state.history = None;
            state.info = None;
            if state.screen == Screen::Messages {
                state.screen = state.tab.screen();
            }
        }
        Code::Char('j') | Code::Down => {
            if let Some(history) = &mut state.history
                && !history.entries.is_empty()
            {
                history.index = (history.index + 1) % history.entries.len();
            }
        }
        Code::Char('k') | Code::Up => {
            if let Some(history) = &mut state.history
                && !history.entries.is_empty()
            {
                history.index = (history.index + history.entries.len() - 1) % history.entries.len();
            }
        }
        Code::Char('z') => {
            state.showcmd = "z".to_owned();
            return Vec::new();
        }
        Code::Char('a') if state.showcmd == "z" => {
            state.showcmd.clear();
            if let Some(history) = &mut state.history
                && !history.revealed.remove(&history.index)
            {
                history.revealed.insert(history.index);
            }
        }
        Code::Char('y') => {
            if let Some(history) = &state.history
                && let Some(entry) = history.entries.get(history.index)
            {
                let value = entry.password.expose().to_owned();
                let id = history.item_id.clone();
                let name = state.cache.item(&id).map(|i| i.name.clone()).unwrap_or_default();
                return copy_value(state, &value, "old password", &name);
            }
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn quick_open_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match overlay_binding(state, "quick_open", key) {
        Some(Some(action)) => return quick_open_action(state, &action),
        Some(None) => {
            state.quick_open = None;
            return Vec::new();
        }
        None => {}
    }
    match key.code {
        Code::Esc => {
            state.quick_open = None;
            return Vec::new();
        }
        Code::Enter => return quick_open_action(state, &Action::new("quick_open.accept")),
        Code::Down | Code::Tab => return quick_open_action(state, &Action::new("quick_open.next")),
        Code::Up | Code::BackTab => {
            return quick_open_action(state, &Action::new("quick_open.prev"));
        }
        Code::Char('y') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new("quick_open.copy_password"));
        }
        Code::Char('u') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new("quick_open.copy_username"));
        }
        Code::Char('t') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new("quick_open.copy_totp"));
        }
        Code::Char('e') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new("quick_open.edit"));
        }
        Code::Backspace => {
            if let Some(quick) = &mut state.quick_open
                && !quick.input.is_empty()
            {
                quick.input.pop();
                quick.cursor = quick.input.chars().count();
                quick.index = 0;
                refresh_quick_open(state);
            }
            return Vec::new();
        }
        Code::Char(c) if key.is_printable() => {
            if let Some(quick) = &mut state.quick_open {
                quick.input.push(c);
                quick.cursor = quick.input.chars().count();
                quick.index = 0;
            }
            refresh_quick_open(state);
            return Vec::new();
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn refresh_quick_open(state: &mut AppState) {
    let Some(quick) = &mut state.quick_open else { return };
    let query = Query::parse(&quick.input);
    let mut matcher = crate::vault::query::QueryMatcher::new(&query);
    let mut hits: Vec<(std::sync::Arc<crate::vault::cache::Row>, u32)> = state
        .cache
        .rows_matching(&Filter::All)
        .into_iter()
        .filter_map(|row| {
            let item = state.cache.item(&row.id)?;
            let flags = state.health.report.as_ref().map(|r| r.flags(&row.id));
            let result =
                matcher.matches(item, &row.folder_path, flags, state.config.vault.search_notes)?;
            Some((row, result.score))
        })
        .collect();
    hits.sort_by(|(a, sa), (b, sb)| {
        sb.cmp(sa)
            .then_with(|| b.favorite.cmp(&a.favorite))
            .then_with(|| a.name_lower.cmp(&b.name_lower))
    });
    quick.results = hits.into_iter().map(|(row, _)| row.id.clone()).collect();
    quick.index = quick.index.min(quick.results.len().saturating_sub(1));
}

pub(crate) fn quick_open_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    let Some(quick) = &mut state.quick_open else { return Vec::new() };
    match action.id.as_str() {
        "quick_open.next" => {
            if !quick.results.is_empty() {
                quick.index = (quick.index + 1) % quick.results.len();
            }
        }
        "quick_open.prev" => {
            if !quick.results.is_empty() {
                quick.index = (quick.index + quick.results.len() - 1) % quick.results.len();
            }
        }
        "quick_open.accept" => {
            if let Some(id) = quick.results.get(quick.index).cloned() {
                state.quick_open = None;
                select_item_by_id(state, &id);
            }
        }
        "quick_open.edit" => {
            if let Some(id) = quick.results.get(quick.index).cloned() {
                state.quick_open = None;
                select_item_by_id(state, &id);
                return apply(state, Action::new("item.edit"));
            }
        }
        "quick_open.copy_password" | "quick_open.copy_username" | "quick_open.copy_totp" => {
            if let Some(id) = quick.results.get(quick.index).cloned()
                && let Some(item) = state.cache.item(&id).cloned()
            {
                let field = match action.id.as_str() {
                    "quick_open.copy_password" => "password",
                    "quick_open.copy_username" => "username",
                    _ => "totp",
                };
                state.quick_open = None;
                if field != "username" && reprompt_required(state, &item) {
                    // Select the item so the re-prompt and the copy target it.
                    select_item_by_id(state, &id);
                    return reprompt_for(state, &item, Action::with("item.copy", &[field]));
                }
                let value = field_value(&item, field, None, state);
                return copy_value(state, &value, field_label(field), &item.name);
            }
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn error_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    if is_ctrl_c(key) {
        return apply(state, Action::new("app.quit"));
    }
    match key.code {
        Code::Char('r') if state.error.as_ref().is_some_and(|e| e.retry) => {
            state.error = None;
            state.screen = Screen::Starting;
            state.session = SessionPhase::Starting;
            let id = next_id(state);
            state.pending.insert(id, Pending::Starting);
            vec![Effect::StartBackend { id }]
        }
        Code::Char('q') => {
            state.quit = true;
            vec![Effect::Quit]
        }
        Code::Char('e') => {
            if let Some(path) = state.error.as_ref().and_then(|e| e.open_file.clone()) {
                return vec![Effect::OpenFile(path)];
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn login_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    if is_ctrl_c(key) {
        return apply(state, Action::new("app.quit"));
    }
    if state.login.box_index == 1 && state.login.method_index == 2 {
        match key.code {
            Code::Esc => state.login.box_index = 0,
            Code::Tab | Code::BackTab | Code::Up | Code::Down => {
                state.login.form_index = (state.login.form_index + 1) % 2;
            }
            Code::Enter => return login_submit(state),
            Code::Backspace => {
                if state.login.form_index == 0 {
                    state.login.client_id.pop();
                } else {
                    state.login.client_secret.pop();
                }
            }
            Code::Char(c) if key.is_printable() => {
                if state.login.form_index == 0 {
                    state.login.client_id.push(c);
                } else {
                    state.login.client_secret.push(c);
                }
            }
            _ => {}
        }
        return Vec::new();
    }
    if state.login.editing_server {
        match key.code {
            Code::Esc => state.login.editing_server = false,
            Code::Enter => {
                state.login.editing_server = false;
                state.login.server_index = 2;
                let url = state.login.server_input.clone();
                if !url.is_empty() {
                    state.login.busy = true;
                    return vec![request(
                        state,
                        CommandKind::ConfigServer(url),
                        Pending::ConfigServer,
                    )];
                }
            }
            Code::Backspace => {
                state.login.server_input.pop();
            }
            Code::Char(c) if key.is_printable() => state.login.server_input.push(c),
            _ => {}
        }
        return Vec::new();
    }
    match key.code {
        Code::Esc => {
            if state.login.box_index == 1 {
                state.login.box_index = 0;
            }
        }
        Code::Tab => state.login.box_index = (state.login.box_index + 1) % 2,
        Code::BackTab => state.login.box_index = (state.login.box_index + 1) % 2,
        Code::Char('j') | Code::Down => {
            if state.login.box_index == 0 {
                state.login.server_index = (state.login.server_index + 1) % 3;
                if state.login.server_index == 2 && state.login.server_input.is_empty() {
                    state.login.editing_server = true;
                }
            } else {
                state.login.method_index = (state.login.method_index + 1) % 3;
            }
        }
        Code::Char('k') | Code::Up => {
            if state.login.box_index == 0 {
                state.login.server_index = (state.login.server_index + 2) % 3;
            } else {
                state.login.method_index = (state.login.method_index + 2) % 3;
            }
        }
        Code::Enter => return login_submit(state),
        _ => {}
    }
    Vec::new()
}

pub(crate) fn login_submit(state: &mut AppState) -> Vec<Effect> {
    if state.login.box_index == 0 {
        if state.login.server_index == 2 && state.login.server_input.is_empty() {
            state.login.editing_server = true;
            return Vec::new();
        }
        state.login.box_index = 1;
        return Vec::new();
    }
    state.login.busy = true;
    state.login.error = None;
    match state.login.method_index {
        0 => vec![Effect::Suspend(SuspendTask::Login { sso: false })],
        1 => vec![Effect::Suspend(SuspendTask::Login { sso: true })],
        _ => {
            if state.login.client_id.is_empty() || state.login.client_secret.is_empty() {
                state.login.busy = false;
                state.login.error = Some("Enter the client id and secret.".to_owned());
                return Vec::new();
            }
            let client_id = SecretString::from(state.login.client_id.clone());
            let client_secret = state.login.client_secret.to_secret();
            vec![request(
                state,
                CommandKind::LoginApiKey { client_id, client_secret },
                Pending::Login,
            )]
        }
    }
}

pub(crate) fn login_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action.id.as_str() {
        "login.submit" => login_submit(state),
        "login.next" => {
            if state.login.box_index == 0 {
                state.login.box_index = 1;
            } else {
                state.login.form_index = (state.login.form_index + 1) % 2;
            }
            Vec::new()
        }
        "login.prev" => {
            if state.login.box_index == 1 {
                state.login.box_index = 0;
            } else {
                state.login.form_index = (state.login.form_index + 1) % 2;
            }
            Vec::new()
        }
        "login.back" => Vec::new(),
        _ => Vec::new(),
    }
}

pub(crate) fn unlock_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    // Printable keys are the password; anything else (`<C-c>` quit,
    // `<C-o>` log out, `<CR>`) comes from the `unlock` keymap context.
    if !key.is_printable()
        && let Some(call) = state.keymap.action("unlock", &[key])
    {
        let action = Action { id: call.id.clone(), args: call.args.clone(), count: None };
        return apply(state, action);
    }
    match key.code {
        Code::Enter => apply(state, Action::new("unlock.submit")),
        Code::Esc => cancel(state),
        Code::Backspace => {
            state.unlock.password.pop();
            Vec::new()
        }
        Code::Char(c) if key.is_printable() => {
            state.unlock.password.push(c);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// `Ctrl-c`, which leaves the session screens (starting, error, login) and
/// cancels the master password prompt, like it does everywhere else.
fn is_ctrl_c(key: Key) -> bool {
    use crate::config::keymap::{Code, Mods};
    key.code == Code::Char('c') && key.mods == Mods::CTRL
}
