//! The episode list shared by the title page and the watch page.

use crate::design::button::tab;
use crate::design::state::section_title;
use crate::design::tokens::{ACCENT, CANVAS, GUTTER, TEXT, TEXT_DIM, TEXT_FAINT, color};
use crate::route::{Route, navigate, replace};
use akidev_core::media_id::{self, anime_episode_id};
use akidev_core::model::{Detail, Episode, Kind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub struct EpisodeList {
    kind: Kind,
    provider_id: String,
    episodes: Vec<Episode>,
    /// An anime reports a count instead of a list.
    count: u32,
    season: u32,
    /// The episode being watched, if any.
    current: Option<String>,
    /// Fits a side panel: narrower padding and no heading.
    compact: bool,
}

impl EpisodeList {
    pub fn new(detail: &Detail, current: Option<String>, compact: bool) -> EpisodeList {
        let provider_id = media_id::parse(&detail.media.id)
            .map(|p| p.provider_id)
            .unwrap_or_default();
        let season = current
            .as_deref()
            .and_then(media_id::parse)
            .and_then(|parsed| parsed.season)
            .or_else(|| detail.episodes.iter().filter_map(|e| e.season).min())
            .unwrap_or(1);
        EpisodeList {
            kind: detail.media.kind,
            provider_id,
            episodes: detail.episodes.clone(),
            count: detail.media.episodes.unwrap_or(0),
            season,
            current,
            compact,
        }
    }
}

impl Render for EpisodeList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = self.compact;
        let section = div().flex().flex_col().gap_4();
        let section = if self.compact {
            section.px_4().pt_2()
        } else {
            section.px(px(GUTTER)).pt_10()
        };
        let heading = (!self.compact).then(|| section_title("Episodes"));
        let note = |text: &str| {
            div()
                .text_size(px(14.))
                .text_color(color(TEXT_DIM))
                .child(text.to_string())
        };

        match self.kind {
            Kind::Movie => div().into_any_element(),
            Kind::Series => {
                let mut seasons: Vec<u32> = self.episodes.iter().filter_map(|e| e.season).collect();
                seasons.sort_unstable();
                seasons.dedup();
                if seasons.is_empty() {
                    return section
                        .children(heading)
                        .child(note("No episode list is available."))
                        .into_any_element();
                }
                let selected = self.season;
                section
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .children(heading)
                            .child(div().flex().flex_wrap().gap_x_5().children(
                                seasons.into_iter().map(|season| {
                                    tab(
                                        SharedString::from(format!("season-{season}")),
                                        &format!("Season {season}"),
                                        season == selected,
                                        cx.listener(move |list, _, _, cx| {
                                            list.season = season;
                                            cx.notify();
                                        }),
                                    )
                                }),
                            )),
                    )
                    .child(
                        div().flex().flex_col().children(
                            self.episodes
                                .iter()
                                .filter(|episode| episode.season == Some(selected))
                                .map(|episode| {
                                    episode_row(
                                        episode,
                                        self.current.as_deref() == Some(&episode.id),
                                        compact,
                                    )
                                }),
                        ),
                    )
                    .into_any_element()
            }
            Kind::Anime => {
                if self.count == 0 {
                    return section
                        .children(heading)
                        .child(note("No episodes have been released yet."))
                        .into_any_element();
                }
                let current = self
                    .current
                    .as_deref()
                    .and_then(media_id::parse)
                    .and_then(|p| p.episode);
                let provider_id = self.provider_id.clone();
                section
                    .child(section_title("Episodes"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children((1..=self.count).map(move |number| {
                                let id = anime_episode_id(&provider_id, number);
                                let selected = current == Some(number);
                                div()
                                    .id(SharedString::from(format!("episode-{number}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(56.))
                                    .h(px(44.))
                                    .rounded_md()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .when(selected, |tile| {
                                        tile.bg(color(TEXT)).text_color(color(CANVAS))
                                    })
                                    .when(!selected, |tile| {
                                        tile.bg(color(TEXT).opacity(0.08))
                                            .text_color(color(TEXT_DIM))
                                            .hover(|style| {
                                                style
                                                    .bg(color(TEXT).opacity(0.16))
                                                    .text_color(color(TEXT))
                                            })
                                    })
                                    .on_click(move |_, _, cx| open(&id, compact, cx))
                                    .child(number.to_string())
                            })),
                    )
                    .into_any_element()
            }
        }
    }
}

fn episode_row(episode: &Episode, current: bool, compact: bool) -> Stateful<Div> {
    let id = episode.id.clone();
    let group = SharedString::from(format!("episode-row-{}", episode.id));
    div()
        .id(group.clone())
        .group(group.clone())
        .flex()
        .items_center()
        .gap_5()
        .px_4()
        .py_3p5()
        .rounded_lg()
        .cursor_pointer()
        .border_b_1()
        .border_color(color(TEXT).opacity(0.06))
        .when(current, |row| row.bg(color(TEXT).opacity(0.07)))
        .hover(|style| style.bg(color(TEXT).opacity(0.07)))
        .on_click(move |_, _, cx| open(&id, compact, cx))
        .child(
            div()
                .w(px(40.))
                .text_size(px(22.))
                .font_weight(FontWeight::BOLD)
                .text_color(if current {
                    color(ACCENT)
                } else {
                    color(TEXT_FAINT)
                })
                .child(episode.number.to_string()),
        )
        .child(
            div()
                .flex_1()
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
                        .child(
                            episode
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("Episode {}", episode.number)),
                        ),
                )
                .children(episode.airdate.as_ref().map(|date| {
                    div()
                        .text_size(px(12.))
                        .text_color(color(TEXT_DIM))
                        .child(date.chars().take(10).collect::<String>())
                })),
        )
        .child(
            div()
                .invisible()
                .group_hover(group, |style| style.visible())
                .child(Icon::new(IconName::Play).small().text_color(color(TEXT))),
        )
}

/// Plays an episode. From the title page that is a step forward; from the watch page's own
/// panel it swaps the episode, so Back still leaves the show.
fn open(id: &str, replacing: bool, cx: &mut App) {
    let route = Route::Watch { id: id.to_string() };
    if replacing {
        replace(route, cx);
    } else {
        navigate(route, cx);
    }
}
