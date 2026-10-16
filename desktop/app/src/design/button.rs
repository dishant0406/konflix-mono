//! Buttons and chips. Plain clickable elements: one primary look, one secondary, and the
//! small round icon button the player and the bars use.

use super::tokens::{ACCENT, CANVAS, TEXT, TEXT_DIM, color};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

fn label_with_icon(label: Option<&str>, icon: Option<IconName>, text: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .children(icon.map(|icon| Icon::new(icon).small().text_color(text)))
        .children(label.map(|label| div().child(label.to_string())))
}

/// The one action on a screen: white, like a cinema's marquee.
pub fn primary_button(
    id: impl Into<ElementId>,
    label: &str,
    icon: Option<IconName>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px_5()
        .py_2p5()
        .rounded_md()
        .bg(color(TEXT))
        .text_color(color(CANVAS))
        .text_size(px(15.))
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(0.82)))
        .active(|style| style.bg(color(TEXT).opacity(0.7)))
        .on_click(on_click)
        .child(label_with_icon(Some(label), icon, color(CANVAS)))
}

/// Everything else: translucent, so it sits on artwork as well as on the canvas.
pub fn secondary_button(
    id: impl Into<ElementId>,
    label: &str,
    icon: Option<IconName>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px_5()
        .py_2p5()
        .rounded_md()
        .bg(color(TEXT).opacity(0.12))
        .text_color(color(TEXT))
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(0.2)))
        .active(|style| style.bg(color(TEXT).opacity(0.28)))
        .on_click(on_click)
        .child(label_with_icon(Some(label), icon, color(TEXT)))
}

/// A round icon-only button.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(36.))
        .rounded_full()
        .text_color(color(TEXT))
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(0.14)))
        .active(|style| style.bg(color(TEXT).opacity(0.22)))
        .on_click(on_click)
        .child(Icon::new(icon).text_color(color(TEXT)))
}

/// A quiet text button: no fill until the pointer is over it.
pub fn ghost_button(
    id: impl Into<ElementId>,
    label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(TEXT))
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(0.1)))
        .active(|style| style.bg(color(TEXT).opacity(0.16)))
        .on_click(on_click)
        .child(label.to_string())
}

/// A navigation tab: text only, the active one bright with an accent underline.
pub fn tab(
    id: impl Into<ElementId>,
    label: &str,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .px_1()
        .py_1()
        .cursor_pointer()
        .text_size(px(14.))
        .on_click(on_click)
        .when(active, |tab| {
            tab.text_color(color(TEXT)).font_weight(FontWeight::BOLD)
        })
        .when(!active, |tab| {
            tab.text_color(color(TEXT_DIM))
                .hover(|style| style.text_color(color(TEXT)))
        })
        .child(label.to_string())
        .child(
            div()
                .h(px(2.))
                .w_full()
                .rounded_full()
                .when(active, |line| line.bg(color(ACCENT))),
        )
}

/// An icon with a short label beside it, for the player's track and panel buttons.
pub fn control_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: Option<String>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap_1p5()
        .h(px(36.))
        .px_2p5()
        .rounded_full()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(TEXT))
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(0.14)))
        .active(|style| style.bg(color(TEXT).opacity(0.22)))
        .on_click(on_click)
        .child(Icon::new(icon).text_color(color(TEXT)))
        .children(label)
}
