//! Rendering. `draw` reads [`AppState`] and the [`Theme`]; it never mutates
//! them. Every screen and popup from `docs/design.md` lives here.

pub mod fx;
pub mod icons;
pub mod layout;
mod screens;
mod widgets;

// `screens` and `widgets` share this module as their namespace: each imports
// by name what it uses (`use crate::ui::{…}`), resolved through these.
use screens::*;
use widgets::*;

use crate::app::state::{
    AppState, EditorState, FormKey, FormKind, MessageLevel, Screen, SessionPhase,
};
use crate::bw::models::{Item, ItemType};
use crate::bw::service::ExportFormat;
use crate::config::keymap::display_keys;
use crate::theme::Theme;
use crate::util::text::{group_code, masked, pad, truncate};
use crate::vault::cache::Row;
use crate::vault::filter::SidebarRole;
use crate::vault::health::HealthCategory;
use icons::{Icons, Role, spinner_frame};
use layout::{LayoutPlan, centered, quick_open as quick_open_rect};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Draws the whole UI.
pub fn draw(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    // The same plan the mouse uses, with the widths dragged by the user.
    let plan = LayoutPlan::compute_sized(
        area,
        state.sidebar_visible,
        state.sidebar_width,
        state.list_width,
    );
    if plan.too_small {
        draw_too_small(frame, area, state);
        // Nothing animated is on screen, and an effect that is never
        // processed never ends.
        state.fx.borrow_mut().cancel();
        return;
    }
    let background = Style::default().bg(state.theme.colour("bg")).fg(state.theme.colour("fg"));
    frame.render_widget(Block::default().style(background), area);
    if plan.topbar.height > 0 {
        draw_topbar(frame, plan.topbar, state);
    }
    match state.screen {
        Screen::Starting => draw_starting(frame, plan.body, state),
        Screen::Error => draw_error(frame, plan.body, state),
        Screen::Login => draw_login(frame, plan.body, state),
        Screen::Unlock => draw_unlock(frame, plan.body, state),
        Screen::Vault => draw_vault(frame, &plan, state),
        Screen::Generator => draw_generator(frame, plan.body, state),
        Screen::Health => draw_health(frame, plan.body, state),
        Screen::Editor => draw_editor(frame, &plan, state),
        Screen::Settings => draw_settings(frame, plan.body, state),
        Screen::KeymapEditor => draw_keymap_editor(frame, plan.body, state),
        Screen::Help => draw_help(frame, plan.body, state),
        Screen::Messages => draw_messages(frame, plan.body, state),
    }
    draw_statusline(frame, plan.statusline, state);
    draw_message_line(frame, plan.message, state);
    draw_overlays(frame, area, plan.body, state);
    draw_toasts(frame, area, state);
    if state.which_key.is_some() {
        draw_which_key(frame, area, state);
    }
    let mut fx = state.fx.borrow_mut();
    if plan.body.width > 0 && plan.body.height > 0 {
        fx.observe_popups(state.overlay_open(), &state.theme, plan.body);
        fx.process(frame.buffer_mut(), plan.body);
    }
}
