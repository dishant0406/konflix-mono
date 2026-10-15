//! Subtitle files, converted to WebVTT and kept.
//!
//! A subtitle's address comes from a source's answer and is never handed to a browser: the list
//! gives an address on this server instead, made from the file's own address, so the same file
//! always has the same one and the edge can keep it. This server fetches the file, converts it,
//! and keeps the result on disk.

use akidev_core::subtitles;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

/// How many addresses are remembered in memory. The rest are on disk.
const REMEMBERED: usize = 5000;

pub struct Subs {
    dir: PathBuf,
    known: Mutex<HashMap<String, String>>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// The token names nothing this server listed.
    Unknown,
    /// The file could not be fetched or read as subtitles.
    Unavailable,
}

/// 128 bits from the address, as hex. Not a secret: it only has to be the same for the same
/// address and not collide.
pub fn token_for(url: &str) -> String {
    let mut parts = [0u64; 2];
    for (salt, part) in parts.iter_mut().enumerate() {
        let mut hasher = DefaultHasher::new();
        salt.hash(&mut hasher);
        url.hash(&mut hasher);
        *part = hasher.finish();
    }
    format!("{:016x}{:016x}", parts[0], parts[1])
}

fn well_formed(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Subs {
    pub fn new(dir: PathBuf) -> Subs {
        let _ = std::fs::create_dir_all(&dir);
        Subs {
            dir,
            known: Mutex::new(HashMap::new()),
        }
    }

    /// The token for a file's address, remembered so the file can be asked for by it.
    pub async fn register(&self, url: &str) -> String {
        let token = token_for(url);
        let new = {
            let mut known = self.known.lock().unwrap();
            if known.len() >= REMEMBERED {
                known.clear();
            }
            known.insert(token.clone(), url.to_string()).is_none()
        };
        // Written down too, so a link the browser still holds works after a restart.
        if new {
            let _ = tokio::fs::write(self.dir.join(format!("{token}.url")), url).await;
        }
        token
    }

    async fn address(&self, token: &str) -> Option<String> {
        if let Some(url) = self.known.lock().unwrap().get(token).cloned() {
            return Some(url);
        }
        let url = tokio::fs::read_to_string(self.dir.join(format!("{token}.url")))
            .await
            .ok()?;
        self.known
            .lock()
            .unwrap()
            .insert(token.to_string(), url.clone());
        Some(url)
    }

    /// The file for a token as WebVTT: from disk when it was made before, otherwise fetched and
    /// converted.
    pub async fn vtt(&self, token: &str) -> Result<String, Error> {
        if !well_formed(token) {
            return Err(Error::Unknown);
        }
        let path = self.dir.join(format!("{token}.vtt"));
        if let Ok(text) = tokio::fs::read_to_string(&path).await {
            return Ok(text);
        }
        let url = self.address(token).await.ok_or(Error::Unknown)?;
        let text = subtitles::fetch(&url).await.map_err(|error| {
            tracing::debug!("[subs] {error:#}");
            Error::Unavailable
        })?;
        let partial = path.with_extension("partial");
        if tokio::fs::write(&partial, &text).await.is_ok() {
            let _ = tokio::fs::rename(&partial, &path).await;
        }
        Ok(text)
    }

    /// Deletes the oldest files until the directory is within `limit` bytes.
    pub async fn prune(&self, limit: u64) {
        let mut files = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await
                && meta.is_file()
            {
                files.push((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    meta.len(),
                    entry.path(),
                ));
            }
        }
        let mut total: u64 = files.iter().map(|file| file.1).sum();
        files.sort_by_key(|file| file.0);
        for (_, size, path) in files {
            if total <= limit {
                break;
            }
            if tokio::fs::remove_file(&path).await.is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_always_has_the_same_token_and_different_files_differ() {
        let a = token_for("https://subs5.strem.io/en/a.srt");
        assert_eq!(a, token_for("https://subs5.strem.io/en/a.srt"));
        assert_ne!(a, token_for("https://subs5.strem.io/en/b.srt"));
        assert!(well_formed(&a));
    }

    #[tokio::test]
    async fn a_token_that_was_never_listed_is_unknown_and_a_malformed_one_is_too() {
        let dir = std::env::temp_dir().join(format!("akidev-subs-{}", std::process::id()));
        let subs = Subs::new(dir.clone());
        assert_eq!(subs.vtt(&"0".repeat(32)).await.unwrap_err(), Error::Unknown);
        assert_eq!(subs.vtt("../etc/passwd").await.unwrap_err(), Error::Unknown);
        assert_eq!(subs.vtt("").await.unwrap_err(), Error::Unknown);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_listed_address_survives_a_restart_and_a_kept_file_is_served_from_disk() {
        let dir = std::env::temp_dir().join(format!("akidev-subs2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = Subs::new(dir.clone());
        let token = first.register("https://subs5.strem.io/en/a.srt").await;
        // A new process: nothing in memory, the address is read back from disk.
        let second = Subs::new(dir.clone());
        assert_eq!(
            second.address(&token).await.as_deref(),
            Some("https://subs5.strem.io/en/a.srt")
        );
        // A converted file already kept is returned without any fetch.
        std::fs::write(
            dir.join(format!("{token}.vtt")),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n",
        )
        .unwrap();
        assert!(second.vtt(&token).await.unwrap().contains("Hi"));
        // The oldest go first when over the limit.
        second.prune(1).await;
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
