//! Music for the web: the catalogue comes from `akidev_core::music`, shared with the desktop app. What
//! is here is the one thing only a server does: fetching a track's audio from YouTube and handing it
//! on. The address yt-dlp finds works only from the machine that asked, so the browser cannot be
//! sent to it.

use akidev_core::music::{Music, Resolved};
use anyhow::anyhow;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;
use std::sync::OnceLock;
use std::time::Duration;

pub use akidev_core::music::is_video_id;

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("an HTTP client")
    })
}

/// The audio of a track, passed on from YouTube. `range` is the browser's `Range` header, which is
/// what lets it seek.
pub async fn stream(music: &Music, video_id: &str, range: Option<&HeaderValue>) -> Response {
    let resolved = match music.resolve(video_id).await {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(video_id, %error, "music: no stream address");
            return status(
                StatusCode::BAD_GATEWAY,
                "Could not find audio for that track.",
            );
        }
    };
    match fetch(&resolved, range).await {
        Ok(response) => response,
        Err(error) => {
            // An address can die early (YouTube rotated it): forget it, so the next try asks again.
            tracing::warn!(video_id, %error, "music: audio fetch failed");
            music.forget(video_id);
            status(StatusCode::BAD_GATEWAY, "The audio did not load.")
        }
    }
}

async fn fetch(resolved: &Resolved, range: Option<&HeaderValue>) -> anyhow::Result<Response> {
    let mut request = http().get(&resolved.url);
    for (name, value) in &resolved.headers {
        request = request.header(name, value);
    }
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    let upstream = request.send().await?;
    let code = upstream.status();
    if !(code == StatusCode::OK || code == StatusCode::PARTIAL_CONTENT) {
        return Err(anyhow!("YouTube answered {code}"));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(resolved.content_type),
    );
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    for name in [header::CONTENT_LENGTH, header::CONTENT_RANGE] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    // The same audio always, so the browser may keep it, but only for this listener.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("cross-origin"),
    );
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = code;
    *response.headers_mut() = headers;
    Ok(response)
}

fn status(code: StatusCode, message: &str) -> Response {
    let mut response = Response::new(Body::from(
        serde_json::json!({ "error": message }).to_string(),
    ));
    *response.status_mut() = code;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}
