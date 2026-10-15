//! The episode list shared by the title page and the watch page.

use crate::design::button::tab;
use crate::design::state::section_title;
use crate::design::tokens::{ACCENT, CANVAS, GUTTER, LINE, TEXT, TEXT_DIM, TEXT_FAINT, color};
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
    /// Fits the watch page's side panel: no heading, no page padding, its own scroll area.
    compact: bool,
    scroll: ScrollHandle,
    /// Set once the list has been scrolled to the current episode.
    scrolled: bool,
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
            scroll: ScrollHandle::new(),
            scrolled: false,
        }
    }
}

impl Render for EpisodeList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.compact {
            return self.panel(cx).into_any_element();
        }
        let section = div().flex().flex_col().gap_4().px(px(GUTTER)).pt_10();
        let note = |text: &str| {
            div()
                .text_size(px(14.))
                .text_color(color(TEXT_DIM))
                .child(text.to_string())
        };

        match self.kind {
            Kind::Movie => div().into_any_element(),
            Kind::Series => {
                let seasons = self.seasons();
                if seasons.is_empty() {
                    return section
                        .child(section_title("Episodes"))
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
                            .child(section_title("Episodes"))
                            .child(self.season_tabs(&seasons, cx)),
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
                                        false,
                                    )
                                }),
                        ),
                    )
                    .into_any_element()
            }
            Kind::Anime => {
                if self.count == 0 {
                    return section
                        .child(section_title("Episodes"))
                        .child(note("No episodes have been released yet."))
                        .into_any_element();
                }
                section
                    .child(section_title("Episodes"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children((1..=self.count).map(|number| self.tile(number, false))),
                    )
                    .into_any_element()
            }
        }
    }
}

impl EpisodeList {
    fn seasons(&self) -> Vec<u32> {
        let mut seasons: Vec<u32> = self.episodes.iter().filter_map(|e| e.season).collect();
        seasons.sort_unstable();
        seasons.dedup();
        seasons
    }

    fn season_tabs(&self, seasons: &[u32], cx: &mut Context<Self>) -> Div {
        let selected = self.season;
        div()
            .flex()
            .flex_wrap()
            .gap_x_5()
            .children(seasons.iter().map(|&season| {
                tab(
                    SharedString::from(format!("season-{season}")),
                    &format!("Season {season}"),
                    season == selected,
                    cx.listener(move |list, _, _, cx| {
                        list.season = season;
                        // A season's list starts at its top.
                        list.scroll.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    }),
                )
            }))
    }

    /// One anime episode as a number tile.
    fn tile(&self, number: u32, compact: bool) -> Stateful<Div> {
        let id = anime_episode_id(&self.provider_id, number);
        let selected = self.current_number() == Some(number);
        let (width, height) = if compact { (52., 40.) } else { (56., 44.) };
        div()
            .id(SharedString::from(format!("episode-{number}")))
            .flex()
            .items_center()
            .justify_center()
            .w(px(width))
            .h(px(height))
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
                    .hover(|style| style.bg(color(TEXT).opacity(0.16)).text_color(color(TEXT)))
            })
            .on_click(move |_, _, cx| open(&id, compact, cx))
            .child(number.to_string())
    }

    fn current_number(&self) -> Option<u32> {
        self.current
            .as_deref()
            .and_then(media_id::parse)
            .and_then(|p| p.episode)
    }

    /// The watch page's side panel. The panel has its own header, so this has no heading and
    /// no page padding: season tabs (for a series) over a list that scrolls, opened at the
    /// episode being watched.
    fn panel(&mut self, cx: &mut Context<Self>) -> Div {
        let root = div().flex_1().min_h_0().flex().flex_col();
        let note = |text: &str| {
            div()
                .px_4()
                .py_2()
                .text_size(px(14.))
                .text_color(color(TEXT_DIM))
                .child(text.to_string())
        };
        let scroll = div()
            .id("episodes-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&self.scroll);

        match self.kind {
            Kind::Movie => root,
            Kind::Series => {
                let seasons = self.seasons();
                if seasons.is_empty() {
                    return root.child(note("No episode list is available."));
                }
                let shown: Vec<&Episode> = self
                    .episodes
                    .iter()
                    .filter(|episode| episode.season == Some(self.season))
                    .collect();
                // Once, when the panel opens: the episode being watched, with the one before
                // it showing so the viewer sees where they are in the list.
                if !self.scrolled {
                    self.scrolled = true;
                    if let Some(at) = shown
                        .iter()
                        .position(|e| self.current.as_deref() == Some(&e.id))
                    {
                        self.scroll.scroll_to_top_of_item(at.saturating_sub(1));
                    }
                }
                root.child(
                    div()
                        .px_4()
                        .pb_1()
                        .border_b_1()
                        .border_color(color(LINE))
                        .child(self.season_tabs(&seasons, cx)),
                )
                .child(
                    scroll.child(
                        div()
                            .flex()
                            .flex_col()
                            .p_2()
                            .children(shown.into_iter().map(|episode| {
                                episode_row(
                                    episode,
                                    self.current.as_deref() == Some(&episode.id),
                                    true,
                                )
                            })),
                    ),
                )
            }
            Kind::Anime => {
                if self.count == 0 {
                    return root.child(note("No episodes have been released yet."));
                }
                if !self.scrolled {
                    self.scrolled = true;
                    if let Some(number) = self.current_number() {
                        // The tiles are the scroll area's own children, six to a row.
                        let row_start = (number.saturating_sub(1) / 6 * 6) as usize;
                        self.scroll.scroll_to_top_of_item(row_start);
                    }
                }
                root.child(
                    scroll
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .content_start()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .children((1..=self.count).map(|number| self.tile(number, true))),
                )
            }
        }
    }
}

fn episode_row(episode: &Episode, current: bool, compact: bool) -> Stateful<Div> {
    let id = episode.id.clone();
    let group = SharedString::from(format!("episode-row-{}", episode.id));
    // The panel is 400px wide: a smaller number and tighter row than the title page's.
    let (number_width, number_size) = if compact { (30., 16.) } else { (40., 22.) };
    div()
        .id(group.clone())
        .group(group.clone())
        .flex()
        .items_center()
        .gap(px(if compact { 12. } else { 20. }))
        .px(px(if compact { 8. } else { 16. }))
        .py(px(if compact { 10. } else { 14. }))
        .rounded_lg()
        .cursor_pointer()
        .when(!compact, |row| {
            row.border_b_1().border_color(color(TEXT).opacity(0.06))
        })
        .when(current, |row| row.bg(color(TEXT).opacity(0.07)))
        .hover(|style| style.bg(color(TEXT).opacity(0.07)))
        .on_click(move |_, _, cx| open(&id, compact, cx))
        .child(
            div()
                .w(px(number_width))
                .flex_none()
                .text_size(px(number_size))
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
                        .text_size(px(if compact { 14. } else { 15. }))
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
