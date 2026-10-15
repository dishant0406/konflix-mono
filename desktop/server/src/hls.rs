//! Serves a file a browser cannot play as it is (an MKV, or a codec it lacks) as HLS: fragmented
//! MP4 segments and a growing playlist, which every browser including iOS plays and seeks in.
//!
//! Why not one live fragmented MP4: iOS refuses a media source that cannot answer a byte-range
//! request with a real 206 and a length, and a stream written as it goes has neither. Segments are
//! complete files, so every response is an ordinary 200 with a Content-Length.
//!
//! ffmpeg reads the file from its stdin, fed by this server, because a torrent's file on disk is
//! sparse until its pieces arrive and ffmpeg reading a path stops at the first gap.
//!
//! Three things keep it light on a small server:
//!   - what ffmpeg is asked to do is the least that works (see `media`);
//!   - the feed stops while the viewer is far enough behind the conversion, so an episode that is
//!     abandoned halfway is not downloaded or written to the end;
//!   - sessions are few, expire when idle, and give their disk back.

use crate::config::Settings;
use crate::media::{AudioMode, VideoMode, codec_args};
use crate::origin::Origin;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tokio::task::JoinHandle;

/// How far ahead of the viewer a copy may run, in segments (a quarter of an hour). Copying costs
/// disk and bandwidth, not CPU, and a longer lead is what lets the viewer scrub forward: a player
/// can only seek within the segments that exist.
const AHEAD: usize = 150;
/// An encode runs this far ahead at most: every segment beyond it is CPU spent on video nobody may
/// watch, and an encode is the one expensive thing here.
const ENCODE_AHEAD: usize = 8;
/// The length ffmpeg is asked to cut segments to.
const SEGMENT_SECONDS: f64 = 6.;
/// A session idle this long can be evicted to make room for a new one.
const EVICTABLE_AFTER: Duration = Duration::from_secs(90);
const START_TIMEOUT: Duration = Duration::from_secs(30);
const ENCODE_START_TIMEOUT: Duration = Duration::from_secs(60);
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(150);

fn clock() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[derive(Debug)]
pub enum StartError {
    /// At the limit of sessions or of encodes: try again shortly.
    Busy,
    Failed(String),
}

#[derive(Debug, PartialEq)]
pub enum SegmentError {
    Invalid,
    Expired,
    NotYet,
}

pub struct Session {
    id: String,
    key: String,
    dir: PathBuf,
    /// How far ahead of the viewer the conversion may run, in segments.
    ahead: usize,
    /// The file's average rate, when its length and duration are known. The feed is paced by it,
    /// which does not lag behind ffmpeg the way counting finished segments does.
    bytes_per_second: Option<f64>,
    last_used: AtomicU64,
    /// The highest segment the viewer has asked for, plus one. Zero before any request.
    requested: AtomicUsize,
    /// Segments ffmpeg has listed so far.
    listed: AtomicUsize,
    finished: AtomicBool,
    dead: AtomicBool,
    duration: Mutex<Option<f64>>,
    reason: Mutex<Option<String>>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Held for the life of a session that re-encodes video, so only one runs at once.
    _permit: Option<OwnedSemaphorePermit>,
}

impl Session {
    /// Where the viewer means to start, so a resume deep into the file lets the conversion run
    /// that far instead of waiting for a player that cannot ask for segments that do not exist yet.
    pub fn begin_at(&self, start: Option<f64>) {
        if let Some(seconds) = start.filter(|s| s.is_finite() && *s > 0.) {
            let segment = (seconds / SEGMENT_SECONDS) as usize;
            self.requested.fetch_max(segment, Ordering::Relaxed);
        }
    }

    pub fn touch(&self) {
        self.last_used.store(clock(), Ordering::Relaxed);
    }

    fn idle(&self) -> Duration {
        Duration::from_millis(clock().saturating_sub(self.last_used.load(Ordering::Relaxed)))
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    pub fn duration(&self) -> Option<f64> {
        *self.duration.lock().unwrap()
    }

    pub fn failure(&self) -> Option<String> {
        self.reason.lock().unwrap().clone()
    }

    fn playlist_path(&self) -> PathBuf {
        self.dir.join("index.m3u8")
    }

    async fn read_playlist(&self) -> Option<Playlist> {
        let text = tokio::fs::read_to_string(self.playlist_path()).await.ok()?;
        Some(Playlist::parse(&text))
    }

    /// Waits until the first segment is listed, or the conversion fails or times out.
    pub async fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.is_dead() {
                return false;
            }
            if self.listed.load(Ordering::Relaxed) > 0 || self.finished.load(Ordering::Relaxed) {
                return true;
            }
            if let Some(playlist) = self.read_playlist().await
                && !playlist.segments.is_empty()
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
    }
}

/// What ffmpeg has written to the playlist so far.
#[derive(Debug, Default, PartialEq)]
pub struct Playlist {
    pub segments: Vec<String>,
    pub finished: bool,
    pub duration: f64,
}

impl Playlist {
    /// The playlist is rewritten while ffmpeg works, so a read can catch a partial line. Only
    /// complete, known lines are matched, which makes a torn read harmless.
    pub fn parse(text: &str) -> Playlist {
        let mut playlist = Playlist::default();
        for line in text.lines().map(str::trim) {
            if is_segment_name(line) {
                playlist.segments.push(line.to_string());
            } else if line == "#EXT-X-ENDLIST" {
                playlist.finished = true;
            } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
                let seconds = rest.split(',').next().and_then(|s| s.parse::<f64>().ok());
                playlist.duration += seconds.unwrap_or(0.);
            }
        }
        playlist
    }
}

/// A playlist that is still growing is treated as live, and a live player starts near the end of
/// it, which for this one means wherever the conversion has got to. This says to start at 0.
pub fn start_at_the_beginning(playlist: String) -> String {
    if playlist.contains("#EXT-X-START") {
        return playlist;
    }
    match playlist.split_once('\n') {
        Some((first, rest)) if first.trim() == "#EXTM3U" => {
            format!("#EXTM3U\n#EXT-X-START:TIME-OFFSET=0,PRECISE=YES\n{rest}")
        }
        _ => playlist,
    }
}

/// `seg00012.m4s`, nothing else: the name is matched before it reaches the filesystem, so it
/// cannot escape the session directory.
pub fn is_segment_name(name: &str) -> bool {
    name.len() == 12
        && name.starts_with("seg")
        && name.ends_with(".m4s")
        && name[3..8].bytes().all(|b| b.is_ascii_digit())
}

pub fn segment_index(name: &str) -> Option<usize> {
    is_segment_name(name)
        .then(|| name[3..8].parse().ok())
        .flatten()
}

pub struct Hls {
    settings: Arc<Settings>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    encode_slots: Arc<Semaphore>,
    /// Other directories whose size counts against the disk budget (the torrent downloads).
    budget_dirs: Vec<PathBuf>,
    ahead: AtomicUsize,
}

impl Hls {
    /// Output from an earlier run is scratch data and everything in it can be made again.
    pub fn new(settings: Arc<Settings>, budget_dirs: Vec<PathBuf>) -> Arc<Hls> {
        let dir = settings.hls_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Hls {
            encode_slots: Arc::new(Semaphore::new(settings.max_video_transcodes)),
            settings,
            sessions: Mutex::new(HashMap::new()),
            budget_dirs,
            ahead: AtomicUsize::new(AHEAD),
        })
    }

    /// Changes how far ahead new sessions may run. For tests, which cannot wait for forty segments.
    #[cfg(test)]
    pub fn set_ahead(&self, segments: usize) {
        self.ahead.store(segments, Ordering::Relaxed);
    }

    pub fn count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// Starts a conversion, or joins the one already running for the same file and modes: two
    /// viewers of one episode share one ffmpeg and one copy on disk.
    pub async fn start(
        self: &Arc<Self>,
        origin: Arc<Origin>,
        video: VideoMode,
        audio: AudioMode,
        duration: Option<f64>,
        start: Option<f64>,
    ) -> Result<Arc<Session>, StartError> {
        let key = format!("{}|{video:?}|{audio:?}", origin.key());

        // Joining, making room and registering happen under one lock, so two requests for one
        // file cannot both start it.
        let session = {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(existing) = sessions.get(&key).filter(|s| !s.is_dead()) {
                existing.touch();
                existing.begin_at(start);
                return Ok(existing.clone());
            }
            sessions.retain(|_, session| !session.is_dead());
            if sessions.len() >= self.settings.max_hls_sessions {
                // Only a session nobody has used for a while is evicted: dropping one that is
                // being watched would send its viewer back to a spinner.
                let victim = sessions
                    .values()
                    .filter(|s| s.idle() >= EVICTABLE_AFTER)
                    .max_by_key(|s| s.idle())
                    .cloned();
                match victim {
                    Some(victim) => {
                        sessions.remove(&victim.key);
                        self.retire(&victim);
                    }
                    None => return Err(StartError::Busy),
                }
            }
            let permit = if video == VideoMode::Encode {
                match self.encode_slots.clone().try_acquire_owned() {
                    Ok(permit) => Some(permit),
                    Err(_) => return Err(StartError::Busy),
                }
            } else {
                None
            };
            let id = new_id();
            let dir = self.settings.hls_dir().join(&id);
            let session = Arc::new(Session {
                id,
                key: key.clone(),
                dir,
                ahead: if video == VideoMode::Encode {
                    self.ahead.load(Ordering::Relaxed).min(ENCODE_AHEAD)
                } else {
                    self.ahead.load(Ordering::Relaxed)
                },
                bytes_per_second: duration
                    .filter(|seconds| *seconds > 0.)
                    .map(|seconds| origin.length() as f64 / seconds),
                last_used: AtomicU64::new(clock()),
                requested: AtomicUsize::new(0),
                listed: AtomicUsize::new(0),
                finished: AtomicBool::new(false),
                dead: AtomicBool::new(false),
                duration: Mutex::new(None),
                reason: Mutex::new(None),
                kill: Mutex::new(None),
                tasks: Mutex::new(Vec::new()),
                _permit: permit,
            });
            session.begin_at(start);
            sessions.insert(key.clone(), session.clone());
            session
        };

        if let Err(error) = self.spawn(&session, origin, video, audio).await {
            self.destroy(&session);
            return Err(StartError::Failed(error));
        }
        self.enforce_disk_budget(&session.id).await;
        Ok(session)
    }

    async fn spawn(
        self: &Arc<Self>,
        session: &Arc<Session>,
        origin: Arc<Origin>,
        video: VideoMode,
        audio: AudioMode,
    ) -> Result<(), String> {
        tokio::fs::create_dir_all(&session.dir)
            .await
            .map_err(|error| format!("could not create the session directory: {error}"))?;

        let mut args: Vec<String> = ["-nostdin", "-v", "error"]
            .into_iter()
            .map(str::to_string)
            .collect();
        if video == VideoMode::Encode {
            // The decoder is multi-threaded by default, and left alone it takes every core. This
            // is an input option, so it has to come before the input.
            args.extend([
                "-threads".to_string(),
                self.settings.transcode_threads.to_string(),
            ]);
        }
        args.extend(
            ["-i", "pipe:0", "-map", "0:v:0", "-map", "0:a:0?"]
                .into_iter()
                .map(str::to_string),
        );
        args.extend(codec_args(video, audio, self.settings.transcode_threads));
        let dir = session.dir.display();
        args.extend(
            [
                "-f",
                "hls",
                "-hls_time",
                "6", // SEGMENT_SECONDS
                "-hls_playlist_type",
                "event",
                "-hls_list_size",
                "0",
                "-hls_segment_type",
                "fmp4",
                "-hls_flags",
                "independent_segments",
                "-hls_fmp4_init_filename",
                "init.mp4",
                "-hls_segment_filename",
            ]
            .into_iter()
            .map(str::to_string),
        );
        args.push(format!("{dir}/seg%05d.m4s"));
        args.push(format!("{dir}/index.m3u8"));

        // A re-encode runs at low priority, so it cannot starve the requests that keep other
        // viewers' pages and streams going.
        let mut child = if video == VideoMode::Encode {
            let niced = Command::new("nice")
                .args(["-n", "10", "ffmpeg"])
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn();
            match niced {
                Ok(child) => child,
                Err(_) => spawn_ffmpeg(&args)?,
            }
        } else {
            spawn_ffmpeg(&args)?
        };
        let stdin = child.stdin.take().ok_or("ffmpeg has no stdin")?;
        let stderr = child.stderr.take().ok_or("ffmpeg has no stderr")?;

        let mut tasks = Vec::new();

        // What ffmpeg says is kept, so a failed conversion can be explained in the log.
        let logged = session.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if !line.is_empty() {
                    tracing::warn!("[hls] {line}");
                    *logged.reason.lock().unwrap() = Some(line);
                }
            }
        }));

        // The feed. It reads the file from the start and stops reading while the viewer is far
        // enough behind the conversion.
        let fed = session.clone();
        let source = origin.clone();
        let hls = self.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(error) = feed(&source, stdin, &fed).await {
                tracing::warn!("[hls] could not read \"{}\": {error:#}", source.name());
                *fed.reason.lock().unwrap() = Some(format!("{error:#}"));
                hls.destroy(&fed);
            }
        }));

        // What is on disk: how many segments exist and whether the conversion is complete. It also
        // ends a session nobody is fetching from any more.
        let watched = session.clone();
        let hls = self.clone();
        let ttl = self.settings.hls_session_ttl;
        tasks.push(tokio::spawn(async move {
            while !watched.is_dead() {
                if let Some(playlist) = watched.read_playlist().await {
                    watched
                        .listed
                        .store(playlist.segments.len(), Ordering::Relaxed);
                    watched.finished.store(playlist.finished, Ordering::Relaxed);
                    if playlist.duration > 0. {
                        *watched.duration.lock().unwrap() = Some(playlist.duration);
                    }
                }
                if watched.idle() >= ttl {
                    tracing::info!("[hls] {} expired", watched.id);
                    hls.destroy(&watched);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }));

        // The process itself. A conversion that dies must not leave a session that never
        // produces a segment, so a failure ends it at once and visibly.
        let (kill_tx, kill_rx) = oneshot::channel();
        *session.kill.lock().unwrap() = Some(kill_tx);
        let ended = session.clone();
        let hls = self.clone();
        tasks.push(tokio::spawn(async move {
            tokio::select! {
                status = child.wait() => {
                    let complete = ended
                        .read_playlist()
                        .await
                        .map(|playlist| playlist.finished)
                        .unwrap_or(false);
                    match status {
                        Ok(status) if status.success() || complete => {
                            ended.finished.store(true, Ordering::Relaxed);
                        }
                        other => {
                            tracing::warn!("[hls] ffmpeg exited with {other:?}");
                            hls.destroy(&ended);
                        }
                    }
                }
                _ = kill_rx => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
        }));

        *session.tasks.lock().unwrap() = tasks;
        Ok(())
    }

    /// Stops a session and gives its disk back. Safe to call more than once.
    pub fn destroy(self: &Arc<Self>, session: &Arc<Session>) {
        {
            let mut sessions = self.sessions.lock().unwrap();
            if sessions
                .get(&session.key)
                .is_some_and(|s| Arc::ptr_eq(s, session))
            {
                sessions.remove(&session.key);
            }
        }
        self.retire(session);
    }

    /// The part of ending a session that does not touch the map.
    fn retire(&self, session: &Arc<Session>) {
        if session.dead.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(kill) = session.kill.lock().unwrap().take() {
            let _ = kill.send(());
        }
        let tasks: Vec<_> = session.tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            // The kill is delivered by the process task itself; the rest only read and wait.
            // Aborting here would also abort the task that has to reap the child, so the process
            // task is left to finish and the others are stopped.
            if !task.is_finished() {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    task.abort();
                });
            }
        }
        let dir = session.dir.clone();
        tokio::spawn(async move {
            // Retried once: ffmpeg may still be writing for a moment after the kill.
            for _ in 0..2 {
                if tokio::fs::remove_dir_all(&dir).await.is_ok() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    pub fn shutdown(self: &Arc<Self>) {
        let all: Vec<_> = self.sessions.lock().unwrap().values().cloned().collect();
        for session in all {
            self.destroy(&session);
        }
    }

    /// Keeps the conversions and the torrent downloads together under the disk budget, oldest idle
    /// conversion first. The session being created is never the one evicted, so an episode larger
    /// than the whole budget still plays; it just cannot be joined by another.
    async fn enforce_disk_budget(self: &Arc<Self>, keep: &str) {
        loop {
            let mut total = dir_size(&self.settings.hls_dir()).await;
            for dir in &self.budget_dirs {
                total += dir_size(dir).await;
            }
            if total <= self.settings.max_cache_bytes {
                return;
            }
            let victim = self
                .sessions
                .lock()
                .unwrap()
                .values()
                .filter(|s| s.id != keep)
                .max_by_key(|s| s.idle())
                .cloned();
            let Some(victim) = victim else { return };
            tracing::warn!("[hls] over the disk budget: dropping {}", victim.id);
            self.destroy(&victim);
            // Its directory goes in the background; give it a moment so the next measure sees it.
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    pub async fn playlist(&self, session: &Session) -> Option<String> {
        if session.is_dead() {
            return None;
        }
        session.touch();
        let text = tokio::fs::read_to_string(session.playlist_path())
            .await
            .ok()?;
        Some(start_at_the_beginning(text))
    }

    /// The file for one segment, once ffmpeg has moved past it. The last segment the playlist lists
    /// may still be being written, so it is only served when ffmpeg has stopped; serving it early
    /// would hand the player a truncated fragment. A segment not yet written is awaited.
    pub async fn segment(
        &self,
        session: &Session,
        name: &str,
    ) -> Result<(PathBuf, u64), SegmentError> {
        if session.is_dead() {
            return Err(SegmentError::Expired);
        }
        session.touch();

        if name == "init.mp4" {
            let path = session.dir.join(name);
            let deadline = Instant::now() + SEGMENT_TIMEOUT;
            loop {
                if let Ok(meta) = tokio::fs::metadata(&path).await
                    && meta.len() > 0
                {
                    return Ok((path, meta.len()));
                }
                if session.is_dead() {
                    return Err(SegmentError::Expired);
                }
                if Instant::now() >= deadline {
                    return Err(SegmentError::NotYet);
                }
                tokio::time::sleep(POLL).await;
            }
        }

        let index = segment_index(name).ok_or(SegmentError::Invalid)?;
        // Asking for a segment lifts the limit on how far ahead the conversion may run, which is
        // what lets a seek past what has been made catch up.
        session.requested.fetch_max(index + 1, Ordering::Relaxed);

        let deadline = Instant::now() + SEGMENT_TIMEOUT;
        loop {
            if session.is_dead() {
                return Err(SegmentError::Expired);
            }
            if let Some(playlist) = session.read_playlist().await
                && let Some(position) = playlist.segments.iter().position(|s| s == name)
                && (position + 1 < playlist.segments.len() || playlist.finished)
            {
                let path = session.dir.join(name);
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    return Ok((path, meta.len()));
                }
            }
            if Instant::now() >= deadline {
                return Err(SegmentError::NotYet);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    pub fn start_timeout(video: VideoMode) -> Duration {
        if video == VideoMode::Encode {
            ENCODE_START_TIMEOUT
        } else {
            START_TIMEOUT
        }
    }
}

fn spawn_ffmpeg(args: &[String]) -> Result<tokio::process::Child, String> {
    Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("could not start ffmpeg: {error}"))
}

/// Copies the file into ffmpeg, pausing while the conversion is well ahead of the viewer.
async fn feed(
    origin: &Origin,
    mut stdin: tokio::process::ChildStdin,
    session: &Session,
) -> anyhow::Result<()> {
    let mut reader = origin.open_at(0).await?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut fed: u64 = 0;
    loop {
        // Held back, not stopped: the viewer's next request lifts it, and so does a seek.
        while !session.is_dead() && ahead_of_viewer(session, fed) {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if session.is_dead() {
            return Ok(());
        }
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        // ffmpeg closing its end is how a finished or failed conversion looks from here.
        if stdin.write_all(&buffer[..read]).await.is_err() {
            return Ok(());
        }
        fed += read as u64;
    }
    let _ = stdin.shutdown().await;
    Ok(())
}

/// Whether the conversion is further ahead of the viewer than it is allowed to be. Either measure
/// is enough: the bytes fed against the viewer's position, which is exact, and the segments ffmpeg
/// has finished, which also works when the file's rate is not known.
fn ahead_of_viewer(session: &Session, fed: u64) -> bool {
    let viewer = session.requested.load(Ordering::Relaxed);
    if session.listed.load(Ordering::Relaxed) > viewer + session.ahead {
        return true;
    }
    match session.bytes_per_second {
        // Two segments of slack: ffmpeg needs the whole of one, and the keyframe after it, before
        // it can write it, and a file's rate is only an average. Less than that can stall at
        // the very first segment.
        Some(rate) => fed as f64 > rate * (SEGMENT_SECONDS * (viewer + session.ahead + 2) as f64),
        None => false,
    }
}

/// The bytes a file really takes on disk. A torrent's file is created at its full length and filled
/// in as pieces arrive, so its length says what it will be, not what it holds.
fn on_disk(meta: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// The bytes under a directory, without following links.
pub async fn dir_size(root: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += on_disk(&meta);
            }
        }
    }
    total
}

fn new_id() -> String {
    let mut bytes = [0u8; 9];
    getrandom::fill(&mut bytes).expect("the system has randomness");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_segment_name_passes() {
        assert!(is_segment_name("seg00000.m4s"));
        assert!(is_segment_name("seg12345.m4s"));
        for bad in [
            "seg0000.m4s",
            "seg000000.m4s",
            "../seg00000.m4s",
            "seg0000a.m4s",
            "init.mp4",
            "seg00000.m4sx",
            "",
        ] {
            assert!(!is_segment_name(bad), "{bad}");
        }
        assert_eq!(segment_index("seg00042.m4s"), Some(42));
        assert_eq!(segment_index("nope"), None);
    }

    #[test]
    fn a_playlist_is_read_for_segments_length_and_the_end() {
        let growing = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6.006000,\nseg00000.m4s\n#EXTINF:6.006000,\nseg00001.m4s\n";
        let playlist = Playlist::parse(growing);
        assert_eq!(playlist.segments, ["seg00000.m4s", "seg00001.m4s"]);
        assert!(!playlist.finished);
        assert!((playlist.duration - 12.012).abs() < 1e-9);
        assert!(Playlist::parse(&format!("{growing}#EXT-X-ENDLIST\n")).finished);
    }

    #[test]
    fn a_growing_playlist_says_to_start_at_the_beginning() {
        let text = "#EXTM3U\n#EXT-X-VERSION:7\n#EXTINF:6.0,\nseg00000.m4s\n".to_string();
        let started = start_at_the_beginning(text);
        assert!(started.starts_with("#EXTM3U\n#EXT-X-START:TIME-OFFSET=0"));
        assert_eq!(started.matches("#EXT-X-START").count(), 1);
        // Said once, and a file that is not a playlist is left alone.
        assert_eq!(start_at_the_beginning(started.clone()), started);
        assert_eq!(start_at_the_beginning("nope".into()), "nope");
    }

    #[test]
    fn a_torn_read_of_the_playlist_is_harmless() {
        // ffmpeg was in the middle of writing the second name.
        let playlist = Playlist::parse("#EXTINF:6.0,\nseg00000.m4s\n#EXTINF:6.0,\nseg000");
        assert_eq!(playlist.segments, ["seg00000.m4s"]);
    }

    #[tokio::test]
    async fn the_size_of_a_directory_counts_everything_under_it() {
        let root = std::env::temp_dir().join(format!("akidev-size-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("x"), vec![1u8; 100_000]).unwrap();
        std::fs::write(root.join("a/b/y"), vec![1u8; 200_000]).unwrap();
        let size = dir_size(&root).await;
        assert!((300_000..400_000).contains(&size), "{size}");
        assert_eq!(dir_size(&root.join("missing")).await, 0);
        // A file made as long as a torrent will be, and filled in later, counts for what it holds.
        let sparse = std::fs::File::create(root.join("sparse")).unwrap();
        sparse.set_len(5 * 1024 * 1024 * 1024).unwrap();
        assert!(dir_size(&root).await < 1_000_000);
        std::fs::remove_dir_all(root).unwrap();
    }
}
