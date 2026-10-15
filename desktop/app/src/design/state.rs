//! Small shared pieces: headings, metadata lines, and the empty and error states.

use super::button::secondary_button;
use super::tokens::{ACCENT, DANGER, TEXT, TEXT_DIM, TEXT_FAINT, color, size};
use gpui_kit::assets::IconName;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::*;

pub fn section_title(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(size::ROW_TITLE))
        .font_weight(FontWeight::BOLD)
        .text_color(color(TEXT))
        .child(text.into())
}

/// "2026 · TV · 24 episodes": quiet facts under a title.
pub fn meta_line(parts: impl IntoIterator<Item = String>) -> Div {
    let mut line = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(size::BODY))
        .text_color(color(TEXT_DIM));
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            line = line.child(div().text_color(color(TEXT_FAINT)).child("·"));
        }
        line = line.child(part);
    }
    line
}

/// "★ 87%", in the accent colour.
pub fn score_label(score: u32) -> Div {
    div()
        .text_size(px(size::BODY))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(ACCENT))
        .child(format!("★ {score}%"))
}

/// A grey block that pulses while something loads.
pub fn skeleton(width: Pixels, height: Pixels) -> Skeleton {
    Skeleton::new().w(width).h(height).rounded_md()
}

fn centred(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    title_color: Hsla,
) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_8()
        .py_16()
        .text_center()
        .child(
            div()
                .text_size(px(18.))
                .font_weight(FontWeight::BOLD)
                .text_color(title_color)
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(520.))
                .text_size(px(size::BODY))
                .text_color(color(TEXT_DIM))
                .child(detail.into()),
        )
}

/// Nothing to show, and nothing went wrong.
pub fn empty_state(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    centred(title, detail, color(TEXT))
}

/// Something went wrong, with a way to try again.
pub fn error_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    centred(title, detail, color(DANGER)).child(div().pt_3().child(secondary_button(
        "retry",
        "Try again",
        Some(IconName::RefreshCw),
        on_retry,
    )))
}

pub fn kind_label(kind: akidev_core::model::Kind) -> &'static str {
    use akidev_core::model::Kind;
    match kind {
        Kind::Anime => "ANIME",
        Kind::Series => "SERIES",
        Kind::Movie => "FILM",
    }
}
