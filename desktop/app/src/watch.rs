//! The watch page: the video fills the window, and its controls come and go.

use crate::backend::Backend;
use crate::design::button::{control_button, icon_button, secondary_button};
use crate::design::state::{error_state, skeleton};
use crate::design::tokens::{
    ACCENT, CANVAS, GUTTER, LINE, SURFACE, TEXT, TEXT_DIM, TOP_BAR, color,
};
use crate::episodes::EpisodeList;
use crate::player::{MAX_WIDTH, Player, PlayerState};
use crate::progress::Entry;
use crate::route::{Route, replace};
use crate::runtime;
use akidev_core::media_id::{self, MediaId, anime_episode_id};
use akidev_core::model::{Detail, Kind};
use akidev_core::providers;
use akidev_core::stream::{Resolution, StreamId};
use akidev_core::torrent::StreamStats;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{InteractiveElementExt as _, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
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
    episodes_open: bool,
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
            id,
            parsed,
            detail: None,
            episodes: None,
            episodes_open: false,
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
        self.save_progress(cx);
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
        let handle = runtime::spawn(async move { streamer.resolve(&id, fresh).await });
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
                self.phase = Phase::Unavailable(reason);
                cx.notify();
                return;
            }
        };
        let backend = cx.global::<Backend>();
        let resume = backend.progress.resume_at(&self.id);
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
            }
            Err(error) => self.phase = Phase::Unavailable(error),
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
            && !self.episodes_open;
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
            "e" if self.episodes.is_some() => {
                self.episodes_open = !self.episodes_open;
                cx.notify();
            }
            "n" => {
                if let Some(next) = self.neighbour(1) {
                    replace(Route::Watch { id: next }, cx);
                }
            }
            "f" => window.toggle_fullscreen(),
            "escape" if self.episodes_open => {
                self.episodes_open = false;
                cx.notify();
            }
            "escape" if window.is_fullscreen() => window.toggle_fullscreen(),
            _ => {}
        }
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
                    .when(state.subtitle_tracks > 0, |row| {
                        row.child(control_button(
                            "subtitles",
                            IconName::Captions,
                            Some(state.subtitle.clone()),
                            cx.listener(|page, _, _, cx| {
                                page.with_player(cx, Player::cycle_subtitles)
                            }),
                        ))
                    })
                    .when(state.audio_tracks > 1, |row| {
                        row.child(control_button(
                            "audio",
                            IconName::Languages,
                            track(&state.audio),
                            cx.listener(|page, _, _, cx| page.with_player(cx, Player::cycle_audio)),
                        ))
                    })
                    .when(self.episodes.is_some(), |row| {
                        row.child(icon_button(
                            "episodes",
                            IconName::List,
                            cx.listener(|page, _, _, cx| {
                                page.episodes_open = !page.episodes_open;
                                cx.notify();
                            }),
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

    fn episodes_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let list = self.episodes.clone().filter(|_| self.episodes_open)?;
        Some(
            div()
                .occlude()
                .absolute()
                .top(px(TOP_BAR))
                .right_0()
                .bottom_0()
                .w(px(400.))
                .flex()
                .flex_col()
                .bg(color(SURFACE).opacity(0.97))
                .border_l_1()
                .border_color(color(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_3()
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(color(TEXT))
                                .child("Episodes"),
                        )
                        .child(icon_button(
                            "close-episodes",
                            IconName::X,
                            cx.listener(|page, _, _, cx| {
                                page.episodes_open = false;
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    div()
                        .id("episodes-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .restrict_scroll_to_axis()
                        .child(list),
                ),
        )
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
            .children(self.episodes_panel(cx))
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
