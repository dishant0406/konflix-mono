//! A horizontally scrolling row of posters, with arrows at the edges on hover.

use crate::design::button::icon_button;
use crate::design::state::{error_state, section_title, skeleton};
use crate::design::tokens::{CANVAS, GUTTER, TILE_HEIGHT, TILE_WIDTH, color, radius};
use crate::progress::Entry;
use crate::route::Route;
use crate::tile::poster_tile;
use akidev_core::model::{Filters, Kind, Media};
use akidev_core::providers;
use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// The space between two tiles of a row.
const TILE_GAP: f32 = 12.;
/// How many tiles past the edge of the row are built, so one is ready before it scrolls in.
const OVERSCAN: i64 = 2;

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
    /// Where the row was last drawn, which says whether it is anywhere near the screen.
    drawn: Rc<Cell<Bounds<Pixels>>>,
    /// How tall the row is when it is built in full, so a row that is not stands in at that height.
    full_height: Option<Pixels>,
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
            drawn: Rc::new(Cell::new(Bounds::default())),
            full_height: None,
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
            drawn: Rc::new(Cell::new(Bounds::default())),
            full_height: None,
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

/// Which tiles of a row of `count` to build, as `first..last`: the ones in view and a couple
/// either side. `scrolled` is how far the row is scrolled, `viewport` how wide it shows.
fn tiles_to_build(scrolled: f32, viewport: f32, count: usize) -> (usize, usize) {
    let stride = TILE_WIDTH + TILE_GAP;
    let first = (((scrolled - GUTTER) / stride).floor() as i64 - OVERSCAN).max(0) as usize;
    let last = ((((scrolled + viewport - GUTTER) / stride).ceil() as i64) + OVERSCAN)
        .clamp(0, count as i64) as usize;
    (first.min(last), last)
}

/// Whether a row that was last drawn from `top` down for `height` is near enough to a screen
/// `screen` tall to be built: on it, or within about a screen below it or half a screen above.
/// A row not drawn yet has no height, and is built to find out how tall it is.
fn near_the_screen(top: f32, height: f32, screen: f32) -> bool {
    height <= 0. || (top < screen * 1.4 && top + height > -screen * 0.4)
}

impl Render for MediaRow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An empty row is hidden once loaded: an outage must not leave a bare heading.
        if matches!(&self.items, Items::Loaded(items) if items.is_empty()) {
            return div().into_any_element();
        }

        // A row well off the screen is not built: it stands in as a blank of the same height, so
        // the page does not move and nothing in it is drawn or loaded until it is near.
        let drawn = self.drawn.get();
        let screen = window.viewport_size().height;
        let near = near_the_screen(
            f32::from(drawn.origin.y),
            f32::from(drawn.size.height),
            f32::from(screen),
        );
        if let (false, Some(height)) = (near, self.full_height) {
            let drawn = self.drawn.clone();
            return div()
                .relative()
                .w_full()
                .h(height)
                .child(
                    canvas(move |bounds, _, _| drawn.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .into_any_element();
        }
        if drawn.size.height > px(0.) {
            self.full_height = Some(drawn.size.height);
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
            Items::Loading => {
                div()
                    .flex()
                    .gap(px(TILE_GAP))
                    .px(px(GUTTER))
                    .children((0..9).map(|_| {
                        skeleton(px(TILE_WIDTH), px(TILE_HEIGHT)).rounded(px(radius::PANEL))
                    }))
                    .overflow_hidden()
                    .into_any_element()
            }
            Items::Loaded(items) => {
                let group = SharedString::from(format!("row-{}", self.title));
                let offset = self.scroll.offset().x;
                let max = self.scroll.max_offset().x;
                // Only the tiles in view, and a couple either side, are built: a row is 24
                // tiles wide, and drawing every one of them on every frame is what made
                // scrolling heavy. Spacers stand in for the rest, so the row keeps its width.
                let stride = TILE_WIDTH + TILE_GAP;
                let viewport = match self.scroll.bounds().size.width {
                    width if width > px(0.) => width,
                    _ => window.viewport_size().width,
                };
                let (first, last) =
                    tiles_to_build(f32::from(-offset).max(0.), f32::from(viewport), items.len());
                let spacer = |tiles: usize| {
                    (tiles > 0).then(|| div().flex_none().w(px(tiles as f32 * stride - TILE_GAP)))
                };
                let track = div()
                    .id(SharedString::from(format!("track-{}", self.title)))
                    .flex()
                    .gap(px(TILE_GAP))
                    .px(px(GUTTER))
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.scroll)
                    .children(spacer(first))
                    .children(items.iter().enumerate().skip(first).take(last - first).map(
                        |(index, media)| {
                            poster_tile(
                                &format!("{}-{index}", self.title),
                                media,
                                self.resume.get(index).cloned(),
                            )
                        },
                    ))
                    .children(spacer(items.len() - last));
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

        let drawn = self.drawn.clone();
        div()
            .relative()
            .flex()
            .flex_col()
            .gap_3()
            .pt_6()
            .child(
                canvas(move |bounds, _, _| drawn.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(heading)
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that would bring in gpui's own `test` attribute in place of the standard one.
    use super::{TILE_GAP, TILE_WIDTH, near_the_screen, tiles_to_build};

    #[test]
    fn a_row_at_rest_builds_the_tiles_in_view_and_a_couple_more() {
        // A 1360 pixel row shows about seven 176 pixel tiles after the margin.
        assert_eq!(tiles_to_build(0., 1360., 24), (0, 10));
    }

    #[test]
    fn the_tiles_follow_the_scroll() {
        let (first, last) = tiles_to_build(2000., 1360., 24);
        assert_eq!((first, last), (8, 20));
        // Scrolled to the end, the range stops at the last tile and is never empty-ended.
        let end = 24. * (TILE_WIDTH + TILE_GAP);
        let (first, last) = tiles_to_build(end, 1360., 24);
        assert_eq!(last, 24);
        assert!(first <= last);
    }

    #[test]
    fn a_short_or_empty_row_builds_what_it_has() {
        assert_eq!(tiles_to_build(0., 1360., 0), (0, 0));
        assert_eq!(tiles_to_build(0., 1360., 3), (0, 3));
        // A row scrolled further than it is long builds nothing, rather than a negative range.
        let (first, last) = tiles_to_build(99999., 1360., 3);
        assert!(first <= last && last <= 3);
    }

    #[test]
    fn only_rows_near_the_screen_are_built() {
        let screen = 860.;
        assert!(near_the_screen(0., 360., screen));
        assert!(near_the_screen(1000., 360., screen), "just below the fold");
        assert!(!near_the_screen(1300., 360., screen), "a screen away");
        assert!(near_the_screen(-300., 360., screen), "just scrolled past");
        assert!(!near_the_screen(-2000., 360., screen), "long gone");
        assert!(
            near_the_screen(5000., 0., screen),
            "not drawn yet: built to be measured"
        );
    }
}
