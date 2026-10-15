//! The window theme for the library widgets (inputs, switches). It carries the same values as `design/tokens.rs`.

use gpui_kit::App;
use gpui_kit::component::theme::{Theme, ThemeMode, ThemeSet};
use std::rc::Rc;

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
