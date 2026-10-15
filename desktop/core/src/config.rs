//! User configuration: the torrent session limits.
//!
//! Stored as TOML in the app's config directory. Every field has a default, so a
//! missing or partial file is not an error and a fresh install needs no setup.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// How long one indexer request may take before it is abandoned.
    pub indexer_timeout_ms: u64,
    /// Where in-progress downloads live. `None` means the app's cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Upper bound on bytes held on disk by active torrents.
    pub max_cache_bytes: u64,
    /// Upper bound on simultaneously active torrents.
    pub max_torrents: usize,
    /// How long to wait for a magnet to find peers and report its file list.
    pub metadata_timeout_ms: u64,
    /// How many candidate releases to try before giving up.
    pub max_candidates: usize,
    /// Upper bound on resolving one episode or film, across all candidates.
    pub resolve_deadline_ms: u64,
    /// Caps what is uploaded to peers, in bytes a second. A server that streams to browsers has
    /// no use for seeding and would only pay for the bandwidth, so it sets a small one. The
    /// desktop app leaves it unlimited.
    pub upload_limit_bps: Option<u32>,
    /// Whether the desktop app looks for a newer version of itself now and then.
    pub check_for_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            indexer_timeout_ms: 5_000,
            cache_dir: None,
            max_cache_bytes: 8 * 1024 * 1024 * 1024,
            max_torrents: 3,
            metadata_timeout_ms: 45_000,
            max_candidates: 3,
            resolve_deadline_ms: 75_000,
            upload_limit_bps: None,
            check_for_updates: true,
        }
    }
}

impl Config {
    /// Reads the file, or the defaults when it does not exist. A file that exists but
    /// cannot be parsed is an error: silently ignoring it would discard the user's settings.
    pub fn load(path: &Path) -> anyhow::Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("reading {}", path.display()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let config = Config::load(Path::new("/definitely/not/here.toml")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let config: Config = toml::from_str("max_torrents = 1").unwrap();
        assert_eq!(config.max_torrents, 1);
        assert_eq!(config.indexer_timeout_ms, 5_000);
    }

    #[test]
    fn update_checks_are_on_unless_turned_off() {
        assert!(Config::default().check_for_updates);
        let config: Config = toml::from_str("check_for_updates = false").unwrap();
        assert!(!config.check_for_updates);
    }

    #[test]
    fn saving_then_loading_round_trips() {
        let path = std::env::temp_dir().join(format!("akidev-config-{}.toml", std::process::id()));
        let config = Config {
            max_candidates: 5,
            ..Config::default()
        };
        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn a_broken_file_is_an_error() {
        let path = std::env::temp_dir().join(format!("akidev-broken-{}.toml", std::process::id()));
        std::fs::write(&path, "max_torrents = [").unwrap();
        let result = Config::load(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_err());
    }
}
