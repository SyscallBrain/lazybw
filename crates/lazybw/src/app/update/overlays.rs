//! Keys for pickers, dialogs, prompts, quick open and the login/unlock
//! screens.

use super::{
    Action, AppState, CommandKind, Effect, FieldKind, Filter, FormKey, ItemType, Key, LoginMethod,
    LoginServer, MessageLevel, Overlay, Pending, PickerKind, PromptState, Query, Screen,
    SecretString, SessionPhase, SuspendTask, UriMatch, apply, cancel, copy_value,
    ctrl_np_as_arrows, field_label, field_value, move_items, next_id, open_new_editor,
    overlay_binding, rebuild_rows, reprompt_for, reprompt_required, request, select_item_by_id,
    sidebar_entries, sync_editor,
};

pub(crate) fn picker_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match overlay_binding(state, "picker", key) {
        Some(Some(action)) => return picker_action(state, &action),
        Some(None) => {
            state.take_picker();
            return Vec::new();
        }
        None => {}
    }
    match key.code {
        Code::Esc => {
            state.take_picker();
            return Vec::new();
        }
        Code::Up => return picker_action(state, &Action::new("picker.prev")),
        Code::Down => return picker_action(state, &Action::new("picker.next")),
        Code::Enter => return picker_action(state, &Action::new("picker.accept")),
        Code::Backspace => {
            if let Some(Overlay::Picker(picker)) = &mut state.overlay {
                if !picker.filter.is_empty() {
                    picker.filter.pop();
                } else {
                    state.take_picker();
                    return Vec::new();
                }
                picker.refilter();
            }
            return Vec::new();
        }
        Code::Char(c) if key.is_printable() => {
            if let Some(Overlay::Picker(picker)) = &mut state.overlay {
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
    let Some(Overlay::Picker(picker)) = &mut state.overlay else { return Vec::new() };
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
    let Some(picker) = state.take_picker() else { return Vec::new() };
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
                state.open(PromptState {
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
            state.take_confirm();
            Vec::new()
        }
        Code::Char('y') | Code::Char('n')
            if state.confirm().is_some_and(|c| c.requires_word.is_some()) =>
        {
            // Typing the required word instead of y/n.
            if let Some(Overlay::Confirm(confirm)) = &mut state.overlay
                && let Code::Char(c) = key.code
            {
                confirm.input.push(c);
            }
            Vec::new()
        }
        Code::Char('y') | Code::Enter => confirm_yes(state),
        Code::Char('n') => {
            let deny = state.take_confirm().and_then(|confirm| confirm.on_deny);
            match deny {
                Some(action) => apply(state, action),
                None => Vec::new(),
            }
        }
        Code::Char('c') if state.confirm().is_some_and(|c| c.on_deny.is_some()) => {
            state.take_confirm();
            Vec::new()
        }
        Code::Char(' ') | Code::Char('x')
            if state.confirm().is_some_and(|confirm| confirm.subfolder_count > 0) =>
        {
            if let Some(Overlay::Confirm(confirm)) = &mut state.overlay {
                confirm.also_subfolders = !confirm.also_subfolders;
            }
            Vec::new()
        }
        Code::Backspace => {
            if let Some(Overlay::Confirm(confirm)) = &mut state.overlay {
                confirm.input.pop();
            }
            Vec::new()
        }
        Code::Char(c) if key.is_printable() => {
            if let Some(Overlay::Confirm(confirm)) = &mut state.overlay {
                confirm.input.push(c);
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn confirm_yes(state: &mut AppState) -> Vec<Effect> {
    let Some(confirm) = state.take_confirm() else { return Vec::new() };
    let mut on_confirm = confirm.on_confirm.clone();
    if on_confirm.id.starts_with("folder.") && on_confirm.id.ends_with(".confirm") {
        on_confirm.args.push(if confirm.also_subfolders { "also" } else { "no" }.to_owned());
    }
    if let Some(word) = confirm.requires_word.clone()
        && confirm.input.trim() != word
    {
        state.message(MessageLevel::Error, format!("Type {word} to confirm."));
        state.open(confirm);
        return Vec::new();
    }
    apply(state, on_confirm)
}

pub(crate) fn prompt_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    let Some(Overlay::Prompt(prompt)) = &mut state.overlay else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.take_prompt();
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
            state.take_prompt();
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
    if is_ctrl_c(key) || key.code == Code::Esc {
        // Closing the dialog also drops an answer still on its way: only
        // the request recorded in the open dialog may run the action.
        state.take_reprompt();
        return Vec::new();
    }
    let verifying = state.verifying_reprompt();
    let Some(Overlay::Reprompt(reprompt)) = &mut state.overlay else { return Vec::new() };
    match key.code {
        Code::Enter => {
            if verifying || reprompt.password.is_empty() {
                return Vec::new();
            }
            reprompt.error = None;
            let password = reprompt.password.take_secret();
            let id = state.next_request_id;
            reprompt.request = Some(id);
            return vec![request(state, CommandKind::Unlock(password), Pending::Reprompt)];
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
            state.overlay = None;
            if state.screen == Screen::Messages {
                state.screen = state.tab.screen();
            }
        }
        Code::Char('j') | Code::Down => {
            if let Some(Overlay::History(history)) = &mut state.overlay
                && !history.entries.is_empty()
            {
                history.index = (history.index + 1) % history.entries.len();
            }
        }
        Code::Char('k') | Code::Up => {
            if let Some(Overlay::History(history)) = &mut state.overlay
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
            if let Some(Overlay::History(history)) = &mut state.overlay
                && !history.revealed.remove(&history.index)
            {
                history.revealed.insert(history.index);
            }
        }
        Code::Char('y') => {
            if let Some(history) = state.history()
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
            state.take_quick_open();
            return Vec::new();
        }
        None => {}
    }
    match key.code {
        Code::Esc => {
            state.take_quick_open();
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
            if let Some(Overlay::QuickOpen(quick)) = &mut state.overlay
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
            if let Some(Overlay::QuickOpen(quick)) = &mut state.overlay {
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
    let Some(Overlay::QuickOpen(quick)) = &mut state.overlay else { return };
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
    let Some(Overlay::QuickOpen(quick)) = &mut state.overlay else { return Vec::new() };
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
                state.take_quick_open();
                select_item_by_id(state, &id);
            }
        }
        "quick_open.edit" => {
            if let Some(id) = quick.results.get(quick.index).cloned() {
                state.take_quick_open();
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
                state.take_quick_open();
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
        Code::Char('r') if state.error().is_some_and(|e| e.retry) => {
            state.enter_session(SessionPhase::Starting);
            let id = next_id(state);
            state.pending.insert(id, Pending::Starting);
            vec![Effect::StartBackend { id }]
        }
        Code::Char('q') => {
            state.quit = true;
            vec![Effect::Quit]
        }
        Code::Char('e') => {
            if let Some(path) = state.error().and_then(|e| e.open_file.clone()) {
                return vec![Effect::OpenFile(path)];
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn login_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::app::state::LoginFocus::{ClientId, ClientSecret, Method, Server, ServerUrl};
    use crate::config::keymap::Code;
    if is_ctrl_c(key) {
        return apply(state, Action::new("app.quit"));
    }
    // While the CLI works on a login or the server URL, the form waits.
    if state.logging_in() {
        return Vec::new();
    }
    // Keys bound in the `login` context (none by default) come first, as on
    // the unlock screen; printable keys stay text for the URL and API fields.
    if !key.is_printable()
        && let Some(call) = state.keymap.action("login", &[key])
    {
        let action = Action { id: call.id.clone(), args: call.args.clone(), count: None };
        return apply(state, action);
    }
    let key = ctrl_np_as_arrows(key);
    let login = &mut state.login;
    match (login.focus, key.code) {
        (_, Code::Enter) => return login_action(state, Action::new("login.submit")),
        (Server | Method | ClientId, Code::Tab)
        | (Server | Method | ClientSecret, Code::BackTab) => {
            return login_action(state, Action::new("login.next"));
        }
        (ClientSecret, Code::Tab) | (ClientId, Code::BackTab) => {
            return login_action(state, Action::new("login.prev"));
        }
        (_, Code::Esc) => return login_action(state, Action::new("login.back")),
        (Server | Method, Code::Char('q')) => return apply(state, Action::new("app.quit")),
        (Server, Code::Char('j') | Code::Down) => {
            login.server = login.server.next();
            if login.server == LoginServer::SelfHosted && login.server_input.is_empty() {
                login.focus = ServerUrl;
            }
        }
        (Server, Code::Char('k') | Code::Up) => login.server = login.server.prev(),
        (Method, Code::Char('j') | Code::Down) => login.method = login.method.next(),
        (Method, Code::Char('k') | Code::Up) => login.method = login.method.prev(),
        (ClientId, Code::Up | Code::Down) => login.focus = ClientSecret,
        (ClientSecret, Code::Up | Code::Down) => login.focus = ClientId,
        (ServerUrl, Code::Backspace) => {
            login.server_input.pop();
        }
        (ClientId, Code::Backspace) => {
            login.client_id.pop();
        }
        (ClientSecret, Code::Backspace) => {
            login.client_secret.pop();
        }
        (ServerUrl, Code::Char(c)) if key.is_printable() => login.server_input.push(c),
        (ClientId, Code::Char(c)) if key.is_printable() => login.client_id.push(c),
        (ClientSecret, Code::Char(c)) if key.is_printable() => login.client_secret.push(c),
        _ => {}
    }
    Vec::new()
}

/// Runs the login method chosen on the method list.
pub(crate) fn login_submit(state: &mut AppState) -> Vec<Effect> {
    state.login.error = None;
    match state.login.method {
        LoginMethod::Password => vec![Effect::Suspend(SuspendTask::Login { sso: false })],
        LoginMethod::Sso => vec![Effect::Suspend(SuspendTask::Login { sso: true })],
        LoginMethod::ApiKey => {
            if state.login.client_id.is_empty() || state.login.client_secret.is_empty() {
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

/// The login screen's transitions; keys and the `login.*` actions both
/// land here. See [`crate::app::state::LoginFocus`].
pub(crate) fn login_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    use crate::app::state::LoginFocus::{ClientId, ClientSecret, Method, Server, ServerUrl};
    if state.logging_in() {
        return Vec::new();
    }
    let login = &mut state.login;
    match (action.id.as_str(), login.focus) {
        // Enter: move on to the next part, or log in from the last one.
        ("login.submit", Server) => {
            login.focus =
                if login.server == LoginServer::SelfHosted && login.server_input.is_empty() {
                    ServerUrl
                } else {
                    Method
                };
        }
        ("login.submit", ServerUrl) => {
            login.focus = Server;
            login.server = LoginServer::SelfHosted;
            let url = login.server_input.clone();
            if !url.is_empty() {
                return vec![request(state, CommandKind::ConfigServer(url), Pending::ConfigServer)];
            }
        }
        ("login.submit", Method) if login.method == LoginMethod::ApiKey => login.focus = ClientId,
        ("login.submit", Method | ClientId | ClientSecret) => return login_submit(state),
        ("login.next" | "login.prev", Server) => login.focus = Method,
        ("login.next" | "login.prev", Method) => login.focus = Server,
        ("login.next" | "login.prev", ClientId) => login.focus = ClientSecret,
        ("login.next" | "login.prev", ClientSecret) => login.focus = ClientId,
        // Back out of the API key form or the URL, then from the methods.
        ("login.back", ClientId | ClientSecret) => login.focus = Method,
        ("login.back", ServerUrl | Method) => login.focus = Server,
        _ => {}
    }
    Vec::new()
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
