//! Posters and banners, made the size they are shown at.
//!
//! The providers' pictures are far bigger than a tile: an AniList cover is 150KB for a 176-pixel
//! tile. Resizing them here, once per size, and sending them as immutable files means the edge
//! caches each one for good and a browser never asks again. The previous way, the site's own image
//! optimiser, answered `Vary: Accept`, which Cloudflare will not cache, so every picture made the
//! trip to this server on every view.
//!
//! The upstream is a host this server was told about and nothing else: the address comes from a
//! request, so it is parsed and allowlisted rather than trusted, and so is every redirect it follows.

use bytes::Bytes;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use image::{DynamicImage, ImageReader, imageops::FilterType};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Cursor;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The widths a picture is made at. Anything else is rounded up to the next, so the number of
/// files per picture is small and the cache is shared between visitors.
pub const WIDTHS: [u32; 11] = [96, 176, 256, 352, 512, 640, 828, 1080, 1280, 1600, 1920];
/// The qualities on offer. The page asks for one and gets the nearest.
const QUALITIES: [u8; 3] = [60, 75, 85];
/// What an upstream picture may weigh, and how many pixels it may hold once decoded.
const MAX_BYTES: usize = 15 * 1024 * 1024;
const MAX_PIXELS: u64 = 40_000_000;

#[derive(Debug, Clone)]
pub struct Picture {
    pub bytes: Bytes,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The address is not one this server fetches from.
    Refused,
    /// The upstream did not give a picture.
    Unavailable,
}

type Flight = Shared<BoxFuture<'static, Result<Picture, Error>>>;

pub struct Images {
    client: reqwest::Client,
    dir: PathBuf,
    hosts: Arc<Vec<String>>,
    /// Only tests, which cannot serve HTTPS from a port on localhost.
    require_https: bool,
    flights: Mutex<HashMap<String, Flight>>,
}

/// The width to make: the smallest on offer that is at least `asked`, or the largest.
pub fn snap_width(asked: u32) -> u32 {
    WIDTHS
        .iter()
        .copied()
        .find(|width| *width >= asked)
        .unwrap_or(WIDTHS[WIDTHS.len() - 1])
}

pub fn snap_quality(asked: u8) -> u8 {
    QUALITIES
        .iter()
        .copied()
        .min_by_key(|quality| quality.abs_diff(asked))
        .unwrap_or(75)
}

fn host_ok(host: &str, allowed: &[String]) -> bool {
    allowed
        .iter()
        .any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
}

/// What a file is, from its first bytes.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// The bytes as they are, with the type their first bytes name. Bytes that are no picture come
/// back empty, which the caller treats as a failure.
fn untouched(bytes: Bytes) -> Picture {
    match sniff(&bytes) {
        Some(content_type) => Picture {
            bytes,
            content_type,
        },
        None => Picture {
            bytes: Bytes::new(),
            content_type: "application/octet-stream",
        },
    }
}

/// The picture at most `width` wide, as a WebP at `quality`. What cannot be read, would lose its
/// transparency, or would not get smaller is returned as it is.
pub fn shrink(bytes: Bytes, width: u32, quality: u8) -> Picture {
    // How big it is is read from the header, before anything is decoded.
    let size = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok());
    let Some((wide, tall)) = size.filter(|(w, h)| u64::from(*w) * u64::from(*h) <= MAX_PIXELS)
    else {
        return untouched(bytes);
    };
    let Ok(picture) = image::load_from_memory(&bytes) else {
        return untouched(bytes);
    };
    // A picture with see-through parts would lose them as a JPEG.
    if picture.color().has_alpha() {
        return untouched(bytes);
    }
    let target = wide.min(width);
    let resized = if target < wide {
        let height = ((u64::from(tall) * u64::from(target)) / u64::from(wide)).max(1) as u32;
        picture.resize_exact(target, height, FilterType::CatmullRom)
    } else {
        picture
    };
    // WebP is a third smaller than a JPEG of the same look, and every browser still in use reads it.
    let rgb = DynamicImage::ImageRgb8(resized.to_rgb8());
    let kept = webp::Encoder::from_rgb(rgb.as_bytes(), rgb.width(), rgb.height())
        .encode(f32::from(quality))
        .to_vec();
    if !kept.is_empty() && kept.len() < bytes.len() {
        Picture {
            bytes: Bytes::from(kept),
            content_type: "image/webp",
        }
    } else {
        untouched(bytes)
    }
}

impl Images {
    pub fn new(dir: PathBuf, hosts: Vec<String>) -> Arc<Images> {
        Images::build(dir, hosts, true)
    }

    fn build(dir: PathBuf, hosts: Vec<String>, require_https: bool) -> Arc<Images> {
        let _ = std::fs::create_dir_all(&dir);
        let hosts = Arc::new(hosts);
        // Redirects are followed only while they stay on allowed hosts: metahub's `images.` host
        // sends a picture on to `live.`, and a redirect anywhere else has left the allowlist.
        let on_list = hosts.clone();
        let client = reqwest::Client::builder()
            .user_agent(akidev_core::http::USER_AGENT)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                let allowed = attempt
                    .url()
                    .host_str()
                    .is_some_and(|host| host_ok(host, &on_list));
                if attempt.previous().len() >= 3 || !allowed {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .expect("the image client builds");
        Arc::new(Images {
            client,
            dir,
            hosts,
            require_https,
            flights: Mutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    pub fn for_tests(dir: PathBuf, hosts: Vec<String>) -> Arc<Images> {
        Images::build(dir, hosts, false)
    }

    fn allowed(&self, url: &str) -> Option<reqwest::Url> {
        let url = reqwest::Url::parse(url).ok()?;
        let scheme_ok = url.scheme() == "https" || (!self.require_https && url.scheme() == "http");
        if !scheme_ok || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        host_ok(url.host_str()?, &self.hosts).then_some(url)
    }

    fn path(&self, url: &str, width: u32, quality: u8) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        self.dir
            .join(format!("{:016x}-{width}-{quality}", hasher.finish()))
    }

    /// The picture, from disk when it was made before, otherwise fetched and made. Requests for the
    /// same picture made together share one fetch.
    pub async fn get(
        self: &Arc<Self>,
        url: &str,
        asked_width: u32,
        asked_quality: u8,
    ) -> Result<Picture, Error> {
        let parsed = self.allowed(url).ok_or(Error::Refused)?;
        let width = snap_width(asked_width.max(1));
        let quality = snap_quality(asked_quality);
        let path = self.path(url, width, quality);

        if let Ok(bytes) = tokio::fs::read(&path).await
            && let Some(content_type) = sniff(&bytes)
        {
            return Ok(Picture {
                bytes: Bytes::from(bytes),
                content_type,
            });
        }

        let key = path.display().to_string();
        let flight = {
            let mut flights = self.flights.lock().unwrap();
            if let Some(running) = flights.get(&key) {
                running.clone()
            } else {
                let images = self.clone();
                let forget = key.clone();
                let flight = async move {
                    let made = images.make(parsed, width, quality, path).await;
                    images.flights.lock().unwrap().remove(&forget);
                    made
                }
                .boxed()
                .shared();
                flights.insert(key, flight.clone());
                flight
            }
        };
        flight.await
    }

    async fn make(
        &self,
        url: reqwest::Url,
        width: u32,
        quality: u8,
        path: PathBuf,
    ) -> Result<Picture, Error> {
        let response = self.client.get(url).send().await.map_err(|error| {
            tracing::debug!("[img] fetch failed: {}", error.without_url());
            Error::Unavailable
        })?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|length| length as usize > MAX_BYTES)
        {
            return Err(Error::Unavailable);
        }
        let bytes = akidev_core::http::read_capped(response, MAX_BYTES)
            .await
            .map_err(|_| Error::Unavailable)?;
        let made = tokio::task::spawn_blocking(move || shrink(Bytes::from(bytes), width, quality))
            .await
            .map_err(|_| Error::Unavailable)?;
        if sniff(&made.bytes).is_none() {
            return Err(Error::Unavailable);
        }
        // Written whole and then moved into place, so a reader never sees half a picture.
        let partial = path.with_extension("partial");
        if tokio::fs::write(&partial, &made.bytes).await.is_ok() {
            let _ = tokio::fs::rename(&partial, &path).await;
        }
        Ok(made)
    }

    /// Deletes the oldest pictures until the directory is within `limit` bytes.
    pub async fn prune(&self, limit: u64) {
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
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

    #[cfg(test)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;
    use image::{ImageBuffer, Rgb, RgbImage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A picture with enough detail that it does not compress to nothing.
    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        use image::ImageEncoder as _;
        use image::codecs::jpeg::JpegEncoder;
        let pixels: RgbImage = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([
                (x * 7 % 256) as u8,
                (y * 13 % 256) as u8,
                ((x ^ y) * 3 % 256) as u8,
            ])
        });
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 98)
            .write_image(
                pixels.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        out
    }

    #[test]
    fn a_width_is_rounded_up_to_one_on_offer() {
        assert_eq!(snap_width(1), 96);
        assert_eq!(snap_width(176), 176);
        assert_eq!(snap_width(177), 256);
        assert_eq!(snap_width(384), 512);
        assert_eq!(snap_width(2880), 1920);
        assert_eq!(snap_quality(75), 75);
        assert_eq!(snap_quality(10), 60);
        assert_eq!(snap_quality(95), 85);
    }

    #[test]
    fn a_big_picture_is_made_smaller_and_keeps_its_shape() {
        let original = Bytes::from(jpeg(1900, 400));
        let made = shrink(original.clone(), 640, 75);
        assert_eq!(made.content_type, "image/webp");
        assert!(
            made.bytes.len() < original.len() / 2,
            "{} vs {}",
            made.bytes.len(),
            original.len()
        );
        let decoded = image::load_from_memory(&made.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (640, 134));
    }

    #[test]
    fn a_picture_is_never_made_bigger() {
        let original = Bytes::from(jpeg(200, 300));
        let made = shrink(original.clone(), 1920, 75);
        let decoded = image::load_from_memory(&made.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (200, 300));
        assert!(made.bytes.len() <= original.len());
    }

    #[test]
    fn what_cannot_be_read_is_not_a_picture() {
        let made = shrink(Bytes::from_static(b"<html>not an image</html>"), 640, 75);
        assert!(made.bytes.is_empty());
    }

    #[test]
    fn a_picture_with_transparency_is_left_alone() {
        let rgba: image::RgbaImage =
            ImageBuffer::from_fn(800, 800, |x, y| image::Rgba([x as u8, y as u8, 9, 128]));
        use image::ImageEncoder as _;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(rgba.as_raw(), 800, 800, image::ExtendedColorType::Rgba8)
            .unwrap();
        let made = shrink(Bytes::from(png.clone()), 96, 75);
        assert_eq!(made.content_type, "image/png");
        assert_eq!(made.bytes.len(), png.len());
    }

    #[test]
    fn only_listed_hosts_over_https_are_fetched() {
        let images = Images::new(
            std::env::temp_dir().join("akidev-img-allow"),
            vec!["anilist.co".into()],
        );
        assert!(images.allowed("https://s4.anilist.co/file/a.jpg").is_some());
        assert!(images.allowed("https://anilist.co/a.jpg").is_some());
        assert!(images.allowed("http://s4.anilist.co/a.jpg").is_none());
        assert!(images.allowed("https://evil.com/anilist.co").is_none());
        assert!(
            images
                .allowed("https://anilist.co.evil.com/a.jpg")
                .is_none()
        );
        assert!(images.allowed("https://xanilist.co/a.jpg").is_none());
        assert!(
            images
                .allowed("https://user:pw@s4.anilist.co/a.jpg")
                .is_none()
        );
        assert!(
            images
                .allowed("http://169.254.169.254/latest/meta-data")
                .is_none()
        );
        assert!(images.allowed("not a url").is_none());
    }

    #[tokio::test]
    async fn a_picture_is_fetched_once_made_once_and_then_read_from_disk() {
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        let source = jpeg(1500, 500);
        let app = Router::new()
            .route(
                "/poster.jpg",
                get(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    let source = source.clone();
                    async move { ([("content-type", "image/jpeg")], source) }
                }),
            )
            .route(
                "/missing.jpg",
                get(|| async { axum::http::StatusCode::NOT_FOUND }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let dir = std::env::temp_dir().join(format!("akidev-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let images = Images::for_tests(dir.clone(), vec!["127.0.0.1".into()]);
        let url = format!("http://127.0.0.1:{port}/poster.jpg");

        // Four at once make one fetch.
        let together = futures::future::join_all((0..4).map(|_| images.get(&url, 300, 75))).await;
        assert!(
            together
                .iter()
                .all(|made| made.as_ref().is_ok_and(|p| p.content_type == "image/webp"))
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let first = together[0].clone().unwrap();
        let decoded = image::load_from_memory(&first.bytes).unwrap();
        assert_eq!(decoded.width(), 352);

        // A second request is the file on disk, and a new size is made from a new fetch.
        let again = images.get(&url, 300, 75).await.unwrap();
        assert_eq!(again.bytes, first.bytes);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        images.get(&url, 1000, 75).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        assert_eq!(
            images
                .get(&format!("http://127.0.0.1:{port}/missing.jpg"), 300, 75)
                .await
                .unwrap_err(),
            Error::Unavailable
        );
        assert_eq!(
            images
                .get("https://evil.example/a.jpg", 300, 75)
                .await
                .unwrap_err(),
            Error::Refused
        );

        // The oldest go first when the directory is over its limit.
        images.prune(1).await;
        let left = std::fs::read_dir(images.dir()).unwrap().count();
        assert_eq!(left, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
