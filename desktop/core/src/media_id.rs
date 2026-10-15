//! Media ids.
//!
//! Every id names the kind, then the provider's own id, then the episode where
//! there is one:
//!
//! ```text
//! anime-16498-ep1     an anime, episode 1
//! series-169-s1e5     a series, season 1 episode 5
//! movie-tt32261958    a film, no episode
//! ```
//!
//! The kind is part of the id because ids come from different providers and would
//! collide: TVMaze's show 1 and YTS's film 1 are different titles. The provider id is
//! alphanumeric because Cinemeta uses IMDb ids ("tt32261958").

use crate::model::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaId {
    pub kind: Kind,
    pub provider_id: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

impl MediaId {
    /// The title's own id, without the episode.
    pub fn key(&self) -> String {
        title_key(self.kind, &self.provider_id)
    }
}

pub fn title_key(kind: Kind, provider_id: &str) -> String {
    format!("{}-{}", kind.as_str(), provider_id)
}

pub fn anime_episode_id(provider_id: &str, episode: u32) -> String {
    format!("anime-{provider_id}-ep{episode}")
}

/// Parses a media id. The suffix must match the kind: a film has no episode and an
/// anime has no season, so a malformed id is rejected rather than reinterpreted.
pub fn parse(id: &str) -> Option<MediaId> {
    let (kind_text, rest) = id.split_once('-')?;
    let kind = Kind::parse(kind_text)?;
    let (provider_id, suffix) = match rest.split_once('-') {
        Some((provider_id, suffix)) => (provider_id, Some(suffix)),
        None => (rest, None),
    };
    if provider_id.is_empty() || !provider_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }

    let mut parsed = MediaId {
        kind,
        provider_id: provider_id.to_string(),
        season: None,
        episode: None,
    };
    let Some(suffix) = suffix else {
        return Some(parsed);
    };

    match kind {
        Kind::Movie => return None,
        Kind::Anime => parsed.episode = Some(digits(suffix.strip_prefix("ep")?)?),
        Kind::Series => {
            let (season, episode) = suffix.strip_prefix('s')?.split_once('e')?;
            parsed.season = Some(digits(season)?);
            parsed.episode = Some(digits(episode)?);
        }
    }
    Some(parsed)
}

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_kind() {
        let anime = parse("anime-16498-ep1").unwrap();
        assert_eq!(
            (anime.kind, anime.episode, anime.season),
            (Kind::Anime, Some(1), None)
        );
        let series = parse("series-169-s1e5").unwrap();
        assert_eq!((series.season, series.episode), (Some(1), Some(5)));
        let film = parse("movie-tt32261958").unwrap();
        assert_eq!(
            (film.provider_id.as_str(), film.episode),
            ("tt32261958", None)
        );
    }

    #[test]
    fn a_title_key_has_no_episode() {
        assert_eq!(parse("series-169").unwrap().episode, None);
        assert_eq!(parse("series-169-s2e3").unwrap().key(), "series-169");
    }

    #[test]
    fn rejects_a_suffix_that_does_not_match_the_kind() {
        assert!(parse("movie-7691-ep1").is_none());
        assert!(parse("anime-1-s1e1").is_none());
        assert!(parse("series-1-ep1").is_none());
        assert!(parse("series-1-s1").is_none());
        assert!(parse("book-1").is_none());
        assert!(parse("anime-").is_none());
        assert!(parse("anime-1-ep").is_none());
    }

    #[test]
    fn builds_ids_that_parse_back() {
        assert_eq!(parse(&anime_episode_id("5", 3)).unwrap().episode, Some(3));
        let id = parse("series-tt1-s2e7").unwrap();
        assert_eq!((id.season, id.episode), (Some(2), Some(7)));
    }
}
