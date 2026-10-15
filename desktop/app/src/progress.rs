//! Where the viewer got to in each title, so the home screen can offer to carry on.

use akidev_core::model::Media;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many titles are remembered.
const KEPT: usize = 20;
/// Before this many seconds there is nothing worth resuming.
const RESUME_AFTER: f64 = 15.;
/// Past this fraction the episode counts as watched.
const FINISHED_AT: f64 = 0.93;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The title, with what the home screen needs to draw it.
    pub media: Media,
    /// The episode or film being watched.
    pub episode_id: String,
    pub position: f64,
    pub duration: f64,
    /// Seconds since the epoch.
    pub updated: u64,
}

impl Entry {
    pub fn new(media: Media, episode_id: String, position: f64, duration: f64) -> Entry {
        let updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        Entry {
            media,
            episode_id,
            position,
            duration,
            updated,
        }
    }

    pub fn fraction(&self) -> f32 {
        if self.duration > 0. {
            (self.position / self.duration).clamp(0., 1.) as f32
        } else {
            0.
        }
    }

    fn finished(&self) -> bool {
        self.duration > 0. && self.position >= self.duration * FINISHED_AT
    }
}

pub struct Progress {
    path: PathBuf,
    /// Most recently watched first, at most one per title.
    entries: Vec<Entry>,
}

impl Progress {
    /// Reads the file. A missing file is an empty history; one that cannot be read is too,
    /// since losing a watch history is better than refusing to start.
    pub fn load(path: PathBuf) -> Progress {
        let entries = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                tracing::warn!("[progress] ignoring unreadable {}: {error}", path.display());
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Progress { path, entries }
    }

    /// What to carry on with, most recent first.
    pub fn recent(&self) -> &[Entry] {
        &self.entries
    }

    /// Where to start `episode_id`, in seconds, when it was left part way.
    pub fn resume_at(&self, episode_id: &str) -> Option<f64> {
        self.entries
            .iter()
            .find(|entry| entry.episode_id == episode_id)
            .map(|entry| entry.position)
            .filter(|&position| position > RESUME_AFTER)
    }

    /// Remembers where the viewer is. A finished episode is forgotten instead.
    pub fn record(&mut self, entry: Entry) {
        self.entries
            .retain(|known| known.media.id != entry.media.id);
        if !entry.finished() {
            self.entries.insert(0, entry);
            self.entries.truncate(KEPT);
        }
        self.save();
    }

    fn save(&self) {
        let written = serde_json::to_vec(&self.entries)
            .map_err(std::io::Error::other)
            .and_then(|bytes| {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                // Written to a temporary name first, so a crash never leaves half a file.
                let partial = self.path.with_extension("part");
                std::fs::write(&partial, bytes)?;
                std::fs::rename(&partial, &self.path)
            });
        if let Err(error) = written {
            tracing::warn!("[progress] could not save: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akidev_core::model::Kind;

    fn media(id: &str) -> Media {
        Media {
            id: id.into(),
            kind: Kind::Series,
            title: id.into(),
            title_variants: vec![],
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
            genres: vec![],
            is_adult: false,
        }
    }

    fn history() -> Progress {
        let path = std::env::temp_dir().join(format!(
            "akidev-progress-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        Progress::load(path)
    }

    #[test]
    fn the_latest_episode_of_a_title_replaces_the_earlier_one() {
        let mut progress = history();
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            100.,
            1000.,
        ));
        progress.record(Entry::new(
            media("series-2"),
            "series-2-s1e1".into(),
            50.,
            1000.,
        ));
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e2".into(),
            200.,
            1000.,
        ));
        let ids: Vec<_> = progress
            .recent()
            .iter()
            .map(|e| e.episode_id.as_str())
            .collect();
        assert_eq!(ids, ["series-1-s1e2", "series-2-s1e1"]);
    }

    #[test]
    fn a_finished_episode_is_forgotten() {
        let mut progress = history();
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            100.,
            1000.,
        ));
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            960.,
            1000.,
        ));
        assert!(progress.recent().is_empty());
    }

    #[test]
    fn only_a_real_stretch_of_watching_is_resumed() {
        let mut progress = history();
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            5.,
            1000.,
        ));
        assert_eq!(progress.resume_at("series-1-s1e1"), None);
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            300.,
            1000.,
        ));
        assert_eq!(progress.resume_at("series-1-s1e1"), Some(300.));
        assert_eq!(progress.resume_at("series-1-s1e2"), None);
    }

    #[test]
    fn only_the_most_recent_titles_are_kept() {
        let mut progress = history();
        for number in 0..30 {
            progress.record(Entry::new(
                media(&format!("series-{number}")),
                format!("series-{number}-s1e1"),
                100.,
                1000.,
            ));
        }
        assert_eq!(progress.recent().len(), KEPT);
        assert_eq!(progress.recent()[0].media.id, "series-29");
    }

    #[test]
    fn history_survives_a_restart_and_a_broken_file_is_ignored() {
        let mut progress = history();
        progress.record(Entry::new(
            media("series-1"),
            "series-1-s1e1".into(),
            100.,
            1000.,
        ));
        let path = progress.path.clone();
        assert_eq!(Progress::load(path.clone()).recent().len(), 1);
        std::fs::write(&path, "{ not json").unwrap();
        assert!(Progress::load(path.clone()).recent().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
