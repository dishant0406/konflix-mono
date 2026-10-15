//! The JSON the API sends. See API.md.
//!
//! These mirror the library's own types but leave out what only the server uses (the other names
//! a title is known by, the IMDb id), so the API stays small and the library stays free to change.

use akidev_core::model::{Detail, Episode, Kind, Media, Person, Trailer};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MediaOut {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub poster: Option<String>,
    pub banner: Option<String>,
    pub description: Option<String>,
    pub format: Option<String>,
    pub year: Option<i32>,
    pub episodes: Option<u32>,
    pub runtime_minutes: Option<u32>,
    pub status: Option<String>,
    pub score: Option<u32>,
    pub popularity: i64,
    pub genres: Vec<String>,
    pub is_adult: bool,
}

impl From<&Media> for MediaOut {
    fn from(media: &Media) -> MediaOut {
        MediaOut {
            id: media.id.clone(),
            kind: media.kind,
            title: media.title.clone(),
            poster: media.poster.clone(),
            banner: media.banner.clone(),
            description: media.description.clone(),
            format: media.format.clone(),
            year: media.year,
            episodes: media.episodes,
            runtime_minutes: media.runtime_minutes,
            status: media.status.clone(),
            score: media.score,
            popularity: media.popularity,
            genres: media.genres.clone(),
            is_adult: media.is_adult,
        }
    }
}

pub fn media_list(list: &[Media]) -> Vec<MediaOut> {
    list.iter().map(MediaOut::from).collect()
}

#[derive(Debug, Serialize)]
pub struct PageOut {
    pub results: Vec<MediaOut>,
    pub has_next: bool,
}

#[derive(Debug, Serialize)]
pub struct RowOut {
    pub title: &'static str,
    pub kind: Kind,
    pub items: Vec<MediaOut>,
}

#[derive(Debug, Serialize)]
pub struct HomeOut {
    pub featured: Vec<MediaOut>,
    pub rows: Vec<RowOut>,
}

#[derive(Debug, Serialize)]
pub struct DetailOut {
    pub media: MediaOut,
    pub studios: Vec<String>,
    pub trailer: Option<Trailer>,
    pub episodes: Vec<Episode>,
    pub cast: Vec<Person>,
    pub related: Vec<MediaOut>,
    pub recommended: Vec<MediaOut>,
}

impl From<&Detail> for DetailOut {
    fn from(detail: &Detail) -> DetailOut {
        DetailOut {
            media: MediaOut::from(&detail.media),
            studios: detail.studios.clone(),
            trailer: detail.trailer.clone(),
            episodes: detail.episodes.clone(),
            cast: detail.cast.clone(),
            related: media_list(&detail.related),
            recommended: media_list(&detail.recommended),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Subtitle {
    pub label: String,
    pub lang: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ready {
    pub available: bool,
    pub url: String,
    pub stream_type: &'static str,
    pub release: String,
    pub source: &'static str,
    pub duration: Option<f64>,
    pub subtitles: Vec<Subtitle>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_media_json_has_the_fields_the_contract_names_and_no_more() {
        let media = Media {
            id: "anime-1".into(),
            kind: Kind::Anime,
            title: "T".into(),
            title_variants: vec!["secret".into()],
            poster: None,
            banner: None,
            description: None,
            format: None,
            year: Some(2020),
            episodes: Some(12),
            runtime_minutes: None,
            status: None,
            score: Some(80),
            popularity: 5,
            genres: vec!["Action".into()],
            is_adult: false,
        };
        let json = serde_json::to_value(MediaOut::from(&media)).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "banner",
                "description",
                "episodes",
                "format",
                "genres",
                "id",
                "is_adult",
                "kind",
                "popularity",
                "poster",
                "runtime_minutes",
                "score",
                "status",
                "title",
                "year"
            ]
        );
        assert_eq!(json["kind"], "anime");
    }
}
