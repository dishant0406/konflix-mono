//! Keeps one client from using the server up.
//!
//! Finding a source searches indexers and may join a swarm, so `/v1/play` is limited hard; the
//! catalogue is cheap and cached, so it is limited loosely. The key is the client's address, taken
//! from the header Cloudflare sets, because every request arrives from the tunnel.

use axum::http::HeaderMap;
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Limiter {
    seen: Mutex<HashMap<(String, &'static str), VecDeque<Instant>>>,
}

impl Limiter {
    /// Records one use and says whether it is within `limit` uses in the last `window`.
    pub fn allow(
        &self,
        client: &str,
        bucket: &'static str,
        limit: usize,
        window: Duration,
    ) -> bool {
        self.allow_at(client, bucket, limit, window, Instant::now())
    }

    fn allow_at(
        &self,
        client: &str,
        bucket: &'static str,
        limit: usize,
        window: Duration,
        now: Instant,
    ) -> bool {
        let mut seen = self.seen.lock().unwrap();
        // Idle clients are forgotten, so the map does not grow with every address ever seen.
        if seen.len() > 10_000 {
            seen.retain(|_, uses| {
                uses.back()
                    .is_some_and(|last| now.duration_since(*last) < window)
            });
        }
        let uses = seen.entry((client.to_string(), bucket)).or_default();
        while uses
            .front()
            .is_some_and(|first| now.duration_since(*first) >= window)
        {
            uses.pop_front();
        }
        if uses.len() >= limit {
            return false;
        }
        uses.push_back(now);
        true
    }
}

/// Who is asking. Cloudflare puts the visitor's address in `CF-Connecting-IP`; behind any other
/// proxy it is the first `X-Forwarded-For` entry; with neither it is the connection itself.
pub fn client_of(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let forwarded = header("cf-connecting-ip")
        .or_else(|| header("x-forwarded-for").and_then(|list| list.split(',').next()));
    forwarded
        .map(str::trim)
        .and_then(|address| address.parse::<IpAddr>().ok())
        .map(|address| address.to_string())
        .or_else(|| peer.map(|peer| peer.ip().to_string()))
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_held_to_its_limit_and_released_when_the_window_passes() {
        let limiter = Limiter::default();
        let start = Instant::now();
        let window = Duration::from_secs(60);
        for _ in 0..3 {
            assert!(limiter.allow_at("a", "play", 3, window, start));
        }
        assert!(!limiter.allow_at("a", "play", 3, window, start));
        // Others are unaffected, and so is another bucket.
        assert!(limiter.allow_at("b", "play", 3, window, start));
        assert!(limiter.allow_at("a", "api", 3, window, start));
        assert!(limiter.allow_at("a", "play", 3, window, start + window));
    }

    #[test]
    fn the_cloudflare_address_wins_and_a_forged_one_is_ignored() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "203.0.113.9".parse().unwrap());
        headers.insert("x-forwarded-for", "198.51.100.1, 10.0.0.1".parse().unwrap());
        assert_eq!(client_of(&headers, None), "203.0.113.9");

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1, 10.0.0.1".parse().unwrap());
        assert_eq!(client_of(&headers, None), "198.51.100.1");

        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "not an address".parse().unwrap());
        let peer: SocketAddr = "192.0.2.7:5000".parse().unwrap();
        assert_eq!(client_of(&headers, Some(peer)), "192.0.2.7");
        assert_eq!(client_of(&HeaderMap::new(), None), "unknown");
    }
}
