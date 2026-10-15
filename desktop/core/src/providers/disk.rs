//! Catalogue answers kept on disk between runs.
//!
//! A browse row is the same this morning as it was last night, so the home screen does not
//! need to wait for the network to show it. An answer that is not too old is read from here at
//! once, and when it is older than its time to live the caller refreshes it in the background.

use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// An answer older than this is not shown while waiting for a new one: it is too old to be the
/// thing a viewer expects to see.
pub const SHOWN_UNTIL: Duration = Duration::from_secs(24 * 60 * 60);

/// Files older than this are deleted when the app starts.
const KEPT_FOR: Duration = Duration::from_secs(2 * 24 * 60 * 60);

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Starts keeping answers in `dir`, and deletes the ones that are no use any more.
pub fn start(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    prune(&dir, SystemTime::now());
    let _ = DIR.set(dir);
}

pub fn enabled() -> bool {
    DIR.get().is_some()
}

/// What was kept for a key, and how old it is.
pub struct Kept {
    pub body: Value,
    pub age: Duration,
}

pub fn read(key: &str) -> Option<Kept> {
    read_in(DIR.get()?, key, SystemTime::now())
}

pub fn write(key: &str, body: &Value) {
    if let Some(dir) = DIR.get() {
        write_in(dir, key, body, SystemTime::now());
    }
}

/// Keeps an answer as if it had been written `age` ago.
#[cfg(test)]
pub fn write_aged(key: &str, body: &Value, age: Duration) {
    write_in(
        DIR.get().expect("started"),
        key,
        body,
        SystemTime::now() - age,
    );
}

fn file(dir: &Path, key: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    dir.join(format!("{:016x}.json", hasher.finish()))
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn read_in(dir: &Path, key: &str, now: SystemTime) -> Option<Kept> {
    let bytes = std::fs::read(file(dir, key)).ok()?;
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    // A different key can land on the same file name; the key kept inside says which it was.
    if stored.key != key {
        return None;
    }
    let age = Duration::from_secs(seconds(now).saturating_sub(stored.at));
    (age <= SHOWN_UNTIL).then_some(Kept {
        body: stored.body,
        age,
    })
}

fn write_in(dir: &Path, key: &str, body: &Value, now: SystemTime) {
    let stored = StoredRef {
        key,
        at: seconds(now),
        body,
    };
    let Ok(bytes) = serde_json::to_vec(&stored) else {
        return;
    };
    // Written under another name first, so a half-written file is never read back.
    let path = file(dir, key);
    let partial = path.with_extension("part");
    if std::fs::write(&partial, bytes).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

fn prune(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > KEPT_FOR);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[derive(serde::Deserialize)]
struct Stored {
    key: String,
    at: u64,
    body: Value,
}

#[derive(serde::Serialize)]
struct StoredRef<'a> {
    key: &'a str,
    at: u64,
    body: &'a Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("akidev-disk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_answer_comes_back_with_its_age() {
        let dir = temp("age");
        let then = SystemTime::now();
        write_in(&dir, "a", &json!({ "n": 1 }), then);
        let hour_later = then + Duration::from_secs(3600);
        let kept = read_in(&dir, "a", hour_later).unwrap();
        assert_eq!(kept.body, json!({ "n": 1 }));
        assert_eq!(kept.age.as_secs() / 60, 60);
        assert!(read_in(&dir, "other", hour_later).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_answer_that_is_too_old_is_not_shown() {
        let dir = temp("old");
        let then = SystemTime::now();
        write_in(&dir, "a", &json!(1), then);
        assert!(read_in(&dir, "a", then + SHOWN_UNTIL - Duration::from_secs(5)).is_some());
        assert!(read_in(&dir, "a", then + SHOWN_UNTIL + Duration::from_secs(5)).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn damaged_files_are_ignored_and_old_ones_pruned() {
        let dir = temp("prune");
        std::fs::write(file(&dir, "a"), b"{not json").unwrap();
        assert!(read_in(&dir, "a", SystemTime::now()).is_none());
        write_in(&dir, "b", &json!(2), SystemTime::now());
        prune(&dir, SystemTime::now() + KEPT_FOR + Duration::from_secs(60));
        assert!(!file(&dir, "b").exists());
        write_in(&dir, "c", &json!(3), SystemTime::now());
        prune(&dir, SystemTime::now());
        assert!(file(&dir, "c").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
