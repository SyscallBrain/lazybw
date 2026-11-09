//! Shared building blocks: panes, rows, scrolling, popups, meters and the
//! bars (topbar, statusline, message line, toasts, which-key, wildmenu).

use crate::ui::{
    Alignment, AppState, Block, BorderType, Borders, Frame, Icons, Instant, ItemType, Line,
    MessageLevel, Modifier, Paragraph, Rect, Role, Screen, SessionPhase, Span, Style, Theme,
    centered, layout, pad, spinner_frame, truncate,
};

pub(crate) fn icons(state: &AppState) -> Icons {
    Icons::new(state.config.ui.icons)
}

pub(crate) fn pane_block(
    title: impl Into<String>,
    right: impl Into<String>,
    focused: bool,
    theme: &Theme,
) -> Block<'static> {
    pane_block_counted(title, None, right, focused, theme)
}

/// A pane block whose title carries a count in `fg.subtle` (` Logins 198 `).
pub(crate) fn pane_block_counted(
    title: impl Into<String>,
    count: Option<usize>,
    right: impl Into<String>,
    focused: bool,
    theme: &Theme,
) -> Block<'static> {
    let border = if focused { theme.colour("border.focus") } else { theme.colour("border") };
    let title_style = if focused { theme.bold("accent") } else { theme.style("fg.muted") };
    let right_text = right.into();
    let mut title_spans = vec![Span::styled(format!(" {}", title.into()), title_style)];
    if let Some(count) = count {
        title_spans.push(Span::styled(format!(" {count}"), theme.style("fg.subtle")));
    }
    title_spans.push(Span::styled(" ", title_style));
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme.colour("bg")))
        .title(Line::from(title_spans));
    if !right_text.is_empty() {
        block = block.title_top(
            Line::from(Span::styled(format!(" {right_text} "), theme.style("fg.muted")))
                .alignment(Alignment::Right),
        );
    }
    block
}

pub(crate) fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

pub(crate) fn text_line(theme: &Theme, token: &str, text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme.style(token)))
}

pub(crate) fn blank() -> Line<'static> {
    Line::default()
}

pub(crate) fn section_label(theme: &Theme, text: &str) -> Line<'static> {
    // `fg.subtle`, not `fg.muted`: section names and field labels are needed
    // to read the screen, so they get the more legible of the two.
    Line::from(Span::styled(text.to_owned(), theme.style("fg.subtle").add_modifier(Modifier::BOLD)))
}

/// Display width of a span list.
pub(crate) fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| crate::util::text::width(&s.content)).sum()
}

/// A row that fills `width` cells: `left`, padding, then `right`, all on the
/// `row` style (so a selection background covers the whole row).
pub(crate) fn fill_line(
    mut left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
    row: Style,
) -> Line<'static> {
    let used = spans_width(&left) + spans_width(&right);
    left.push(Span::styled(" ".repeat(width.saturating_sub(used)), row));
    left.extend(right);
    Line::from(left).style(row)
}

/// Keeps `selected` inside a window of `visible` rows, remembering the offset
/// per view so scrolling is stable and clicks map to what is on screen.
pub(crate) fn follow(
    state: &AppState,
    key: &'static str,
    selected: usize,
    visible: usize,
    total: usize,
) -> usize {
    let visible = visible.max(1);
    let mut map = state.ui_scroll.borrow_mut();
    let mut offset = map.get(key).copied().unwrap_or(0);
    if selected < offset {
        offset = selected;
    } else if selected >= offset + visible {
        offset = selected + 1 - visible;
    }
    offset = offset.min(total.saturating_sub(visible));
    map.insert(key, offset);
    offset
}

/// A scrollbar on the right border of a pane when its content overflows.
pub(crate) fn draw_scrollbar(
    frame: &mut Frame<'_>,
    pane: Rect,
    offset: usize,
    visible: usize,
    total: usize,
    focused: bool,
    theme: &Theme,
) {
    use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
    if total <= visible || pane.height < 4 {
        return;
    }
    let mut scroll = ScrollbarState::new(total.saturating_sub(visible) + 1).position(offset);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("┃")
        .track_style(theme.style("border"))
        .thumb_style(if focused { theme.style("accent") } else { theme.style("fg.subtle") });
    let area = Rect { y: pane.y + 1, height: pane.height - 2, ..pane };
    frame.render_stateful_widget(bar, area, &mut scroll);
}

/// Clears the popup area and paints its drop shadow (one column right, one
/// row below) so nothing behind bleeds through.
pub(crate) fn popup_surface(frame: &mut Frame<'_>, rect: Rect, theme: &Theme) {
    let screen = frame.area();
    let shadow = theme.colour("shadow");
    let buffer = frame.buffer_mut();
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;
    if right < screen.x + screen.width {
        for y in rect.y + 1..=bottom.min(screen.y + screen.height - 1) {
            buffer[(right, y)].set_bg(shadow);
        }
    }
    if bottom < screen.y + screen.height {
        for x in rect.x + 1..=right.min(screen.x + screen.width - 1) {
            buffer[(x, bottom)].set_bg(shadow);
        }
    }
    frame.render_widget(ratatui::widgets::Clear, rect);
}

/// A password coloured by character class (letters `fg`, digits `pw.digit`,
/// symbols `pw.symbol`).
pub(crate) fn password_spans(theme: &Theme, value: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_token = "";
    for ch in value.chars() {
        let token = if ch.is_ascii_digit() {
            "pw.digit"
        } else if ch.is_alphanumeric() || ch == ' ' {
            "fg"
        } else {
            "pw.symbol"
        };
        if token != run_token && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), base.patch(theme.style(run_token))));
        }
        run_token = token;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, base.patch(theme.style(run_token))));
    }
    spans
}

/// The 10-cell strength meter, coloured by score.
pub(crate) fn meter_spans(theme: &Theme, strength: u8) -> Vec<Span<'static>> {
    let filled = usize::from(strength.min(4) + 1) * 2;
    let token = match strength {
        0 => "error",
        1 => "warn",
        2 => "caution",
        _ => "ok",
    };
    vec![
        Span::styled("▰".repeat(filled), theme.style(token)),
        Span::styled("▱".repeat(10 - filled), theme.style("border")),
    ]
}

pub(crate) fn draw_centered(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    state: &AppState,
) {
    let height = lines.len() as u16;
    let rect = centered(area, area.width.min(70), height.min(area.height));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
    let _ = state;
}

pub(crate) fn logo(theme: &Theme) -> Vec<Line<'static>> {
    let accent = theme.bold("accent");
    let diamond = theme.style("type.note");
    vec![
        Line::from(vec![
            Span::styled("        ", Style::default()),
            Span::styled("╻  ┏━┓╺━┓╻ ╻┏┓ ╻ ╻", accent),
        ]),
        Line::from(vec![
            Span::styled("     ◆  ", diamond),
            Span::styled("┃  ┣━┫┏━┛┗┳┛┣┻┓┃╻┃", accent),
        ]),
        Line::from(vec![
            Span::styled("        ", Style::default()),
            Span::styled("┗━╸╹ ╹┗━╸ ╹ ┗━┛┗┻┛", accent),
        ]),
    ]
}

pub(crate) fn draw_topbar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let icons = icons(state);
    let bar = Style::default().bg(theme.colour("bg.bar")).fg(theme.colour("fg"));
    let mut spans = vec![
        // The padlock of the project logo with Nerd Font icons; the other
        // modes keep the diamond, as no padlock renders reliably there.
        Span::styled(
            format!(" {} ", icons.get(Role::Logo)),
            theme.style(if icons.mode == crate::config::IconMode::Nerdfont {
                "accent"
            } else {
                "type.note"
            }),
        ),
        Span::styled("lazybw", theme.bold("accent")),
        Span::raw("  "),
    ];
    for tab in [
        crate::app::state::Tab::Vault,
        crate::app::state::Tab::Generator,
        crate::app::state::Tab::Health,
    ] {
        let active = state.tab == tab && state.screen.is_tab();
        if active {
            let style = theme.on("fg", "selection").add_modifier(Modifier::BOLD);
            spans.push(Span::styled(" ", style));
            spans.push(Span::styled(
                tab.number().to_string(),
                style.fg(theme.colour("accent")).add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::styled(
                format!(" {}", tab.label()),
                style.add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::styled(" ", style));
        } else {
            spans.push(Span::styled(
                format!(" {} {} ", tab.number(), tab.label()),
                theme.style("fg.muted"),
            ));
        }
    }
    let (state_text, state_token) = match &state.session {
        SessionPhase::Ready => (format!("{} unlocked", icons.get(Role::Unlock)), "ok"),
        SessionPhase::Loading => (format!("{} syncing", spinner_frame(tick(state))), "info"),
        SessionPhase::Locked => (format!("{} locked", icons.get(Role::Lock)), "error"),
        SessionPhase::LoggedOut => ("logged out".to_owned(), "fg.muted"),
        SessionPhase::Starting => (format!("{} starting", spinner_frame(tick(state))), "info"),
        SessionPhase::Failed(_) => ("error".to_owned(), "error"),
    };
    // The session state always shows; the server and then the email give
    // way when the bar is too narrow for all three.
    let separator = Span::styled(" │ ", theme.style("border"));
    let server =
        state.server.clone().unwrap_or_else(|| "bitwarden.com".to_owned()).replace("https://", "");
    let mut room = (area.width as usize)
        .saturating_sub(spans_width(&spans) + crate::util::text::width(&state_text) + 2);
    let mut right = Vec::new();
    let fits = |text: &str, room: &mut usize| {
        let needed = crate::util::text::width(text) + 3;
        if needed <= *room {
            *room -= needed;
            true
        } else {
            false
        }
    };
    let show_server = fits(&server, &mut room);
    let show_email = state.email.as_deref().is_some_and(|email| fits(email, &mut room));
    if show_email && let Some(email) = &state.email {
        right.push(Span::styled(email.clone(), theme.style("fg.subtle")));
        right.push(separator.clone());
    }
    if show_server {
        right.push(Span::styled(server, theme.style("fg.subtle")));
        right.push(separator);
    }
    right.push(Span::styled(state_text, theme.style(state_token)));
    right.push(Span::raw(" "));
    let line = fill_line(spans, right, area.width as usize, bar);
    frame.render_widget(Paragraph::new(line).style(bar), area);
}

pub(crate) fn tick(state: &AppState) -> usize {
    state.started.elapsed().as_millis() as usize / 100
}

pub(crate) fn draw_statusline(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let icons = icons(state);
    let mode = state.mode_name();
    let mode_token = match mode {
        "INSERT" => "mode.insert",
        "VISUAL" => "mode.visual",
        "COMMAND" => "mode.command",
        "SEARCH" => "mode.search",
        "LOCKED" => "mode.locked",
        _ => "mode.normal",
    };
    let bar = Style::default().bg(theme.colour("bg.bar")).fg(theme.colour("fg"));
    let mut spans = vec![Span::styled(
        format!(" {mode} "),
        Style::default()
            .bg(theme.colour(mode_token))
            .fg(theme.colour("bg.bar"))
            .add_modifier(Modifier::BOLD),
    )];
    let separator = icons.separator(state.config.ui.statusline_separator).0;
    let crumbs = state.breadcrumb();
    let segment = Style::default().bg(theme.colour("bg.highlight")).fg(theme.colour("fg.alt"));
    if crumbs.is_empty() {
        spans.push(Span::styled(
            separator.to_string(),
            Style::default().bg(theme.colour("bg.bar")).fg(theme.colour(mode_token)),
        ));
    } else {
        spans.push(Span::styled(
            separator.to_string(),
            Style::default().bg(theme.colour("bg.highlight")).fg(theme.colour(mode_token)),
        ));
        let mut text = String::from(" ");
        text.push_str(&crumbs.join(" › "));
        text.push(' ');
        spans.push(Span::styled(text, segment));
        spans.push(Span::styled(
            separator.to_string(),
            Style::default().bg(theme.colour("bg.bar")).fg(theme.colour("bg.highlight")),
        ));
    }
    let divider = || Span::styled(" │ ", Style::default().fg(theme.colour("bg.highlight")));
    let mut right: Vec<Span<'static>> = Vec::new();
    if !state.showcmd.is_empty() {
        right.push(Span::styled(state.showcmd.clone(), theme.bold("caution")));
        right.push(Span::raw("   "));
    }
    if let Some(countdown) = state.clipboard.countdown(Instant::now()) {
        right.push(Span::styled(
            format!("{} clipboard {countdown}s", icons.get(Role::Clipboard)),
            theme.style("caution"),
        ));
        right.push(divider());
    }
    if state.screen == Screen::Health && state.config.health.check_exposed {
        right.push(Span::styled("HIBP via k-anonymity", theme.style("fg.muted")));
        right.push(divider());
    }
    if let Some(last) = state.last_sync {
        let text = if state.pending.values().any(|p| matches!(p, crate::app::state::Pending::Sync))
        {
            format!("{} syncing", spinner_frame(tick(state)))
        } else {
            format!("{} {}", icons.get(Role::Sync), crate::util::time::relative(state.now, last))
        };
        right.push(Span::styled(text, theme.style("fg.muted")));
        right.push(divider());
    }
    if state.session.is_ready()
        && let Some(lock) = state.config.security.auto_lock.get()
        && !lock.is_zero()
    {
        let remaining = lock.as_secs().saturating_sub(state.last_input.elapsed().as_secs());
        let when =
            (state.now + chrono::Duration::seconds(remaining as i64)).with_timezone(&chrono::Local);
        right.push(Span::styled(
            format!("{} locks {}", icons.get(Role::Lock), when.format("%H:%M")),
            theme.style("fg.subtle"),
        ));
        right.push(divider());
    }
    let (position, total) = state.position();
    if total > 0 {
        right.push(Span::styled(
            format!(" {position}/{total} "),
            Style::default()
                .bg(theme.colour(mode_token))
                .fg(theme.colour("bg.bar"))
                .add_modifier(Modifier::BOLD),
        ));
    } else if right.last().is_some_and(|s| s.content.contains('│')) {
        right.pop();
        right.push(Span::raw(" "));
    }
    let line = fill_line(spans, right, area.width as usize, bar);
    frame.render_widget(Paragraph::new(line).style(bar), area);
}

pub(crate) fn draw_message_line(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let line = if let Some(cmdline) = state.cmdline() {
        Line::from(vec![
            Span::styled(":", theme.bold("accent")),
            Span::styled(cmdline.input.clone(), theme.style("fg")),
        ])
    } else if let Some(prompt) = state.prompt() {
        Line::from(vec![
            Span::styled(format!("{}: ", prompt.title), theme.bold("accent")),
            Span::styled(prompt.input.text(), theme.style("fg")),
        ])
    } else if state.search.active {
        Line::from(vec![
            Span::styled("/", theme.bold("accent")),
            Span::styled(state.search.input.clone(), theme.style("fg")),
        ])
    } else if state.editor.as_ref().is_some_and(|e| e.insert) {
        Line::from(Span::styled(
            "-- INSERT --",
            theme.style("mode.insert").add_modifier(Modifier::BOLD),
        ))
    } else if state.visual_anchor.is_some() {
        Line::from(Span::styled(
            "-- VISUAL --",
            theme.style("mode.visual").add_modifier(Modifier::BOLD),
        ))
    } else if let Some(message) = &state.status_message {
        let token = match message.level {
            MessageLevel::Info => "fg.alt",
            MessageLevel::Ok => "ok",
            MessageLevel::Warn => "warn",
            MessageLevel::Error => "error",
        };
        Line::from(Span::styled(message.text.clone(), theme.style(token)))
    } else {
        Line::default()
    };
    frame.render_widget(Paragraph::new(line).style(Style::default().bg(theme.colour("bg"))), area);
}

pub(crate) fn draw_toasts(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let mut y = area.y + 1;
    for (index, toast) in state.toasts.iter().enumerate().take(3) {
        // Grow with the text up to 60 columns, then wrap: a fixed width cut
        // messages such as "Copied username for “…”. Clipboard clears in 30s."
        let max_width = 60.min(area.width.saturating_sub(2));
        let wanted = crate::util::text::width(&toast.title)
            .max(toast.body.as_deref().map_or(0, crate::util::text::width))
            + 6;
        let width = (wanted as u16).clamp(30.min(max_width), max_width);
        let inner = width.saturating_sub(2) as usize;
        let title_lines = wrap_text(&toast.title, inner.saturating_sub(2).max(1));
        let body_lines: Vec<String> = toast
            .body
            .as_deref()
            .map(|body| wrap_text(body, inner.max(1)).into_iter().take(3).collect())
            .unwrap_or_default();
        let height = (title_lines.len() + body_lines.len()) as u16 + 2;
        if y + height > area.y + area.height {
            break;
        }
        let offset = state.fx.borrow().toast_offset(toast.created, width);
        let x = area.x.saturating_add(area.width.saturating_sub(width + 1)).saturating_add(offset);
        let rect = Rect { x, y, width: width.min(area.x + area.width - x), height };
        popup_surface(frame, rect, theme);
        let token = match toast.level {
            MessageLevel::Info => "info",
            MessageLevel::Ok => "ok",
            MessageLevel::Warn => "warn",
            MessageLevel::Error => "error",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.colour(token)))
            .style(Style::default().bg(theme.colour("bg.float")));
        let icon = match toast.level {
            MessageLevel::Error => "✗",
            MessageLevel::Warn => "⚠",
            MessageLevel::Info => "•",
            MessageLevel::Ok => "✓",
        };
        let mut lines = Vec::new();
        for (row, text) in title_lines.into_iter().enumerate() {
            let lead = if row == 0 { format!("{icon} ") } else { "  ".to_owned() };
            lines.push(Line::from(vec![
                Span::styled(lead, theme.style(token)),
                Span::styled(text, theme.style("fg")),
            ]));
        }
        for text in body_lines {
            lines.push(Line::from(Span::styled(text, theme.style("fg.muted"))));
        }
        frame.render_widget(Paragraph::new(lines).block(block), rect);
        y += height + 1;
        let _ = index;
    }
}

pub(crate) fn draw_which_key(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let Some(which) = &state.which_key else { return };
    let theme = &state.theme;
    // Anchored just above the statusline and message line.
    let above = Rect { height: area.height.saturating_sub(2), ..area };
    let inner_width = above.width.saturating_sub(4) as usize;
    let columns = if inner_width >= 120 {
        4
    } else if inner_width >= 80 {
        3
    } else {
        2
    };
    let column_width = (inner_width / columns).max(12);
    let key_width = which
        .continuations
        .iter()
        .map(|(key, _, _)| crate::util::text::width(key))
        .max()
        .unwrap_or(1)
        + 2;
    let label_width = column_width.saturating_sub(key_width + 1).max(4);
    let rows = which.continuations.len().div_ceil(columns);
    let height = (rows.max(1) as u16 + 2).min(above.height);
    let rect = layout::bottom_popup(above, height);
    // Docked on the statusline: no drop shadow, which would paint over the
    // statusline row (the mode pill and position pill lost their colours).
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.colour("border.focus")))
        .style(Style::default().bg(theme.colour("bg.float")))
        .title(Span::styled(format!(" {} ", which.prefix), theme.bold("accent")))
        .title_top(
            Line::from(Span::styled(" Esc cancel ", theme.style("fg.muted")))
                .alignment(Alignment::Right),
        );
    let mut lines = Vec::new();
    for chunk in which.continuations.chunks(columns) {
        let mut spans = vec![Span::raw(" ")];
        for (key, group, label) in chunk {
            spans.push(Span::styled(pad(key, key_width), theme.bold("accent")));
            let style = if *group { theme.style("type.note") } else { theme.style("fg") };
            spans.push(Span::styled(pad(&truncate(label, label_width), label_width + 1), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// "Login", "Secure note": the type label in sentence case.
pub(crate) fn type_label(kind: ItemType) -> String {
    let label = kind.label();
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Soft-wraps `text` to `width` columns at word boundaries (long words are
/// cut), always returning at least one line.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    // The candidate line grows in place; when the next word does not fit,
    // the append is rolled back instead of rebuilding the whole line per
    // word (`format!("{current} {word}")` copies the line once per word).
    let mut current = String::new();
    for word in text.split(' ') {
        let saved = current.len();
        if saved != 0 {
            current.push(' ');
        }
        current.push_str(word);
        if crate::util::text::width(&current) <= width {
            continue;
        }
        current.truncate(saved);
        if saved != 0 {
            out.push(std::mem::take(&mut current));
        }
        // Long word: hard-cut at the same char boundaries, by slicing
        // instead of re-collecting the tail every round.
        let mut rest = word;
        while crate::util::text::width(rest) > width {
            let cut_end = rest.char_indices().nth(width).map_or(rest.len(), |(i, _)| i);
            out.push(rest[..cut_end].to_owned());
            rest = &rest[cut_end..];
        }
        current = rest.to_owned();
    }
    out.push(current);
    out
}

pub(crate) fn totp_style(state: &AppState, code: &crate::totp::TotpCode) -> Style {
    match code.urgency() {
        crate::totp::Urgency::Fresh => state.theme.style("totp.fresh"),
        crate::totp::Urgency::Expiring => state.theme.style("totp.expiring"),
        crate::totp::Urgency::Critical => state.theme.style("totp.critical"),
    }
}

pub(crate) fn totp_bar(
    state: &AppState,
    code: &crate::totp::TotpCode,
    width: usize,
) -> Line<'static> {
    let total = width.saturating_sub(6).clamp(10, 60) as u64;
    let filled = ((code.remaining as f32 / code.period as f32) * total as f32).round() as usize;
    let style = totp_style(state, code);
    Line::from(vec![
        Span::styled("━".repeat(filled.min(total as usize)), style),
        Span::styled(
            "━".repeat((total as usize).saturating_sub(filled)),
            state.theme.style("border"),
        ),
        Span::styled(format!(" {:>3}s", code.remaining), style),
    ])
}

pub(crate) fn strength_label(strength: u8) -> &'static str {
    match strength {
        0 => "very weak",
        1 => "weak",
        2 => "fair",
        3 => "strong",
        _ => "very strong",
    }
}

pub(crate) fn popup_frame(
    frame: &mut Frame<'_>,
    title: &str,
    theme: &Theme,
    area: Rect,
    width: u16,
    height: u16,
) -> (Rect, Block<'static>) {
    let rect = centered(area, width, height);
    popup_surface(frame, rect, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.colour("border.focus")))
        .style(Style::default().bg(theme.colour("bg.float")))
        .title(Span::styled(format!(" {title} "), theme.bold("accent")));
    (rect, block)
}

pub(crate) fn draw_wildmenu(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let Some(cmdline) = state.cmdline() else { return };
    if !cmdline.show_completions || cmdline.completions.is_empty() {
        return;
    }
    let theme = &state.theme;
    let height = cmdline.completions.len().min(10) as u16 + 2;
    let width =
        cmdline.completions.iter().map(|c| c.chars().count()).max().unwrap_or(10) as u16 + 6;
    // Docked just above the statusline (which stays readable), with no
    // shadow: one would land on the statusline or the command line.
    let height = height.min(area.height.saturating_sub(4));
    let rect = Rect {
        x: area.x + 1,
        y: area.y + area.height.saturating_sub(height + 2),
        width: width.min(area.width.saturating_sub(2)),
        height,
    };
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.colour("border.focus")))
        .style(Style::default().bg(theme.colour("bg.float")));
    let visible = (rect.height.saturating_sub(2) as usize).max(1);
    let total = cmdline.completions.len();
    let offset = follow(state, "wildmenu", cmdline.completion_index, visible, total);
    let lines: Vec<Line<'static>> = cmdline
        .completions
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(index, completion)| {
            let style = if index == cmdline.completion_index {
                theme.selected_row()
            } else {
                theme.style("fg")
            };
            Line::from(Span::styled(format!(" {completion}"), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), rect);
    draw_scrollbar(frame, rect, offset, visible, total, true, theme);
}

/// Shortens text from the left ("… › Bancos"), keeping its end.
pub(crate) fn truncate_left(text: &str, max: usize) -> String {
    if crate::util::text::width(text) <= max {
        return text.to_owned();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1; // the ellipsis
    for ch in text.chars().rev() {
        let w = crate::util::text::width(&ch.to_string());
        if used + w > max {
            break;
        }
        used += w;
        kept.push(ch);
    }
    kept.reverse();
    format!("…{}", kept.into_iter().collect::<String>())
}

/// The key bound to an action in a context, as hints display it, so a hint
/// follows the user's remaps. The shortest binding wins (`Esc` over `Ctrl-c`).
pub(crate) fn key_hint(state: &AppState, context: &str, action: &str) -> Option<String> {
    state
        .keymap
        .bindings_in(context)
        .into_iter()
        .filter(|(_, call)| call.as_text() == action)
        .map(|(keys, _)| crate::config::keymap::display_keys(&keys))
        .min_by_key(|text| (text.chars().count(), text.clone()))
}

/// A row of `key label` hints that fits `width`: entries are kept in order
/// of importance, and the last one (how to leave) is always shown. Actions
/// with no binding are left out rather than advertising a dead key.
pub(crate) fn hints_line(
    state: &AppState,
    context: &str,
    items: &[(&str, &str)],
    width: usize,
) -> Line<'static> {
    let theme = &state.theme;
    let resolved: Vec<(String, &str)> = items
        .iter()
        .filter_map(|(action, label)| key_hint(state, context, action).map(|key| (key, *label)))
        .collect();
    let cost = |(key, label): &(String, &str)| {
        crate::util::text::width(key) + crate::util::text::width(label) + 3
    };
    let Some((last, rest)) = resolved.split_last() else { return Line::default() };
    let mut room = width.saturating_sub(cost(last) + 1);
    let mut chosen: Vec<&(String, &str)> = Vec::new();
    for item in rest {
        let needed = cost(item);
        if needed > room {
            break;
        }
        room -= needed;
        chosen.push(item);
    }
    chosen.push(last);
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in chosen {
        spans.push(Span::styled(key.clone(), theme.style("hint.key")));
        spans.push(Span::styled(format!(" {label}  "), theme.style("fg.subtle")));
    }
    Line::from(spans)
}
