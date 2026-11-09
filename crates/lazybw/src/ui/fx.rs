//! Motion: every effect from `docs/design.md` §7.
//!
//! Visual only: input is processed immediately and effects never delay state
//! changes. Everything is gated by `ui.animations` and disabled by `NO_COLOR`
//! (the caller sets `enabled`). Screen fades use tachyonfx; the shake, row
//! flash and toast sweep are small state machines so they compose with the
//! hand-drawn widgets.

use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use std::time::{Duration, Instant};
use tachyonfx::{Interpolation, Motion, fx};

/// How long a new toast takes to slide in from the right.
const TOAST_SLIDE: Duration = Duration::from_millis(180);

/// The trigger points from design.md §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
    /// A popup fades in from the float background.
    PopupOpen,
    /// A popup fades out.
    PopupClose,
    /// A toast sweeps in from the right.
    ToastIn,
    /// Rows dissolve when an item is trashed.
    Dissolve,
    /// The copied row flashes.
    Flash,
    /// TOTP digits coalesce when the code changes.
    Coalesce,
    /// The master password box shakes.
    Shake,
    /// The panes sweep in after unlocking.
    Sweep,
    /// A tab fades in.
    TabFade,
}

/// The animation manager.
pub struct Fx {
    manager: tachyonfx::EffectManager<String>,
    enabled: bool,
    last: Instant,
    shake_since: Option<Instant>,
    flash_since: Option<Instant>,
    totp_since: Option<Instant>,
    popup_open: bool,
}

impl std::fmt::Debug for Fx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fx")
            .field("enabled", &self.enabled)
            .field("running", &self.manager.is_running())
            .field("popup_open", &self.popup_open)
            .finish_non_exhaustive()
    }
}

impl Fx {
    /// A manager, enabled when animations are on.
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self {
            manager: tachyonfx::EffectManager::default(),
            enabled,
            last: Instant::now(),
            shake_since: None,
            flash_since: None,
            totp_since: None,
            popup_open: false,
        }
    }

    /// Enables or disables motion; disabling cancels running effects.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.cancel();
        }
    }

    /// Drops every running effect. Used when no frame can show them (the
    /// terminal is too small): a tachyonfx effect only advances when
    /// [`Fx::process`] runs, so it would otherwise stay "active" and keep
    /// the main loop drawing frames for nothing.
    pub fn cancel(&mut self) {
        self.manager = tachyonfx::EffectManager::default();
        self.shake_since = None;
        self.flash_since = None;
        self.totp_since = None;
    }

    /// True when a frame should be requested.
    #[must_use]
    pub fn is_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        self.manager.is_running()
            || self.shake_since.is_some_and(|since| since.elapsed() < Duration::from_millis(300))
            || self.flash_since.is_some_and(|since| since.elapsed() < Duration::from_millis(250))
            || self.totp_since.is_some_and(|since| since.elapsed() < Duration::from_millis(200))
    }

    /// Fires an effect for the whole body area.
    pub fn trigger(&mut self, kind: FxKind, theme: &Theme, area: Rect) {
        if !self.enabled {
            return;
        }
        let bg = theme.colour("bg");
        let float = theme.colour("bg.float");
        match kind {
            FxKind::PopupOpen | FxKind::PopupClose => {
                let effect = fx::fade_from(float, bg, (160u32, Interpolation::QuadOut));
                self.manager.add_unique_effect(format!("{kind:?}"), effect);
            }
            FxKind::Sweep => {
                let effect =
                    fx::sweep_in(Motion::LeftToRight, 6, 0, bg, (250u32, Interpolation::QuadOut));
                self.manager.add_unique_effect(format!("{kind:?}"), effect);
            }
            FxKind::TabFade => {
                let effect = fx::fade_from(bg, bg, (120u32, Interpolation::Linear));
                self.manager.add_unique_effect(format!("{kind:?}"), effect);
            }
            FxKind::Dissolve => {
                self.flash_since = Some(Instant::now());
            }
            FxKind::Flash => {
                self.flash_since = Some(Instant::now());
            }
            FxKind::Coalesce => {
                self.totp_since = Some(Instant::now());
            }
            FxKind::Shake => {
                self.shake_since = Some(Instant::now());
            }
            FxKind::ToastIn => {}
        }
        let _ = area;
    }

    /// Applies running effects to the buffer.
    pub fn process(&mut self, buffer: &mut Buffer, area: Rect) {
        if !self.enabled {
            return;
        }
        let elapsed = self.last.elapsed().min(Duration::from_millis(50));
        self.last = Instant::now();
        let millis = elapsed.as_millis().min(u128::from(u32::MAX)) as u32;
        self.manager.process_effects(tachyonfx::Duration::from_millis(millis), buffer, area);
    }

    /// Detects popup open/close transitions and animates them.
    pub fn observe_popups(&mut self, open: bool, theme: &Theme, area: Rect) {
        if open != self.popup_open {
            let kind = if open { FxKind::PopupOpen } else { FxKind::PopupClose };
            self.trigger(kind, theme, area);
        }
        self.popup_open = open;
    }

    /// The horizontal shake offset in columns (±1), or 0.
    #[must_use]
    pub fn shake_offset(&self) -> i16 {
        let Some(since) = self.shake_since else { return 0 };
        let elapsed = since.elapsed().as_millis();
        if elapsed >= 300 {
            return 0;
        }
        // Three oscillations in 300 ms.
        let phase = (elapsed / 50) % 4;
        match phase {
            0 => 1,
            1 => -1,
            2 => 1,
            _ => -1,
        }
    }

    /// True while the copied or trashed row should flash.
    #[must_use]
    pub fn flash_active(&self) -> bool {
        self.flash_since.is_some_and(|since| since.elapsed() < Duration::from_millis(250))
    }

    /// True while TOTP digits should stand out after a change.
    #[must_use]
    pub fn totp_flash(&self) -> bool {
        self.totp_since.is_some_and(|since| since.elapsed() < Duration::from_millis(200))
    }

    /// The style override for a flashing row.
    #[must_use]
    pub fn flash_style(&self, theme: &Theme) -> Option<Style> {
        self.flash_active()
            .then(|| Style::default().bg(theme.colour("selection")).fg(theme.colour("fg")))
    }

    /// The style for flashing TOTP digits.
    #[must_use]
    pub fn totp_style(&self, theme: &Theme) -> Option<Style> {
        self.totp_flash().then(|| theme.style("totp.code").add_modifier(Modifier::BOLD))
    }

    /// True while a toast created at `created` is still sliding in.
    #[must_use]
    pub fn toast_sliding(&self, created: Instant) -> bool {
        self.enabled && created.elapsed() < TOAST_SLIDE
    }

    /// The sweep-in offset for a toast that just appeared.
    #[must_use]
    pub fn toast_offset(&self, created: Instant, width: u16) -> u16 {
        if !self.enabled {
            return 0;
        }
        let total = TOAST_SLIDE.as_millis() as u32;
        let elapsed = created.elapsed().as_millis().min(u128::from(total)) as u32;
        ((u32::from(width) * (total - elapsed) / total) as u16).min(width)
    }

    /// A dimming cell style for the popup backdrop.
    #[must_use]
    pub fn backdrop_style(theme: &Theme, progress: f32) -> Style {
        let fg = blend(theme.colour("fg"), theme.colour("bg.bar"), 0.55 * progress);
        let bg = blend(theme.colour("bg"), theme.colour("bg.bar"), 0.55 * progress);
        Style::default().fg(fg).bg(bg)
    }

    /// True when a colour is a concrete RGB/index colour.
    #[must_use]
    pub fn is_colour(color: Color) -> bool {
        !matches!(color, Color::Reset)
    }
}

/// Blends `from` toward `to` by `amount` (0–1), for RGB and indexed colours.
#[must_use]
pub fn blend(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    match (from, to) {
        (Color::Rgb(fr, fg, fb), Color::Rgb(tr, tg, tb)) => {
            Color::Rgb(mix(fr, tr, amount), mix(fg, tg, amount), mix(fb, tb, amount))
        }
        (Color::Indexed(f), Color::Indexed(t)) => Color::Indexed(mix(f, t, amount)),
        (other, _) => other,
    }
}

fn mix(from: u8, to: u8, amount: f32) -> u8 {
    (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_manager_is_inactive() {
        let mut fx = Fx::new(false);
        let theme = crate::theme::builtin::by_name("nord").unwrap();
        fx.trigger(FxKind::Shake, &theme, Rect::new(0, 0, 10, 10));
        assert!(!fx.is_active());
        assert_eq!(fx.shake_offset(), 0);
    }

    #[test]
    fn shake_decays() {
        let theme = crate::theme::builtin::by_name("nord").unwrap();
        let mut fx = Fx::new(true);
        fx.trigger(FxKind::Shake, &theme, Rect::new(0, 0, 10, 10));
        assert!(fx.is_active());
        assert_ne!(fx.shake_offset(), 0);
        std::thread::sleep(Duration::from_millis(320));
        assert_eq!(fx.shake_offset(), 0);
    }

    #[test]
    fn blending_moves_towards_the_target() {
        assert_eq!(
            blend(Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255), 0.5),
            Color::Rgb(128, 128, 128)
        );
        assert_eq!(blend(Color::Rgb(1, 2, 3), Color::Rgb(9, 9, 9), 0.0), Color::Rgb(1, 2, 3));
        assert_eq!(blend(Color::Red, Color::Blue, 0.5), Color::Red);
    }
}
