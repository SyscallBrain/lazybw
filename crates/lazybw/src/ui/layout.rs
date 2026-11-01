//! Responsive layout: regions for the topbar, body, statusline and message
//! line, and the three-pane split with its breakpoints.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The layout mode for a width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// Three panes with a full sidebar (≥ 140 cols).
    Wide,
    /// Sidebar becomes a five-column rail (100–139).
    Medium,
    /// One pane at a time (< 100).
    Narrow,
}

impl Breakpoint {
    /// Picks the breakpoint for a width.
    #[must_use]
    pub fn for_width(width: u16) -> Self {
        if width >= 140 {
            Self::Wide
        } else if width >= 100 {
            Self::Medium
        } else {
            Self::Narrow
        }
    }
}

/// The regions of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutPlan {
    /// Topbar row.
    pub topbar: Rect,
    /// Body.
    pub body: Rect,
    /// Statusline row.
    pub statusline: Rect,
    /// Message row.
    pub message: Rect,
    /// Sidebar (empty in narrow mode).
    pub sidebar: Rect,
    /// The icon rail (medium mode).
    pub rail: Rect,
    /// Item list.
    pub list: Rect,
    /// Detail pane.
    pub detail: Rect,
    /// Breakpoint in use.
    pub breakpoint: Breakpoint,
    /// Compact mode forced by a short terminal.
    pub compact: bool,
    /// The terminal is too small for the app.
    pub too_small: bool,
}

impl LayoutPlan {
    /// Computes the layout for an area with the default pane widths.
    #[must_use]
    pub fn compute(area: Rect, sidebar_visible: bool) -> Self {
        Self::compute_sized(area, sidebar_visible, None, None)
    }

    /// Computes the layout with optional user-dragged pane widths.
    #[must_use]
    pub fn compute_sized(
        area: Rect,
        sidebar_visible: bool,
        sidebar_width: Option<u16>,
        list_width_override: Option<u16>,
    ) -> Self {
        let too_small = area.width < 60 || area.height < 16;
        let compact_height = area.height < 24;
        let topbar_height = if compact_height { 0 } else { 1 };
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(topbar_height),
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);
        let topbar = vertical[0];
        let body = vertical[1];
        let statusline = vertical[2];
        let message = vertical[3];
        let breakpoint = Breakpoint::for_width(area.width);
        let empty = Rect::new(body.x, body.y, 0, body.height);
        let (sidebar, rail, list, detail) = match (breakpoint, sidebar_visible) {
            (Breakpoint::Wide, true) => {
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(sidebar_width.unwrap_or(26).clamp(16, 40)),
                        Constraint::Length(
                            list_width_override
                                .unwrap_or_else(|| list_width(body.width.saturating_sub(26))),
                        ),
                        Constraint::Min(40),
                    ])
                    .split(body);
                (columns[0], empty, columns[1], columns[2])
            }
            (Breakpoint::Wide, false) => {
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(
                            list_width_override.unwrap_or_else(|| list_width(body.width)),
                        ),
                        Constraint::Min(40),
                    ])
                    .split(body);
                (empty, empty, columns[0], columns[1])
            }
            (Breakpoint::Medium, true) => {
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(5),
                        Constraint::Length(
                            list_width_override
                                .unwrap_or_else(|| list_width(body.width.saturating_sub(5))),
                        ),
                        Constraint::Min(40),
                    ])
                    .split(body);
                (empty, columns[0], columns[1], columns[2])
            }
            (Breakpoint::Medium, false) => {
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(list_width(body.width)), Constraint::Min(40)])
                    .split(body);
                (empty, empty, columns[0], columns[1])
            }
            (Breakpoint::Narrow, _) => (empty, empty, body, empty),
        };
        Self {
            topbar,
            body,
            statusline,
            message,
            sidebar,
            rail,
            list,
            detail,
            breakpoint,
            compact: compact_height,
            too_small,
        }
    }

    /// The main pane for the current focus in narrow mode.
    #[must_use]
    pub fn narrow_focus(&self) -> Rect {
        self.body
    }
}

fn list_width(available: u16) -> u16 {
    let preferred = (f32::from(available) * 0.4) as u16;
    preferred.clamp(36, 56).min(available.saturating_sub(48))
}

/// Is the editor drawn in two columns at this content width?
#[must_use]
pub fn editor_two_columns(content_width: u16) -> bool {
    content_width >= 118
}

/// The editor's content width for a terminal size (the sidebar stays when
/// it is visible; the editor takes the list and detail panes).
#[must_use]
pub fn editor_content_width(
    size: (u16, u16),
    sidebar_visible: bool,
    sidebar_width: Option<u16>,
    list_width: Option<u16>,
) -> u16 {
    let plan = LayoutPlan::compute_sized(
        Rect::new(0, 0, size.0, size.1),
        sidebar_visible,
        sidebar_width,
        list_width,
    );
    let area = if plan.sidebar.width > 0 { plan.list.union(plan.detail) } else { plan.body };
    area.width.saturating_sub(2)
}

/// Centres a rectangle of the given size in `area`.
#[must_use]
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// A popup anchored to the bottom, full width minus 2 columns.
#[must_use]
pub fn bottom_popup(area: Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    Rect {
        x: area.x + 1,
        y: area.y + area.height - height,
        width: area.width.saturating_sub(2),
        height,
    }
}

/// The quick-open popup: 84% × 70%.
#[must_use]
pub fn quick_open(area: Rect) -> Rect {
    let width = (u32::from(area.width) * 84 / 100) as u16;
    let height = (u32::from(area.height) * 70 / 100) as u16;
    centered(area, width.max(60), height.max(12))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    #[test]
    fn breakpoints() {
        assert_eq!(Breakpoint::for_width(160), Breakpoint::Wide);
        assert_eq!(Breakpoint::for_width(139), Breakpoint::Medium);
        assert_eq!(Breakpoint::for_width(120), Breakpoint::Medium);
        assert_eq!(Breakpoint::for_width(99), Breakpoint::Narrow);
    }

    #[test]
    fn wide_layout_has_three_panes() {
        let plan = LayoutPlan::compute(area(160, 45), true);
        assert_eq!(plan.sidebar.width, 26);
        assert!(plan.list.width >= 36);
        assert!(plan.detail.width >= 48);
        assert_eq!(plan.topbar.height, 1);
        assert_eq!(plan.statusline.height, 1);
        assert_eq!(plan.message.height, 1);
    }

    #[test]
    fn medium_layout_uses_a_rail() {
        let plan = LayoutPlan::compute(area(120, 36), true);
        assert_eq!(plan.rail.width, 5);
        assert_eq!(plan.sidebar.width, 0);
        assert!(plan.list.width >= 34);
        assert!(plan.detail.width >= 40);
    }

    #[test]
    fn narrow_layout_is_one_pane() {
        let plan = LayoutPlan::compute(area(90, 30), true);
        assert_eq!(plan.list.width, 90);
        assert_eq!(plan.detail.width, 0);
    }

    #[test]
    fn too_small_threshold() {
        assert!(LayoutPlan::compute(area(59, 20), true).too_small);
        assert!(LayoutPlan::compute(area(80, 15), true).too_small);
        assert!(!LayoutPlan::compute(area(60, 16), true).too_small);
    }

    #[test]
    fn short_terminal_merges_topbar() {
        let plan = LayoutPlan::compute(area(120, 22), true);
        assert_eq!(plan.topbar.height, 0);
        assert!(plan.compact);
    }

    #[test]
    fn popups_are_centered() {
        let popup = centered(area(100, 40), 40, 10);
        assert_eq!(popup.x, 30);
        assert_eq!(popup.y, 15);
        let quick = quick_open(area(160, 45));
        assert!(quick.width >= 60);
        assert!(quick.height >= 12);
    }
}
