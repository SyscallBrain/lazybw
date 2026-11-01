//! Generator and health tab actions.

use super::{
    Action, AppState, CommandKind, Effect, FormKind, HealthCategory, ItemType, MessageLevel,
    Pending, Screen, Tab, apply, copy_value, editable, reprompt_for, reprompt_required, request,
    select_item_by_id, switch_tab,
};

pub(crate) fn generator_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action.id.as_str() {
        "generator.regenerate" => {
            vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )]
        }
        "generator.copy" => {
            let value = state.generator.output.clone();
            if !value.is_empty() {
                return copy_value(state, &value, "password", "the generator");
            }
            Vec::new()
        }
        "generator.toggle_mode" => {
            state.generator.request.passphrase = !state.generator.request.passphrase;
            state.generator.row = 0;
            vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )]
        }
        "generator.increase" | "generator.decrease" | "generator.toggle_option" => {
            let delta: i16 = match action.id.as_str() {
                "generator.increase" => 1,
                "generator.decrease" => -1,
                _ => 0,
            };
            let row = state.generator.row;
            let req = &mut state.generator.request;
            let step = |value: u8, min: i16, max: i16, wrap: bool| -> u8 {
                let next = i16::from(value) + if delta == 0 { 1 } else { delta };
                if wrap && next > max { min as u8 } else { next.clamp(min, max) as u8 }
            };
            // Rows match the order drawn by `ui::draw_generator`.
            if req.passphrase {
                match row {
                    0 => req.words = step(req.words, 3, 20, false),
                    1 => {
                        const SEPARATORS: [&str; 5] = ["-", " ", ".", "_", ","];
                        let index =
                            SEPARATORS.iter().position(|s| *s == req.separator).unwrap_or(0);
                        let next = if delta < 0 { index + SEPARATORS.len() - 1 } else { index + 1 };
                        req.separator = SEPARATORS[next % SEPARATORS.len()].to_owned();
                    }
                    2 => req.capitalize = !req.capitalize,
                    3 => req.include_number = !req.include_number,
                    _ => {}
                }
            } else {
                match row {
                    0 => req.length = step(req.length, 5, 128, false),
                    1 => req.uppercase = !req.uppercase,
                    2 => req.lowercase = !req.lowercase,
                    3 => req.numbers = !req.numbers,
                    4 => req.special = !req.special,
                    5 => req.min_numbers = step(req.min_numbers, 0, 9, delta == 0),
                    6 => req.min_special = step(req.min_special, 0, 9, delta == 0),
                    7 => req.avoid_ambiguous = !req.avoid_ambiguous,
                    _ => {}
                }
                if !(req.uppercase || req.lowercase || req.numbers || req.special) {
                    req.lowercase = true;
                    state.message(MessageLevel::Warn, "At least one character set stays on.");
                }
            }
            vec![request(
                state,
                CommandKind::Generate(state.generator.request.clone()),
                Pending::Generate,
            )]
        }
        "generator.use" => {
            if state.generator.popup {
                let value = state.generator.output.clone();
                if let Some(editor) = &mut state.editor
                    && let Some(row) = editor.rows.get_mut(editor.row)
                    && matches!(row.kind, FormKind::Secret)
                {
                    row.value = value;
                    editor.dirty = true;
                    editor.revealed = true;
                }
                state.generator.popup = false;
                state.screen = Screen::Editor;
            } else {
                // From the tab: put the password into the login selected in
                // the vault list, after a confirmation.
                let value = state.generator.output.clone();
                let target = state.selected_item().cloned();
                match target {
                    Some(item) if item.kind == ItemType::Login && !value.is_empty() => {
                        if !editable(state, &item) {
                            return Vec::new();
                        }
                        let id = item.id.clone().unwrap_or_default();
                        let action = Action::with("generator.use.confirm", &[&id]);
                        if reprompt_required(state, &item) {
                            return reprompt_for(state, &item, action);
                        }
                        return apply(state, action);
                    }
                    _ => state.message(
                        MessageLevel::Info,
                        "Select a login in the Vault tab first, or press Ctrl-g in the editor.",
                    ),
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn health_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action.id.as_str() {
        "health.rerun" => {
            state.health.running = true;
            vec![request(state, CommandKind::Health, Pending::Health)]
        }
        "health.next_category" => {
            let categories = HealthCategory::ALL;
            let index = categories.iter().position(|c| *c == state.health.category).unwrap_or(0);
            state.health.category = categories[(index + 1) % categories.len()];
            state.health.index = 0;
            Vec::new()
        }
        "health.prev_category" => {
            let categories = HealthCategory::ALL;
            let index = categories.iter().position(|c| *c == state.health.category).unwrap_or(0);
            state.health.category = categories[(index + categories.len() - 1) % categories.len()];
            state.health.index = 0;
            Vec::new()
        }
        "health.open_item" => {
            if let Some(report) = &state.health.report
                && let Some(record) = report.items_in(state.health.category).get(state.health.index)
            {
                let id = record.id.clone();
                select_item_by_id(state, &id);
            }
            Vec::new()
        }
        "health.open" => switch_tab(state, Tab::Health),
        _ => Vec::new(),
    }
}
