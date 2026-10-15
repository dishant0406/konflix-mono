//! The featured banner at the top of the home screen.

use crate::backend::Backend;
use crate::design::button::{primary_button, secondary_button};
use crate::design::state::{kind_label, meta_line, skeleton};
use crate::design::tokens::{ACCENT, CANVAS, GUTTER, TEXT, TEXT_DIM, color, size};
use crate::route::{Route, navigate};
use akidev_core::model::{Kind, Media};
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;

/// How long a title is featured before the next one fades in.
const ROTATE_EVERY: Duration = Duration::from_secs(9);

pub struct Hero {
    /// `None` while loading.
    items: Option<Vec<Media>>,
    index: usize,
    _rotation: Option<Task<()>>,
}

impl Hero {
    pub fn new() -> Hero {
        Hero {
            items: None,
            index: 0,
            _rotation: None,
        }
    }

    pub fn set_items(&mut self, items: Vec<Media>, cx: &mut Context<Self>) {
        self.items = Some(items);
        self.index = 0;
        self.restart_rotation(cx);
        cx.notify();
    }

    /// Starts the countdown to the next title again, so a manual choice is not cut short.
    fn restart_rotation(&mut self, cx: &mut Context<Self>) {
        self._rotation = Some(cx.spawn(async move |hero, cx| {
            loop {
                cx.background_executor().timer(ROTATE_EVERY).await;
                if hero
                    .update(cx, |hero, cx| hero.show(hero.index + 1, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn show(&mut self, index: usize, cx: &mut Context<Self>) {
        let count = self.items.as_ref().map_or(0, Vec::len);
        if count > 0 {
            self.index = index % count;
            cx.notify();
        }
    }
}

impl Render for Hero {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = (window.viewport_size().height * 0.64).clamp(px(460.), px(680.));
        let frame = div()
            .relative()
            .w_full()
            .h(height)
            .overflow_hidden()
            .bg(color(CANVAS));

        let Some(items) = &self.items else {
            return frame
                .child(skeleton(px(2000.), height).rounded_none())
                .into_any_element();
        };
        let Some(media) = items.get(self.index).cloned() else {
            return div().h(px(96.)).into_any_element();
        };
        let count = items.len();

        let art = media.banner.clone().or_else(|| media.poster.clone());
        // A film plays. A show resumes where the viewer left it, or opens its title.
        let resume = cx
            .global::<Backend>()
            .progress
            .recent()
            .iter()
            .find(|entry| entry.media.id == media.id)
            .map(|entry| entry.episode_id.clone());
        let play_route = match resume {
            Some(id) => Route::Watch { id },
            None if media.kind == Kind::Movie => Route::Watch {
                id: media.id.clone(),
            },
            None => Route::Title {
                id: media.id.clone(),
            },
        };
        let title_route = Route::Title {
            id: media.id.clone(),
        };
        let facts = [
            media.year.map(|y| y.to_string()),
            media.format.clone().map(|f| f.to_lowercase()),
            media.score.filter(|&s| s > 0).map(|s| format!("★ {s}%")),
        ];

        let slide = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(art.map(|url| {
                div().absolute().size_full().child(
                    img(url)
                        .id(SharedString::from(format!("hero-art-{}", media.id)))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
            }))
            // One scrim across, so the words read over a busy picture, and one up from the
            // bottom, so the banner melts into the page.
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(color(CANVAS), 0.),
                linear_color_stop(color(CANVAS).opacity(0.), 0.72),
            )))
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(color(CANVAS).opacity(0.), 0.55),
                linear_color_stop(color(CANVAS), 1.),
            )))
            .child(
                div()
                    .absolute()
                    .left(px(GUTTER))
                    .bottom(px(72.))
                    .max_w(px(580.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color(ACCENT))
                                    .child(kind_label(media.kind)),
                            )
                            .child(meta_line(facts.into_iter().flatten())),
                    )
                    .child(
                        div()
                            .text_size(px(size::BILLBOARD))
                            .line_height(relative(1.05))
                            .font_weight(FontWeight::BLACK)
                            .text_color(color(TEXT))
                            .line_clamp(2)
                            .child(media.title.clone()),
                    )
                    .children(media.description.clone().map(|text| {
                        div()
                            .text_size(px(16.))
                            .line_height(relative(1.45))
                            .text_color(color(TEXT_DIM))
                            .line_clamp(3)
                            .child(text)
                    }))
                    .when(media.description.is_none(), |column| {
                        column.child(
                            div().text_size(px(15.)).text_color(color(TEXT_DIM)).child(
                                media
                                    .genres
                                    .iter()
                                    .take(3)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(" · "),
                            ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .pt_2()
                            .child(primary_button(
                                "hero-play",
                                "Play",
                                Some(IconName::Play),
                                move |_, _, cx| navigate(play_route.clone(), cx),
                            ))
                            .child(secondary_button(
                                "hero-info",
                                "More info",
                                Some(IconName::Info),
                                move |_, _, cx| navigate(title_route.clone(), cx),
                            )),
                    ),
            )
            .with_animation(
                SharedString::from(format!("hero-fade-{}", self.index)),
                Animation::new(Duration::from_millis(700)).with_easing(ease_out_quint()),
                |slide, progress| slide.opacity(progress),
            );

        let dots = div()
            .absolute()
            .bottom(px(32.))
            .right(px(GUTTER))
            .flex()
            .items_center()
            .gap_2()
            .children((0..count).map(|dot| {
                let active = dot == self.index;
                div()
                    .id(SharedString::from(format!("hero-dot-{dot}")))
                    .h(px(6.))
                    .w(px(if active { 24. } else { 6. }))
                    .rounded_full()
                    .cursor_pointer()
                    .bg(if active {
                        color(TEXT)
                    } else {
                        color(TEXT).opacity(0.3)
                    })
                    .hover(|style| style.bg(color(TEXT).opacity(0.7)))
                    .on_click(cx.listener(move |hero, _, _, cx| {
                        hero.show(dot, cx);
                        hero.restart_rotation(cx);
                    }))
            }));

        frame.child(slide).child(dots).into_any_element()
    }
}
