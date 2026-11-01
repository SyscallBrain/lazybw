//! One module per screen or group of screens (design.md §4).

mod editor;
mod overlays;
mod session;
mod settings;
mod tools;
mod vault;

pub(crate) use editor::*;
pub(crate) use overlays::*;
pub(crate) use session::*;
pub(crate) use settings::*;
pub(crate) use tools::*;
pub(crate) use vault::*;
