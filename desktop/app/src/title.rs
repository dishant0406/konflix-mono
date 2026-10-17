//! A title's detail page: a banner with the actions, then episodes, cast and related titles.

use crate::backend::Backend;
use crate::design::button::{primary_button, secondary_button};
use crate::design::state::{
    error_state, kind_label, meta_line, score_label, section_title, skeleton,
};
use crate::design::tokens::{
    ACCENT, CANVAS, GUTTER, TEXT, TEXT_DIM, TEXT_FAINT, TOP_BAR, WASH_2, color, font, size,
};
use crate::episodes::EpisodeList;
use crate::route::{Route, navigate};
use crate::row::MediaRow;
use crate::task::load;
use akidev_core::media_id::{self, anime_episode_id};
use akidev_core::model::{Detail, Kind, Media, Person};
use akidev_core::providers;
use gpui_kit::assets::IconName;
use gpui_kit::*;

enum State {
    Loading,
    Failed,
    Loaded(Box<Detail>),
}

pub struct TitlePage {
    id: String,
    state: State,
    episodes: Option<Entity<EpisodeList>>,
    related: Option<Entity<MediaRow>>,
    recommended: Option<Entity<MediaRow>>,
}

impl TitlePage {
    pub fn new(id: String, cx: &mut Context<Self>) -> TitlePage {
        let mut page = TitlePage {
            id,
            state: State::Loading,
            episodes: None,
            related: None,
            recommended: None,
        };
        page.fetch(cx);
        page
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        self.state = State::Loading;
        cx.notify();
        let id = self.id.clone();
        load(
            cx,
            async move { providers::detail(&id).await },
            |page, detail, cx| {
                match detail {
                    None => page.state = State::Failed,
                    Some(detail) => {
                        page.episodes = Some(cx.new(|_| EpisodeList::new(&detail, None, false)));
                        page.related = row("Related", &detail.related, cx);
                        page.recommended = row("More like this", &detail.recommended, cx);
                        page.state = State::Loaded(Box::new(detail));
                    }
                }
                cx.notify();
            },
        );
    }
}

fn row(title: &str, items: &[Media], cx: &mut Context<TitlePage>) -> Option<Entity<MediaRow>> {
    (!items.is_empty()).then(|| {
        let (title, items) = (title.to_string(), items.to_vec());
        cx.new(|_| MediaRow::with_items(title, items))
    })
}

/// What plays when Play is pressed: the film itself, or the first episode.
fn first_playable(detail: &Detail) -> Option<String> {
    let media = &detail.media;
    let parsed = media_id::parse(&media.id)?;
    match media.kind {
        Kind::Movie => Some(media.id.clone()),
        Kind::Series => detail
            .episodes
            .iter()
            .min_by_key(|e| (e.season, e.number))
            .map(|e| e.id.clone()),
        Kind::Anime => Some(anime_episode_id(&parsed.provider_id, 1)),
    }
}

impl Render for TitlePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = (window.viewport_size().height * 0.7).clamp(px(480.), px(700.));
        let detail = match &self.state {
            State::Loading => {
                return div()
                    .h(height)
                    .child(skeleton(px(3000.), height).rounded_none())
                    .into_any_element();
            }
            State::Failed => {
                return div()
                    .pt(px(160.))
                    .child(error_state(
                        "Couldn't load this title",
                        "It may not exist, or the catalogue didn't answer.",
                        cx.listener(|page, _, _, cx| page.fetch(cx)),
                    ))
                    .into_any_element();
            }
            State::Loaded(detail) => detail,
        };
        let media = &detail.media;

        let art = media.banner.clone().or_else(|| media.poster.clone());
        let facts = [
            media.year.map(|y| y.to_string()),
            media
                .format
                .as_deref()
                .and_then(crate::design::state::format_label),
            media.runtime_minutes.map(|m| format!("{m} min")),
            media
                .status
                .as_ref()
                .map(|s| crate::design::state::sentence(&s.replace('_', " "))),
            media
                .episodes
                .filter(|_| media.kind != Kind::Movie)
                .map(|n| format!("{n} episodes")),
        ];
        // Carry on where the viewer left off, if they have started this title.
        let started = cx
            .global::<Backend>()
            .progress
            .recent()
            .iter()
            .find(|entry| entry.media.id == media.id)
            .map(|entry| entry.episode_id.clone());
        let play = match started {
            Some(id) => Some((id, "Resume")),
            None => first_playable(detail).map(|id| (id, "Play")),
        };
        let trailer = detail
            .trailer
            .as_ref()
            .filter(|t| t.site.eq_ignore_ascii_case("youtube"))
            .map(|t| t.id.clone());

        // At least `height` tall, and taller when what it says needs the room: the text is in the
        // flow, so a long title or description makes the banner grow and does not climb under the bar.
        let hero = div()
            .relative()
            .w_full()
            .min_h(height)
            .flex()
            .flex_col()
            .justify_end()
            .overflow_hidden()
            .bg(color(CANVAS))
            .children(art.map(|url| {
                div().absolute().size_full().child(
                    img(url)
                        .id(SharedString::from(format!("title-art-{}", media.id)))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
            }))
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(color(CANVAS), 0.),
                linear_color_stop(color(CANVAS).opacity(0.), 0.7),
            )))
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(color(CANVAS).opacity(0.), 0.5),
                linear_color_stop(color(CANVAS), 1.),
            )))
            .child(
                div()
                    .relative()
                    .px(px(GUTTER))
                    .pt(px(TOP_BAR + 40.))
                    .pb(px(48.))
                    .max_w(px(640. + 2. * GUTTER))
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(ACCENT))
                                    .child(kind_label(media.kind)),
                            )
                            .children(media.score.filter(|&s| s > 0).map(score_label)),
                    )
                    .child(
                        div()
                            .text_size(px(size::BILLBOARD))
                            .line_height(relative(1.15))
                            .font_family(font::TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color(TEXT))
                            .line_clamp(3)
                            .child(media.title.clone()),
                    )
                    .child(meta_line(facts.into_iter().flatten()))
                    .children(media.description.clone().map(|text| {
                        // One paragraph: a description's own line breaks would get past the clamp.
                        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        div()
                            .text_size(px(15.))
                            .line_height(relative(1.5))
                            .text_color(color(TEXT_DIM))
                            .text_ellipsis()
                            .line_clamp(4)
                            .child(text)
                    }))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(TEXT_FAINT))
                            .child(
                                media
                                    .genres
                                    .iter()
                                    .take(4)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(" · "),
                            ),
                    )
                    .children((!detail.studios.is_empty()).then(|| {
                        div()
                            .text_size(px(13.))
                            .text_color(color(TEXT_FAINT))
                            .child(detail.studios.join(", "))
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .pt_2()
                            .children(play.map(|(id, label)| {
                                primary_button(
                                    "play",
                                    label,
                                    Some(IconName::Play),
                                    move |_, _, cx| navigate(Route::Watch { id: id.clone() }, cx),
                                )
                            }))
                            .children(trailer.map(|id| {
                                secondary_button(
                                    "trailer",
                                    "Trailer",
                                    Some(IconName::ExternalLink),
                                    move |_, _, cx| {
                                        cx.open_url(&format!(
                                            "https://www.youtube.com/watch?v={id}"
                                        ))
                                    },
                                )
                            })),
                    ),
            );

        div()
            .pb_16()
            .child(hero)
            .children(self.episodes.clone())
            .children(cast(&detail.cast))
            .children(self.related.clone())
            .children(self.recommended.clone())
            .into_any_element()
    }
}

fn cast(people: &[Person]) -> Option<Div> {
    if people.is_empty() {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_col()
            .gap_4()
            .px(px(GUTTER))
            .pt_10()
            .child(section_title("Cast"))
            .child(div().flex().flex_wrap().gap_x_6().gap_y_4().children(
                people.iter().enumerate().map(|(index, person)| {
                    let avatar = div()
                        .flex_none()
                        .size(px(52.))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(color(TEXT).opacity(WASH_2));
                    let avatar = match &person.image {
                        Some(url) => avatar.child(
                            img(url.clone())
                                .id(SharedString::from(format!("cast-{index}")))
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        ),
                        None => avatar,
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .w(px(230.))
                        .child(avatar)
                        .child(
                            div()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(color(TEXT))
                                        .truncate()
                                        .child(person.name.clone()),
                                )
                                .children(person.role.clone().map(|role| {
                                    div()
                                        .text_size(px(12.))
                                        .text_color(color(TEXT_DIM))
                                        .truncate()
                                        .child(role)
                                })),
                        )
                }),
            )),
    )
}
