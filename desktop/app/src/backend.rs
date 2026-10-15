//! The streaming backend, shared by every screen.

use akidev_core::config::Config;
use akidev_core::stream::Streamer;
use gpui_kit::Global;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Backend {
    /// The reason it failed to start, when it did: browsing still works without it.
    pub streamer: Result<Arc<Streamer>, String>,
    pub config: Config,
    /// The settings the torrent engine started with. Some apply only at the next start.
    pub started_with: Config,
    pub config_path: PathBuf,
    /// Where torrents download. Deleted on quit.
    pub downloads_dir: PathBuf,
    pub progress: crate::progress::Progress,
    /// The volume the viewer left the last video at, so the next episode starts there.
    pub volume: f64,
    pub muted: bool,
}

impl Global for Backend {}

impl Backend {
    /// Reads the settings and starts the torrent engine and the local stream server.
    pub fn start(
        config_path: PathBuf,
        progress_path: PathBuf,
        default_torrent_dir: PathBuf,
        sources_dir: PathBuf,
    ) -> Backend {
        // The copy saved by an earlier run is used at once, and a newer one is fetched in
        // the background: indexers move often, and a build is not needed to follow them.
        akidev_core::sources::load_saved(&sources_dir);
        crate::runtime::spawn(akidev_core::sources::keep_fresh(sources_dir));

        let config = Config::load(&config_path).unwrap_or_else(|error| {
            // Kept aside, so the next Save does not overwrite what the user wrote.
            let kept = config_path.with_extension("toml.broken");
            tracing::error!(
                "[config] {error:#}; using the defaults, and keeping the file as {}",
                kept.display()
            );
            let _ = std::fs::rename(&config_path, kept);
            Config::default()
        });
        let torrent_dir = config.cache_dir.clone().unwrap_or(default_torrent_dir);
        let downloads_dir = akidev_core::torrent::Engine::downloads_dir(&torrent_dir);
        let streamer =
            crate::runtime::block_on(Streamer::start(config.clone(), torrent_dir.clone()))
                .map(Arc::new)
                .map_err(|error| format!("{error:#}"));
        if let Err(error) = &streamer {
            tracing::error!("[backend] could not start streaming: {error}");
        }
        Backend {
            streamer,
            started_with: config.clone(),
            config,
            config_path,
            downloads_dir,
            progress: crate::progress::Progress::load(progress_path),
            volume: 100.,
            muted: false,
        }
    }
}
