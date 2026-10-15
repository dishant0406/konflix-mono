//! Turns a media id into something a player can open.
//!
//! media id -> title -> indexer search -> rank releases -> race the best swarms
//! -> a localhost URL, or an explicit "nothing available" with the reason.

use crate::config::Config;
use crate::indexers::{SearchOptions, search_all};
use crate::media_id::{self, MediaId};
use crate::model::{Detail, Kind, Release};
use crate::releases::{
    Audio, Codec, is_batch, movie_search_terms, parse_audio, parse_codec, parse_resolution,
    rank_movie_releases, rank_movie_releases_by, rank_releases, rank_releases_by, search_terms,
    series_search_terms,
};
use crate::torrent::{Engine, StreamStats};
use crate::{providers, serve};
use futures::FutureExt as _;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Which file of which torrent is playing, for asking how its stream is going.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamId {
    pub info_hash: String,
    pub file_index: usize,
}

/// A candidate that has not found peers after this long is joined by the next one.
const STAGGER: Duration = Duration::from_secs(6);

/// How many ranked releases a plan offers. More than a torrent is tried with, because a
/// debrid source only needs one of them to be cached and checking a hash is cheap, while
/// opening a torrent is not.
pub const OFFERED: usize = 24;

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    Ready {
        url: String,
        release: String,
        stream: StreamId,
    },
    /// Nothing could be played. A normal outcome, not an error: the UI says so.
    Unavailable { reason: String },
}

pub struct Streamer {
    config: Mutex<Config>,
    engine: Arc<Engine>,
    address: SocketAddr,
    /// Secret in every stream URL, so only the player can read the server.
    token: String,
}

impl Streamer {
    /// Starts the torrent engine, downloading under a directory it owns inside `dir`,
    /// and the localhost server the player reads from.
    pub async fn start(config: Config, dir: PathBuf) -> anyhow::Result<Streamer> {
        let engine = Arc::new(Engine::start(&config, &dir).await?);
        let server = serve::start(engine.clone()).await?;
        Ok(Streamer {
            config: Mutex::new(config),
            engine,
            address: server.address,
            token: server.token,
        })
    }

    /// The torrent engine alone, for a server that reads the files itself and has no player
    /// to hand a localhost address to. `resolve` is not meant to be used on one of these.
    pub async fn engine_only(config: Config, dir: PathBuf) -> anyhow::Result<Streamer> {
        let engine = Arc::new(Engine::start(&config, &dir).await?);
        Ok(Streamer {
            config: Mutex::new(config),
            engine,
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            token: String::new(),
        })
    }

    /// How a stream is going: bytes fetched, speed and peers.
    pub fn stats(&self, stream: &StreamId) -> Option<StreamStats> {
        self.engine.stats(&stream.info_hash, stream.file_index)
    }

    /// Replaces the settings used by later searches. The torrent limits are read once
    /// at start, so they apply after a restart.
    pub fn set_config(&self, config: Config) {
        *self.config.lock().unwrap() = config;
    }

    /// Finds a playable source for one episode or film. Never fails. `fresh` is for a
    /// retry: it drops what is cached for the releases about to be tried, so a swarm
    /// that died is joined again instead of handing back the same dead torrent.
    pub async fn resolve(&self, id: &str, fresh: bool) -> Resolution {
        match self.plan(id).await {
            Ok(plan) => self.resolve_plan(&plan, fresh).await,
            Err(reason) => unavailable(&reason),
        }
    }

    /// Plays the release a viewer picked from `Plan::choices`, and no other. Never fails.
    pub async fn resolve_release(&self, id: &str, info_hash: &str) -> Resolution {
        let plan = match self.plan(id).await {
            Ok(plan) => plan,
            Err(reason) => return unavailable(&reason),
        };
        match plan.only(info_hash) {
            Some(only) => self.resolve_plan(&only, false).await,
            None => unavailable("That release is no longer listed. Pick another."),
        }
    }

    /// Opens the best of a plan's releases and says where to play it.
    pub async fn resolve_plan(&self, plan: &Plan, fresh: bool) -> Resolution {
        match self.open_best(plan, fresh).await {
            Ok((release, playable)) => Resolution::Ready {
                url: serve::url(
                    self.address,
                    &self.token,
                    &playable.info_hash,
                    playable.file_index,
                ),
                release: release.title,
                stream: StreamId {
                    info_hash: playable.info_hash,
                    file_index: playable.file_index,
                },
            },
            Err(reason) => unavailable(&reason),
        }
    }

    /// The title and the ranked releases for one episode or film, before any is opened.
    /// The reason is for the viewer, and is the whole story when it is an `Err`.
    pub async fn plan(&self, id: &str) -> Result<Plan, String> {
        let config = self.config.lock().unwrap().clone();
        let Some(parsed) = media_id::parse(id) else {
            return Err("Unrecognised media id.".into());
        };
        // A title without an episode is a detail page. A film is the exception: it has
        // no episode by definition.
        if parsed.kind != Kind::Movie && parsed.episode.is_none() {
            return Err("No episode was requested.".into());
        }
        let Some(detail) = providers::detail(&parsed.key()).await else {
            return Err("Unknown title.".into());
        };
        let (terms, ranked, extra) = find_releases(&config, &parsed, &detail).await;
        Ok(Plan {
            id: id.to_string(),
            parsed,
            detail,
            terms,
            ranked,
            extra,
        })
    }

    /// Races the best releases of a plan and returns the first that is ready. The rest are
    /// dropped, which deletes their torrents.
    pub async fn open_best(
        &self,
        plan: &Plan,
        fresh: bool,
    ) -> Result<(Release, crate::torrent::Playable), String> {
        let config = self.config.lock().unwrap().clone();
        let id = plan.id.as_str();
        let parsed = &plan.parsed;
        let started = Instant::now();
        let deadline = started + Duration::from_millis(config.resolve_deadline_ms);
        let candidates = candidates(&plan.ranked, config.max_candidates);

        if candidates.is_empty() {
            let term = plan
                .terms
                .first()
                .map(String::as_str)
                .unwrap_or(&plan.detail.media.title);
            return Err(format!("No torrent found for \"{term}\"."));
        }

        tracing::debug!(
            "[stream] {id}: trying {:?}",
            candidates
                .iter()
                .map(|r| (&r.title, r.seeders, r.size_bytes))
                .collect::<Vec<_>>()
        );

        // The best release starts at once and the others join, a few seconds apart, only if
        // it is slow. The first to be ready wins and the rest are dropped, which deletes
        // their torrents. Trying them one after another meant a dead swarm cost its whole
        // timeout before the next was even asked.
        // A candidate also starts early once every one ahead of it has failed.
        let (failures, failed) = tokio::sync::watch::channel(0usize);
        let failures = Arc::new(failures);
        let attempts = candidates.iter().enumerate().map(|(index, release)| {
            let engine = self.engine.clone();
            let trackers = crate::sources::current().trackers.clone();
            let failures = failures.clone();
            let mut failed = failed.clone();
            async move {
                let _ = tokio::time::timeout(
                    STAGGER * index as u32,
                    failed.wait_for(|&count| count >= index),
                )
                .await;
                let left = deadline.saturating_duration_since(Instant::now());
                match engine
                    .open(
                        release,
                        parsed.episode,
                        parsed.season,
                        left,
                        fresh,
                        &trackers,
                    )
                    .await
                {
                    Ok(playable) => Ok((release, playable)),
                    Err(error) => {
                        // Logged rather than shown: it names releases and upstream errors,
                        // and the viewer can act on none of it.
                        tracing::warn!("[stream] {id}: \"{}\": {error:#}", release.title);
                        failures.send_modify(|count| *count += 1);
                        Err(error)
                    }
                }
            }
            .boxed()
        });

        match futures::future::select_ok(attempts).await {
            Ok(((release, playable), _unfinished)) => {
                tracing::info!(
                    "[stream] {id}: \"{}\" after {:?}, from {} candidates",
                    release.title,
                    started.elapsed(),
                    plan.ranked.len()
                );
                Ok((Release::clone(release), playable))
            }
            Err(_) => Err("Found releases, but none could be played.".into()),
        }
    }

    /// The torrent engine, for a caller that reads the files itself.
    pub fn engine(&self) -> Arc<Engine> {
        self.engine.clone()
    }
}

/// What is known about one episode or film before any torrent is opened.
#[derive(Debug, Clone)]
pub struct Plan {
    pub id: String,
    pub parsed: MediaId,
    pub detail: Detail,
    /// The first of these is what a person would search for by hand.
    pub terms: Vec<String>,
    /// Releases found and ranked, best first. A source decides how many to try.
    pub ranked: Vec<Release>,
    /// Releases that fit the title and the episode but that the player cannot be counted on to
    /// decode (HEVC, AV1). Never tried on their own; offered when a viewer picks for themselves.
    pub extra: Vec<Release>,
}

/// The releases worth trying, best first. A count of zero may only mean the indexer does
/// not know it, so releases without seeders are tried last rather than dropped.
fn candidates(ranked: &[Release], limit: usize) -> Vec<&Release> {
    let (seeded, unseeded): (Vec<&Release>, Vec<&Release>) =
        ranked.iter().partition(|release| release.seeders > 0);
    seeded.into_iter().chain(unseeded).take(limit).collect()
}

fn unavailable(reason: &str) -> Resolution {
    Resolution::Unavailable {
        reason: reason.to_string(),
    }
}

/// The search terms, the releases they found that can be played, best first, and the others
/// that fit but may not decode.
async fn find_releases(
    config: &Config,
    parsed: &MediaId,
    detail: &Detail,
) -> (Vec<String>, Vec<Release>, Vec<Release>) {
    let media = &detail.media;
    let titles = if media.title_variants.is_empty() {
        vec![media.title.clone()]
    } else {
        media.title_variants.clone()
    };
    let year = media.year.and_then(|year| u32::try_from(year).ok());
    // Torrentio and EZTV are searched by IMDb id rather than by text, and Torrentio
    // takes the season and episode in its path. Indexers that need none of it ignore it.
    let options = SearchOptions {
        kind: parsed.kind,
        imdb_id: detail.imdb_id.clone(),
        season: parsed.season,
        episode: parsed.episode,
    };

    let (terms, mut ranked, mut all) = match (parsed.kind, parsed.episode) {
        (Kind::Movie, _) => {
            let terms = movie_search_terms(&titles, year);
            let found = search_all(config, &terms, &options).await;
            let ranked = rank_movie_releases(&found, year, &media.title, &titles);
            let all = rank_movie_releases_by(&found, year, &media.title, &titles, false);
            (terms, ranked, all)
        }
        (Kind::Series, Some(episode)) => {
            let terms = series_search_terms(&titles, parsed.season, Some(episode));
            let found = search_all(config, &terms, &options).await;
            let ranked = rank_releases(&found, episode, &media.title, &titles, parsed.season);
            let all =
                rank_releases_by(&found, episode, &media.title, &titles, parsed.season, false);
            (terms, ranked, all)
        }
        (Kind::Anime, Some(episode)) => {
            let terms = search_terms(&titles, Some(episode));
            let found = search_all(config, &terms, &options).await;
            let ranked = rank_releases(&found, episode, &media.title, &titles, parsed.season);
            let all =
                rank_releases_by(&found, episode, &media.title, &titles, parsed.season, false);
            (terms, ranked, all)
        }
        (_, None) => (Vec::new(), Vec::new(), Vec::new()),
    };
    ranked.truncate(config.max_candidates.max(OFFERED));
    all.retain(|release| {
        !ranked
            .iter()
            .any(|kept| kept.info_hash == release.info_hash)
    });
    all.truncate(OFFERED);
    (terms, ranked, all)
}

/// One release a viewer may pick, with what can be told from its name.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Choice {
    /// The info hash, which is what picking one passes back.
    pub id: String,
    pub title: String,
    pub size_bytes: Option<u64>,
    pub seeders: u32,
    /// Which indexer found it.
    pub source: String,
    /// 480, 720, 1080, 2160.
    pub resolution: Option<u32>,
    /// `h264`, `hevc`, `av1`.
    pub codec: Option<&'static str>,
    /// `lossless` (FLAC, DTS, AC-3...) or `compressed` (AAC, MP3, Opus).
    pub audio: Option<&'static str>,
    /// A whole season or series in one torrent, of which the episode is one file.
    pub pack: bool,
    /// Not played unless picked: a codec that may not decode where it is shown.
    pub uncommon: bool,
}

fn choice(release: &Release, uncommon: bool) -> Choice {
    Choice {
        id: release.info_hash.clone(),
        title: release.title.clone(),
        size_bytes: release.size_bytes,
        seeders: release.seeders,
        source: release.source.clone(),
        resolution: parse_resolution(&release.title),
        codec: parse_codec(&release.title).map(|codec| match codec {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Av1 => "av1",
        }),
        audio: parse_audio(&release.title).map(|audio| match audio {
            Audio::Lossless => "lossless",
            Audio::Compressed => "compressed",
        }),
        pack: is_batch(&release.title),
        uncommon,
    }
}

impl Plan {
    /// Every release that fits, in the order they would be tried, and then the ones with a codec
    /// that may not decode. For a viewer who wants to pick one.
    pub fn choices(&self) -> Vec<Choice> {
        self.ranked
            .iter()
            .map(|release| choice(release, false))
            .chain(self.extra.iter().map(|release| choice(release, true)))
            .collect()
    }

    /// The plan with only the release a viewer chose, wherever it was listed.
    pub fn only(&self, info_hash: &str) -> Option<Plan> {
        let release = self
            .ranked
            .iter()
            .chain(&self.extra)
            .find(|release| release.info_hash.eq_ignore_ascii_case(info_hash))?;
        Some(Plan {
            ranked: vec![release.clone()],
            extra: Vec::new(),
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(title: &str, seeders: u32) -> Release {
        Release {
            title: title.into(),
            info_hash: "a".repeat(40),
            magnet: String::new(),
            size_bytes: None,
            seeders,
            source: "test".into(),
        }
    }

    fn titles(found: Vec<&Release>) -> Vec<&str> {
        found
            .into_iter()
            .map(|release| release.title.as_str())
            .collect()
    }

    #[test]
    fn a_release_nobody_seeds_is_tried_last() {
        let ranked = [
            release("dead", 0),
            release("alive", 4),
            release("also alive", 1),
        ];
        assert_eq!(
            titles(candidates(&ranked, 3)),
            ["alive", "also alive", "dead"]
        );
        assert_eq!(titles(candidates(&ranked, 2)), ["alive", "also alive"]);
    }

    #[test]
    fn unseeded_releases_are_tried_when_they_are_all_there_is() {
        let ranked = [release("a", 0), release("b", 0)];
        assert_eq!(titles(candidates(&ranked, 3)), ["a", "b"]);
    }

    #[test]
    fn only_the_best_few_are_tried() {
        let ranked = [
            release("a", 9),
            release("b", 8),
            release("c", 7),
            release("d", 6),
        ];
        assert_eq!(titles(candidates(&ranked, 2)), ["a", "b"]);
        assert!(candidates(&[], 3).is_empty());
    }

    fn plan_with(ranked: Vec<Release>, extra: Vec<Release>) -> Plan {
        use crate::model::Media;
        Plan {
            id: "series-1-s1e1".into(),
            parsed: media_id::parse("series-1-s1e1").unwrap(),
            detail: Detail {
                media: Media {
                    id: "series-1".into(),
                    kind: Kind::Series,
                    title: "Show".into(),
                    title_variants: Vec::new(),
                    poster: None,
                    banner: None,
                    description: None,
                    format: None,
                    year: None,
                    episodes: None,
                    runtime_minutes: None,
                    status: None,
                    score: None,
                    popularity: 0,
                    genres: Vec::new(),
                    is_adult: false,
                },
                studios: Vec::new(),
                trailer: None,
                episodes: Vec::new(),
                cast: Vec::new(),
                related: Vec::new(),
                recommended: Vec::new(),
                imdb_id: None,
            },
            terms: Vec::new(),
            ranked,
            extra,
        }
    }

    fn named(title: &str, hash: char) -> Release {
        Release {
            info_hash: hash.to_string().repeat(40),
            ..release(title, 3)
        }
    }

    #[test]
    fn what_can_be_told_from_a_name_is_listed_and_the_uncommon_come_last() {
        let plan = plan_with(
            vec![named("Show S01E01 1080p WEB-DL x264 AAC", 'a')],
            vec![named("Show S01E01 2160p HEVC FLAC", 'b')],
        );
        let choices = plan.choices();
        assert_eq!(choices.len(), 2);
        assert_eq!(
            (
                choices[0].resolution,
                choices[0].codec,
                choices[0].audio,
                choices[0].uncommon
            ),
            (Some(1080), Some("h264"), Some("compressed"), false)
        );
        assert_eq!(
            (
                choices[1].resolution,
                choices[1].codec,
                choices[1].audio,
                choices[1].uncommon
            ),
            (Some(2160), Some("hevc"), Some("lossless"), true)
        );
        assert_eq!(choices[0].id, "a".repeat(40));
    }

    #[test]
    fn a_release_that_was_picked_is_the_only_one_tried_wherever_it_was_listed() {
        let plan = plan_with(
            vec![named("one", 'a'), named("two", 'b')],
            vec![named("three hevc", 'c')],
        );
        for (hash, title) in [('b', "two"), ('c', "three hevc")] {
            let only = plan
                .only(&hash.to_string().repeat(40).to_uppercase())
                .unwrap();
            assert_eq!(only.ranked.len(), 1);
            assert_eq!(only.ranked[0].title, title);
            assert!(only.extra.is_empty());
        }
        assert!(plan.only(&"d".repeat(40)).is_none());
    }
}
