//! An album: its cover and title over a wash of the cover's colour, and its tracks.

use crate::design::state::error_state;
use crate::design::tokens::{GUTTER, TEXT, TEXT_DIM, TOP_BAR, color};
use crate::music_player::{Changed, hub};
use crate::music_ui::{actions, cover, hero, loading_album, tint_of, track_list};
use crate::route::{Route, navigate};
use crate::task::load;
use akidev_core::music::AlbumOut;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

enum State {
    Loading,
    Failed,
    Loaded(Box<AlbumOut>),
}

pub struct AlbumPage {
    id: String,
    state: State,
    tint: u32,
    _changes: Subscription,
}

/// The wash before the cover's own colour is known.
const NEUTRAL: u32 = 0x464646;

impl AlbumPage {
    pub fn new(id: String, cx: &mut Context<Self>) -> AlbumPage {
        let player = hub(cx).player.clone();
        let changes = cx.subscribe(&player, |_, _, _: &Changed, cx| cx.notify());
        let mut page = AlbumPage {
            id,
            state: State::Loading,
            tint: NEUTRAL,
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
            async move { music.album(&id).await.ok() },
            |page, album, cx| {
                match album {
                    Some(album) => {
                        if let Some(cover) = album.cover.clone() {
                            load(cx, tint_of(cover), |page, tint, cx| {
                                if let Some(tint) = tint {
                                    page.tint = tint;
                                    cx.notify();
                                }
                            });
                        }
                        page.state = State::Loaded(Box::new(album));
                    }
                    None => page.state = State::Failed,
                }
                cx.notify();
            },
        );
    }
}

impl Render for AlbumPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let album = match &self.state {
            State::Loaded(album) => album.clone(),
            State::Failed => {
                return div().pt(px(TOP_BAR + 24.)).child(error_state(
                    "That album could not be loaded",
                    "It may have been removed.",
                    cx.listener(|page, _, _, cx| page.fetch(cx)),
                ));
            }
            State::Loading => return loading_album(),
        };
        let artists = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_1()
            .min_h(px(20.))
            .line_height(px(20.))
            .text_size(px(14.))
            .text_color(color(TEXT_DIM))
            .children(album.artists.iter().enumerate().flat_map(|(n, artist)| {
                let id = artist.id.clone();
                let name = artist.name.clone();
                let mut parts = Vec::new();
                if n > 0 {
                    parts.push(div().child(",").into_any_element());
                }
                parts.push(
                    div()
                        .id(SharedString::from(format!("album-artist-{n}")))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .when(id.is_some(), |d| {
                            d.cursor_pointer().hover(|s| s.underline()).on_click({
                                let id = id.clone().unwrap_or_default();
                                move |_, _, cx| navigate(Route::MusicArtist { id: id.clone() }, cx)
                            })
                        })
                        .child(name)
                        .into_any_element(),
                );
                parts
            }))
            .child(div().child(format!("· {}", album.year)))
            .child(div().child(format!(
                "· {} songs, {}",
                album.tracks.len(),
                album.duration
            )));
        div()
            .pb_10()
            .child(hero(
                self.tint,
                cover(album.cover.clone(), px(232.), false, "album-hero"),
                album.kind.as_deref().unwrap_or("Album"),
                &album.title,
                artists,
            ))
            .child(actions(&album.tracks))
            .child(
                div()
                    .px(px(GUTTER - 16.))
                    .child(track_list(&album.tracks, true, "album", cx)),
            )
    }
}
