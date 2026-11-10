//! Keys for pickers, dialogs, prompts, quick open and the login/unlock
//! screens.

use super::{
    Action, AppState, CommandKind, Deferred, Effect, FieldKind, Filter, FormKey, ItemType, Key,
    LoginMethod, LoginServer, MessageLevel, Overlay, Pending, PickerKind, PromptAction,
    PromptState, Query, Screen, SecretString, SessionPhase, SuspendTask, UriMatch, accept_prompt,
    apply, cancel, copy_value, ctrl_np_as_arrows, field_label, field_value, move_items, next_id,
    open_new_editor, overlay_binding, rebuild_rows, reprompt_for, reprompt_required, request, run,
    select_item_by_id, sidebar_entries, sync_editor,
};
use crate::app::actions::ActionId;

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
        Code::Up => return picker_action(state, &Action::new(ActionId::PickerPrev)),
        Code::Down => return picker_action(state, &Action::new(ActionId::PickerNext)),
        Code::Enter => return picker_action(state, &Action::new(ActionId::PickerAccept)),
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
        _ => {
            if let Some(Overlay::Picker(picker)) = &mut state.overlay
                && crate::input::line::edit_at_end(&mut picker.filter, key)
            {
                picker.refilter();
            }
        }
    }
    Vec::new()
}

pub(crate) fn picker_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    let Some(Overlay::Picker(picker)) = &mut state.overlay else { return Vec::new() };
    match action.id {
        ActionId::PickerNext => {
            if !picker.filtered.is_empty() {
                picker.index = (picker.index + 1) % picker.filtered.len();
            }
        }
        ActionId::PickerPrev => {
            if !picker.filtered.is_empty() {
                picker.index = (picker.index + picker.filtered.len() - 1) % picker.filtered.len();
            }
        }
        ActionId::PickerAccept => {
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
            state.search = crate::app::state::SearchState::default();
        }
        PickerKind::ItemType => {
            let kind = ItemType::parse(value).unwrap_or(ItemType::Login);
            open_new_editor(state, kind);
        }
        PickerKind::Folder => {
            if value == "__create__" {
                state.open(PromptState::new("New folder", "", PromptAction::NewFolder));
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
        // Chosen from the picker (also the Settings row): kept in config.toml,
        // unlike `:theme`, which tries one for this session.
        PickerKind::Theme => return super::apply_setting(state, "ui.theme", value),
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
    // The `confirm` context decides (y, n, Enter, Esc by default), except for
    // printable keys while a confirmation word is being typed.
    let typing = state.confirm().is_some_and(|c| c.requires_word.is_some());
    if !(typing && key.is_printable())
        && let Some(call) = state.keymap.action("confirm", &[key])
    {
        return Action::from_call(call).map(|action| apply(state, action)).unwrap_or_default();
    }
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
                Some(deferred) => run(state, deferred),
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
    if let Deferred::RenameFolder { also, .. } | Deferred::DeleteFolder { also, .. } =
        &mut on_confirm
    {
        *also = confirm.also_subfolders;
    }
    if let Some(word) = confirm.requires_word.clone()
        && confirm.input.trim() != word
    {
        state.message(MessageLevel::Error, format!("Type {word} to confirm."));
        state.open(confirm);
        return Vec::new();
    }
    run(state, on_confirm)
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
            let on_accept = prompt.on_accept;
            state.take_prompt();
            return accept_prompt(state, on_accept, value);
        }
        _ => {
            prompt.input.edit_key(key);
        }
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
        _ if is_ctrl(key, 'u') => reprompt.password.clear(),
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
        Code::Enter => return quick_open_action(state, &Action::new(ActionId::QuickOpenAccept)),
        Code::Down | Code::Tab => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenNext));
        }
        Code::Up | Code::BackTab => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenPrev));
        }
        Code::Char('y') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenCopyPassword));
        }
        Code::Char('u') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenCopyUsername));
        }
        Code::Char('t') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenCopyTotp));
        }
        Code::Char('e') if key.mods.contains(crate::config::keymap::Mods::CTRL) => {
            return quick_open_action(state, &Action::new(ActionId::QuickOpenEdit));
        }
        _ => {
            if let Some(Overlay::QuickOpen(quick)) = &mut state.overlay
                && crate::input::line::edit_at_end(&mut quick.input, key)
            {
                quick.cursor = quick.input.chars().count();
                quick.index = 0;
                refresh_quick_open(state);
            }
        }
    }
    Vec::new()
}

pub(crate) fn refresh_quick_open(state: &mut AppState) {
    let Some(Overlay::QuickOpen(quick)) = &mut state.overlay else { return };
    let query = Query::parse(&quick.input);
    let mut matcher = crate::vault::query::QueryMatcher::new(&query);
    let health = state.health.report.as_ref().filter(|_| query.uses_health());
    let mut hits: Vec<(std::sync::Arc<crate::vault::cache::Row>, u32)> = state
        .cache
        .rows_matching(&Filter::All)
        .into_iter()
        .filter_map(|row| {
            let item = state.cache.item(&row.id)?;
            let flags = health.map(|r| r.flags(&row.id));
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
    match action.id {
        ActionId::QuickOpenNext => {
            if !quick.results.is_empty() {
                quick.index = (quick.index + 1) % quick.results.len();
            }
        }
        ActionId::QuickOpenPrev => {
            if !quick.results.is_empty() {
                quick.index = (quick.index + quick.results.len() - 1) % quick.results.len();
            }
        }
        ActionId::QuickOpenAccept => {
            if let Some(id) = quick.results.get(quick.index).cloned() {
                state.take_quick_open();
                select_item_by_id(state, &id);
            }
        }
        ActionId::QuickOpenEdit => {
            if let Some(id) = quick.results.get(quick.index).cloned() {
                state.take_quick_open();
                select_item_by_id(state, &id);
                return apply(state, Action::new(ActionId::ItemEdit));
            }
        }
        ActionId::QuickOpenCopyPassword
        | ActionId::QuickOpenCopyUsername
        | ActionId::QuickOpenCopyTotp => {
            if let Some(id) = quick.results.get(quick.index).cloned()
                && let Some(item) = state.cache.item(&id).cloned()
            {
                let field = match action.id {
                    ActionId::QuickOpenCopyPassword => "password",
                    ActionId::QuickOpenCopyUsername => "username",
                    _ => "totp",
                };
                state.take_quick_open();
                if field != "username" && reprompt_required(state, &item) {
                    // Select the item so the re-prompt and the copy target it.
                    select_item_by_id(state, &id);
                    return reprompt_for(state, &item, Action::with(ActionId::ItemCopy, &[field]));
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
        return apply(state, Action::new(ActionId::AppQuit));
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

/// Keys typed while the CLI starts are the master password, typed ahead:
/// `bw serve` takes most of a second to come up, and people start typing as
/// soon as lazybw opens. Enter unlocks as soon as the CLI reports a locked
/// vault; if it reports anything else, what was typed is dropped (`Started`
/// in `update/backend.rs`). Only Ctrl-c leaves from here, so a `q` in the
/// password cannot quit and send the rest of it to the shell.
pub(crate) fn starting_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    if is_ctrl_c(key) {
        return apply(state, Action::new(ActionId::AppQuit));
    }
    let unlock = &mut state.unlock;
    match key.code {
        Code::Enter => unlock.submit_when_ready = !unlock.password.is_empty(),
        Code::Esc => *unlock = crate::app::state::UnlockState::default(),
        Code::Backspace => {
            unlock.password.pop();
            unlock.submit_when_ready = false;
        }
        _ if is_ctrl(key, 'u') => {
            unlock.password.clear();
            unlock.submit_when_ready = false;
        }
        Code::Char(c) if key.is_printable() => {
            unlock.password.push(c);
            unlock.submit_when_ready = false;
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn login_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::app::state::LoginFocus::{ClientId, ClientSecret, Method, Server, ServerUrl};
    use crate::config::keymap::Code;
    if is_ctrl_c(key) {
        return apply(state, Action::new(ActionId::AppQuit));
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
        return Action::from_call(call).map(|action| apply(state, action)).unwrap_or_default();
    }
    let key = ctrl_np_as_arrows(key);
    let login = &mut state.login;
    match (login.focus, key.code) {
        (_, Code::Enter) => return login_action(state, Action::new(ActionId::LoginSubmit)),
        (Server | Method | ClientId, Code::Tab)
        | (Server | Method | ClientSecret, Code::BackTab) => {
            return login_action(state, Action::new(ActionId::LoginNext));
        }
        (ClientSecret, Code::Tab) | (ClientId, Code::BackTab) => {
            return login_action(state, Action::new(ActionId::LoginPrev));
        }
        (_, Code::Esc) => return login_action(state, Action::new(ActionId::LoginBack)),
        (Server | Method, Code::Char('q')) => return apply(state, Action::new(ActionId::AppQuit)),
        // Passing over "Self-hosted" does not open its URL field: Enter does
        // (`login.submit`), so the next `k` still moves instead of typing.
        (Server, Code::Char('j') | Code::Down) => login.server = login.server.next(),
        (Server, Code::Char('k') | Code::Up) => login.server = login.server.prev(),
        (Method, Code::Char('j') | Code::Down) => login.method = login.method.next(),
        (Method, Code::Char('k') | Code::Up) => login.method = login.method.prev(),
        (ClientId, Code::Up | Code::Down) => login.focus = ClientSecret,
        (ClientSecret, Code::Up | Code::Down) => login.focus = ClientId,
        (ServerUrl, _) => {
            crate::input::line::edit_at_end(&mut login.server_input, key);
        }
        (ClientId, _) => {
            crate::input::line::edit_at_end(&mut login.client_id, key);
        }
        (ClientSecret, Code::Backspace) => {
            login.client_secret.pop();
        }
        (ClientSecret, _) if is_ctrl(key, 'u') => login.client_secret.clear(),
        (ClientSecret, Code::Char(c)) if key.is_printable() => login.client_secret.push(c),
        _ => {}
    }
    Vec::new()
}

/// Tells `bw` which server to use, remembering the URL so the answer can
/// record it (`ServerConfigured` in `update/backend.rs`).
fn configure_server(state: &mut AppState, url: String) -> Vec<Effect> {
    state.login.configuring_server = Some(url.clone());
    vec![request(state, CommandKind::ConfigServer(url), Pending::ConfigServer)]
}

/// Logs in with the chosen method, on the chosen server.
///
/// `bw login` goes to whatever server `bw` has configured, which while
/// logged out is not known and need not be the one highlighted here (an
/// earlier self-hosted setup, say). So the server is set first, and the
/// login starts when `bw` has accepted it.
pub(crate) fn login_submit(state: &mut AppState) -> Vec<Effect> {
    state.login.error = None;
    if state.login.method == LoginMethod::ApiKey
        && (state.login.client_id.is_empty() || state.login.client_secret.is_empty())
    {
        state.login.error = Some("Enter the client id and secret.".to_owned());
        return Vec::new();
    }
    if state.login.server_needs_configuring() {
        let url = state.login.server_url().to_owned();
        if url.is_empty() {
            state.login.error = Some("Enter the server URL.".to_owned());
            state.login.focus = crate::app::state::LoginFocus::ServerUrl;
            return Vec::new();
        }
        state.login.login_after_server = true;
        return configure_server(state, url);
    }
    start_login(state)
}

/// Runs the login method chosen on the method list; the server is set.
pub(crate) fn start_login(state: &mut AppState) -> Vec<Effect> {
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
    match (action.id, login.focus) {
        // Enter: move on to the next part, or log in from the last one.
        (ActionId::LoginSubmit, Server) => {
            login.focus =
                if login.server == LoginServer::SelfHosted && login.server_input.is_empty() {
                    ServerUrl
                } else {
                    Method
                };
        }
        (ActionId::LoginSubmit, ServerUrl) => {
            login.focus = Server;
            login.server = LoginServer::SelfHosted;
            let url = login.server_input.trim().to_owned();
            if !url.is_empty() {
                return configure_server(state, url);
            }
        }
        (ActionId::LoginSubmit, Method) if login.method == LoginMethod::ApiKey => {
            login.focus = ClientId
        }
        (ActionId::LoginSubmit, Method | ClientId | ClientSecret) => return login_submit(state),
        (ActionId::LoginNext | ActionId::LoginPrev, Server) => login.focus = Method,
        (ActionId::LoginNext | ActionId::LoginPrev, Method) => login.focus = Server,
        (ActionId::LoginNext | ActionId::LoginPrev, ClientId) => login.focus = ClientSecret,
        (ActionId::LoginNext | ActionId::LoginPrev, ClientSecret) => login.focus = ClientId,
        // Back out of the API key form or the URL, then from the methods.
        (ActionId::LoginBack, ClientId | ClientSecret) => login.focus = Method,
        (ActionId::LoginBack, ServerUrl | Method) => login.focus = Server,
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
        return Action::from_call(call).map(|action| apply(state, action)).unwrap_or_default();
    }
    match key.code {
        Code::Enter => apply(state, Action::new(ActionId::UnlockSubmit)),
        Code::Esc => cancel(state),
        Code::Backspace => {
            state.unlock.password.pop();
            Vec::new()
        }
        _ if is_ctrl(key, 'u') => {
            state.unlock.password.clear();
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
    is_ctrl(key, 'c')
}

/// `Ctrl` and a letter, nothing else held.
pub(crate) fn is_ctrl(key: Key, letter: char) -> bool {
    use crate::config::keymap::{Code, Mods};
    key.code == Code::Char(letter) && key.mods == Mods::CTRL
}
