//! The routes. See API.md for what each takes and returns.

use crate::config::Settings;
use crate::deliver::{self, Deliverer, Registry};
use crate::dto::{DetailOut, HomeOut, PageOut, RowOut, media_list};
use crate::hls::Hls;
use crate::limits::{Limiter, client_of};
use crate::play::{Outcome, Player};
use akidev_core::media_id;
use akidev_core::model::{Filters, Kind, Media, Sort};
use akidev_core::{providers, search};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub player: Arc<Player>,
    pub hls: Arc<Hls>,
    pub registry: Arc<Registry>,
    pub limiter: Arc<Limiter>,
    pub engine: Option<Arc<akidev_core::torrent::Engine>>,
    pub images: Arc<crate::img::Images>,
    pub subs: Arc<crate::subs::Subs>,
    pub ffmpeg: bool,
}

const PER_PAGE: u32 = 24;
const FEATURED: usize = 9;
const SEARCH_MAX: usize = 60;
/// How long a browser and the edge keep a list, and how long past that they may show it while a
/// fresh one is fetched. A list changes over hours, and showing the last one at once is what makes a
/// page seen before open instantly.
const CATALOGUE_CACHE: &str = "public, max-age=300, stale-while-revalidate=3600";
const TITLE_CACHE: &str = "public, max-age=900, stale-while-revalidate=3600";

struct Row {
    title: &'static str,
    kind: Kind,
    sort: Sort,
    genre: Option<&'static str>,
}

/// The home page's rows, in order. Each draws from one catalogue, so the page mixes all three.
/// The same ones the desktop app shows.
const ROWS: [Row; 10] = [
    Row {
        title: "Trending Anime",
        kind: Kind::Anime,
        sort: Sort::Trending,
        genre: None,
    },
    Row {
        title: "New Releases",
        kind: Kind::Movie,
        sort: Sort::Recent,
        genre: None,
    },
    Row {
        title: "Popular Series",
        kind: Kind::Series,
        sort: Sort::Popularity,
        genre: None,
    },
    Row {
        title: "Newly Released Anime",
        kind: Kind::Anime,
        sort: Sort::Recent,
        genre: None,
    },
    Row {
        title: "Most Watched Films",
        kind: Kind::Movie,
        sort: Sort::Popularity,
        genre: None,
    },
    Row {
        title: "New Series",
        kind: Kind::Series,
        sort: Sort::Recent,
        genre: None,
    },
    Row {
        title: "Top Rated Anime",
        kind: Kind::Anime,
        sort: Sort::Score,
        genre: None,
    },
    Row {
        title: "Action Films",
        kind: Kind::Movie,
        sort: Sort::Popularity,
        genre: Some("Action"),
    },
    Row {
        title: "Drama Series",
        kind: Kind::Series,
        sort: Sort::Score,
        genre: Some("Drama"),
    },
    Row {
        title: "Romance Anime",
        kind: Kind::Anime,
        sort: Sort::Popularity,
        genre: Some("Romance"),
    },
];

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn cached<T: serde::Serialize>(policy: &'static str, body: T) -> Response {
    ([(header::CACHE_CONTROL, policy)], Json(body)).into_response()
}

fn parse_sort(text: &str) -> Option<Sort> {
    [
        Sort::Trending,
        Sort::Popularity,
        Sort::Score,
        Sort::Recent,
        Sort::Title,
    ]
    .into_iter()
    .find(|sort| sort.as_str() == text)
}

pub fn router(state: AppState) -> Router {
    // JSON is compressed. The video routes are not: compressing a range response would break its
    // length, and video does not compress anyway.
    let api = Router::new()
        .route("/health", get(health))
        .route("/v1/home", get(home))
        .route("/v1/catalogue", get(catalogue))
        .route("/v1/search", get(search_route))
        .route("/v1/titles/{id}", get(title))
        .route("/v1/img", get(image))
        .route("/v1/releases", get(releases))
        .route("/v1/subtitles", get(subtitle_list))
        .route("/v1/subtitles/file/{token}", get(subtitle_file))
        .route("/v1/play", post(play))
        .layer(CompressionLayer::new().gzip(true))
        .with_state(state.clone());

    let deliverer = Deliverer {
        registry: state.registry.clone(),
        hls: state.hls.clone(),
    };
    let streams = Router::new()
        .route("/s/{token}", get(deliver::direct))
        .route("/s/{token}/{file}", get(deliver::hls))
        .with_state(deliverer);

    api.merge(streams).layer(cors(&state.settings))
}

#[derive(Deserialize)]
struct ImageQuery {
    u: String,
    w: Option<u32>,
    q: Option<u8>,
}

/// A poster or banner at the size asked for. The answer never changes for the same address, so it
/// is cached for good by the edge and by the browser.
async fn image(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<ImageQuery>,
) -> Response {
    // A page shows dozens at once, so pictures have an allowance of their own.
    let client = client_of(&headers, Some(peer));
    if !state
        .limiter
        .allow(&client, "img", 2400, Duration::from_secs(60))
    {
        return too_many();
    }
    match state
        .images
        .get(&query.u, query.w.unwrap_or(640), query.q.unwrap_or(75))
        .await
    {
        Ok(picture) => (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(picture.content_type),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
                // The site loads these from another origin, with no credentials.
                (
                    HeaderName::from_static("cross-origin-resource-policy"),
                    HeaderValue::from_static("cross-origin"),
                ),
            ],
            picture.bytes,
        )
            .into_response(),
        Err(crate::img::Error::Refused) => error(
            StatusCode::BAD_REQUEST,
            "That picture is not one this server fetches.",
        ),
        Err(crate::img::Error::Unavailable) => {
            error(StatusCode::BAD_GATEWAY, "The picture could not be fetched.")
        }
    }
}

fn cors(settings: &Settings) -> CorsLayer {
    let origins: Vec<HeaderValue> = settings
        .cors_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::HEAD, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::RANGE])
        .expose_headers([
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::CONTENT_LENGTH,
        ])
        .max_age(Duration::from_secs(3600));
    if settings.cors_origins.iter().any(|origin| origin == "*") {
        layer.allow_origin(AllowOrigin::any())
    } else if let [only] = origins.as_slice() {
        // One origin is answered the same way for everyone, so the response carries no `Vary:
        // Origin`. That matters at the edge: a cached copy that was made for a request without an
        // Origin header would otherwise be handed to the page with no CORS headers at all.
        layer
            .allow_origin(AllowOrigin::exact(only.clone()))
            .vary([] as [HeaderName; 0])
    } else {
        layer.allow_origin(AllowOrigin::list(origins))
    }
}

/// Counts a catalogue request against the client's allowance.
fn within_api_limit(state: &AppState, headers: &HeaderMap, peer: SocketAddr) -> bool {
    let client = client_of(headers, Some(peer));
    state.limiter.allow(
        &client,
        "api",
        state.settings.api_limit,
        Duration::from_secs(60),
    )
}

fn too_many() -> Response {
    let mut response = error(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many requests. Slow down a little.",
    );
    response.headers_mut().insert(
        HeaderName::from_static("retry-after"),
        HeaderValue::from_static("30"),
    );
    response
}

async fn health(State(state): State<AppState>) -> Response {
    let debrid = match state.player.debrid() {
        Some(debrid) => json!({ "configured": true, "reachable": debrid.status().await }),
        None => json!({ "configured": false }),
    };
    let cache = crate::hls::dir_size(&state.settings.cache_dir).await;
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "ffmpeg": state.ffmpeg,
        "sources": state.player.sources(),
        "debrid": debrid,
        "active_torrents": state.engine.as_ref().map_or(0, |engine| engine.active_torrents()),
        "hls_sessions": state.hls.count(),
        "resolving": state.player.resolving(),
        "streams": state.registry.len(),
        "cache_bytes": cache,
    }))
    .into_response()
}

async fn home(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let featured = futures::future::join_all(Kind::ALL.map(|kind| async move {
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
    }));
    let rows = futures::future::join_all(ROWS.iter().map(|row| async move {
        let filters = Filters {
            sort: Some(row.sort),
            genre: row.genre.map(str::to_string),
            page: 1,
            per_page: PER_PAGE,
            ..Default::default()
        };
        let items = providers::list(row.kind, &filters)
            .await
            .map(|page| page.results)
            .unwrap_or_default();
        RowOut {
            title: row.title,
            kind: row.kind,
            items: media_list(&items),
        }
    }));
    let (featured, rows) = futures::future::join(featured, rows).await;
    // The banner draws from the front of each catalogue, so it features all three, and only
    // titles that have wide art.
    let featured: Vec<Media> = search::interleave(featured, FEATURED * 2)
        .into_iter()
        .filter(|media| media.banner.is_some())
        .take(FEATURED)
        .collect();
    // A home page with nothing on it is an outage, not a page.
    if featured.is_empty() && rows.iter().all(|row| row.items.is_empty()) {
        return error(StatusCode::BAD_GATEWAY, "No catalogue answered.");
    }
    cached(
        CATALOGUE_CACHE,
        HomeOut {
            featured: media_list(&featured),
            rows,
        },
    )
}

#[derive(Deserialize)]
struct CatalogueQuery {
    kind: Option<String>,
    sort: Option<String>,
    genre: Option<String>,
    page: Option<u32>,
}

async fn catalogue(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<CatalogueQuery>,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let kinds: Vec<Kind> = match query.kind.as_deref().unwrap_or("all") {
        "all" => Kind::ALL.to_vec(),
        other => match Kind::parse(other) {
            Some(kind) => vec![kind],
            None => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "kind must be anime, series, movie or all.",
                );
            }
        },
    };
    let sort = match query.sort.as_deref() {
        None => Sort::Popularity,
        Some(text) => match parse_sort(text) {
            Some(sort) => sort,
            None => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "sort must be popularity, trending, score, recent or title.",
                );
            }
        },
    };
    let genre = query
        .genre
        .filter(|genre| !genre.is_empty() && genre.len() <= 40);
    let page = query.page.unwrap_or(1).clamp(1, 500);
    let filters = Filters {
        sort: Some(sort),
        genre,
        page,
        per_page: PER_PAGE,
        ..Default::default()
    };

    let answers = futures::future::join_all(kinds.iter().map(|&kind| {
        let filters = filters.clone();
        async move { providers::list(kind, &filters).await }
    }))
    .await;
    let answered: Vec<_> = answers.into_iter().filter_map(Result::ok).collect();
    if answered.is_empty() {
        return error(StatusCode::BAD_GATEWAY, "No catalogue answered.");
    }
    let has_next = answered.iter().any(|page| page.has_next);
    let merged = search::interleave(
        answered.into_iter().map(|page| page.results).collect(),
        usize::MAX,
    );
    cached(
        CATALOGUE_CACHE,
        PageOut {
            results: media_list(&merged),
            has_next,
        },
    )
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<usize>,
}

async fn search_route(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let term = query.q.unwrap_or_default();
    let term = term.trim();
    if term.is_empty() || term.chars().count() > 100 {
        return error(StatusCode::BAD_REQUEST, "q must be 1 to 100 characters.");
    }
    let limit = query.limit.unwrap_or(SEARCH_MAX).clamp(1, SEARCH_MAX);
    match search::search(term, &Kind::ALL, limit).await {
        Ok(results) => cached(CATALOGUE_CACHE, json!({ "results": media_list(&results) })),
        Err(_) => error(StatusCode::BAD_GATEWAY, "No catalogue answered."),
    }
}

async fn title(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let Some(parsed) = media_id::parse(&id) else {
        return error(StatusCode::BAD_REQUEST, "That is not a media id.");
    };
    match providers::detail(&parsed.key()).await {
        Some(detail) => cached(TITLE_CACHE, DetailOut::from(&detail)),
        None => error(StatusCode::NOT_FOUND, "That title is not in the catalogue."),
    }
}

#[derive(Deserialize)]
struct PlayBody {
    id: String,
    #[serde(default)]
    hevc: bool,
    /// Where the viewer means to start, in seconds.
    start: Option<f64>,
    /// The info hash of a release the viewer picked from `/v1/releases`. That one is played, and
    /// no other.
    release: Option<String>,
}

async fn play(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PlayBody>,
) -> Response {
    let client = client_of(&headers, Some(peer));
    if !state.limiter.allow(
        &client,
        "play",
        state.settings.play_limit,
        Duration::from_secs(600),
    ) {
        return too_many();
    }
    match media_id::parse(&body.id) {
        Some(parsed) if parsed.kind == Kind::Movie || parsed.episode.is_some() => {}
        _ => return error(StatusCode::BAD_REQUEST, "That is not an episode or a film."),
    }
    if let Some(release) = &body.release
        && !(release.len() == 40 && release.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return error(
            StatusCode::BAD_REQUEST,
            "release must be a 40 character info hash.",
        );
    }
    match state
        .player
        .play(&body.id, body.hevc, body.start, body.release)
        .await
    {
        Outcome::Ready(ready) => {
            ([(header::CACHE_CONTROL, "no-store")], Json(ready)).into_response()
        }
        Outcome::Unavailable(reason) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({ "available": false, "reason": reason })),
        )
            .into_response(),
        Outcome::Busy => error(StatusCode::SERVICE_UNAVAILABLE, "busy"),
    }
}

/// What an episode or a film can be played from, for a viewer to pick. Which of them the debrid
/// service holds is said, because those start at once and the others wait for a swarm.
async fn releases(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
) -> Response {
    let client = client_of(&headers, Some(peer));
    // Finding them searches every indexer the first time, so this has an allowance like a play.
    if !state.limiter.allow(
        &client,
        "releases",
        state.settings.play_limit * 2,
        Duration::from_secs(600),
    ) {
        return too_many();
    }
    match media_id::parse(&query.id) {
        Some(parsed) if parsed.kind == Kind::Movie || parsed.episode.is_some() => {}
        _ => return error(StatusCode::BAD_REQUEST, "That is not an episode or a film."),
    }
    match state.player.releases(&query.id).await {
        Ok((choices, held)) => {
            let list: Vec<serde_json::Value> = choices
                .into_iter()
                .map(|choice| {
                    let cached = held.as_ref().map(|held| held.contains(&choice.id));
                    let mut value = serde_json::to_value(&choice).unwrap_or_default();
                    value["cached"] = json!(cached);
                    value
                })
                .collect();
            // Searches are kept for ten minutes in the library, so a short life here is enough.
            (
                [(header::CACHE_CONTROL, "private, max-age=120")],
                Json(json!({ "releases": list })),
            )
                .into_response()
        }
        Err(reason) => error(StatusCode::NOT_FOUND, &reason),
    }
}

#[derive(Deserialize)]
struct IdQuery {
    id: String,
}

/// The subtitles there are for an episode or a film, a few per language, as a menu: where each came
/// from, what it is for, and an address on this server for the file as WebVTT.
async fn subtitle_list(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let parsed = match media_id::parse(&query.id) {
        Some(parsed) if parsed.kind == Kind::Movie || parsed.episode.is_some() => parsed,
        _ => return error(StatusCode::BAD_REQUEST, "That is not an episode or a film."),
    };
    let Some(detail) = providers::detail(&parsed.key()).await else {
        return error(StatusCode::NOT_FOUND, "That title is not in the catalogue.");
    };
    let found = akidev_core::subtitles::find(
        &akidev_core::subtitles::query_for(&detail, &parsed),
        Duration::from_secs(8),
    )
    .await;
    let public = &state.settings.public_url;
    let mut list = Vec::with_capacity(found.len());
    for subtitle in found {
        let token = state.subs.register(&subtitle.url).await;
        list.push(json!({
            "id": subtitle.id,
            "provider": subtitle.provider,
            "lang": subtitle.lang,
            "language": subtitle.language,
            "label": subtitle.label,
            "hearing_impaired": subtitle.hearing_impaired,
            "url": format!("{public}/v1/subtitles/file/{token}.vtt"),
        }));
    }
    let policy = if list.is_empty() {
        // Nothing yet may be something soon, and a source that was down should not be remembered.
        "public, max-age=60"
    } else {
        "public, max-age=600, stale-while-revalidate=3600"
    };
    (
        [(header::CACHE_CONTROL, policy)],
        Json(json!({ "subtitles": list })),
    )
        .into_response()
}

async fn subtitle_file(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Response {
    if !within_api_limit(&state, &headers, peer) {
        return too_many();
    }
    let token = token.strip_suffix(".vtt").unwrap_or(&token);
    match state.subs.vtt(token).await {
        Ok(text) => (
            [
                (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
                // A file never changes under its token, so it is kept for good.
                (header::CACHE_CONTROL, "public, max-age=604800, immutable"),
            ],
            text,
        )
            .into_response(),
        Err(crate::subs::Error::Unknown) => error(
            StatusCode::NOT_FOUND,
            "That subtitle is not listed. Ask for the list again.",
        ),
        Err(crate::subs::Error::Unavailable) => error(
            StatusCode::BAD_GATEWAY,
            "The subtitle could not be fetched.",
        ),
    }
}
