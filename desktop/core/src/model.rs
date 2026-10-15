//! The shapes every provider produces and the UI renders.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Anime,
    Series,
    Movie,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Anime, Kind::Series, Kind::Movie];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Anime => "anime",
            Kind::Series => "series",
            Kind::Movie => "movie",
        }
    }

    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.as_str() == text)
    }
}

/// How a browse row or a search is ordered. Providers map these onto what they have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sort {
    Trending,
    Popularity,
    Score,
    Recent,
    Title,
}

impl Sort {
    /// The name the definitions use for this sort.
    pub fn as_str(self) -> &'static str {
        match self {
            Sort::Trending => "trending",
            Sort::Popularity => "popularity",
            Sort::Score => "score",
            Sort::Recent => "recent",
            Sort::Title => "title",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub sort: Option<Sort>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub page: u32,
    pub per_page: u32,
}

/// One card: everything a poster grid, a row or the hero needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Media {
    /// The title's key, e.g. `series-tt0903747` or `anime-16498`. See `media_id`.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    /// Other names the title is known by, used to match release names.
    pub title_variants: Vec<String>,
    pub poster: Option<String>,
    pub banner: Option<String>,
    pub description: Option<String>,
    pub format: Option<String>,
    pub year: Option<i32>,
    /// Total episodes, when the provider reports one.
    pub episodes: Option<u32>,
    pub runtime_minutes: Option<u32>,
    pub status: Option<String>,
    /// 0-100, so every provider's rating reads on one scale.
    pub score: Option<u32>,
    pub popularity: i64,
    pub genres: Vec<String>,
    pub is_adult: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub results: Vec<Media>,
    pub has_next: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// A full media id, e.g. `series-tt0903747-s1e5`.
    pub id: String,
    pub season: Option<u32>,
    pub number: u32,
    pub name: Option<String>,
    pub airdate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub role: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trailer {
    pub site: String,
    pub id: String,
}

/// A title in full, for the detail and watch pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub media: Media,
    pub studios: Vec<String>,
    pub trailer: Option<Trailer>,
    /// Empty for a film, and for an anime whose provider reports only a count.
    pub episodes: Vec<Episode>,
    pub cast: Vec<Person>,
    pub related: Vec<Media>,
    pub recommended: Vec<Media>,
    /// Streaming search keys the stream layer needs. Not rendered.
    pub imdb_id: Option<String>,
}

/// One torrent an indexer found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub title: String,
    /// 40 lowercase hex characters. See `magnet::to_hex_info_hash`.
    pub info_hash: String,
    pub magnet: String,
    pub size_bytes: Option<u64>,
    pub seeders: u32,
    /// Which indexer, or which tracker an aggregating indexer got it from.
    pub source: String,
}
