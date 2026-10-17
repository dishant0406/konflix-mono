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
        // Files from before images were shrunk have no suffix, and are only taking room.
        let unshrunk = !entry.file_name().to_string_lossy().ends_with("-s");
        if unshrunk || age.is_some_and(|age| age > MAX_AGE) {
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

/// The longest sides an image is kept at: twice the size it is drawn at, for a sharp picture on
/// a screen with twice the pixels. A poster is drawn at 176 by 264, a banner at the window's width.
const POSTER_LIMIT: (u32, u32) = (352, 528);
const BANNER_LIMIT: (u32, u32) = (1600, 900);

/// `bytes` as a picture no bigger than it needs to be. What cannot be read, or is already small
/// enough, or would not get smaller, is returned as it is.
fn shrink(bytes: Vec<u8>) -> Vec<u8> {
    use image::{DynamicImage, ImageEncoder as _, codecs::jpeg::JpegEncoder, imageops::FilterType};
    let Ok(picture) = image::load_from_memory(&bytes) else {
        return bytes;
    };
    let (width, height) = (picture.width(), picture.height());
    let (wide, tall) = if height > width {
        POSTER_LIMIT
    } else {
        BANNER_LIMIT
    };
    // A picture with see-through parts would lose them as a JPEG.
    if (width <= wide && height <= tall) || picture.color().has_alpha() {
        return bytes;
    }
    let small = DynamicImage::ImageRgb8(picture.resize(wide, tall, FilterType::Lanczos3).to_rgb8());
    let mut kept = Vec::new();
    let encoded = JpegEncoder::new_with_quality(&mut kept, 88).write_image(
        small.as_bytes(),
        small.width(),
        small.height(),
        small.color().into(),
    );
    if encoded.is_ok() && kept.len() < bytes.len() {
        kept
    } else {
        bytes
    }
}

async fn fetch(url: String, dir: PathBuf) -> anyhow::Result<Vec<u8>> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let path = dir.join(format!("{:016x}-s", hasher.finish()));

    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(bytes);
    }
    let response = akidev_core::http::client()
        .get(&url)
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?.to_vec();
    // Shrunk before it is kept, so what is decoded, held and drawn is no bigger than it is shown.
    let bytes = tokio::task::spawn_blocking(move || shrink(bytes)).await?;
    // Written to a temporary name first, so a half-written file is never read back.
    let partial = path.with_extension("part");
    if tokio::fs::write(&partial, &bytes).await.is_ok() {
        let _ = tokio::fs::rename(&partial, &path).await;
    }
    Ok(bytes)
}

/// How many decoded pictures are kept. A page shows a few dozen; the rest of what was scrolled
/// past comes back from the disk copy, which is quick, instead of staying decoded in memory
/// (a poster is about 0.7 MB decoded) and on the graphics card for as long as the app runs.
const KEPT: usize = 96;

/// Decoded pictures, with the least recently drawn dropped past `KEPT`. GPUI's default keeps
/// every picture it has ever drawn.
pub struct PictureCache {
    items: std::collections::HashMap<gpui_kit::Resource, (gpui_kit::ImageCacheItem, u64)>,
    clock: u64,
}

impl PictureCache {
    pub fn new(cx: &mut gpui_kit::App) -> gpui_kit::Entity<PictureCache> {
        use gpui_kit::AppContext as _;
        let cache = cx.new(|_| PictureCache {
            items: Default::default(),
            clock: 0,
        });
        cx.observe_release(&cache, |cache, cx| {
            for (_, (item, _)) in std::mem::take(&mut cache.items) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        cache
    }
}

impl gpui_kit::ImageCache for PictureCache {
    fn load(
        &mut self,
        resource: &gpui_kit::Resource,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> Option<Result<std::sync::Arc<gpui_kit::RenderImage>, gpui_kit::ImageCacheError>> {
        self.clock += 1;
        let clock = self.clock;
        let entry = self
            .items
            .entry(resource.clone())
            .or_insert_with(|| (gpui_kit::ImageCacheItem::new(resource, cx), clock));
        entry.1 = clock;
        let loaded = entry.0.use_image(window);
        if self.items.len() > KEPT {
            // Only what the last frames did not draw is dropped, so nothing on screen flickers.
            let mut by_age: Vec<_> = self.items.iter().map(|(key, (_, at))| (*at, key.clone())).collect();
            by_age.sort_unstable_by_key(|(at, _)| *at);
            for (_, key) in by_age.into_iter().take(self.items.len() - KEPT * 3 / 4) {
                if let Some((item, _)) = self.items.remove(&key)
                    && let Some(Ok(image)) = item.get()
                {
                    cx.drop_image(image, Some(window));
                }
            }
        }
        loaded
    }
}
