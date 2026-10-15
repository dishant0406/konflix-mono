//! The palette, type scale and spacing every screen draws from.
//!
//! A neutral near-black canvas with grey steps for surfaces: the artwork is the colour on
//! screen, and the interface stays out of its way. Amber is the one accent and is used only
//! for progress, the active tab and focus. `assets/ayu-dark.json` carries the same values for
//! the library widgets (inputs, switches), so they match.

use gpui_kit::{Hsla, rgb};

/// The window background.
pub const CANVAS: u32 = 0x0b0b0d;
/// Panels and cards resting on the canvas.
pub const SURFACE: u32 = 0x141416;
/// Anything raised above a surface: hover fills, menus.
pub const RAISED: u32 = 0x1c1c1f;
pub const LINE: u32 = 0x27272a;
pub const TEXT: u32 = 0xf4f4f5;
pub const TEXT_DIM: u32 = 0xa1a1aa;
pub const TEXT_FAINT: u32 = 0x71717a;
pub const ACCENT: u32 = 0xe6b450;
pub const DANGER: u32 = 0xf87171;

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
