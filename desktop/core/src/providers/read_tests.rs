//! What each provider definition in `sources.json` reads out of the answers its service gives.
//!
//! The answers are the shapes the real services send (checked against them when the definitions
//! were written), so a change to a definition that breaks how a title is read fails here.

use super::card;
use super::detail::read_for_test;
use super::fetch::Asking;
use super::list::{shape_for_test, shape_search_for_test};
use crate::calendar::parse_date_ms;
use crate::definition::ProviderDef;
use crate::expression::Row;
use crate::model::{Filters, Kind, Media, Sort, Trailer};
use crate::sources;
use serde_json::{Value, json};

fn def(id: &str) -> ProviderDef {
    sources::current()
        .provider(id)
        .expect("the provider is defined")
        .clone()
}

fn media(id: &str, kind: Kind, value: &Value) -> Option<Media> {
    card::media(
        &def(id).card,
        Row {
            value,
            parent: None,
            kind,
        },
    )
}

fn filters(sort: Option<Sort>, page: u32, per_page: u32) -> Filters {
    Filters {
        sort,
        page,
        per_page,
        ..Default::default()
    }
}

fn day(text: &str) -> i64 {
    parse_date_ms(text).unwrap()
}

// ---------------------------------------------------------------------------------------------
// AniList
// ---------------------------------------------------------------------------------------------

fn bocchi() -> Value {
    json!({
        "id": 130003,
        "title": { "romaji": "Bocchi the Rock!", "english": null, "native": "ぼっち・ざ・ろっく！" },
        "coverImage": { "extraLarge": "https://img/xl.jpg", "large": "https://img/l.jpg" },
        "bannerImage": "https://img/banner.jpg",
        "format": "TV", "seasonYear": null, "startDate": { "year": 2022 },
        "episodes": 12, "duration": 24, "status": "FINISHED",
        "averageScore": 85, "meanScore": 84, "popularity": 250000,
        "genres": ["Comedy", "Music"], "isAdult": false,
        "studios": { "nodes": [{ "name": "CloverWorks" }] }
    })
}

#[test]
fn an_anilist_card_prefers_english_then_romaji_then_native() {
    let card = media("anilist", Kind::Anime, &bocchi()).unwrap();
    assert_eq!(card.id, "anime-130003");
    assert_eq!(card.title, "Bocchi the Rock!");
    assert_eq!(
        card.title_variants,
        vec!["Bocchi the Rock!", "ぼっち・ざ・ろっく！"]
    );
    assert_eq!(card.poster.as_deref(), Some("https://img/xl.jpg"));
    assert_eq!(
        (card.year, card.score, card.episodes),
        (Some(2022), Some(85), Some(12))
    );
    assert_eq!(card.runtime_minutes, Some(24));
    assert_eq!(card.genres, vec!["Comedy", "Music"]);
    assert!(!card.is_adult);

    let title = |value: Value| media("anilist", Kind::Anime, &value).unwrap().title;
    assert_eq!(
        title(json!({ "id": 1, "title": { "english": "E", "romaji": "R", "native": "N" } })),
        "E"
    );
    assert_eq!(
        title(json!({ "id": 1, "title": { "romaji": "", "native": "N" } })),
        "N"
    );
    assert_eq!(title(json!({ "id": 1, "title": null })), "Untitled");
    assert!(media("anilist", Kind::Anime, &json!({ "title": {} })).is_none());
}

#[test]
fn an_airing_show_counts_the_episodes_already_aired() {
    let episodes = |value: Value| media("anilist", Kind::Anime, &value).unwrap().episodes;
    assert_eq!(
        episodes(json!({ "id": 1, "episodes": null, "nextAiringEpisode": { "episode": 6 } })),
        Some(5)
    );
    assert_eq!(
        episodes(json!({ "id": 1, "episodes": null, "nextAiringEpisode": { "episode": 1 } })),
        None
    );
    assert_eq!(episodes(json!({ "id": 1 })), None);
    assert_eq!(episodes(json!({ "id": 1, "episodes": 0 })), None);
}

#[test]
fn an_anilist_page_drops_rows_without_a_cover_and_knows_if_there_is_more() {
    let mut bare = bocchi();
    bare["coverImage"] = json!({ "extraLarge": null, "large": null });
    let body = json!({ "data": { "Page": { "pageInfo": { "hasNextPage": true }, "media": [bocchi(), bare, null] } } });
    let page = shape_for_test(
        &def("anilist"),
        Kind::Anime,
        &filters(None, 1, 24),
        &[body],
        0,
    );
    assert_eq!(page.results.len(), 1);
    assert!(page.has_next);
}

#[test]
fn anilist_is_asked_only_for_the_filters_that_are_set() {
    let provider = def("anilist");
    let list = provider.list.as_ref().unwrap();
    let variables = |sort: Option<Sort>, genre: Option<&str>, year: Option<i32>| {
        let mut asking = Asking::new(&provider, Kind::Anime);
        asking.sort = sort;
        asking.per_page = 24;
        asking.genre = genre;
        asking.year = year;
        let request = asking.prepare(&list.request).unwrap();
        let body = serde_json::to_value(request_json(&request)).unwrap();
        body["variables"].clone()
    };
    // AniList treats a null filter as "match nothing", so an unset one is not sent at all.
    let plain = variables(None, None, None);
    assert_eq!(
        plain,
        json!({ "page": 1, "perPage": 24, "sort": "POPULARITY_DESC" })
    );

    let filtered = variables(Some(Sort::Score), Some("Comedy"), Some(2022));
    assert_eq!(
        filtered,
        json!({ "page": 1, "perPage": 24, "sort": "SCORE_DESC", "genre": "Comedy", "year": 2022 })
    );

    // A date sort leaves out what is not out yet and what has no date. The date is a number.
    let recent = variables(Some(Sort::Recent), None, None);
    assert_eq!(recent["sort"], "START_DATE_DESC");
    assert_eq!(recent["statusNot"], "NOT_YET_RELEASED");
    assert_eq!(recent["startDateGreater"], 19000101);
}

fn request_json(request: &super::fetch::Prepared) -> Value {
    request.json_for_test()
}

#[test]
fn an_anilist_detail_carries_cast_studios_trailer_and_related_anime() {
    let mut title = bocchi();
    title["description"] = json!("Line one<br>Line two &amp; <i>more</i>");
    title["trailer"] = json!({ "id": "abc", "site": "youtube" });
    title["characters"] = json!({ "edges": [
        { "role": "MAIN", "node": { "name": { "full": "Hitori Gotoh" }, "image": { "large": "https://img/h.jpg" } } }
    ] });
    title["relations"] = json!({ "edges": [
        { "relationType": "ADAPTATION", "node": { "id": 99, "type": "MANGA", "title": { "romaji": "Manga" } } },
        { "relationType": "SEQUEL", "node": { "id": 100, "type": "ANIME", "title": { "romaji": "Season 2" } } }
    ] });
    title["recommendations"] = json!({ "nodes": [
        { "mediaRecommendation": { "id": 101, "title": { "romaji": "K-On!" }, "meanScore": 80 } },
        { "mediaRecommendation": null }
    ] });
    let answer = json!({ "data": { "Media": title } });
    let detail = read_for_test(&def("anilist"), Kind::Anime, &answer, [None, None]).unwrap();
    assert_eq!(
        detail.media.description.as_deref(),
        Some("Line one\nLine two & more")
    );
    assert_eq!(detail.studios, vec!["CloverWorks"]);
    assert_eq!(
        detail.trailer,
        Some(Trailer {
            site: "youtube".into(),
            id: "abc".into()
        })
    );
    assert_eq!(detail.cast[0].name, "Hitori Gotoh");
    assert_eq!(detail.cast[0].role.as_deref(), Some("MAIN"));
    assert_eq!(detail.cast[0].image.as_deref(), Some("https://img/h.jpg"));
    assert_eq!(detail.related.len(), 1);
    assert_eq!(detail.related[0].id, "anime-100");
    assert_eq!(detail.recommended.len(), 1);
    assert_eq!(detail.recommended[0].score, Some(80));
    assert!(detail.episodes.is_empty());
    assert_eq!(detail.imdb_id, None);
    // A count is all AniList gives for the episodes, and it stays.
    assert_eq!(detail.media.episodes, Some(12));
    assert!(
        read_for_test(
            &def("anilist"),
            Kind::Anime,
            &json!({ "data": { "Media": null } }),
            [None, None]
        )
        .is_none()
    );
}

// ---------------------------------------------------------------------------------------------
// Cinemeta (a Stremio catalogue)
// ---------------------------------------------------------------------------------------------

fn dhamaal() -> Value {
    json!({
        "id": "tt27548557", "imdb_id": "tt27548557", "type": "movie", "name": "Dhamaal 4",
        "poster": "https://live.metahub.space/poster/small/tt27548557/img",
        "background": "https://live.metahub.space/background/medium/tt27548557/img",
        "releaseInfo": "2026", "released": "2026-06-06T00:00:00.000Z",
        "runtime": "143 min", "imdbRating": "6.7", "genres": ["Comedy"],
        "cast": ["Ajay Devgn"], "popularity": 8.65
    })
}

fn breaking_bad_meta() -> Value {
    json!({
        "id": "tt0903747", "imdb_id": "tt0903747", "type": "series", "name": "Breaking Bad",
        "poster": "https://images.metahub.space/poster/small/tt0903747/img",
        "background": "https://images.metahub.space/background/medium/tt0903747/img",
        "releaseInfo": "2008\u{2013}2013", "released": "2008-01-20T00:00:00.000Z",
        "status": "Ended", "runtime": "49 min", "imdbRating": "9.5",
        "genres": ["Crime", "Drama"],
        "trailers": [{"source": "VFkjBy2b50Q", "type": "Trailer"}],
        "videos": [
            {"id": "tt0903747:0:1", "title": "Good Cop / Bad Cop", "season": 0, "number": 1},
            {"id": "tt0903747:1:1", "title": "Pilot", "season": 1, "number": 1, "released": "2008-01-21T05:00:00.000Z"},
            {"id": "tt0903747:1:2", "title": "Cat in the Bag", "season": 1, "number": 2, "released": "2008-01-27T05:00:00.000Z"},
            {"id": "tt0903747:5:16", "title": "Felina", "season": 5, "number": 16, "released": "2013-09-29T05:00:00.000Z"}
        ]
    })
}

#[test]
fn a_film_catalog_entry_maps_onto_the_card_shape() {
    let card = media("cinemeta", Kind::Movie, &dhamaal()).unwrap();
    assert_eq!(card.id, "movie-tt27548557");
    assert_eq!(card.title, "Dhamaal 4");
    assert_eq!(card.title_variants, vec!["Dhamaal 4"]);
    assert_eq!(card.year, Some(2026));
    assert_eq!(card.runtime_minutes, Some(143));
    assert_eq!(card.score, Some(67));
    assert_eq!(card.popularity, 865);
    assert_eq!(card.genres, vec!["Comedy"]);
    assert!(card.banner.unwrap().contains("background"));
    assert!(card.poster.unwrap().contains("poster"));
    assert_eq!(card.episodes, None);
    assert_eq!(card.format.as_deref(), Some("MOVIE"));
    assert_eq!(card.status, None);
}

#[test]
fn a_series_maps_with_its_status() {
    let card = media("cinemeta", Kind::Series, &breaking_bad_meta()).unwrap();
    assert_eq!(card.id, "series-tt0903747");
    assert_eq!(card.format.as_deref(), Some("TV"));
    assert_eq!(card.status.as_deref(), Some("FINISHED"));
    assert_eq!(card.year, Some(2008));
    let status = |text: &str| {
        let mut meta = breaking_bad_meta();
        meta["status"] = json!(text);
        media("cinemeta", Kind::Series, &meta).unwrap().status
    };
    assert_eq!(status("Canceled").as_deref(), Some("CANCELLED"));
    assert_eq!(status("Continuing").as_deref(), Some("RELEASING"));
    assert_eq!(status("In Development"), None);
}

#[test]
fn an_entry_with_no_id_is_dropped_rather_than_rendered_as_a_dead_card() {
    assert!(media("cinemeta", Kind::Movie, &json!({ "name": "Ghost" })).is_none());
    assert!(
        media(
            "cinemeta",
            Kind::Movie,
            &json!({ "imdb_id": "tt1", "name": "Real" })
        )
        .is_some()
    );
    // The other id is used when one is missing.
    assert_eq!(
        media("cinemeta", Kind::Movie, &json!({ "id": "tt2" }))
            .unwrap()
            .id,
        "movie-tt2"
    );
}

#[test]
fn a_film_detail_carries_the_cast_trailer_and_imdb_id() {
    let mut meta = dhamaal();
    meta["description"] = json!("The Dhamaal boys are back for another treasure hunt.");
    meta["trailers"] = json!([{"source": "Mf8Yiy3FL24", "type": "Trailer"}]);
    meta["credits_cast"] = json!([
        {"character": "Guddu", "name": "Ajay Devgn", "profile_path": "/vnHQQFzTjJ0.jpg", "id": 42803},
        {"character": "Adi", "name": "Arshad Warsi", "profile_path": null, "id": 85889}
    ]);
    let film = read_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &json!({ "meta": meta }),
        [None, None],
    )
    .unwrap();
    assert_eq!(
        film.media.description.as_deref(),
        Some("The Dhamaal boys are back for another treasure hunt.")
    );
    assert_eq!(
        film.trailer,
        Some(Trailer {
            site: "YouTube".into(),
            id: "Mf8Yiy3FL24".into()
        })
    );
    assert_eq!(film.cast.len(), 2);
    assert_eq!(film.cast[0].role.as_deref(), Some("Guddu"));
    assert_eq!(
        film.cast[0].image.as_deref(),
        Some("https://image.tmdb.org/t/p/w185/vnHQQFzTjJ0.jpg")
    );
    assert_eq!(film.cast[1].image, None);
    assert_eq!(film.imdb_id.as_deref(), Some("tt27548557"));
    assert!(film.studios.is_empty() && film.related.is_empty() && film.recommended.is_empty());
}

#[test]
fn a_series_detail_lists_its_episodes_without_the_specials() {
    let answer = json!({ "meta": breaking_bad_meta() });
    let show = read_for_test(&def("cinemeta"), Kind::Series, &answer, [None, None]).unwrap();
    // Season 0 is specials: counting them reported 71 episodes for a 62-episode show.
    assert_eq!(show.episodes.len(), 3);
    assert!(
        show.episodes
            .iter()
            .all(|episode| episode.season >= Some(1))
    );
    assert_eq!(show.episodes[0].id, "series-tt0903747-s1e1");
    assert_eq!(show.episodes[2].id, "series-tt0903747-s5e16");
    assert_eq!(show.episodes[0].name.as_deref(), Some("Pilot"));
    assert_eq!(
        show.episodes[0].airdate.as_deref(),
        Some("2008-01-21T05:00:00.000Z")
    );
    assert_eq!(show.media.episodes, Some(3));
    assert_eq!(show.imdb_id.as_deref(), Some("tt0903747"));
    // An episode with no season or number cannot be addressed.
    let mut meta = breaking_bad_meta();
    meta["videos"] = json!([{ "season": 1 }, { "number": 1 }]);
    let bare = read_for_test(
        &def("cinemeta"),
        Kind::Series,
        &json!({ "meta": meta }),
        [None, None],
    )
    .unwrap();
    assert!(bare.episodes.is_empty());
    assert_eq!(bare.media.episodes, None);
}

#[test]
fn a_cast_list_with_names_only_still_produces_credits() {
    let mut meta = dhamaal();
    meta["cast"] = json!(["Ajay Devgn", "Arshad Warsi"]);
    let film = read_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &json!({ "meta": meta }),
        [None, None],
    )
    .unwrap();
    assert_eq!(film.cast.len(), 2);
    assert_eq!(film.cast[0].role, None);
    assert_eq!(film.cast[0].name, "Ajay Devgn");
}

#[test]
fn the_extra_is_one_segment_and_a_search_is_encoded() {
    let provider = def("cinemeta");
    let list = provider.list.as_ref().unwrap();
    let url = |kind, sort, skip| {
        let mut asking = Asking::new(&provider, kind);
        asking.sort = sort;
        asking.skip = skip;
        asking.prepare(&list.request).unwrap().url_for_test()
    };
    assert_eq!(
        url(Kind::Series, None, 0),
        "https://v3-cinemeta.strem.io/catalog/series/top.json"
    );
    assert_eq!(
        url(Kind::Movie, Some(Sort::Recent), 48),
        "https://v3-cinemeta.strem.io/catalog/movie/year/skip=48.json"
    );
    assert_eq!(
        url(Kind::Movie, Some(Sort::Score), 0),
        "https://v3-cinemeta.strem.io/catalog/movie/imdbRating.json"
    );

    let search = provider.search.as_ref().unwrap();
    let mut asking = Asking::new(&provider, Kind::Movie);
    asking.term = Some("dil se");
    assert_eq!(
        asking.prepare(&search.request).unwrap().url_for_test(),
        "https://v3-cinemeta.strem.io/catalog/movie/top/search=dil%20se.json"
    );
}

fn metas(entries: &[Value]) -> Value {
    json!({ "metas": entries })
}

fn entry(id: &str, released: &str, genres: &[&str]) -> Value {
    json!({ "imdb_id": id, "name": id, "released": released, "releaseInfo": &released[..4], "genres": genres })
}

#[test]
fn an_announced_title_is_kept_out_of_every_row() {
    let now = day("2026-10-13");
    let body = metas(&[
        entry("tt1", "2026-06-06T00:00:00.000Z", &[]),
        entry("tt2", "2026-12-02T00:00:00.000Z", &[]),
        json!({ "imdb_id": "tt3", "name": "year only", "releaseInfo": "2027" }),
        json!({ "imdb_id": "tt4", "name": "year only", "releaseInfo": "2025" }),
        json!({ "imdb_id": "tt5", "name": "no date at all" }),
    ]);
    let page = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &filters(None, 1, 24),
        &[body],
        now,
    );
    let ids: Vec<&str> = page.results.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["movie-tt1", "movie-tt4", "movie-tt5"]);
}

#[test]
fn whether_the_catalogue_goes_on_is_decided_before_anything_is_left_out() {
    // A full page holding a title that is not out yet still has a next page.
    let mut entries: Vec<Value> = (0..47)
        .map(|n| entry(&format!("tt{n}"), "2020-01-01T00:00:00.000Z", &[]))
        .collect();
    entries.push(entry("tt99", "2030-01-01T00:00:00.000Z", &[]));
    let page = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &filters(None, 1, 12),
        &[metas(&entries)],
        day("2026-10-13"),
    );
    assert!(page.has_next);
    assert_eq!(page.results.len(), 12);
    let short = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &filters(None, 1, 12),
        &[metas(&entries[..10])],
        day("2026-10-13"),
    );
    assert!(!short.has_next);
}

#[test]
fn a_genre_or_the_date_sort_reads_a_window_and_pages_within_it() {
    let now = day("2026-10-13");
    let first = metas(&[
        entry("tt1", "2020-01-01T00:00:00.000Z", &["Comedy"]),
        entry("tt2", "2024-01-01T00:00:00.000Z", &["Drama"]),
        entry("tt3", "2025-01-01T00:00:00.000Z", &["comedy", "Drama"]),
    ]);
    // The same title on two scanned pages is one.
    let second = metas(&[
        entry("tt3", "2025-01-01T00:00:00.000Z", &["Comedy"]),
        entry("tt4", "2023-01-01T00:00:00.000Z", &["Comedy"]),
    ]);
    let comedy = Filters {
        genre: Some("Comedy".into()),
        page: 1,
        per_page: 2,
        ..Default::default()
    };
    let page = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &comedy,
        &[first.clone(), second.clone()],
        now,
    );
    let ids: Vec<&str> = page.results.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["movie-tt1", "movie-tt3"]);
    assert!(page.has_next);
    let second_page = Filters {
        page: 2,
        ..comedy.clone()
    };
    let page = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &second_page,
        &[first.clone(), second.clone()],
        now,
    );
    assert_eq!(
        page.results
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["movie-tt4"]
    );
    assert!(!page.has_next);

    // Newest first, whatever order the catalogue had.
    let recent = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &filters(Some(Sort::Recent), 1, 24),
        &[first, second],
        now,
    );
    assert_eq!(
        recent
            .results
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["movie-tt3", "movie-tt2", "movie-tt4", "movie-tt1"]
    );

    let year = Filters {
        year: Some(2024),
        page: 1,
        per_page: 24,
        ..Default::default()
    };
    let page = shape_for_test(
        &def("cinemeta"),
        Kind::Movie,
        &year,
        &[metas(&[
            entry("tt2", "2024-01-01T00:00:00.000Z", &[]),
            entry("tt1", "2020-01-01T00:00:00.000Z", &[]),
        ])],
        now,
    );
    assert_eq!(page.results.len(), 1);
}

#[test]
fn a_cinemeta_search_has_no_next_page_and_keeps_what_it_is_given() {
    let body = metas(&[dhamaal(), json!({ "name": "no id" })]);
    let page = shape_search_for_test(&def("cinemeta"), Kind::Movie, &body);
    assert_eq!(page.results.len(), 1);
    assert!(!page.has_next);
}

// ---------------------------------------------------------------------------------------------
// TVMaze
// ---------------------------------------------------------------------------------------------

fn tvmaze_show() -> Value {
    json!({
        "id": 169, "name": "Breaking Bad", "type": "Scripted", "status": "Ended",
        "premiered": "2008-01-20", "averageRuntime": 60, "weight": 98,
        "rating": {"average": 9.2}, "genres": ["Drama", "Crime"],
        "image": {"medium": "https://m/169.jpg", "original": "https://o/169.jpg"},
        "network": {"id": 20, "name": "AMC", "country": {"code": "US"}},
        "externals": {"imdb": "tt0903747"},
        "summary": "<p>A teacher &amp; <b>cook</b>.<br>Second line.</p>",
        "_embedded": {
            "episodes": [
                {"season": 1, "number": 5, "name": "Gray Matter", "airdate": "2008-02-10"},
                {"season": 1, "number": null, "name": "Special"}
            ],
            "cast": [{"person": {"id": 1, "name": "Bryan Cranston", "image": {"medium": "https://p/1.jpg"}},
                      "character": {"name": "Walter White"}}]
        }
    })
}

#[test]
fn a_tvmaze_show_maps_onto_the_card_shape() {
    let card = media("tvmaze", Kind::Series, &tvmaze_show()).unwrap();
    assert_eq!(card.id, "series-169");
    assert_eq!(card.title, "Breaking Bad");
    assert_eq!(card.poster.as_deref(), Some("https://o/169.jpg"));
    assert_eq!(card.banner, None);
    assert_eq!(card.format.as_deref(), Some("Scripted"));
    assert_eq!(card.year, Some(2008));
    assert_eq!(card.runtime_minutes, Some(60));
    assert_eq!(card.status.as_deref(), Some("Ended"));
    assert_eq!(card.score, Some(92));
    assert_eq!(card.popularity, 98);
    assert_eq!(card.genres, vec!["Drama", "Crime"]);
    let plain = media("tvmaze", Kind::Series, &json!({ "id": 1, "name": "X" })).unwrap();
    assert_eq!(plain.format.as_deref(), Some("TV"));
}

#[test]
fn a_tvmaze_detail_carries_episodes_cast_and_the_imdb_id() {
    let detail = read_for_test(&def("tvmaze"), Kind::Series, &tvmaze_show(), [None, None]).unwrap();
    assert_eq!(detail.imdb_id.as_deref(), Some("tt0903747"));
    assert_eq!(detail.studios, vec!["AMC"]);
    // The episode with no number cannot be addressed, so it is dropped.
    assert_eq!(detail.episodes.len(), 1);
    assert_eq!(detail.episodes[0].id, "series-169-s1e5");
    assert_eq!(detail.media.episodes, Some(1));
    assert_eq!(detail.cast[0].name, "Bryan Cranston");
    assert_eq!(detail.cast[0].role.as_deref(), Some("Walter White"));
    assert_eq!(detail.cast[0].image.as_deref(), Some("https://p/1.jpg"));
    assert_eq!(
        detail.media.description.as_deref(),
        Some("A teacher & cook.\nSecond line.")
    );
}

#[test]
fn only_a_well_formed_imdb_id_is_kept() {
    let imdb = |value: Value| {
        let mut show = tvmaze_show();
        show["externals"] = value;
        read_for_test(&def("tvmaze"), Kind::Series, &show, [None, None])
            .unwrap()
            .imdb_id
    };
    assert_eq!(
        imdb(json!({"imdb": "tt0903747"})).as_deref(),
        Some("tt0903747")
    );
    assert_eq!(imdb(json!({"imdb": "0903747"})), None);
    assert_eq!(imdb(json!({"imdb": null})), None);
    assert_eq!(imdb(json!({})), None);
}

fn show(id: u64, weight: u32, rating: Option<f64>, premiered: &str) -> Value {
    json!({"id": id, "name": format!("Show {id}"), "weight": weight, "rating": {"average": rating},
           "premiered": premiered, "image": {"medium": "x"}})
}

fn schedule(shows: &[Value]) -> Value {
    Value::Array(
        shows
            .iter()
            .map(|show| json!({ "_embedded": { "show": show } }))
            .collect(),
    )
}

fn order(sort: Sort, shows: &[Value]) -> Vec<String> {
    let page = shape_for_test(
        &def("tvmaze"),
        Kind::Series,
        &filters(Some(sort), 1, 50),
        &[schedule(shows)],
        day("2026-10-14"),
    );
    page.results.into_iter().map(|media| media.id).collect()
}

#[test]
fn popularity_breaks_a_weight_tie_by_rating_then_premiere() {
    let shows = [
        show(1, 100, Some(7.0), "2010-01-01"),
        show(2, 100, Some(9.0), "2010-01-01"),
        show(3, 100, Some(9.0), "2020-01-01"),
        show(4, 50, Some(10.0), "2020-01-01"),
    ];
    assert_eq!(
        order(Sort::Popularity, &shows),
        ["series-3", "series-2", "series-1", "series-4"]
    );
    assert_eq!(
        order(Sort::Trending, &shows),
        order(Sort::Popularity, &shows)
    );
}

#[test]
fn an_unrated_show_is_not_ranked_as_badly_rated() {
    let shows = [
        show(1, 90, None, "2010-01-01"),
        show(2, 10, Some(1.0), "2010-01-01"),
    ];
    assert_eq!(order(Sort::Score, &shows), ["series-2", "series-1"]);
}

#[test]
fn recent_orders_by_premiere_and_title_by_name() {
    let shows = [
        show(1, 90, None, "2010-01-01"),
        show(2, 10, None, "2024-05-01"),
    ];
    assert_eq!(order(Sort::Recent, &shows), ["series-2", "series-1"]);
    let mut named = [
        show(1, 1, None, "2010-01-01"),
        show(2, 1, None, "2010-01-01"),
    ];
    named[0]["name"] = json!("beta");
    named[1]["name"] = json!("Alpha");
    assert_eq!(order(Sort::Title, &named), ["series-2", "series-1"]);
}

#[test]
fn the_pool_is_deduplicated_and_drops_unwatchable_shows() {
    let pool = schedule(&[
        show(1, 1, None, "2020-01-01"),
        show(1, 1, None, "2020-01-01"),
        show(2, 1, None, "2027-06-01"),
        show(3, 1, None, "2026-11-01"),
        json!({"id": 4, "name": "No image", "premiered": "2020-01-01"}),
    ]);
    let page = shape_for_test(
        &def("tvmaze"),
        Kind::Series,
        &filters(None, 1, 50),
        &[pool],
        day("2026-10-14"),
    );
    let ids: Vec<&str> = page.results.iter().map(|m| m.id.as_str()).collect();
    // Both are as popular as each other, so the later premiere comes first.
    assert_eq!(ids, ["series-3", "series-1"]);
}

#[test]
fn the_pool_is_filtered_by_genre_and_paged_here() {
    let mut shows: Vec<Value> = (1..=5)
        .map(|id| show(id, 100 - id as u32, None, "2020-01-01"))
        .collect();
    shows[0]["genres"] = json!(["Comedy"]);
    shows[2]["genres"] = json!(["comedy"]);
    shows[4]["genres"] = json!(["Comedy"]);
    let comedy = |page| Filters {
        genre: Some("Comedy".into()),
        page,
        per_page: 2,
        ..Default::default()
    };
    let first = shape_for_test(
        &def("tvmaze"),
        Kind::Series,
        &comedy(1),
        &[schedule(&shows)],
        day("2026-10-14"),
    );
    assert_eq!(
        first
            .results
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["series-1", "series-3"]
    );
    assert!(first.has_next);
    let second = shape_for_test(
        &def("tvmaze"),
        Kind::Series,
        &comedy(2),
        &[schedule(&shows)],
        day("2026-10-14"),
    );
    assert_eq!(second.results.len(), 1);
    assert!(!second.has_next);
}

#[test]
fn a_tvmaze_search_keeps_shows_with_an_image() {
    let body = json!([{ "score": 1, "show": tvmaze_show() }, { "score": 1, "show": { "id": 2, "name": "No image" } }]);
    let page = shape_search_for_test(&def("tvmaze"), Kind::Series, &body);
    assert_eq!(page.results.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// YTS
// ---------------------------------------------------------------------------------------------

fn dangal() -> Value {
    json!({
        "id": 7691, "title": "Dangal", "title_english": "Dangal", "year": 2016, "rating": 8.3,
        "runtime": 161, "genres": ["Action", "Biography"], "like_count": 120,
        "large_cover_image": "https://y/large.jpg", "medium_cover_image": "https://y/medium.jpg",
        "background_image_original": "https://y/bg.jpg", "imdb_code": "tt5074352",
        "yt_trailer_code": "x6Q_hbDVTZM", "description_full": "A wrestler trains his daughters.",
        "cast": [{"name": "Aamir Khan", "character_name": "Mahavir", "url_small_image": "https://y/a.jpg"}]
    })
}

#[test]
fn a_yts_movie_maps_onto_the_card_shape() {
    let card = media("yts", Kind::Movie, &dangal()).unwrap();
    assert_eq!(card.id, "movie-7691");
    assert_eq!(card.title, "Dangal");
    assert_eq!(card.title_variants, vec!["Dangal"]);
    assert_eq!(card.poster.as_deref(), Some("https://y/large.jpg"));
    assert_eq!(card.banner.as_deref(), Some("https://y/bg.jpg"));
    assert_eq!(card.format.as_deref(), Some("MOVIE"));
    assert_eq!(
        (card.year, card.score, card.runtime_minutes),
        (Some(2016), Some(83), Some(161))
    );
    assert_eq!(card.popularity, 120);
    assert_eq!(
        (card.episodes, card.status.as_deref()),
        (Some(1), Some("FINISHED"))
    );
}

#[test]
fn distinct_names_become_title_variants() {
    let card = media(
        "yts",
        Kind::Movie,
        &json!({"id": 2, "title": "Dangal", "title_english": "Dangal: The Film"}),
    )
    .unwrap();
    assert_eq!(card.title, "Dangal: The Film");
    assert_eq!(card.title_variants, vec!["Dangal: The Film", "Dangal"]);
}

#[test]
fn a_yts_detail_carries_the_trailer_cast_imdb_id_and_suggestions() {
    let suggestions = json!({ "status": "ok", "data": { "movies": [{"id": 1, "title": "Sultan", "year": 2016}] } });
    let answer = json!({ "status": "ok", "data": { "movie": dangal() } });
    let detail = read_for_test(
        &def("yts"),
        Kind::Movie,
        &answer,
        [None, Some(&suggestions)],
    )
    .unwrap();
    assert_eq!(
        detail.media.description.as_deref(),
        Some("A wrestler trains his daughters.")
    );
    assert_eq!(
        detail.trailer,
        Some(Trailer {
            site: "youtube".into(),
            id: "x6Q_hbDVTZM".into()
        })
    );
    assert_eq!(detail.cast[0].role.as_deref(), Some("Mahavir"));
    assert_eq!(detail.imdb_id.as_deref(), Some("tt5074352"));
    assert_eq!(detail.recommended[0].id, "movie-1");
    assert!(detail.episodes.is_empty());
    let none = read_for_test(&def("yts"), Kind::Movie, &answer, [None, None]).unwrap();
    assert!(none.recommended.is_empty());
}

#[test]
fn a_placeholder_movie_for_an_unknown_id_is_not_a_title() {
    let missing = |movie: Value| {
        read_for_test(
            &def("yts"),
            Kind::Movie,
            &json!({ "data": { "movie": movie } }),
            [None, None],
        )
    };
    assert!(missing(json!({"id": 0, "title": null})).is_none());
    assert!(missing(json!({"id": 5, "title": null})).is_none());
    assert!(missing(json!({"id": 0, "title": "Ghost"})).is_none());
    assert!(missing(dangal()).is_some());
}

fn yts_page(movies: &[Value], total: u64) -> Value {
    json!({ "status": "ok", "data": { "movie_count": total, "movies": movies } })
}

fn film(id: u64, year: Option<i64>) -> Value {
    json!({ "id": id, "title": format!("Film {id}"), "year": year })
}

#[test]
fn yts_pages_upstream_and_says_whether_more_follows() {
    let body = yts_page(
        &[film(1, Some(2020)), film(2, Some(2021)), film(3, None)],
        100,
    );
    let page = shape_for_test(
        &def("yts"),
        Kind::Movie,
        &filters(None, 1, 24),
        std::slice::from_ref(&body),
        day("2026-10-13"),
    );
    // A film with no year is left out of every row.
    assert_eq!(page.results.len(), 2);
    assert!(page.has_next);
    let last = shape_for_test(
        &def("yts"),
        Kind::Movie,
        &filters(None, 5, 24),
        &[body],
        day("2026-10-13"),
    );
    assert!(!last.has_next);
}

#[test]
fn a_new_release_row_excludes_the_announced_and_the_backfilled() {
    let now = day("2026-10-13");
    let body = yts_page(
        &[
            film(1, Some(2027)),
            film(2, Some(2026)),
            film(3, Some(2025)),
            film(4, Some(1957)),
            film(5, Some(2021)),
            film(6, None),
        ],
        600,
    );
    let page = shape_for_test(
        &def("yts"),
        Kind::Movie,
        &filters(Some(Sort::Recent), 1, 24),
        std::slice::from_ref(&body),
        now,
    );
    assert_eq!(
        page.results
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["movie-2", "movie-3"]
    );
    // A widened page was not the whole catalogue, so there is no next page to offer.
    assert!(!page.has_next);
    // Another sort shows them all.
    let all = shape_for_test(
        &def("yts"),
        Kind::Movie,
        &filters(Some(Sort::Popularity), 1, 24),
        &[body],
        now,
    );
    assert_eq!(all.results.len(), 5);
}

#[test]
fn a_year_is_filtered_here_because_the_service_ignores_it() {
    let body = yts_page(
        &[
            film(1, Some(2018)),
            film(2, Some(2024)),
            film(3, Some(2024)),
        ],
        600,
    );
    let year = Filters {
        year: Some(2024),
        page: 1,
        per_page: 24,
        ..Default::default()
    };
    let page = shape_for_test(&def("yts"), Kind::Movie, &year, &[body], day("2026-10-13"));
    assert_eq!(
        page.results
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["movie-2", "movie-3"]
    );
    assert!(!page.has_next);
}

#[test]
fn a_yts_search_says_whether_there_are_more_than_it_asked_for() {
    let many = shape_search_for_test(
        &def("yts"),
        Kind::Movie,
        &yts_page(&[film(1, Some(2020))], 80),
    );
    assert!(many.has_next);
    let few = shape_search_for_test(
        &def("yts"),
        Kind::Movie,
        &yts_page(&[film(1, Some(2020))], 1),
    );
    assert!(!few.has_next);
    // No movies key at all is an ordinary empty result.
    let none = shape_search_for_test(
        &def("yts"),
        Kind::Movie,
        &json!({ "status": "ok", "data": { "movie_count": 0 } }),
    );
    assert!(none.results.is_empty());
}

#[test]
fn yts_is_asked_for_the_genre_in_lower_case_and_the_sort_by_its_name() {
    let provider = def("yts");
    let list = provider.list.as_ref().unwrap();
    let mut asking = Asking::new(&provider, Kind::Movie);
    asking.sort = Some(Sort::Recent);
    asking.per_page = 50;
    asking.genre = Some("Sci-Fi");
    let request = asking.prepare(&list.request).unwrap();
    let query = request.query_for_test();
    assert!(query.contains(&("genre".to_string(), "sci-fi".to_string())));
    assert!(query.contains(&("sort_by".to_string(), "date_added".to_string())));
    assert!(query.contains(&("limit".to_string(), "50".to_string())));
    // No genre means no genre parameter.
    let mut bare = Asking::new(&provider, Kind::Movie);
    bare.per_page = 24;
    let query = bare.prepare(&list.request).unwrap().query_for_test();
    assert!(query.iter().all(|(name, _)| name != "genre"));
    assert!(query.contains(&("sort_by".to_string(), "download_count".to_string())));
}

// ---------------------------------------------------------------------------------------------
// The chains
// ---------------------------------------------------------------------------------------------

#[test]
fn each_kind_has_a_chain_of_providers_that_serve_it() {
    let sources = sources::current();
    for kind in Kind::ALL {
        let chain = super::chain(&sources, kind);
        assert!(!chain.is_empty());
        assert!(chain.iter().all(|provider| provider.kinds.contains(&kind)));
    }
    let ids = |kind| -> Vec<String> {
        super::chain(&sources, kind)
            .iter()
            .map(|p| p.id.clone())
            .collect()
    };
    assert_eq!(ids(Kind::Anime), ["anilist"]);
    assert_eq!(ids(Kind::Series), ["cinemeta", "tvmaze"]);
    assert_eq!(ids(Kind::Movie), ["cinemeta", "yts"]);
}

#[test]
fn a_provider_whose_request_has_a_misspelt_hole_is_refused() {
    let mut broken = def("tvmaze");
    broken.search.as_mut().unwrap().request.url = "{base}/search/shows?q={trem}".into();
    assert!(super::check(&broken).is_err());
    assert!(super::check(&def("tvmaze")).is_ok());
}
