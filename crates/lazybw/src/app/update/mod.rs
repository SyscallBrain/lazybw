//! The update function: state transition from messages to effects.
//!
//! All user behaviour funnels through [`apply`], so an action triggered by a
//! key, a `:` command, a picker or the keymap editor behaves identically.

mod apply;
mod backend;
mod cmdline;
mod editor;
mod mouse;
mod overlays;
mod settings;
mod tools;
mod vault;
mod wizards;

// The submodules are one update function split by area; this module is
// their shared namespace. Each submodule imports by name what it uses
// (`use super::{…}`), and these re-exports are what makes those names
// resolve here, so they stay globs on purpose.
use apply::*;
use backend::*;
use cmdline::*;
use editor::*;
use mouse::*;
use overlays::*;
pub use settings::*;
use tools::*;
pub use vault::*;
use wizards::*;

use crate::app::effect::Effect;
use crate::app::msg::{EditorTarget, Msg, SuspendTask, Suspended};
use crate::app::state::{
    Action, AppState, CaptureState, CmdlineState, ConfirmState, DetailState, EditorState, FormKey,
    FormKind, FormRow, HistoryState, InfoState, MessageLevel, Pending, PickerEntry, PickerKind,
    PickerState, PromptState, QuickOpenState, RepromptState, Screen, SessionPhase, SettingKind,
    SettingRow, Tab, UnlockState, WhichKeyState,
};
use crate::bw::models::{
    Card, Field, FieldKind, Identity, Item, ItemType, Login, LoginUri, SshKey, UriMatch,
};
use crate::bw::service::{CommandKind, ExportFormat, ImportRequest};
use crate::config::keymap::{Key, Keymap, Source, display_keys, parse_keys};
use crate::config::{Density, IconMode, SortOrder};
use crate::vault::cache::Filter;
use crate::vault::health::HealthCategory;
use crate::vault::query::Query;
use chrono::Utc;
use secrecy::SecretString;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

/// Applies a message and returns the effects to run.
pub fn update(state: &mut AppState, msg: Msg) -> Vec<Effect> {
    let folds_before = state.folds_restored.then(|| state.collapsed.clone());
    let editor_before = state.editor.as_ref().map(|e| (e.snapshot(), e.insert));
    let mut effects = update_inner(state, msg);
    record_editor_change(state, editor_before);
    update_theme_preview(state);
    // Persist folder folds whenever they change (after the saved ones were
    // restored, so the first load never overwrites them with defaults).
    if let Some(before) = folds_before
        && before != state.collapsed
    {
        effects.push(Effect::SaveFolds);
    }
    if !state.dispatcher.is_pending() {
        state.which_key = None;
    }
    // Revealed fields re-hide whenever the selection moves to another item.
    let selected = state.selected_id();
    if selected != state.detail_item {
        if state.detail_item.is_some() {
            state.detail = DetailState::default();
        }
        state.detail_item = selected;
    }
    effects
}
/// Records one undo step when the editor form changed. Typing in one INSERT
/// session is a single step, as in Vim.
fn record_editor_change(
    state: &mut AppState,
    before: Option<(crate::app::state::EditorSnapshot, bool)>,
) {
    let Some(editor) = &mut state.editor else { return };
    if std::mem::take(&mut editor.history.restoring) {
        return;
    }
    let Some((before, was_insert)) = before else { return };
    let changed = before.item != editor.item
        || before.rows.len() != editor.rows.len()
        || before.rows.iter().zip(&editor.rows).any(|(a, b)| a.value != b.value);
    if changed {
        editor.history.revision += 1;
    }
    if changed && !(was_insert && editor.insert && editor.history.insert_recorded) {
        editor.history.undo.push(before);
        editor.history.redo.clear();
        if editor.history.undo.len() > 200 {
            editor.history.undo.remove(0);
            editor.history.saved_depth = editor.history.saved_depth.saturating_sub(1);
        }
        editor.history.insert_recorded = editor.insert;
    }
    if !editor.insert {
        editor.history.insert_recorded = false;
    }
}

fn update_inner(state: &mut AppState, msg: Msg) -> Vec<Effect> {
    state.dirty = true;
    match msg {
        Msg::Key(event) => on_key(state, event),
        Msg::Paste(text) => on_paste(state, text),
        Msg::Mouse(event) => on_mouse(state, event),
        Msg::Resize(cols, rows) => {
            state.size = (cols, rows);
            Vec::new()
        }
        Msg::Tick => on_tick(state),
        Msg::Backend(event) => on_backend(state, event),
        Msg::Reload(event) => on_reload(state, event),
        Msg::FocusLost => {
            if state.config.security.hide_on_focus_lost {
                state.detail.revealed.clear();
                if let Some(editor) = &mut state.editor {
                    editor.revealed = false;
                }
            }
            Vec::new()
        }
        Msg::FocusGained => Vec::new(),
        Msg::Suspended(result) => on_suspended(state, result),
        Msg::Wake => {
            if state.session == SessionPhase::Starting {
                let id = next_id(state);
                state.pending.insert(id, Pending::Starting);
                vec![Effect::StartBackend { id }]
            } else {
                Vec::new()
            }
        }
        Msg::LockSignal => apply(state, Action::new("vault.lock")),
    }
}
fn body_rect(state: &AppState) -> ratatui::layout::Rect {
    let plan = crate::ui::layout::LayoutPlan::compute_sized(
        ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1),
        state.sidebar_visible,
        state.sidebar_width,
        state.list_width,
    );
    plan.body
}
fn animate(state: &mut AppState, kind: crate::ui::fx::FxKind) {
    let area = body_rect(state);
    let theme = state.theme.clone();
    state.fx.borrow_mut().trigger(kind, &theme, area);
}
fn next_id(state: &mut AppState) -> u64 {
    let id = state.next_request_id;
    state.next_request_id += 1;
    id
}
fn request(state: &mut AppState, kind: CommandKind, pending: Pending) -> Effect {
    let id = next_id(state);
    state.pending.insert(id, pending);
    Effect::Backend { id, kind }
}
fn toast(state: &mut AppState, level: MessageLevel, text: impl Into<String>) {
    state.toast(level, text);
}
fn copy_value(state: &mut AppState, text: &str, label: &str, item_name: &str) -> Vec<Effect> {
    if text.is_empty() {
        toast(state, MessageLevel::Warn, "Nothing to copy.");
        return Vec::new();
    }
    let timeout = state.config.security.clipboard_timeout.get();
    match state.clipboard.copy(text, timeout, Instant::now()) {
        Ok(()) => {
            state.clipboard_label = Some(label.to_owned());
            animate(state, crate::ui::fx::FxKind::Flash);
            let seconds = timeout.map(|d| d.as_secs()).unwrap_or(0);
            let message = if timeout.is_some() {
                format!("Copied {label} for “{item_name}”. Clipboard clears in {seconds}s.")
            } else {
                format!("Copied {label} for “{item_name}”.")
            };
            toast(state, MessageLevel::Ok, message);
        }
        Err(err) => {
            state.toast_with(MessageLevel::Error, "Couldn't copy.", err);
        }
    }
    Vec::new()
}
fn on_tick(state: &mut AppState) -> Vec<Effect> {
    let mut effects = Vec::new();
    state.now = Utc::now();
    state.expire(Instant::now());
    if state.clipboard.tick(Instant::now()) {
        state.message(MessageLevel::Info, "Clipboard cleared.");
    }
    // Resolve an ambiguous sequence after timeoutlen.
    if state.dispatcher.is_pending() {
        let now = Instant::now();
        let elapsed = state.dispatcher.since.map(|since| now.duration_since(since));
        let chain = state.contexts();
        if elapsed.is_some_and(|e| e >= state.config.timeoutlen())
            && state.dispatcher.timeout_applies(&state.keymap, &chain)
        {
            if let Some(action) = state.dispatcher.timeout(&state.keymap, &chain) {
                effects.extend(apply(state, action));
            }
            state.showcmd = state.dispatcher.pending_display(&state.keymap);
        }
    }
    // Which-key.
    if state.dispatcher.which_key_due(Instant::now(), state.config.which_key_delay()) {
        if state.which_key.is_none() {
            let chain = state.contexts();
            state.which_key = Some(WhichKeyState {
                prefix: state.dispatcher.pending_display(&state.keymap),
                continuations: state.dispatcher.continuations(&state.keymap, &chain),
                since: Instant::now(),
            });
        }
    } else if !state.dispatcher.is_pending() {
        state.which_key = None;
    }
    // Auto-lock.
    if state.session.is_ready()
        && let Some(limit) = state.config.security.auto_lock.get()
        && !limit.is_zero()
        && state.last_input.elapsed() >= limit
    {
        state.last_input = Instant::now();
        effects.extend(apply(state, Action::new("vault.lock")));
    }
    // Background sync.
    if state.session.is_ready()
        && let Some(interval) = state.config.vault.sync_interval.get()
        && interval >= std::time::Duration::from_secs(30)
    {
        let due = state.last_sync_at.is_none_or(|last| last.elapsed() >= interval);
        if due && !state.pending.values().any(|p| matches!(p, Pending::Sync)) {
            state.last_sync_at = Some(Instant::now());
            effects.push(request(state, CommandKind::Sync { force: false }, Pending::Sync));
        }
    }
    // TOTP changes coalesce the digits.
    if state.session.is_ready()
        && let Some(item) = state.selected_item()
        && let Some(secret) = item.login.as_ref().and_then(|login| login.totp.as_ref())
    {
        let code = crate::totp::current(secret.expose(), state.now.timestamp() as u64)
            .ok()
            .map(|code| code.code);
        if let Some(code) = code {
            if state.last_totp_code.as_deref().is_some_and(|last| last != code) {
                animate(state, crate::ui::fx::FxKind::Coalesce);
            }
            state.last_totp_code = Some(code);
        }
    }
    if state.fx.borrow().is_active() {
        state.dirty = true;
    }
    // Reveal timeout.
    if state.detail.hide_at.is_some_and(|at| Instant::now() >= at) {
        state.detail.revealed.clear();
        state.detail.hide_at = None;
    }
    effects
}
fn on_reload(state: &mut AppState, event: crate::config::watch::ReloadEvent) -> Vec<Effect> {
    use crate::config::watch::ReloadEvent;
    match event {
        ReloadEvent::Config => {
            let (config, warnings) = match crate::config::Config::load(&state.paths.config_file()) {
                Ok(loaded) => loaded,
                Err(err) => {
                    state.toast_with(
                        MessageLevel::Error,
                        "Config not reloaded.",
                        format!("{err}. Keeping the previous settings."),
                    );
                    return Vec::new();
                }
            };
            if !config.validate().is_empty() {
                state.toast(
                    MessageLevel::Error,
                    "Config not reloaded: invalid values. Keeping the previous settings.",
                );
                return Vec::new();
            }
            for warning in warnings {
                state.message(MessageLevel::Warn, warning);
            }
            let mut effects = Vec::new();
            if config.ui.theme != state.config.ui.theme {
                effects.extend(apply(state, Action::with("theme.set", &[&config.ui.theme])));
            }
            state.config = config;
            state.theme.resolve(crate::theme::truecolor_available(), crate::theme::no_color());
            state.message(MessageLevel::Ok, "Config reloaded.");
            effects
        }
        ReloadEvent::Themes => {
            state.message(MessageLevel::Ok, "Themes reloaded.");
            Vec::new()
        }
        ReloadEvent::Error(err) => {
            state.message(MessageLevel::Error, format!("Config watcher: {err}"));
            Vec::new()
        }
    }
}
fn show_backend_error(state: &mut AppState, err: &crate::bw::BwError) {
    if err.requires_lock() {
        state.drop_secrets();
        state.toast(MessageLevel::Warn, "Vault is locked.");
        return;
    }
    if err.requires_login() {
        state.session = SessionPhase::LoggedOut;
        state.screen = Screen::Login;
        state.toast(MessageLevel::Warn, "You are not logged in.");
        return;
    }
    if err.is_disconnected() {
        state.backend_alive = false;
        state.toast_with(
            MessageLevel::Error,
            "Bitwarden CLI stopped. Restarting…",
            err.to_string(),
        );
        return;
    }
    state.toast_with(MessageLevel::Error, "Bitwarden CLI error.", err.to_string());
}
fn on_key(state: &mut AppState, event: crossterm::event::KeyEvent) -> Vec<Effect> {
    let Some(key) = crate::input::keys::from_event(event) else { return Vec::new() };
    state.last_input = Instant::now();

    if state.keymap_editor.capture.is_some() && state.screen == Screen::KeymapEditor {
        return capture_key(state, key);
    }
    if state.notes.is_some() && state.screen == Screen::Editor {
        return notes_key(state, key);
    }
    if state.prompt.is_some() {
        return prompt_key(state, key);
    }
    if state.cmdline.is_some() {
        return cmdline_key(state, key);
    }
    if state.quick_open.is_some() {
        return quick_open_key(state, key);
    }
    if state.picker.is_some() {
        return picker_key(state, key);
    }
    if state.confirm.is_some() {
        return confirm_key(state, key);
    }
    if state.remprompt.is_some() {
        return reprompt_key(state, key);
    }
    if state.export.is_some() {
        return export_key(state, key);
    }
    if state.import.is_some() {
        return import_key(state, key);
    }
    if state.history.is_some() || state.info.is_some() {
        return popup_key(state, key);
    }
    if state.search.active {
        return search_key(state, key);
    }
    match state.screen {
        Screen::Unlock => unlock_key(state, key),
        Screen::Login => login_key(state, key),
        Screen::Starting | Screen::Error => error_key(state, key),
        Screen::Editor if state.editor.as_ref().is_some_and(|e| e.insert) => {
            editor_insert_key(state, key)
        }
        _ => dispatch(state, key),
    }
}
/// What the keymap binds to `key` in an overlay that takes text input.
///
/// The picker, quick open and the command line read printable keys as text,
/// so only keys with a modifier or a special key (`<C-n>`, `<Down>`, `<CR>`)
/// are looked up. Returns `Some(None)` for `app.cancel`, which the caller
/// treats as `Esc`, and `None` when nothing is bound.
pub(crate) fn overlay_binding(state: &AppState, context: &str, key: Key) -> Option<Option<Action>> {
    if key.is_printable() {
        return None;
    }
    let call = state.keymap.action(context, &[key])?;
    if call.id == "app.cancel" {
        return Some(None);
    }
    let prefix = format!("{context}.");
    call.id
        .starts_with(&prefix)
        .then(|| Some(Action { id: call.id.clone(), args: call.args.clone(), count: None }))
}

/// `Ctrl-n` and `Ctrl-p` move like `Down` and `Up` in the lists that read
/// keys directly (export format, import preview, history, login choices).
pub(crate) fn ctrl_np_as_arrows(key: Key) -> Key {
    use crate::config::keymap::{Code, Mods};
    if key.mods == Mods::CTRL {
        match key.code {
            Code::Char('n') => return Key { code: Code::Down, mods: Mods::default() },
            Code::Char('p') => return Key { code: Code::Up, mods: Mods::default() },
            _ => {}
        }
    }
    key
}

fn dispatch(state: &mut AppState, key: Key) -> Vec<Effect> {
    let chain = state.contexts();
    let text_context = None;
    match state.dispatcher.feed(key, &state.keymap, &chain, text_context, Instant::now()) {
        crate::input::dispatcher::Dispatch::Action(action) => {
            state.showcmd.clear();
            apply(state, action)
        }
        crate::input::dispatcher::Dispatch::Pending => {
            state.showcmd = state.dispatcher.pending_display(&state.keymap);
            Vec::new()
        }
        crate::input::dispatcher::Dispatch::Text(ch) => {
            // Should not happen outside a text screen; insert where sensible.
            if let Some(cmdline) = &mut state.cmdline {
                cmdline.input.push(ch);
                cmdline.cursor += 1;
            }
            Vec::new()
        }
        crate::input::dispatcher::Dispatch::Unhandled(_) => Vec::new(),
        crate::input::dispatcher::Dispatch::Discard => Vec::new(),
    }
}
fn on_paste(state: &mut AppState, text: String) -> Vec<Effect> {
    // Password fields take the first line only (a trailing newline from a
    // password manager must not submit or corrupt the value).
    let first_line = text.lines().next().unwrap_or_default();
    if let Some(reprompt) = &mut state.remprompt {
        reprompt.password.push_str(first_line);
        return Vec::new();
    }
    if let Some(export) = &mut state.export {
        match (export.step, export.focus) {
            (0, 1) => export.path.push_str(first_line),
            (1, 0) => export.password.push_str(first_line),
            (1, 1) => export.confirmation.push_str(first_line),
            (2, _) => export.master.push_str(first_line),
            _ => {}
        }
        return Vec::new();
    }
    if let Some(import) = &mut state.import {
        if import.step == 1 {
            if import.password_focus {
                import.password.push_str(first_line);
            } else {
                import.path.push_str(first_line);
            }
        }
        return Vec::new();
    }
    if state.screen == Screen::Unlock && state.prompt.is_none() && state.confirm.is_none() {
        state.unlock.password.push_str(first_line);
        state.unlock.error = None;
        return Vec::new();
    }
    if let Some(cmdline) = &mut state.cmdline {
        cmdline.input.push_str(&text);
        cmdline.cursor = cmdline.input.chars().count();
    } else if let Some(quick) = &mut state.quick_open {
        quick.input.push_str(&text);
        quick.cursor = quick.input.chars().count();
        refresh_quick_open(state);
    } else if state.search.active {
        state.search.input.push_str(&text);
        state.search.query = Some(Query::parse(&state.search.input));
        state.search.index = 0;
        state.list_row = 0;
    } else if let Some(editor) = &mut state.editor
        && editor.insert
    {
        let text = text.replace('\n', " ");
        editor_value_insert(state, &text);
    }
    Vec::new()
}
fn cancel(state: &mut AppState) -> Vec<Effect> {
    if state.generator_popup_open() && state.confirm.is_none() && state.picker.is_none() {
        state.generator.popup = false;
    } else if state.confirm.is_some() {
        state.confirm = None;
    } else if state.picker.is_some() {
        state.picker = None;
    } else if state.remprompt.is_some() {
        state.remprompt = None;
        state.reprompt_action = None;
    } else if state.prompt.is_some() {
        state.prompt = None;
    } else if state.history.is_some() {
        state.history = None;
    } else if state.info.is_some() {
        state.info = None;
    } else if state.quick_open.is_some() {
        state.quick_open = None;
    } else if state.cmdline.is_some() {
        state.cmdline = None;
    } else if state.search.active {
        state.search.active = false;
        state.search.input.clear();
        state.search.query = None;
    } else if state.keymap_editor.capture.is_some() {
        state.keymap_editor.capture = None;
    } else if state.screen == Screen::Editor {
        close_editor(state);
    } else if state.screen == Screen::Help || state.screen == Screen::Messages {
        state.screen = state.tab.screen();
    } else if state.screen == Screen::Settings || state.screen == Screen::KeymapEditor {
        state.screen = Screen::Vault;
    } else if state.visual_anchor.is_some() {
        state.visual_anchor = None;
    }
    state.dispatcher.reset();
    state.showcmd.clear();
    Vec::new()
}
fn switch_tab(state: &mut AppState, tab: Tab) -> Vec<Effect> {
    animate(state, crate::ui::fx::FxKind::TabFade);
    state.tab = tab;
    if state.session.is_ready() {
        state.screen = tab.screen();
    }
    match tab {
        Tab::Generator => {
            return vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )];
        }
        Tab::Health if state.health.report.is_none() && !state.health.running => {
            state.health.running = true;
            return vec![request(state, CommandKind::Health, Pending::Health)];
        }
        _ => {}
    }
    Vec::new()
}
