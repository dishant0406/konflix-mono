//! The Music tab: a search box over either the home shelves or the results.

use crate::design::state::{empty_state, error_state, skeleton};
use crate::design::tokens::{GUTTER, TEXT, TEXT_DIM, TOP_BAR, color};
use crate::music_player::{Changed, hub, play};
use crate::music_ui::{Shelf, ShelfItem, cover, loading_rows, names, track_list};
use crate::route::{Route, navigate};
use crate::task::load;
use akidev_core::music::SearchOut;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The genres on offer: its name, the file of its motif, the colour the motif is drawn in, and the
/// card's own colour (the accent, mostly black). The same list and files as the site's.
const GENRES: [(&str, &str, u32, u32); 12] = [
    ("Pop", "pop", 0xe0809a, 0x2b1a20),
    ("Hip-Hop", "hip-hop", 0xd9ab4c, 0x282114),
    ("Rock", "rock", 0xe0644f, 0x2b1815),
    ("Bollywood", "bollywood", 0xe8924a, 0x2b1e14),
    ("Electronic", "electronic", 0x5f9fe0, 0x141f2c),
    ("R&B", "rnb", 0xa083e0, 0x1d182b),
    ("Indie", "indie", 0x62b78f, 0x14251e),
    ("Jazz", "jazz", 0xcf935c, 0x261c14),
    ("K-Pop", "kpop", 0xe07fbb, 0x2b1724),
    ("Lo-fi", "lofi", 0x7fb0b0, 0x172424),
    ("Classical", "classical", 0xc9bb70, 0x25231a),
    ("Metal", "metal", 0xa8a8a8, 0x1d1d1d),
];

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Songs,
    Albums,
    Artists,
}

enum Answer {
    Loading,
    Failed,
    Loaded(SearchOut),
}

pub struct MusicPage {
    /// What the results on show are for. Empty shows the home shelves.
    term: String,
    results: Option<Answer>,
    home: Answer,
    filter: Filter,
    /// Bumped on every search, so a slow answer to an old one is ignored.
    request: u64,
    home_shelves: Vec<Entity<Shelf>>,
    result_shelves: Vec<Entity<Shelf>>,
    _subscriptions: Vec<Subscription>,
}

impl MusicPage {
    pub fn new(term: String, cx: &mut Context<Self>) -> MusicPage {
        let player = hub(cx).player.clone();
        let changes = cx.subscribe(&player, |_, _, _: &Changed, cx| cx.notify());
        let mut page = MusicPage {
            term: String::new(),
            results: None,
            home: Answer::Loading,
            filter: Filter::All,
            request: 0,
            home_shelves: Vec::new(),
            result_shelves: Vec::new(),
            _subscriptions: vec![changes],
        };
        page.load_home(cx);
        // Opening the tab is the sign music is coming: have yt-dlp fetched and warmed before a track needs it.
        let dir = hub(cx).ytdlp_dir.clone();
        crate::runtime::spawn(async move {
            if let Err(error) = akidev_core::music::install_ytdlp(&dir).await {
                tracing::warn!("[music] could not set up yt-dlp: {error:#}");
            }
        });
        if !term.is_empty() {
            page.search(term, cx);
        }
        page
    }

    fn load_home(&mut self, cx: &mut Context<Self>) {
        self.home = Answer::Loading;
        let music = hub(cx).music.clone();
        load(
            cx,
            async move { music.home().await.ok() },
            |page, found, cx| {
                page.home = match found {
                    Some(found) => {
                        page.home_shelves = shelves(&found, "home", cx);
                        Answer::Loaded(found)
                    }
                    None => Answer::Failed,
                };
                cx.notify();
            },
        );
    }

    fn search(&mut self, term: String, cx: &mut Context<Self>) {
        self.request += 1;
        let request = self.request;
        self.term = term.clone();
        if term.is_empty() {
            self.results = None;
            cx.notify();
            return;
        }
        self.results = Some(Answer::Loading);
        self.filter = Filter::All;
        let music = hub(cx).music.clone();
        load(
            cx,
            async move { music.search(&term).await.ok() },
            move |page, found, cx| {
                if page.request != request {
                    return;
                }
                page.results = Some(match found {
                    Some(found) => {
                        page.result_shelves = shelves(&found, "results", cx);
                        Answer::Loaded(found)
                    }
                    None => Answer::Failed,
                });
                cx.notify();
            },
        );
        cx.notify();
    }

    fn chips(&self, cx: &mut Context<Self>) -> Div {
        let chip = |label: &'static str, filter: Filter, cx: &mut Context<Self>| {
            let active = self.filter == filter;
            div()
                .id(SharedString::from(format!("music-chip-{label}")))
                .px_4()
                .h(px(32.))
                .flex()
                .items_center()
                .rounded_full()
                .cursor_pointer()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .when(active, |chip| {
                    chip.bg(color(TEXT)).text_color(color(0x000000))
                })
                .when(!active, |chip| {
                    chip.bg(color(TEXT).opacity(0.1))
                        .text_color(color(TEXT))
                        .hover(|style| style.bg(color(TEXT).opacity(0.16)))
                })
                .on_click(cx.listener(move |page, _, _, cx| {
                    page.filter = filter;
                    cx.notify();
                }))
                .child(label)
        };
        div()
            .flex()
            .gap_2()
            .px(px(GUTTER))
            .child(chip("All", Filter::All, cx))
            .child(chip("Songs", Filter::Songs, cx))
            .child(chip("Albums", Filter::Albums, cx))
            .child(chip("Artists", Filter::Artists, cx))
    }

    fn results_view(&mut self, found: &SearchOut, cx: &mut Context<Self>) -> Div {
        let all = self.filter == Filter::All;
        let albums = self.result_shelves.get(1).cloned();
        let artists = self.result_shelves.first().cloned();
        let top = self.top_result(found, cx);
        div()
            .flex()
            .flex_col()
            .gap_8()
            .child(self.chips(cx))
            .when(all, |view| {
                view.child(div().flex().gap_8().px(px(GUTTER)).children(top).when(
                    !found.songs.is_empty(),
                    |row| {
                        row.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(heading("Songs"))
                                .child(div().mx(px(-16.)).child(track_list(
                                    &found.songs[..found.songs.len().min(4)],
                                    false,
                                    "top-songs",
                                    cx,
                                ))),
                        )
                    },
                ))
            })
            .when(self.filter == Filter::Songs, |view| {
                view.child(div().px(px(GUTTER - 16.)).child(track_list(
                    &found.songs,
                    false,
                    "songs",
                    cx,
                )))
            })
            .when(all || self.filter == Filter::Artists, |view| {
                view.children(artists.filter(|_| !found.artists.is_empty()))
            })
            .when(all || self.filter == Filter::Albums, |view| {
                view.children(albums.filter(|_| !found.albums.is_empty()))
            })
    }

    /// The result worth leading with: the artist when the search is their name, else the first song.
    fn top_result(&self, found: &SearchOut, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let term = self.term.to_lowercase();
        let artist = found.artists.first();
        let song = found.songs.first();
        let as_artist = artist.filter(|a| a.name.to_lowercase().contains(&term) || song.is_none());
        let (art, title, subtitle, route, tracks) = match (as_artist, song) {
            (Some(artist), _) => (
                cover(artist.cover.clone(), px(96.), true, "top"),
                artist.name.clone(),
                "Artist".to_string(),
                Some(Route::MusicArtist {
                    id: artist.id.clone(),
                }),
                None,
            ),
            (None, Some(song)) => (
                cover(song.cover.clone(), px(96.), false, "top"),
                song.title.clone(),
                format!("Song · {}", names(&song.artists)),
                song.album
                    .as_ref()
                    .and_then(|a| a.id.clone())
                    .map(|id| Route::MusicAlbum { id }),
                Some(found.songs.clone()),
            ),
            _ => return None,
        };
        let _ = cx;
        Some(
            div()
                .id("music-top-result")
                .group("top-result")
                .relative()
                .w(px(340.))
                .flex_none()
                .flex()
                .flex_col()
                .gap_4()
                .p_5()
                .rounded_lg()
                .bg(color(TEXT).opacity(0.06))
                .hover(|style| style.bg(color(TEXT).opacity(0.1)))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    if let Some(route) = &route {
                        navigate(route.clone(), cx);
                    }
                })
                .child(heading_top())
                .child(art)
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(28.))
                                .font_weight(FontWeight::EXTRA_BOLD)
                                .text_color(color(TEXT))
                                .child(title),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(14.))
                                .text_color(color(TEXT_DIM))
                                .child(subtitle),
                        ),
                )
                .children(tracks.map(|tracks| {
                    div()
                        .id("music-top-play")
                        .absolute()
                        .right(px(20.))
                        .bottom(px(20.))
                        .size(px(48.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(color(TEXT))
                        .invisible()
                        .group_hover("top-result", |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            play(tracks.clone(), 0, cx);
                        })
                        .child(Icon::new(IconName::Play).text_color(color(0x000000)))
                })),
        )
    }

    fn home_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let _ = window;
        let body = match &self.home {
            Answer::Loading => div()
                .flex()
                .gap_3()
                .px(px(GUTTER))
                .overflow_hidden()
                .children((0..6).map(|_| skeleton(px(176.), px(224.)))),
            Answer::Failed => error_state(
                "Music is not answering",
                "Try again in a moment.",
                cx.listener(|page, _, _, cx| page.load_home(cx)),
            ),
            Answer::Loaded(_) => div()
                .flex()
                .flex_col()
                .gap_10()
                .children(self.home_shelves.iter().cloned()),
        };
        let tiles = GENRES.map(|(name, slug, accent, tint)| {
            div()
                .id(SharedString::from(format!("genre-{slug}")))
                .relative()
                .w(px(190.))
                .h(px(112.))
                .p_4()
                .overflow_hidden()
                .rounded_lg()
                .bg(color(tint))
                .border_1()
                .border_color(color(TEXT).opacity(0.06))
                .cursor_pointer()
                .hover(|style| style.border_color(color(TEXT).opacity(0.16)))
                .text_size(px(16.))
                .font_weight(FontWeight::BOLD)
                .text_color(color(TEXT))
                .on_click(move |_, _, cx| {
                    navigate(
                        Route::Music {
                            term: format!("{name} music"),
                        },
                        cx,
                    )
                })
                // The motif is a shape painted in the accent colour, set off the corner so the card crops it.
                .child(
                    div().absolute().right(px(-16.)).bottom(px(-20.)).child(
                        svg()
                            .path(format!("genres/{slug}.svg"))
                            .size(px(112.))
                            .text_color(color(accent)),
                    ),
                )
                .child(div().relative().child(name))
        });
        div().flex().flex_col().gap_10().child(body).child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .px(px(GUTTER))
                .child(heading("Browse"))
                .child(div().flex().flex_wrap().gap_3().children(tiles)),
        )
    }
}

fn heading(text: &'static str) -> Div {
    div()
        .text_size(px(20.))
        .font_weight(FontWeight::BOLD)
        .text_color(color(TEXT))
        .child(text)
}

fn heading_top() -> Div {
    heading("Top result")
}

/// The shelves for an answer: artists first, then albums.
fn shelves(found: &SearchOut, _key: &str, cx: &mut Context<MusicPage>) -> Vec<Entity<Shelf>> {
    let mut out = Vec::new();
    if !found.artists.is_empty() {
        let items = found
            .artists
            .iter()
            .cloned()
            .map(ShelfItem::Artist)
            .collect();
        out.push(cx.new(|_| {
            Shelf::new(
                if _key == "home" {
                    "Popular artists"
                } else {
                    "Artists"
                },
                items,
            )
        }));
    }
    if !found.albums.is_empty() {
        let items = found.albums.iter().cloned().map(ShelfItem::Album).collect();
        out.push(cx.new(|_| {
            Shelf::new(
                if _key == "home" {
                    "Albums to start with"
                } else {
                    "Albums"
                },
                items,
            )
        }));
    }
    out
}

impl Render for MusicPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match (self.term.is_empty(), self.results.take()) {
            (true, results) => {
                self.results = results;
                self.home_view(window, cx)
            }
            (false, Some(Answer::Loaded(found))) => {
                let view = self.results_view(&found, cx);
                self.results = Some(Answer::Loaded(found));
                view
            }
            (false, Some(Answer::Failed)) => {
                self.results = Some(Answer::Failed);
                let term = self.term.clone();
                error_state(
                    "Search is not answering",
                    "Try again in a moment.",
                    cx.listener(move |page, _, _, cx| page.search(term.clone(), cx)),
                )
            }
            (false, other) => {
                self.results = other;
                loading_rows(6)
            }
        };
        let empty = matches!(&self.results, Some(Answer::Loaded(found)) if found.songs.is_empty() && found.albums.is_empty() && found.artists.is_empty());
        div().pt(px(TOP_BAR + 24.)).pb_10().child(if empty {
            empty_state(
                format!("No results for \"{}\"", self.term),
                "Check the spelling, or try the artist's name.",
            )
        } else {
            body
        })
    }
}
