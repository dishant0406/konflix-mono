//! An on/off switch: a small amber track with a round knob.

use super::tokens::{ACCENT, CANVAS, TEXT, WASH_3, color};
use gpui_kit::*;

pub fn toggle(
    id: impl Into<ElementId>,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex_none()
        .w(px(40.))
        .h(px(22.))
        .rounded_full()
        .cursor_pointer()
        .bg(if on {
            color(ACCENT)
        } else {
            color(TEXT).opacity(WASH_3)
        })
        .on_click(on_click)
        .child(
            div()
                .absolute()
                .top(px(3.))
                .left(px(if on { 21. } else { 3. }))
                .size(px(16.))
                .rounded_full()
                .bg(if on { color(CANVAS) } else { color(TEXT) }),
        )
}
