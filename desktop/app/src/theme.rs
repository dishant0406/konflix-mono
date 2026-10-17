//! The window theme for the library widgets (inputs, switches). It carries the same values as `design/tokens.rs`.

use gpui_kit::App;
use gpui_kit::component::theme::{Theme, ThemeMode, ThemeSet};
use std::rc::Rc;

/// The interface face and the title face, kept small: Hanken Grotesk in the three weights the
/// screens use and Shippori Mincho for titles (Latin letters, punctuation and 秋 only). Both are
/// SIL Open Font License. Every font the UI draws is named in `design::tokens::font`.
pub fn fonts(cx: &mut App) {
    use std::borrow::Cow::Borrowed;
    let faces = [
        &include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf")[..],
        &include_bytes!("../assets/fonts/HankenGrotesk-Medium.ttf")[..],
        &include_bytes!("../assets/fonts/HankenGrotesk-SemiBold.ttf")[..],
        &include_bytes!("../assets/fonts/ShipporiMincho-Regular.ttf")[..],
        &include_bytes!("../assets/fonts/ShipporiMincho-SemiBold.ttf")[..],
    ];
    if let Err(error) = cx
        .text_system()
        .add_fonts(faces.into_iter().map(Borrowed).collect())
    {
        tracing::warn!("[theme] could not load the fonts: {error}");
    }
}

const THEME: &str = include_str!("../assets/theme.json");

pub fn apply(cx: &mut App) {
    let set: ThemeSet = serde_json::from_str(THEME).expect("theme.json is a valid theme");
    let config = Rc::new(
        set.themes
            .into_iter()
            .next()
            .expect("theme.json holds a theme"),
    );
    Theme::update(cx, |theme| theme.dark_theme = config);
    Theme::change(ThemeMode::Dark, None, cx);
}
