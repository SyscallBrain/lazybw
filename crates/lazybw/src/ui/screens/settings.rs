//! Settings, keymap editor, help and messages.

use crate::ui::{
    AppState, BTreeMap, BTreeSet, Block, BorderType, Borders, Frame, Line, MessageLevel, Paragraph,
    Rect, Span, Style, blank, centered, display_keys, draw_scrollbar, fill_line, follow, inner,
    pad, pane_block, popup_surface, section_label, text_line, truncate,
};

pub(crate) fn draw_settings(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let block = pane_block("Settings", "", true, theme);
    let content = inner(area);
    let width = content.width as usize;
    let rows = crate::app::update::settings_rows(state);
    let mut lines = Vec::new();
    let mut selected_line = 0;
    for (index, row) in rows.iter().enumerate() {
        if let Some(section) = row.section {
            if !lines.is_empty() {
                lines.push(blank());
            }
            lines.push(section_label(theme, &format!(" {section}")));
        }
        let selected = index == state.settings.index;
        if selected {
            selected_line = lines.len();
        }
        let style = if selected { theme.selected_row() } else { Style::default() };
        let value: Vec<Span<'static>> = match &row.kind {
            crate::app::state::SettingKind::Bool => {
                if row.value == "true" {
                    vec![Span::styled("☑", theme.style("ok"))]
                } else {
                    vec![Span::styled("☐", theme.style("fg.subtle"))]
                }
            }
            crate::app::state::SettingKind::Option(_) => {
                let shown = if row.value == " " { "space" } else { row.value.as_str() };
                vec![Span::styled(format!("‹ {shown} ›"), theme.style("type.note"))]
            }
            crate::app::state::SettingKind::Action if row.key == "ui.theme" => {
                vec![Span::styled(format!("‹ {} ›", row.value), theme.style("type.note"))]
            }
            crate::app::state::SettingKind::Text if row.value.is_empty() => {
                vec![Span::styled("$VISUAL / $EDITOR", theme.style("fg.subtle"))]
            }
            _ if row.key == "security.reprompt_grace"
                && matches!(row.value.as_str(), "0s" | "0ms") =>
            {
                vec![
                    Span::styled("0s", theme.style("fg")),
                    Span::styled("  ask every time", theme.style("fg.subtle")),
                ]
            }
            _ => vec![Span::styled(row.value.clone(), theme.style("fg"))],
        };
        let modified = row.key != "keybindings"
            && crate::app::update::default_config_value(row.key) != row.value;
        let hint = match &row.kind {
            crate::app::state::SettingKind::Action if row.key == "ui.theme" => "Enter pick ",
            crate::app::state::SettingKind::Action => "Enter open ",
            crate::app::state::SettingKind::Text => ":set to change ",
            _ => "",
        };
        let mut left = vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(if modified { "●" } else { " " }, theme.style("accent")),
            Span::styled(
                format!("{:<28}", row.label),
                if selected { theme.bold("fg") } else { theme.style("fg") },
            ),
        ];
        left.extend(value);
        let right = if selected && !hint.is_empty() {
            vec![Span::styled(hint, theme.style("fg.muted"))]
        } else {
            Vec::new()
        };
        lines.push(fill_line(left, right, width, style));
    }
    frame.render_widget(block, area);
    let visible = content.height.saturating_sub(3) as usize;
    let total = lines.len();
    let offset = follow(state, "settings", selected_line, visible, total);
    let shown: Vec<Line<'static>> = lines.into_iter().skip(offset).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), Rect { height: visible as u16, ..content });
    let description = rows
        .get(state.settings.index)
        .and_then(|row| setting_description(row.key))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {description}"), theme.style("fg.alt")))),
        Rect { y: content.y + content.height.saturating_sub(2), height: 1, ..content },
    );
    let hint = Line::from(vec![
        Span::styled(" ●", theme.style("accent")),
        Span::styled(" changed · ", theme.style("fg.muted")),
        Span::styled("h l", theme.style("hint.key")),
        Span::styled(" change · ", theme.style("fg.muted")),
        Span::styled("r", theme.style("hint.key")),
        Span::styled(" reset · ", theme.style("fg.muted")),
        Span::styled("Esc", theme.style("hint.key")),
        Span::styled(" close · changes are saved to ", theme.style("fg.muted")),
        Span::styled(
            state
                .paths
                .config_file()
                .display()
                .to_string()
                .replace(&std::env::var("HOME").unwrap_or_else(|_| "\u{0}".to_owned()), "~"),
            theme.style("fg.subtle"),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(hint),
        Rect { y: content.y + content.height.saturating_sub(1), height: 1, ..content },
    );
    draw_scrollbar(
        frame,
        Rect { height: area.height - 1, ..area },
        offset,
        visible,
        total,
        true,
        theme,
    );
}

pub(crate) fn draw_keymap_editor(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let context = state
        .keymap_editor
        .contexts
        .get(state.keymap_editor.context_index)
        .cloned()
        .unwrap_or_else(|| "All".to_owned());
    let title = if state.keymap_editor.dirty { "Keybindings [+]" } else { "Keybindings" };
    let block = pane_block(title, format!("context ‹ {context} › Tab"), true, theme);
    let rows = crate::app::update::keymap_rows(state);
    let mut lines = Vec::new();
    if !state.keymap_editor.filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("/ ", theme.style("accent")),
            Span::styled(state.keymap_editor.filter.clone(), theme.style("fg")),
        ]));
    }
    lines.push(section_label(
        theme,
        "  DESCRIPTION                      ACTION                    KEYS",
    ));
    // The header (and filter) stay put; the rows scroll with the cursor.
    let inner_area = inner(area);
    let visible = (inner_area.height as usize).saturating_sub(lines.len()).max(1);
    let offset = follow(state, "keymap_editor", state.keymap_editor.index, visible, rows.len());
    for (index, (description, action, keys, _context)) in
        rows.iter().enumerate().skip(offset).take(visible)
    {
        let selected = index == state.keymap_editor.index;
        let style = if selected { theme.selected_row() } else { theme.style("fg") };
        lines.push(Line::from(vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(format!(" {:<32}", truncate(description, 32)), style),
            Span::styled(format!("{:<26}", truncate(action, 26)), theme.style("fg.muted")),
            Span::styled(keys.clone(), theme.style("accent")),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    draw_scrollbar(frame, area, offset, visible, rows.len(), true, theme);
    if let Some(capture) = &state.keymap_editor.capture {
        let rect = centered(area, 62, 9);
        popup_surface(frame, rect, theme);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.colour("border.focus")))
            .style(Style::default().bg(theme.colour("bg.float")))
            .title(Span::styled(
                format!(" Rebind “{}” · {} ", capture.action, capture.context),
                theme.bold("accent"),
            ));
        let keys = capture.keys.join(" ");
        let mut lines = vec![Line::from(vec![
            Span::styled("Press the new keys:   ", theme.style("fg")),
            Span::styled(keys, theme.bold("accent")),
            Span::styled("▏", theme.style("accent")),
        ])];
        if let Some(conflict) = &capture.conflict {
            lines.push(Line::from(Span::styled(
                format!("⚠ {conflict} Replace it? y / n"),
                theme.style("warn"),
            )));
        }
        lines.extend([
            blank(),
            Line::from(Span::styled(
                "Enter confirm · Backspace remove last key · Esc cancel",
                theme.style("fg.muted"),
            )),
            Line::from(Span::styled(
                "To bind Enter, Backspace or Escape, edit config.toml.",
                theme.style("fg.subtle"),
            )),
        ]);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }
}

pub(crate) fn draw_help(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let block = pane_block("Help", "/ search · q close", true, theme);
    let content = inner(area);
    let width = content.width as usize;
    let mut chain = state.help_chain.clone();
    if chain.is_empty() {
        chain.push("global".to_owned());
    }
    let needle = state.help_filter.to_lowercase();
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" ❯ ", theme.style("type.note")),
        if needle.is_empty() {
            Span::styled("press / to search", theme.style("fg.subtle"))
        } else {
            Span::styled(state.help_filter.clone(), theme.style("fg"))
        },
    ]));
    if !state.keyboard_enhanced {
        lines.push(text_line(
            theme,
            "fg.subtle",
            " This terminal reports Ctrl-i as Tab, so <C-i> falls back to <Tab>.",
        ));
    }
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut shown = 0;
    for context in &chain {
        // Effective keys per action in this context; a key already taken by
        // an earlier (more specific) context is shadowed and not listed.
        let mut by_action: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for (keys, call) in state.keymap.bindings_in(context) {
            let notation = display_keys(&keys);
            if !seen.insert((notation.clone(), String::new())) && chain.first() != Some(context) {
                continue;
            }
            let text = call.as_text();
            by_action
                .entry(text.clone())
                .or_insert_with(|| (crate::config::keymap::action_description(&call), Vec::new()))
                .1
                .push(notation);
        }
        let mut rows: Vec<(String, String, Vec<String>)> = by_action
            .into_iter()
            .map(|(action, (description, keys))| (description, action, keys))
            .filter(|(description, action, keys)| {
                needle.is_empty()
                    || description.to_lowercase().contains(&needle)
                    || action.to_lowercase().contains(&needle)
                    || keys.iter().any(|k| k.to_lowercase().contains(&needle))
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort();
        lines.push(blank());
        lines.push(section_label(theme, &format!(" {}", context.to_uppercase().replace('_', " "))));
        for (description, action, keys) in rows {
            shown += 1;
            lines.push(fill_line(
                vec![
                    Span::styled(format!("  {}", pad(&keys.join("  "), 20)), theme.bold("accent")),
                    Span::styled(description, theme.style("fg")),
                ],
                vec![Span::styled(format!("{action} "), theme.style("fg.subtle"))],
                width,
                Style::default(),
            ));
        }
    }
    if shown == 0 {
        lines.push(blank());
        lines.push(text_line(theme, "fg.muted", "  No binding matches the search."));
    }
    let visible = content.height as usize;
    let total = lines.len();
    let offset = state.help_offset.min(total.saturating_sub(visible));
    state.ui_pages.borrow_mut().insert("help", visible);
    state.ui_scroll.borrow_mut().insert("help.max", total.saturating_sub(visible));
    let lines: Vec<Line<'static>> = lines.into_iter().skip(offset).take(visible).collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
    draw_scrollbar(frame, area, offset, visible, total, true, theme);
}

pub(crate) fn draw_messages(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let block = pane_block("Messages", "Esc close", true, theme);
    let mut lines = Vec::new();
    let visible = inner(area).height as usize;
    let max = state.messages.len().saturating_sub(visible);
    state.ui_pages.borrow_mut().insert("messages", visible);
    state.ui_scroll.borrow_mut().insert("messages.max", max);
    for message in state.messages.iter().skip(state.messages_offset.min(max)).take(visible) {
        let token = match message.level {
            MessageLevel::Info => "fg.alt",
            MessageLevel::Ok => "ok",
            MessageLevel::Warn => "warn",
            MessageLevel::Error => "error",
        };
        lines.push(Line::from(vec![
            Span::styled(message.time.format("%H:%M:%S ").to_string(), theme.style("fg.subtle")),
            Span::styled(message.text.clone(), theme.style(token)),
        ]));
    }
    if lines.is_empty() {
        lines.push(text_line(theme, "fg.muted", "No messages yet."));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One line about what a setting does, shown for the selected row.
fn setting_description(key: &str) -> Option<&'static str> {
    Some(match key {
        "ui.theme" => "Colours of the whole interface; Enter previews each theme.",
        "ui.transparent" => "Leave the background to the terminal (popups stay solid).",
        "ui.icons" => "Nerd Font icons, plain Unicode symbols, or ASCII only.",
        "ui.animations" => "Short fades and slides when panes and popups open.",
        "ui.list_density" => "Two lines per item (with the username) or one.",
        "ui.statusline_separator" => "The shape between the status line segments.",
        "ui.show_hints" => "Key hints next to the selected field and row.",
        "ui.date_format" => "\"3 days ago\" or ISO dates.",
        "ui.mouse" => "Clicks, scrolling and dragging pane borders.",
        "security.clipboard_timeout" => "Copied values are cleared after this long.",
        "security.auto_lock" => "Lock the vault after this long without a key press.",
        "security.reveal_timeout" => "Revealed passwords hide again after this long.",
        "security.hide_on_focus_lost" => "Hide revealed values when the terminal loses focus.",
        "security.totp" => "Show verification codes computed from authenticator keys.",
        "security.clear_clipboard_on_exit" => "Clear a value lazybw copied when it quits.",
        "security.reprompt_grace" => {
            "After a correct master password re-prompt, don't ask again for this long."
        }
        "vault.sync_interval" => "How often to sync with the server in the background.",
        "vault.sort" => "The order of the item list.",
        "vault.search_notes" => "Let search look inside notes as well.",
        "vault.show_empty_types" => "List item types that have no items.",
        "vault.confirm_trash" => "Ask before dd moves an item to the trash (u undoes it).",
        "generator.type" => "What the generator makes by default.",
        "health.check_exposed" => {
            "Look passwords up in Have I Been Pwned (only partial hashes leave the machine)."
        }
        "health.old_password_age" => "Passwords unchanged for longer than this count as old.",
        "editor.command" => "The editor for Ctrl-e; empty means $VISUAL, then $EDITOR.",
        key if key.starts_with("generator.") => "A default for the password generator.",
        "keybindings" => "Change any key; Enter opens the keybinding editor.",
        _ => return None,
    })
}
