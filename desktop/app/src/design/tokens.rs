//! The palette, type scale and spacing every screen draws from.
//!
//! A neutral near-black canvas with grey steps for surfaces: the artwork is the colour on
//! screen, and the interface stays out of its way. Amber is the one accent and is used only
//! for progress, the active tab and focus. `assets/ayu-dark.json` carries the same values for
//! the library widgets (inputs, switches), so they match.

use gpui_kit::{Hsla, rgb};

/// The window background.
pub const CANVAS: u32 = 0x0c0b0a;
/// Panels and cards resting on the canvas.
pub const SURFACE: u32 = 0x151413;
/// Anything raised above a surface: hover fills, menus.
pub const RAISED: u32 = 0x1e1c1a;
pub const LINE: u32 = 0x2a2825;
pub const TEXT: u32 = 0xf1ede6;
pub const TEXT_DIM: u32 = 0xa39d92;
pub const TEXT_FAINT: u32 = 0x756f66;
pub const ACCENT: u32 = 0xe6b450;
pub const DANGER: u32 = 0xe8786e;

/// The three fills for anything lifted off the canvas: the text colour at these opacities.
pub const WASH: f32 = 0.06;
pub const WASH_2: f32 = 0.1;
pub const WASH_3: f32 = 0.16;

pub fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Horizontal padding of every screen's content.
pub const GUTTER: f32 = 40.;
/// The unified title bar: window controls, navigation and search in one row.
pub const TOP_BAR: f32 = 56.;

/// Type scale, in pixels.
pub mod size {
    pub const BILLBOARD: f32 = 56.;
    pub const DISPLAY: f32 = 36.;
    pub const ROW_TITLE: f32 = 20.;
    pub const BODY: f32 = 14.;
}

/// Poster tile, 2:3.
pub const TILE_WIDTH: f32 = 176.;
pub const TILE_HEIGHT: f32 = 264.;

/// Font families. The interface face is the theme's own (`font.family` in `assets/theme.json`);
/// titles use the mincho, in regular or semibold.
pub mod font {
    pub const TITLE: &str = "Shippori Mincho";
}

/// One corner radius scale, in pixels: badges, buttons, tiles and panels, popups.
pub mod radius {
    pub const BADGE: f32 = 4.;
    pub const BUTTON: f32 = 8.;
    pub const PANEL: f32 = 12.;
    pub const POPUP: f32 = 16.;
}
