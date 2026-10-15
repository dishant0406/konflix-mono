//! The window: one bar that is both title bar and navigation, and the current screen below it.

use crate::browse::BrowsePage;
use crate::design::button::{icon_button, tab};
use crate::design::tokens::{CANVAS, LINE, RAISED, TEXT, TOP_BAR, color};
use crate::home::HomePage;
use crate::palette::Palette;
use crate::route::{Navigator, Route, navigate};
use crate::settings::SettingsPage;
use crate::title::TitlePage;
use crate::watch::WatchPage;
use akidev_core::model::Kind;
use gpui_kit::assets::IconName;
use gpui_kit::component::TitleBar;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use std::time::Duration;

actions!(akidev, [OpenSearch]);

/// How long a toast stays.
const TOAST_FOR: Duration = Duration::from_secs(3);

/// How far a page scrolls before the bar turns from a fade into a solid.
const SOLID_AFTER: f32 = 24.;

struct Toast {
    id: u64,
    text: String,
}

pub struct AkidevApp {
    page: AnyView,
    history: Vec<Route>,
    scroll: ScrollHandle,
    palette: Entity<Palette>,
    toasts: Vec<Toast>,
    next_toast: u64,
    focus: FocusHandle,
    focused: bool,
    palette_was_open: bool,
    /// The Akidev mark drawn beside the name.
    mark: Arc<Image>,
    _palette_changes: Subscription,
}

impl AkidevApp {
    pub fn new(first: Route, window: &mut Window, cx: &mut Context<Self>) -> AkidevApp {
        let page = Self::build(&first, cx);
        cx.set_global(Navigator(cx.weak_entity()));
        let palette = cx.new(|cx| Palette::new(window, cx));
        let palette_changes = cx.observe(&palette, |_, _, cx| cx.notify());
        AkidevApp {
            page,
            history: vec![first],
            scroll: ScrollHandle::new(),
            palette,
            toasts: Vec::new(),
            next_toast: 0,
            focus: cx.focus_handle(),
            focused: false,
            palette_was_open: false,
            mark: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/mark.png").to_vec(),
            )),
            _palette_changes: palette_changes,
        }
    }

    fn build(route: &Route, cx: &mut Context<Self>) -> AnyView {
        match route {
            Route::Home => cx.new(HomePage::new).into(),
            Route::Browse { kind, term } => {
                let (kind, term) = (*kind, term.clone());
                cx.new(|cx| BrowsePage::new(kind, term, cx)).into()
            }
            Route::Watch { id } => {
                let id = id.clone();
                cx.new(|cx| WatchPage::new(id, cx)).into()
            }
            Route::Title { id } => {
                let id = id.clone();
                cx.new(|cx| TitlePage::new(id, cx)).into()
            }
            Route::Settings => cx.new(SettingsPage::new).into(),
        }
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette
            .update(cx, |palette, cx| palette.show(window, cx));
    }

    pub fn toast(&mut self, text: String, cx: &mut Context<Self>) {
        self.next_toast += 1;
        let id = self.next_toast;
        self.toasts.push(Toast { id, text });
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(TOAST_FOR).await;
            let _ = app.update(cx, |app, cx| {
                app.toasts.retain(|toast| toast.id != id);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn route(&self) -> &Route {
        self.history.last().expect("history is never empty")
    }

    pub fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route() == &route {
            return;
        }
        self.show(&route, cx);
        self.history.push(route);
    }

    /// One screen back. With nowhere to go back to (the app was opened straight onto a
    /// video), it goes Home, so the back button always leads somewhere.
    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.history.len() > 1 {
            self.history.pop();
            let route = self.route().clone();
            self.show(&route, cx);
        } else if self.route() != &Route::Home {
            self.replace(Route::Home, cx);
        }
    }

    pub fn replace(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route() == &route {
            return;
        }
        self.show(&route, cx);
        *self.history.last_mut().expect("history is never empty") = route;
    }

    /// Swaps the screen and starts it at the top.
    fn show(&mut self, route: &Route, cx: &mut Context<Self>) {
        self.page = Self::build(route, cx);
        self.scroll = ScrollHandle::new();
        cx.notify();
    }

    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.route().clone();
        let scrolled = self.scroll.offset().y < px(-SOLID_AFTER);
        let browsing = |wanted: Option<Kind>| matches!(&route, Route::Browse { kind, term } if *kind == wanted && term.is_empty());
        let background: Background = if scrolled {
            color(CANVAS).into()
        } else {
            linear_gradient(
                180.,
                linear_color_stop(color(CANVAS).opacity(0.85), 0.),
                linear_color_stop(color(CANVAS).opacity(0.), 1.),
            )
        };

        let tabs = [
            ("Home", None),
            ("Anime", Some(Kind::Anime)),
            ("Series", Some(Kind::Series)),
            ("Films", Some(Kind::Movie)),
        ];
        let can_go_back = self.history.len() > 1 || self.route() != &Route::Home;

        // `occlude`: a click on the bar must not also reach the page scrolled under it.
        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .block_mouse_except_scroll()
            .child(
                TitleBar::new()
                    .h(px(TOP_BAR))
                    .pl(px(92.))
                    .pr(px(24.))
                    .bg(background)
                    .border_color(if scrolled {
                        color(LINE)
                    } else {
                        color(CANVAS).opacity(0.)
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w_full()
                            .gap_6()
                            .child(
                                icon_button(
                                    "back",
                                    IconName::ChevronLeft,
                                    cx.listener(|app, _, _, cx| app.back(cx)),
                                )
                                .when(!can_go_back, |button| button.opacity(0.35)),
                            )
                            .child(
                                div()
                                    .id("logo")
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::BLACK)
                                    .text_color(color(TEXT))
                                    .child(img(self.mark.clone()).size(px(28.)).flex_none())
                                    .child("AKIDEV")
                                    .on_click(|_, _, cx| navigate(Route::Home, cx)),
                            )
                            .child(div().flex().items_center().gap_5().children(tabs.map(
                                |(label, kind)| {
                                    let active = match kind {
                                        None => matches!(route, Route::Home),
                                        kind => browsing(kind),
                                    };
                                    tab(
                                        SharedString::from(format!("tab-{label}")),
                                        label,
                                        active,
                                        move |_, _, cx| match kind {
                                            None => navigate(Route::Home, cx),
                                            kind => navigate(
                                                Route::Browse {
                                                    kind,
                                                    term: String::new(),
                                                },
                                                cx,
                                            ),
                                        },
                                    )
                                },
                            )))
                            .child(div().flex_1())
                            .child(icon_button("search", IconName::Search, |_, _, cx| {
                                navigate(
                                    Route::Browse {
                                        kind: None,
                                        term: String::new(),
                                    },
                                    cx,
                                )
                            }))
                            .child(icon_button("settings", IconName::Settings, |_, _, cx| {
                                navigate(Route::Settings, cx)
                            })),
                    ),
            )
    }
}

impl Render for AkidevApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus starts on the window itself, so Cmd-K works before anything is clicked.
        if !self.focused {
            window.focus(&self.focus, cx);
            self.focused = true;
        }
        let palette_open = self.palette.read(cx).is_open();
        // A closed palette leaves focus on its hidden input; the window takes it back.
        if self.palette_was_open && !palette_open {
            window.focus(&self.focus, cx);
        }
        self.palette_was_open = palette_open;
        // The watch page draws its own bar over the picture.
        let immersive = matches!(self.route(), Route::Watch { .. });

        div()
            .size_full()
            .relative()
            .bg(color(CANVAS))
            .text_color(color(TEXT))
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|app, _: &OpenSearch, window, cx| app.open_search(window, cx)))
            .child(
                div()
                    .id("page")
                    .size_full()
                    .overflow_y_scroll()
                    // A swipe that is mostly sideways belongs to a row, and one that is mostly
                    // down belongs to the page: each takes its own axis, locked per gesture.
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.scroll)
                    .child(self.page.clone()),
            )
            .when(!immersive, |shell| shell.child(self.top_bar(cx)))
            .when(palette_open, |shell| shell.child(self.palette.clone()))
            .child(
                div()
                    .absolute()
                    .bottom(px(28.))
                    .left_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .children(self.toasts.iter().map(|toast| {
                        div()
                            .max_w(px(560.))
                            .px_4()
                            .py_2p5()
                            .rounded_2xl()
                            .bg(color(RAISED))
                            .border_1()
                            .border_color(color(LINE))
                            .shadow_lg()
                            .text_size(px(14.))
                            .text_color(color(TEXT))
                            .child(toast.text.clone())
                    })),
            )
    }
}
