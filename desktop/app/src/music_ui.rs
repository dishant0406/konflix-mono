//! The pieces the music screens share: covers, track rows, album and artist cards, the shelves
//! that hold them, and the wash of colour behind an album or artist page.

use crate::design::tokens::{CANVAS, GUTTER, TEXT, TEXT_DIM, TOP_BAR, WASH, WASH_2, color, radius};
use crate::music_player::{hub, play, toggle};
use crate::route::{Route, navigate};
use akidev_core::music::{AlbumCard, ArtistCard, Credit, Track};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

pub fn names(credits: &[Credit]) -> String {
    credits
        .iter()
        .map(|credit| credit.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A square cover, or a note where there is none. `key` tells two pictures of the same address apart.
pub fn cover(url: Option<String>, size: Pixels, round: bool, key: &str) -> Div {
    let frame = div()
        .flex_none()
        .size(size)
        .overflow_hidden()
        .bg(color(TEXT).opacity(WASH_2))
        .when(round, |frame| frame.rounded(size / 2.))
        .when(!round, |frame| frame.rounded(px(radius::BUTTON)));
    match url {
        Some(url) => frame.child(
            img(url)
                .id(SharedString::from(format!("cover-{key}")))
                .size_full()
                .when(round, |picture| picture.rounded(size / 2.))
                .object_fit(ObjectFit::Cover)
                .with_fallback(|| note().into_any_element()),
        ),
        None => frame.child(note()),
    }
}

fn note() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(IconName::Music).text_color(color(TEXT).opacity(0.4)))
}

/// `table` is an album: numbered rows, no covers (they are all the same cover). Otherwise a small
/// cover leads each row. Pressing a row plays it and queues the rest of the list after it.
pub fn track_list(tracks: &[Track], table: bool, key: &str, cx: &mut App) -> Div {
    let player = hub(cx).player.read(cx);
    let current = player.current().map(|track| track.id.clone());
    let playing = player.is_playing();
    let all = Rc::new(tracks.to_vec());

    let mut list = div().flex().flex_col();
    if table {
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .h(px(36.))
                .px_4()
                .mb_1()
                .border_b_1()
                .border_color(color(TEXT).opacity(WASH))
                .text_size(px(12.))
                .text_color(color(TEXT_DIM))
                .child(div().w(px(32.)).text_center().child("#"))
                .child(div().flex_1().child("Title"))
                .child(
                    div().w(px(48.)).flex().justify_end().child(
                        Icon::new(IconName::Clock)
                            .small()
                            .text_color(color(TEXT_DIM)),
                    ),
                ),
        );
    }
    for (index, track) in tracks.iter().enumerate() {
        let is_current = current.as_deref() == Some(track.id.as_str());
        let group = SharedString::from(format!("{key}-row-{index}"));
        let all = all.clone();
        let lead = if table {
            div()
                .relative()
                .flex_none()
                .w(px(32.))
                .h(px(40.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(color(TEXT_DIM))
                        .group_hover(group.clone(), |style| style.invisible())
                        .child(if is_current {
                            Icon::new(IconName::Volume2)
                                .small()
                                .text_color(color(TEXT))
                                .into_any_element()
                        } else {
                            (index + 1).to_string().into_any_element()
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .invisible()
                        .group_hover(group.clone(), |style| style.visible())
                        .child(
                            Icon::new(if is_current && playing {
                                IconName::Pause
                            } else {
                                IconName::Play
                            })
                            .small()
                            .text_color(color(TEXT)),
                        ),
                )
        } else {
            div()
                .relative()
                .flex_none()
                .size(px(40.))
                .child(cover(
                    track.cover.clone(),
                    px(40.),
                    false,
                    &format!("{key}-{index}"),
                ))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::BUTTON))
                        .bg(color(0x000000).opacity(0.55))
                        .when(!is_current, |overlay| {
                            overlay
                                .invisible()
                                .group_hover(group.clone(), |style| style.visible())
                        })
                        .child(
                            Icon::new(if is_current && playing {
                                IconName::Pause
                            } else {
                                IconName::Play
                            })
                            .small()
                            .text_color(color(TEXT)),
                        ),
                )
        };
        let artists = track
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        list = list.child(
            div()
                .id(group.clone())
                .group(group)
                .flex()
                .items_center()
                .gap_4()
                .h(px(56.))
                .px_4()
                .rounded(px(radius::BUTTON))
                .cursor_pointer()
                .when(is_current, |row| row.bg(color(TEXT).opacity(WASH)))
                .hover(|style| style.bg(color(TEXT).opacity(WASH)))
                .on_click(move |_, _, cx| {
                    if is_current {
                        toggle(cx);
                    } else {
                        play((*all).clone(), index, cx);
                    }
                })
                .child(lead)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(14.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(TEXT))
                                .child(track.title.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(color(TEXT_DIM))
                                .child(artists),
                        ),
                )
                .when(!table, |row| {
                    row.child(
                        div()
                            .w(relative(0.3))
                            .truncate()
                            .text_size(px(13.))
                            .text_color(color(TEXT_DIM))
                            .child(
                                track
                                    .album
                                    .as_ref()
                                    .map(|album| album.name.clone())
                                    .unwrap_or_default(),
                            ),
                    )
                })
                .child(
                    div()
                        .w(px(48.))
                        .text_right()
                        .text_size(px(13.))
                        .text_color(color(TEXT_DIM))
                        .child(track.duration.clone().unwrap_or_default()),
                ),
        );
    }
    list
}

pub const CARD_WIDTH: f32 = 176.;

fn card(
    id: SharedString,
    art: Div,
    title: String,
    subtitle: String,
    route: Route,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .w(px(CARD_WIDTH))
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .rounded(px(radius::PANEL))
        .cursor_pointer()
        .hover(|style| style.bg(color(TEXT).opacity(WASH)))
        .on_click(move |_, _, cx| navigate(route.clone(), cx))
        .child(art)
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .h(px(20.))
                        .line_height(px(20.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .h(px(18.))
                        .line_height(px(18.))
                        .text_size(px(13.))
                        .text_color(color(TEXT_DIM))
                        .child(subtitle),
                ),
        )
}

pub fn album_card(album: &AlbumCard, key: &str) -> Stateful<Div> {
    let subtitle = [album.year.clone(), Some(album.artist.clone())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    card(
        SharedString::from(format!("album-{key}-{}", album.id)),
        cover(
            album.cover.clone(),
            px(CARD_WIDTH - 24.),
            false,
            &format!("{key}-{}", album.id),
        ),
        album.title.clone(),
        subtitle,
        Route::MusicAlbum {
            id: album.id.clone(),
        },
    )
}

pub fn artist_card(artist: &ArtistCard, key: &str) -> Stateful<Div> {
    card(
        SharedString::from(format!("artist-{key}-{}", artist.id)),
        cover(
            artist.cover.clone(),
            px(CARD_WIDTH - 24.),
            true,
            &format!("{key}-{}", artist.id),
        ),
        artist.name.clone(),
        "Artist".into(),
        Route::MusicArtist {
            id: artist.id.clone(),
        },
    )
}

/// A heading over a row of cards that scrolls sideways, with arrows at the edges on hover.
pub struct Shelf {
    title: SharedString,
    key: SharedString,
    items: Vec<ShelfItem>,
    scroll: ScrollHandle,
}

pub enum ShelfItem {
    Album(AlbumCard),
    Artist(ArtistCard),
}

impl Shelf {
    pub fn new(title: &str, items: Vec<ShelfItem>) -> Shelf {
        Shelf {
            title: title.to_string().into(),
            key: title.to_lowercase().replace(' ', "-").into(),
            items,
            scroll: ScrollHandle::new(),
        }
    }

    fn scroll_by(&self, direction: f32) {
        let bounds = self.scroll.bounds();
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset();
        let step = bounds.size.width * 0.85 * direction;
        let x = (offset.x - step).clamp(-max.x, px(0.));
        self.scroll.set_offset(point(x, offset.y));
    }

    fn edge(&self, left: bool, group: SharedString, cx: &mut Context<Self>) -> Div {
        let direction = if left { -1. } else { 1. };
        let (from, to) = if left { (0.92, 0.) } else { (0., 0.92) };
        div()
            .absolute()
            .top_0()
            .h(px(CARD_WIDTH + 60.))
            .w(px(64.))
            .flex()
            .items_center()
            .justify_center()
            .when_else(left, |edge| edge.left_0(), |edge| edge.right_0())
            .bg(linear_gradient(
                90.,
                linear_color_stop(color(CANVAS).opacity(from), 0.),
                linear_color_stop(color(CANVAS).opacity(to), 1.),
            ))
            .invisible()
            .group_hover(group, |style| style.visible())
            .child(crate::design::button::icon_button(
                if left { "shelf-left" } else { "shelf-right" },
                if left {
                    IconName::ChevronLeft
                } else {
                    IconName::ChevronRight
                },
                cx.listener(move |shelf, _, _, cx| {
                    shelf.scroll_by(direction);
                    cx.notify();
                }),
            ))
    }
}

impl Render for Shelf {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let group = SharedString::from(format!("shelf-{}", self.key));
        let cards: Vec<AnyElement> = self
            .items
            .iter()
            .map(|item| match item {
                ShelfItem::Album(album) => album_card(album, &self.key).into_any_element(),
                ShelfItem::Artist(artist) => artist_card(artist, &self.key).into_any_element(),
            })
            .collect();
        div()
            .group(group.clone())
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .px(px(GUTTER))
                    .h(px(28.))
                    .line_height(px(28.))
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT))
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id(SharedString::from(format!("{}-row", self.key)))
                            .flex()
                            .gap_1()
                            .px(px(GUTTER - 12.))
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&self.scroll)
                            .children(cards),
                    )
                    .child(self.edge(true, group.clone(), cx))
                    .child(self.edge(false, group, cx)),
            )
    }
}

/// Where the title is: it is `TITLE_SIZE` tall and one line, and a long one is made smaller.
const TITLE_SIZE: f32 = 64.;

/// The wash and the row of cover and text, which the hero and its skeleton are both made on.
fn hero_frame(tint: u32, art: Div, column: Div) -> Div {
    div()
        .relative()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(420.))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(color(tint).opacity(0.55), 0.),
                    linear_color_stop(color(CANVAS), 1.),
                )),
        )
        .child(
            div()
                .relative()
                .flex()
                .items_end()
                .gap_6()
                .px(px(GUTTER))
                .pb_6()
                .pt(px(TOP_BAR + 32.))
                .child(art)
                .child(column.min_w_0().flex().flex_col().gap_2()),
        )
}

/// The top of an album or artist page: the cover beside a large title, on a wash of the cover's
/// own colour that fades into the page and starts behind the bar.
pub fn hero(tint: u32, art: Div, kind: &str, title: &str, details: impl IntoElement) -> Div {
    let size = if title.chars().count() > 28 {
        44.
    } else {
        TITLE_SIZE
    };
    hero_frame(
        tint,
        art,
        div()
            .child(
                div()
                    .h(px(20.))
                    .line_height(px(20.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT))
                    .child(kind.to_string()),
            )
            .child(
                div()
                    .text_size(px(size))
                    .line_height(px(size * 1.05))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT))
                    .line_clamp(2)
                    .child(title.to_string()),
            )
            .child(details),
    )
}

/// The one large white play button under a hero, and shuffle beside it.
pub fn actions(tracks: &[Track]) -> Div {
    let all = Rc::new(tracks.to_vec());
    let shuffle = all.clone();
    div()
        .flex()
        .items_center()
        .gap_5()
        .px(px(GUTTER))
        .py_4()
        .child(
            div()
                .id("music-play-all")
                .flex()
                .items_center()
                .justify_center()
                .size(px(56.))
                .rounded_full()
                .bg(color(TEXT))
                .cursor_pointer()
                .hover(|style| style.bg(color(TEXT).opacity(0.85)))
                .on_click(move |_, _, cx| play((*all).clone(), 0, cx))
                .child(Icon::new(IconName::Play).text_color(color(0x000000))),
        )
        .child(
            div()
                .id("music-shuffle-all")
                .flex()
                .items_center()
                .justify_center()
                .size(px(40.))
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(color(TEXT).opacity(WASH_2)))
                .on_click(move |_, _, cx| {
                    crate::music_player::play_shuffled((*shuffle).clone(), cx)
                })
                .child(Icon::new(IconName::Shuffle).text_color(color(TEXT_DIM))),
        )
}

/// A grey block. Everything the skeletons are made of.
fn bone() -> gpui_kit::component::skeleton::Skeleton {
    gpui_kit::component::skeleton::Skeleton::new()
}

/// A line of text's worth of height with a grey bar of `width` in it (a share of the line), so a
/// skeleton line is as tall as the real one and the two swap without anything moving.
fn line(height: f32, bar: f32, width: DefiniteLength) -> Div {
    div()
        .h(px(height))
        .flex()
        .items_center()
        .child(bone().h(px(bar)).w(width).rounded(px(radius::BUTTON)))
}

/// What a shelf looks like before its cards arrive: the same heading, row and cards, in grey.
pub fn shelf_skeleton(round: bool) -> Div {
    let cover = px(CARD_WIDTH - 24.);
    let cards = (0..10).map(|_| {
        div()
            .flex_none()
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                bone()
                    .size(cover)
                    .when(round, |cover| cover.rounded_full())
                    .when(!round, |cover| cover.rounded(px(radius::BUTTON))),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(line(20., 14., relative(0.7)))
                    .child(line(18., 12., relative(0.45))),
            )
    });
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().px(px(GUTTER)).child(line(28., 20., px(160.).into())))
        .child(
            div()
                .flex()
                .gap_1()
                .px(px(GUTTER - 12.))
                .overflow_hidden()
                .children(cards),
        )
}

/// The rows of a track list before its tracks arrive: the same header, row height and padding.
pub fn track_skeleton(table: bool, rows: usize) -> Div {
    let mut list = div().flex().flex_col();
    if table {
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .h(px(36.))
                .px_4()
                .mb_1()
                .border_b_1()
                .border_color(color(TEXT).opacity(WASH))
                .child(
                    div()
                        .w(px(32.))
                        .flex()
                        .justify_center()
                        .child(bone().w(px(10.)).h(px(10.)).rounded(px(radius::BUTTON))),
                )
                .child(
                    div()
                        .flex_1()
                        .child(bone().w(px(36.)).h(px(10.)).rounded(px(radius::BUTTON))),
                )
                .child(
                    div()
                        .w(px(48.))
                        .flex()
                        .justify_end()
                        .child(bone().w(px(14.)).h(px(10.)).rounded(px(radius::BUTTON))),
                ),
        );
    }
    list.children((0..rows).map(|_| {
        div()
            .flex()
            .items_center()
            .gap_4()
            .h(px(56.))
            .px_4()
            .child(if table {
                div()
                    .flex_none()
                    .w(px(32.))
                    .flex()
                    .justify_center()
                    .child(bone().w(px(10.)).h(px(12.)).rounded(px(radius::BUTTON)))
            } else {
                div()
                    .flex_none()
                    .size(px(40.))
                    .child(bone().size(px(40.)).rounded(px(radius::BUTTON)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(line(20., 14., relative(0.4)))
                    .child(line(18., 12., relative(0.25))),
            )
            .when(!table, |row| {
                row.child(div().w(relative(0.3)).child(line(18., 12., relative(0.5))))
            })
            .child(
                div()
                    .w(px(48.))
                    .flex()
                    .justify_end()
                    .child(bone().w(px(30.)).h(px(12.)).rounded(px(radius::BUTTON))),
            )
    }))
}

/// The hero before the page arrives: the same wash, cover and rows of text, in grey.
fn hero_skeleton(round: bool) -> Div {
    hero_frame(
        0x464646,
        div().flex_none().size(px(232.)).child(
            bone()
                .size(px(232.))
                .when(round, |cover| cover.rounded_full())
                .when(!round, |cover| cover.rounded(px(radius::BUTTON))),
        ),
        div()
            .child(line(20., 12., px(48.).into()))
            .child(
                div().h(px(TITLE_SIZE * 1.05)).flex().items_center().child(
                    bone()
                        .h(px(TITLE_SIZE * 0.8))
                        .w(px(480.))
                        .rounded(px(radius::BUTTON)),
                ),
            )
            .child(line(20., 14., px(260.).into())),
    )
}

/// The play and shuffle buttons, as grey circles of the same size.
fn actions_skeleton() -> Div {
    div()
        .flex()
        .items_center()
        .gap_5()
        .px(px(GUTTER))
        .py_4()
        .child(bone().size(px(56.)).rounded_full())
        .child(bone().size(px(40.)).rounded_full())
}

/// An album page before it arrives: its hero, buttons and numbered rows.
pub fn loading_album() -> Div {
    div()
        .pb_10()
        .child(hero_skeleton(false))
        .child(actions_skeleton())
        .child(div().px(px(GUTTER - 16.)).child(track_skeleton(true, 12)))
}

/// An artist page before it arrives: the hero, buttons, popular songs and two shelves.
pub fn loading_artist() -> Div {
    div()
        .pb_10()
        .flex()
        .flex_col()
        .gap_8()
        .child(div().child(hero_skeleton(true)).child(actions_skeleton()))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px(px(GUTTER - 16.))
                .child(div().px_4().pb_2().child(line(28., 20., px(90.).into())))
                .child(track_skeleton(true, 5)),
        )
        .child(shelf_skeleton(false))
        .child(shelf_skeleton(false))
}

/// The colour a cover is mostly made of, as 0xRRGGBB, for the wash behind a page. Colourful pixels
/// count for more, so a dark cover with a red mark gives red rather than grey.
pub async fn tint_of(url: String) -> Option<u32> {
    let bytes = akidev_core::http::client()
        .get(&url)
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;
    tokio::task::spawn_blocking(move || average(&bytes))
        .await
        .ok()?
}

fn average(bytes: &[u8]) -> Option<u32> {
    let small = image::load_from_memory(bytes)
        .ok()?
        .resize_exact(24, 24, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let (mut r, mut g, mut b, mut weight) = (0f64, 0f64, 0f64, 0f64);
    for pixel in small.pixels() {
        let [pr, pg, pb, _] = pixel.0;
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        let w = 0.1 + f64::from(max - min) / 255. + if max > 30 && max < 235 { 0.3 } else { 0. };
        r += f64::from(pr) * w;
        g += f64::from(pg) * w;
        b += f64::from(pb) * w;
        weight += w;
    }
    (weight > 0.).then(|| {
        (((r / weight) as u32) << 16) | (((g / weight) as u32) << 8) | ((b / weight) as u32)
    })
}
