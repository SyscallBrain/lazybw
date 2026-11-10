//! The item editor form.

use crate::ui::{
    AppState, EditorState, FormKey, FormKind, Frame, LayoutPlan, Line, Paragraph, Rect, Span,
    Style, Widget, blank, draw_scrollbar, draw_sidebar, fill_line, follow, group_code, inner,
    key_hint, meter_spans, pad, pane_block, password_spans, section_label, spans_width,
    strength_label, text_line, truncate, type_label,
};

pub(crate) fn draw_editor(frame: &mut Frame<'_>, plan: &LayoutPlan, state: &AppState) {
    let Some(editor) = &state.editor else { return };
    let theme = &state.theme;
    let area = if plan.sidebar.width > 0 {
        draw_sidebar(frame, plan.sidebar, state);
        plan.list.union(plan.detail)
    } else {
        plan.body
    };
    let kind = type_label(editor.item.kind).to_lowercase();
    let mut title = if editor.is_new {
        format!("New {kind}")
    } else {
        format!("Edit {kind} · {}", editor.item.name)
    };
    if editor.dirty {
        title.push_str(" [+]");
    }
    let save = key_hint(state, "editor", "editor.save").unwrap_or_else(|| ":w".to_owned());
    let block = pane_block(title, format!("{save} save · :q close"), true, theme);
    let content = inner(area);
    if let Some(notes) = &state.notes {
        let notes_block = pane_block(
            format!("{} · Notes", editor.item.name),
            "Tab leave · Ctrl-e $EDITOR · Ctrl-s save",
            true,
            theme,
        );
        frame.render_widget(notes_block, area);
        let mut notes = notes.borrow_mut();
        edtui::EditorView::new(&mut notes.editor).render(content, frame.buffer_mut());
        return;
    }
    frame.render_widget(block, area);
    if editor.remote_change {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " This item changed on the server. :w overwrites it, :e! reloads it.",
                theme.style("warn"),
            )),
            Rect { height: 1, ..content },
        );
    }
    let content = if editor.remote_change {
        Rect { y: content.y + 1, height: content.height.saturating_sub(1), ..content }
    } else {
        content
    };
    if crate::ui::layout::editor_two_columns(content.width) {
        // Two columns: credentials and options on the left, lists and notes
        // on the right, separated by a rule.
        let left_width = content.width * 56 / 100;
        let left = Rect { width: left_width, ..content };
        let rule = Rect { x: content.x + left_width, width: 1, ..content };
        let right = Rect {
            x: content.x + left_width + 2,
            width: content.width.saturating_sub(left_width + 2),
            ..content
        };
        draw_form(frame, left, state, editor, |key| !is_list_row(key), "form_left", 22);
        let rule_lines: Vec<Line<'static>> = (0..rule.height)
            .map(|_| Line::from(Span::styled("│", theme.style("border"))))
            .collect();
        frame.render_widget(Paragraph::new(rule_lines), rule);
        draw_form(frame, right, state, editor, is_list_row, "form_right", 16);
    } else {
        draw_form(frame, content, state, editor, |_| true, "form", 22);
    }
}

/// Rows the wide editor shows in its right column.
pub(crate) fn is_list_row(key: FormKey) -> bool {
    key.in_right_column()
}

pub(crate) fn draw_form(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    editor: &EditorState,
    include: impl Fn(FormKey) -> bool,
    scroll_key: &'static str,
    label_width: usize,
) {
    let theme = &state.theme;
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut active_line = 0;
    for (index, row) in editor.rows.iter().enumerate() {
        if !include(row.key) {
            continue;
        }
        if let Some(section) = row.section {
            if !lines.is_empty() {
                lines.push(blank());
            }
            lines.push(section_label(theme, &format!(" {section}")));
        }
        let active = index == editor.row;
        if active {
            active_line = lines.len();
        }
        let value = crate::app::form::value(&editor.item, row.key, &state.cache);
        let row_style = if active { theme.active_row() } else { Style::default() };
        let mode = if editor.insert { "mode.insert" } else { "accent" };
        let label_style = if active { theme.bold(mode) } else { theme.style("fg.alt") };
        let mut left = vec![Span::styled(
            if active { "▌" } else { " " },
            theme.style(if active { mode } else { "accent" }),
        )];
        if row.kind == FormKind::Action {
            // A button row: its label is the whole row (no value column,
            // so it is neither repeated nor cut to the label width).
            left.push(Span::styled(
                format!(" {}", row.label),
                if active { theme.bold(mode) } else { theme.style("accent") },
            ));
        } else {
            left.push(Span::styled(format!(" {}", pad(&row.label, label_width)), label_style));
            left.extend(form_value_spans(state, editor, row, active));
        }
        let mut right = Vec::new();
        if row.key == FormKey::Totp
            && !value.is_empty()
            && let Ok(code) = crate::totp::current(value, state.now.timestamp() as u64)
        {
            right.push(Span::styled(
                group_code(&code.code, code.digits as usize),
                theme.style("totp.code"),
            ));
            right.push(Span::styled(" ✓ ", theme.style("ok")));
        }
        if active
            && state.config.ui.show_hints
            && let Some(hint) = &row.hint
        {
            right.push(Span::styled(format!("{hint} "), theme.style("fg.muted")));
        }
        if spans_width(&left) + spans_width(&right) > width {
            right.clear();
        }
        lines.push(fill_line(left, right, width, row_style));
        if let Some(error) = editor.errors.get(&index) {
            let token = if error.contains("Luhn") { "warn" } else { "error" };
            lines.push(text_line(theme, token, format!("{}{error}", " ".repeat(label_width + 2))));
        }
        if row.key == FormKey::Password && !value.is_empty() {
            let strength = crate::vault::health::local_strength(value);
            let mut spans = vec![Span::raw(" ".repeat(label_width + 2))];
            spans.extend(meter_spans(theme, strength));
            spans.push(Span::styled(
                format!(" {}", strength_label(strength)),
                theme.style("fg.muted"),
            ));
            lines.push(Line::from(spans));
        }
        if row.key == FormKey::Notes {
            let notes: Vec<&str> = value.lines().collect();
            let shown = notes.len().clamp(1, 8);
            for (number, line) in notes.iter().take(shown).enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {:>3} ", number + 1), theme.style("fg.subtle")),
                    Span::styled(truncate(line, width.saturating_sub(7)), theme.style("fg")),
                ]));
            }
            if notes.len() > shown {
                lines.push(text_line(
                    theme,
                    "fg.subtle",
                    format!("      … {} more lines", notes.len() - shown),
                ));
            } else if notes.is_empty() {
                lines.push(text_line(theme, "fg.subtle", "    ~"));
            }
        }
    }
    let visible = area.height as usize;
    let total = lines.len();
    let selected_here = editor.rows.get(editor.row).is_some_and(|row| include(row.key));
    let offset = if selected_here {
        follow(state, scroll_key, active_line + 1, visible, total)
    } else {
        state
            .ui_scroll
            .borrow()
            .get(scroll_key)
            .copied()
            .unwrap_or(0)
            .min(total.saturating_sub(visible))
    };
    let shown: Vec<Line<'static>> = lines.into_iter().skip(offset).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), area);
    // More fields than fit: a scrollbar on the column's right edge says so.
    if total > visible && area.width > 2 {
        let edge = Rect {
            x: area.x,
            y: area.y.saturating_sub(1),
            width: area.width,
            height: area.height + 2,
        };
        draw_scrollbar(frame, edge, offset, visible, total, selected_here, &state.theme);
    }
}

pub(crate) fn form_value_spans(
    state: &AppState,
    editor: &EditorState,
    row: &crate::app::state::FormRow,
    active: bool,
) -> Vec<Span<'static>> {
    let theme = &state.theme;
    let value = crate::app::form::value(&editor.item, row.key, &state.cache);
    let cursor = |mut spans: Vec<Span<'static>>| {
        if active && editor.insert {
            spans.push(Span::styled("▏", theme.style("mode.insert")));
        }
        spans
    };
    match &row.kind {
        FormKind::Secret => {
            if value.is_empty() {
                cursor(Vec::new())
            } else if editor.revealed && active {
                cursor(password_spans(theme, value, Style::default()))
            } else {
                cursor(vec![Span::styled(
                    "•".repeat(value.chars().count().min(32)),
                    theme.style("fg.alt"),
                )])
            }
        }
        FormKind::Bool => vec![if value == "true" {
            Span::styled("☑", theme.style("ok"))
        } else {
            Span::styled("☐", theme.style("fg.subtle"))
        }],
        FormKind::Option(_) => {
            vec![Span::styled(format!("‹ {} ›", value), theme.style("type.note"))]
        }
        FormKind::Action => vec![Span::styled(
            format!("[ {} ]", row.label.trim_start_matches("  + ").trim()),
            theme.style("accent"),
        )],
        FormKind::ReadOnly => vec![Span::styled(value.to_owned(), theme.style("fg.muted"))],
        FormKind::Notes => {
            let count = value.lines().count();
            vec![Span::styled(
                match count {
                    0 => "empty".to_owned(),
                    1 => "1 line".to_owned(),
                    n => format!("{n} lines"),
                },
                theme.style("fg.subtle"),
            )]
        }
        FormKind::Text => {
            if matches!(row.key, FormKey::FieldType(_)) {
                vec![Span::styled(format!("[{}]", value), theme.style("fg.muted"))]
            } else {
                cursor(vec![Span::styled(value.to_owned(), theme.style("fg"))])
            }
        }
    }
}
