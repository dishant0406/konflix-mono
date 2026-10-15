//! Turns a media id into a URL a browser can play.
//!
//!   media id -> title -> indexer search -> rank releases
//!     -> each configured source in turn: debrid, the torrent engine, the Internet Archive
//!     -> read the file's header, pick the lightest delivery (see `media`)
//!     -> a token, and the URL it unlocks
//!
//! It never fails: when nothing can be played the answer says why, in words a viewer can read.

use crate::config::Settings;
use crate::deliver::{Delivery, Registry};
use crate::dto::Ready;
use crate::hls::{Hls, StartError};
use crate::media::{self, Delivery as Plan};
use crate::origin::Origin;
use crate::sources::{archive, debrid::Debrid};
use akidev_core::model::Release;
use akidev_core::stream::Streamer;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Finding sources at once. More than this would only queue behind the indexers.
const CONCURRENT_RESOLVES: usize = 4;
/// How long a request waits for one of those, before it is told the server is busy.
const QUEUE_FOR: Duration = Duration::from_secs(3);
/// How many torrents are tried in turn when the one that opened cannot be played.
const TORRENT_TRIES: usize = 3;

#[derive(Clone)]
pub enum Outcome {
    Ready(Ready),
    Unavailable(String),
    Busy,
}

pub struct Player {
    settings: Arc<Settings>,
    /// Absent in tests, which stand a file in for a swarm and must not bind the DHT port.
    streamer: Option<Arc<Streamer>>,
    debrid: Option<Arc<Debrid>>,
    hls: Arc<Hls>,
    registry: Arc<Registry>,
    slots: Arc<Semaphore>,
    /// Requests for the same episode made together are answered by one search.
    flights: Mutex<HashMap<String, Shared<BoxFuture<'static, Outcome>>>>,
}

/// Why one source did not give a playable file.
pub(crate) enum Miss {
    Reason(String),
    Busy,
}

impl Player {
    pub fn new(
        settings: Arc<Settings>,
        streamer: Option<Arc<Streamer>>,
        debrid: Option<Arc<Debrid>>,
        hls: Arc<Hls>,
        registry: Arc<Registry>,
    ) -> Arc<Player> {
        Arc::new(Player {
            settings,
            streamer,
            debrid,
            hls,
            registry,
            slots: Arc::new(Semaphore::new(CONCURRENT_RESOLVES)),
            flights: Mutex::new(HashMap::new()),
        })
    }

    pub fn sources(&self) -> Vec<String> {
        self.settings.stream_sources.clone()
    }

    pub fn debrid(&self) -> Option<&Arc<Debrid>> {
        self.debrid.as_ref()
    }

    /// Finds a playable source. `hevc` says the browser can play HEVC, which lets that video be
    /// kept as it is rather than converted.
    pub async fn play(self: &Arc<Self>, id: &str, hevc: bool, start: Option<f64>) -> Outcome {
        // A resume is part of the request, but only to the minute: it decides how far a conversion
        // may run, not what is found.
        let key = format!(
            "{id}|{hevc}|{}",
            start.map_or(0, |seconds| (seconds / 60.) as u64)
        );
        let flight = {
            let mut flights = self.flights.lock().unwrap();
            if let Some(running) = flights.get(&key) {
                running.clone()
            } else {
                let player = self.clone();
                let id = id.to_string();
                let forget = key.clone();
                let flight = async move {
                    let outcome = player.resolve(&id, hevc, start).await;
                    player.flights.lock().unwrap().remove(&forget);
                    outcome
                }
                .boxed()
                .shared();
                flights.insert(key.clone(), flight.clone());
                flight
            }
        };
        flight.await
    }

    async fn resolve(self: &Arc<Self>, id: &str, hevc: bool, start: Option<f64>) -> Outcome {
        let Ok(Ok(_slot)) =
            tokio::time::timeout(QUEUE_FOR, self.slots.clone().acquire_owned()).await
        else {
            return Outcome::Busy;
        };

        let Some(streamer) = &self.streamer else {
            return Outcome::Unavailable("This server has no torrent engine.".into());
        };
        let mut plan = match streamer.plan(id).await {
            Ok(plan) => plan,
            Err(reason) => return Outcome::Unavailable(reason),
        };
        // What costs this server least is tried first: a release a browser can play without a
        // re-encode (10-bit H.264 is the common one it cannot), and one light enough to download
        // faster than it plays (a REMUX is not). The order is otherwise the library's own.
        let movie = plan.parsed.kind == akidev_core::model::Kind::Movie;
        plan.ranked
            .sort_by_key(|release| (needs_encode_by_name(&release.title), heavy(release, movie)));

        let deadline = Instant::now() + Duration::from_millis(self.settings.resolve_deadline_ms);
        let episode = plan.parsed.episode;
        let season = plan.parsed.season;
        let mut reasons: Vec<String> = Vec::new();

        for source in &self.settings.stream_sources {
            if Instant::now() >= deadline {
                reasons.push("Timed out looking for a playable source.".into());
                break;
            }
            let attempt = match source.as_str() {
                "debrid" => match &self.debrid {
                    None => Err(Miss::Reason("The debrid source is not configured.".into())),
                    Some(debrid) => match debrid
                        .find(
                            &plan.ranked,
                            episode,
                            season,
                            self.settings.max_candidates,
                            deadline,
                        )
                        .await
                    {
                        Ok(found) => {
                            self.deliver(found.origin, found.release, "debrid", hevc, start)
                                .await
                        }
                        Err(reason) => Err(Miss::Reason(reason)),
                    },
                },
                "torrent" => {
                    // A release whose file turns out to be unreadable or unplayable is dropped and
                    // the next is tried, a few times, instead of giving up on the source.
                    let mut attempt = Err(Miss::Reason("No torrent could be played.".into()));
                    for _ in 0..TORRENT_TRIES {
                        if Instant::now() >= deadline {
                            break;
                        }
                        match streamer.open_best(&plan, false).await {
                            Ok((release, playable)) => {
                                let origin = Origin::Torrent {
                                    engine: streamer.engine(),
                                    info_hash: playable.info_hash.clone(),
                                    file_index: playable.file_index,
                                    length: playable.length,
                                    name: playable.file_name,
                                };
                                attempt = self
                                    .deliver(origin, release.title, "torrent", hevc, start)
                                    .await;
                                match &attempt {
                                    Err(Miss::Reason(_)) => plan
                                        .ranked
                                        .retain(|each| each.info_hash != playable.info_hash),
                                    _ => break,
                                }
                            }
                            Err(reason) => {
                                attempt = Err(Miss::Reason(reason));
                                break;
                            }
                        }
                    }
                    attempt
                }
                "archive-org" => {
                    let titles = if plan.detail.media.title_variants.is_empty() {
                        vec![plan.detail.media.title.clone()]
                    } else {
                        plan.detail.media.title_variants.clone()
                    };
                    match archive::find(
                        &self.settings.archive_org_base,
                        &titles,
                        &plan.terms,
                        episode,
                        season,
                        deadline,
                    )
                    .await
                    {
                        Ok(found) => Ok(Ready {
                            available: true,
                            url: found.url,
                            stream_type: "file",
                            release: found.title,
                            source: "archive-org",
                            duration: None,
                            subtitles: Vec::new(),
                        }),
                        Err(reason) => Err(Miss::Reason(reason)),
                    }
                }
                other => Err(Miss::Reason(format!("Unknown stream source \"{other}\"."))),
            };
            match attempt {
                Ok(ready) => return Outcome::Ready(ready),
                Err(Miss::Busy) => return Outcome::Busy,
                Err(Miss::Reason(reason)) => {
                    tracing::info!("[play] {id}: {source}: {reason}");
                    reasons.push(reason);
                }
            }
        }

        // When no indexer found anything at all, say so: the sources' own "nothing here" reasons
        // are true but describe the fallbacks, not the actual problem. The reason names the first
        // term, because that is what a person would search for by hand to check.
        if plan.ranked.is_empty() {
            let term = plan
                .terms
                .first()
                .cloned()
                .unwrap_or_else(|| plan.detail.media.title.clone());
            return Outcome::Unavailable(format!("No torrent found for \"{term}\"."));
        }
        // The last reason is from the source that got furthest: a source is only asked when the
        // ones before it declined.
        Outcome::Unavailable(
            reasons
                .pop()
                .unwrap_or_else(|| "No stream source could serve this episode.".into()),
        )
    }

    /// Reads the file's header, picks the lightest delivery and hands out a token.
    pub(crate) async fn deliver(
        &self,
        origin: Origin,
        release: String,
        source: &'static str,
        hevc: bool,
        start: Option<f64>,
    ) -> Result<Ready, Miss> {
        let origin = Arc::new(origin);
        let probe = media::probe(&origin).await.map_err(|error| {
            tracing::warn!("[play] {release}: {error:#}");
            Miss::Reason("The file could not be read.".into())
        })?;
        let plan = media::decide(&probe, hevc);
        tracing::info!(
            "[play] {release}: {:?} {:?} -> {plan:?}",
            probe.container,
            probe.video
        );

        let public = &self.settings.public_url;
        match plan {
            Plan::Unsupported(reason) => Err(Miss::Reason(reason)),
            Plan::Direct => {
                let token = self.registry.register(Delivery::Direct { origin });
                Ok(Ready {
                    available: true,
                    url: format!("{public}/s/{token}"),
                    stream_type: "file",
                    release,
                    source,
                    duration: probe.duration,
                    subtitles: Vec::new(),
                })
            }
            Plan::Hls { video, audio } => {
                let session = match self
                    .hls
                    .start(origin, video, audio, probe.duration, start)
                    .await
                {
                    Ok(session) => session,
                    Err(StartError::Busy) => return Err(Miss::Busy),
                    Err(StartError::Failed(reason)) => {
                        tracing::warn!("[play] {release}: {reason}");
                        return Err(Miss::Reason(
                            "This release could not be prepared for playback.".into(),
                        ));
                    }
                };
                if !session.wait_ready(Hls::start_timeout(video)).await {
                    let reason = session.failure().unwrap_or_default();
                    tracing::warn!("[play] {release}: no playlist in time ({reason})");
                    self.hls.destroy(&session);
                    return Err(Miss::Reason(
                        "This release could not be prepared for playback.".into(),
                    ));
                }
                let duration = session.duration().or(probe.duration);
                let token = self.registry.register(Delivery::Hls { session });
                Ok(Ready {
                    available: true,
                    url: format!("{public}/s/{token}/index.m3u8"),
                    stream_type: "hls",
                    release,
                    source,
                    duration,
                    subtitles: Vec::new(),
                })
            }
        }
    }
}

/// Whether a release is too big to download faster than it plays: a REMUX, or a single episode or
/// film of a size that only a fast swarm can keep up with.
pub fn heavy(release: &Release, movie: bool) -> bool {
    static REMUX: OnceLock<Regex> = OnceLock::new();
    let remux = REMUX
        .get_or_init(|| Regex::new(r"(?i)(?:^|[^a-z0-9])(?:bd)?remux(?:$|[^a-z0-9])").unwrap())
        .is_match(&release.title);
    let limit: u64 = if movie { 9 } else { 3 } * 1024 * 1024 * 1024;
    remux || release.size_bytes.is_some_and(|size| size > limit)
}

/// Whether a release's name says it carries 10-bit video.
pub fn needs_encode_by_name(title: &str) -> bool {
    static TEN_BIT: OnceLock<Regex> = OnceLock::new();
    TEN_BIT
        .get_or_init(|| {
            Regex::new(r"(?i)(?:^|[^a-z0-9])(?:hi10p?|10[- ]?bits?|x265|hevc|av1)(?:$|[^a-z0-9])")
                .unwrap()
        })
        .is_match(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_bit_releases_are_the_ones_tried_last() {
        for title in [
            "[Group] Show - 05 (BD 1080p Hi10P FLAC)",
            "[Group] Show - 05 [1080p][10bit]",
            "[LX] Show - 01 [v2][1080p x264 10bits EAC3]",
            "Show.S01E05.1080p.10-bit.x264",
            "Show S01E05 1080p HEVC",
        ] {
            assert!(needs_encode_by_name(title), "{title}");
        }
        for title in [
            "[SubsPlease] Show - 05 (1080p) [ABCD1234].mkv",
            "Show.S01E05.1080p.WEB-DL.x264-GRP",
            "Show 05 720p h264 AAC",
        ] {
            assert!(!needs_encode_by_name(title), "{title}");
        }
    }

    fn release(title: &str, size: Option<u64>) -> Release {
        Release {
            title: title.into(),
            info_hash: "a".repeat(40),
            magnet: String::new(),
            size_bytes: size,
            seeders: 1,
            source: "t".into(),
        }
    }

    #[test]
    fn a_remux_or_a_huge_file_is_heavy() {
        let gib = 1024 * 1024 * 1024;
        assert!(heavy(
            &release("Show S04E01 1080p FLAC AVC REMUX-FraMeSToR", None),
            false
        ));
        assert!(heavy(&release("Show 05 BDRemux", None), false));
        assert!(heavy(&release("Show 05 1080p", Some(4 * gib)), false));
        assert!(!heavy(&release("Show 05 1080p", Some(4 * gib)), true));
        assert!(heavy(&release("Film 1080p", Some(12 * gib)), true));
        assert!(!heavy(
            &release("[SubsPlease] Show - 05 (1080p)", Some(gib)),
            false
        ));
        assert!(!heavy(&release("Show 05 1080p", None), false));
    }

    #[test]
    fn releases_that_need_no_encode_sort_first_and_keep_their_order() {
        let mut titles = vec!["a 10bit", "b 1080p", "c Hi10P", "d 720p"];
        titles.sort_by_key(|title| needs_encode_by_name(title));
        assert_eq!(titles, ["b 1080p", "d 720p", "a 10bit", "c Hi10P"]);
    }
}
