//! Popups and wizards drawn over the current screen.

use crate::ui::{
    Alignment, AppState, Block, BorderType, Borders, Constraint, Direction, ExportFormat, Frame,
    Icons, ItemType, Layout, Line, Paragraph, Rect, Span, Style, Theme, Wrap, blank,
    detail_content, draw_generator_popup, draw_scrollbar, draw_wildmenu, fill_line, follow,
    hints_line, icons, inner, meter_spans, pad, pane_block, popup_frame, popup_surface,
    quick_open_rect, section_label, spinner_frame, strength_label, text_line, tick, truncate,
};

pub(crate) fn draw_overlays(frame: &mut Frame<'_>, area: Rect, body: Rect, state: &AppState) {
    use crate::app::state::Overlay;
    if state.overlay_open() && body.width > 0 {
        // Dim everything behind a modal by 55% toward the bar background.
        let style = crate::ui::fx::Fx::backdrop_style(&state.theme, 1.0);
        frame.buffer_mut().set_style(body, style);
    }
    if state.generator_popup_open() {
        draw_generator_popup(frame, body, state);
    }
    match &state.overlay {
        Some(Overlay::QuickOpen(_)) => draw_quick_open(frame, area, state),
        Some(Overlay::Cmdline(_)) => draw_wildmenu(frame, area, state),
        Some(Overlay::Export(export)) => draw_export(frame, area, state, export),
        Some(Overlay::Import(import)) => draw_import(frame, area, state, import),
        Some(Overlay::Picker(picker)) => draw_picker(frame, area, state, picker),
        Some(Overlay::Confirm(confirm)) => draw_confirm(frame, area, state, confirm),
        Some(Overlay::Prompt(prompt)) => draw_prompt(frame, area, state, prompt),
        Some(Overlay::Reprompt(reprompt)) => draw_reprompt(frame, area, state, reprompt),
        Some(Overlay::History(history)) => draw_history(frame, area, state, history),
        Some(Overlay::Info(info)) => draw_info(frame, area, state, info),
        None => {}
    }
}

pub(crate) fn draw_quick_open(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let Some(quick) = state.quick_open() else { return };
    let theme = &state.theme;
    let rect = quick_open_rect(area);
    popup_surface(frame, rect, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.colour("border.focus")))
        .style(Style::default().bg(theme.colour("bg.float")))
        .title(Span::styled(" Find ", theme.bold("accent")))
        .title_top(
            Line::from(Span::styled(
                format!(" {} items ", state.cache.active_items().count()),
                theme.style("fg.muted"),
            ))
            .alignment(Alignment::Right),
        );
    let inner_area = inner(rect);
    let split = if inner_area.width >= 100 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(Rect { height: inner_area.height.saturating_sub(1), ..inner_area })
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(Rect { height: inner_area.height.saturating_sub(1), ..inner_area })
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("❯ ", theme.style("type.note")),
            Span::styled(quick.input.clone(), theme.style("fg")),
            Span::styled("▏", theme.style("accent")),
        ]),
        Line::from(Span::styled("─".repeat(split[0].width as usize), theme.style("border"))),
    ];
    let icons = icons(state);
    let list_width = split[0].width as usize;
    let visible = (split[0].height as usize).saturating_sub(2);
    let offset = follow(state, "quick_open", quick.index, visible, quick.results.len());
    // The match count sits at the right of the prompt line.
    lines[0] = fill_line(
        std::mem::take(&mut lines[0].spans),
        vec![Span::styled(
            format!("{} / {} ", quick.results.len(), state.cache.active_items().count()),
            theme.style("fg.muted"),
        )],
        list_width,
        Style::default(),
    );
    for (index, id) in quick.results.iter().enumerate().skip(offset).take(visible) {
        let Some(row) = state.cache.row(id) else { continue };
        let selected = index == quick.index;
        let style = if selected { theme.selected_row() } else { Style::default() };
        let folder = truncate(&row.folder_path, 24);
        let room = list_width.saturating_sub(crate::util::text::width(&folder) + 6);
        let name = truncate(&row.name, room);
        let mut left = vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(
                format!("{} ", icons.get(Icons::item(row.kind))),
                theme.style(Theme::type_token(row.kind)),
            ),
            Span::styled(name.clone(), if selected { theme.bold("fg") } else { theme.style("fg") }),
        ];
        // Without the preview pane, the subtitle (username, card ending) is
        // what tells two "Gmail" entries apart.
        let subtitle_room = room.saturating_sub(crate::util::text::width(&name) + 2);
        if split[1].width == 0 && !row.subtitle.is_empty() && subtitle_room > 4 {
            left.push(Span::styled(
                format!("  {}", truncate(&row.subtitle, subtitle_room)),
                theme.style("fg.subtle"),
            ));
        }
        lines.push(fill_line(
            left,
            vec![Span::styled(format!("{folder} "), theme.style("fg.muted"))],
            list_width,
            style,
        ));
    }
    if quick.results.is_empty() {
        lines.push(text_line(theme, "fg.muted", "No matches."));
    }
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), split[0]);
    let footer =
        Rect { y: inner_area.y + inner_area.height.saturating_sub(1), height: 1, ..inner_area };
    frame.render_widget(
        Paragraph::new(hints_line(
            state,
            "quick_open",
            &[
                ("quick_open.accept", "open"),
                ("quick_open.copy_password", "password"),
                ("quick_open.copy_username", "username"),
                ("quick_open.copy_totp", "code"),
                ("quick_open.edit", "edit"),
                ("app.cancel", "close"),
            ],
            footer.width as usize,
        )),
        footer,
    );
    if split[1].width > 10 {
        let preview = state
            .cache
            .row(quick.results.get(quick.index).map_or("", String::as_str))
            .and_then(|row| state.cache.item(&row.id));
        let block = pane_block("Preview", "", false, theme);
        if let Some(item) = preview {
            let lines = detail_content(state, item, split[1].width as usize - 2).lines;
            frame.render_widget(Paragraph::new(lines).block(block), split[1]);
        } else {
            frame.render_widget(block, split[1]);
        }
    }
}

pub(crate) fn draw_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    picker: &crate::app::state::PickerState,
) {
    let theme = &state.theme;
    const MAX_ROWS: usize = 12;
    let height = (picker.filtered.len().min(MAX_ROWS) as u16) + 4;
    // Row: bar (1) + accelerator (2) + label column (32) + note, inside
    // the border (2) with one column of air on the right.
    let widest_note =
        picker.entries.iter().map(|e| crate::util::text::width(&e.note)).max().unwrap_or(0);
    let width = (1 + 2 + 32 + widest_note + 2 + 1).max(40) as u16;
    let (rect, block) = popup_frame(frame, &picker.title, theme, area, width, height);
    let mut lines = Vec::new();
    if !picker.filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("❯ ", theme.style("type.note")),
            Span::styled(picker.filter.clone(), theme.style("fg")),
        ]));
    }
    // The rows that fit under the filter line; the window follows the
    // cursor so it never moves past the last visible entry.
    let filter_rows = usize::from(!picker.filter.is_empty());
    let visible =
        (rect.height.saturating_sub(2) as usize).saturating_sub(filter_rows).clamp(1, MAX_ROWS);
    let total = picker.filtered.len();
    let offset = follow(state, "picker", picker.index, visible, total);
    for (position, entry_index) in picker.filtered.iter().enumerate().skip(offset).take(visible) {
        let Some(entry) = picker.entries.get(*entry_index) else { continue };
        let selected = position == picker.index;
        let style = if selected { theme.selected_row() } else { theme.style("fg") };
        let accelerator =
            entry.accelerator.map(|c| format!("{c} ")).unwrap_or_else(|| "  ".to_owned());
        lines.push(Line::from(vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(accelerator, theme.style("accent")),
            Span::styled(format!("{:<32}", truncate(&entry.label, 32)), style),
            Span::styled(entry.note.clone(), theme.style("fg.subtle")),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).block(block), rect);
    draw_scrollbar(frame, rect, offset, visible, total, true, theme);
}

pub(crate) fn draw_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    confirm: &crate::app::state::ConfirmState,
) {
    let theme = &state.theme;
    let width: u16 = 72;
    // Message (wrapped), a blank, the optional word and subfolder rows, the
    // key/button line, inside the border.
    let message_rows =
        crate::util::text::width(&confirm.message).div_ceil(width as usize - 2).max(1);
    let rows = message_rows
        + 1
        + if confirm.requires_word.is_some() { 2 } else { 0 }
        + usize::from(confirm.subfolder_count > 0)
        + 1;
    let height = rows as u16 + 2;
    let (rect, block) = popup_frame(frame, &confirm.title, theme, area, width, height);
    let mut lines =
        vec![Line::from(Span::styled(confirm.message.clone(), theme.style("fg"))), blank()];
    if let Some(word) = &confirm.requires_word {
        lines.push(Line::from(vec![
            Span::styled(format!("Type {word}: "), theme.style("warn")),
            Span::styled(confirm.input.clone(), theme.style("fg")),
            Span::styled("▏", theme.style("accent")),
        ]));
        lines.push(blank());
    }
    if confirm.subfolder_count > 0 {
        lines.push(Line::from(vec![
            Span::styled(if confirm.also_subfolders { "☑ " } else { "☐ " }, theme.style("accent")),
            Span::styled(
                format!(
                    "Also {} {} subfolder(s)   (space toggles)",
                    if confirm.confirm_label == "Delete" { "delete" } else { "rename" },
                    confirm.subfolder_count
                ),
                theme.style("fg"),
            ),
        ]));
    }
    let style = if confirm.danger { theme.bold("error") } else { theme.bold("accent") };
    let key = |k: &'static str, text: String| {
        vec![Span::styled(k, theme.style("hint.key")), Span::styled(text, theme.style("fg.muted"))]
    };
    let mut hints = key("y", format!(" {} · ", confirm.confirm_label.to_lowercase()));
    let buttons = if confirm.on_deny.is_some() {
        hints.extend(key("n", " discard · ".to_owned()));
        hints.extend(key("c", " cancel".to_owned()));
        format!("[ Cancel ]   [ Discard ]   [ {} ]", confirm.confirm_label)
    } else {
        hints.extend(key("n", " cancel".to_owned()));
        format!("[ Cancel ]   [ {} ]", confirm.confirm_label)
    };
    // Keys on the left, buttons on the right, on one line.
    lines.push(fill_line(
        hints,
        vec![Span::styled(buttons, style)],
        rect.width.saturating_sub(2) as usize,
        Style::default(),
    ));
    frame.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), rect);
}

pub(crate) fn draw_prompt(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    prompt: &crate::app::state::PromptState,
) {
    let theme = &state.theme;
    let (rect, block) = popup_frame(frame, &prompt.title, theme, area, 62, 5);
    let lines = vec![Line::from(vec![
        Span::styled("› ", theme.style("type.note")),
        Span::styled(prompt.input.text(), theme.style("fg")),
        Span::styled("▏", theme.style("accent")),
    ])];
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

pub(crate) fn draw_reprompt(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    reprompt: &crate::app::state::RepromptState,
) {
    let theme = &state.theme;
    let (rect, block) = popup_frame(frame, "Master password required", theme, area, 62, 7);
    let lines = vec![
        Line::from(Span::styled(
            format!("“{}” asks for your master password.", reprompt.item_name),
            theme.style("fg"),
        )),
        blank(),
        Line::from(vec![
            Span::styled("●".repeat(reprompt.password.char_count().min(40)), theme.style("fg")),
            Span::styled("▏", theme.style("accent")),
        ]),
        if state.verifying_reprompt() {
            Line::from(Span::styled(
                format!("{} Checking…", spinner_frame(tick(state))),
                theme.style("info"),
            ))
        } else {
            Line::from(Span::styled(
                reprompt.error.clone().unwrap_or_else(|| "Enter confirm · Esc cancel".to_owned()),
                if reprompt.error.is_some() {
                    theme.style("error")
                } else {
                    theme.style("fg.subtle")
                },
            ))
        },
    ];
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

pub(crate) fn draw_history(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    history: &crate::app::state::HistoryState,
) {
    let theme = &state.theme;
    let (rect, block) = popup_frame(frame, "Password history", theme, area, 64, 12);
    let mut lines = Vec::new();
    if let Some(current) = &history.current {
        lines.push(Line::from(vec![
            Span::styled("current   ", theme.style("fg.muted")),
            Span::styled("•".repeat(current.chars().count().min(24)), theme.style("fg")),
        ]));
    }
    for (index, entry) in history.entries.iter().enumerate() {
        let selected = index == history.index;
        let shown = if history.revealed.contains(&index) {
            entry.password.expose().to_owned()
        } else {
            "•".repeat(entry.password.expose().chars().count().min(24))
        };
        let used = entry
            .last_used_date
            .as_deref()
            .and_then(|d| {
                crate::util::time::date(state.now, d, state.config.ui.date_format.is_iso())
            })
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(
                format!(" {shown:<26}"),
                if selected { theme.selected_row() } else { theme.style("fg") },
            ),
            Span::styled(format!("used until {used}"), theme.style("fg.subtle")),
        ]));
    }
    lines
        .push(Line::from(Span::styled("za reveal · yy copy · Esc close", theme.style("fg.muted"))));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

pub(crate) fn draw_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    info: &crate::app::state::InfoState,
) {
    let theme = &state.theme;
    let (rect, block) =
        popup_frame(frame, "Item details", theme, area, 72, (info.lines.len() as u16 + 3).min(20));
    let lines: Vec<Line<'static>> = info
        .lines
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!(" {:<18}", label), theme.style("fg.muted")),
                Span::styled(value.clone(), theme.style("fg")),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

pub(crate) fn draw_export(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    export: &crate::app::state::ExportState,
) {
    use crate::app::state::ExportStep;
    let theme = &state.theme;
    let total = if export.format.needs_password() { 3 } else { 2 };
    let step = if export.step.choosing_format() {
        1
    } else if export.step.choosing_password() {
        2
    } else {
        total
    };
    let (rect, block) = popup_frame(frame, "Export vault", theme, area, 76, 16);
    let block = block.title_top(
        Line::from(Span::styled(format!(" {step}/{total} "), theme.style("fg.muted")))
            .alignment(Alignment::Right),
    );
    let width = rect.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    let field = |focused: bool, label: &str, value: Line<'static>| -> Line<'static> {
        let mut spans = vec![
            Span::styled(if focused { "▌" } else { " " }, theme.style("accent")),
            Span::styled(format!("{label:<10}"), theme.style("fg.muted")),
        ];
        spans.extend(value.spans);
        if focused {
            spans.push(Span::styled("▏", theme.style("accent")));
        }
        fill_line(
            spans,
            Vec::new(),
            width,
            if focused { theme.active_row() } else { Style::default() },
        )
    };
    let secret = |value: &str| {
        Line::from(Span::styled("•".repeat(value.chars().count().min(40)), theme.style("fg")))
    };
    if export.step.choosing_format() {
        lines.push(section_label(theme, " FORMAT"));
        for format in [
            ExportFormat::Json,
            ExportFormat::PasswordProtectedJson,
            ExportFormat::AccountEncryptedJson,
            ExportFormat::Csv,
        ] {
            let selected = format == export.format;
            let note = match format {
                ExportFormat::Json => "readable, every field",
                ExportFormat::PasswordProtectedJson => "for backups",
                ExportFormat::AccountEncryptedJson => "this account only",
                ExportFormat::Csv => "logins and notes only",
            };
            lines.push(fill_line(
                vec![
                    Span::styled(
                        if selected && export.step == ExportStep::Format { "▌" } else { " " },
                        theme.style("accent"),
                    ),
                    Span::styled(
                        if selected { "◉ " } else { "○ " },
                        theme.style(if selected { "accent" } else { "fg.subtle" }),
                    ),
                    Span::styled(
                        format.label().to_owned(),
                        if selected { theme.bold("fg") } else { theme.style("fg") },
                    ),
                ],
                vec![Span::styled(format!("{note} "), theme.style("fg.muted"))],
                width,
                Style::default(),
            ));
        }
        lines.push(section_label(theme, " DESTINATION"));
        lines.push(field(
            export.step == ExportStep::Destination,
            "File",
            Line::from(Span::styled(export.path.clone(), theme.style("fg"))),
        ));
        if export.format.is_plaintext() {
            lines.push(text_line(
                theme,
                "warn",
                " ⚠ This file will contain every password in plain text. Keep it on an",
            ));
            lines.push(text_line(
                theme,
                "warn",
                "   encrypted disk and delete it when you no longer need it.",
            ));
        } else {
            lines.push(blank());
            lines.push(blank());
        }
        lines.push(text_line(
            theme,
            "fg.subtle",
            " j k format · Tab field / complete path · Enter next · Esc cancel",
        ));
    } else if export.step.choosing_password() {
        lines.push(text_line(
            theme,
            "fg",
            " Choose a password for the file. You need it to import it again.",
        ));
        lines.push(blank());
        lines.push(field(
            export.step == ExportStep::FilePassword,
            "Password",
            secret(export.password.expose()),
        ));
        lines.push(field(
            export.step == ExportStep::ConfirmPassword,
            "Confirm",
            secret(export.confirmation.expose()),
        ));
        if !export.password.is_empty() {
            let strength = crate::vault::health::local_strength(export.password.expose());
            let mut spans = vec![Span::raw("           ")];
            spans.extend(meter_spans(theme, strength));
            spans.push(Span::styled(
                format!(" {}", strength_label(strength)),
                theme.style("fg.muted"),
            ));
            lines.push(Line::from(spans));
        }
        lines.push(blank());
        lines.push(text_line(theme, "fg.subtle", " Tab switch field · Enter next · Esc cancel"));
    } else {
        lines.push(text_line(theme, "fg", " Confirm with your master password."));
        lines.push(text_line(
            theme,
            "fg.muted",
            format!(" {} → {}", export.format.label(), export.path),
        ));
        lines.push(blank());
        lines.push(field(true, "Master", secret(export.master.expose())));
        lines.push(blank());
        lines.push(text_line(theme, "fg.subtle", " Enter export · Esc cancel"));
    }
    lines.push(blank());
    if state.exporting() {
        lines.push(Line::from(Span::styled(
            format!(" {} Exporting…", spinner_frame(tick(state))),
            theme.style("info"),
        )));
    } else if let Some(error) = &export.error {
        lines.push(Line::from(Span::styled(format!(" {error}"), theme.style("error"))));
    }
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

pub(crate) fn draw_import(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    import: &crate::app::state::ImportState,
) {
    use crate::app::state::ImportStep;
    let theme = &state.theme;
    let title = match import.step {
        ImportStep::Format => "Import · Format".to_owned(),
        ImportStep::Path | ImportStep::Password => "Import · File".to_owned(),
        ImportStep::Preview(_) => "Import · Preview".to_owned(),
    };
    let total = if import.format == "bitwardenjson" { 3 } else { 2 };
    let height = match &import.step {
        ImportStep::Format => 20,
        ImportStep::Path | ImportStep::Password => 11,
        ImportStep::Preview(preview) => (14 + preview.duplicates.len().min(8) as u16).min(26),
    };
    let (rect, block) = popup_frame(frame, &title, theme, area, 80, height);
    let block = block.title_top(
        Line::from(Span::styled(
            format!(" {}/{total} ", import.step.number().min(total)),
            theme.style("fg.muted"),
        ))
        .alignment(Alignment::Right),
    );
    let width = rect.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    let hint = |pairs: &[(&str, &str)]| -> Line<'static> {
        let mut spans = vec![Span::raw(" ")];
        for (index, (key, text)) in pairs.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(" · ", theme.style("fg.subtle")));
            }
            spans.push(Span::styled((*key).to_owned(), theme.style("hint.key")));
            spans.push(Span::styled(format!(" {text}"), theme.style("fg.muted")));
        }
        Line::from(spans)
    };
    match &import.step {
        ImportStep::Format => {
            lines.push(Line::from(vec![
                Span::styled(" ❯ ", theme.style("type.note")),
                Span::styled(import.filter.clone(), theme.style("fg")),
                Span::styled("▏", theme.style("accent")),
            ]));
            lines.push(Line::from(Span::styled(
                format!(" {}", "─".repeat(width.saturating_sub(2))),
                theme.style("border"),
            )));
            let formats = import.filtered_formats();
            let visible = (height as usize).saturating_sub(7);
            let offset =
                follow(state, "import_formats", import.format_index, visible, formats.len());
            for (index, format) in formats.iter().enumerate().skip(offset).take(visible) {
                let selected = index == import.format_index;
                let note = match *format {
                    "bitwardenjson" => "Bitwarden (json) · preview and duplicates",
                    "bitwardenpasswordprotected" => "Bitwarden (password-protected json)",
                    "bitwardencsv" => "Bitwarden (csv)",
                    _ => "",
                };
                lines.push(fill_line(
                    vec![
                        Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
                        Span::styled(
                            format!(" {format:<30}"),
                            if selected { theme.bold("fg") } else { theme.style("fg") },
                        ),
                        Span::styled(note, theme.style("fg.muted")),
                    ],
                    Vec::new(),
                    width,
                    if selected { theme.selected_row() } else { Style::default() },
                ));
            }
            if formats.is_empty() {
                lines.push(text_line(theme, "fg.muted", "  No format matches."));
            }
            while lines.len() < visible + 2 {
                lines.push(blank());
            }
            lines.push(hint(&[
                ("type", "filter"),
                ("↑ ↓", "move"),
                ("Enter", "next"),
                ("Esc", "cancel"),
            ]));
        }
        ImportStep::Path | ImportStep::Password => {
            let password_focus = matches!(import.step, ImportStep::Password);
            lines.push(section_label(theme, " FILE"));
            lines.push(Line::from(vec![
                Span::styled(if password_focus { "  " } else { "▌ " }, theme.style("accent")),
                Span::styled(import.path.clone(), theme.style("fg")),
                if password_focus {
                    Span::raw("")
                } else {
                    Span::styled("▏", theme.style("accent"))
                },
            ]));
            lines.push(text_line(theme, "fg.subtle", format!("  format {}", import.format)));
            if import.needs_password() {
                lines.push(section_label(theme, " FILE PASSWORD"));
                lines.push(Line::from(vec![
                    Span::styled(if password_focus { "▌ " } else { "  " }, theme.style("accent")),
                    Span::styled(
                        "•".repeat(import.password.char_count().min(40)),
                        theme.style("fg"),
                    ),
                    if password_focus {
                        Span::styled("▏", theme.style("accent"))
                    } else {
                        Span::raw("")
                    },
                ]));
            } else {
                lines.push(blank());
                lines.push(blank());
            }
            lines.push(blank());
            let next = if import.format == "bitwardenjson" { "preview" } else { "import" };
            let tab = if import.needs_password() { "switch field" } else { "complete path" };
            lines.push(hint(&[("Tab", tab), ("Enter", next), ("Esc", "back")]));
        }
        ImportStep::Preview(preview) => {
            let icons = icons(state);
            lines.push(text_line(theme, "fg", format!(" {}", import.path)));
            lines.push(text_line(theme, "fg.muted", " Bitwarden JSON · not encrypted"));
            lines.push(blank());
            let mut counts = vec![Span::styled(
                format!(" {}  ", crate::util::text::items(preview.items.len())),
                theme.bold("fg"),
            )];
            for kind in [
                ItemType::Login,
                ItemType::SecureNote,
                ItemType::Card,
                ItemType::Identity,
                ItemType::SshKey,
            ] {
                let n = preview.items.iter().filter(|item| item.kind == kind).count();
                if n > 0 {
                    counts.push(Span::styled(
                        format!("{} ", icons.get(Icons::item(kind))),
                        theme.style(Theme::type_token(kind)),
                    ));
                    counts.push(Span::styled(
                        if n == 1 {
                            format!("1 {}  ", kind.label())
                        } else {
                            format!("{n} {}  ", kind.plural().to_lowercase())
                        },
                        theme.style("fg.alt"),
                    ));
                }
            }
            lines.push(Line::from(counts));
            let new_folders = if preview.new_folders.is_empty() {
                "none new".to_owned()
            } else {
                format!("{} new: {}", preview.new_folders.len(), preview.new_folders.join(", "))
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {} folders  ", preview.folders.len()), theme.bold("fg")),
                Span::styled(
                    truncate(&new_folders, width.saturating_sub(16)),
                    theme.style("fg.alt"),
                ),
            ]));
            lines.push(blank());
            if preview.duplicates.is_empty() {
                lines.push(text_line(
                    theme,
                    "fg.muted",
                    " No duplicates of items already in the vault.",
                ));
            } else {
                lines.push(fill_line(
                    vec![
                        section_label(
                            theme,
                            &format!(
                                " POSSIBLE DUPLICATES ({}) · ☑ = skipped",
                                preview.duplicates.len()
                            ),
                        )
                        .spans[0]
                            .clone(),
                    ],
                    vec![Span::styled("x skip or keep · a all ", theme.style("fg.muted"))],
                    width,
                    Style::default(),
                ));
                let visible = 8;
                let offset =
                    follow(state, "import_dupes", preview.index, visible, preview.duplicates.len());
                for (index, duplicate) in
                    preview.duplicates.iter().enumerate().skip(offset).take(visible)
                {
                    let selected = index == preview.index;
                    let kind = preview
                        .items
                        .get(duplicate.index)
                        .map(|i| i.kind)
                        .unwrap_or(ItemType::Login);
                    lines.push(fill_line(
                        vec![
                            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
                            // With skipping off the marks have no effect, so
                            // they are drawn faint.
                            match (preview.skip.contains(&duplicate.index), preview.skip_enabled) {
                                (true, true) => Span::styled("☑ ", theme.style("ok")),
                                (true, false) => Span::styled("☑ ", theme.style("fg.subtle")),
                                (false, _) => Span::styled("☐ ", theme.style("fg.subtle")),
                            },
                            Span::styled(
                                format!("{} ", icons.get(Icons::item(kind))),
                                theme.style(Theme::type_token(kind)),
                            ),
                            Span::styled(
                                pad(&truncate(&duplicate.name, 26), 28),
                                theme.style("fg"),
                            ),
                            Span::styled(
                                truncate(&duplicate.detail, width.saturating_sub(36)),
                                theme.style("fg.muted"),
                            ),
                        ],
                        Vec::new(),
                        width,
                        if selected { theme.selected_row() } else { Style::default() },
                    ));
                }
                lines.push(blank());
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    if preview.skip_enabled {
                        Span::styled("☑ ", theme.style("ok"))
                    } else {
                        Span::styled("☐ ", theme.style("fg.subtle"))
                    },
                    if preview.skip_enabled {
                        Span::styled("Skip the checked duplicates", theme.style("fg"))
                    } else {
                        Span::styled("Skipping is off: every item is imported", theme.style("fg"))
                    },
                    Span::styled("   s toggles", theme.style("fg.subtle")),
                ]));
            }
            let skipped = preview.skipped();
            lines.push(fill_line(
                if skipped > 0 {
                    vec![Span::styled(format!(" {skipped} skipped"), theme.style("fg.muted"))]
                } else {
                    Vec::new()
                },
                vec![
                    Span::styled("[ Back ]", theme.style("fg.muted")),
                    Span::raw("   "),
                    Span::styled(
                        format!(
                            "[ Import {} ]",
                            crate::util::text::items(preview.items.len().saturating_sub(skipped))
                        ),
                        theme.bold("accent"),
                    ),
                    Span::raw(" "),
                ],
                width,
                Style::default(),
            ));
        }
    }
    if state.importing() {
        lines.push(Line::from(Span::styled(
            format!(" {} Importing…", spinner_frame(tick(state))),
            theme.style("info"),
        )));
    } else if let Some(error) = &import.error {
        lines.push(Line::from(Span::styled(format!(" {error}"), theme.style("error"))));
    }
    frame.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), rect);
}
