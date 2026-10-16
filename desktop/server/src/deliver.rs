//! The URLs a player is given, and what answers them.
//!
//!   /s/{token}                the file, with range support
//!   /s/{token}/index.m3u8     the HLS playlist
//!   /s/{token}/init.mp4       and its segments, seg00000.m4s ...
//!
//! A token is 192 random bits that map to one delivery in memory: whoever holds it can watch that
//! episode, and nothing else. It is handed out by `/v1/play`, expires after a few hours, and
//! vanishes with a restart, which the player answers by asking for a source again.

use crate::hls::{Hls, SegmentError, Session};
use crate::limits::{Usage, client_of};
use crate::origin::Origin;
use akidev_core::serve::parse_range;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::TryStreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

const TOKEN_TTL: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_TOKENS: usize = 2000;

#[derive(Clone)]
pub enum Delivery {
    /// The file as it is, which is an MP4 a browser plays.
    Direct {
        origin: Arc<Origin>,
    },
    Hls {
        session: Arc<Session>,
    },
}

struct Entry {
    delivery: Delivery,
    expires: Instant,
}

#[derive(Default)]
pub struct Registry {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Registry {
    pub fn register(&self, delivery: Delivery) -> String {
        let mut bytes = [0u8; 24];
        getrandom::fill(&mut bytes).expect("the system has randomness");
        let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();

        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap();
        // Expired tokens go first, so a burst of new ones cannot evict one someone is using.
        entries.retain(|_, entry| entry.expires > now);
        while entries.len() >= MAX_TOKENS {
            let oldest = entries
                .iter()
                .min_by_key(|(_, entry)| entry.expires)
                .map(|(token, _)| token.clone());
            match oldest {
                Some(oldest) => entries.remove(&oldest),
                None => break,
            };
        }
        entries.insert(
            token.clone(),
            Entry {
                delivery,
                expires: now + TOKEN_TTL,
            },
        );
        token
    }

    /// Unknown and expired are the same answer, so a token cannot be probed.
    pub fn get(&self, token: &str) -> Option<Delivery> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get(token)?;
        if entry.expires <= Instant::now() {
            entries.remove(token);
            return None;
        }
        Some(entry.delivery.clone())
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

fn expired() -> Response {
    json_error(
        StatusCode::NOT_FOUND,
        "This stream has expired. Reload the episode.",
    )
}

/// Token shape: the hex this server issues. Checked before the token is used as a key.
fn well_formed(token: &str) -> bool {
    token.len() == 48 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Clone)]
pub struct Deliverer {
    pub registry: Arc<Registry>,
    pub hls: Arc<Hls>,
    pub usage: Arc<Usage>,
    /// Video bytes one client may be sent in a day. Zero means no limit.
    pub daily_allowance: u64,
}

impl Deliverer {
    /// The client's address, or `None` if it has been sent its day's allowance of video.
    fn admit(&self, headers: &HeaderMap, peer: SocketAddr) -> Option<String> {
        let client = client_of(headers, Some(peer));
        (!self.usage.used_up(&client, self.daily_allowance)).then_some(client)
    }

    /// A video body that adds what is actually sent to the client's count for the day.
    fn counted<S>(&self, client: String, stream: S) -> Body
    where
        S: futures::Stream<Item = std::io::Result<axum::body::Bytes>> + Send + 'static,
    {
        let usage = self.usage.clone();
        Body::from_stream(stream.inspect_ok(move |chunk| usage.add(&client, chunk.len() as u64)))
    }
}

pub async fn direct(
    State(deliverer): State<Deliverer>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(token): Path<String>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if !well_formed(&token) {
        return json_error(StatusCode::BAD_REQUEST, "Invalid stream token.");
    }
    let Some(client) = deliverer.admit(&headers, peer) else {
        return crate::api::daily_limit();
    };
    match deliverer.registry.get(&token) {
        Some(Delivery::Direct { origin }) => {
            serve_range(&deliverer, client, &origin, method, &headers).await
        }
        _ => expired(),
    }
}

pub async fn hls(
    State(deliverer): State<Deliverer>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((token, file)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !well_formed(&token) {
        return json_error(StatusCode::BAD_REQUEST, "Invalid stream token.");
    }
    let Some(client) = deliverer.admit(&headers, peer) else {
        return crate::api::daily_limit();
    };
    let Some(Delivery::Hls { session }) = deliverer.registry.get(&token) else {
        return expired();
    };

    if file == "index.m3u8" {
        return match deliverer.hls.playlist(&session).await {
            Some(text) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
                .header(header::CONTENT_LENGTH, text.len())
                // The playlist grows as ffmpeg works, so a cached copy would freeze the stream.
                .header(header::CACHE_CONTROL, "no-store")
                .body(Body::from(text))
                .unwrap(),
            None => expired(),
        };
    }

    match deliverer.hls.segment(&session, &file).await {
        Ok((path, length)) => match tokio::fs::File::open(&path).await {
            Ok(opened) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "video/mp4")
                .header(header::CONTENT_LENGTH, length)
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .body(deliverer.counted(client, ReaderStream::new(opened)))
                .unwrap(),
            Err(_) => json_error(StatusCode::NOT_FOUND, "That segment is not available yet."),
        },
        Err(SegmentError::Invalid) => json_error(StatusCode::BAD_REQUEST, "Invalid segment."),
        Err(SegmentError::Expired) => expired(),
        Err(SegmentError::NotYet) => {
            json_error(StatusCode::NOT_FOUND, "That segment is not available yet.")
        }
    }
}

/// An MP4 with range support. A range that cannot be satisfied is answered with 416, never with
/// the whole file.
async fn serve_range(
    deliverer: &Deliverer,
    client: String,
    origin: &Origin,
    method: Method,
    headers: &HeaderMap,
) -> Response {
    let size = origin.length();
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let (status, start, end) = match range {
        None => (StatusCode::OK, 0, size.saturating_sub(1)),
        Some(value) => match parse_range(value, size) {
            Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
            None => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                    .body(Body::empty())
                    .unwrap();
            }
        },
    };
    let length = if size == 0 { 0 } else { end - start + 1 };

    let mut response = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"))
        .header(header::CONTENT_LENGTH, length)
        // `private`: a shared cache must never hold one viewer's video for another.
        .header(header::CACHE_CONTROL, "private, max-age=3600");
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    if method == Method::HEAD || length == 0 {
        return response.body(Body::empty()).unwrap();
    }

    let opened = match origin.open_at(start).await {
        Ok(opened) => opened,
        Err(error) => {
            tracing::warn!("[deliver] {error:#}");
            return json_error(StatusCode::BAD_GATEWAY, "The source could not be read.");
        }
    };
    // The reader carries its pin with it, so a torrent stays held until the response ends or the
    // player hangs up.
    response
        .body(deliverer.counted(client, ReaderStream::new(opened.take(length))))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_delivery() -> Delivery {
        Delivery::Direct {
            origin: Arc::new(Origin::File {
                path: "/nope".into(),
                length: 1,
                name: "x.mp4".into(),
            }),
        }
    }

    #[test]
    fn a_token_is_192_random_bits_of_hex_and_finds_its_delivery() {
        let registry = Registry::default();
        let token = registry.register(direct_delivery());
        assert!(well_formed(&token));
        assert!(registry.get(&token).is_some());
        assert!(registry.get(&"0".repeat(48)).is_none());
        assert_ne!(token, registry.register(direct_delivery()));
    }

    #[test]
    fn a_token_has_to_be_the_shape_this_server_issues() {
        assert!(!well_formed("short"));
        assert!(!well_formed(&"g".repeat(48)));
        assert!(!well_formed(&"../".repeat(16)));
    }

    #[test]
    fn the_registry_is_bounded() {
        let registry = Registry::default();
        for _ in 0..MAX_TOKENS + 10 {
            registry.register(direct_delivery());
        }
        assert!(registry.len() <= MAX_TOKENS);
    }
}
