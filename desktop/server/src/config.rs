//! Settings, read from the environment once at start.
//!
//! Every value has a default that suits a single small server, so a bare `akidev-server` runs.
//! The names are the ones the Node API used, so the deploy configuration carries over.

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Settings {
    pub port: u16,
    /// What the browser is told to call this server, in the URLs it is handed.
    pub public_url: String,
    /// Origins allowed to call the API from a page.
    pub cors_origins: Vec<String>,
    /// Scratch data: torrent downloads, HLS output, the catalogue cache, the signed definitions.
    pub cache_dir: PathBuf,
    /// Where bytes come from, in order of preference: `debrid`, `torrent`, `archive-org`.
    pub stream_sources: Vec<String>,
    pub torbox_token: String,
    pub debrid_timeout: Duration,
    /// Hosts a debrid link may point at. An entry matches that host and its subdomains.
    pub debrid_cdn_hosts: Vec<String>,
    pub archive_org_base: String,
    pub max_torrents: usize,
    pub max_cache_bytes: u64,
    pub metadata_timeout_ms: u64,
    pub max_candidates: usize,
    pub resolve_deadline_ms: u64,
    pub indexer_timeout_ms: u64,
    /// Upload cap towards peers, bytes a second. A server has no use for seeding.
    pub upload_limit_bps: u32,
    /// Conversions running at once. Each is an ffmpeg process, and a copy costs disk and bandwidth
    /// rather than CPU, so this can be higher than the number of encodes.
    pub max_hls_sessions: usize,
    /// Of those, how many may re-encode video, which is the one expensive thing here.
    pub max_video_transcodes: usize,
    /// A session nobody has fetched from for this long is destroyed.
    pub hls_session_ttl: Duration,
    /// Threads a video re-encode may use.
    pub transcode_threads: usize,
    /// Hosts a picture may be fetched from. An entry matches that host and its subdomains.
    pub image_hosts: Vec<String>,
    /// The most the resized pictures may take on disk.
    pub image_cache_bytes: u64,
    /// Plays a client may start in ten minutes. Finding a source is the expensive call.
    pub play_limit: usize,
    /// Catalogue requests a client may make a minute.
    pub api_limit: usize,
}

fn text(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn list(name: &str, default: &str) -> Vec<String> {
    text(name, default)
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// A positive number, or the default when the variable is missing or not one.
fn number<T: std::str::FromStr + PartialOrd + Default>(name: &str, default: T) -> T {
    match std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<T>().ok())
    {
        Some(value) if value > T::default() => value,
        _ => default,
    }
}

impl Settings {
    pub fn from_env() -> Settings {
        let port: u16 = number("PORT", 9000);
        let public_url = text("PUBLIC_URL", &format!("http://localhost:{port}"));
        Settings {
            port,
            public_url: public_url.trim_end_matches('/').to_string(),
            cors_origins: list(
                "CORS_ORIGINS",
                "https://akidev.live,http://localhost:3000,http://localhost:3100",
            ),
            cache_dir: PathBuf::from(text("CACHE_DIR", "./cache")),
            stream_sources: list("STREAM_SOURCES", &text("STREAM_SOURCE", "torrent")),
            torbox_token: text("TORBOX_API_KEY", ""),
            debrid_timeout: Duration::from_millis(number("DEBRID_TIMEOUT_MS", 15_000)),
            debrid_cdn_hosts: list("DEBRID_CDN_HOSTS", "tb-cdn.pw,tb-cdn.io"),
            archive_org_base: text("ARCHIVE_ORG_BASE_URL", "https://archive.org")
                .trim_end_matches('/')
                .to_string(),
            max_torrents: number("MAX_TORRENTS", 2),
            max_cache_bytes: number("MAX_CACHE_BYTES", 8 * 1024 * 1024 * 1024),
            metadata_timeout_ms: number("METADATA_TIMEOUT_MS", 45_000),
            max_candidates: number("MAX_CANDIDATES", 3),
            resolve_deadline_ms: number("RESOLVE_DEADLINE_MS", 75_000),
            indexer_timeout_ms: number("INDEXER_TIMEOUT_MS", 5_000),
            upload_limit_bps: number("UPLOAD_LIMIT_BPS", 256 * 1024),
            max_hls_sessions: number("MAX_HLS_SESSIONS", 4),
            max_video_transcodes: number("MAX_VIDEO_TRANSCODES", 1),
            hls_session_ttl: Duration::from_secs(number("HLS_SESSION_TTL_MINUTES", 30u64) * 60),
            transcode_threads: number("TRANSCODE_THREADS", 2),
            image_hosts: list(
                "IMAGE_HOSTS",
                "anilist.co,kitsu.app,tvmaze.com,metahub.space,media-amazon.com,tmdb.org,yts.gg,yts.mx",
            ),
            image_cache_bytes: number("IMAGE_CACHE_BYTES", 768 * 1024 * 1024),
            play_limit: number("PLAY_LIMIT_PER_10_MIN", 20),
            api_limit: number("API_LIMIT_PER_MIN", 240),
        }
    }

    /// The torrent settings `akidev-core` is configured with.
    pub fn core(&self) -> akidev_core::config::Config {
        akidev_core::config::Config {
            indexer_timeout_ms: self.indexer_timeout_ms,
            cache_dir: Some(self.cache_dir.clone()),
            max_cache_bytes: self.max_cache_bytes,
            max_torrents: self.max_torrents,
            metadata_timeout_ms: self.metadata_timeout_ms,
            max_candidates: self.max_candidates,
            resolve_deadline_ms: self.resolve_deadline_ms,
            upload_limit_bps: Some(self.upload_limit_bps),
        }
    }

    pub fn hls_dir(&self) -> PathBuf {
        self.cache_dir.join("hls")
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.cache_dir.join("sources")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_dir.join("img")
    }

    pub fn catalogue_dir(&self) -> PathBuf {
        self.cache_dir.join("catalog")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_environment_gives_a_working_small_server() {
        let settings = Settings::from_env();
        assert_eq!(settings.max_torrents, 2);
        assert_eq!(settings.max_hls_sessions, 4);
        assert_eq!(settings.max_video_transcodes, 1);
        assert!(
            settings
                .cors_origins
                .contains(&"https://akidev.live".to_string())
        );
        assert!(!settings.public_url.ends_with('/'));
    }

    #[test]
    fn a_list_drops_blanks_and_spaces() {
        // SAFETY: tests in this module that touch the environment use their own names.
        unsafe { std::env::set_var("AKIDEV_TEST_LIST", " a, b ,,c ") };
        assert_eq!(list("AKIDEV_TEST_LIST", "x"), ["a", "b", "c"]);
        assert_eq!(list("AKIDEV_TEST_LIST_UNSET", "x,y"), ["x", "y"]);
    }

    #[test]
    fn a_number_must_be_positive_and_parse() {
        unsafe { std::env::set_var("AKIDEV_TEST_NUM", "0") };
        assert_eq!(number("AKIDEV_TEST_NUM", 7u32), 7);
        unsafe { std::env::set_var("AKIDEV_TEST_NUM", "twelve") };
        assert_eq!(number("AKIDEV_TEST_NUM", 7u32), 7);
        unsafe { std::env::set_var("AKIDEV_TEST_NUM", "12") };
        assert_eq!(number("AKIDEV_TEST_NUM", 7u32), 12);
    }
}
