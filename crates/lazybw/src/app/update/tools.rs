//! Generator and health tab actions.

use super::{
    Action, AppState, CommandKind, Deferred, Effect, FormKind, HealthCategory, ItemType,
    MessageLevel, Pending, Screen, Tab, copy_value, editable, generate, request, run,
    select_item_by_id, switch_tab,
};
use crate::app::actions::ActionId;

pub(crate) fn generator_action(state: &mut AppState, action: Action) -> Vec<Effect> {
    match action.id {
        ActionId::GeneratorRegenerate => {
            vec![generate(state, true, false)]
        }
        ActionId::GeneratorCopy => {
            let value = state.generator.output.clone();
            if !value.is_empty() {
                return copy_value(state, &value, "password", "the generator");
            }
            Vec::new()
        }
        ActionId::GeneratorToggleMode => {
            state.generator.request.passphrase = !state.generator.request.passphrase;
            state.generator.row = 0;
            vec![generate(state, true, false)]
        }
        ActionId::GeneratorIncrease
        | ActionId::GeneratorDecrease
        | ActionId::GeneratorToggleOption => {
            let delta: i16 = match action.id {
                ActionId::GeneratorIncrease => 1,
                ActionId::GeneratorDecrease => -1,
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
            vec![generate(state, false, false)]
        }
        ActionId::GeneratorUse => {
            if state.generator.popup {
                let value = state.generator.output.clone();
                if let Some(editor) = &mut state.editor
                    && let Some(row) = editor.rows.get(editor.row)
                    && matches!(row.kind, FormKind::Secret)
                {
                    let key = row.key;
                    crate::app::form::set(&mut editor.item, key, &value, &state.cache);
                    editor.dirty = true;
                    editor.revealed = true;
                }
                crate::app::form::wipe(value);
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
                        // Asks for the master password first when the item wants it.
                        return run(state, Deferred::ConfirmUseGenerated(id));
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
    match action.id {
        ActionId::HealthRerun if state.checking_health() => Vec::new(),
        ActionId::HealthRerun => vec![request(
            state,
            CommandKind::Health { check_exposed: state.config.health.check_exposed },
            Pending::Health,
        )],
        ActionId::HealthNextCategory => {
            let categories = HealthCategory::ALL;
            let index = categories.iter().position(|c| *c == state.health.category).unwrap_or(0);
            state.health.category = categories[(index + 1) % categories.len()];
            state.health.index = 0;
            Vec::new()
        }
        ActionId::HealthPrevCategory => {
            let categories = HealthCategory::ALL;
            let index = categories.iter().position(|c| *c == state.health.category).unwrap_or(0);
            state.health.category = categories[(index + categories.len() - 1) % categories.len()];
            state.health.index = 0;
            Vec::new()
        }
        ActionId::HealthOpenItem => {
            if let Some(report) = &state.health.report
                && let Some(record) = report.item_in(state.health.category, state.health.index)
            {
                let id = record.id.clone();
                select_item_by_id(state, &id);
            }
            Vec::new()
        }
        ActionId::HealthOpen => switch_tab(state, Tab::Health),
        _ => Vec::new(),
    }
}
