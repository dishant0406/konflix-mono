//! Music: search and browse on YouTube Music, and the audio of a track.
//!
//! The catalogue comes from `ytmapi-rs` (vendored, see vendor/ytmapi-rs/PATCH.md), which speaks
//! YouTube Music's own web API with no key. The audio comes from yt-dlp: YouTube hides the stream
//! address behind client checks that change often, and yt-dlp is the one tool that keeps up with
//! them. The address it finds only works from the machine that asked, so the server fetches the
//! bytes and hands them on rather than sending the browser to YouTube.

use anyhow::{Context, anyhow};
use futures::future::join_all;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{OnceCell, Semaphore};
use ytmapi_rs::YtMusic;
use ytmapi_rs::auth::noauth::NoAuthToken;
use ytmapi_rs::common::{AlbumID, ArtistChannelID, Thumbnail, YoutubeID};

/// How long a resolved stream address is reused. They are valid for hours; this only has to
/// cover a listening session, including skipping back to a track.
const ADDRESS_TTL: Duration = Duration::from_secs(30 * 60);
const ADDRESS_MAX: usize = 500;
/// yt-dlp runs one process per track and takes a second or two of CPU. Two at once is plenty
/// for a track change, and a burst of requests waits instead of exhausting the box.
const RESOLVERS: usize = 2;
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(25);
const HOME_TTL: Duration = Duration::from_secs(3600);
const HOME_PARALLEL: usize = 6;
/// The names the home page opens on. A search for each gives the artist or album at the top.
const HOME_ARTISTS: [&str; 14] = [
    "Taylor Swift",
    "The Weeknd",
    "Arijit Singh",
    "Billie Eilish",
    "Kendrick Lamar",
    "Daft Punk",
    "Dua Lipa",
    "A. R. Rahman",
    "Drake",
    "Coldplay",
    "Bad Bunny",
    "Ed Sheeran",
    "Eminem",
    "Post Malone",
];
const HOME_ALBUMS: [&str; 12] = [
    "Random Access Memories Daft Punk",
    "After Hours The Weeknd",
    "Midnights Taylor Swift",
    "DAMN. Kendrick Lamar",
    "Future Nostalgia Dua Lipa",
    "Happier Than Ever Billie Eilish",
    "Aashiqui 2",
    "Parachutes Coldplay",
    "Divide Ed Sheeran",
    "Hollywood's Bleeding Post Malone",
    "Un Verano Sin Ti Bad Bunny",
    "Her Loss Drake",
];

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<Credit>,
    pub album: Option<Credit>,
    pub duration: Option<String>,
    pub explicit: bool,
    pub cover: Option<String>,
}

/// A name and, when YouTube Music knows one, the id to open its page with.
#[derive(Debug, Clone, Serialize)]
pub struct Credit {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumCard {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<String>,
    pub kind: Option<String>,
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtistCard {
    pub id: String,
    pub name: String,
    pub subscribers: Option<String>,
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchOut {
    pub songs: Vec<Track>,
    pub albums: Vec<AlbumCard>,
    pub artists: Vec<ArtistCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumOut {
    pub id: String,
    pub title: String,
    pub kind: Option<String>,
    pub year: String,
    pub duration: String,
    pub artists: Vec<Credit>,
    pub description: Option<String>,
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtistOut {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub subscribers: Option<String>,
    pub cover: Option<String>,
    pub songs: Vec<Track>,
    pub albums: Vec<AlbumCard>,
    pub singles: Vec<AlbumCard>,
    pub related: Vec<ArtistCard>,
}

/// What yt-dlp found for a track: where its audio is and what to say to fetch it.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub url: String,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
}

pub struct Music {
    // Creating a client asks YouTube Music for a visitor id, which can fail, so it happens on first
    // use and is retried on the next one if it did.
    yt: OnceCell<YtMusic<NoAuthToken>>,
    ytdlp: String,
    resolvers: Semaphore,
    addresses: Mutex<HashMap<String, (Instant, Resolved)>>,
    front: Mutex<Option<(Instant, SearchOut)>>,
    /// Trust the machine's own certificates instead of yt-dlp's bundled list. A desktop on a company
    /// network or behind antivirus has its own roots that only the system knows.
    system_certs: bool,
    ca_bundle: OnceCell<Option<std::path::PathBuf>>,
}

impl Music {
    /// `ytdlp` is the program that finds audio: a path, or a name looked up on PATH.
    pub fn new(ytdlp: impl Into<String>) -> Music {
        Music {
            yt: OnceCell::new(),
            ytdlp: ytdlp.into(),
            resolvers: Semaphore::new(RESOLVERS),
            addresses: Mutex::new(HashMap::new()),
            front: Mutex::new(None),
            system_certs: false,
            ca_bundle: OnceCell::new(),
        }
    }

    /// yt-dlp then checks YouTube's certificate against the system's roots, as the rest of the app does.
    pub fn with_system_certs(mut self) -> Music {
        self.system_certs = true;
        self
    }

    /// A file of the system's root certificates for yt-dlp to read, when the system keeps them somewhere
    /// it cannot find by itself.
    async fn system_ca_file(&self) -> Option<std::path::PathBuf> {
        self.ca_bundle
            .get_or_init(|| async {
                let beside = std::path::Path::new(&self.ytdlp).parent()?.to_path_buf();
                if cfg!(target_os = "macos") {
                    // The keychains hold the roots an employer or a security tool added, as well as Apple's.
                    let home = std::env::var("HOME").unwrap_or_default();
                    let login = format!("{home}/Library/Keychains/login.keychain-db");
                    let out = tokio::process::Command::new("security")
                        .args(["find-certificate", "-a", "-p"])
                        .args([
                            "/Library/Keychains/System.keychain",
                            "/System/Library/Keychains/SystemRootCertificates.keychain",
                            login.as_str(),
                        ])
                        .output()
                        .await
                        .ok()?;
                    if out.stdout.is_empty() {
                        return None;
                    }
                    let path = beside.join("ca.pem");
                    tokio::fs::write(&path, &out.stdout).await.ok()?;
                    Some(path)
                } else {
                    [
                        "/etc/ssl/certs/ca-certificates.crt",
                        "/etc/pki/tls/certs/ca-bundle.crt",
                    ]
                    .into_iter()
                    .map(std::path::PathBuf::from)
                    .find(|path| path.exists())
                }
            })
            .await
            .clone()
    }

    /// Keeps yt-dlp current, now and then daily: when YouTube changes how it hands out audio, the
    /// fix is a new yt-dlp, and a server that is up for weeks would otherwise stop playing.
    pub fn keep_updated(self: &std::sync::Arc<Self>) {
        let music = self.clone();
        tokio::spawn(async move {
            loop {
                let update = tokio::process::Command::new(&music.ytdlp)
                    .arg("-U")
                    .kill_on_drop(true)
                    .output();
                match tokio::time::timeout(Duration::from_secs(120), update).await {
                    Ok(Ok(out)) if out.status.success() => {
                        let said = String::from_utf8_lossy(&out.stdout);
                        tracing::info!(
                            "music: yt-dlp update: {}",
                            said.trim().lines().last().unwrap_or("")
                        );
                    }
                    // Not installed, a package-managed copy that cannot update itself, or offline.
                    _ => tracing::debug!("music: yt-dlp was not updated"),
                }
                tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
            }
        });
    }

    async fn yt(&self) -> anyhow::Result<&YtMusic<NoAuthToken>> {
        self.yt
            .get_or_try_init(|| async {
                YtMusic::new_unauthenticated()
                    .await
                    .map_err(|e| anyhow!("YouTube Music: {e}"))
            })
            .await
    }

    async fn songs(&self, term: &str) -> anyhow::Result<Vec<Track>> {
        let found = self
            .yt()
            .await?
            .search_songs(term)
            .await
            .map_err(|e| anyhow!("songs: {e}"))?;
        Ok(found
            .into_iter()
            .map(|s| Track {
                id: s.video_id.get_raw().to_string(),
                title: s.title,
                artists: vec![Credit {
                    id: None,
                    name: s.artist,
                }],
                album: s.album.map(|a| Credit {
                    id: Some(a.id.get_raw().to_string()),
                    name: a.name,
                }),
                duration: Some(s.duration).filter(|d| !d.is_empty()),
                explicit: is_explicit(&s.explicit),
                cover: cover(&s.thumbnails),
            })
            .collect())
    }

    async fn albums(&self, term: &str) -> anyhow::Result<Vec<AlbumCard>> {
        let found = self
            .yt()
            .await?
            .search_albums(term)
            .await
            .map_err(|e| anyhow!("albums: {e}"))?;
        Ok(found
            .into_iter()
            .map(|a| AlbumCard {
                id: a.album_id.get_raw().to_string(),
                title: a.title,
                artist: a.artist,
                year: Some(a.year).filter(|y| !y.is_empty()),
                kind: None,
                cover: cover(&a.thumbnails),
            })
            .collect())
    }

    async fn artists(&self, term: &str) -> anyhow::Result<Vec<ArtistCard>> {
        let found = self
            .yt()
            .await?
            .search_artists(term)
            .await
            .map_err(|e| anyhow!("artists: {e}"))?;
        Ok(found
            .into_iter()
            .map(|a| ArtistCard {
                id: a.browse_id.get_raw().to_string(),
                name: a.artist,
                subscribers: a.subscribers,
                cover: cover(&a.thumbnails),
            })
            .collect())
    }

    pub async fn search(&self, term: &str) -> anyhow::Result<SearchOut> {
        // Three filtered searches answer faster and more fully than the mixed one, which the
        // library is retiring.
        let (songs, albums, artists) =
            tokio::join!(self.songs(term), self.albums(term), self.artists(term));
        // One kind failing leaves the others; all three failing is an outage.
        if songs.is_err() && albums.is_err() && artists.is_err() {
            return Err(anyhow!("YouTube Music did not answer"));
        }
        Ok(SearchOut {
            songs: songs.unwrap_or_default(),
            albums: albums.unwrap_or_default(),
            artists: artists.unwrap_or_default(),
        })
    }

    /// What the music home page shows: the first result for each name in `HOME_ARTISTS` and
    /// `HOME_ALBUMS`, kept for an hour. YouTube Music's own home feed needs a signed-in account, and
    /// a search for "top hits" returns spam uploads, so the page is a list we choose.
    pub async fn home(&self) -> anyhow::Result<SearchOut> {
        if let Some((at, found)) = &*self.front.lock().unwrap()
            && at.elapsed() < HOME_TTL
        {
            return Ok(found.clone());
        }
        // A few at a time, so the first visit does not send two dozen requests at once.
        let mut artists = Vec::new();
        for names in HOME_ARTISTS.chunks(HOME_PARALLEL) {
            let answers = join_all(names.iter().map(|name| self.artists(name))).await;
            artists.extend(
                answers
                    .into_iter()
                    .filter_map(|a| a.ok()?.into_iter().next()),
            );
        }
        let mut albums = Vec::new();
        for names in HOME_ALBUMS.chunks(HOME_PARALLEL) {
            let answers = join_all(names.iter().map(|name| self.albums(name))).await;
            albums.extend(
                answers
                    .into_iter()
                    .filter_map(|a| a.ok()?.into_iter().next()),
            );
        }
        let found = SearchOut {
            songs: Vec::new(),
            albums,
            artists,
        };
        if found.albums.is_empty() && found.artists.is_empty() {
            return Err(anyhow!("YouTube Music did not answer"));
        }
        *self.front.lock().unwrap() = Some((Instant::now(), found.clone()));
        Ok(found)
    }

    pub async fn album(&self, id: &str) -> anyhow::Result<AlbumOut> {
        let yt = self.yt().await?;
        let album = yt
            .get_album(AlbumID::from_raw(id.to_string()))
            .await
            .map_err(|e| anyhow!("album: {e}"))?;
        let art = cover(&album.thumbnails);
        let artists: Vec<Credit> = album
            .artists
            .iter()
            .map(|a| Credit {
                id: a.id.as_ref().map(|id| id.get_raw().to_string()),
                name: a.name.clone(),
            })
            .collect();
        let tracks = album
            .tracks
            .into_iter()
            .map(|t| Track {
                id: t.video_id.get_raw().to_string(),
                title: t.title,
                // An album's tracks do not repeat the credits, so they carry the album's.
                artists: artists.clone(),
                album: Some(Credit {
                    id: Some(id.to_string()),
                    name: album.title.clone(),
                }),
                duration: Some(t.duration).filter(|d| !d.is_empty()),
                explicit: is_explicit(&t.explicit),
                cover: art.clone(),
            })
            .collect();
        Ok(AlbumOut {
            id: id.to_string(),
            title: album.title,
            kind: Some(format!("{:?}", album.category)),
            year: album.year,
            duration: album.duration,
            artists,
            description: album.description,
            cover: art,
            tracks,
        })
    }

    pub async fn artist(&self, id: &str) -> anyhow::Result<ArtistOut> {
        let yt = self.yt().await?;
        let artist = yt
            .get_artist(ArtistChannelID::from_raw(id.to_string()))
            .await
            .map_err(|e| anyhow!("artist: {e}"))?;
        let art = cover(&artist.thumbnails);
        let releases = artist.top_releases;
        let cards = |list: Option<ytmapi_rs::parse::GetArtistAlbums>| -> Vec<AlbumCard> {
            list.map(|l| l.results)
                .unwrap_or_default()
                .into_iter()
                .map(|a| AlbumCard {
                    id: a.album_id.get_raw().to_string(),
                    title: a.title,
                    artist: artist.name.clone(),
                    year: Some(a.year).filter(|y| !y.is_empty()),
                    kind: None,
                    cover: cover(&a.thumbnails),
                })
                .collect()
        };
        let songs = releases
            .songs
            .map(|s| s.results)
            .unwrap_or_default()
            .into_iter()
            .map(|s| Track {
                id: s.video_id.get_raw().to_string(),
                title: s.title,
                artists: s
                    .artists
                    .into_iter()
                    .map(|a| Credit {
                        id: a.id.map(|id| id.get_raw().to_string()),
                        name: a.name,
                    })
                    .collect(),
                album: Some(Credit {
                    id: Some(s.album.id.get_raw().to_string()),
                    name: s.album.name,
                }),
                duration: None,
                explicit: is_explicit(&s.explicit),
                cover: None,
            })
            .collect();
        Ok(ArtistOut {
            id: id.to_string(),
            name: artist.name.clone(),
            description: artist.description,
            subscribers: artist.subscribers,
            cover: art,
            songs,
            albums: cards(releases.albums),
            singles: cards(releases.singles),
            related: releases
                .related
                .map(|r| r.results)
                .unwrap_or_default()
                .into_iter()
                .map(|r| ArtistCard {
                    id: r.browse_id.get_raw().to_string(),
                    name: r.title,
                    subscribers: Some(r.subscribers).filter(|s| !s.is_empty()),
                    cover: None,
                })
                .collect(),
        })
    }

    /// Where the audio of a track is, and what to say to fetch it. Found with yt-dlp and kept for a while.
    pub async fn resolve(&self, video_id: &str) -> anyhow::Result<Resolved> {
        if let Some(found) = self.remembered(video_id) {
            return Ok(found);
        }
        let _slot = self.resolvers.acquire().await?;
        // A request ahead in the queue may have found it while this one waited.
        if let Some(found) = self.remembered(video_id) {
            return Ok(found);
        }
        let found = self.ask_ytdlp(video_id).await?;
        let mut addresses = self.addresses.lock().unwrap();
        if addresses.len() >= ADDRESS_MAX {
            addresses.retain(|_, (at, _)| at.elapsed() < ADDRESS_TTL);
            if addresses.len() >= ADDRESS_MAX {
                addresses.clear();
            }
        }
        addresses.insert(video_id.to_string(), (Instant::now(), found.clone()));
        Ok(found)
    }

    /// Forgets the address of a track, so the next ask looks it up again (it died early).
    pub fn forget(&self, video_id: &str) {
        self.addresses.lock().unwrap().remove(video_id);
    }

    fn remembered(&self, video_id: &str) -> Option<Resolved> {
        let addresses = self.addresses.lock().unwrap();
        let (at, found) = addresses.get(video_id)?;
        (at.elapsed() < ADDRESS_TTL).then(|| found.clone())
    }

    async fn ask_ytdlp(&self, video_id: &str) -> anyhow::Result<Resolved> {
        // m4a (AAC) plays everywhere, Safari included; failing that, whatever audio is best.
        let mut command = tokio::process::Command::new(&self.ytdlp);
        command.args([
            "--no-playlist",
            "--no-warnings",
            "--no-cache-dir",
            "--socket-timeout",
            "10",
            "-f",
            "bestaudio[ext=m4a]/bestaudio",
            "-j",
        ]);
        if self.system_certs {
            // Without this yt-dlp trusts only the certificates it ships with, which a network that
            // inspects traffic with its own root fails ("self-signed certificate in chain").
            command.args(["--compat-options", "no-certifi"]);
            if let Some(file) = self.system_ca_file().await {
                command.env("SSL_CERT_FILE", file);
            }
        }
        let run = command
            .arg("--")
            .arg(format!("https://music.youtube.com/watch?v={video_id}"))
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(RESOLVE_TIMEOUT, run)
            .await
            .context("yt-dlp took too long")?
            .context("yt-dlp did not start")?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "yt-dlp: {}",
                said.lines().last().unwrap_or("failed")
            ));
        }
        let info: Value = serde_json::from_slice(&output.stdout).context("yt-dlp output")?;
        let url = info["url"]
            .as_str()
            .ok_or_else(|| anyhow!("yt-dlp gave no url"))?
            .to_string();
        let content_type = match info["ext"].as_str() {
            Some("m4a") | Some("mp4") => "audio/mp4",
            Some("webm") => "audio/webm",
            Some("mp3") => "audio/mpeg",
            Some("opus") | Some("ogg") => "audio/ogg",
            _ => "application/octet-stream",
        };
        let headers = info["http_headers"]
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Resolved {
            url,
            content_type,
            headers,
        })
    }
}

/// The yt-dlp release for this machine: a zip holding the program and its files in one folder.
/// The single-file build unpacks itself on every start, which an antivirus can stretch to 20 seconds
/// a run; the folder build starts in a fraction of a second.
fn ytdlp_release() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        ("yt-dlp_macos.zip", "yt-dlp_macos")
    } else if cfg!(target_os = "windows") {
        ("yt-dlp_win.zip", "yt-dlp.exe")
    } else if cfg!(target_arch = "aarch64") {
        ("yt-dlp_linux_aarch64.zip", "yt-dlp_linux_aarch64")
    } else {
        ("yt-dlp_linux.zip", "yt-dlp_linux")
    }
}

/// Where yt-dlp is, or will be once `install_ytdlp` has run, for a program kept under `dir`.
pub fn ytdlp_program(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("yt-dlp-dist").join(ytdlp_release().1)
}

/// yt-dlp under `dir`, downloaded from its releases the first time, and run once so the system has
/// looked it over (the first run of a new program can take half a minute on a machine with security
/// software, and that must not be a track's wait). A desktop cannot be assumed to have it, and the
/// folder build needs nothing else (no Python). `Music::keep_updated` then keeps it current, since
/// YouTube changes often enough to outdate a copy within weeks.
pub async fn install_ytdlp(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    // One installer at a time: opening the music tab and pressing play can both ask.
    static INSTALLING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _only = INSTALLING.lock().await;

    let program = ytdlp_program(dir);
    let ready = dir.join("yt-dlp-dist").join(".ready");
    if ready.exists() && program.exists() {
        return Ok(program);
    }
    if !program.exists() {
        tokio::fs::create_dir_all(dir).await?;
        // The earlier single-file copy is of no use any more.
        let _ = tokio::fs::remove_file(dir.join(if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        }))
        .await;
        let url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
            ytdlp_release().0
        );
        let bytes = crate::http::client()
            .get(&url)
            .timeout(Duration::from_secs(600))
            .send()
            .await?
            .error_for_status()
            .context("downloading yt-dlp")?
            .bytes()
            .await?;
        let (target, partial) = (dir.join("yt-dlp-dist"), dir.join("yt-dlp-dist.part"));
        let unpacked = partial.clone();
        // Unpacked beside the final place and moved into it, so a half-finished copy is never run.
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let _ = std::fs::remove_dir_all(&unpacked);
            zip::ZipArchive::new(std::io::Cursor::new(bytes))?.extract(&unpacked)?;
            Ok(())
        })
        .await??;
        let _ = tokio::fs::remove_dir_all(&target).await;
        tokio::fs::rename(&partial, &target).await?;
    }
    // The first run, whose cost is paid here. Its answer does not matter, only that it ran.
    let _ = tokio::time::timeout(
        Duration::from_secs(180),
        tokio::process::Command::new(&program)
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await;
    tokio::fs::write(&ready, b"").await?;
    Ok(program)
}

/// YouTube video ids are 11 characters of letters, digits, `-` and `_`. Anything else is not one,
/// and is not passed on to a process.
pub fn is_video_id(text: &str) -> bool {
    text.len() == 11
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_explicit(explicit: &ytmapi_rs::common::Explicit) -> bool {
    matches!(explicit, ytmapi_rs::common::Explicit::IsExplicit)
}

/// The largest picture, which the image route then resizes.
fn cover(thumbnails: &[Thumbnail]) -> Option<String> {
    thumbnails
        .iter()
        .max_by_key(|t| t.width * t.height)
        .map(|t| t.url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_video_ids_pass() {
        assert!(is_video_id("dQw4w9WgXcQ"));
        assert!(is_video_id("a-b_c1D2e3F"));
        assert!(!is_video_id("short"));
        assert!(!is_video_id("--exec=evil;"));
        assert!(!is_video_id("dQw4w9WgXcQ1"));
    }

    // Live: downloads yt-dlp into a fresh folder, then finds a track with it, as the desktop app does.
    #[tokio::test]
    #[ignore]
    async fn live_install_then_find_an_address() {
        let dir = std::env::temp_dir().join(format!("akidev-ytdlp-{}", std::process::id()));
        let program = install_ytdlp(&dir).await.unwrap();
        assert_eq!(program, ytdlp_program(&dir));
        let started = Instant::now();
        let music = Music::new(program.to_string_lossy().into_owned()).with_system_certs();
        let resolved = music.resolve("dQw4w9WgXcQ").await.unwrap();
        println!("found in {:?}", started.elapsed());
        assert!(resolved.url.starts_with("https://"));
        let _ = std::fs::remove_dir_all(dir);
    }

    // Live, with the standalone yt-dlp the desktop app downloads, trusting the system's certificates:
    // `YT_DLP=/path/to/yt-dlp cargo test -p akidev-core system_certs -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_system_certs_find_an_address() {
        let program = std::env::var("YT_DLP").expect("YT_DLP names the yt-dlp to try");
        let music = Music::new(program).with_system_certs();
        let resolved = music.resolve("dQw4w9WgXcQ").await.unwrap();
        assert!(resolved.url.starts_with("https://"));
    }

    // Live: needs the network and yt-dlp. `cargo test -p akidev-core music -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_search_album_and_address() {
        let music = Music::new("yt-dlp");
        let found = music.search("daft punk get lucky").await.unwrap();
        assert!(!found.songs.is_empty() && !found.albums.is_empty() && !found.artists.is_empty());
        let album = music.album(&found.albums[0].id).await.unwrap();
        assert!(!album.tracks.is_empty());
        let artist = music.artist(&found.artists[0].id).await.unwrap();
        assert!(!artist.songs.is_empty());
        let resolved = music.resolve(&found.songs[0].id).await.unwrap();
        assert!(resolved.url.starts_with("https://"));
    }
}
