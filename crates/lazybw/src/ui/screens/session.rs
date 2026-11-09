//! Starting, error, login, unlock and "terminal too small" screens.

use crate::ui::{
    Alignment, AppState, Block, BorderType, Borders, Constraint, Direction, Frame, Layout, Line,
    Paragraph, Rect, Span, Style, Wrap, blank, centered, draw_centered, logo, pane_block,
    spinner_frame, text_line, tick,
};

pub(crate) fn draw_starting(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let mut lines = logo(theme);
    lines.push(blank());
    lines.push(Line::from(Span::styled(
        format!("{} Starting Bitwarden CLI…", spinner_frame(tick(state))),
        theme.style("fg"),
    )));
    let cli = state
        .bw_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "locating bw…".to_owned());
    let version = state
        .capabilities
        .as_ref()
        .map(|c| c.version.to_string())
        .unwrap_or_else(|| "?".to_owned());
    lines.push(Line::from(Span::styled(format!("bw {version} · {cli}"), theme.style("fg.muted"))));
    // The master password typed ahead (`starting_key`), masked.
    let typed = state.unlock.password.char_count();
    if typed > 0 {
        let hint = if state.unlock.submit_when_ready {
            "unlocks when ready"
        } else {
            "Enter unlocks when ready"
        };
        lines.push(blank());
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", "●".repeat(typed.min(46))), theme.style("fg")),
            Span::styled(hint, theme.style("fg.muted")),
        ]));
    }
    draw_centered(frame, area, lines, state);
}

pub(crate) fn draw_error(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let Some(error) = state.error() else { return };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.colour("error")))
        .title(Span::styled(format!(" {} ", error.title), theme.bold("error")))
        .style(Style::default().bg(theme.colour("bg")));
    let mut lines: Vec<Line<'static>> =
        error.lines.iter().map(|line| text_line(theme, "fg", line.clone())).collect();
    if !error.detail.is_empty() {
        lines.push(blank());
        for line in error.detail.iter().take(20) {
            lines.push(text_line(theme, "fg.muted", format!("  {line}")));
        }
    }
    lines.push(blank());
    lines.push(text_line(theme, "fg.alt", error.actions.clone()));
    let rect = centered(area, 90, lines.len() as u16 + 2);
    frame.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), rect);
}

pub(crate) fn draw_login(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    use crate::app::state::{LoginFocus, LoginMethod, LoginServer, Pending};
    let theme = &state.theme;
    let login = &state.login;
    let mut lines = logo(theme);
    lines.push(Line::from(Span::styled("Log in to Bitwarden", theme.bold("fg"))));
    lines.push(blank());
    let servers = [
        (LoginServer::UnitedStates, "bitwarden.com", "United States"),
        (LoginServer::EuropeanUnion, "bitwarden.eu", "European Union"),
        (LoginServer::SelfHosted, "Self-hosted…", "your server URL"),
    ];
    let server_focused = matches!(login.focus, LoginFocus::Server | LoginFocus::ServerUrl);
    let rect = centered(area, 56, (lines.len() + 14) as u16);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(lines.len() as u16),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), chunks[0]);
    let block = pane_block("Server", "", server_focused, theme);
    let mut server_lines = Vec::new();
    for (server, name, note) in servers {
        let selected = login.server == server;
        let style = if selected && server_focused {
            theme.selected_row()
        } else if selected {
            theme.inactive_row()
        } else {
            theme.style("fg")
        };
        let bar = if selected { "▌" } else { " " };
        server_lines.push(Line::from(vec![
            Span::styled(bar.to_owned(), theme.style("accent")),
            Span::styled(format!(" {:<32}", name), style),
            Span::styled(format!("{note:<16}"), theme.style("fg.subtle")),
        ]));
    }
    frame.render_widget(Paragraph::new(server_lines).block(block), chunks[1]);
    let method_focused = !server_focused;
    let block = pane_block("Method", "", method_focused, theme);
    let methods = [
        (LoginMethod::Password, "Email and password", "two-step login"),
        (LoginMethod::Sso, "Single sign-on", "opens a browser"),
        (LoginMethod::ApiKey, "API key", "id and secret"),
    ];
    let mut method_lines = Vec::new();
    for (method, name, note) in methods {
        let selected = login.method == method;
        let style = if selected && method_focused {
            theme.selected_row()
        } else if selected {
            theme.inactive_row()
        } else {
            theme.style("fg")
        };
        let bar = if selected { "▌" } else { " " };
        method_lines.push(Line::from(vec![
            Span::styled(bar.to_owned(), theme.style("accent")),
            Span::styled(format!(" {:<32}", name), style),
            Span::styled(format!("{note:<16}"), theme.style("fg.subtle")),
        ]));
    }
    frame.render_widget(Paragraph::new(method_lines).block(block), chunks[2]);
    let cursor = |focus: LoginFocus| if login.focus == focus { "▏" } else { "" };
    let hint = if login.focus == LoginFocus::ServerUrl {
        format!("Server URL: {}▏", login.server_input)
    } else if state.logging_in() {
        let doing = if state.awaiting(|p| matches!(p, Pending::ConfigServer)) {
            "Saving the server…"
        } else {
            "Logging in…"
        };
        format!("{} {doing}", spinner_frame(tick(state)))
    } else if let Some(error) = &login.error {
        error.clone()
    } else if matches!(login.focus, LoginFocus::ClientId | LoginFocus::ClientSecret) {
        format!(
            "client_id: {}{}   client_secret: {}{}",
            login.client_id,
            cursor(LoginFocus::ClientId),
            "•".repeat(login.client_secret.char_count().min(20)),
            cursor(LoginFocus::ClientSecret),
        )
    } else {
        "Enter continue · j k move · Tab switch box · q quit".to_owned()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            hint,
            if login.error.is_some() { theme.style("error") } else { theme.style("fg.muted") },
        ))
        .alignment(Alignment::Center),
        chunks[3],
    );
}

pub(crate) fn draw_unlock(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let unlock = &state.unlock;
    let mut lines = logo(theme);
    lines.push(blank());
    lines.push(Line::from(Span::styled("Vault locked", theme.bold("fg"))));
    lines.push(Line::from(Span::styled(
        format!(
            "{} · {}",
            state.email.clone().unwrap_or_default(),
            state
                .server
                .clone()
                .unwrap_or_else(|| "bitwarden.com".to_owned())
                .replace("https://", "")
        ),
        theme.style("fg.muted"),
    )));
    let rect = centered(area, 52, (lines.len() + 8) as u16);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(lines.len() as u16),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(rect);
    let mut context = Vec::new();
    if let Some(last) = state.last_sync {
        context.push(format!("last sync {}", crate::util::time::relative(state.now, last)));
    }
    frame.render_widget(
        Paragraph::new(Span::styled(context.join(" · "), theme.style("fg.subtle")))
            .alignment(Alignment::Center),
        chunks[2],
    );
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), chunks[0]);
    let block = pane_block("Master password", "", true, theme);
    let shown = format!(" {}▏", "●".repeat(unlock.password.char_count().min(46)));
    let mut input_rect = chunks[1];
    let offset = state.fx.borrow().shake_offset();
    if offset != 0 {
        input_rect.x = input_rect.x.saturating_add_signed(offset);
    }
    frame.render_widget(
        Paragraph::new(Span::styled(shown, theme.style("fg"))).block(block),
        input_rect,
    );
    let line = if state.unlocking() {
        Line::from(Span::styled(
            format!("{} Unlocking…", spinner_frame(tick(state))),
            theme.style("info"),
        ))
    } else if let Some(error) = &unlock.error {
        Line::from(Span::styled(error.clone(), theme.style("error")))
    } else {
        Line::from(Span::styled(
            "Enter unlock    Ctrl-o log out    Ctrl-c quit",
            theme.style("fg.muted"),
        ))
    };
    frame
        .render_widget(Paragraph::new(vec![blank(), line]).alignment(Alignment::Center), chunks[3]);
}

pub(crate) fn draw_too_small(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let need = "lazybw needs at least 60×16.";
    let current = format!("Current: {}×{}.", area.width, area.height);
    let line = format!("{need} {current}");
    // Split in two rather than cut "Current: …" off in a narrow terminal.
    let lines: Vec<Line<'_>> = if crate::util::text::width(&line) <= usize::from(area.width) {
        vec![Line::from(line)]
    } else {
        vec![Line::from(need), Line::from(current)]
    };
    let height = (lines.len() as u16).min(area.height);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(state.theme.colour("fg.muted")))
            .alignment(Alignment::Center),
        centered(area, area.width, height),
    );
}
