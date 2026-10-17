//! The whole delivery path against real ffmpeg: a generated file stands in for a swarm.
//!
//! Each test makes a short clip in the shape being tested (an MKV, FLAC audio, MPEG-4 video), asks
//! the player to deliver it, and fetches what a browser would, through the real router. They need
//! ffmpeg and ffprobe, and say so and pass when those are missing.

use crate::api::{self, AppState};
use crate::config::Settings;
use crate::deliver::Registry;
use crate::hls::Hls;
use crate::limits::Limiter;
use crate::media;
use crate::origin::Origin;
use crate::play::Player;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tower::ServiceExt;

struct Rig {
    router: Router,
    player: Arc<Player>,
    hls: Arc<Hls>,
    dir: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.hls.shutdown();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("akidev-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn rig(name: &str, tweak: impl FnOnce(&mut Settings)) -> Option<Rig> {
    if !media::ffmpeg_available().await {
        eprintln!("ffmpeg is not installed: skipping {name}");
        return None;
    }
    let dir = scratch(name);
    let mut settings = Settings::from_env();
    settings.cache_dir = dir.clone();
    settings.public_url = "http://api.test".into();
    settings.stream_url = "http://api.test".into();
    settings.stream_sources = vec!["torrent".into()];
    tweak(&mut settings);
    let settings = Arc::new(settings);
    let hls = Hls::new(settings.clone(), vec![]);
    let registry = Arc::new(Registry::default());
    let player = Player::new(settings.clone(), None, None, hls.clone(), registry.clone());
    let state = AppState {
        settings,
        player: player.clone(),
        hls: hls.clone(),
        registry,
        limiter: Arc::new(Limiter::default()),
        usage: Arc::new(crate::limits::Usage::default()),
        engine: None,
        subs: Arc::new(crate::subs::Subs::new(dir.join("subs"))),
        images: crate::img::Images::for_tests(dir.join("img"), vec!["127.0.0.1".into()]),
        music: Arc::new(akidev_core::music::Music::new("yt-dlp")),
        ffmpeg: true,
    };
    Some(Rig {
        router: api::router(state),
        player,
        hls,
        dir,
    })
}

/// A short clip: 320x180, 10 seconds, with the given codecs in the given container.
fn clip(dir: &Path, file: &str, video: &[&str], audio: &[&str]) -> PathBuf {
    let path = dir.join(file);
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=24",
        ])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
        .args(["-t", "10"])
        .args(video)
        .args(audio)
        .arg(&path)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "could not make {file}");
    path
}

fn origin(path: &Path) -> Origin {
    Origin::File {
        path: path.to_path_buf(),
        length: std::fs::metadata(path).unwrap().len(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
    }
}

const H264: [&str; 8] = [
    "-c:v",
    "libx264",
    "-preset",
    "ultrafast",
    "-pix_fmt",
    "yuv420p",
    "-g",
    "48",
];
const AAC: [&str; 2] = ["-c:a", "aac"];

async fn get(
    router: &Router,
    url: &str,
    range: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let path = url.trim_start_matches("http://api.test");
    let mut request = Request::builder()
        .uri(path)
        .header(header::ORIGIN, "https://akidev.live");
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("192.0.2.1:4000".parse::<SocketAddr>().unwrap()));
    let response = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes().to_vec();
    (parts.status, parts.headers, bytes)
}

async fn deliver(rig: &Rig, path: &Path, hevc: bool) -> Result<crate::dto::Ready, String> {
    rig.player
        .deliver(origin(path), "test release".into(), "torrent", hevc, None)
        .await
        .map_err(|miss| match miss {
            crate::play::Miss::Busy => "busy".to_string(),
            crate::play::Miss::Reason(reason) => reason,
        })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn an_mp4_a_browser_plays_is_served_as_it_is_with_ranges() {
    let Some(rig) = rig("direct", |_| {}).await else {
        return;
    };
    let mut args = H264.to_vec();
    args.extend(["-movflags", "+faststart"]);
    let path = clip(&rig.dir, "a.mp4", &args, &AAC);
    let size = std::fs::metadata(&path).unwrap().len();

    let ready = deliver(&rig, &path, false).await.unwrap();
    assert_eq!(ready.stream_type, "file");
    assert!(ready.url.starts_with("http://api.test/s/") && !ready.url.ends_with(".m3u8"));
    assert!(ready.duration.is_some_and(|d| (d - 10.).abs() < 0.5));

    let (status, headers, body) = get(&rig.router, &ready.url, Some("bytes=0-99")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        headers[header::CONTENT_RANGE],
        format!("bytes 0-99/{size}").as_str()
    );
    assert_eq!(body.len(), 100);
    assert_eq!(headers[header::CONTENT_TYPE], "video/mp4");
    // The page's origin may read it, and the range headers are visible to it.
    assert_eq!(
        headers["access-control-allow-origin"],
        "https://akidev.live"
    );
    assert!(
        headers["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .contains("content-range")
    );

    let (status, _, whole) = get(&rig.router, &ready.url, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(whole, std::fs::read(&path).unwrap());

    let (status, headers, _) = get(&rig.router, &ready.url, Some(&format!("bytes={size}-"))).await;
    assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        headers[header::CONTENT_RANGE],
        format!("bytes */{size}").as_str()
    );
}

#[tokio::test]
async fn an_mkv_is_repackaged_and_every_segment_can_be_fetched() {
    let Some(rig) = rig("mkv", |_| {}).await else {
        return;
    };
    let path = clip(&rig.dir, "a.mkv", &H264, &AAC);

    let ready = deliver(&rig, &path, false).await.unwrap();
    assert_eq!(ready.stream_type, "hls");
    assert!(ready.url.ends_with("/index.m3u8"));

    let (status, headers, playlist) = get(&rig.router, &ready.url, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/vnd.apple.mpegurl"
    );
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let text = String::from_utf8(playlist).unwrap();
    assert!(text.starts_with("#EXTM3U") && text.contains("init.mp4"));

    let base = ready.url.trim_end_matches("index.m3u8").to_string();
    let (status, _, init) = get(&rig.router, &format!("{base}init.mp4"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(contains(&init, b"ftyp"));
    let (status, headers, segment) = get(&rig.router, &format!("{base}seg00000.m4s"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_LENGTH],
        segment.len().to_string().as_str()
    );
    assert!(!segment.is_empty());

    // The last segment is only served once the conversion has finished, and it does.
    let (status, _, _) = get(&rig.router, &format!("{base}seg00001.m4s"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, playlist) = get(&rig.router, &ready.url, None).await;
    assert!(
        String::from_utf8(playlist)
            .unwrap()
            .contains("#EXT-X-ENDLIST")
    );

    // A name that is not a segment never reaches the filesystem.
    let (status, _, _) = get(&rig.router, &format!("{base}..%2Fsecret"), None).await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND);
    let (status, _, _) = get(&rig.router, &format!("{base}seg9.m4s"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn video_that_is_fine_is_copied_and_flac_audio_becomes_aac() {
    let Some(rig) = rig("audio", |_| {}).await else {
        return;
    };
    let path = clip(&rig.dir, "flac.mkv", &H264, &["-c:a", "flac"]);
    let ready = deliver(&rig, &path, false).await.unwrap();
    let base = ready.url.trim_end_matches("index.m3u8").to_string();
    let (_, _, init) = get(&rig.router, &format!("{base}init.mp4"), None).await;
    assert!(contains(&init, b"mp4a"), "audio should be AAC");
    assert!(!contains(&init, b"fLaC"));
    assert!(contains(&init, b"avc1"), "video should still be H.264");
}

#[tokio::test]
async fn video_a_browser_cannot_decode_is_encoded_and_only_one_at_a_time() {
    let Some(rig) = rig("encode", |s| {
        s.max_video_transcodes = 1;
        s.max_hls_sessions = 3;
        s.transcode_threads = 1;
    })
    .await
    else {
        return;
    };
    let first = clip(&rig.dir, "one.mkv", &["-c:v", "mpeg4"], &AAC);
    let second = clip(&rig.dir, "two.mkv", &["-c:v", "mpeg4", "-g", "10"], &AAC);

    let ready = deliver(&rig, &first, false).await.unwrap();
    let base = ready.url.trim_end_matches("index.m3u8").to_string();
    let (_, _, init) = get(&rig.router, &format!("{base}init.mp4"), None).await;
    assert!(
        contains(&init, b"avc1"),
        "the video should have been encoded to H.264"
    );

    // The slot is held while the first session lives, so a second encode is turned away.
    assert_eq!(deliver(&rig, &second, false).await.unwrap_err(), "busy");
    // A copy needs no slot.
    let copy = clip(&rig.dir, "three.mkv", &H264, &AAC);
    assert!(deliver(&rig, &copy, false).await.is_ok());
}

#[tokio::test]
async fn two_viewers_of_one_file_share_one_conversion() {
    let Some(rig) = rig("share", |_| {}).await else {
        return;
    };
    let path = clip(&rig.dir, "a.mkv", &H264, &AAC);
    let a = deliver(&rig, &path, false).await.unwrap();
    let b = deliver(&rig, &path, false).await.unwrap();
    assert_ne!(a.url, b.url, "each viewer has their own token");
    assert_eq!(rig.hls.count(), 1);
}

#[tokio::test]
async fn sessions_are_limited_and_an_active_one_is_not_evicted() {
    let Some(rig) = rig("limit", |s| s.max_hls_sessions = 1).await else {
        return;
    };
    let a = clip(&rig.dir, "a.mkv", &H264, &AAC);
    let b = clip(&rig.dir, "b.mkv", &H264, &["-c:a", "aac", "-ac", "1"]);
    assert!(deliver(&rig, &a, false).await.is_ok());
    assert_eq!(deliver(&rig, &b, false).await.unwrap_err(), "busy");
}

#[tokio::test]
async fn a_destroyed_session_answers_expired_and_leaves_no_files() {
    let Some(rig) = rig("expire", |_| {}).await else {
        return;
    };
    let path = clip(&rig.dir, "a.mkv", &H264, &AAC);
    let ready = deliver(&rig, &path, false).await.unwrap();
    let base = ready.url.trim_end_matches("index.m3u8").to_string();
    assert_eq!(
        get(&rig.router, &format!("{base}seg00000.m4s"), None)
            .await
            .0,
        StatusCode::OK
    );

    rig.hls.shutdown();
    let (status, _, body) = get(&rig.router, &format!("{base}seg00000.m4s"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(&body).contains("expired"));
    let (status, _, _) = get(&rig.router, &ready.url, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The directory goes away in the background.
    let hls_dir = rig.dir.join("hls");
    for _ in 0..40 {
        if std::fs::read_dir(&hls_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the session directory was not removed");
}

#[tokio::test]
async fn a_file_that_is_not_video_is_turned_away() {
    let Some(rig) = rig("novideo", |_| {}).await else {
        return;
    };
    let path = rig.dir.join("a.mkv");
    std::fs::write(&path, b"this is not a video at all").unwrap();
    assert!(deliver(&rig, &path, false).await.is_err());
}

#[tokio::test]
async fn bad_requests_are_refused_before_any_work_is_done() {
    let Some(rig) = rig("validation", |s| s.play_limit = 2).await else {
        return;
    };

    for url in [
        "/v1/catalogue?kind=books",
        "/v1/catalogue?sort=loudest",
        "/v1/search",
        "/v1/search?q=%20%20",
        "/v1/titles/not-an-id",
    ] {
        let (status, _, body) = get(&rig.router, url, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert!(String::from_utf8_lossy(&body).contains("error"), "{url}");
    }

    let post = |body: &'static str| {
        let router = rig.router.clone();
        async move {
            let mut request = Request::builder()
                .method("POST")
                .uri("/v1/play")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo("192.0.2.9:1".parse::<SocketAddr>().unwrap()));
            router.oneshot(request).await.unwrap().status()
        }
    };
    // A title with no episode is not something to play.
    assert_eq!(
        post(r#"{"id":"series-tt0903747"}"#).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(post(r#"{"id":"../../etc"}"#).await, StatusCode::BAD_REQUEST);
    // And the third attempt within the window is told to slow down.
    assert_eq!(
        post(r#"{"id":"nonsense"}"#).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn an_unknown_or_malformed_token_is_not_a_stream() {
    let Some(rig) = rig("token", |_| {}).await else {
        return;
    };
    let (status, _, _) = get(&rig.router, &format!("/s/{}", "0".repeat(48)), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(&rig.router, "/s/short", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = get(
        &rig.router,
        &format!("/s/{}/index.m3u8", "0".repeat(48)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn video_urls_name_the_stream_host_when_there_is_one() {
    let Some(rig) = rig("stream-host", |settings| {
        settings.stream_url = "http://stream.test".into();
    })
    .await
    else {
        return;
    };
    let mut args = H264.to_vec();
    args.extend(["-movflags", "+faststart"]);
    let path = clip(&rig.dir, "a.mp4", &args, &AAC);

    let ready = deliver(&rig, &path, false).await.unwrap();
    assert!(
        ready.url.starts_with("http://stream.test/s/"),
        "{}",
        ready.url
    );
}

#[tokio::test]
async fn video_sent_is_counted_and_a_client_past_its_day_is_turned_away() {
    let Some(rig) = rig("daily", |settings| {
        settings.max_stream_bytes_per_day = 1000;
    })
    .await
    else {
        return;
    };
    let mut args = H264.to_vec();
    args.extend(["-movflags", "+faststart"]);
    let path = clip(&rig.dir, "a.mp4", &args, &AAC);
    let ready = deliver(&rig, &path, false).await.unwrap();

    let (_, _, before) = get(&rig.router, "/health", None).await;
    let before: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(before["video_bytes_sent"], 0);

    // Under the allowance, a range is served and counted.
    let (status, _, body) = get(&rig.router, &ready.url, Some("bytes=0-499")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body.len(), 500);
    let (_, _, after) = get(&rig.router, "/health", None).await;
    let after: serde_json::Value = serde_json::from_slice(&after).unwrap();
    assert_eq!(after["video_bytes_sent"], 500);

    // The rest of the allowance goes in one more request, which is allowed because the client was
    // under it when the request began.
    let (status, _, _) = get(&rig.router, &ready.url, Some("bytes=500-1499")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);

    // Now the day's allowance is used up: the video and a new play are both refused, and the answer
    // names the desktop app.
    let (status, _, body) = get(&rig.router, &ready.url, Some("bytes=0-99")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(contains(&body, b"akidev.live/download"));

    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/play")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"id":"movie-tt0111161"}"#))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("192.0.2.1:4000".parse::<SocketAddr>().unwrap()));
    let response = rig.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn without_an_allowance_video_is_never_refused() {
    let Some(rig) = rig("no-allowance", |_| {}).await else {
        return;
    };
    let mut args = H264.to_vec();
    args.extend(["-movflags", "+faststart"]);
    let path = clip(&rig.dir, "a.mp4", &args, &AAC);
    let ready = deliver(&rig, &path, false).await.unwrap();
    for _ in 0..3 {
        let (status, _, _) = get(&rig.router, &ready.url, None).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn health_says_what_is_running() {
    let Some(rig) = rig("health", |_| {}).await else {
        return;
    };
    let (status, _, body) = get(&rig.router, "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["ffmpeg"], true);
    assert_eq!(json["debrid"]["configured"], false);
    assert_eq!(json["sources"][0], "torrent");
}

#[tokio::test]
async fn the_options_preflight_names_the_site() {
    let Some(rig) = rig("cors", |_| {}).await else {
        return;
    };
    let mut request = Request::builder()
        .method("OPTIONS")
        .uri("/v1/play")
        .header(header::ORIGIN, "https://akidev.live")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("192.0.2.1:4000".parse::<SocketAddr>().unwrap()));
    let response = rig.router.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://akidev.live"
    );

    let mut request = Request::builder()
        .uri("/health")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("192.0.2.1:4000".parse::<SocketAddr>().unwrap()));
    let response = rig.router.clone().oneshot(request).await.unwrap();
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[tokio::test]
async fn the_conversion_waits_when_the_viewer_is_far_behind_and_catches_up_on_a_seek() {
    let Some(rig) = rig("throttle", |_| {}).await else {
        return;
    };
    rig.hls.set_ahead(0);
    // Noise does not compress, so each segment is a megabyte or so and the feed is what limits
    // how far the conversion gets, not what ffmpeg had buffered.
    let path = rig.dir.join("noise.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=640x360:r=24,geq=random(1)*255:128:128",
        ])
        .args([
            "-t",
            "30",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "48",
            "-crf",
            "10",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());

    let ready = deliver(&rig, &path, false).await.unwrap();
    let base = ready.url.trim_end_matches("index.m3u8").to_string();
    // Give it time to run as far as it is allowed to.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let (_, _, playlist) = get(&rig.router, &ready.url, None).await;
    let listed = crate::hls::Playlist::parse(&String::from_utf8(playlist).unwrap());
    assert!(!listed.finished, "it should be holding back, not finished");
    assert!(
        listed.segments.len() < 4,
        "ran on to {} segments",
        listed.segments.len()
    );

    // Asking for a segment well past what exists lifts the limit, so it is made.
    let (status, _, _) = get(&rig.router, &format!("{base}seg00004.m4s"), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn an_answer_does_not_depend_on_who_asked_so_the_edge_may_keep_it() {
    // The edge ignores `Vary: Origin`, so a copy made for a request without an Origin header would
    // be handed to the page. With one allowed origin every answer names it, and nothing varies.
    let Some(rig) = rig("edge", |s| {
        s.cors_origins = vec!["https://akidev.live".into()]
    })
    .await
    else {
        return;
    };
    let (_, headers, _) = get(&rig.router, "/health", None).await;
    let without_origin = {
        let mut request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo("192.0.2.1:4000".parse::<SocketAddr>().unwrap()));
        rig.router.clone().oneshot(request).await.unwrap()
    };
    assert_eq!(
        without_origin.headers()["access-control-allow-origin"],
        "https://akidev.live"
    );
    assert_eq!(
        headers["access-control-allow-origin"],
        "https://akidev.live"
    );
    for response_headers in [&headers, without_origin.headers()] {
        let vary = response_headers
            .get_all(header::VARY)
            .iter()
            .map(|v| v.to_str().unwrap().to_lowercase())
            .collect::<Vec<_>>()
            .join(",");
        assert!(!vary.contains("origin"), "vary: {vary}");
    }
}

#[tokio::test]
async fn a_picture_from_an_unlisted_host_is_refused() {
    let Some(rig) = rig("img-refused", |_| {}).await else {
        return;
    };
    for url in [
        "/v1/img?u=https%3A%2F%2Fevil.example%2Fa.jpg&w=300",
        "/v1/img?u=http%3A%2F%2F169.254.169.254%2Flatest&w=300",
        "/v1/img?u=file%3A%2F%2F%2Fetc%2Fpasswd",
    ] {
        let (status, _, body) = get(&rig.router, url, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert!(String::from_utf8_lossy(&body).contains("error"));
    }
}

#[tokio::test]
async fn the_release_and_subtitle_routes_refuse_what_is_not_an_episode_or_a_film() {
    let Some(rig) = rig("lists", |_| {}).await else {
        return;
    };
    for url in [
        "/v1/releases?id=series-tt0903747",
        "/v1/releases?id=nonsense",
        "/v1/subtitles?id=anime-16498",
        "/v1/subtitles?id=../../etc",
    ] {
        let (status, _, body) = get(&rig.router, url, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert!(String::from_utf8_lossy(&body).contains("error"), "{url}");
    }
    // A token the server never listed is not a file, and its shape is checked before it is used.
    let (status, _, _) = get(
        &rig.router,
        &format!("/v1/subtitles/file/{}.vtt", "0".repeat(32)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(&rig.router, "/v1/subtitles/file/..%2F..%2Fetc.vtt", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_release_to_play_must_be_an_info_hash() {
    let Some(rig) = rig("release", |s| s.play_limit = 50).await else {
        return;
    };
    let post = |body: String| {
        let router = rig.router.clone();
        async move {
            let mut request = Request::builder()
                .method("POST")
                .uri("/v1/play")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo("192.0.2.9:1".parse::<SocketAddr>().unwrap()));
            router.oneshot(request).await.unwrap().status()
        }
    };
    for bad in [
        "nothex",
        "abc",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        "../../../etc/passwd",
    ] {
        assert_eq!(
            post(format!(r#"{{"id":"movie-tt0111161","release":"{bad}"}}"#)).await,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
}
