//! The `:` command line and session key mappings.

use super::{
    Action, AppState, Effect, Key, MessageLevel, Overlay, Source, apply, apply_setting,
    default_config_value, display_keys, overlay_binding, parse_keys, resolve_setting_key,
    setting_value, sidebar_entries,
};
use crate::app::actions::ActionId;

pub(crate) fn cmdline_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match overlay_binding(state, "cmdline", key) {
        Some(Some(action)) => return apply(state, action),
        Some(None) => {
            state.take_cmdline();
            return Vec::new();
        }
        None => {}
    }
    let Some(Overlay::Cmdline(cmdline)) = &mut state.overlay else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.take_cmdline();
            return Vec::new();
        }
        Code::Enter => return apply(state, Action::new(ActionId::CmdlineExecute)),
        Code::Tab => return apply(state, Action::new(ActionId::CmdlineCompleteNext)),
        Code::BackTab => return apply(state, Action::new(ActionId::CmdlineCompletePrev)),
        Code::Up => return apply(state, Action::new(ActionId::CmdlineHistoryPrev)),
        Code::Down => return apply(state, Action::new(ActionId::CmdlineHistoryNext)),
        // Backspace on an empty line leaves it, as in Vim.
        Code::Backspace if cmdline.input.is_empty() => {
            state.take_cmdline();
            return Vec::new();
        }
        _ => {
            if !crate::input::line::edit_string(&mut cmdline.input, &mut cmdline.cursor, key) {
                return Vec::new();
            }
            cmdline.history_index = None;
        }
    }
    refresh_completions(state);
    Vec::new()
}

pub(crate) fn refresh_completions(state: &mut AppState) {
    let Some(input) = state.cmdline().map(|c| c.input.clone()) else { return };
    let completions = crate::app::commands::completions(&input, state);
    if let Some(Overlay::Cmdline(cmdline)) = &mut state.overlay {
        cmdline.completions = completions;
        cmdline.completion_index = 0;
        cmdline.show_completions = !cmdline.completions.is_empty();
    }
}

pub(crate) fn cmdline_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    match action.id {
        ActionId::CmdlineExecute => {
            let Some(cmdline) = state.take_cmdline() else { return Vec::new() };
            let line = cmdline.input.trim().to_owned();
            let mut effects = Vec::new();
            if !line.is_empty() {
                remember_command(state, line);
                effects.push(Effect::SaveHistory);
            }
            let command = crate::app::commands::parse(&cmdline.input);
            effects.extend(execute_command(state, command));
            effects
        }
        ActionId::CmdlineCompleteNext | ActionId::CmdlineCompletePrev => {
            let Some(Overlay::Cmdline(cmdline)) = &mut state.overlay else { return Vec::new() };
            if cmdline.completions.is_empty() {
                refresh_completions(state);
                return Vec::new();
            }
            // A completion replaces everything after the command name, which
            // is what it was matched against: a folder name has spaces, and
            // replacing only the last word would pile the names up
            // (`:move Social Work` after `Social media`).
            let trimmed_start = cmdline.input.len() - cmdline.input.trim_start().len();
            let prefix_end = cmdline.input[trimmed_start..]
                .find(char::is_whitespace)
                .map(|i| trimmed_start + i + 1)
                .unwrap_or(trimmed_start);
            let head = cmdline.input[..prefix_end].to_owned();
            let selected = cmdline.completions[cmdline.completion_index].clone();
            cmdline.input = format!("{head}{selected}");
            cmdline.completion_index = (cmdline.completion_index + 1) % cmdline.completions.len();
            cmdline.cursor = cmdline.input.chars().count();
            Vec::new()
        }
        ActionId::CmdlineHistoryPrev | ActionId::CmdlineHistoryNext => {
            let older = action.id == ActionId::CmdlineHistoryPrev;
            let history = &state.command_history;
            let Some(Overlay::Cmdline(cmdline)) = &mut state.overlay else { return Vec::new() };
            // Only the entries that start with what was typed, as in Vim.
            if cmdline.history_index.is_none() {
                cmdline.draft = cmdline.input.clone();
            }
            let matching: Vec<&String> =
                history.iter().rev().filter(|line| line.starts_with(&cmdline.draft)).collect();
            let next = match (cmdline.history_index, older) {
                (None, true) => Some(0),
                (Some(index), true) => Some((index + 1).min(matching.len().saturating_sub(1))),
                (Some(0) | None, false) => None,
                (Some(index), false) => Some(index - 1),
            };
            match next.and_then(|index| matching.get(index).map(|line| (index, line))) {
                Some((index, line)) => {
                    cmdline.input = (*line).clone();
                    cmdline.history_index = Some(index);
                }
                None => {
                    cmdline.input = cmdline.draft.clone();
                    cmdline.history_index = None;
                }
            }
            cmdline.cursor = cmdline.input.chars().count();
            cmdline.show_completions = false;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// How many command lines are kept.
pub const HISTORY_LIMIT: usize = 200;

/// Adds a command line to the history: newest last, no repeats.
pub(crate) fn remember_command(state: &mut AppState, line: String) {
    state.command_history.retain(|old| *old != line);
    state.command_history.push(line);
    let excess = state.command_history.len().saturating_sub(HISTORY_LIMIT);
    state.command_history.drain(..excess);
}

pub(crate) fn execute_command(
    state: &mut AppState,
    command: crate::app::commands::Command,
) -> Vec<Effect> {
    use crate::app::commands::Command;
    match command {
        Command::Action(action) => apply(state, action),
        Command::Generate { passphrase, size } => {
            let request = &mut state.generator.request;
            request.passphrase = passphrase;
            if passphrase {
                request.words = size.unwrap_or(request.words).clamp(3, 20);
            } else {
                request.length = size.unwrap_or(request.length).clamp(5, 128);
            }
            vec![super::generate(state, true, true)]
        }
        Command::Message(level, text) => {
            state.message(level, text);
            Vec::new()
        }
        Command::SetFilter(filter) => {
            state.filter = filter.clone();
            state.list_row = 0;
            if let Some(index) =
                sidebar_entries(state).iter().position(|e| e.filter.as_ref() == Some(&filter))
            {
                state.sidebar_row = index;
            }
            Vec::new()
        }
        Command::Set { key, value, mode } => set_option(state, &key, value.as_deref(), mode),
        Command::Map { context, keys, action } => {
            session_map(state, context, keys, action);
            Vec::new()
        }
        Command::Unmap { context, keys } => {
            session_unmap(state, context, keys);
            Vec::new()
        }
        Command::None => Vec::new(),
    }
}

pub(crate) fn set_option(
    state: &mut AppState,
    key: &str,
    value: Option<&str>,
    mode: crate::app::commands::SetMode,
) -> Vec<Effect> {
    use crate::app::commands::SetMode;
    let dotted = resolve_setting_key(key);
    if dotted.is_empty() {
        state.message(MessageLevel::Error, format!("E518: Unknown option: {key}"));
        return Vec::new();
    }
    match mode {
        SetMode::Show => {
            let current = setting_value(state, &dotted);
            state.message(MessageLevel::Info, format!("{dotted}={current}"));
            Vec::new()
        }
        SetMode::Reset => {
            let default = default_config_value(&dotted);
            apply_setting(state, &dotted, &default)
        }
        SetMode::Toggle => {
            let current = setting_value(state, &dotted);
            let next = if current == "true" { "false" } else { "true" };
            apply_setting(state, &dotted, next)
        }
        SetMode::Set => {
            let Some(value) = value else {
                state.message(MessageLevel::Error, "E471: Argument required");
                return Vec::new();
            };
            apply_setting(state, &dotted, value)
        }
    }
}

pub(crate) fn session_map(
    state: &mut AppState,
    context: Option<String>,
    keys: Option<String>,
    action: Option<String>,
) {
    let Some(keys) = keys else { return };
    let Some(action_text) = action else { return };
    let contexts = context.unwrap_or_else(|| {
        state.contexts().first().cloned().unwrap_or_else(|| "global".to_owned())
    });
    let Ok(keys) = parse_keys(&keys) else {
        state.message(MessageLevel::Error, format!("Invalid key notation: {keys}"));
        return;
    };
    if !crate::app::actions::is_known(action_call_id(&action_text)) {
        state.message(MessageLevel::Error, format!("Unknown action: {action_text}"));
        return;
    }
    let call = crate::config::keymap::ActionCall::parse(&action_text, Source::Session);
    state.keymap.bind(&contexts, &keys, call);
    state.message(MessageLevel::Ok, format!("Mapped {} in {contexts}.", display_keys(&keys)));
}

pub(crate) fn action_call_id(action_text: &str) -> &str {
    action_text.split_whitespace().next().unwrap_or("")
}

pub(crate) fn session_unmap(state: &mut AppState, context: Option<String>, keys: Option<String>) {
    let Some(keys) = keys else { return };
    let contexts = context.unwrap_or_else(|| "global".to_owned());
    if let Ok(keys) = parse_keys(&keys) {
        state.keymap.unbind(&contexts, &keys);
        state.message(MessageLevel::Ok, format!("Unmapped {} in {contexts}.", display_keys(&keys)));
    }
}
