//! The Akidev server: the catalogue and the streaming, over REST, on `akidev-core`.
//!
//! The desktop app is built on the same library, so there is one copy of every indexer, provider
//! and ranking rule, and a fix lands in both.

mod api;
mod config;
mod deliver;
mod dto;
mod hls;
mod img;
mod limits;
mod media;
mod origin;
mod play;
mod sources;
mod subs;

#[cfg(test)]
mod e2e;

use akidev_core::stream::Streamer;
use akidev_core::torrent::Engine;
use anyhow::Context;
use config::Settings;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// A torrent nobody has read from for this long is dropped, with its files.
const TORRENT_IDLE: Duration = Duration::from_secs(20 * 60);

fn main() -> anyhow::Result<()> {
    // `akidev-server --check` asks the running server for /health and exits 0 when it answers. A
    // container's health check runs it, so the image needs no curl.
    if std::env::args().nth(1).as_deref() == Some("--check") {
        std::process::exit(health_check());
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

fn health_check() -> i32 {
    use std::io::{Read, Write};
    let port = Settings::from_env().port;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(3))
    else {
        return 1;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    if stream
        .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return 1;
    }
    let mut head = [0u8; 12];
    match stream.read_exact(&mut head) {
        Ok(()) if &head[8..12] == b" 200" => 0,
        _ => 1,
    }
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "akidev_server=info,akidev_core=info".into()),
        )
        .init();

    let settings = Arc::new(Settings::from_env());
    std::fs::create_dir_all(&settings.cache_dir)
        .with_context(|| format!("creating {}", settings.cache_dir.display()))?;

    // The signed definitions of the indexers and providers: the last good copy first, then kept
    // fresh in the background.
    akidev_core::sources::load_saved(&settings.sources_dir());
    tokio::spawn(akidev_core::sources::keep_fresh(settings.sources_dir()));
    akidev_core::providers::keep_answers_in(settings.catalogue_dir());

    let ffmpeg = media::ffmpeg_available().await;
    if !ffmpeg {
        tracing::warn!("ffmpeg or ffprobe is missing: only MP4 files can be played");
    }

    let streamer =
        Arc::new(Streamer::engine_only(settings.core(), settings.cache_dir.clone()).await?);
    let engine = streamer.engine();
    let hls = hls::Hls::new(
        settings.clone(),
        vec![Engine::downloads_dir(&settings.cache_dir)],
    );
    let registry = Arc::new(deliver::Registry::default());
    let images = img::Images::new(settings.images_dir(), settings.image_hosts.clone());
    let subs = Arc::new(subs::Subs::new(settings.subtitles_dir()));

    let wants_debrid = settings
        .stream_sources
        .iter()
        .any(|source| source == "debrid");
    let debrid = if wants_debrid {
        let debrid = sources::debrid::Debrid::new(
            settings.torbox_token.clone(),
            settings.debrid_timeout,
            settings.debrid_cdn_hosts.clone(),
        );
        if debrid.is_none() {
            tracing::warn!(
                "STREAM_SOURCES lists debrid but TORBOX_API_KEY is not set: it is skipped"
            );
        }
        debrid
    } else {
        None
    };

    let player = play::Player::new(
        settings.clone(),
        Some(streamer.clone()),
        debrid,
        hls.clone(),
        registry.clone(),
    );
    let state = api::AppState {
        settings: settings.clone(),
        player,
        hls: hls.clone(),
        registry,
        limiter: Arc::new(limits::Limiter::default()),
        engine: Some(engine.clone()),
        images: images.clone(),
        subs: subs.clone(),
        ffmpeg,
    };

    {
        // Resized pictures are kept between runs, so a deploy does not send every visitor back to
        // the providers. The oldest go when they outgrow their share of the disk.
        let images = images.clone();
        let limit = settings.image_cache_bytes;
        tokio::spawn(async move {
            loop {
                images.prune(limit).await;
                tokio::time::sleep(Duration::from_secs(600)).await;
            }
        });
    }

    {
        // Converted subtitles are small, and kept in the same way.
        let subs = subs.clone();
        tokio::spawn(async move {
            loop {
                subs.prune(64 * 1024 * 1024).await;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
    }

    {
        let engine = engine.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let dropped = engine.sweep_idle(TORRENT_IDLE).await;
                if dropped > 0 {
                    tracing::info!("[torrent] dropped {dropped} idle torrent(s)");
                }
            }
        });
    }

    let address = SocketAddr::from(([0, 0, 0, 0], settings.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("binding {address}"))?;
    tracing::info!(
        "akidev-server {} listening on {} (sources: {})",
        env!("CARGO_PKG_VERSION"),
        settings.public_url,
        settings.stream_sources.join(", ")
    );

    // A stop signal lets requests finish, but a video response can last for hours, so open
    // connections are given a few seconds and then let go.
    let stop = Arc::new(tokio::sync::Notify::new());
    let stopped = stop.clone();
    tokio::spawn(async move {
        shutdown().await;
        stopped.notify_waiters();
    });
    let graceful = {
        let stop = stop.clone();
        async move { stop.notified().await }
    };
    let server = axum::serve(
        listener,
        api::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(graceful);
    tokio::select! {
        served = server => served?,
        _ = async {
            stop.notified().await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        } => tracing::info!("closing connections that are still open"),
    }

    // Conversions are killed and their files removed, so a restart starts clean.
    hls.shutdown();
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
