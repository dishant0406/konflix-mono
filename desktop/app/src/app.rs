//! The window: one bar that is both title bar and navigation, and the current screen below it.

use crate::browse::BrowsePage;
use crate::design::button::{icon_button, keep_from_title_bar, tab};
use crate::design::tokens::{CANVAS, LINE, RAISED, TEXT, TOP_BAR, color};
use crate::history::History;
use crate::home::HomePage;
use crate::music_album::AlbumPage;
use crate::music_artist::ArtistPage;
use crate::music_home::MusicPage;
use crate::music_player::{BAR_HEIGHT, Changed, hub};
use crate::palette::Palette;
use crate::party::{Party, PartyHandle};
use crate::route::{Navigator, Route, navigate};
use crate::settings::SettingsPage;
use crate::title::TitlePage;
use crate::updates::{Shared, Updates};
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

/// What one step of the history shows. Kept with the step, so Back finds the page as it was left:
/// scrolled where it was, with its rows already loaded.
#[derive(Default)]
struct Screen {
    /// `None` until the step is first shown, and again once the viewer has left the watch page,
    /// which must stop playing and so is built afresh on return.
    view: Option<AnyView>,
    scroll: ScrollHandle,
}

pub struct AkidevApp {
    history: History<Screen>,
    palette: Entity<Palette>,
    party: Entity<Party>,
    updates: Entity<Updates>,
    toasts: Vec<Toast>,
    next_toast: u64,
    focus: FocusHandle,
    focused: bool,
    palette_was_open: bool,
    /// The Akidev mark drawn beside the name.
    mark: Arc<Image>,
    pictures: Entity<crate::images::PictureCache>,
    _palette_changes: Subscription,
    _party_changes: Subscription,
    _update_changes: Subscription,
    /// Redraws the page when music starts or stops, so it leaves room for the bar.
    _music_changes: Subscription,
}

impl AkidevApp {
    pub fn new(first: Route, window: &mut Window, cx: &mut Context<Self>) -> AkidevApp {
        crate::analytics::screen_of(&first);
        cx.set_global(Navigator(cx.weak_entity()));
        let palette = cx.new(|cx| Palette::new(window, cx));
        let palette_changes = cx.observe(&palette, |_, _, cx| cx.notify());
        let party = cx.new(|cx| Party::new(window, cx));
        cx.set_global(PartyHandle(party.clone()));
        let party_changes = cx.observe(&party, |_, _, cx| cx.notify());
        let updates = cx.new(Updates::new);
        cx.set_global(Shared(updates.clone()));
        let update_changes = cx.observe(&updates, |_, _, cx| cx.notify());
        let music_changes =
            cx.subscribe(&hub(cx).player.clone(), |_, _, _: &Changed, cx| cx.notify());
        // Built last: a screen may read the update state (Settings does) or navigate.
        let screen = Screen {
            view: Some(Self::build(&first, cx)),
            scroll: ScrollHandle::new(),
        };
        AkidevApp {
            history: History::new(first, screen),
            palette,
            party,
            updates,
            toasts: Vec::new(),
            next_toast: 0,
            focus: cx.focus_handle(),
            focused: false,
            palette_was_open: false,
            pictures: crate::images::PictureCache::new(cx),
            mark: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/mark.png").to_vec(),
            )),
            _palette_changes: palette_changes,
            _party_changes: party_changes,
            _update_changes: update_changes,
            _music_changes: music_changes,
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
            Route::Music { term } => {
                let term = term.clone();
                cx.new(|cx| MusicPage::new(term, cx)).into()
            }
            Route::MusicAlbum { id } => {
                let id = id.clone();
                cx.new(|cx| AlbumPage::new(id, cx)).into()
            }
            Route::MusicArtist { id } => {
                let id = id.clone();
                cx.new(|cx| ArtistPage::new(id, cx)).into()
            }
            Route::Settings => cx.new(SettingsPage::new).into(),
        }
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let music = matches!(
            self.route(),
            Route::Music { .. } | Route::MusicAlbum { .. } | Route::MusicArtist { .. }
        );
        self.palette
            .update(cx, |palette, cx| palette.show(window, music, cx));
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
        self.history.route()
    }

    pub fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route() == &route {
            return;
        }
        self.leave();
        self.history.go(route, Screen::default());
        self.arrive(cx);
    }

    /// One screen back. With nowhere to go back to (the app was opened straight onto a
    /// video), it goes Home, so the back button always leads somewhere.
    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.history.back() {
            self.arrive(cx);
        }
    }

    pub fn replace(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route() == &route {
            return;
        }
        self.history.replace(route, Screen::default());
        self.arrive(cx);
    }

    /// Called before the viewer moves on. The watch page is let go, so its video stops; every
    /// other page stays as it is, for Back.
    fn leave(&mut self) {
        if matches!(self.route(), Route::Watch { .. }) {
            self.history.current_mut().view = None;
        }
    }

    /// Shows the current step, building its page if it has none.
    fn arrive(&mut self, cx: &mut Context<Self>) {
        let route = self.route().clone();
        crate::analytics::screen_of(&route);
        if self.history.current().view.is_none() {
            self.history.current_mut().view = Some(Self::build(&route, cx));
        }
        cx.notify();
    }

    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.route().clone();
        let scrolled = self.history.current().scroll.offset().y < px(-SOLID_AFTER);
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
        let can_go_back = self.history.can_go_back();

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
                    // Room for the traffic lights on a Mac; elsewhere the bar's own window
                    // buttons are on the right.
                    .pl(px(if cfg!(target_os = "macos") { 92. } else { 20. }))
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
                                    .on_mouse_down(MouseButton::Left, keep_from_title_bar)
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
                            .child(div().flex().items_center().child(tab(
                                "tab-Music",
                                "Music",
                                matches!(
                                    route,
                                    Route::Music { .. }
                                        | Route::MusicAlbum { .. }
                                        | Route::MusicArtist { .. }
                                ),
                                |_, _, cx| {
                                    navigate(
                                        Route::Music {
                                            term: String::new(),
                                        },
                                        cx,
                                    )
                                },
                            )))
                            .child(div().flex_1())
                            .children(self.updates.update(cx, |updates, cx| updates.button(cx)))
                            .child(crate::party::button(cx))
                            .child(icon_button(
                                "search",
                                IconName::Search,
                                cx.listener(|app, _, window, cx| {
                                    // On the music screens the magnifier opens the same panel as Cmd-K.
                                    if matches!(
                                        app.route(),
                                        Route::Music { .. }
                                            | Route::MusicAlbum { .. }
                                            | Route::MusicArtist { .. }
                                    ) {
                                        app.open_search(window, cx);
                                    } else {
                                        // Not `navigate`: this handler is already inside the window, which
                                        // cannot be asked to update itself again.
                                        app.go(
                                            Route::Browse {
                                                kind: None,
                                                term: String::new(),
                                            },
                                            cx,
                                        );
                                    }
                                }),
                            ))
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
        let has_music = hub(cx).player.read(cx).current().is_some();

        let pictures = self.pictures.clone();
        let shell = div()
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
                    .track_scroll(&self.history.current().scroll)
                    .children(self.history.current().view.clone())
                    // Room at the foot of every page, so the music bar never covers its last line.
                    .when(has_music && !immersive, |page| {
                        page.child(div().h(px(BAR_HEIGHT)).w_full())
                    }),
            )
            .when(!immersive, |shell| shell.child(self.top_bar(cx)))
            .when(!immersive, |shell| {
                // The player draws its bar in normal flow; this puts it along the bottom.
                shell.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .w_full()
                        .child(hub(cx).player.clone()),
                )
            })
            .when(palette_open, |shell| shell.child(self.palette.clone()))
            .when(self.party.read(cx).is_open(), |shell| {
                shell.child(self.party.clone())
            })
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
            );
        image_cache(pictures).size_full().child(shell)
    }
}
