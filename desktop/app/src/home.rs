//! The home screen: a featured banner and rows from every catalogue.

use crate::backend::Backend;
use crate::hero::Hero;
use crate::row::{MediaRow, RowSpec};
use crate::task::load;
use akidev_core::model::{Filters, Kind, Media, Sort};
use akidev_core::providers;
use akidev_core::search::interleave;
use gpui_kit::*;

fn row(title: &'static str, kind: Kind, sort: Sort, genre: Option<&str>) -> RowSpec {
    RowSpec {
        title,
        kind,
        filters: Filters {
            sort: Some(sort),
            genre: genre.map(str::to_string),
            page: 1,
            per_page: 24,
            ..Default::default()
        },
    }
}

/// The rows, in order. Each draws from one catalogue, so the page mixes all three.
fn rows() -> Vec<RowSpec> {
    vec![
        row("Trending Anime", Kind::Anime, Sort::Trending, None),
        row("New Releases", Kind::Movie, Sort::Recent, None),
        row("Popular Series", Kind::Series, Sort::Popularity, None),
        row("Newly Released Anime", Kind::Anime, Sort::Recent, None),
        row("Most Watched Films", Kind::Movie, Sort::Popularity, None),
        row("New Series", Kind::Series, Sort::Recent, None),
        row("Top Rated Anime", Kind::Anime, Sort::Score, None),
        row(
            "Action Films",
            Kind::Movie,
            Sort::Popularity,
            Some("Action"),
        ),
        row("Drama Series", Kind::Series, Sort::Score, Some("Drama")),
        row(
            "Romance Anime",
            Kind::Anime,
            Sort::Popularity,
            Some("Romance"),
        ),
    ]
}

/// The front of one catalogue, for the banner.
async fn featured_from(kind: Kind) -> Vec<Media> {
    let filters = Filters {
        sort: Some(Sort::Trending),
        page: 1,
        per_page: 6,
        ..Default::default()
    };
    providers::list(kind, &filters)
        .await
        .map(|page| page.results)
        .unwrap_or_default()
}

/// Starts asking for what the home screen shows, before there is a window to show it in. The
/// screen then finds the requests already on their way instead of making them itself.
pub fn prefetch() {
    for kind in Kind::ALL {
        crate::runtime::spawn(featured_from(kind));
    }
    for spec in rows() {
        crate::runtime::spawn(async move { providers::list(spec.kind, &spec.filters).await });
    }
}

pub struct HomePage {
    hero: Entity<Hero>,
    rows: Vec<Entity<MediaRow>>,
}

impl HomePage {
    pub fn new(cx: &mut Context<Self>) -> HomePage {
        let hero = cx.new(|_| Hero::new());
        let history = cx.global::<Backend>().progress.recent().to_vec();
        let rows = (!history.is_empty())
            .then(|| cx.new(|_| MediaRow::continuing(&history)))
            .into_iter()
            .chain(
                rows()
                    .into_iter()
                    .map(|spec| cx.new(|cx| MediaRow::loading(spec, cx))),
            )
            .collect();

        // The banner draws from the front of each catalogue, so it features all three.
        let hero_handle = hero.clone();
        load(
            cx,
            async {
                let lists = futures::future::join_all(
                    Kind::ALL.map(|kind| async move { featured_from(kind).await }),
                )
                .await;
                interleave(lists, 9)
                    .into_iter()
                    .filter(|media: &Media| media.banner.is_some())
                    .collect::<Vec<_>>()
            },
            move |_, featured, cx| {
                hero_handle.update(cx, |hero, cx| hero.set_items(featured, cx));
            },
        );
        HomePage { hero, rows }
    }
}

impl Render for HomePage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pb_16()
            .child(self.hero.clone())
            .children(self.rows.iter().cloned())
    }
}
