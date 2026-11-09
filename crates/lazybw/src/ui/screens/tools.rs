//! The generator and health tabs.

use crate::ui::{
    Alignment, AppState, Block, BorderType, Borders, Constraint, Direction, Frame, HealthCategory,
    Instant, ItemType, Layout, Line, Modifier, Paragraph, Rect, Span, Style, blank, draw_scrollbar,
    fill_line, follow, inner, key_hint, meter_spans, pad, pane_block, password_spans, popup_frame,
    section_label, spans_width, spinner_frame, strength_label, text_line, tick, truncate,
};

pub(crate) fn draw_generator(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let generator = &state.generator;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);
    draw_generator_panel(frame, chunks[0], pane_block("Generator", "", true, theme), state, false);
    // History.
    let block = pane_block("This session", "", false, theme);
    let history_width = chunks[1].width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for (value, when) in generator.history.iter().take(chunks[1].height.saturating_sub(5) as usize)
    {
        let secs = Instant::now().saturating_duration_since(*when).as_secs();
        let ago = if secs < 60 { "now".to_owned() } else { format!("{} min", secs / 60) };
        lines.push(fill_line(
            vec![
                Span::raw(" "),
                Span::styled(truncate(value, history_width.saturating_sub(10)), theme.style("fg")),
            ],
            vec![Span::styled(format!("{ago} "), theme.style("fg.subtle"))],
            history_width,
            Style::default(),
        ));
    }
    if lines.is_empty() {
        lines.push(text_line(theme, "fg.muted", " Nothing generated yet."));
    }
    lines.push(Line::from(Span::styled(
        format!(" {}", "─".repeat(history_width.saturating_sub(2))),
        theme.style("border"),
    )));
    lines.push(text_line(theme, "fg.muted", " Kept in memory only. Cleared on lock."));
    frame.render_widget(Paragraph::new(lines).block(block), chunks[1]);
}

pub(crate) fn category_token(category: HealthCategory) -> &'static str {
    match category {
        HealthCategory::Exposed => "error",
        HealthCategory::Reused => "warn",
        HealthCategory::Weak => "caution",
        HealthCategory::Old | HealthCategory::Insecure => "fg.alt",
    }
}

pub(crate) fn draw_health(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let block = pane_block(
        "Vault health",
        match &state.health.report {
            Some(report) => format!("{} logins checked", report.checked),
            None => String::new(),
        },
        true,
        theme,
    );
    let content = inner(area);
    let width = content.width as usize;
    if state.health.report.is_none() {
        let lines = if state.checking_health() {
            let count = state.cache.active_items().filter(|i| i.kind == ItemType::Login).count();
            let mut lines = vec![
                blank(),
                Line::from(Span::styled(
                    format!("{} Checking {count} logins…", spinner_frame(tick(state))),
                    theme.style("info"),
                )),
            ];
            if state.config.health.check_exposed {
                lines.push(Line::from(Span::styled(
                    "This contacts Have I Been Pwned with partial hashes only.",
                    theme.style("fg.muted"),
                )));
            }
            lines
        } else {
            vec![
                blank(),
                text_line(theme, "fg", "Check your passwords for breaches, reuse and weakness."),
                Line::from(vec![
                    Span::styled("r", theme.bold("hint.key")),
                    Span::styled(" run check", theme.style("fg.muted")),
                ]),
            ]
        };
        let mut padded = vec![blank(); (content.height as usize / 2).saturating_sub(2)];
        padded.extend(lines);
        frame.render_widget(Paragraph::new(padded).block(block).alignment(Alignment::Center), area);
        return;
    }
    let Some(report) = &state.health.report else { return };
    frame.render_widget(block, area);
    // Score line. The projection is dropped, then shortened, before it is cut.
    let mut score = vec![
        Span::styled(" Password score ", theme.style("fg.muted")),
        Span::styled(
            format!("{} / 100  ", report.score),
            Style::default()
                .fg(theme.colour(score_token(report.score)))
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let filled = usize::from(report.score.min(100)) / 10;
    score.push(Span::styled("▰".repeat(filled), theme.style(score_token(report.score))));
    score.push(Span::styled("▱".repeat(10 - filled), theme.style("border")));
    let room = width.saturating_sub(spans_width(&score));
    let long =
        format!("   would be {} after fixing the exposed and reused passwords", report.potential);
    let short = format!("   would be {} if fixed", report.potential);
    if let Some(text) = [long, short].into_iter().find(|t| crate::util::text::width(t) <= room) {
        score.push(Span::styled(text, theme.style("fg.muted")));
    }
    frame.render_widget(Paragraph::new(Line::from(score)), Rect { height: 1, ..content });
    // Categories: five cards when they fit, otherwise one compact row, so
    // every category stays visible at any supported width.
    let card_width = 15u16;
    let cards_fit =
        content.width >= card_width * HealthCategory::ALL.len() as u16 && content.height >= 8;
    let count_of = |category: HealthCategory| {
        if category == HealthCategory::Exposed && !state.config.health.check_exposed {
            "—".to_owned()
        } else {
            category.count(report).to_string()
        }
    };
    let list_top = if cards_fit {
        for (index, category) in HealthCategory::ALL.iter().enumerate() {
            let x = content.x + index as u16 * card_width;
            let selected = *category == state.health.category;
            let token = category_token(*category);
            let rect = Rect { x, y: content.y + 1, width: card_width, height: 4 };
            let card = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if selected { theme.style(token) } else { theme.style("border") });
            // Highlight only inside the border: a background on the border
            // cells shows as a slab half a cell outside the line drawing.
            let body = card.inner(rect);
            let body_style = if selected {
                Style::default().bg(theme.colour("bg.highlight"))
            } else {
                Style::default()
            };
            let lines = vec![
                Line::from(Span::styled(format!(" {}", count_of(*category)), theme.bold(token))),
                Line::from(Span::styled(format!(" {}", category.label()), theme.style("fg.muted"))),
            ];
            frame.render_widget(card, rect);
            frame.render_widget(Paragraph::new(lines).style(body_style), body);
        }
        6
    } else {
        // Chips laid out by hand so a category never splits across lines.
        let mut rows: Vec<Vec<Span<'static>>> = vec![vec![Span::raw(" ")]];
        let mut used = 1;
        for category in HealthCategory::ALL {
            let selected = category == state.health.category;
            let token = category_token(category);
            let style = if selected { theme.selected_row() } else { Style::default() };
            let count = format!(" {} ", count_of(category));
            let label = format!("{} ", category.label());
            let chip_width =
                crate::util::text::width(&count) + crate::util::text::width(&label) + 1;
            if used + chip_width > width && used > 1 {
                rows.push(vec![Span::raw(" ")]);
                used = 1;
            }
            let row = rows.last_mut().expect("rows starts with one line");
            row.push(Span::styled(count, theme.bold(token).patch(style)));
            row.push(Span::styled(label, theme.style("fg.alt").patch(style)));
            row.push(Span::raw(" "));
            used += chip_width;
        }
        let height = rows.len().min(2) as u16;
        let lines: Vec<Line<'static>> = rows.into_iter().take(2).map(Line::from).collect();
        frame.render_widget(Paragraph::new(lines), Rect { y: content.y + 1, height, ..content });
        1 + height + 1
    };
    // The list.
    let list_area = Rect {
        x: content.x,
        y: content.y + list_top,
        width: content.width,
        height: content.height.saturating_sub(list_top),
    };
    let mut header = state.health.category.label().to_uppercase();
    if state.health.category == HealthCategory::Exposed {
        header.push_str(" · MOST SEVERE FIRST");
    }
    let mut lines = vec![section_label(theme, &format!(" {header}"))];
    let items = report.items_in(state.health.category);
    if items.is_empty() {
        lines.push(text_line(theme, "fg.muted", "  Nothing here."));
    }
    let visible = (list_area.height as usize).saturating_sub(1);
    let offset = follow(state, "health", state.health.index, visible, items.len());
    let name_width =
        items.iter().map(|r| crate::util::text::width(&r.name)).max().unwrap_or(10).clamp(12, 36);
    for (index, record) in items.iter().enumerate().skip(offset).take(visible) {
        let selected = index == state.health.index;
        let row = if selected { theme.selected_row() } else { Style::default() };
        let detail = match state.health.category {
            HealthCategory::Exposed => record
                .exposed_count
                .map(|n| format!("seen {} times in data breaches", crate::util::time::thousands(n)))
                .or_else(|| record.exposed_error.clone())
                .unwrap_or_default(),
            HealthCategory::Reused => record
                .reuse_count
                .map(|n| {
                    format!(
                        "same password as {} other item{}",
                        n.saturating_sub(1),
                        if n == 2 { "" } else { "s" }
                    )
                })
                .unwrap_or_default(),
            HealthCategory::Weak => strength_label(record.strength).to_owned(),
            HealthCategory::Old => state
                .cache
                .item(&record.id)
                .and_then(|item| {
                    item.login
                        .as_ref()
                        .and_then(|l| l.password_revision_date.clone())
                        .or_else(|| item.creation_date.clone())
                })
                .and_then(|d| crate::util::time::relative_from_str(state.now, &d))
                .map(|r| format!("changed {r}"))
                .unwrap_or_else(|| "older than the configured age".to_owned()),
            HealthCategory::Insecure => "has an http:// website".to_owned(),
        };
        let token = category_token(state.health.category);
        let mut right = Vec::new();
        if selected && state.config.ui.show_hints {
            for (action, label, separator) in [
                ("health.open_item", "open", " · "),
                ("item.generate_password", "replace password", " "),
            ] {
                if let Some(key) = key_hint(state, "health", action) {
                    right.push(Span::styled(key, theme.style("hint.key")));
                    right.push(Span::styled(
                        format!(" {label}{separator}"),
                        theme.style("fg.muted"),
                    ));
                }
            }
            // The hint gives way when it would cover the row's own data.
            if width.saturating_sub(name_width + 8 + spans_width(&right))
                < crate::util::text::width(&detail).min(24)
            {
                right.clear();
            }
        }
        let detail_width = width.saturating_sub(name_width + 8 + spans_width(&right));
        lines.push(fill_line(
            vec![
                Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
                Span::styled(" ● ", theme.style(token)),
                Span::styled(
                    pad(&truncate(&record.name, name_width), name_width + 2),
                    if selected { theme.bold("fg") } else { theme.style("fg") },
                ),
                Span::styled(truncate(&detail, detail_width), theme.style("fg.muted")),
            ],
            right,
            width,
            row,
        ));
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    draw_scrollbar(
        frame,
        Rect { y: list_area.y, height: list_area.height + 1, ..area },
        offset,
        visible,
        items.len(),
        true,
        theme,
    );
}

pub(crate) fn score_token(score: u8) -> &'static str {
    if score >= 80 {
        "ok"
    } else if score >= 50 {
        "caution"
    } else {
        "error"
    }
}

/// The generator itself (mode tabs, output, strength, options, hints) inside
/// `block`; shared by the Generator tab and the editor's `Ctrl-g` popup.
pub(crate) fn draw_generator_panel(
    frame: &mut Frame<'_>,
    pane: Rect,
    block: Block<'static>,
    state: &AppState,
    popup: bool,
) {
    let theme = &state.theme;
    let generator = &state.generator;
    let content = inner(pane);
    let width = content.width as usize;
    frame.render_widget(block, pane);
    let request = &generator.request;
    // Mode tabs.
    let tab = |label: &str, on: bool| {
        if on {
            Span::styled(
                format!("[ {label} ]"),
                theme.on("fg", "selection").add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(format!("  {label}  "), theme.style("fg.muted"))
        }
    };
    let tabs = fill_line(
        vec![
            Span::raw(" "),
            tab("Password", !request.passphrase),
            Span::raw(" "),
            tab("Passphrase", request.passphrase),
        ],
        vec![Span::styled("Tab ", theme.style("hint.key"))],
        width,
        Style::default(),
    );
    frame.render_widget(Paragraph::new(tabs), Rect { height: 1, ..content });
    // Output box.
    let output_area = Rect { x: content.x, y: content.y + 1, width: content.width, height: 3 };
    let output_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.style("border"));
    let value = if generator.output.is_empty() {
        vec![Span::styled("…", theme.style("fg.subtle"))]
    } else {
        password_spans(
            theme,
            &truncate(&generator.output, width.saturating_sub(22)),
            Modifier::BOLD.into(),
        )
    };
    let mut output_spans = vec![Span::raw(" ")];
    output_spans.extend(value);
    let output_line = fill_line(
        output_spans,
        vec![
            Span::styled("r", theme.style("hint.key")),
            Span::styled(" new · ", theme.style("fg.muted")),
            Span::styled("yy", theme.style("hint.key")),
            Span::styled(" copy ", theme.style("fg.muted")),
        ],
        width.saturating_sub(2),
        Style::default(),
    );
    frame.render_widget(Paragraph::new(output_line).block(output_block), output_area);
    let mut lines = Vec::new();
    if !generator.output.is_empty() {
        let strength = crate::vault::health::local_strength(&generator.output);
        let mut spans = vec![Span::raw(" ")];
        spans.extend(meter_spans(theme, strength));
        let size = if request.passphrase {
            format!("{} words", generator.output.split(request.separator.as_str()).count())
        } else {
            format!("{} characters", generator.output.chars().count())
        };
        spans.push(Span::styled(
            format!(" {} · {size}", strength_label(strength)),
            theme.style("fg.muted"),
        ));
        lines.push(Line::from(spans));
    } else {
        lines.push(blank());
    }
    lines.push(blank());
    let slider = |value: u8, min: u8, max: u8| -> Vec<Span<'static>> {
        let cells = 24usize;
        let position = (usize::from(value.saturating_sub(min)) * (cells - 1))
            / usize::from((max - min).max(1));
        vec![
            Span::styled(pad(&value.to_string(), 5), theme.bold("fg")),
            Span::styled("━".repeat(position), theme.style("accent")),
            Span::styled("●", theme.style("accent")),
            Span::styled("─".repeat(cells - 1 - position), theme.style("border")),
            Span::styled(format!("  {min}–{max}"), theme.style("fg.subtle")),
        ]
    };
    let check = |on: bool, text: &str| -> Vec<Span<'static>> {
        vec![
            if on {
                Span::styled("☑ ", theme.style("ok"))
            } else {
                Span::styled("☐ ", theme.style("fg.subtle"))
            },
            Span::styled(text.to_owned(), theme.style(if on { "fg" } else { "fg.muted" })),
        ]
    };
    let number = |n: u8| vec![Span::styled(format!("‹ {n} ›"), theme.style("type.note"))];
    let options: Vec<(&str, Vec<Span<'static>>)> = if request.passphrase {
        vec![
            ("Words", slider(request.words, 3, 20)),
            (
                "Separator",
                vec![Span::styled(
                    format!(
                        "‹ {} ›",
                        if request.separator == " " { "space" } else { request.separator.as_str() }
                    ),
                    theme.style("type.note"),
                )],
            ),
            ("Capitalize", check(request.capitalize, "Word")),
            ("Include number", check(request.include_number, "word7")),
        ]
    } else {
        vec![
            ("Length", slider(request.length, 5, 128)),
            ("Uppercase", check(request.uppercase, "A-Z")),
            ("Lowercase", check(request.lowercase, "a-z")),
            ("Numbers", check(request.numbers, "0-9")),
            ("Special", check(request.special, "!@#$%^&*")),
            ("Minimum numbers", number(request.min_numbers)),
            ("Minimum special", number(request.min_special)),
            ("Avoid ambiguous", check(request.avoid_ambiguous, "l 1 I O 0")),
        ]
    };
    for (index, (label, value)) in options.into_iter().enumerate() {
        let selected = index == generator.row;
        let row = if selected { theme.active_row() } else { Style::default() };
        let mut spans = vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style("accent")),
            Span::styled(
                format!(" {label:<20}"),
                if selected { theme.bold("accent") } else { theme.style("fg.alt") },
            ),
        ];
        spans.extend(value);
        lines.push(fill_line(spans, Vec::new(), width, row));
    }
    lines.push(blank());
    let hint = |key: &str, text: &str| {
        vec![
            Span::styled(key.to_owned(), theme.style("hint.key")),
            Span::styled(format!(" {text} · "), theme.style("fg.muted")),
        ]
    };
    let mut hints = vec![Span::raw(" ")];
    hints.extend(hint("j k", "option"));
    hints.extend(hint("h l", "change"));
    hints.extend(hint("x", "toggle"));
    hints.extend(hint("r", "new"));
    if popup {
        hints.extend(hint("Enter", "use"));
        hints.push(Span::styled("Esc", theme.style("hint.key")));
        hints.push(Span::styled(" cancel", theme.style("fg.muted")));
    } else {
        hints.push(Span::styled("yy", theme.style("hint.key")));
        hints.push(Span::styled(" copy", theme.style("fg.muted")));
    }
    lines.push(Line::from(hints));
    if let Some(error) = &generator.error {
        lines.push(text_line(theme, "error", format!(" {error}")));
    }
    let rest = Rect {
        x: content.x,
        y: content.y + 4,
        width: content.width,
        height: content.height.saturating_sub(4),
    };
    frame.render_widget(Paragraph::new(lines), rest);
}

/// The generator as a popup over the editor (`Ctrl-g`); `Enter` inserts.
pub(crate) fn draw_generator_popup(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let rows = if state.generator.request.passphrase { 4 } else { 8 };
    let error = u16::from(state.generator.error.is_some());
    let height = (rows as u16 + 10 + error).min(area.height);
    let width = 72.min(area.width.saturating_sub(2));
    let (rect, block) = popup_frame(frame, "Generate password", theme, area, width, height);
    draw_generator_panel(frame, rect, block, state, true);
}
