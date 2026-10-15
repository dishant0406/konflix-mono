//! A small in-memory TTL cache shared by providers and indexers.
//!
//! Metadata providers are rate limited (AniList allows 30 requests a minute) and
//! indexers are slow, so a repeated page load must not go back upstream. The map is
//! bounded because keys include user-typed search terms.

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 500;

/// How long an empty result stays valid. Short on purpose: an empty answer is usually a
/// blocked mirror or a rate limit, and caching it for the full TTL would blackhole the
/// search for everyone.
pub const EMPTY_TTL: Duration = Duration::from_secs(60);

struct Entry {
    expires: Instant,
    stored: Instant,
    value: Arc<dyn Any + Send + Sync>,
}

fn store() -> &'static Mutex<HashMap<String, Entry>> {
    static STORE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

/// Returns the cached value for `key`, or loads it. A failed load is not cached.
/// `ttl_for` picks the lifetime from the loaded value, so an empty result can live
/// shorter than a real one.
pub async fn cached<T, F, Fut>(
    key: &str,
    ttl_for: impl Fn(&T) -> Duration,
    load: F,
) -> anyhow::Result<T>
where
    T: Clone + Send + Sync + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    {
        let map = store().lock().unwrap();
        if let Some(entry) = map.get(key)
            && entry.expires > Instant::now()
            && let Some(value) = entry.value.downcast_ref::<T>()
        {
            return Ok(value.clone());
        }
    }

    let value = load().await?;
    let now = Instant::now();
    let mut map = store().lock().unwrap();
    map.insert(
        key.to_string(),
        Entry {
            expires: now + ttl_for(&value),
            stored: now,
            value: Arc::new(value.clone()),
        },
    );
    while map.len() > MAX_ENTRIES {
        let oldest = map
            .iter()
            .min_by_key(|(_, entry)| entry.stored)
            .map(|(key, _)| key.clone());
        match oldest {
            Some(key) => map.remove(&key),
            None => break,
        };
    }
    Ok(value)
}

/// The TTL for a list: the short one when it is empty.
pub fn list_ttl<T>(full: Duration) -> impl Fn(&Vec<T>) -> Duration {
    move |value| if value.is_empty() { EMPTY_TTL } else { full }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn a_hit_does_not_load_again() {
        let loads = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            let loads = loads.clone();
            let value = cached(
                "test:hit",
                |_: &u32| Duration::from_secs(60),
                || async move {
                    loads.fetch_add(1, Ordering::SeqCst);
                    Ok(7u32)
                },
            )
            .await
            .unwrap();
            assert_eq!(value, 7);
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failure_is_not_cached() {
        let failed = cached(
            "test:fail",
            |_: &u32| Duration::from_secs(60),
            || async { Err(anyhow::anyhow!("down")) },
        )
        .await;
        assert!(failed.is_err());
        let value = cached(
            "test:fail",
            |_: &u32| Duration::from_secs(60),
            || async { Ok(1u32) },
        )
        .await
        .unwrap();
        assert_eq!(value, 1);
    }

    #[tokio::test]
    async fn an_empty_list_lives_shorter() {
        let ttl = list_ttl::<u8>(Duration::from_secs(600));
        assert_eq!(ttl(&vec![]), EMPTY_TTL);
        assert_eq!(ttl(&vec![1]), Duration::from_secs(600));
    }
}
