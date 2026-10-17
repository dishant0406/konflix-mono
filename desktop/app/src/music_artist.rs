//! An artist: their photo and name over a wash of its colour, their popular songs, and their releases.

use crate::design::state::error_state;
use crate::design::tokens::{GUTTER, TEXT, TEXT_DIM, TOP_BAR, color};
use crate::music_player::{Changed, hub};
use crate::music_ui::{
    Shelf, ShelfItem, actions, cover, hero, loading_artist, tint_of, track_list,
};
use crate::task::load;
use akidev_core::music::ArtistOut;
use gpui_kit::*;

enum State {
    Loading,
    Failed,
    Loaded(Box<ArtistOut>),
}

pub struct ArtistPage {
    id: String,
    state: State,
    tint: u32,
    shelves: Vec<Entity<Shelf>>,
    _changes: Subscription,
}

const NEUTRAL: u32 = 0x464646;

impl ArtistPage {
    pub fn new(id: String, cx: &mut Context<Self>) -> ArtistPage {
        let player = hub(cx).player.clone();
        let changes = cx.subscribe(&player, |_, _, _: &Changed, cx| cx.notify());
        let mut page = ArtistPage {
            id,
            state: State::Loading,
            tint: NEUTRAL,
            shelves: Vec::new(),
            _changes: changes,
        };
        page.fetch(cx);
        page
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        self.state = State::Loading;
        let music = hub(cx).music.clone();
        let id = self.id.clone();
        load(
            cx,
            async move { music.artist(&id).await.ok() },
            |page, artist, cx| {
                match artist {
                    Some(artist) => {
                        if let Some(cover) = artist.cover.clone() {
                            load(cx, tint_of(cover), |page, tint, cx| {
                                if let Some(tint) = tint {
                                    page.tint = tint;
                                    cx.notify();
                                }
                            });
                        }
                        page.shelves = shelves(&artist, cx);
                        page.state = State::Loaded(Box::new(artist));
                    }
                    None => page.state = State::Failed,
                }
                cx.notify();
            },
        );
    }
}

fn shelves(artist: &ArtistOut, cx: &mut Context<ArtistPage>) -> Vec<Entity<Shelf>> {
    let mut out = Vec::new();
    for (title, albums) in [("Albums", &artist.albums), ("Singles", &artist.singles)] {
        if !albums.is_empty() {
            let items = albums.iter().cloned().map(ShelfItem::Album).collect();
            out.push(cx.new(|_| Shelf::new(title, items)));
        }
    }
    if !artist.related.is_empty() {
        let items = artist
            .related
            .iter()
            .cloned()
            .map(ShelfItem::Artist)
            .collect();
        out.push(cx.new(|_| Shelf::new("Fans also like", items)));
    }
    out
}

impl Render for ArtistPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let artist = match &self.state {
            State::Loaded(artist) => artist.clone(),
            State::Failed => {
                return div().pt(px(TOP_BAR + 24.)).child(error_state(
                    "That artist could not be loaded",
                    "Try again in a moment.",
                    cx.listener(|page, _, _, cx| page.fetch(cx)),
                ));
            }
            State::Loading => return loading_artist(),
        };
        let details = div()
            .min_h(px(20.))
            .line_height(px(20.))
            .text_size(px(14.))
            .text_color(color(TEXT_DIM))
            .child(
                artist
                    .subscribers
                    .clone()
                    .map(|s| format!("{s} subscribers"))
                    .unwrap_or_default(),
            );
        div()
            .pb_10()
            .flex()
            .flex_col()
            .gap_8()
            .child(
                div()
                    .child(hero(
                        self.tint,
                        cover(artist.cover.clone(), px(232.), true, "artist-hero"),
                        "Artist",
                        &artist.name,
                        details,
                    ))
                    .child(actions(&artist.songs)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px(px(GUTTER - 16.))
                    .child(
                        div().px_4().pb_2().child(
                            div()
                                .h(px(28.))
                                .line_height(px(28.))
                                .text_size(px(20.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(color(TEXT))
                                .child("Popular"),
                        ),
                    )
                    .child(track_list(&artist.songs, true, "popular", cx)),
            )
            .children(self.shelves.iter().cloned())
            .child(
                div()
                    .px(px(GUTTER))
                    .max_w(px(900.))
                    .text_size(px(14.))
                    .line_height(px(22.))
                    .text_color(color(TEXT_DIM))
                    .child(artist.description.clone().unwrap_or_default()),
            )
    }
}
