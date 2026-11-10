//! The vault tab: sidebar, rail, item list and the detail pane.

use crate::bw::models::{Card, Identity, Login, SshKey};
use crate::ui::{
    Alignment, AppState, Block, BorderType, Borders, Frame, Icons, Item, ItemType, LayoutPlan,
    Line, Modifier, Paragraph, Rect, Role, Row, SidebarRole, Span, Style, Theme, blank,
    draw_scrollbar, fill_line, follow, group_code, icons, inner, layout, masked, meter_spans, pad,
    pane_block, pane_block_counted, password_spans, section_label, spans_width, spinner_frame,
    strength_label, text_line, tick, totp_bar, truncate, truncate_left, type_label, wrap_text,
};

pub(crate) fn draw_vault(frame: &mut Frame<'_>, plan: &LayoutPlan, state: &AppState) {
    let narrow = plan.breakpoint == layout::Breakpoint::Narrow;
    if narrow {
        match state.focus {
            crate::app::state::Focus::Sidebar => draw_sidebar(frame, plan.body, state),
            crate::app::state::Focus::List => draw_list(frame, plan.body, state),
            crate::app::state::Focus::Detail => draw_detail(frame, plan.body, state),
        }
        return;
    }
    if plan.sidebar.width > 0 {
        draw_sidebar(frame, plan.sidebar, state);
    } else if plan.rail.width > 0 {
        draw_rail(frame, plan.rail, state);
    }
    draw_list(frame, plan.list, state);
    draw_detail(frame, plan.detail, state);
}

pub(crate) fn draw_sidebar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let focused = state.focus == crate::app::state::Focus::Sidebar;
    let entries = crate::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    );
    let block = pane_block("Filters", "", focused, theme);
    let content = inner(area);
    let width = content.width as usize;
    let visible = content.height as usize;
    let offset = follow(state, "sidebar", state.sidebar_row, visible, entries.len());
    let icons = icons(state);
    let mut lines = Vec::new();
    for (index, entry) in entries.iter().enumerate().skip(offset).take(visible) {
        let selected = index == state.sidebar_row;
        match &entry.role {
            SidebarRole::Section(label) => {
                lines.push(Line::from(Span::styled(
                    format!(" {label}"),
                    theme.style("fg.subtle").add_modifier(Modifier::BOLD),
                )));
            }
            SidebarRole::Separator => {
                lines.push(Line::from(Span::styled(
                    format!(" {}", "─".repeat(width.saturating_sub(2))),
                    theme.style("border"),
                )));
            }
            role => {
                let (icon, token) = match role {
                    SidebarRole::All => (icons.get(Role::All), "fg.alt"),
                    SidebarRole::Favorites => (icons.get(Role::Favorite), "fav"),
                    SidebarRole::Type(kind) => {
                        (icons.get(Icons::item(*kind)), Theme::type_token(*kind))
                    }
                    SidebarRole::Folder { has_children, expanded, real, .. } => (
                        folder_glyph(icons, *has_children, *expanded),
                        if *real { "fg.alt" } else { "fg.muted" },
                    ),
                    SidebarRole::NoFolder => (folder_glyph(icons, false, false), "fg.muted"),
                    SidebarRole::Archive => (icons.get(Role::Archive), "fg.alt"),
                    SidebarRole::Trash => (icons.get(Role::Trash), "error"),
                    _ => (' ', "fg"),
                };
                let indent = match &entry.role {
                    SidebarRole::Folder { path, .. } => path.matches('/').count(),
                    _ => 0,
                };
                let active = entry.filter.as_ref() == Some(&state.filter);
                let row = if selected && focused {
                    theme.selected_row()
                } else if active {
                    theme.inactive_row()
                } else {
                    Style::default()
                };
                let label_style = match &entry.role {
                    SidebarRole::Folder { real: false, .. } => theme.style("fg.muted"),
                    _ => theme.style("fg"),
                };
                let bar = if selected && focused { "▌" } else { " " };
                let count = entry.count.map(|c| c.to_string()).unwrap_or_default();
                // Nerd Font: a fold arrow column before the folder icon, so a
                // closed parent never looks like a folder without subfolders.
                let arrow = match &entry.role {
                    SidebarRole::Folder { has_children, expanded, .. }
                        if icons.mode == crate::config::IconMode::Nerdfont =>
                    {
                        fold_arrow(*has_children, *expanded)
                    }
                    SidebarRole::NoFolder if icons.mode == crate::config::IconMode::Nerdfont => " ",
                    _ => "",
                };
                let prefix = format!(" {}{arrow}{} ", "  ".repeat(indent), icon);
                let label_width =
                    width.saturating_sub(1 + crate::util::text::width(&prefix) + count.len() + 2);
                lines.push(fill_line(
                    vec![
                        Span::styled(bar.to_owned(), theme.style("accent")),
                        Span::styled(prefix, theme.style(token)),
                        Span::styled(truncate(&entry.label, label_width.max(3)), label_style),
                    ],
                    vec![Span::styled(format!("{count} "), theme.style("fg.subtle"))],
                    width,
                    row,
                ));
            }
        }
    }
    if lines.is_empty() {
        lines.push(text_line(theme, "fg.muted", " No filters"));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    draw_scrollbar(frame, area, offset, visible, entries.len(), focused, theme);
}

/// The fold arrow of a folder row: open, closed, or none for a leaf.
pub(crate) fn fold_arrow(has_children: bool, expanded: bool) -> &'static str {
    match (has_children, expanded) {
        (true, true) => "▾",
        (true, false) => "▸",
        (false, _) => " ",
    }
}

/// The folder glyph. Nerd Font draws a folder (the arrow is a separate
/// column); Unicode and ASCII use the arrow itself, and a dot for leaves.
pub(crate) fn folder_glyph(icons: Icons, has_children: bool, expanded: bool) -> char {
    match icons.mode {
        crate::config::IconMode::Nerdfont => {
            icons.get(if has_children && expanded { Role::FolderOpen } else { Role::FolderClosed })
        }
        _ if has_children => {
            icons.get(if expanded { Role::FolderOpen } else { Role::FolderClosed })
        }
        crate::config::IconMode::Ascii => '-',
        crate::config::IconMode::Unicode => '·',
    }
}

pub(crate) fn draw_rail(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let focused = state.focus == crate::app::state::Focus::Sidebar;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.style(if focused { "border.focus" } else { "border" }))
        .style(Style::default().bg(theme.colour("bg")));
    let icons = icons(state);
    // Same entries as navigation, so the cursor and the icons agree.
    let entries = crate::vault::filter::entries(
        &state.cache,
        &state.collapsed,
        state.config.vault.show_empty_types,
    );
    let slots = crate::vault::filter::rail_slots(&entries);
    let cursor = crate::vault::filter::rail_position(&entries, &slots, state.sidebar_row);
    let in_folder = matches!(
        state.filter,
        crate::vault::cache::Filter::Folder(_) | crate::vault::cache::Filter::NoFolder
    );
    let mut lines = Vec::new();
    for (position, slot) in slots.iter().enumerate() {
        if lines.len() + 2 >= area.height as usize {
            break;
        }
        let selected = cursor == Some(position);
        let (glyph, token, active) = match slot {
            crate::vault::filter::RailSlot::Divider => ('─', "border", false),
            // One folder glyph opens the folder picker (<leader>ff).
            crate::vault::filter::RailSlot::Folders(_) => {
                (icons.get(Role::FolderOpen), "fg.alt", in_folder)
            }
            crate::vault::filter::RailSlot::Entry(index) => {
                let entry = &entries[*index];
                let active = entry.filter.as_ref() == Some(&state.filter);
                let (glyph, token) = match &entry.role {
                    SidebarRole::All => (icons.get(Role::All), "fg.alt"),
                    SidebarRole::Favorites => (icons.get(Role::Favorite), "fav"),
                    SidebarRole::Type(kind) => {
                        (icons.get(Icons::item(*kind)), Theme::type_token(*kind))
                    }
                    SidebarRole::Archive => (icons.get(Role::Archive), "fg.alt"),
                    SidebarRole::Trash => (icons.get(Role::Trash), "error"),
                    _ => (icons.get(Role::FolderClosed), "fg.alt"),
                };
                (glyph, token, active)
            }
        };
        let row = if selected && focused {
            theme.selected_row()
        } else if active {
            theme.inactive_row()
        } else {
            Style::default()
        };
        let bar = if (selected && focused) || active { "▌" } else { " " };
        lines.push(fill_line(
            vec![
                Span::styled(bar, theme.style("accent")),
                Span::styled(format!("{glyph}"), theme.style(token)),
            ],
            Vec::new(),
            area.width.saturating_sub(2) as usize,
            row,
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

pub(crate) fn draw_list(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let focused = state.focus == crate::app::state::Focus::List;
    let rows = state.visible_rows();
    let sort = state.config.vault.sort.label();
    let arrow = if state.sort_reversed { "↑" } else { "↓" };
    let right = format!("{sort} {arrow}");
    // Room for the title: the border, the right title and the count.
    let room = (area.width as usize)
        .saturating_sub(crate::util::text::width(&right) + rows.len().to_string().len() + 9);
    let title = truncate_left(&state.filter.display_title(), room.max(6));
    let block = pane_block_counted(title, Some(rows.len()), right, focused, theme);
    let content = inner(area);
    let width = content.width as usize;
    let comfortable =
        state.config.ui.list_density == crate::config::Density::Comfortable && area.height >= 24;
    let row_height = if comfortable { 2 } else { 1 };
    let visible = (content.height as usize / row_height).max(1);
    if rows.is_empty() {
        let loading;
        let (what, how) = match &state.filter {
            // Nothing has arrived yet: the vault is not empty.
            _ if state.session == crate::app::state::SessionPhase::Loading => {
                loading = format!("{} Loading the vault…", spinner_frame(tick(state)));
                (loading.as_str(), "")
            }
            crate::vault::cache::Filter::Trash => {
                ("No items in Trash", "Press dd on an item to move it to the trash.")
            }
            crate::vault::cache::Filter::Folder(_) => {
                ("No items in this folder", "Press a to add one, or m on an item to move it here.")
            }
            _ if state.search.query.is_some() => ("No matches", "Press Esc to clear the search."),
            _ => ("No items", "Press a to add one, or i to start an import."),
        };
        let mut lines = vec![blank(); (content.height as usize / 2).saturating_sub(1)];
        lines.push(Line::from(Span::styled(what, theme.style("fg"))));
        lines.push(Line::from(Span::styled(how, theme.style("fg.muted"))));
        frame.render_widget(Paragraph::new(lines).block(block).alignment(Alignment::Center), area);
        return;
    }
    let offset = follow(state, "list", state.list_row, visible, rows.len());
    let mut lines: Vec<Line<'static>> = Vec::new();
    let flash = state.fx.borrow().flash_style(theme);
    for (index, row) in rows.iter().enumerate().skip(offset).take(visible) {
        let current = index == state.list_row;
        let marked = state.is_selected(index);
        let style = if current && focused {
            flash.unwrap_or_else(|| theme.selected_row())
        } else if current || marked {
            theme.inactive_row()
        } else {
            Style::default()
        };
        let bar = if (current && focused) || marked { "▌" } else { " " };
        let bar_style = if marked && state.visual_anchor.is_some() {
            theme.style("mode.visual")
        } else {
            theme.style("accent")
        };
        let icon = brand_or_type(state, row);
        let indicators = row_indicators(state, row);
        let indicators_width = spans_width(&indicators);
        let name_width = width.saturating_sub(4 + indicators_width + 2);
        let name_style = if current { theme.bold("fg") } else { theme.style("fg") };
        let mut right = indicators;
        right.push(Span::raw(" "));
        lines.push(fill_line(
            vec![
                Span::styled(bar, bar_style),
                Span::styled(format!(" {icon} "), theme.style(Theme::type_token(row.kind))),
                Span::styled(truncate(&row.name, name_width), name_style),
            ],
            right,
            width,
            style,
        ));
        if comfortable {
            let subtitle = if row.deleted {
                state
                    .cache
                    .item(&row.id)
                    .and_then(|item| item.deleted_date.as_deref())
                    .and_then(|d| crate::util::time::relative_from_str(state.now, d))
                    .map(|r| format!("deleted {r}"))
                    .unwrap_or_default()
            } else {
                row.subtitle.clone()
            };
            lines.push(fill_line(
                vec![
                    Span::styled(bar, bar_style),
                    Span::raw("   "),
                    // The username is how two entries with the same name are
                    // told apart: legible colour, not the faintest one.
                    Span::styled(
                        truncate(&subtitle, width.saturating_sub(5)),
                        theme.style("fg.subtle"),
                    ),
                ],
                Vec::new(),
                width,
                style,
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    draw_scrollbar(frame, area, offset, visible, rows.len(), focused, theme);
}

pub(crate) fn brand_or_type(state: &AppState, row: &Row) -> char {
    let icons = icons(state);
    // Brands only exist in Nerd Font mode; the registrable domain is
    // resolved once per row (an `OnceLock` inside the row), so a frame only
    // runs the glyph match.
    if icons.mode == crate::config::IconMode::Nerdfont
        && row.kind == ItemType::Login
        && let Some(domain) = row.domain.get_or_init(|| {
            state
                .cache
                .item(&row.id)
                .and_then(|item| item.login.as_ref().and_then(|l| l.first_uri()))
                .and_then(crate::vault::dupes::registrable_domain)
        })
        && let Some(glyph) = icons.brand(domain)
    {
        return glyph;
    }
    icons.get(Icons::item(row.kind))
}

/// Right-aligned list indicators, in the order of design.md §4.5.
pub(crate) fn row_indicators(state: &AppState, row: &Row) -> Vec<Span<'static>> {
    let theme = &state.theme;
    let icons = icons(state);
    let mut out = Vec::new();
    let mut push = |text: String, token: &str| {
        out.push(Span::raw(" "));
        out.push(Span::styled(text, theme.style(token)));
    };
    if row.favorite {
        push(icons.get(Role::Favorite).to_string(), "fav");
    }
    if row.totp && state.config.security.totp {
        push(icons.get(Role::Totp).to_string(), "info");
    }
    if row.passkey {
        push(icons.get(Role::Passkey).to_string(), "fg.alt");
    }
    if row.attachments > 0 {
        push(format!("{}{}", icons.get(Role::Attachment), row.attachments), "fg.subtle");
    }
    if row.has_health_issue() {
        push(icons.get(Role::Warning).to_string(), if row.exposed { "error" } else { "warn" });
    }
    if row.reprompt {
        push(icons.get(Role::Reprompt).to_string(), "fg.subtle");
    }
    out
}

/// Detail-pane lines plus, for each selectable field, the line its value is on.
pub(crate) struct DetailView {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) anchors: Vec<(String, usize)>,
}

impl DetailView {
    fn label(&mut self, theme: &Theme, text: &str) {
        self.lines.push(section_label(theme, text));
    }

    fn value(&mut self, key: &str, line: Line<'static>) {
        self.anchors.push((key.to_owned(), self.lines.len()));
        self.lines.push(line);
    }
}

pub(crate) fn draw_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let focused = state.focus == crate::app::state::Focus::Detail;
    let Some(item) = state.selected_item() else {
        let block = pane_block("Detail", "", focused, theme);
        frame.render_widget(
            Paragraph::new(Span::styled("Select an item", theme.style("fg.muted")))
                .block(block)
                .alignment(Alignment::Center),
            area,
        );
        return;
    };
    let right = match item.kind {
        ItemType::Login => "Login",
        ItemType::Card => "Card",
        ItemType::Identity => "Identity",
        ItemType::SecureNote => "Secure note",
        ItemType::SshKey => "SSH key",
        ItemType::Other(_) => "Read-only",
    };
    let block = pane_block(item.name.clone(), right, focused, theme);
    let content = inner(area);
    let width = content.width as usize;
    let mut view = detail_content(state, item, width.saturating_sub(2));
    if item.is_deleted() {
        view.lines.insert(
            0,
            Line::from(Span::styled(
                "In trash · r restore · D delete permanently",
                theme.style("warn"),
            )),
        );
        for anchor in &mut view.anchors {
            anchor.1 += 1;
        }
    }
    // Inline hints: the active field when the detail pane has focus, the
    // copy shortcuts on their fields when the list has focus.
    for line in &mut view.lines {
        line.spans.insert(0, Span::raw(" "));
    }
    let fields = crate::app::update::detail_fields(state);
    let active_key = fields.get(state.detail.row).map(|(key, _, _)| key.clone());
    let list_focused = state.focus == crate::app::state::Focus::List;
    for (key, line_index) in &view.anchors {
        let active = focused && active_key.as_deref() == Some(key.as_str());
        let secret = fields.iter().any(|(k, _, s)| k == key && *s);
        let hint = if !state.config.ui.show_hints {
            ""
        } else if active {
            match key.as_str() {
                "passkey" => "dd remove",
                k if k.starts_with("uri:") => "gx open · yy copy",
                _ if secret => "za reveal · yy copy",
                _ => "yy copy",
            }
        } else if list_focused {
            match key.as_str() {
                "username" => "yu",
                "password" => "za reveal · yp",
                "totp" => "yt",
                "uri:0" => "gx",
                _ => "",
            }
        } else {
            ""
        };
        let Some(line) = view.lines.get_mut(*line_index) else { continue };
        let mut spans = std::mem::take(&mut line.spans);
        let row = if active { theme.active_row() } else { Style::default() };
        if active {
            // The gutter column carries the selection bar.
            spans[0] = Span::styled("▌", theme.style("accent"));
        }
        let hint_spans = if hint.is_empty() {
            Vec::new()
        } else {
            vec![Span::styled(hint.to_owned(), theme.style("fg.muted")), Span::raw(" ")]
        };
        // Leave the value intact when there is no room for the hint.
        let hint_spans = if spans_width(&spans) + spans_width(&hint_spans) + 2 > width {
            Vec::new()
        } else {
            hint_spans
        };
        *line = fill_line(spans, hint_spans, width, row);
    }
    let visible = content.height as usize;
    let active_line = active_key
        .and_then(|key| view.anchors.iter().find(|(k, _)| *k == key).map(|(_, l)| *l))
        .unwrap_or(0);
    let total = view.lines.len();
    let offset = if focused {
        follow(state, "detail", active_line + 1, visible, total)
    } else {
        state.ui_scroll.borrow_mut().insert("detail", 0);
        0
    };
    let shown: Vec<Line<'static>> = view.lines.into_iter().skip(offset).take(visible).collect();
    frame.render_widget(Paragraph::new(shown).block(block), area);
    draw_scrollbar(frame, area, offset, visible, total, focused, theme);
}

pub(crate) fn detail_content(state: &AppState, item: &Item, width: usize) -> DetailView {
    let theme = &state.theme;
    let icons = icons(state);
    let mut view = DetailView { lines: Vec::new(), anchors: Vec::new() };
    let folder = item
        .folder_id
        .as_ref()
        .and_then(|id| state.cache.folder_path(id))
        .map(|p| format!(" · {p}"))
        .unwrap_or_default();
    let edited = item
        .revision_date
        .as_deref()
        .and_then(|d| crate::util::time::relative_from_str(state.now, d))
        .map(|r| format!(" · edited {r}"))
        .unwrap_or_default();
    let mut header = vec![
        Span::styled(
            format!("{} ", icons.get(Icons::item(item.kind))),
            theme.style(Theme::type_token(item.kind)),
        ),
        Span::styled(item.name.clone(), theme.bold("fg")),
    ];
    let mut header_right = Vec::new();
    if item.favorite {
        header_right.push(Span::styled(
            format!("{} favorite", icons.get(Role::Favorite)),
            theme.style("fav"),
        ));
    }
    if item.wants_reprompt() {
        header_right.push(Span::styled(
            format!("  {} ", icons.get(Role::Reprompt)),
            theme.style("fg.subtle"),
        ));
    }
    if header_right.is_empty() {
        header.push(Span::raw(""));
    }
    view.lines.push(fill_line(header, header_right, width, Style::default()));
    view.lines.push(text_line(
        theme,
        "fg.muted",
        format!("{}{folder}{edited}", type_label(item.kind)),
    ));
    let revealed = |key: &str| state.detail.revealed.contains(key);
    let reprompt = item.wants_reprompt();
    let hidden = |len: usize| -> Line<'static> {
        if reprompt {
            Line::from(vec![
                Span::styled("••••••", theme.style("fg.alt")),
                Span::styled(" (master password required)", theme.style("fg.subtle")),
            ])
        } else {
            Line::from(Span::styled(masked(len, 20), theme.style("fg.alt")))
        }
    };
    let plain = |value: String| Line::from(Span::styled(value, theme.style("fg")));
    match item.kind {
        ItemType::Login => {
            // Borrowed: the whole login (username, password, TOTP) is cloned
            // per frame otherwise, and every dropped `Secrets` runs zeroize.
            let login_default = Login::default();
            let login = item.login.as_ref().unwrap_or(&login_default);
            view.label(theme, "USERNAME");
            view.value(
                "username",
                plain(login.username.as_ref().map(|s| s.expose().to_owned()).unwrap_or_default()),
            );
            let password = login.password.as_ref().map(|s| s.expose()).unwrap_or("");
            view.label(theme, "PASSWORD");
            if revealed("password") {
                view.value(
                    "password",
                    Line::from(password_spans(theme, password, Style::default())),
                );
            } else if password.is_empty() {
                view.value("password", Line::from(Span::styled("—", theme.style("fg.subtle"))));
            } else {
                view.value("password", hidden(password.chars().count()));
            }
            if !password.is_empty() && !reprompt {
                let strength = state
                    .health
                    .report
                    .as_ref()
                    .and_then(|r| item.id.as_ref().and_then(|id| r.entry(id)))
                    .map(|record| record.strength)
                    .unwrap_or_else(|| crate::vault::health::local_strength(password));
                let changed = login
                    .password_revision_date
                    .as_deref()
                    .or(item.creation_date.as_deref())
                    .and_then(|d| crate::util::time::relative_from_str(state.now, d))
                    .map(|r| format!(" · changed {r}"))
                    .unwrap_or_default();
                let mut spans = meter_spans(theme, strength);
                spans.push(Span::styled(
                    format!(" {}{changed}", strength_label(strength)),
                    theme.style("fg.muted"),
                ));
                view.lines.push(Line::from(spans));
            }
            if login.has_totp() && state.config.security.totp {
                view.label(theme, "VERIFICATION CODE");
                if let Ok(code) = crate::totp::current(
                    login.totp.as_ref().map(|s| s.expose()).unwrap_or(""),
                    state.now.timestamp() as u64,
                ) {
                    let grouped = if code.steam {
                        code.code.clone()
                    } else {
                        group_code(&code.code, code.digits as usize)
                    };
                    let code_style = state
                        .fx
                        .borrow()
                        .totp_style(theme)
                        .unwrap_or_else(|| theme.bold("totp.code"));
                    view.value("totp", Line::from(Span::styled(grouped, code_style)));
                    view.lines.push(totp_bar(state, &code, width));
                } else {
                    view.value("totp", text_line(theme, "warn", "Not a valid authenticator key"));
                }
            }
            if let Some(uris) = &login.uris
                && uris.iter().any(|u| !u.uri_str().is_empty())
            {
                view.label(theme, "WEBSITES");
                for (index, uri) in uris.iter().enumerate() {
                    if uri.uri_str().is_empty() {
                        continue;
                    }
                    let insecure = uri.uri_str().starts_with("http://");
                    view.value(
                        &format!("uri:{index}"),
                        Line::from(vec![
                            Span::styled(uri.uri_str().to_owned(), theme.style("link")),
                            Span::styled(
                                format!("  {}", uri.match_kind().label().to_lowercase()),
                                theme.style("fg.subtle"),
                            ),
                            if insecure {
                                Span::styled("  no HTTPS", theme.style("warn"))
                            } else {
                                Span::raw("")
                            },
                        ]),
                    );
                }
            }
            if let Some(credentials) = login.fido2_credentials.as_ref().filter(|c| !c.is_empty()) {
                view.label(theme, "PASSKEY");
                let first = &credentials[0];
                let rp = first.get("rpId").and_then(|v| v.as_str()).unwrap_or("");
                let user = first.get("userName").and_then(|v| v.as_str()).unwrap_or("");
                let created = first
                    .get("creationDate")
                    .and_then(|v| v.as_str())
                    .and_then(|d| {
                        crate::util::time::date(state.now, d, state.config.ui.date_format.is_iso())
                    })
                    .map(|d| format!(" · created {d}"))
                    .unwrap_or_default();
                view.value(
                    "passkey",
                    Line::from(vec![
                        Span::styled(format!("{rp} · {user}{created}"), theme.style("fg")),
                        Span::styled("  read-only", theme.style("fg.subtle")),
                    ]),
                );
            }
        }
        ItemType::Card => {
            let card_default = Card::default();
            let card = item.card.as_ref().unwrap_or(&card_default);
            view.label(theme, "CARDHOLDER NAME");
            view.value("cardholder", plain(card.cardholder_name.clone().unwrap_or_default()));
            view.label(theme, "NUMBER");
            let number = card.number.as_ref().map(|s| s.expose()).unwrap_or("");
            let brand = card.brand.as_deref().unwrap_or("");
            let mut spans = if revealed("number") {
                vec![Span::styled(
                    crate::util::card::mask_card_number(number, true),
                    theme.style("fg"),
                )]
            } else if number.is_empty() {
                vec![Span::styled("—", theme.style("fg.subtle"))]
            } else if reprompt {
                hidden(0).spans
            } else {
                vec![Span::styled(
                    crate::util::card::mask_card_number(number, false),
                    theme.style("fg.alt"),
                )]
            };
            if !brand.is_empty() {
                spans.push(Span::styled(format!("   {brand}"), theme.style("fg.subtle")));
            }
            view.value("number", Line::from(spans));
            view.label(theme, "EXPIRATION");
            let month = card.exp_month.as_deref().unwrap_or("");
            let year = card.exp_year.as_deref().unwrap_or("");
            let expired = match (month.parse::<u32>(), year.parse::<i32>()) {
                (Ok(m), Ok(y)) => {
                    use chrono::Datelike;
                    (y, m) < (state.now.year(), state.now.month())
                }
                _ => false,
            };
            let expiry = if month.is_empty() && year.is_empty() {
                Line::from(Span::styled("—", theme.style("fg.subtle")))
            } else if expired {
                Line::from(vec![
                    Span::styled(format!("{month:0>2} / {year}"), theme.style("warn")),
                    Span::styled("  expired", theme.style("warn")),
                ])
            } else {
                plain(format!("{month:0>2} / {year}"))
            };
            view.value("expiry", expiry);
            view.label(theme, "SECURITY CODE");
            let code = card.code.as_ref().map(|s| s.expose()).unwrap_or("");
            view.value(
                "code",
                if code.is_empty() {
                    Line::from(Span::styled("—", theme.style("fg.subtle")))
                } else if revealed("code") {
                    plain(code.to_owned())
                } else {
                    hidden(code.chars().count())
                },
            );
        }
        ItemType::Identity => {
            let identity_default = Identity::default();
            let identity = item.identity.as_ref().unwrap_or(&identity_default);
            view.label(theme, "NAME");
            view.value("id:name", plain(identity.full_name()));
            view.label(theme, "CONTACT");
            let contact: Vec<(&str, &str)> = [
                ("Email", identity.email.as_deref()),
                ("Phone", identity.phone.as_deref()),
                ("Username", identity.username.as_deref()),
                ("Company", identity.company.as_deref()),
            ]
            .into_iter()
            .filter_map(|(label, value)| value.filter(|v| !v.is_empty()).map(|v| (label, v)))
            .collect();
            if contact.is_empty() {
                view.value("id:contact", Line::from(Span::styled("—", theme.style("fg.subtle"))));
            }
            for (index, (label, value)) in contact.into_iter().enumerate() {
                let line = Line::from(vec![
                    Span::styled(format!("{label:<12}"), theme.style("fg.muted")),
                    Span::styled(value.to_owned(), theme.style("fg")),
                ]);
                if index == 0 {
                    view.value("id:contact", line);
                } else {
                    view.lines.push(line);
                }
            }
            view.label(theme, "ADDRESS");
            let city_line = [
                identity.postal_code.as_deref(),
                identity.city.as_deref(),
                identity.state.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
            let address: Vec<&str> = [
                identity.address1.as_deref(),
                identity.address2.as_deref(),
                identity.address3.as_deref(),
                (!city_line.is_empty()).then_some(city_line.as_str()),
                identity.country.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect();
            if address.is_empty() {
                view.value("id:address", Line::from(Span::styled("—", theme.style("fg.subtle"))));
            }
            for (index, line) in address.into_iter().enumerate() {
                if index == 0 {
                    view.value("id:address", plain(line.to_owned()));
                } else {
                    view.lines.push(plain(line.to_owned()));
                }
            }
            view.label(theme, "IDENTIFIERS");
            for (key, label, secret) in [
                ("id:ssn", "SSN", &identity.ssn),
                ("id:passport", "Passport number", &identity.passport_number),
                ("id:license", "Licence number", &identity.license_number),
            ] {
                let value = secret.as_ref().map(|s| s.expose()).unwrap_or("");
                let mut spans = vec![Span::styled(format!("{label:<18}"), theme.style("fg.muted"))];
                if value.is_empty() {
                    spans.push(Span::styled("—", theme.style("fg.subtle")));
                } else if revealed(key) {
                    spans.push(Span::styled(value.to_owned(), theme.style("fg")));
                } else {
                    spans.extend(hidden(value.chars().count()).spans);
                }
                view.value(key, Line::from(spans));
            }
        }
        ItemType::SecureNote => {
            let notes = item.notes.as_ref().map(|n| n.expose()).unwrap_or("");
            if reprompt && !revealed("notes") {
                view.value("notes", hidden(0));
            } else if notes.lines().next().is_none() {
                view.value(
                    "notes",
                    Line::from(Span::styled("Empty note", theme.style("fg.subtle"))),
                );
            } else {
                // Plain text, soft-wrapped, with relative line numbers while
                // the pane has focus (like Vim's relativenumber).
                let focused = state.focus == crate::app::state::Focus::Detail;
                let current = state.detail.row;
                let total = notes.lines().count();
                let number_width = total.to_string().len().max(2);
                let text_width = width.saturating_sub(number_width + 2).max(10);
                for (index, line) in notes.lines().enumerate() {
                    let wrapped = wrap_text(line, text_width);
                    for (part_index, part) in wrapped.into_iter().enumerate() {
                        let mut spans = Vec::new();
                        if focused {
                            let number = if part_index > 0 {
                                String::new()
                            } else if index == current {
                                (index + 1).to_string()
                            } else {
                                index.abs_diff(current).to_string()
                            };
                            let style = if index == current && part_index == 0 {
                                theme.bold("accent")
                            } else {
                                theme.style("fg.subtle")
                            };
                            spans.push(Span::styled(format!("{number:>number_width$} "), style));
                        }
                        spans.push(Span::styled(part, theme.style("fg")));
                        if part_index == 0 {
                            view.value(&format!("note:{index}"), Line::from(spans));
                        } else {
                            view.lines.push(Line::from(spans));
                        }
                    }
                }
            }
        }
        ItemType::SshKey => {
            let key_default = SshKey::default();
            let key = item.ssh_key.as_ref().unwrap_or(&key_default);
            view.label(theme, "PUBLIC KEY");
            view.value("public_key", plain(truncate(&key.public_key, width.saturating_sub(10))));
            view.label(theme, "FINGERPRINT");
            view.value("fingerprint", plain(key.key_fingerprint.clone()));
            view.label(theme, "PRIVATE KEY");
            let private = key.private_key.as_ref().map(|s| s.expose()).unwrap_or("");
            if revealed("private_key") {
                let mut first = true;
                for line in private.lines() {
                    if first {
                        view.value("private_key", plain(line.to_owned()));
                        first = false;
                    } else {
                        view.lines.push(plain(line.to_owned()));
                    }
                }
            } else if reprompt {
                view.value("private_key", hidden(0));
            } else {
                view.value(
                    "private_key",
                    Line::from(vec![
                        Span::styled("-----BEGIN OPENSSH PRIVATE KEY----- ", theme.style("fg.alt")),
                        Span::styled("••••", theme.style("fg.alt")),
                    ]),
                );
            }
        }
        ItemType::Other(_) => {
            view.lines.push(Line::from(Span::styled(
                format!("This item type ({}) is shown read-only.", type_label(item.kind)),
                theme.style("warn"),
            )));
            for (label, value, secret) in item.unknown_fields() {
                view.label(theme, &label.to_uppercase());
                let key = format!("unknown:{label}");
                let line = if secret && !revealed(&key) {
                    hidden(value.chars().count())
                } else {
                    plain(value)
                };
                view.value(&key, line);
            }
        }
    }
    // Custom fields, for every type.
    if let Some(fields) = item.fields.as_ref().filter(|f| !f.is_empty()) {
        view.label(theme, "CUSTOM FIELDS");
        let name_width = fields
            .iter()
            .map(|f| crate::util::text::width(f.name_str()))
            .max()
            .unwrap_or(0)
            .clamp(8, 24)
            + 2;
        for (index, field) in fields.iter().enumerate() {
            let key = format!("field:{index}");
            let value = field.value.as_ref().map(|v| v.expose()).unwrap_or("");
            let mut spans = vec![Span::styled(
                pad(&truncate(field.name_str(), name_width - 2), name_width),
                theme.style("fg.muted"),
            )];
            match field.kind() {
                crate::bw::models::FieldKind::Hidden if !revealed(&key) => {
                    spans.extend(hidden(value.chars().count()).spans);
                }
                crate::bw::models::FieldKind::Boolean => spans.push(if value == "true" {
                    Span::styled("☑ yes", theme.style("ok"))
                } else {
                    Span::styled("☐ no", theme.style("fg.subtle"))
                }),
                crate::bw::models::FieldKind::Linked => spans.push(Span::styled(
                    field.linked_id.map_or_else(
                        || "→ linked field".to_owned(),
                        |id| format!("→ linked field {id}"),
                    ),
                    theme.style("fg.alt"),
                )),
                _ => spans.push(Span::styled(value.to_owned(), theme.style("fg"))),
            }
            spans.push(Span::styled(
                format!("  {}", field.kind().label()),
                theme.style("fg.subtle"),
            ));
            view.value(&key, Line::from(spans));
        }
    }
    if item.kind != ItemType::SecureNote
        && let Some(notes) = item.notes.as_ref().filter(|n| !n.is_empty())
    {
        view.label(theme, "NOTES");
        if reprompt && !revealed("notes") {
            view.value("notes", hidden(0));
        } else {
            for (index, line) in notes.expose().lines().take(40).enumerate() {
                if index == 0 {
                    view.value("notes", plain(line.to_owned()));
                } else {
                    view.lines.push(plain(line.to_owned()));
                }
            }
        }
    }
    // Footer.
    view.lines.push(Line::from(Span::styled("─".repeat(width), theme.style("border"))));
    let created = item
        .creation_date
        .as_deref()
        .and_then(|d| crate::util::time::date(state.now, d, state.config.ui.date_format.is_iso()))
        .unwrap_or_default();
    let mut footer_right = Vec::new();
    if item.kind == ItemType::Login {
        let history = item.password_history.as_ref().map_or(0, Vec::len);
        footer_right
            .push(Span::styled(format!("password history ({history}) "), theme.style("fg.subtle")));
        footer_right.push(Span::styled("gh ", theme.style("hint.key")));
    }
    let created = if created.is_empty() { String::new() } else { format!("created {created}") };
    view.lines.push(fill_line(
        vec![Span::styled(created, theme.style("fg.subtle"))],
        footer_right,
        width,
        Style::default(),
    ));
    for warning in health_warnings(state, item) {
        view.lines.push(Line::from(Span::styled(warning, theme.style("warn"))));
    }
    view
}

pub(crate) fn health_warnings(state: &AppState, item: &Item) -> Vec<String> {
    let Some(id) = &item.id else { return Vec::new() };
    let Some(report) = &state.health.report else { return Vec::new() };
    let Some(record) = report.entry(id) else { return Vec::new() };
    let mut out = Vec::new();
    if record.flags.exposed {
        let seen = record.exposed_count.unwrap_or(0);
        out.push(format!(
            "Exposed: seen {} times in data breaches.",
            crate::util::time::thousands(seen)
        ));
    }
    if record.flags.reused {
        out.push("Reused: another item uses this password.".to_owned());
    }
    if record.flags.weak {
        out.push("Weak password.".to_owned());
    }
    if record.flags.old {
        out.push("Old password.".to_owned());
    }
    if record.flags.insecure {
        out.push("Insecure website (http://).".to_owned());
    }
    out
}
