//! The search palette (Cmd-K): results appear as you type, and Enter opens the best one.

use crate::design::state::{kind_label, meta_line};
use crate::design::tokens::{CANVAS, LINE, RAISED, SURFACE, TEXT, TEXT_DIM, TEXT_FAINT, color};
use crate::route::{Route, navigate};
use akidev_core::model::{Kind, Media};
use akidev_core::search::search;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// How long typing must pause before a search starts.
const PAUSE: Duration = Duration::from_millis(250);
const MAX_RESULTS: usize = 8;

pub struct Palette {
    input: Entity<InputState>,
    open: bool,
    results: Vec<Media>,
    searching: bool,
    failed: bool,
    selected: usize,
    /// The newest request. A search that finds it has changed after its pause drops out.
    latest: Arc<AtomicU32>,
    _events: Subscription,
}

impl Palette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Palette {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search anime, series and films"));
        let events = cx.subscribe(&input, |palette, _, event: &InputEvent, cx| match event {
            InputEvent::Change => palette.typed(cx),
            InputEvent::PressEnter { .. } => palette.confirm(cx),
            _ => {}
        });
        Palette {
            input,
            open: false,
            results: Vec::new(),
            searching: false,
            failed: false,
            selected: 0,
            latest: Arc::new(AtomicU32::new(0)),
            _events: events,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.results.clear();
        self.searching = false;
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.latest.fetch_add(1, Ordering::SeqCst);
        cx.notify();
    }

    fn term(&self, cx: &App) -> String {
        self.input.read(cx).value().trim().to_string()
    }

    fn typed(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        let request = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
        self.selected = 0;
        if term.is_empty() {
            self.results.clear();
            self.searching = false;
            cx.notify();
            return;
        }
        // Old results are not offered while a new search is pending: Enter would open one.
        self.results.clear();
        self.searching = true;
        self.failed = false;
        cx.notify();

        let latest = self.latest.clone();
        crate::task::load(
            cx,
            async move {
                tokio::time::sleep(PAUSE).await;
                if latest.load(Ordering::SeqCst) != request {
                    return None;
                }
                Some(search(&term, &Kind::ALL, MAX_RESULTS).await)
            },
            move |palette, answer, cx| {
                // Typing on has superseded this search, or the palette was closed.
                let Some(answer) =
                    answer.filter(|_| palette.latest.load(Ordering::SeqCst) == request)
                else {
                    return;
                };
                palette.failed = answer.is_err();
                palette.results = answer.unwrap_or_default();
                palette.searching = false;
                cx.notify();
            },
        );
    }

    /// Enter: the highlighted result, or the full results page when nothing is highlighted.
    fn confirm(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        if let Some(media) = self.results.get(self.selected) {
            navigate(
                Route::Title {
                    id: media.id.clone(),
                },
                cx,
            );
        } else if !term.is_empty() {
            navigate(Route::Browse { kind: None, term }, cx);
        } else {
            return;
        }
        self.close(cx);
    }

    fn row(&self, index: usize, media: &Media, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = media.id.clone();
        let facts = [
            media.year.map(|y| y.to_string()),
            media.score.filter(|&s| s > 0).map(|s| format!("★ {s}%")),
        ];
        div()
            .id(SharedString::from(format!("palette-{index}")))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_lg()
            .cursor_pointer()
            .when(index == self.selected, |row| row.bg(color(RAISED)))
            .hover(|style| style.bg(color(RAISED)))
            .on_click(cx.listener(move |palette, _, _, cx| {
                navigate(Route::Title { id: id.clone() }, cx);
                palette.close(cx);
            }))
            .child(
                div()
                    .flex_none()
                    .w(px(36.))
                    .h(px(54.))
                    .rounded_md()
                    .overflow_hidden()
                    .bg(color(LINE))
                    .children(media.poster.clone().map(|url| {
                        img(url)
                            .id(SharedString::from(format!("palette-poster-{index}")))
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                    })),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color(TEXT))
                            .truncate()
                            .child(media.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color(TEXT_FAINT))
                                    .child(kind_label(media.kind)),
                            )
                            .child(meta_line(facts.into_iter().flatten()).text_size(px(12.))),
                    ),
            )
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let term_empty = self.input.read(cx).value().trim().is_empty();
        let results: Vec<Stateful<Div>> = self
            .results
            .iter()
            .enumerate()
            .map(|(i, media)| self.row(i, media, cx))
            .collect();
        let hint = if term_empty {
            Some("Type to search anime, series and films.")
        } else if self.searching && results.is_empty() {
            Some("Searching...")
        } else if self.failed {
            Some("Couldn't reach the catalogue. Check your connection.")
        } else if results.is_empty() {
            Some("No matches. Press Enter to search everywhere.")
        } else {
            None
        };

        div()
            .id("palette")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(110.))
            .bg(color(CANVAS).opacity(0.72))
            .on_click(cx.listener(|palette, _, _, cx| palette.close(cx)))
            .on_key_down(cx.listener(|palette, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => palette.close(cx),
                    "down" if !palette.results.is_empty() => {
                        palette.selected = (palette.selected + 1) % palette.results.len();
                        cx.notify();
                    }
                    "up" if !palette.results.is_empty() => {
                        palette.selected =
                            (palette.selected + palette.results.len() - 1) % palette.results.len();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .id("palette-panel")
                    .w(px(640.))
                    .flex()
                    .flex_col()
                    .rounded_2xl()
                    .overflow_hidden()
                    .bg(color(SURFACE))
                    .border_1()
                    .border_color(color(LINE))
                    .shadow_lg()
                    // A click inside the panel is not a click on the scrim.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .py_2()
                            .child(
                                Icon::new(IconName::Search)
                                    .small()
                                    .text_color(color(TEXT_DIM)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.input).appearance(false)),
                            ),
                    )
                    .when(hint.is_some() || !results.is_empty(), |panel| {
                        panel.child(div().h(px(1.)).bg(color(LINE))).child(
                            div()
                                .flex()
                                .flex_col()
                                .p_2()
                                .children(results)
                                .children(hint.map(|text| {
                                    div()
                                        .px_3()
                                        .py_4()
                                        .text_size(px(14.))
                                        .text_color(color(TEXT_DIM))
                                        .child(text)
                                })),
                        )
                    }),
            )
    }
}
