// A release build on Windows is a window, not a console program.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod analytics;
mod app;
mod audio;
mod backend;
mod browse;
mod captions;
mod choices;
mod design;
mod episodes;
mod genre_assets;
mod hero;
mod history;
mod home;
mod images;
mod music_album;
mod music_artist;
mod music_home;
mod music_player;
mod music_ui;
mod palette;
mod panels;
mod player;
mod prefetch;
mod progress;
mod route;
mod row;
mod runtime;
mod settings;
mod task;
mod theme;
mod tile;
mod title;
mod updater;
mod updates;
mod watch;

use app::AkidevApp;
use gpui_kit::*;
use route::Route;
use std::sync::Arc;

fn main() {
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn,akidev=info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .with(analytics::ErrorLogs)
            .init();
    }
    let dirs = directories::ProjectDirs::from("live", "akidev", "Akidev")
        .expect("a home directory exists");
    let image_cache = dirs.cache_dir().join("images");
    let config_path = dirs.config_dir().join("config.toml");
    let progress_path = dirs.config_dir().join("progress.json");
    let subtitle_language_path = dirs.config_dir().join("subtitles.json");
    let subtitles_dir = dirs.cache_dir().join("subtitles");
    let torrent_cache = dirs.cache_dir().join("torrents");
    let sources_dir = dirs.cache_dir().join("sources");
    let ytdlp_dir = dirs.cache_dir().join("yt-dlp");
    akidev_core::providers::keep_answers_in(dirs.cache_dir().join("catalog"));
    let first = Route::from_args(std::env::args().skip(1));
    // The definitions saved by an earlier run are used at once. What the home screen needs is
    // asked for now, while the window is still being made.
    akidev_core::sources::load_saved(&sources_dir);
    if first == Route::Home {
        home::prefetch();
    }

    application()
        .with_assets(genre_assets::AppAssets)
        .run(move |cx| {
            init(cx);
            cx.set_global(backend::Backend::start(
                config_path,
                progress_path,
                subtitle_language_path,
                subtitles_dir,
                torrent_cache,
                sources_dir,
            ));
            theme::apply(cx);
            start_music(&ytdlp_dir, cx);
            cx.set_http_client(Arc::new(images::ImageClient::new(image_cache)));
            cx.bind_keys([KeyBinding::new("secondary-k", app::OpenSearch, None)]);
            quit_with_the_window(cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Akidev".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(18.), px(20.))),
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1360.), px(860.)),
                    cx,
                ))),
                window_min_size: Some(size(px(960.), px(620.))),
                // The bar draws itself and moves the window itself, so AppKit must not also treat
                // it as a drag region.
                app_owns_titlebar_drag: true,
                ..Default::default()
            };
            open_window(options, cx, |window, cx| {
                cx.new(|cx| AkidevApp::new(first, window, cx))
            })
            .expect("window opens");
        });
}

/// The music catalogue and its player. yt-dlp, which finds the audio, is fetched the first time a
/// track is played and kept up to date from then on.
fn start_music(ytdlp_dir: &std::path::Path, cx: &mut App) {
    let program = akidev_core::music::ytdlp_program(ytdlp_dir);
    let music = Arc::new(
        akidev_core::music::Music::new(program.to_string_lossy().into_owned()).with_system_certs(),
    );
    runtime::block_on(async { music.keep_updated() });
    let player = cx.new(music_player::MusicPlayer::new);
    cx.set_global(music_player::MusicHub {
        music,
        ytdlp_dir: ytdlp_dir.to_path_buf(),
        player,
    });
}

actions!(akidev, [Quit]);

/// Closing the window ends the app, and so does Cmd-Q (Ctrl-Q elsewhere). On the way out the downloads are deleted.
fn quit_with_the_window(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus([Menu {
        name: "Akidev".into(),
        items: vec![MenuItem::action("Quit Akidev", Quit)],
        disabled: false,
    }]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    // Quitting gives async work only a moment, so the downloads are deleted directly. The
    // sockets close with the process.
    cx.on_app_quit(|cx| {
        let dir = cx.global::<backend::Backend>().downloads_dir.clone();
        async move {
            let _ = std::fs::remove_dir_all(dir);
            runtime::run(analytics::finish()).await;
        }
    })
    .detach();
}
