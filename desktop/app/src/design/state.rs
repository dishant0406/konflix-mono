//! Small shared pieces: headings, metadata lines, and the empty and error states.

use super::button::secondary_button;
use super::tokens::{ACCENT, DANGER, TEXT, TEXT_DIM, TEXT_FAINT, color, font, radius, size};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::*;

pub fn section_title(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(font::TITLE)
        .text_size(px(size::ROW_TITLE))
        .font_weight(FontWeight::SEMIBOLD)
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
    Skeleton::new()
        .w(width)
        .h(height)
        .rounded(px(radius::BUTTON))
}

/// A title card: a short rule, a small caps line saying what kind of moment this is, one line in
/// the title face, a few quiet words, and the ways forward as `actions`. Every empty and error
/// screen is this, so they read as part of the film and not as a system notice.
pub fn state_card(
    eyebrow: Option<&str>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    trouble: bool,
    actions: Vec<AnyElement>,
) -> Div {
    let tone = if trouble { DANGER } else { ACCENT };
    let mut card = div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px_8()
        .py(px(80.))
        .text_center()
        .child(div().h(px(1.)).w(px(32.)).mb_5().bg(color(tone)));
    if let Some(eyebrow) = eyebrow {
        card = card.child(
            div()
                .mb_3()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(if trouble { DANGER } else { TEXT_FAINT }))
                .child(spaced(eyebrow)),
        );
    }
    card = card
        .child(
            div()
                .max_w(px(560.))
                .font_family(font::TITLE)
                .text_size(px(28.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(TEXT))
                .child(title.into()),
        )
        .child(
            div()
                .mt_3()
                .max_w(px(460.))
                .text_size(px(size::BODY))
                .text_color(color(TEXT_DIM))
                .child(detail.into()),
        );
    if !actions.is_empty() {
        card = card.child(
            div()
                .mt(px(28.))
                .flex()
                .flex_wrap()
                .items_center()
                .justify_center()
                .gap_3()
                .children(actions),
        );
    }
    card
}

/// Capitals with a hair of space between letters: GPUI has no letter spacing, and small capitals
/// read as a label only when they are loose.
pub fn spaced(text: &str) -> String {
    text.to_uppercase()
        .chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join("\u{200A}")
}

/// Nothing to show, and nothing went wrong.
pub fn empty_state(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    state_card(None, title, detail, false, Vec::new())
}

/// Something went wrong, with a way to try again.
pub fn error_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    state_card(
        Some("Something went wrong"),
        title,
        detail,
        true,
        vec![secondary_button("retry", "Try again", None, on_retry).into_any_element()],
    )
}

/// "tv" becomes "TV", "special" becomes "Special", and "movie" is left out: the kind already says
/// film.
pub fn format_label(format: &str) -> Option<String> {
    let word = format.trim().to_lowercase();
    match word.len() {
        0 => None,
        _ if word == "movie" => None,
        1..=3 => Some(word.to_uppercase()),
        _ => {
            let mut letters = word.chars();
            let first = letters.next()?.to_uppercase().collect::<String>();
            Some(first + letters.as_str())
        }
    }
}

/// The first letter in capitals and the rest as it is: "NOT YET RELEASED" arrives lower-cased.
pub fn sentence(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut letters = lower.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + letters.as_str(),
        None => String::new(),
    }
}

pub fn kind_label(kind: akidev_core::model::Kind) -> String {
    use akidev_core::model::Kind;
    spaced(match kind {
        Kind::Anime => "Anime",
        Kind::Series => "Series",
        Kind::Movie => "Film",
    })
}
