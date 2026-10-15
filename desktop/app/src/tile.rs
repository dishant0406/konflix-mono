//! A poster tile: clean at rest, with the title, facts and actions laid over it on hover.

use crate::design::state::{kind_label, meta_line};
use crate::design::tokens::{
    ACCENT, CANVAS, RAISED, TEXT, TEXT_DIM, TILE_HEIGHT, TILE_WIDTH, color,
};
use crate::route::{Route, navigate};
use akidev_core::model::{Kind, Media};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// `resume` marks a title the viewer is part way through: how far (0 to 1), and the episode
/// to carry on with. It draws a progress bar and makes the tile resume rather than open.
pub fn poster_tile(key: &str, media: &Media, resume: Option<(f32, Route)>) -> Stateful<Div> {
    let group = SharedString::from(format!("tile-{key}"));
    let title_route = Route::Title {
        id: media.id.clone(),
    };
    // A film plays straight away. A show needs its episode list first, so it opens the title.
    let play_route = match &resume {
        Some((_, route)) => route.clone(),
        None if media.kind == Kind::Movie => Route::Watch {
            id: media.id.clone(),
        },
        None => title_route.clone(),
    };
    let click_route = if resume.is_some() {
        play_route.clone()
    } else {
        title_route.clone()
    };
    let watched = resume.as_ref().map(|(fraction, _)| *fraction);

    let name = media.title.clone();
    let no_poster = {
        let name = name.clone();
        move || tile_placeholder(&name).into_any_element()
    };
    let poster = match &media.poster {
        Some(url) => div().size_full().child(
            img(url.clone())
                .id(SharedString::from(format!("poster-{key}")))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(no_poster),
        ),
        None => tile_placeholder(&name),
    };

    let facts = [
        media.year.map(|y| y.to_string()),
        media.format.clone().map(|f| f.to_lowercase()),
        media.score.filter(|&s| s > 0).map(|s| format!("★ {s}%")),
    ];
    let open = title_route.clone();

    let overlay = div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .flex_col()
        .justify_end()
        .gap_1p5()
        .p_3()
        .bg(linear_gradient(
            0.,
            linear_color_stop(color(CANVAS).opacity(0.97), 0.),
            linear_color_stop(color(CANVAS).opacity(0.), 0.8),
        ))
        .invisible()
        .group_hover(group.clone(), |style| style.visible())
        .child(
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(color(ACCENT))
                .child(kind_label(media.kind)),
        )
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::BOLD)
                .text_color(color(TEXT))
                .line_clamp(2)
                .child(name.clone()),
        )
        .child(meta_line(facts.into_iter().flatten()).text_size(px(11.)))
        .child(
            div()
                .flex()
                .gap_2()
                .pt_1()
                .child(round_button(
                    format!("play-{key}"),
                    IconName::Play,
                    true,
                    play_route,
                ))
                .child(round_button(
                    format!("info-{key}"),
                    IconName::Info,
                    false,
                    open,
                )),
        );

    let poster_box = div()
        .relative()
        .w(px(TILE_WIDTH))
        .h(px(TILE_HEIGHT))
        .rounded_lg()
        .overflow_hidden()
        .bg(color(RAISED))
        .border_1()
        .border_color(color(TEXT).opacity(0.0))
        .group_hover(group.clone(), |style| {
            style.border_color(color(TEXT).opacity(0.4))
        })
        .child(poster)
        .child(overlay)
        .children(watched.map(|fraction| {
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .w_full()
                .h(px(3.))
                .bg(color(TEXT).opacity(0.25))
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction.clamp(0., 1.)))
                        .bg(color(ACCENT)),
                )
        }));

    div()
        .id(group.clone())
        .group(group)
        .flex_none()
        .w(px(TILE_WIDTH))
        .flex()
        .flex_col()
        .gap_2()
        .cursor_pointer()
        .on_hover({
            let id = media.id.clone();
            move |&on, _, cx| crate::prefetch::hovered(&id, on, cx)
        })
        .on_click(move |_, _, cx| navigate(click_route.clone(), cx))
        .child(poster_box)
        .child(
            div()
                .text_size(px(13.))
                .text_color(color(TEXT_DIM))
                .truncate()
                .child(name),
        )
}

/// What shows when a title has no poster: its name, so the tile is still recognisable.
fn tile_placeholder(name: &str) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .text_center()
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(TEXT_DIM))
        .child(name.to_string())
}

fn round_button(id: String, icon: IconName, filled: bool, route: Route) -> Stateful<Div> {
    let (background, foreground) = if filled {
        (color(TEXT), color(CANVAS))
    } else {
        (color(TEXT).opacity(0.16), color(TEXT))
    };
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .justify_center()
        .size(px(32.))
        .rounded_full()
        .bg(background)
        .cursor_pointer()
        .when(filled, |button| {
            button.hover(|style| style.bg(color(TEXT).opacity(0.85)))
        })
        .when(!filled, |button| {
            button.hover(|style| style.bg(color(TEXT).opacity(0.28)))
        })
        .on_click(move |_, _, cx| {
            // The tile behind this button opens the title too, and that must not also run.
            cx.stop_propagation();
            navigate(route.clone(), cx);
        })
        .child(Icon::new(icon).small().text_color(foreground))
}
