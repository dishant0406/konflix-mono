//! Serves the chosen file over HTTP on localhost, with byte-range support.
//!
//! The player reads the video from this address. Range support is what makes
//! seeking work: a seek is a new request starting at the target byte, and the torrent
//! reader then prioritises the pieces from there.

use crate::torrent::Engine;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::StreamExt;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// What every request is checked against.
struct Guard {
    engine: Arc<Engine>,
    /// The only `Host` header value a local player sends.
    host: String,
    token: String,
}

/// Where the server listens and the secret every URL it hands out carries.
pub struct Server {
    pub address: SocketAddr,
    pub token: String,
}

/// Starts the server on a free localhost port. A web page can request any localhost
/// port, so the path carries a random per-session token that only the player is given.
pub async fn start(engine: Arc<Engine>) -> anyhow::Result<Server> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let token = new_token()?;
    let guard = Arc::new(Guard {
        engine,
        host: address.to_string(),
        token: token.clone(),
    });
    let app = Router::new()
        .route("/stream/{token}/{info_hash}/{file_index}", get(stream))
        .with_state(guard);
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!("[serve] stopped: {error}");
        }
    });
    Ok(Server { address, token })
}

/// The URL a player reads one file of a torrent from.
pub fn url(address: SocketAddr, token: &str, info_hash: &str, file_index: usize) -> String {
    format!("http://{address}/stream/{token}/{info_hash}/{file_index}")
}

/// 128 random bits from the operating system, as hex.
fn new_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Compares without stopping at the first difference, so response timing does not
/// reveal how much of a guessed token was right.
fn same_token(expected: &str, given: &str) -> bool {
    if expected.len() != given.len() {
        return false;
    }
    let differing = expected
        .bytes()
        .zip(given.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));
    differing == 0
}

/// Parses a `Range` header against a file size. `None` means the header cannot be
/// satisfied, which is answered with 416 and never with the whole file.
pub fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    let (start, end) = spec.split_once('-')?;
    if start.is_empty() && end.is_empty() {
        return None;
    }
    if start.is_empty() {
        // A suffix range: the last `length` bytes. "bytes=-0" asks for nothing.
        let length: u64 = end.parse().ok().filter(|&length| length > 0)?;
        return Some((size.saturating_sub(length), size.checked_sub(1)?));
    }
    let start: u64 = start.parse().ok()?;
    let end = if end.is_empty() {
        size.checked_sub(1)?
    } else {
        end.parse::<u64>().ok()?.min(size.checked_sub(1)?)
    };
    (start <= end && start < size).then_some((start, end))
}

fn content_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("avi") => "video/x-msvideo",
        _ => "application/octet-stream",
    }
}

async fn stream(
    State(guard): State<Arc<Guard>>,
    Path((token, info_hash, file_index)): Path<(String, String, usize)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    // A request carrying a foreign Host header is a DNS-rebinding page reaching this
    // port through its own domain name. It does not stop a page that requests the
    // address directly: the token does that.
    let local_host = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| host == guard.host);
    if !local_host {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !same_token(&guard.token, &token) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (playable, mut file, reader) = match guard.engine.stream(&info_hash, file_index).await {
        Ok(opened) => opened,
        Err(error) => {
            tracing::warn!("[serve] {error:#}");
            return (
                StatusCode::GONE,
                "This stream has expired. Reopen the title.",
            )
                .into_response();
        }
    };
    let size = playable.length;

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
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(content_type(&playable.file_name)),
        )
        .header(header::CONTENT_LENGTH, length);
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }

    if method == Method::HEAD || length == 0 {
        return response.body(Body::empty()).unwrap();
    }

    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    // The reader guard rides along with the body, so the torrent stays pinned until
    // the response ends or the player hangs up.
    let body = ReaderStream::new(file.take(length)).map(move |chunk| {
        let _pinned = &reader;
        chunk
    });
    response.body(Body::from_stream(body)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::{new_token, parse_range, same_token, url};

    #[test]
    fn a_token_is_128_random_bits_as_hex() {
        let token = new_token().unwrap();
        assert_eq!(token.len(), 32);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(token, new_token().unwrap());
    }

    #[test]
    fn only_the_exact_token_is_accepted() {
        assert!(same_token("abc123", "abc123"));
        assert!(!same_token("abc123", "abc124"));
        assert!(!same_token("abc123", "abc12"));
        assert!(!same_token("abc123", ""));
    }

    #[test]
    fn the_url_names_the_token_torrent_and_file() {
        let address = "127.0.0.1:4000".parse().unwrap();
        assert_eq!(
            url(address, "tok", "abcd", 5),
            "http://127.0.0.1:4000/stream/tok/abcd/5"
        );
    }

    #[test]
    fn open_ended_and_bounded_ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=90-500", 100), Some((90, 99)));
    }

    #[test]
    fn a_suffix_range_counts_from_the_end() {
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-500", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=-0", 100), None);
    }

    #[test]
    fn unsatisfiable_or_malformed_ranges_are_refused() {
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=50-10", 100), None);
        assert_eq!(parse_range("bytes=-", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
        assert_eq!(parse_range("bytes=a-b", 100), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
    }
}
