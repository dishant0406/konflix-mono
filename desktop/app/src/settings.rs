//! Settings: how much disk the torrents may use, and how many are kept. Two sliders that
//! snap to a few sensible stops, saved as soon as they are let go.

use crate::backend::Backend;
use crate::design::button::ghost_button;
use crate::design::toggle::toggle;
use crate::design::tokens::{
    ACCENT, DANGER, GUTTER, TEXT, TEXT_DIM, TEXT_FAINT, TOP_BAR, color, size,
};
use crate::updates::{Shared, Status};
use akidev_core::config::Config;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

const GIB: u64 = 1024 * 1024 * 1024;

/// The values a slider can take. Few and round, because nobody needs 37 GiB.
const DISK_STOPS: [u64; 9] = [2, 4, 8, 16, 32, 64, 128, 256, 512];
const TORRENT_STOPS: [u64; 7] = [1, 2, 3, 4, 6, 8, 12];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Limit {
    Disk,
    Torrents,
}

impl Limit {
    fn stops(self) -> &'static [u64] {
        match self {
            Limit::Disk => &DISK_STOPS,
            Limit::Torrents => &TORRENT_STOPS,
        }
    }

    fn value(self, config: &Config) -> u64 {
        match self {
            Limit::Disk => config.max_cache_bytes / GIB,
            Limit::Torrents => config.max_torrents as u64,
        }
    }

    fn set(self, config: &mut Config, value: u64) {
        match self {
            Limit::Disk => config.max_cache_bytes = value * GIB,
            Limit::Torrents => config.max_torrents = value as usize,
        }
    }
}

pub struct SettingsPage {
    config: Config,
    /// Where each slider was last drawn, so a click can be turned into a stop.
    disk_bar: Rc<Cell<Bounds<Pixels>>>,
    torrents_bar: Rc<Cell<Bounds<Pixels>>>,
    /// The slider being dragged.
    dragging: Option<Limit>,
}

impl SettingsPage {
    pub fn new(cx: &mut Context<Self>) -> SettingsPage {
        // The About row follows the update as it downloads.
        let updates = cx.global::<Shared>().0.clone();
        cx.observe(&updates, |_, _, cx| cx.notify()).detach();
        SettingsPage {
            config: cx.global::<Backend>().config.clone(),
            disk_bar: Rc::new(Cell::new(Bounds::default())),
            torrents_bar: Rc::new(Cell::new(Bounds::default())),
            dragging: None,
        }
    }

    fn bar(&self, limit: Limit) -> &Rc<Cell<Bounds<Pixels>>> {
        match limit {
            Limit::Disk => &self.disk_bar,
            Limit::Torrents => &self.torrents_bar,
        }
    }

    /// The index of the stop nearest to `x`, and the one the setting is at.
    fn stop_under(&self, limit: Limit, x: Pixels) -> usize {
        let bar = self.bar(limit).get();
        let stops = limit.stops().len();
        if bar.size.width <= px(0.) {
            return 0;
        }
        let fraction = ((x - bar.origin.x) / bar.size.width).clamp(0., 1.);
        (fraction * (stops - 1) as f32).round() as usize
    }

    fn pick(&mut self, limit: Limit, x: Pixels, cx: &mut Context<Self>) {
        let value = limit.stops()[self.stop_under(limit, x)];
        if limit.value(&self.config) != value {
            limit.set(&mut self.config, value);
            cx.notify();
        }
    }

    /// Keeps what was chosen, and tells the engine what it can use at once.
    fn commit(&mut self, cx: &mut Context<Self>) {
        self.dragging = None;
        let saved = &cx.global::<Backend>().config;
        if *saved != self.config {
            let path = cx.global::<Backend>().config_path.clone();
            match self.config.save(&path) {
                Ok(()) => {
                    if let Ok(streamer) = &cx.global::<Backend>().streamer {
                        streamer.set_config(self.config.clone());
                    }
                    cx.global_mut::<Backend>().config = self.config.clone();
                }
                Err(error) => {
                    crate::route::toast(format!("Could not save settings: {error:#}"), cx)
                }
            }
        }
        cx.notify();
    }

    fn toggle_updates(&mut self, cx: &mut Context<Self>) {
        self.config.check_for_updates = !self.config.check_for_updates;
        self.commit(cx);
    }

    fn restore(&mut self, cx: &mut Context<Self>) {
        self.config = Config {
            cache_dir: self.config.cache_dir.clone(),
            // Restoring the limits leaves the update choice as it was.
            check_for_updates: self.config.check_for_updates,
            ..Config::default()
        };
        self.commit(cx);
    }

    /// One setting: its name, its value large, a track to move it along and what it does.
    fn slider(
        &self,
        limit: Limit,
        name: &'static str,
        unit: &'static str,
        note: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let stops = limit.stops();
        let current = stops
            .iter()
            .enumerate()
            .min_by_key(|(_, stop)| stop.abs_diff(limit.value(&self.config)))
            .map_or(0, |(index, _)| index);
        let at = |index: usize| index as f32 / (stops.len() - 1) as f32;
        let shown = at(current);
        let bounds = self.bar(limit).clone();

        let track = div()
            .id(match limit {
                Limit::Disk => "disk",
                Limit::Torrents => "torrents",
            })
            .relative()
            .w_full()
            .h(px(28.))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |page, event: &MouseDownEvent, _, cx| {
                    page.dragging = Some(limit);
                    page.pick(limit, event.position.x, cx);
                }),
            )
            .on_mouse_move(cx.listener(move |page, event: &MouseMoveEvent, _, cx| {
                if page.dragging == Some(limit) && event.pressed_button == Some(MouseButton::Left) {
                    page.pick(limit, event.position.x, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|page, _, _, cx| page.commit(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|page, _, _, cx| page.commit(cx)),
            )
            .child(
                canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(13.))
                    .w_full()
                    .h(px(2.))
                    .rounded_full()
                    .bg(color(TEXT).opacity(0.14)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(13.))
                    .w(relative(shown))
                    .h(px(2.))
                    .rounded_full()
                    .bg(color(ACCENT)),
            )
            .children((0..stops.len()).map(|index| {
                div()
                    .absolute()
                    .top(px(10.))
                    .left(relative(at(index)))
                    .w(px(1.))
                    .h(px(8.))
                    .bg(color(TEXT).opacity(if index <= current { 0. } else { 0.22 }))
            }))
            .child(
                div()
                    .absolute()
                    .top(px(7.))
                    .left(relative(shown))
                    .ml(px(-7.))
                    .size(px(14.))
                    .rounded_full()
                    .bg(color(TEXT)),
            );

        let labels = div()
            .relative()
            .w_full()
            .h(px(16.))
            .children(stops.iter().enumerate().map(|(index, stop)| {
                div()
                    .absolute()
                    .left(relative(at(index)))
                    .ml(px(-24.))
                    .w(px(48.))
                    .text_center()
                    .text_size(px(11.))
                    .text_color(if index == current {
                        color(TEXT)
                    } else {
                        color(TEXT_FAINT)
                    })
                    .child(stop.to_string())
            }));

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(TEXT_DIM))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .pt_1()
                    .pb_4()
                    .child(
                        div()
                            .text_size(px(64.))
                            .line_height(px(68.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color(TEXT))
                            .child(stops[current].to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .text_color(color(TEXT_DIM))
                            .child(unit),
                    ),
            )
            .child(track)
            .child(labels)
            .child(
                div()
                    .pt_4()
                    .text_size(px(13.))
                    .text_color(color(TEXT_FAINT))
                    .child(note),
            )
    }
}

impl SettingsPage {
    /// The version, and quietly beside it whether there is a newer one.
    fn about(&self, cx: &mut Context<Self>) -> Div {
        let updates = cx.global::<Shared>().0.clone();
        let status = updates.read(cx).status(cx);
        let press = {
            let updates = updates.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                updates.update(cx, |updates, cx| updates.activate(cx))
            }
        };
        let quiet =
            |text: String, tone: u32| div().text_size(px(13.)).text_color(color(tone)).child(text);
        // The button's padding hangs out so its text lines up with the toggle above.
        let side = div().flex().items_center().gap_1().mr(px(-8.));
        let side = match status {
            Status::UpToDate => side.child(quiet("Up to date".into(), TEXT_FAINT)),
            Status::Available(version) => side
                .child(quiet(format!("{version} available"), TEXT_DIM))
                .child(ghost_button("about-update", "Update", press)),
            Status::Busy(text) => side.child(quiet(text, TEXT_DIM)),
            Status::Failed(sentence) => side.child(quiet(sentence, DANGER)).child(ghost_button(
                "about-retry",
                "Retry",
                press,
            )),
            Status::Page => side.child(ghost_button("about-page", "Open the download page", press)),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_8()
            .h(px(28.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(TEXT_DIM))
                    .child(concat!("Akidev ", env!("CARGO_PKG_VERSION"))),
            )
            .child(side)
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = &cx.global::<Backend>().started_with;
        let restart = self.config.max_cache_bytes != started.max_cache_bytes
            || self.config.max_torrents != started.max_torrents;
        let custom = self.config.max_cache_bytes != Config::default().max_cache_bytes
            || self.config.max_torrents != Config::default().max_torrents;

        div()
            .flex()
            .justify_center()
            .pt(px(TOP_BAR + 28.))
            .pb_16()
            .px(px(GUTTER))
            .child(
                div()
                    .w_full()
                    .max_w(px(640.))
                    .flex()
                    .flex_col()
                    .gap_12()
                    .child(
                        div()
                            .text_size(px(size::DISPLAY))
                            .font_weight(FontWeight::BLACK)
                            .text_color(color(TEXT))
                            .child("Settings"),
                    )
                    .child(self.slider(
                        Limit::Disk,
                        "Disk for torrents",
                        "GiB",
                        "The most the drive is asked to hold while you watch. It is all deleted when you quit.",
                        cx,
                    ))
                    .child(self.slider(
                        Limit::Torrents,
                        "Torrents kept ready",
                        "at once",
                        "Opening another one past this drops the one you watched longest ago.",
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_8()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(color(TEXT_DIM))
                                            .child("Check for updates"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(color(TEXT_FAINT))
                                            .child("Looks for a newer Akidev every few hours. Nothing is installed until you press Update."),
                                    ),
                            )
                            .child(toggle(
                                "check-updates",
                                self.config.check_for_updates,
                                cx.listener(|page, _, _, cx| page.toggle_updates(cx)),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(20.))
                            .child(div().when(custom, |side| {
                                side.child(
                                    div()
                                        .id("restore")
                                        .text_size(px(13.))
                                        .text_color(color(TEXT_DIM))
                                        .cursor_pointer()
                                        .hover(|style| style.text_color(color(TEXT)))
                                        .on_click(cx.listener(|page, _, _, cx| page.restore(cx)))
                                        .child("Restore defaults"),
                                )
                            }))
                            .child(div().when(restart, |side| {
                                side.flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(px(13.))
                                    .text_color(color(TEXT_DIM))
                                    .child(div().size(px(6.)).rounded_full().bg(color(ACCENT)))
                                    .child("Takes effect the next time Akidev starts")
                            })),
                    )
                    .child(self.about(cx)),
            )
    }
}
