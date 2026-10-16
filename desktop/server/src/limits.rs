//! Keeps one client from using the server up.
//!
//! Finding a source searches indexers and may join a swarm, so `/v1/play` is limited hard; the
//! catalogue is cheap and cached, so it is limited loosely. The key is the client's address, taken
//! from the headers the proxy in front of this server sets.

use axum::http::HeaderMap;
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// How many video bytes this server has sent, in all and to each client today. Video is what costs
/// bandwidth, so this is what a daily allowance is counted in.
#[derive(Default)]
pub struct Usage {
    total: AtomicU64,
    /// Client -> (the day it counts for, bytes sent that day).
    today: Mutex<HashMap<String, (u64, u64)>>,
}

impl Usage {
    pub fn add(&self, client: &str, bytes: u64) {
        self.add_on(client, bytes, day());
    }

    /// Whether the client has been sent `allowance` bytes or more today. Zero means no limit.
    pub fn used_up(&self, client: &str, allowance: u64) -> bool {
        allowance > 0 && self.sent_on(client, day()) >= allowance
    }

    /// Video bytes sent since the server started.
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    fn add_on(&self, client: &str, bytes: u64, day: u64) {
        self.total.fetch_add(bytes, Ordering::Relaxed);
        let mut today = self.today.lock().unwrap();
        if let Some(entry) = today.get_mut(client) {
            if entry.0 != day {
                *entry = (day, 0);
            }
            entry.1 += bytes;
            return;
        }
        // A new client. Those from earlier days are forgotten now, so the map does not grow with
        // every address seen, and the sweep happens only when something is added, not per chunk.
        if today.len() > 10_000 {
            today.retain(|_, (counted_on, _)| *counted_on == day);
        }
        today.insert(client.to_string(), (day, bytes));
    }

    fn sent_on(&self, client: &str, day: u64) -> u64 {
        match self.today.lock().unwrap().get(client) {
            Some((counted_on, bytes)) if *counted_on == day => *bytes,
            _ => 0,
        }
    }
}

/// The day number since the Unix epoch, in UTC.
fn day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400)
}

/// An address as a visitor's identity. An IPv6 visitor holds a whole /64, so every address in it is
/// the same visitor; counting each separately would let one visitor dodge the allowance.
fn whole_network(address: IpAddr) -> String {
    match address {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
    }
}

/// Who is asking. The proxy in front of this server (Caddy) puts the visitor's address in
/// `CF-Connecting-IP` and `X-Forwarded-For`, replacing whatever the client sent; with neither it is
/// the connection itself.
pub fn client_of(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let forwarded = header("cf-connecting-ip")
        .or_else(|| header("x-forwarded-for").and_then(|list| list.split(',').next()));
    forwarded
        .map(str::trim)
        .and_then(|address| address.parse::<IpAddr>().ok())
        .map(whole_network)
        .or_else(|| peer.map(|peer| whole_network(peer.ip())))
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_counted_per_day_and_total_counts_everyone() {
        let usage = Usage::default();
        usage.add_on("a", 600, 10);
        usage.add_on("a", 600, 10);
        usage.add_on("b", 50, 10);
        assert_eq!(usage.sent_on("a", 10), 1200);
        assert_eq!(usage.sent_on("b", 10), 50);
        assert_eq!(usage.total(), 1250);
        // A new day starts the client at zero again.
        assert_eq!(usage.sent_on("a", 11), 0);
        usage.add_on("a", 5, 11);
        assert_eq!(usage.sent_on("a", 11), 5);
    }

    #[test]
    fn no_allowance_means_no_limit_and_an_allowance_is_reached_not_exceeded() {
        let usage = Usage::default();
        usage.add("a", 1_000_000);
        assert!(!usage.used_up("a", 0));
        assert!(!usage.used_up("a", 1_000_001));
        assert!(usage.used_up("a", 1_000_000));
        assert!(!usage.used_up("someone else", 1_000_000));
    }

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

    #[test]
    fn an_ipv6_visitor_is_one_client_across_its_whole_network() {
        let mut one = HeaderMap::new();
        one.insert("cf-connecting-ip", "2001:db8:1:2::1".parse().unwrap());
        let mut other = HeaderMap::new();
        other.insert(
            "cf-connecting-ip",
            "2001:db8:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap(),
        );
        let mut elsewhere = HeaderMap::new();
        elsewhere.insert("cf-connecting-ip", "2001:db8:1:3::1".parse().unwrap());
        assert_eq!(client_of(&one, None), client_of(&other, None));
        assert_ne!(client_of(&one, None), client_of(&elsewhere, None));
    }

    #[test]
    fn clients_from_earlier_days_are_forgotten_when_a_new_one_arrives_past_the_cap() {
        let usage = Usage::default();
        for n in 0..10_001 {
            usage.add_on(&format!("old-{n}"), 1, 1);
        }
        usage.add_on("new", 1, 2);
        assert_eq!(usage.today.lock().unwrap().len(), 1);
    }
}
