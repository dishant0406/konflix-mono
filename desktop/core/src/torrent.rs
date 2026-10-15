//! The torrent engine: joins a swarm, picks the file to play and hands out its bytes.
//!
//! This streams: it does not download the file. No file is ever selected, so the only
//! pieces fetched are the ones a reader asks for and a window ahead of where it is
//! reading. Selecting the file made the engine fetch every remaining piece at full speed
//! behind the playhead, which is downloading with playback on top. What has been read
//! stays on disk until the torrent is evicted or the app quits. Disk and sockets are
//! bounded by evicting the oldest torrent.

use crate::config::Config;
use crate::model::Release;
use crate::releases::{MIN_VIDEO_BYTES, VideoFile, pick_video_file};
use anyhow::{Context, bail};
use librqbit::api::TorrentIdOrHash;
use librqbit::dht::Id20;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions,
};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

type Handle = Arc<ManagedTorrent>;

/// A readable, seekable view of the chosen file.
pub trait FileReader: AsyncRead + AsyncSeek + Send + Unpin {}
impl<T: AsyncRead + AsyncSeek + Send + Unpin> FileReader for T {}

/// The file chosen out of a torrent. A pack holds many episodes, so the hash alone does
/// not name what plays: the file index does.
#[derive(Debug, Clone, PartialEq)]
pub struct Playable {
    pub info_hash: String,
    pub file_index: usize,
    pub file_name: String,
    pub length: u64,
}

/// One playable file. Several entries share a torrent when a pack serves more than one
/// of its episodes.
struct Entry {
    playable: Playable,
    torrent_id: usize,
    handle: Handle,
    last_used: Instant,
    /// How many responses are reading this file right now.
    readers: Arc<AtomicUsize>,
    /// A stream held open for as long as the file is playable. librqbit disconnects from
    /// the peers it no longer needs once nothing is being read, and then waits on a slow
    /// reconnect for the next read, which is what a seek is. An open stream keeps them.
    _hold: Option<Box<dyn FileReader>>,
}

/// Held while a response reads from a torrent, so eviction leaves it alone.
pub struct Reader(Arc<AtomicUsize>);

impl Drop for Reader {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Deletes a torrent from the session when dropped, unless disarmed. `open` is dropped
/// when the viewer leaves the watch page, and without this a torrent added by then
/// would stay in the session, downloading and holding a slot, with no entry to evict it.
struct AddedTorrent {
    session: Arc<Session>,
    hash: Option<Id20>,
}

impl AddedTorrent {
    fn disarm(mut self) {
        self.hash = None;
    }
}

impl Drop for AddedTorrent {
    fn drop(&mut self) {
        let Some(hash) = self.hash else { return };
        let session = self.session.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = session.delete(TorrentIdOrHash::Hash(hash), true).await {
                    tracing::debug!("[torrent] delete failed: {error:#}");
                }
            });
        }
    }
}

pub struct Engine {
    session: Arc<Session>,
    max_torrents: usize,
    max_cache_bytes: u64,
    metadata_timeout: Duration,
    active: Mutex<Vec<Entry>>,
}

/// How a stream is going, for the "buffering" readout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamStats {
    pub downloaded_bytes: u64,
    pub bytes_per_second: u64,
    pub peers: u32,
}

/// How much of each end of the file is fetched as soon as it is chosen. Players read the
/// start, and look for an index at the end, before they show a frame. Asking for both up
/// front makes those reads local.
const WARM_UP_BYTES: u64 = 3 * 1024 * 1024;
/// How long a swarm gets to deliver the start of the file before it is called slow. A slow
/// swarm is still used when nothing faster turns up: the wait only lets a faster candidate
/// that is racing it win.
const HEAD_WAIT: Duration = Duration::from_secs(15);

/// The directory the engine owns inside the user's chosen one. It is the only place
/// the engine deletes from, so a `cache_dir` that holds other files is never wiped.
const DOWNLOADS_DIR: &str = "akidev-torrents";

impl Engine {
    /// Where downloads live for a given base directory.
    pub fn downloads_dir(base: &Path) -> PathBuf {
        base.join(DOWNLOADS_DIR)
    }

    /// Starts a session whose downloads live under `downloads_dir(base)`. That
    /// directory is emptied first: it holds scratch data from a previous run, and
    /// deleting it costs nothing but a re-download.
    pub async fn start(config: &Config, base: &Path) -> anyhow::Result<Engine> {
        let dir = Engine::downloads_dir(base);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let session = Session::new_with_opts(
            dir,
            SessionOptions {
                persistence: None,
                fastresume: false,
                ratelimits: librqbit::limits::LimitsConfig {
                    upload_bps: config.upload_limit_bps.and_then(std::num::NonZeroU32::new),
                    download_bps: None,
                },
                ..Default::default()
            },
        )
        .await?;
        Ok(Engine::new(session, config))
    }

    fn new(session: Arc<Session>, config: &Config) -> Engine {
        Engine {
            session,
            max_torrents: config.max_torrents.max(1),
            max_cache_bytes: config.max_cache_bytes,
            metadata_timeout: Duration::from_millis(config.metadata_timeout_ms),
            active: Mutex::new(Vec::new()),
        }
    }

    /// Joins the release's swarm and chooses the file to play.
    ///
    /// `budget` is what is left of the caller's overall deadline: waiting for peers is
    /// clamped to it, so one slow swarm cannot push the whole request past its limit.
    /// `fresh` drops anything cached for this release first, so a retry after a dead
    /// swarm does not get the same dead torrent back. Several opens may run at once, which
    /// is how the resolver races its candidates.
    pub async fn open(
        &self,
        release: &Release,
        episode: Option<u32>,
        season: Option<u32>,
        budget: Duration,
        fresh: bool,
        trackers: &[String],
    ) -> anyhow::Result<Playable> {
        self.open_with_peers(release, episode, season, budget, fresh, trackers, &[])
            .await
    }

    /// `open`, also trying `peers` directly. Tests use it to reach a seeder on localhost.
    #[allow(clippy::too_many_arguments)]
    async fn open_with_peers(
        &self,
        release: &Release,
        episode: Option<u32>,
        season: Option<u32>,
        budget: Duration,
        fresh: bool,
        trackers: &[String],
        peers: &[SocketAddr],
    ) -> anyhow::Result<Playable> {
        let hash = Id20::from_str(&release.info_hash).context("bad info hash")?;

        if fresh {
            let dropped = {
                let mut active = self.active.lock().unwrap();
                let before = active.len();
                active.retain(|e| e.playable.info_hash != release.info_hash);
                active.len() != before
            };
            if dropped {
                self.remove(TorrentIdOrHash::Hash(hash)).await;
            }
        }

        // A pack already in the session is reused: the torrent is added once and only
        // the file choice is redone, because each episode is a different file.
        let existing = self
            .active
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.playable.info_hash == release.info_hash)
            .map(|e| (e.torrent_id, e.handle.clone(), e.readers.clone()));
        if let Some((torrent_id, handle, readers)) = existing {
            // Pinned while the new file is chosen, so a racing open cannot evict the
            // torrent underneath it. The sibling entry's stream already keeps the swarm
            // connected, so no further stream is held for this file.
            readers.fetch_add(1, Ordering::SeqCst);
            let _pin = Reader(readers);
            let (file_index, playable) =
                tokio::time::timeout(budget, self.choose_file(&handle, release, episode, season))
                    .await
                    .context("timed out choosing a file")??;
            fetch_start(&handle, file_index).await;
            return Ok(self.commit(torrent_id, handle, file_index, playable, None));
        }

        // A torrent dropped a moment ago may still be deleting; adding it again now would
        // attach to the one being removed.
        for _ in 0..40 {
            if self.session.get(TorrentIdOrHash::Hash(hash)).is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        self.make_room().await;

        let wait = self.metadata_timeout.min(budget);
        let options = AddTorrentOptions {
            // Nothing is selected: see the module docs.
            only_files: Some(Vec::new()),
            overwrite: true,
            trackers: Some(trackers.to_vec()),
            initial_peers: (!peers.is_empty()).then(|| peers.to_vec()),
            ..Default::default()
        };
        // Armed before the add: if this future is dropped from here until the entry is
        // committed, the torrent is deleted by hash whatever stage it had reached.
        let guard = AddedTorrent {
            session: self.session.clone(),
            hash: Some(hash),
        };
        let added = tokio::time::timeout(
            wait,
            self.session
                .add_torrent(AddTorrent::from_url(&release.magnet), Some(options)),
        )
        .await;

        let (torrent_id, handle) = match added {
            Ok(Ok(
                AddTorrentResponse::Added(id, handle)
                | AddTorrentResponse::AlreadyManaged(id, handle),
            )) => (id, handle),
            Ok(Ok(AddTorrentResponse::ListOnly(_))) => {
                guard.disarm();
                self.remove(TorrentIdOrHash::Hash(hash)).await;
                bail!("unexpected list-only response")
            }
            Ok(Err(error)) => {
                guard.disarm();
                bail!("{error:#}")
            }
            Err(_) => {
                // The add may have left a half-started torrent behind. Awaited here
                // rather than left to the guard, so an immediate retry cannot race it.
                guard.disarm();
                self.remove(TorrentIdOrHash::Hash(hash)).await;
                bail!("timed out waiting for peers");
            }
        };

        let chosen =
            match tokio::time::timeout(wait, self.choose_file(&handle, release, episode, season))
                .await
            {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!("timed out reading the file list")),
            };
        match chosen {
            Ok((file_index, playable)) => {
                // Until the swarm has delivered the start of the file, this future can be
                // dropped (a faster candidate won), and the guard then deletes the torrent.
                let held = tokio::time::timeout(
                    HEAD_WAIT,
                    hold_open(&handle, file_index, playable.length),
                )
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("timed out opening the file")));
                fetch_start(&handle, file_index).await;
                match held {
                    Ok(hold) => {
                        let playable =
                            self.commit(torrent_id, handle, file_index, playable, Some(hold));
                        guard.disarm();
                        Ok(playable)
                    }
                    Err(error) => {
                        guard.disarm();
                        self.remove(TorrentIdOrHash::Id(torrent_id)).await;
                        Err(error)
                    }
                }
            }
            Err(error) => {
                guard.disarm();
                self.remove(TorrentIdOrHash::Id(torrent_id)).await;
                Err(error)
            }
        }
    }

    /// Records the chosen file as playable, or refreshes it when it already is.
    fn commit(
        &self,
        torrent_id: usize,
        handle: Handle,
        file_index: usize,
        playable: Playable,
        hold: Option<Box<dyn FileReader>>,
    ) -> Playable {
        let mut active = self.active.lock().unwrap();
        if let Some(entry) = active.iter_mut().find(|e| {
            e.playable.info_hash == playable.info_hash && e.playable.file_index == file_index
        }) {
            entry.last_used = Instant::now();
            return entry.playable.clone();
        }
        tokio::spawn(fetch_end(handle.clone(), file_index, playable.length));
        active.push(Entry {
            playable: playable.clone(),
            torrent_id,
            handle,
            last_used: Instant::now(),
            readers: Arc::new(AtomicUsize::new(0)),
            _hold: hold,
        });
        playable
    }

    async fn choose_file(
        &self,
        handle: &Handle,
        release: &Release,
        episode: Option<u32>,
        season: Option<u32>,
    ) -> anyhow::Result<(usize, Playable)> {
        let files: Vec<VideoFile> = handle.with_metadata(|metadata| {
            metadata
                .file_infos
                .iter()
                .enumerate()
                .map(|(index, file)| VideoFile {
                    index,
                    name: file.relative_filename.to_string_lossy().into_owned(),
                    length: file.len,
                })
                .collect()
        })?;

        let Some(file) = pick_video_file(&files, episode, MIN_VIDEO_BYTES, season) else {
            bail!("no playable video file in \"{}\"", release.title);
        };

        handle.wait_until_initialized().await?;
        let name = file
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&file.name)
            .to_string();
        Ok((
            file.index,
            Playable {
                info_hash: release.info_hash.clone(),
                file_index: file.index,
                file_name: name,
                length: file.length,
            },
        ))
    }

    /// Opens a reader at the start of a playable file.
    pub async fn stream(
        &self,
        info_hash: &str,
        file_index: usize,
    ) -> anyhow::Result<(Playable, Box<dyn FileReader>, Reader)> {
        let (handle, playable, readers) = {
            let mut active = self.active.lock().unwrap();
            let entry = active
                .iter_mut()
                .find(|e| e.playable.info_hash == info_hash && e.playable.file_index == file_index)
                .context("this stream has expired")?;
            entry.last_used = Instant::now();
            // Counted under the lock that eviction also takes, so the entry is either
            // pinned here or already gone.
            entry.readers.fetch_add(1, Ordering::SeqCst);
            (
                entry.handle.clone(),
                entry.playable.clone(),
                entry.readers.clone(),
            )
        };
        let reader = Reader(readers);
        let stream = handle.stream(playable.file_index).await?;
        Ok((playable, Box::new(stream), reader))
    }

    /// How many torrents are in the session.
    pub fn active_torrents(&self) -> usize {
        let active = self.active.lock().unwrap();
        let mut ids: Vec<usize> = active.iter().map(|e| e.torrent_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// How a stream is going, or `None` once it is gone.
    pub fn stats(&self, info_hash: &str, file_index: usize) -> Option<StreamStats> {
        let handle = self
            .active
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.playable.info_hash == info_hash && e.playable.file_index == file_index)
            .map(|e| e.handle.clone())?;
        let live = handle.stats().live?;
        Some(StreamStats {
            downloaded_bytes: live.snapshot.downloaded_and_checked_bytes,
            bytes_per_second: live.download_speed.as_bytes(),
            peers: live.snapshot.peer_stats.live,
        })
    }

    /// Frees room for one more torrent. A torrent being read is never dropped: the
    /// limit is allowed to be exceeded until its response ends.
    ///
    /// Both limits are checked because a count alone does not bound disk: a 1080p
    /// episode is roughly 1.4GB.
    async fn make_room(&self) {
        loop {
            // Picking the victim and removing its entries happen under one lock, so a
            // reader cannot start between the "nobody is reading" check and the removal.
            let victim = {
                let mut active = self.active.lock().unwrap();
                let bytes: u64 = active.iter().map(|entry| entry.playable.length).sum();
                let mut torrents: Vec<usize> = active.iter().map(|e| e.torrent_id).collect();
                torrents.sort_unstable();
                torrents.dedup();
                if torrents.len() < self.max_torrents && bytes < self.max_cache_bytes {
                    return;
                }
                // A torrent is idle when none of its files is being read.
                let idle = |id: usize| {
                    active
                        .iter()
                        .filter(|e| e.torrent_id == id)
                        .all(|e| e.readers.load(Ordering::SeqCst) == 0)
                };
                let victim = active
                    .iter()
                    .filter(|entry| idle(entry.torrent_id))
                    .min_by_key(|entry| entry.last_used)
                    .map(|entry| entry.torrent_id);
                if let Some(torrent_id) = victim {
                    active.retain(|entry| entry.torrent_id != torrent_id);
                }
                victim
            };
            let Some(torrent_id) = victim else { return };
            self.remove(TorrentIdOrHash::Id(torrent_id)).await;
        }
    }

    /// Drops every torrent that nobody has read from for `idle`, and its files. A server calls
    /// this on a timer, so an episode watched an hour ago does not hold a slot and its disk.
    pub async fn sweep_idle(&self, idle: Duration) -> usize {
        let victims: Vec<usize> = {
            let mut active = self.active.lock().unwrap();
            let mut torrents: Vec<usize> = active.iter().map(|e| e.torrent_id).collect();
            torrents.sort_unstable();
            torrents.dedup();
            let stale: Vec<usize> = torrents
                .into_iter()
                .filter(|&id| {
                    active.iter().filter(|e| e.torrent_id == id).all(|e| {
                        e.readers.load(Ordering::SeqCst) == 0 && e.last_used.elapsed() >= idle
                    })
                })
                .collect();
            active.retain(|entry| !stale.contains(&entry.torrent_id));
            stale
        };
        for &id in &victims {
            self.remove(TorrentIdOrHash::Id(id)).await;
        }
        victims.len()
    }

    /// Deletes a torrent and its files. The deletion runs as its own task, so it still
    /// completes when the caller is dropped half way, which happens when the viewer
    /// leaves the page that was waiting.
    async fn remove(&self, id: TorrentIdOrHash) {
        let session = self.session.clone();
        let deleted = tokio::spawn(async move {
            if let Err(error) = session.delete(id, true).await {
                tracing::debug!("[torrent] delete failed: {error:#}");
            }
        });
        let _ = deleted.await;
    }
}

/// Opens a stream parked at the end of the file: it asks for no pieces, and counts as a
/// reader, which keeps the swarm connected.
async fn hold_open(
    handle: &Handle,
    file_index: usize,
    length: u64,
) -> anyhow::Result<Box<dyn FileReader>> {
    let mut stream = handle.clone().stream(file_index).await?;
    stream.seek(std::io::SeekFrom::Start(length)).await?;
    Ok(Box::new(stream))
}

/// Reads `bytes` from `from` in a file, which makes the engine fetch those pieces first.
async fn read_range(
    handle: Handle,
    file_index: usize,
    from: u64,
    bytes: u64,
) -> anyhow::Result<()> {
    let mut stream = handle.stream(file_index).await?;
    stream.seek(std::io::SeekFrom::Start(from)).await?;
    let mut remaining = bytes;
    let mut buffer = vec![0u8; 256 * 1024];
    while remaining > 0 {
        let want = remaining.min(buffer.len() as u64) as usize;
        let got = stream.read(&mut buffer[..want]).await?;
        if got == 0 {
            break;
        }
        remaining -= got as u64;
    }
    Ok(())
}

/// Waits for the start of the file, up to `HEAD_WAIT`. It is the first thing a player reads,
/// and how quickly it arrives says whether the swarm can keep up with playback.
async fn fetch_start(handle: &Handle, file_index: usize) {
    let _ = tokio::time::timeout(
        HEAD_WAIT,
        read_range(handle.clone(), file_index, 0, WARM_UP_BYTES),
    )
    .await;
}

/// Fetches the end of the file in the background: players look there for an index (an MP4's
/// moov, an MKV's cues) before showing a frame. Failure only means the player waits instead.
async fn fetch_end(handle: Handle, file_index: usize, length: u64) {
    let from = length.saturating_sub(WARM_UP_BYTES);
    let _ = tokio::time::timeout(
        Duration::from_secs(60),
        read_range(handle, file_index, from, length - from),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use librqbit::spawn_utils::BlockingSpawner;
    use librqbit::{CreateTorrentOptions, ListenerOptions, create_torrent};

    /// A session that touches no network, holding one tiny torrent that is added from
    /// bytes, so no peers are needed.
    async fn session_with_torrent(name: &str) -> (Arc<Session>, Id20, PathBuf) {
        let dir = std::env::temp_dir().join(format!("akidev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let session = Session::new_with_opts(
            dir.clone(),
            SessionOptions {
                dht: None,
                disable_trackers: true,
                persistence: None,
                fastresume: false,
                listen: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut bytes =
            b"d4:infod6:lengthi5e4:name5:a.mkv12:piece lengthi16384e6:pieces20:".to_vec();
        bytes.extend_from_slice(&[7u8; 20]);
        bytes.extend_from_slice(b"ee");
        let response = session
            .add_torrent(
                AddTorrent::from_bytes(bytes),
                Some(AddTorrentOptions {
                    paused: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        let AddTorrentResponse::Added(_, handle) = response else {
            panic!("expected the torrent to be added");
        };
        (session, handle.info_hash(), dir)
    }

    async fn torrent_count(session: &Session) -> usize {
        session.with_torrents(|torrents| torrents.count())
    }

    #[tokio::test]
    async fn dropping_an_armed_guard_deletes_the_torrent() {
        let (session, hash, dir) = session_with_torrent("armed").await;
        assert_eq!(torrent_count(&session).await, 1);
        drop(AddedTorrent {
            session: session.clone(),
            hash: Some(hash),
        });
        for _ in 0..50 {
            if torrent_count(&session).await == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(torrent_count(&session).await, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_disarmed_guard_leaves_the_torrent() {
        let (session, hash, dir) = session_with_torrent("disarmed").await;
        AddedTorrent {
            session: session.clone(),
            hash: Some(hash),
        }
        .disarm();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(torrent_count(&session).await, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A file of incompressible bytes, so nothing about the transfer is shortened.
    fn write_noise(path: &Path, bytes: usize) {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let data: Vec<u8> = (0..bytes)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 32) as u8
            })
            .collect();
        std::fs::write(path, data).unwrap();
    }

    /// The point of the engine: playing a file must fetch what is read and a window ahead of
    /// it, not the whole file. Selecting the file made a seeder on this machine hand over all
    /// 128 MiB within seconds while only 6 MiB had been read.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn streaming_fetches_a_window_and_not_the_whole_file() {
        const FILE: usize = 128 * 1024 * 1024;
        const READ: u64 = 6 * 1024 * 1024;
        let base = std::env::temp_dir().join(format!("akidev-stream-{}", std::process::id()));
        let (seed_dir, leech_dir) = (base.join("seed"), base.join("leech"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&seed_dir).unwrap();
        std::fs::create_dir_all(&leech_dir).unwrap();
        write_noise(&seed_dir.join("movie.mkv"), FILE);

        let local = |dir: &Path, listen: bool| {
            let dir = dir.to_path_buf();
            async move {
                Session::new_with_opts(
                    dir,
                    SessionOptions {
                        dht: None,
                        disable_trackers: true,
                        persistence: None,
                        fastresume: false,
                        listen: listen.then(|| ListenerOptions {
                            listen_addr: "127.0.0.1:0".parse().unwrap(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
            }
        };

        let torrent = create_torrent(
            &seed_dir.join("movie.mkv"),
            CreateTorrentOptions {
                name: None,
                trackers: Vec::new(),
                piece_length: Some(256 * 1024),
            },
            &BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let seeder = local(&seed_dir, true).await;
        let seeding = seeder
            .add_torrent(
                AddTorrent::from_bytes(torrent.as_bytes().unwrap()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        seeding.wait_until_completed().await.unwrap();

        let engine = Engine::new(local(&leech_dir, false).await, &Config::default());
        let hash = torrent.info_hash().as_string();
        let release = Release {
            title: "Movie 2024".into(),
            info_hash: hash.clone(),
            magnet: format!("magnet:?xt=urn:btih:{hash}"),
            size_bytes: None,
            seeders: 1,
            source: "test".into(),
        };
        let playable = engine
            .open_with_peers(
                &release,
                None,
                None,
                Duration::from_secs(30),
                false,
                &[],
                &[seeder.listen_addr().unwrap()],
            )
            .await
            .unwrap();
        assert_eq!(playable.length, FILE as u64);

        let started = Instant::now();
        let (_, mut reader, _pinned) = engine.stream(&hash, playable.file_index).await.unwrap();
        let mut buffer = vec![0u8; 256 * 1024];
        let mut read = 0;
        while read < READ {
            read += reader.read(&mut buffer).await.unwrap() as u64;
        }
        let first_read = started.elapsed();
        // Long enough for an engine that downloads everything to have done so.
        tokio::time::sleep(Duration::from_secs(4)).await;

        let stats = engine.stats(&hash, playable.file_index).unwrap();
        let fetched = stats.downloaded_bytes;

        // The peers are idle now. A read or seek must still get its bytes at once, and not wait
        // for an idle peer's five second poll: librqbit as published takes 5s for the first and
        // a second or more for the second.
        let asked = Instant::now();
        reader
            .seek(std::io::SeekFrom::Start(FILE as u64 * 7 / 10))
            .await
            .unwrap();
        reader.read_exact(&mut buffer).await.unwrap();
        let waited = asked.elapsed();

        let _ = std::fs::remove_dir_all(&base);
        assert!(
            first_read < Duration::from_millis(1500),
            "the first read took {first_read:?}"
        );
        assert!(
            waited < Duration::from_millis(750),
            "a seek waited {waited:?}"
        );
        assert!(fetched >= READ, "fetched only {fetched} bytes");
        assert!(
            fetched < FILE as u64 / 2,
            "fetched {fetched} of {FILE} bytes after reading {READ}: that is downloading, not streaming"
        );
    }

    #[test]
    fn downloads_live_in_a_subdirectory_the_engine_owns() {
        let base = Path::new("/home/user/videos");
        assert_eq!(
            Engine::downloads_dir(base),
            Path::new("/home/user/videos/akidev-torrents")
        );
    }
}
