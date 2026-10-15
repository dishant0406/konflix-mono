//! A horizontally scrolling row of posters, with arrows at the edges on hover.

use crate::design::button::icon_button;
use crate::design::state::{error_state, section_title, skeleton};
use crate::design::tokens::{CANVAS, GUTTER, TILE_HEIGHT, TILE_WIDTH, color};
use crate::progress::Entry;
use crate::route::Route;
use crate::tile::poster_tile;
use akidev_core::model::{Filters, Kind, Media};
use akidev_core::providers;
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What a row shows, before it has loaded.
#[derive(Clone)]
pub struct RowSpec {
    pub title: &'static str,
    pub kind: Kind,
    pub filters: Filters,
}

enum Items {
    Loading,
    Loaded(Vec<Media>),
    Failed,
}

pub struct MediaRow {
    title: SharedString,
    spec: Option<RowSpec>,
    items: Items,
    /// For a row of titles being watched: how far through each one is, and where it resumes.
    /// Parallel to the items, and empty for every other row.
    resume: Vec<(f32, Route)>,
    scroll: ScrollHandle,
}

impl MediaRow {
    /// A row whose items are already known, e.g. a title's related titles.
    pub fn with_items(title: impl Into<SharedString>, items: Vec<Media>) -> MediaRow {
        MediaRow {
            title: title.into(),
            spec: None,
            items: Items::Loaded(items),
            resume: Vec::new(),
            scroll: ScrollHandle::new(),
        }
    }

    /// The titles the viewer is part way through, each resuming where they left off.
    pub fn continuing(entries: &[Entry]) -> MediaRow {
        let items = entries.iter().map(|entry| entry.media.clone()).collect();
        let mut row = MediaRow::with_items("Continue watching", items);
        row.resume = entries
            .iter()
            .map(|entry| {
                (
                    entry.fraction(),
                    Route::Watch {
                        id: entry.episode_id.clone(),
                    },
                )
            })
            .collect();
        row
    }

    /// A row that fetches its items from the provider chain.
    pub fn loading(spec: RowSpec, cx: &mut Context<Self>) -> MediaRow {
        let mut row = MediaRow {
            title: spec.title.into(),
            spec: Some(spec),
            items: Items::Loading,
            resume: Vec::new(),
            scroll: ScrollHandle::new(),
        };
        row.fetch(cx);
        row
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        let Some(RowSpec { kind, filters, .. }) = self.spec.clone() else {
            return;
        };
        self.items = Items::Loading;
        cx.notify();
        crate::task::load(
            cx,
            async move { providers::list(kind, &filters).await },
            |row, page, cx| {
                row.items = match page {
                    Ok(page) => Items::Loaded(page.results),
                    Err(_) => Items::Failed,
                };
                cx.notify();
            },
        );
    }

    fn scroll_by(&self, direction: f32) {
        let bounds = self.scroll.bounds();
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset();
        let step = bounds.size.width * 0.85 * direction;
        let x = (offset.x - step).clamp(-max.x, px(0.));
        self.scroll.set_offset(point(x, offset.y));
    }

    fn edge(
        &self,
        id: &'static str,
        icon: IconName,
        left: bool,
        group: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let (from, to) = if left { (0., 1.) } else { (1., 0.) };
        let scrolls = if left { -1. } else { 1. };
        div()
            .absolute()
            .top_0()
            .h(px(TILE_HEIGHT))
            .w(px(64.))
            .flex()
            .items_center()
            .justify_center()
            .when_else(left, |edge| edge.left_0(), |edge| edge.right_0())
            .bg(linear_gradient(
                90.,
                linear_color_stop(color(CANVAS).opacity(0.92 * from), 0.),
                linear_color_stop(color(CANVAS).opacity(0.92 * to), 1.),
            ))
            .invisible()
            .group_hover(group, |style| style.visible())
            .child(icon_button(
                id,
                icon,
                cx.listener(move |row, _, _, cx| {
                    row.scroll_by(scrolls);
                    cx.notify();
                }),
            ))
    }
}

impl Render for MediaRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An empty row is hidden once loaded: an outage must not leave a bare heading.
        if matches!(&self.items, Items::Loaded(items) if items.is_empty()) {
            return div().into_any_element();
        }

        let heading = div()
            .px(px(GUTTER))
            .child(section_title(self.title.clone()));
        let body = match &self.items {
            Items::Failed => div()
                .px(px(GUTTER))
                .child(error_state(
                    "Couldn't load this row",
                    "The catalogue didn't answer. Check your connection.",
                    cx.listener(|row, _, _, cx| row.fetch(cx)),
                ))
                .into_any_element(),
            Items::Loading => div()
                .flex()
                .gap_3()
                .px(px(GUTTER))
                .children((0..9).map(|_| skeleton(px(TILE_WIDTH), px(TILE_HEIGHT)).rounded_lg()))
                .overflow_hidden()
                .into_any_element(),
            Items::Loaded(items) => {
                let group = SharedString::from(format!("row-{}", self.title));
                let offset = self.scroll.offset().x;
                let max = self.scroll.max_offset().x;
                let track = div()
                    .id(SharedString::from(format!("track-{}", self.title)))
                    .flex()
                    .gap_3()
                    .px(px(GUTTER))
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.scroll)
                    .children(items.iter().enumerate().map(|(index, media)| {
                        poster_tile(
                            &format!("{}-{index}", self.title),
                            media,
                            self.resume.get(index).cloned(),
                        )
                    }));
                div()
                    .relative()
                    .group(group.clone())
                    .child(track)
                    .when(offset < px(-4.), |row| {
                        row.child(self.edge("left", IconName::ChevronLeft, true, group.clone(), cx))
                    })
                    .when(max > px(0.) && offset > -max + px(4.), |row| {
                        row.child(self.edge(
                            "right",
                            IconName::ChevronRight,
                            false,
                            group.clone(),
                            cx,
                        ))
                    })
                    .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .pt_6()
            .child(heading)
            .child(body)
            .into_any_element()
    }
}
