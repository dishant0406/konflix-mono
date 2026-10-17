//! Browsing one catalogue, or all of them, and the full results of a search.

use crate::design::button::{secondary_button, tab};
use crate::design::state::{empty_state, error_state, skeleton, spaced};
use crate::design::tokens::{
    ACCENT, GUTTER, LINE, TEXT, TEXT_DIM, TEXT_FAINT, TILE_HEIGHT, TILE_WIDTH, TOP_BAR, color,
    font, radius, size,
};
use crate::task::load;
use crate::tile::poster_tile;
use akidev_core::model::{Filters, Kind, Media, Sort};
use akidev_core::providers;
use akidev_core::search::{interleave, search};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const PER_PAGE: u32 = 24;

/// The genres worth offering, in the providers' own spellings. AniList and Cinemeta name
/// their genres differently, and a genre the catalogue lacks would only ever return nothing.
fn genres_for(kind: Option<Kind>) -> &'static [&'static str] {
    match kind {
        Some(Kind::Anime) => &[
            "Action",
            "Adventure",
            "Comedy",
            "Drama",
            "Ecchi",
            "Fantasy",
            "Horror",
            "Mahou Shoujo",
            "Mecha",
            "Music",
            "Mystery",
            "Psychological",
            "Romance",
            "Sci-Fi",
            "Slice of Life",
            "Sports",
            "Supernatural",
            "Thriller",
        ],
        Some(_) => &[
            "Action",
            "Adventure",
            "Animation",
            "Biography",
            "Comedy",
            "Crime",
            "Documentary",
            "Drama",
            "Family",
            "Fantasy",
            "History",
            "Horror",
            "Music",
            "Mystery",
            "Romance",
            "Sci-Fi",
            "Sport",
            "Thriller",
            "War",
        ],
        None => &[
            "Action",
            "Adventure",
            "Comedy",
            "Drama",
            "Fantasy",
            "Horror",
            "Mystery",
            "Romance",
            "Sci-Fi",
            "Thriller",
        ],
    }
}

const SORTS: [(Sort, &str); 5] = [
    (Sort::Popularity, "Popular"),
    (Sort::Trending, "Trending"),
    (Sort::Score, "Top rated"),
    (Sort::Recent, "Newest"),
    (Sort::Title, "A–Z"),
];

enum State {
    Loading,
    Loaded,
    Failed,
}

pub struct BrowsePage {
    /// The catalogue shown. A search covers every catalogue, and `kind` then only narrows
    /// the results already found.
    kind: Option<Kind>,
    term: String,
    genre: Option<&'static str>,
    sort: Sort,
    results: Vec<Media>,
    state: State,
    /// The last page that arrived.
    page: u32,
    has_more: bool,
    /// A further page is on its way, so another is not asked for yet.
    loading_more: bool,
    /// Bumped on every new request, so a slow answer to an old one is ignored.
    request: u32,
    /// The list of genres is unfolded.
    genres_open: bool,
}

impl BrowsePage {
    pub fn new(kind: Option<Kind>, term: String, cx: &mut Context<Self>) -> BrowsePage {
        let mut page = BrowsePage {
            kind,
            term,
            genre: None,
            sort: Sort::Popularity,
            results: Vec::new(),
            state: State::Loading,
            page: 0,
            has_more: false,
            loading_more: false,
            request: 0,
            genres_open: false,
        };
        page.fetch(1, cx);
        page
    }

    fn searching(&self) -> bool {
        !self.term.is_empty()
    }

    /// Browsing asks only the chosen catalogue. A search asks all of them, then narrows.
    fn kinds(&self) -> Vec<Kind> {
        match (self.searching(), self.kind) {
            (false, Some(kind)) => vec![kind],
            _ => Kind::ALL.to_vec(),
        }
    }

    fn fetch(&mut self, page: u32, cx: &mut Context<Self>) {
        self.request += 1;
        if page == 1 {
            self.state = State::Loading;
        }
        self.loading_more = page > 1;
        cx.notify();

        let request = self.request;
        let kinds = self.kinds();
        let term = self.term.clone();
        let filters = Filters {
            sort: Some(self.sort),
            genre: self.genre.map(str::to_string),
            page,
            per_page: PER_PAGE,
            ..Default::default()
        };

        load(
            cx,
            async move {
                if !term.is_empty() {
                    return search(&term, &kinds, 60).await.map(|found| (found, false));
                }
                let pages = futures::future::join_all(kinds.iter().map(|&kind| {
                    let filters = filters.clone();
                    async move { providers::list(kind, &filters).await }
                }))
                .await;
                let answered: Vec<_> = pages.into_iter().filter_map(Result::ok).collect();
                if answered.is_empty() {
                    anyhow::bail!("no catalogue answered");
                }
                let has_next = answered.iter().any(|page| page.has_next);
                let merged = interleave(
                    answered.into_iter().map(|page| page.results).collect(),
                    usize::MAX,
                );
                Ok((merged, has_next))
            },
            move |this, answer, cx| {
                if this.request != request {
                    return;
                }
                this.loading_more = false;
                match answer {
                    // A failed further page leaves what is already on screen alone.
                    Err(_) if page > 1 => crate::route::toast("Couldn't load more titles.", cx),
                    Err(_) => this.state = State::Failed,
                    Ok((found, has_next)) => {
                        this.page = page;
                        if page == 1 {
                            this.results = found;
                        } else {
                            // Providers can repeat a title across pages, and a duplicate tile
                            // is a visible bug, so only titles not already shown are added.
                            for media in found {
                                if !this.results.iter().any(|shown| shown.id == media.id) {
                                    this.results.push(media);
                                }
                            }
                        }
                        this.has_more = has_next;
                        this.state = State::Loaded;
                    }
                }
                cx.notify();
            },
        );
    }

    fn heading(&self) -> String {
        if self.searching() {
            return format!("Results for “{}”", self.term);
        }
        match self.kind {
            Some(Kind::Anime) => "Anime",
            Some(Kind::Series) => "Series",
            Some(Kind::Movie) => "Films",
            None => "Browse",
        }
        .to_string()
    }
}

impl Render for BrowsePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown: Vec<&Media> = self
            .results
            .iter()
            .filter(|media| !self.searching() || self.kind.is_none_or(|kind| media.kind == kind))
            .collect();

        let filters = if self.searching() {
            let kinds = [
                (None, "All"),
                (Some(Kind::Anime), "Anime"),
                (Some(Kind::Series), "Series"),
                (Some(Kind::Movie), "Films"),
            ];
            div().flex().gap_5().children(kinds.map(|(kind, label)| {
                tab(
                    SharedString::from(format!("kind-{label}")),
                    label,
                    self.kind == kind,
                    cx.listener(move |page, _, _, cx| {
                        page.kind = kind;
                        cx.notify();
                    }),
                )
            }))
        } else {
            let sorts = div().flex().gap_5().children(SORTS.map(|(sort, label)| {
                tab(
                    SharedString::from(format!("sort-{label}")),
                    label,
                    self.sort == sort,
                    cx.listener(move |page, _, _, cx| {
                        page.sort = sort;
                        page.fetch(1, cx);
                    }),
                )
            }));
            let genre = div()
                .id("genres")
                .flex()
                .items_center()
                .gap_1p5()
                // Level with the text of the tabs, whose underline sits below it.
                .pb(px(7.))
                .cursor_pointer()
                .text_size(px(14.))
                .on_click(cx.listener(|page, _, _, cx| {
                    page.genres_open = !page.genres_open;
                    cx.notify();
                }))
                .child(
                    div()
                        .text_color(color(TEXT_DIM))
                        .child(if self.genre.is_some() {
                            "Genre"
                        } else {
                            "All genres"
                        }),
                )
                .children(self.genre.map(|genre| {
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .child(genre)
                }))
                .child(
                    Icon::new(if self.genres_open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .small()
                    .text_color(color(TEXT_DIM)),
                );
            let list = div()
                .flex()
                .flex_wrap()
                .gap_x_6()
                .gap_y_2()
                .pt_5()
                .children(genres_for(self.kind).iter().map(|&genre| {
                    let active = self.genre == Some(genre);
                    div()
                        .id(SharedString::from(format!("genre-{genre}")))
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .cursor_pointer()
                        .text_size(px(14.))
                        .text_color(color(if active { TEXT } else { TEXT_DIM }))
                        .when(active, |item| item.font_weight(FontWeight::SEMIBOLD))
                        .hover(|style| style.text_color(color(TEXT)))
                        .on_click(cx.listener(move |page, _, _, cx| {
                            page.genre = if page.genre == Some(genre) {
                                None
                            } else {
                                Some(genre)
                            };
                            page.fetch(1, cx);
                        }))
                        .child(
                            div()
                                .size(px(5.))
                                .rounded_full()
                                .when(active, |dot| dot.bg(color(ACCENT))),
                        )
                        .child(genre)
                }));
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(sorts)
                        .child(genre),
                )
                .when(self.genres_open, |filters| filters.child(list))
        };

        let body =
            match self.state {
                State::Loading => div()
                    .flex()
                    .flex_wrap()
                    .gap_5()
                    .children((0..12).map(|_| {
                        skeleton(px(TILE_WIDTH), px(TILE_HEIGHT)).rounded(px(radius::PANEL))
                    }))
                    .into_any_element(),
                State::Failed => error_state(
                    "Couldn't reach the catalogue",
                    "Check your connection and try again.",
                    cx.listener(|page, _, _, cx| page.fetch(1, cx)),
                )
                .into_any_element(),
                State::Loaded if shown.is_empty() => {
                    let detail = if self.searching() {
                        "Nothing matched. Try another spelling or fewer words."
                    } else {
                        "Nothing matches these filters."
                    };
                    empty_state("No results", detail).into_any_element()
                }
                State::Loaded => {
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x_5()
                        .gap_y_6()
                        .children(shown.iter().enumerate().map(|(index, media)| {
                            poster_tile(&format!("browse-{index}"), media, None)
                        }))
                        .into_any_element()
                }
            };

        let more = (matches!(self.state, State::Loaded) && self.has_more).then(|| {
            let line = || div().h(px(1.)).flex_1().bg(color(LINE));
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .pt_10()
                .pb_6()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .w(px(420.))
                        .child(line())
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(TEXT_FAINT))
                                .child(spaced(&format!("{} shown", shown.len()))),
                        )
                        .child(line()),
                )
                .child(
                    div()
                        .w(px(144.))
                        .flex()
                        .justify_center()
                        .child(secondary_button(
                            "more",
                            if self.loading_more {
                                "Loading…"
                            } else {
                                "Show more"
                            },
                            None,
                            cx.listener(|page, _, _, cx| {
                                if !page.loading_more {
                                    page.fetch(page.page + 1, cx);
                                }
                            }),
                        )),
                )
        });

        div()
            .flex()
            .flex_col()
            .gap_6()
            .px(px(GUTTER))
            .pt(px(TOP_BAR + 28.))
            .pb_16()
            .child(
                div()
                    .text_size(px(size::DISPLAY))
                    .font_family(font::TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT))
                    .child(self.heading()),
            )
            .child(filters)
            .child(body)
            .children(more)
    }
}
