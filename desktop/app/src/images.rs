//! Loads poster and banner images for GPUI.
//!
//! GPUI's default HTTP client fails every request on desktop, so this one fetches with
//! reqwest. Responses are kept on disk, because GPUI's own cache lives in memory only and
//! every launch would otherwise download every poster again.

use anyhow::Context as _;
use futures::future::BoxFuture;
use gpui_kit::http_client::{AsyncBody, HttpClient, Request, Response, Url, http::HeaderValue};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Cached images not touched for this long are removed at startup.
const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub struct ImageClient {
    dir: PathBuf,
}

impl ImageClient {
    pub fn new(dir: PathBuf) -> ImageClient {
        let _ = std::fs::create_dir_all(&dir);
        remove_stale(&dir);
        ImageClient { dir }
    }
}

fn remove_stale(dir: &PathBuf) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok());
        if age.is_some_and(|age| age > MAX_AGE) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl HttpClient for ImageClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let url = request.uri().to_string();
        let dir = self.dir.clone();
        Box::pin(async move {
            let bytes = crate::runtime::run(fetch(url, dir))
                .await
                .context("the image task failed")??;
            Ok(Response::builder()
                .status(200)
                .body(AsyncBody::from_bytes(bytes.into()))?)
        })
    }
}

async fn fetch(url: String, dir: PathBuf) -> anyhow::Result<Vec<u8>> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let path = dir.join(format!("{:016x}", hasher.finish()));

    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(bytes);
    }
    let response = akidev_core::http::client()
        .get(&url)
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?.to_vec();
    // Written to a temporary name first, so a half-written file is never read back.
    let partial = path.with_extension("part");
    if tokio::fs::write(&partial, &bytes).await.is_ok() {
        let _ = tokio::fs::rename(&partial, &path).await;
    }
    Ok(bytes)
}
