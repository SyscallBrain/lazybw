//! The `:` command line and session key mappings.

use super::{
    Action, AppState, Effect, Key, MessageLevel, Source, apply, apply_setting,
    default_config_value, display_keys, overlay_binding, parse_keys, resolve_setting_key,
    setting_value, sidebar_entries,
};

pub(crate) fn cmdline_key(state: &mut AppState, key: Key) -> Vec<Effect> {
    use crate::config::keymap::Code;
    match overlay_binding(state, "cmdline", key) {
        Some(Some(action)) => return apply(state, action),
        Some(None) => {
            state.cmdline = None;
            return Vec::new();
        }
        None => {}
    }
    let Some(cmdline) = &mut state.cmdline else { return Vec::new() };
    match key.code {
        Code::Esc => {
            state.cmdline = None;
            return Vec::new();
        }
        Code::Enter => return apply(state, Action::new("cmdline.execute")),
        Code::Tab => return apply(state, Action::new("cmdline.complete_next")),
        Code::BackTab => return apply(state, Action::new("cmdline.complete_prev")),
        Code::Up => return apply(state, Action::new("cmdline.history_prev")),
        Code::Down => return apply(state, Action::new("cmdline.history_next")),
        Code::Backspace => {
            cmdline.input.pop();
            cmdline.cursor = cmdline.cursor.saturating_sub(1);
        }
        Code::Left => cmdline.cursor = cmdline.cursor.saturating_sub(1),
        Code::Right => cmdline.cursor = (cmdline.cursor + 1).min(cmdline.input.chars().count()),
        Code::Char(c) if key.is_printable() => {
            let mut chars: Vec<char> = cmdline.input.chars().collect();
            let at = cmdline.cursor.min(chars.len());
            chars.insert(at, c);
            cmdline.cursor = at + 1;
            cmdline.input = chars.into_iter().collect();
        }
        _ => {}
    }
    refresh_completions(state);
    Vec::new()
}

pub(crate) fn refresh_completions(state: &mut AppState) {
    let Some(input) = state.cmdline.as_ref().map(|c| c.input.clone()) else { return };
    let completions = crate::app::commands::completions(&input, state);
    if let Some(cmdline) = &mut state.cmdline {
        cmdline.completions = completions;
        cmdline.completion_index = 0;
        cmdline.show_completions = !cmdline.completions.is_empty();
    }
}

pub(crate) fn cmdline_action(state: &mut AppState, action: &Action) -> Vec<Effect> {
    match action.id.as_str() {
        "cmdline.execute" => {
            let Some(cmdline) = state.cmdline.take() else { return Vec::new() };
            let command = crate::app::commands::parse(&cmdline.input);
            execute_command(state, command)
        }
        "cmdline.complete_next" | "cmdline.complete_prev" => {
            let Some(cmdline) = &mut state.cmdline else { return Vec::new() };
            if cmdline.completions.is_empty() {
                refresh_completions(state);
                return Vec::new();
            }
            if cmdline.completion_index == 0 {
                // Replace the last word with the first completion.
                let prefix_end =
                    cmdline.input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
                let head = cmdline.input[..prefix_end].to_owned();
                let selected = cmdline.completions[0].clone();
                cmdline.input = format!("{head}{selected}");
            } else {
                let prefix_end =
                    cmdline.input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
                let head = cmdline.input[..prefix_end].to_owned();
                let selected = cmdline.completions[cmdline.completion_index].clone();
                cmdline.input = format!("{head}{selected}");
            }
            cmdline.completion_index = (cmdline.completion_index + 1) % cmdline.completions.len();
            cmdline.cursor = cmdline.input.chars().count();
            Vec::new()
        }
        "cmdline.history_prev" | "cmdline.history_next" => Vec::new(),
        _ => Vec::new(),
    }
}

pub(crate) fn execute_command(
    state: &mut AppState,
    command: crate::app::commands::Command,
) -> Vec<Effect> {
    use crate::app::commands::Command;
    match command {
        Command::Action(action) => apply(state, action),
        Command::Message(level, text) => {
            state.message(level, text);
            Vec::new()
        }
        Command::SetFilter(filter) => {
            state.filter = filter.clone();
            state.list_row = 0;
            state.list_offset = 0;
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
