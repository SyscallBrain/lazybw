//! Mouse clicks, wheel and divider drags.

use super::{
    Action, AppState, Effect, Screen, Tab, apply, detail_fields, navigation, sidebar_entries,
    switch_tab,
};

pub(crate) fn on_mouse(state: &mut AppState, event: crossterm::event::MouseEvent) -> Vec<Effect> {
    use crossterm::event::{MouseButton, MouseEventKind};
    let area = ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1);
    let plan = crate::ui::layout::LayoutPlan::compute_sized(
        area,
        state.sidebar_visible,
        state.sidebar_width,
        state.list_width,
    );
    let point = ratatui::layout::Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !plan.topbar.contains(point) {
                if plan.sidebar.width > 0 && near_right_edge(plan.sidebar, event.column) {
                    state.drag = Some(crate::app::state::DragTarget::Sidebar);
                    return Vec::new();
                }
                if plan.list.width > 0 && near_right_edge(plan.list, event.column) {
                    state.drag = Some(crate::app::state::DragTarget::List);
                    return Vec::new();
                }
            }
            click(state, &plan, event.column, event.row)
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            match state.drag {
                Some(crate::app::state::DragTarget::Sidebar) => {
                    state.sidebar_width =
                        Some(event.column.saturating_sub(plan.body.x).clamp(16, 40));
                }
                Some(crate::app::state::DragTarget::List) => {
                    let start = if plan.sidebar.width > 0 {
                        plan.sidebar.right()
                    } else if plan.rail.width > 0 {
                        plan.rail.right()
                    } else {
                        plan.body.x
                    };
                    state.list_width = Some(event.column.saturating_sub(start).clamp(28, 70));
                }
                None => {}
            }
            Vec::new()
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.drag = None;
            Vec::new()
        }
        MouseEventKind::ScrollDown => {
            wheel(state, true);
            Vec::new()
        }
        MouseEventKind::ScrollUp => {
            wheel(state, false);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn near_right_edge(rect: ratatui::layout::Rect, column: u16) -> bool {
    column + 1 == rect.x + rect.width || column + 2 == rect.x + rect.width
}

pub(crate) fn click(
    state: &mut AppState,
    plan: &crate::ui::layout::LayoutPlan,
    x: u16,
    y: u16,
) -> Vec<Effect> {
    let point = ratatui::layout::Position::new(x, y);
    if plan.topbar.contains(point) {
        // Tabs: " ◆ lazybw " then " 1 Vault " and so on.
        let mut cursor = plan.topbar.x + 11;
        for tab in [Tab::Vault, Tab::Generator, Tab::Health] {
            let width = format!(" {} {} ", tab.number(), tab.label()).chars().count() as u16;
            if x >= cursor && x < cursor + width {
                return switch_tab(state, tab);
            }
            cursor += width;
        }
        return Vec::new();
    }
    if plan.sidebar.width > 0 && plan.sidebar.contains(point) {
        let entries = sidebar_entries(state);
        let inner_y = y.saturating_sub(plan.sidebar.y + 1) as usize;
        let offset = state.ui_scroll.borrow().get("sidebar").copied().unwrap_or(0);
        if let Some((index, entry)) = entries.iter().enumerate().skip(offset).nth(inner_y)
            && entry.selectable()
        {
            state.sidebar_row = index;
            state.focus = crate::app::state::Focus::Sidebar;
            // A click on a parent's fold arrow (border, bar, space, indent)
            // opens or closes it instead of filtering.
            if let crate::vault::filter::SidebarRole::Folder { path, has_children: true, .. } =
                &entry.role
            {
                let depth = path.matches('/').count() as u16;
                let arrow = plan.sidebar.x + 3 + depth * 2;
                if x == arrow {
                    if !state.collapsed.remove(path) {
                        state.collapsed.insert(path.clone());
                    }
                    return Vec::new();
                }
            }
            if let Some(filter) = &entry.filter {
                state.filter = filter.clone();
                state.list_row = 0;
                state.list_offset = 0;
            }
        }
        return Vec::new();
    }
    if plan.rail.width > 0 && plan.rail.contains(point) {
        let entries = sidebar_entries(state);
        let inner_y = y.saturating_sub(plan.rail.y + 1) as usize;
        let slots = crate::vault::filter::rail_slots(&entries);
        if let Some(slot) = slots.get(inner_y).copied()
            && let Some(index) = slot.entry()
        {
            state.sidebar_row = index;
            state.focus = crate::app::state::Focus::Sidebar;
            if matches!(slot, crate::vault::filter::RailSlot::Folders(_)) {
                return apply(state, Action::new("folder.jump"));
            }
            if let Some(filter) = entries[index].filter.clone() {
                state.filter = filter;
                state.list_row = 0;
                state.list_offset = 0;
            }
        }
        return Vec::new();
    }
    if plan.list.width > 0 && plan.list.contains(point) {
        state.focus = crate::app::state::Focus::List;
        let comfortable = state.config.ui.list_density == crate::config::Density::Comfortable
            && plan.list.height >= 24;
        let row_height = if comfortable { 2 } else { 1 };
        let inner_y = y.saturating_sub(plan.list.y + 1) as usize;
        let offset = list_offset(state, plan.list.height.saturating_sub(2) as usize, row_height);
        let index = offset + inner_y / row_height;
        let len = state.visible_rows().len();
        if index < len {
            state.list_row = index;
        }
        return Vec::new();
    }
    if plan.detail.width > 0 && plan.detail.contains(point) {
        state.focus = crate::app::state::Focus::Detail;
        let inner_y = y.saturating_sub(plan.detail.y + 1) as usize;
        let fields = detail_fields(state).len();
        if fields > 0 {
            state.detail.row = inner_y.saturating_sub(3).min(fields - 1);
        }
        return Vec::new();
    }
    Vec::new()
}

pub(crate) fn list_offset(state: &AppState, height: usize, row_height: usize) -> usize {
    let visible = (height / row_height).max(1);
    let mut offset = state.ui_scroll.borrow().get("list").copied().unwrap_or(0);
    if state.list_row < offset {
        offset = state.list_row;
    } else if state.list_row >= offset + visible {
        offset = state.list_row + 1 - visible;
    }
    offset
}

pub(crate) fn wheel(state: &mut AppState, down: bool) {
    let action = if down { "nav.down" } else { "nav.up" };
    let area = ratatui::layout::Rect::new(0, 0, state.size.0, state.size.1);
    let plan = crate::ui::layout::LayoutPlan::compute_sized(
        area,
        state.sidebar_visible,
        state.sidebar_width,
        state.list_width,
    );
    let focus_sidebar = plan.sidebar.width > 0
        || plan.rail.width > 0 && state.focus == crate::app::state::Focus::Sidebar;
    for _ in 0..3 {
        if focus_sidebar && state.screen == Screen::Vault {
            state.focus = crate::app::state::Focus::Sidebar;
        }
        let effects = navigation(state, &Action::new(action));
        let _ = effects;
    }
    if state.detail.row > 0 {
        state.detail.offset = state.detail.row.saturating_sub(2);
    }
}
