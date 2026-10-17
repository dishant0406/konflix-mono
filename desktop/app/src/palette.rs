//! The search palette (Cmd-K): results appear as you type, and Enter opens the best one.

use crate::design::state::{kind_label, meta_line};
use crate::design::tokens::{CANVAS, LINE, RAISED, SURFACE, TEXT, TEXT_DIM, TEXT_FAINT, color};
use crate::music_player::{hub, play};
use crate::music_ui::{cover, names};
use crate::route::{Route, navigate};
use akidev_core::model::{Kind, Media};
use akidev_core::music::{SearchOut, Track};
use akidev_core::search::{combine, search_kind};
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

/// What pressing a music row does.
#[derive(Clone)]
enum MusicAction {
    /// Plays the song at this place in the songs found, and queues the rest.
    Play(usize),
    Open(Route),
}

/// One row of a music search: a song, an artist or an album.
struct MusicRow {
    title: String,
    label: &'static str,
    facts: Vec<String>,
    cover: Option<String>,
    round: bool,
    action: MusicAction,
}

/// The first few of each kind of result, songs first.
fn music_rows(found: &SearchOut) -> Vec<MusicRow> {
    let songs = found
        .songs
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, song)| MusicRow {
            title: song.title.clone(),
            label: "SONG",
            facts: [Some(names(&song.artists)), song.duration.clone()]
                .into_iter()
                .flatten()
                .collect(),
            cover: song.cover.clone(),
            round: false,
            action: MusicAction::Play(index),
        });
    let artists = found.artists.iter().take(2).map(|artist| MusicRow {
        title: artist.name.clone(),
        label: "ARTIST",
        facts: artist.subscribers.clone().into_iter().collect(),
        cover: artist.cover.clone(),
        round: true,
        action: MusicAction::Open(Route::MusicArtist {
            id: artist.id.clone(),
        }),
    });
    let albums = found.albums.iter().take(3).map(|album| MusicRow {
        title: album.title.clone(),
        label: "ALBUM",
        facts: [Some(album.artist.clone()), album.year.clone()]
            .into_iter()
            .flatten()
            .collect(),
        cover: album.cover.clone(),
        round: false,
        action: MusicAction::Open(Route::MusicAlbum {
            id: album.id.clone(),
        }),
    });
    songs.chain(artists).chain(albums).collect()
}

pub struct Palette {
    input: Entity<InputState>,
    open: bool,
    /// Searching music (on the music screens) rather than titles.
    music: bool,
    music_rows: Vec<MusicRow>,
    /// Every song the search found, which a pressed song queues.
    music_songs: Vec<Track>,
    results: Vec<Media>,
    /// What each catalogue has said about the term being searched, `None` for one that failed.
    answers: Vec<(Kind, Option<Vec<Media>>)>,
    /// Searches still out for the current term.
    pending: usize,
    searching: bool,
    failed: bool,
    /// The results on screen are for an earlier term, kept until the new ones come so the list
    /// does not blink empty while typing. Enter does not open one of them.
    stale: bool,
    selected: usize,
    /// The newest request. A search that finds it has changed after its pause drops out.
    latest: Arc<AtomicU32>,
    _events: Subscription,
}

impl Palette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Palette {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let events = cx.subscribe(&input, |palette, _, event: &InputEvent, cx| match event {
            InputEvent::Change => palette.typed(cx),
            InputEvent::PressEnter { .. } => palette.confirm(cx),
            _ => {}
        });
        Palette {
            input,
            open: false,
            music: false,
            music_rows: Vec::new(),
            music_songs: Vec::new(),
            results: Vec::new(),
            answers: Vec::new(),
            pending: 0,
            searching: false,
            failed: false,
            stale: false,
            selected: 0,
            latest: Arc::new(AtomicU32::new(0)),
            _events: events,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn show(&mut self, window: &mut Window, music: bool, cx: &mut Context<Self>) {
        self.open = true;
        self.music = music;
        self.music_rows.clear();
        self.music_songs.clear();
        self.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.results.clear();
        self.answers.clear();
        self.pending = 0;
        self.searching = false;
        self.stale = false;
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.latest.fetch_add(1, Ordering::SeqCst);
        cx.notify();
    }

    /// How many rows are showing.
    fn len(&self) -> usize {
        if self.music {
            self.music_rows.len()
        } else {
            self.results.len()
        }
    }

    fn term(&self, cx: &App) -> String {
        self.input.read(cx).value().trim().to_string()
    }

    fn typed(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        let request = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
        self.selected = 0;
        self.answers.clear();
        if term.is_empty() {
            self.results.clear();
            self.music_rows.clear();
            self.music_songs.clear();
            self.pending = 0;
            self.searching = false;
            self.stale = false;
            cx.notify();
            return;
        }
        if self.music {
            self.typed_music(term, request, cx);
            return;
        }
        // The last results stay up, marked as old, until the first of the new ones arrives.
        self.stale = !self.results.is_empty();
        self.searching = true;
        self.failed = false;
        self.pending = Kind::ALL.len();
        cx.notify();

        // One search per catalogue, each shown when it arrives: one that is slow, or has to try
        // several providers, does not hold back the others.
        for kind in Kind::ALL {
            let (latest, wanted) = (self.latest.clone(), term.clone());
            crate::task::load(
                cx,
                async move {
                    tokio::time::sleep(PAUSE).await;
                    if latest.load(Ordering::SeqCst) != request {
                        return None;
                    }
                    Some(search_kind(kind, &wanted).await.ok())
                },
                {
                    let term = term.clone();
                    move |palette, answer, cx| {
                        // Typing on has superseded this search, or the palette was closed.
                        let Some(answer) =
                            answer.filter(|_| palette.latest.load(Ordering::SeqCst) == request)
                        else {
                            return;
                        };
                        palette.received(kind, answer, &term, cx);
                    }
                },
            );
        }
    }

    /// A music search: one request, shown when it arrives.
    fn typed_music(&mut self, term: String, request: u32, cx: &mut Context<Self>) {
        self.stale = !self.music_rows.is_empty();
        self.searching = true;
        self.failed = false;
        cx.notify();
        let music = hub(cx).music.clone();
        let latest = self.latest.clone();
        crate::task::load(
            cx,
            async move {
                tokio::time::sleep(PAUSE).await;
                if latest.load(Ordering::SeqCst) != request {
                    return None;
                }
                Some(music.search(&term).await.ok())
            },
            move |palette, answer, cx| {
                let Some(answer) =
                    answer.filter(|_| palette.latest.load(Ordering::SeqCst) == request)
                else {
                    return;
                };
                palette.searching = false;
                palette.stale = false;
                palette.selected = 0;
                match answer {
                    Some(found) => {
                        palette.music_rows = music_rows(&found);
                        palette.music_songs = found.songs;
                    }
                    None => {
                        palette.music_rows.clear();
                        palette.failed = true;
                    }
                }
                cx.notify();
            },
        );
    }

    /// Does what a music row does: plays the song, or opens the album or artist.
    fn activate_music(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(action) = self.music_rows.get(index).map(|row| row.action.clone()) else {
            return;
        };
        match action {
            MusicAction::Play(song) => play(self.music_songs.clone(), song, cx),
            MusicAction::Open(route) => navigate(route, cx),
        }
        self.close(cx);
    }

    /// One catalogue's answer to the current term: the list is rebuilt from everything that has
    /// come back so far.
    fn received(
        &mut self,
        kind: Kind,
        answer: Option<Vec<Media>>,
        term: &str,
        cx: &mut Context<Self>,
    ) {
        self.answers.push((kind, answer));
        self.pending = self.pending.saturating_sub(1);
        // In the order of the catalogues, so a late answer slots in and does not shuffle the rest.
        let lists = Kind::ALL
            .iter()
            .filter_map(|kind| {
                self.answers
                    .iter()
                    .find(|(answered, _)| answered == kind)
                    .and_then(|(_, media)| media.clone())
            })
            .collect();
        self.results = combine(lists, term, MAX_RESULTS);
        self.stale = false;
        self.selected = 0;
        self.searching = self.pending > 0;
        self.failed = self.pending == 0 && self.answers.iter().all(|(_, media)| media.is_none());
        cx.notify();
    }

    /// Enter: the highlighted result, or the full results page when nothing is highlighted.
    fn confirm(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        if self.music {
            if !self.stale && self.selected < self.music_rows.len() {
                self.activate_music(self.selected, cx);
            } else if !term.is_empty() {
                navigate(Route::Music { term }, cx);
                self.close(cx);
            }
            return;
        }
        let settled = (!self.stale)
            .then(|| self.results.get(self.selected))
            .flatten();
        if let Some(media) = settled {
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

    fn music_row(&self, index: usize, row: &MusicRow, cx: &mut Context<Self>) -> Stateful<Div> {
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
            .on_click(cx.listener(move |palette, _, _, cx| palette.activate_music(index, cx)))
            .child(cover(
                row.cover.clone(),
                px(44.),
                row.round,
                &format!("palette-{index}"),
            ))
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
                            .child(row.title.clone()),
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
                                    .child(row.label),
                            )
                            .child(meta_line(row.facts.clone()).text_size(px(12.))),
                    ),
            )
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
        let stale = self.stale;
        let results: Vec<Stateful<Div>> = if self.music {
            self.music_rows
                .iter()
                .enumerate()
                .map(|(i, row)| self.music_row(i, row, cx))
                .collect()
        } else {
            self.results
                .iter()
                .enumerate()
                .map(|(i, media)| self.row(i, media, cx))
                .collect()
        };
        let hint = if term_empty {
            Some(if self.music {
                "Type to search songs, albums and artists."
            } else {
                "Type to search anime, series and films."
            })
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
                    "down" if palette.len() > 0 => {
                        palette.selected = (palette.selected + 1) % palette.len();
                        cx.notify();
                    }
                    "up" if palette.len() > 0 => {
                        palette.selected = (palette.selected + palette.len() - 1) % palette.len();
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
                                .when(stale, |list| list.opacity(0.5))
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
