//! The watch page: the video fills the window, and its controls come and go.

use crate::backend::Backend;
use crate::captions::{self, Group, Start};
use crate::design::button::{control_button, icon_button, secondary_button};
use crate::design::state::{error_state, skeleton};
use crate::design::tokens::{
    ACCENT, CANVAS, GUTTER, LINE, SURFACE, TEXT, TEXT_DIM, TOP_BAR, color,
};
use crate::episodes::EpisodeList;
use crate::panels::{
    self, SourceAction, SourcesView, SubtitleAction, SubtitleMenu, back_to_sources,
};
use crate::player::{MAX_WIDTH, Player, PlayerState, SubtitleTrack};
use crate::progress::Entry;
use crate::route::{Route, replace};
use crate::runtime;
use akidev_core::media_id::{self, MediaId, anime_episode_id};
use akidev_core::model::{Detail, Kind};
use akidev_core::providers;
use akidev_core::stream::{Choice, Resolution, StreamId};
use akidev_core::subtitles::{self, Subtitle};
use akidev_core::torrent::StreamStats;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{InteractiveElementExt as _, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::AbortHandle;

/// Controls hide after this long without the mouse or keyboard.
const HIDE_AFTER: Duration = Duration::from_secs(3);
/// How often the watch position is written to disk.
const SAVE_EVERY: Duration = Duration::from_secs(10);
/// How long a seek or volume message stays up.
const MESSAGE_FOR: Duration = Duration::from_millis(1400);
/// "Next episode" is offered this many seconds before the end.
const NEXT_OFFER: f64 = 40.;
/// How long to wait for the subtitle sites. Playback never waits for them.
const SUBTITLES_WAIT: Duration = Duration::from_secs(8);

/// The side panels. One is open at a time.
#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Episodes,
    Sources,
    Subtitles,
}

/// The releases the source picker lists. The first call takes seconds, so it has a state.
enum Sources {
    Loading,
    Ready(Vec<Choice>),
    Failed(String),
}

/// What the page tells analytics: how long it took to start, and how long it played.
struct Stats {
    id: String,
    resolving_since: Option<Instant>,
    playing_since: Option<Instant>,
    watched: Duration,
}

impl Stats {
    fn new(id: String) -> Stats {
        Stats {
            id,
            resolving_since: None,
            playing_since: None,
            watched: Duration::ZERO,
        }
    }

    /// Closes the stretch that was playing, if one was.
    fn stop(&mut self) {
        if let Some(since) = self.playing_since.take() {
            self.watched += since.elapsed();
        }
    }
}

impl Drop for Stats {
    fn drop(&mut self) {
        self.stop();
        if self.watched >= Duration::from_secs(5) {
            crate::analytics::capture(
                "playback_ended",
                serde_json::json!({ "title_id": self.id, "watched_seconds": self.watched.as_secs() }),
            );
        }
    }
}

enum Phase {
    Resolving,
    Unavailable(String),
    Playing,
}

/// Stops a background task when the page that wanted its answer goes away.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The picture. It is its own view so a new frame redraws only the picture, not the page.
struct VideoView {
    frame: Option<Arc<RenderImage>>,
    _frames: Task<()>,
}

impl VideoView {
    fn new(
        player: Rc<Player>,
        mut frames: mpsc::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> VideoView {
        let task = cx.spawn(async move |view, cx| {
            while frames.next().await.is_some() {
                let taken = view.update(cx, |view, cx| {
                    let Some(new) = player.take_frame() else {
                        return;
                    };
                    if let Some(old) = view.frame.replace(new) {
                        cx.drop_image(old, None);
                    }
                    cx.notify();
                });
                if taken.is_err() {
                    break;
                }
            }
        });
        // The last frame stays in the GPU atlas until it is dropped explicitly.
        cx.on_release(|view, cx| {
            if let Some(frame) = view.frame.take() {
                cx.drop_image(frame, None);
            }
        })
        .detach();
        VideoView {
            frame: None,
            _frames: task,
        }
    }
}

impl Render for VideoView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x000000)).children(
            self.frame
                .clone()
                .map(|image| img(image).size_full().object_fit(ObjectFit::Contain)),
        )
    }
}

pub struct WatchPage {
    id: String,
    parsed: Option<MediaId>,
    detail: Option<Detail>,
    episodes: Option<Entity<EpisodeList>>,
    panel: Option<Panel>,
    /// Asked for the first time the picker opens.
    sources: Option<Sources>,
    /// The subtitles found online, by language. Empty until (and unless) the sites answer.
    online: Vec<Group>,
    /// The sites have answered (or failed to), so "nothing in that language" is known.
    online_ready: bool,
    /// The viewer's language has been looked for in this video, so it is done only once.
    subtitles_started: bool,
    /// The online subtitle on screen, by id.
    online_in_use: Option<String>,
    /// The language in the subtitle panel whose options are showing.
    language_open: Option<String>,
    /// Where to carry on after the viewer switches release: the same moment, even when it is
    /// too early in the video for progress to offer to resume it.
    resume_from: Option<f64>,
    phase: Phase,
    player: Option<Rc<Player>>,
    video: Option<Entity<VideoView>>,
    state: PlayerState,
    release: Option<String>,
    /// Which torrent file is playing, to ask how its download is going.
    stream: Option<StreamId>,
    transfer: Option<StreamStats>,
    focus: FocusHandle,
    /// Keys reach the page only once it holds focus, which it takes on its first frame.
    took_focus: bool,
    /// Where the seek bar was last drawn, so a click can be turned into a position.
    bar: Rc<Cell<Bounds<Pixels>>>,
    /// While the seek bar is held: where the viewer has dragged to, 0 to 1.
    scrubbing: Option<f32>,
    controls_visible: bool,
    last_activity: Instant,
    /// A brief message over the picture ("+10s", "Volume 55%").
    message: Option<(String, Instant)>,
    last_saved: Instant,
    /// Set once the end has been dealt with, so the next episode is started only once and
    /// the finished episode is not saved again over it.
    finished: bool,
    resolving: Option<AbortOnDrop>,
    stats: Stats,
    _tick: Task<()>,
}

impl WatchPage {
    pub fn new(id: String, cx: &mut Context<Self>) -> WatchPage {
        let parsed = media_id::parse(&id);

        if let Some(parsed) = &parsed {
            let key = parsed.key();
            let current = id.clone();
            crate::task::load(
                cx,
                async move { providers::detail(&key).await },
                move |page, detail, cx| {
                    if let Some(detail) = detail
                        .as_ref()
                        .filter(|detail| detail.media.kind != Kind::Movie)
                    {
                        page.episodes =
                            Some(cx.new(|_| EpisodeList::new(detail, Some(current), true)));
                    }
                    if detail.is_none() {
                        crate::route::toast("Couldn't load this title's details.", cx);
                    }
                    page.find_subtitles(detail.as_ref(), cx);
                    page.detail = detail;
                    cx.notify();
                },
            );
        }

        // Playback state is polled, not pushed: a few times a second is plenty for a clock
        // and a seek bar, and it keeps the player thread free of the UI.
        let tick = cx.spawn(async move |page, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if page.update(cx, |page, cx| page.tick(cx)).is_err() {
                    break;
                }
            }
        });

        cx.on_release(|page, cx| page.save_progress(cx)).detach();

        let mut page = WatchPage {
            stats: Stats::new(id.clone()),
            id,
            parsed,
            detail: None,
            episodes: None,
            panel: None,
            sources: None,
            online: Vec::new(),
            online_ready: false,
            subtitles_started: false,
            online_in_use: None,
            language_open: None,
            resume_from: None,
            phase: Phase::Resolving,
            player: None,
            video: None,
            state: PlayerState::default(),
            release: None,
            stream: None,
            transfer: None,
            focus: cx.focus_handle(),
            took_focus: false,
            bar: Rc::new(Cell::new(Bounds::default())),
            scrubbing: None,
            controls_visible: true,
            last_activity: Instant::now(),
            message: None,
            last_saved: Instant::now(),
            finished: false,
            resolving: None,
            _tick: tick,
        };
        page.start(false, cx);
        page
    }

    /// Looks for a source and starts playing it. `fresh` forgets a torrent that already
    /// failed, for a retry.
    fn start(&mut self, fresh: bool, cx: &mut Context<Self>) {
        self.begin(None, fresh, cx);
    }

    /// Plays the release with this info hash, from where the viewer is.
    fn play_release(&mut self, info_hash: String, cx: &mut Context<Self>) {
        self.panel = None;
        self.begin(Some(info_hash), false, cx);
    }

    /// Opens a release and plays it: the one asked for, or else the best that works.
    fn begin(&mut self, release: Option<String>, fresh: bool, cx: &mut Context<Self>) {
        self.save_progress(cx);
        if release.is_some() && self.state.position > 0. {
            self.resume_from = Some(self.state.position);
        }
        self.stats.stop();
        self.stats.resolving_since = Some(Instant::now());
        self.online_in_use = None;
        self.subtitles_started = false;
        self.player = None;
        self.video = None;
        self.stream = None;
        self.transfer = None;
        self.state = PlayerState::default();
        self.phase = Phase::Resolving;
        self.controls_visible = true;
        cx.notify();

        let streamer = match &cx.global::<Backend>().streamer {
            Ok(streamer) => streamer.clone(),
            Err(error) => {
                self.phase = Phase::Unavailable(format!("Streaming could not start: {error}"));
                return;
            }
        };
        let id = self.id.clone();
        let handle = runtime::spawn(async move {
            match release {
                Some(info_hash) => streamer.resolve_release(&id, &info_hash).await,
                None => streamer.resolve(&id, fresh).await,
            }
        });
        self.resolving = Some(AbortOnDrop(handle.abort_handle()));
        cx.spawn(async move |page, cx| {
            // An aborted task means the page was left, so there is nobody to tell.
            let Ok(resolution) = handle.await else { return };
            let _ = page.update(cx, |page, cx| page.resolved(resolution, cx));
        })
        .detach();
    }

    fn resolved(&mut self, resolution: Resolution, cx: &mut Context<Self>) {
        let (url, release, stream) = match resolution {
            Resolution::Ready {
                url,
                release,
                stream,
            } => (url, release, stream),
            Resolution::Unavailable { reason } => {
                crate::analytics::capture(
                    "playback_failed",
                    serde_json::json!({ "stage": "no_source", "title_id": self.id }),
                );
                self.phase = Phase::Unavailable(reason);
                cx.notify();
                return;
            }
        };
        let backend = cx.global::<Backend>();
        let resume = self
            .resume_from
            .take()
            .or_else(|| backend.progress.resume_at(&self.id));
        let (volume, muted) = (backend.volume, backend.muted);
        let (sender, receiver) = mpsc::channel::<()>(1);
        match Player::start(&url, resume, volume, muted, sender) {
            Ok(player) => {
                let player = Rc::new(player);
                self.video = Some(cx.new(|cx| VideoView::new(player.clone(), receiver, cx)));
                self.player = Some(player);
                self.release = Some(release);
                self.stream = Some(stream);
                self.phase = Phase::Playing;
                self.stats.playing_since = Some(Instant::now());
                crate::analytics::capture(
                    "playback_started",
                    serde_json::json!({
                        "title_id": self.id,
                        "start_ms": self.stats.resolving_since.map(|since| since.elapsed().as_millis() as u64),
                        "resumed": resume.is_some(),
                    }),
                );
            }
            Err(error) => {
                crate::analytics::capture(
                    "playback_failed",
                    serde_json::json!({ "stage": "player", "title_id": self.id }),
                );
                crate::analytics::error("player_start", &error);
                self.phase = Phase::Unavailable(error);
            }
        }
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let state = player.state();

        if let Some(error) = &state.error {
            self.phase = Phase::Unavailable(format!("The video could not be played: {error}"));
            self.player = None;
            self.video = None;
            self.controls_visible = true;
            cx.notify();
            return;
        }

        let mut changed = state != self.state;
        self.state = state;

        // What the next episode and the next visit start at.
        let backend = cx.global_mut::<Backend>();
        (backend.volume, backend.muted) = (self.state.volume, self.state.muted);

        let transfer = match (&self.stream, &cx.global::<Backend>().streamer) {
            (Some(stream), Ok(streamer)) => streamer.stats(stream),
            _ => None,
        };
        if transfer != self.transfer {
            self.transfer = transfer;
            changed = true;
        }

        let idle = self.last_activity.elapsed() > HIDE_AFTER;
        let hide = idle
            && !self.state.paused
            && !self.state.buffering
            && self.scrubbing.is_none()
            && self.panel.is_none();
        if hide && self.controls_visible {
            self.controls_visible = false;
            changed = true;
        }
        if self
            .message
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() > MESSAGE_FOR)
        {
            self.message = None;
            changed = true;
        }
        self.start_subtitles(cx);
        if self.last_saved.elapsed() > SAVE_EVERY && !self.state.paused {
            self.save_progress(cx);
        }
        // An end well short of the duration is a cut-off stream, not a finished episode.
        let watched = self.state.duration > 0. && self.state.position >= self.state.duration * 0.9;
        if self.state.ended && !self.finished && watched {
            self.finish(cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Remembers where the viewer is, so Home can offer to carry on.
    fn save_progress(&mut self, cx: &mut App) {
        if self.finished {
            return;
        }
        let (Some(detail), true) = (
            &self.detail,
            self.state.duration > 0. && self.state.position > 0.,
        ) else {
            return;
        };
        let entry = Entry::new(
            detail.media.clone(),
            self.id.clone(),
            self.state.position,
            self.state.duration,
        );
        cx.global_mut::<Backend>().progress.record(entry);
        self.last_saved = Instant::now();
    }

    /// The end of the video: the next episode is remembered as where to carry on, and starts.
    /// A film or a last episode is simply forgotten.
    fn finish(&mut self, cx: &mut Context<Self>) {
        self.finished = true;
        let Some(detail) = &self.detail else { return };
        let next = self.neighbour(1);
        let entry = match &next {
            Some(next) => Entry::new(detail.media.clone(), next.clone(), 0., 0.),
            None => Entry::new(
                detail.media.clone(),
                self.id.clone(),
                self.state.duration,
                self.state.duration,
            ),
        };
        cx.global_mut::<Backend>().progress.record(entry);
        if let Some(next) = next {
            replace(Route::Watch { id: next }, cx);
        }
    }

    /// Any input brings the controls back.
    fn activity(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        if !self.controls_visible {
            self.controls_visible = true;
            cx.notify();
        }
    }

    fn say(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.message = Some((text.into(), Instant::now()));
        cx.notify();
    }

    /// The episode before (`-1`) or after (`1`) this one, if there is one.
    fn neighbour(&self, delta: i32) -> Option<String> {
        let parsed = self.parsed.as_ref()?;
        let detail = self.detail.as_ref()?;
        match parsed.kind {
            Kind::Movie => None,
            Kind::Anime => {
                let next = i64::from(parsed.episode?) + i64::from(delta);
                let count = i64::from(detail.media.episodes.unwrap_or(0));
                (next >= 1 && next <= count)
                    .then(|| anime_episode_id(&parsed.provider_id, next as u32))
            }
            Kind::Series => {
                let mut episodes: Vec<_> = detail.episodes.iter().collect();
                episodes.sort_by_key(|e| (e.season, e.number));
                let at = episodes.iter().position(|e| e.id == self.id)?;
                let next = at as i64 + i64::from(delta);
                usize::try_from(next)
                    .ok()
                    .and_then(|index| episodes.get(index))
                    .map(|e| e.id.clone())
            }
        }
    }

    /// The seek bar position under `x`, 0 to 1.
    fn bar_fraction(&self, x: Pixels) -> f32 {
        let bar = self.bar.get();
        if bar.size.width <= px(0.) {
            return 0.;
        }
        ((x - bar.origin.x) / bar.size.width).clamp(0., 1.)
    }

    fn finish_scrub(&mut self, cx: &mut Context<Self>) {
        let (Some(fraction), Some(player)) = (self.scrubbing.take(), &self.player) else {
            return;
        };
        if self.state.duration > 0. {
            player.seek_to(f64::from(fraction) * self.state.duration);
        }
        cx.notify();
    }

    fn with_player(&mut self, cx: &mut Context<Self>, action: impl FnOnce(&Player)) {
        if let Some(player) = &self.player {
            action(player);
        }
        self.activity(cx);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.activity(cx);
        // Shortcuts with Cmd or Ctrl belong to the system (Cmd-C is not "subtitles").
        if event.keystroke.modifiers.modified() {
            return;
        }
        let key = event.keystroke.key.as_str();
        // Holding a toggle must not flicker it; seeking and volume are meant to repeat.
        let repeats = matches!(key, "left" | "right" | "up" | "down");
        if event.is_held && !repeats {
            return;
        }
        match key {
            "space" | "k" => self.with_player(cx, Player::toggle_pause),
            "left" => {
                self.with_player(cx, |player| player.seek_by(-10.));
                self.say("−10s", cx);
            }
            "right" => {
                self.with_player(cx, |player| player.seek_by(10.));
                self.say("+10s", cx);
            }
            "up" => {
                self.with_player(cx, |player| player.change_volume(5.));
                self.say(
                    format!("Volume {}%", (self.state.volume + 5.).min(100.) as u32),
                    cx,
                );
            }
            "down" => {
                self.with_player(cx, |player| player.change_volume(-5.));
                self.say(
                    format!("Volume {}%", (self.state.volume - 5.).max(0.) as u32),
                    cx,
                );
            }
            "m" => self.with_player(cx, Player::toggle_mute),
            "c" => self.with_player(cx, Player::cycle_subtitles),
            "a" => self.with_player(cx, Player::cycle_audio),
            "e" if self.episodes.is_some() => self.toggle_panel(Panel::Episodes, cx),
            "n" => {
                if let Some(next) = self.neighbour(1) {
                    replace(Route::Watch { id: next }, cx);
                }
            }
            "f" => window.toggle_fullscreen(),
            "escape" if self.panel.is_some() => {
                self.panel = None;
                cx.notify();
            }
            "escape" if window.is_fullscreen() => window.toggle_fullscreen(),
            _ => {}
        }
    }

    /// Opens a panel, or closes it if it is the one showing. Opening one closes the other.
    fn toggle_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.panel = (self.panel != Some(panel)).then_some(panel);
        if self.panel == Some(Panel::Sources) && self.sources.is_none() {
            self.load_sources(cx);
        }
        self.activity(cx);
        cx.notify();
    }

    /// Asks for every release of this episode or film. It runs in the background: the first
    /// call searches the indexers and takes several seconds.
    fn load_sources(&mut self, cx: &mut Context<Self>) {
        let streamer = match &cx.global::<Backend>().streamer {
            Ok(streamer) => streamer.clone(),
            Err(error) => {
                self.sources = Some(Sources::Failed(format!(
                    "Streaming could not start: {error}"
                )));
                return;
            }
        };
        self.sources = Some(Sources::Loading);
        let id = self.id.clone();
        crate::task::load(
            cx,
            async move { streamer.plan(&id).await.map(|plan| plan.choices()) },
            |page, plan, cx| {
                page.sources = Some(match plan {
                    Ok(choices) => {
                        crate::analytics::capture(
                            "sources_loaded",
                            serde_json::json!({ "title_id": page.id, "count": choices.len() }),
                        );
                        Sources::Ready(choices)
                    }
                    Err(reason) => {
                        crate::analytics::capture(
                            "sources_failed",
                            serde_json::json!({ "title_id": page.id }),
                        );
                        Sources::Failed(reason)
                    }
                });
                cx.notify();
            },
        );
    }

    /// Looks for subtitles on the sites. Playback does not wait for this, and when it fails
    /// or finds nothing the menu simply has no online section.
    fn find_subtitles(&mut self, detail: Option<&Detail>, cx: &mut Context<Self>) {
        let (Some(detail), Some(parsed)) = (detail, &self.parsed) else {
            return;
        };
        let query = subtitles::query_for(detail, parsed);
        crate::task::load(
            cx,
            async move { subtitles::find(&query, SUBTITLES_WAIT).await },
            |page, found, cx| {
                page.online = captions::group(found);
                page.online_ready = true;
                cx.notify();
            },
        );
    }

    /// Once the picture is up, starts the subtitles in the language the viewer chose last.
    ///
    /// A track inside the file in that language is used. An online subtitle is used only when
    /// the file has none in it: a track made for this very release is a better fit than one
    /// from the sites, and needs no download. When neither exists, mpv's own choice stands.
    fn start_subtitles(&mut self, cx: &mut Context<Self>) {
        if self.subtitles_started || !self.state.video_ready {
            return;
        }
        let Some(language) = cx
            .global::<Backend>()
            .subtitle_language
            .language()
            .map(str::to_string)
        else {
            self.subtitles_started = true;
            return;
        };
        let embedded: Vec<&SubtitleTrack> = self
            .state
            .subtitle_list
            .iter()
            .filter(|track| !track.external)
            .collect();
        let start = captions::start_with(&language, &embedded, &self.online);
        match start {
            Some(Start::Embedded(id)) => {
                let name = self
                    .state
                    .subtitle_list
                    .iter()
                    .find(|track| track.id == id)
                    .map(captions::track_name)
                    .unwrap_or_default();
                self.subtitles_started = true;
                self.use_embedded(id, &name, cx);
            }
            Some(Start::Online(subtitle)) => {
                let subtitle = subtitle.clone();
                self.subtitles_started = true;
                self.use_online(subtitle, false, cx);
            }
            // The sites may still answer.
            None if !self.online_ready => {}
            None => self.subtitles_started = true,
        }
    }

    fn use_embedded(&mut self, id: i64, name: &str, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            player.select_subtitle(Some(id));
        }
        self.online_in_use = None;
        self.say(format!("Subtitles: {name}"), cx);
    }

    /// Downloads an online subtitle and has mpv show it. `remember` is for the viewer's own
    /// choice; the one picked for them at the start does not change what they chose.
    fn use_online(&mut self, subtitle: Subtitle, remember: bool, cx: &mut Context<Self>) {
        let dir = cx.global::<Backend>().subtitles_dir.clone();
        self.online_in_use = Some(subtitle.id.clone());
        if remember {
            crate::analytics::capture(
                "subtitle_chosen",
                serde_json::json!({ "language": subtitle.lang, "provider": subtitle.provider }),
            );
        }
        crate::task::load(
            cx,
            captions::download(dir, subtitle.url.clone()),
            move |page, file, cx| match file {
                Ok(path) => page.show_subtitle_file(&path, &subtitle, remember, cx),
                Err(error) => {
                    tracing::warn!("[subtitles] {}: {error}", subtitle.id);
                    if page.online_in_use.as_deref() == Some(subtitle.id.as_str()) {
                        page.online_in_use = None;
                    }
                    page.say("Couldn't load that subtitle", cx);
                }
            },
        );
    }

    fn show_subtitle_file(
        &mut self,
        path: &Path,
        subtitle: &Subtitle,
        remember: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(player) = self.player.clone() else {
            return;
        };
        // One downloaded track at a time: the last one stays loaded in mpv otherwise, and
        // cycling with the key would walk through all of them.
        for track in self
            .state
            .subtitle_list
            .iter()
            .filter(|track| track.external)
        {
            player.remove_subtitle(track.id);
        }
        player.add_subtitle(&path.to_string_lossy(), &subtitle.language, &subtitle.lang);
        if remember {
            cx.global_mut::<Backend>()
                .subtitle_language
                .set(Some(subtitle.lang.clone()));
        }
        self.say(format!("Subtitles: {}", subtitle.language), cx);
    }

    fn subtitle_action(&mut self, action: SubtitleAction, cx: &mut Context<Self>) {
        match action {
            SubtitleAction::Language(code) => {
                self.language_open =
                    (self.language_open.as_deref() != Some(code.as_str())).then_some(code);
            }
            SubtitleAction::Off => {
                if let Some(player) = &self.player {
                    player.select_subtitle(None);
                }
                self.online_in_use = None;
                // Off is a choice too: the next video does not start with subtitles.
                cx.global_mut::<Backend>().subtitle_language.set(None);
                self.panel = None;
                self.say("Subtitles: Off", cx);
            }
            SubtitleAction::Embedded(id) => {
                let name = self
                    .state
                    .subtitle_list
                    .iter()
                    .find(|track| track.id == id)
                    .map(captions::track_name)
                    .unwrap_or_default();
                self.panel = None;
                self.use_embedded(id, &name, cx);
            }
            SubtitleAction::Online(id) => {
                let found = self
                    .online
                    .iter()
                    .flat_map(|group| &group.options)
                    .find(|option| option.id == id)
                    .cloned();
                if let Some(subtitle) = found {
                    self.panel = None;
                    self.use_online(subtitle, true, cx);
                }
            }
        }
        cx.notify();
    }

    /// "Breaking Bad · S1 E3 · Gray Matter".
    fn title_line(&self) -> String {
        let Some(detail) = &self.detail else {
            return String::new();
        };
        let title = detail.media.title.clone();
        let Some(parsed) = &self.parsed else {
            return title;
        };
        let name = detail
            .episodes
            .iter()
            .find(|e| e.id == self.id)
            .and_then(|e| e.name.clone());
        match (parsed.season, parsed.episode) {
            (Some(season), Some(episode)) => format!(
                "{title} · S{season} E{episode}{}",
                name.map(|n| format!(" · {n}")).unwrap_or_default()
            ),
            (None, Some(episode)) => format!("{title} · Episode {episode}"),
            _ => title,
        }
    }

    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .child(
                TitleBar::new()
                    .h(px(TOP_BAR))
                    .pl(px(92.))
                    .pr(px(24.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(color(CANVAS).opacity(0.85), 0.),
                        linear_color_stop(color(CANVAS).opacity(0.), 1.),
                    ))
                    .border_color(color(CANVAS).opacity(0.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w_full()
                            .gap_4()
                            .child(icon_button(
                                "watch-back",
                                IconName::ChevronLeft,
                                |_, _, cx| leave(cx),
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(TEXT))
                                    .truncate()
                                    .child(self.title_line()),
                            ),
                    ),
            )
            .on_mouse_move(cx.listener(|page, _, _, cx| page.activity(cx)))
    }

    fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.state;
        let shown = self.scrubbing.unwrap_or(if state.duration > 0. {
            (state.position / state.duration).clamp(0., 1.) as f32
        } else {
            0.
        });
        let bar = self.bar.clone();
        let remaining = (state.duration - state.position).max(0.);

        let seek = div()
            .id("seek")
            .relative()
            .flex_1()
            .h(px(20.))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, event: &MouseDownEvent, _, cx| {
                    page.scrubbing = Some(page.bar_fraction(event.position.x));
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|page, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    page.scrubbing = Some(page.bar_fraction(event.position.x));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|page, _, _, cx| page.finish_scrub(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|page, _, _, cx| page.finish_scrub(cx)),
            )
            .child(
                canvas(move |bounds, _, _| bar.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(color(TEXT).opacity(0.25))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(relative(shown))
                            .rounded_full()
                            .bg(color(ACCENT)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(shown))
                    .ml(px(-6.))
                    .size(px(12.))
                    .rounded_full()
                    .bg(color(TEXT)),
            );

        let track = |text: &str| {
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        };
        let next = self.neighbour(1);

        div()
            .w_full()
            .px(px(GUTTER))
            .pb_6()
            .pt(px(72.))
            .flex()
            .flex_col()
            .gap_2()
            .bg(linear_gradient(
                0.,
                linear_color_stop(color(CANVAS).opacity(0.92), 0.),
                linear_color_stop(color(CANVAS).opacity(0.), 1.),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(52.))
                            .text_size(px(12.))
                            .text_color(color(TEXT))
                            .child(clock(state.position)),
                    )
                    .child(seek)
                    .child(
                        div()
                            .w(px(52.))
                            .text_size(px(12.))
                            .text_color(color(TEXT_DIM))
                            .child(format!("−{}", clock(remaining))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(icon_button(
                        "pause",
                        if state.paused {
                            IconName::Play
                        } else {
                            IconName::Pause
                        },
                        cx.listener(|page, _, _, cx| page.with_player(cx, Player::toggle_pause)),
                    ))
                    .child(icon_button(
                        "back-10",
                        IconName::RotateCcw,
                        cx.listener(|page, _, _, cx| {
                            page.with_player(cx, |player| player.seek_by(-10.))
                        }),
                    ))
                    .child(icon_button(
                        "forward-10",
                        IconName::RotateCw,
                        cx.listener(|page, _, _, cx| {
                            page.with_player(cx, |player| player.seek_by(10.))
                        }),
                    ))
                    .child(control_button(
                        "mute",
                        if state.muted || state.volume < 1. {
                            IconName::VolumeX
                        } else {
                            IconName::Volume2
                        },
                        Some(if state.muted {
                            "Muted".to_string()
                        } else {
                            format!("{}%", state.volume as u32)
                        }),
                        cx.listener(|page, _, _, cx| page.with_player(cx, Player::toggle_mute)),
                    ))
                    .child(div().flex_1())
                    .child(control_button(
                        "subtitles",
                        IconName::Captions,
                        Some(state.subtitle.clone()),
                        cx.listener(|page, _, _, cx| page.toggle_panel(Panel::Subtitles, cx)),
                    ))
                    .when(state.audio_tracks > 1, |row| {
                        row.child(control_button(
                            "audio",
                            IconName::Languages,
                            track(&state.audio),
                            cx.listener(|page, _, _, cx| page.with_player(cx, Player::cycle_audio)),
                        ))
                    })
                    .child(icon_button(
                        "sources",
                        IconName::Layers,
                        cx.listener(|page, _, _, cx| page.toggle_panel(Panel::Sources, cx)),
                    ))
                    .when(self.episodes.is_some(), |row| {
                        row.child(icon_button(
                            "episodes",
                            IconName::List,
                            cx.listener(|page, _, _, cx| page.toggle_panel(Panel::Episodes, cx)),
                        ))
                    })
                    .children(next.map(|id| {
                        icon_button("next", IconName::SkipForward, move |_, _, cx| {
                            replace(Route::Watch { id: id.clone() }, cx)
                        })
                    }))
                    .child(icon_button(
                        "fullscreen",
                        IconName::Maximize,
                        |_, window, _| window.toggle_fullscreen(),
                    )),
            )
    }

    /// The card that offers the next episode as this one runs out.
    fn next_card(&self) -> Option<impl IntoElement> {
        let next = self.neighbour(1)?;
        let remaining = self.state.duration - self.state.position;
        if self.state.duration <= 0. || remaining > NEXT_OFFER || self.state.ended {
            return None;
        }
        Some(
            div()
                .occlude()
                .absolute()
                .right(px(GUTTER))
                .bottom(px(150.))
                .flex()
                .items_center()
                .gap_4()
                .px_4()
                .py_3()
                .rounded_xl()
                .bg(color(SURFACE).opacity(0.95))
                .border_1()
                .border_color(color(LINE))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .child("Next episode"),
                )
                .child(secondary_button(
                    "next-now",
                    "Play now",
                    Some(IconName::SkipForward),
                    move |_, _, cx| replace(Route::Watch { id: next.clone() }, cx),
                )),
        )
    }

    /// The panel on the right, if one is open.
    fn side_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let panel = self.panel?;
        let page = cx.entity();
        let close = cx.listener(|page, _, _, cx| {
            page.panel = None;
            cx.notify();
        });
        Some(match panel {
            Panel::Episodes => {
                let list = self.episodes.clone()?;
                panels::shell("Episodes", close, list)
            }
            Panel::Sources => {
                let view = match &self.sources {
                    None | Some(Sources::Loading) => SourcesView::Loading,
                    Some(Sources::Failed(reason)) => SourcesView::Failed(reason),
                    Some(Sources::Ready(choices)) => SourcesView::Ready(choices),
                };
                let playing = self.stream.as_ref().map(|stream| stream.info_hash.as_str());
                let act = Rc::new(move |action: SourceAction, cx: &mut App| {
                    page.update(cx, |page, cx| match action {
                        SourceAction::Pick(info_hash) => page.play_release(info_hash, cx),
                        SourceAction::Retry => {
                            page.load_sources(cx);
                            cx.notify();
                        }
                    })
                });
                panels::shell("Sources", close, panels::sources_body(view, playing, act))
            }
            Panel::Subtitles => {
                let on_screen = self.state.sid.is_some_and(|sid| {
                    self.state
                        .subtitle_list
                        .iter()
                        .any(|track| track.id == sid && track.external)
                });
                let menu = SubtitleMenu {
                    selected: self.state.sid,
                    embedded: self
                        .state
                        .subtitle_list
                        .iter()
                        .filter(|track| !track.external)
                        .collect(),
                    online: &self.online,
                    online_in_use: self.online_in_use.as_deref().filter(|_| on_screen),
                    expanded: self.language_open.as_deref(),
                };
                let act = Rc::new(move |action: SubtitleAction, cx: &mut App| {
                    page.update(cx, |page, cx| page.subtitle_action(action, cx))
                });
                panels::shell("Subtitles", close, panels::subtitles_body(menu, act))
            }
        })
    }

    /// What fills the window: a status while looking, the picture while playing.
    fn screen(&self, cx: &mut Context<Self>) -> Div {
        let frame = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgb(0x000000));
        match &self.phase {
            Phase::Resolving => frame
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Spinner::new())
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .child("Finding the best source"),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(color(TEXT_DIM))
                        .child("This can take up to a minute."),
                ),
            Phase::Unavailable(reason) => frame.flex().items_center().justify_center().child(
                error_state(
                    "No playable source found",
                    reason.clone(),
                    cx.listener(|page, _, _, cx| page.start(true, cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(back_to_sources(cx.listener(|page, _, _, cx| {
                            page.toggle_panel(Panel::Sources, cx)
                        })))
                        .children(self.neighbour(-1).map(|id| {
                            secondary_button(
                                "try-previous",
                                "Previous episode",
                                None,
                                move |_, _, cx| replace(Route::Watch { id: id.clone() }, cx),
                            )
                        }))
                        .children(self.neighbour(1).map(|id| {
                            secondary_button("try-next", "Next episode", None, move |_, _, cx| {
                                replace(Route::Watch { id: id.clone() }, cx)
                            })
                        })),
                ),
            ),
            Phase::Playing => {
                let waiting = self.state.buffering || !self.state.video_ready;
                frame
                    .children(self.video.clone())
                    .child(
                        div()
                            .id("surface")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .on_click(cx.listener(|page, _, _, cx| {
                                page.with_player(cx, Player::toggle_pause)
                            }))
                            .on_double_click(|_, window, _| window.toggle_fullscreen()),
                    )
                    .children(waiting.then(|| {
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(Spinner::new())
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(TEXT))
                                    .child(if self.state.video_ready {
                                        "Buffering"
                                    } else {
                                        "Starting"
                                    }),
                            )
                            .children(self.transfer.map(|transfer| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(color(TEXT_DIM))
                                    .child(transfer_line(&transfer))
                            }))
                    }))
            }
        }
    }
}

/// Back to where the viewer came from: the title page of what is playing.
fn leave(cx: &mut App) {
    crate::route::go_back(cx);
}

impl Render for WatchPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keys go to this page. When nothing has focus (at the start, or after the search
        // palette closed and took its input with it) it takes focus back.
        if !self.took_focus {
            window.focus(&self.focus, cx);
            self.took_focus = true;
        }
        let viewport = window.viewport_size();
        // Frames are rendered no larger than the picture is shown.
        if let Some(player) = &self.player {
            let width = viewport.width.min(px(MAX_WIDTH as f32));
            player.set_frame_size(
                f32::from(width) as u32,
                f32::from(width * (viewport.height / viewport.width)) as u32,
            );
        }

        let visible = self.controls_visible;
        let playing = matches!(self.phase, Phase::Playing);
        let fade = |element: Div, id: &'static str| {
            element
                .when(!visible, |element| element.invisible())
                .with_animation(
                    SharedString::from(format!("{id}-{visible}")),
                    Animation::new(Duration::from_millis(250)).with_easing(ease_out_quint()),
                    move |element, progress| {
                        element.opacity(if visible { progress } else { 1. - progress })
                    },
                )
        };

        div()
            .id("watch")
            .relative()
            .w(viewport.width)
            .h(viewport.height)
            .overflow_hidden()
            .bg(rgb(0x000000))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|page, event: &KeyDownEvent, window, cx| {
                    page.on_key(event, window, cx)
                }),
            )
            .on_mouse_move(cx.listener(|page, _, _, cx| page.activity(cx)))
            .child(self.screen(cx))
            .child(fade(
                div()
                    .when(visible, |bar| bar.occlude())
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .child(self.top_bar(cx)),
                "top",
            ))
            .when(playing, |page| {
                page.child(fade(
                    div()
                        .when(visible, |bar| bar.occlude())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .w_full()
                        .child(self.controls(cx)),
                    "controls",
                ))
                .children(self.next_card())
            })
            .children(self.message.as_ref().map(|(text, _)| {
                div()
                    .absolute()
                    .top(px(96.))
                    .left_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .rounded_full()
                            .bg(color(CANVAS).opacity(0.8))
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color(TEXT))
                            .child(text.clone()),
                    )
            }))
            .children(self.side_panel(cx))
            .when(self.detail.is_none() && !playing, |page| {
                page.child(
                    div()
                        .absolute()
                        .bottom(px(24.))
                        .left(px(GUTTER))
                        .child(skeleton(px(160.), px(10.))),
                )
            })
    }
}

/// "3.1 MB/s · 22 peers".
fn transfer_line(transfer: &StreamStats) -> String {
    let megabytes = transfer.bytes_per_second as f64 / 1_000_000.;
    format!("{megabytes:.1} MB/s · {} peers", transfer.peers)
}

/// 3725 seconds as "1:02:05".
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.) as u64;
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamStats, clock, transfer_line};

    #[test]
    fn describes_a_transfer() {
        let transfer = StreamStats {
            downloaded_bytes: 0,
            bytes_per_second: 3_140_000,
            peers: 22,
        };
        assert_eq!(transfer_line(&transfer), "3.1 MB/s · 22 peers");
    }

    #[test]
    fn formats_a_position() {
        assert_eq!(clock(0.), "0:00");
        assert_eq!(clock(65.4), "1:05");
        assert_eq!(clock(3725.), "1:02:05");
        assert_eq!(clock(-3.), "0:00");
    }
}
