//! The music player: one for the whole app, so what is playing carries on as screens change, and
//! the bar along the bottom that shows it.

use crate::audio::{AudioPlayer, AudioState};
use crate::design::tokens::{LINE, TEXT, TEXT_DIM, TEXT_FAINT, color};
use crate::music_ui::{cover, names};
use akidev_core::music::{Music, Track};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// How often the playback state is read from mpv.
const TICK: Duration = Duration::from_millis(250);
/// Previous goes back a track only when the track is this far in or less.
const RESTART_AFTER: f64 = 3.;
pub const BAR_HEIGHT: f32 = 80.;

/// The catalogue, and where yt-dlp lives. Set once at start.
pub struct MusicHub {
    pub music: Arc<Music>,
    pub ytdlp_dir: PathBuf,
    pub player: Entity<MusicPlayer>,
}

impl Global for MusicHub {}

pub fn hub(cx: &App) -> &MusicHub {
    cx.global::<MusicHub>()
}

/// Plays `tracks` from `start`; the rest queue up after it.
pub fn play(tracks: Vec<Track>, start: usize, cx: &mut App) {
    let player = hub(cx).player.clone();
    player.update(cx, |player, cx| player.play(tracks, start, cx));
}

pub fn play_shuffled(tracks: Vec<Track>, cx: &mut App) {
    let player = hub(cx).player.clone();
    player.update(cx, |player, cx| player.play_shuffled(tracks, cx));
}

pub fn toggle(cx: &mut App) {
    let player = hub(cx).player.clone();
    player.update(cx, |player, cx| player.toggle(cx));
}

/// Told to the screens that mark the current track, when it or whether it plays changes.
pub struct Changed;

impl EventEmitter<Changed> for MusicPlayer {}

#[derive(Clone, Copy, PartialEq)]
pub enum Repeat {
    Off,
    All,
    One,
}

enum Status {
    Idle,
    /// Finding the audio (and, the first time, fetching yt-dlp).
    Loading,
    Failed(String),
}

pub struct MusicPlayer {
    queue: Vec<Track>,
    /// The queue before shuffling, so turning shuffle off puts it back.
    ordered: Option<Vec<Track>>,
    index: usize,
    shuffle: bool,
    repeat: Repeat,
    audio: Option<AudioPlayer>,
    status: Status,
    state: AudioState,
    volume: f32,
    /// Bumped for every track asked for, so a slow answer for a skipped one is ignored.
    request: u64,
    /// The end of this request has been handled, so the next track starts once.
    ended_handled: bool,
    queue_open: bool,
    seek_bar: Rc<Cell<Bounds<Pixels>>>,
    volume_bar: Rc<Cell<Bounds<Pixels>>>,
    scrubbing: Option<f32>,
}

impl MusicPlayer {
    pub fn new(cx: &mut Context<Self>) -> MusicPlayer {
        cx.spawn(async move |player, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if player.update(cx, |player, cx| player.tick(cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
        MusicPlayer {
            queue: Vec::new(),
            ordered: None,
            index: 0,
            shuffle: false,
            repeat: Repeat::Off,
            audio: None,
            status: Status::Idle,
            state: AudioState::default(),
            volume: 1.,
            request: 0,
            ended_handled: false,
            queue_open: false,
            seek_bar: Rc::new(Cell::new(Bounds::default())),
            volume_bar: Rc::new(Cell::new(Bounds::default())),
            scrubbing: None,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.queue.get(self.index)
    }

    pub fn is_playing(&self) -> bool {
        self.current().is_some() && !self.state.paused && !matches!(self.status, Status::Failed(_))
    }

    pub fn play(&mut self, tracks: Vec<Track>, start: usize, cx: &mut Context<Self>) {
        self.set_queue(tracks, start, self.shuffle);
        self.load_current(cx);
    }

    pub fn play_shuffled(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        if tracks.is_empty() {
            return;
        }
        let start = (time_seed() as usize) % tracks.len();
        self.set_queue(tracks, start, true);
        self.load_current(cx);
    }

    fn set_queue(&mut self, tracks: Vec<Track>, start: usize, shuffle: bool) {
        self.shuffle = shuffle;
        if shuffle && let Some(first) = tracks.get(start).cloned() {
            let mut rest: Vec<Track> = tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != start)
                .map(|(_, t)| t.clone())
                .collect();
            shuffle_in_place(&mut rest);
            self.ordered = Some(tracks);
            self.queue = std::iter::once(first).chain(rest).collect();
            self.index = 0;
        } else {
            self.ordered = None;
            self.queue = tracks;
            self.index = start.min(self.queue.len().saturating_sub(1));
        }
    }

    /// Finds the audio of the current track and starts it.
    fn load_current(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.current().cloned() else {
            return;
        };
        self.request += 1;
        let request = self.request;
        self.status = Status::Loading;
        self.state = AudioState::default();
        self.ended_handled = false;
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        let (music, dir) = {
            let hub = hub(cx);
            (hub.music.clone(), hub.ytdlp_dir.clone())
        };
        crate::task::load(
            cx,
            async move {
                akidev_core::music::install_ytdlp(&dir).await?;
                music.resolve(&track.id).await
            },
            move |player, result, cx| {
                if player.request != request {
                    return;
                }
                match result {
                    Ok(resolved) => player.start_audio(resolved, cx),
                    Err(error) => {
                        tracing::warn!("[music] no audio: {error:#}");
                        player.status =
                            Status::Failed("Could not find audio for this track.".into());
                        cx.notify();
                        cx.emit(Changed);
                    }
                }
            },
        );
        cx.notify();
        cx.emit(Changed);
    }

    fn start_audio(&mut self, resolved: akidev_core::music::Resolved, cx: &mut Context<Self>) {
        if self.audio.is_none() {
            match AudioPlayer::new(f64::from(self.volume) * 100.) {
                Ok(audio) => self.audio = Some(audio),
                Err(error) => {
                    self.status = Status::Failed(error);
                    cx.notify();
                    return;
                }
            }
        }
        let agent = resolved
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
            .map(|(_, value)| value.as_str());
        if let Some(audio) = &self.audio {
            audio.load(&resolved.url, agent);
        }
        self.status = Status::Idle;
        cx.notify();
        cx.emit(Changed);
    }

    /// Reads mpv's state: redraws the bar if it moved, and moves on when a track ends or fails.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let Some(audio) = &self.audio else {
            return;
        };
        let state = audio.state();
        let changed_flags = state.paused != self.state.paused;
        if state != self.state && self.scrubbing.is_none() || changed_flags {
            let failed = state.error.clone();
            self.state = state;
            if let Some(error) = failed {
                tracing::warn!("[music] playback failed: {error}");
                self.status = Status::Failed("This track could not be played.".into());
                if self.index + 1 < self.queue.len() {
                    self.index += 1;
                    self.load_current(cx);
                }
            }
            cx.notify();
            if changed_flags {
                cx.emit(Changed);
            }
        }
        if self.state.ended && !self.ended_handled {
            self.ended_handled = true;
            match self.repeat {
                Repeat::One => {
                    if let Some(audio) = &self.audio {
                        audio.seek_to(0.);
                        audio.resume();
                    }
                    self.ended_handled = false;
                    self.state.ended = false;
                }
                _ => self.next(cx),
            }
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(audio) = &self.audio {
            audio.toggle_pause();
        }
        cx.notify();
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        if self.index + 1 < self.queue.len() {
            self.index += 1;
            self.load_current(cx);
        } else if self.repeat == Repeat::All && !self.queue.is_empty() {
            self.index = 0;
            self.load_current(cx);
        } else if let Some(audio) = &self.audio {
            // The last track ended: it stays loaded, paused at its end.
            if !self.state.paused {
                audio.toggle_pause();
            }
        }
    }

    pub fn previous(&mut self, cx: &mut Context<Self>) {
        if self.state.position > RESTART_AFTER || self.index == 0 {
            if let Some(audio) = &self.audio {
                audio.seek_to(0.);
            }
        } else {
            self.index -= 1;
            self.load_current(cx);
        }
    }

    fn jump(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.index {
            if let Some(audio) = &self.audio {
                audio.seek_to(0.);
            }
        } else if index < self.queue.len() {
            self.index = index;
            self.load_current(cx);
        }
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        let Some(current) = self.current().cloned() else {
            self.shuffle = !self.shuffle;
            cx.notify();
            return;
        };
        if !self.shuffle {
            // What is ahead is shuffled; what was played stays where it is.
            self.ordered = Some(self.queue.clone());
            let mut rest = self.queue.split_off(self.index + 1);
            shuffle_in_place(&mut rest);
            self.queue.extend(rest);
            self.shuffle = true;
        } else {
            let back = self.ordered.take().unwrap_or_else(|| self.queue.clone());
            self.index = back.iter().position(|t| t.id == current.id).unwrap_or(0);
            self.queue = back;
            self.shuffle = false;
        }
        cx.notify();
    }

    fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
        cx.notify();
    }

    fn seek_fraction(&self, x: Pixels) -> f32 {
        fraction_in(self.seek_bar.get(), x)
    }

    fn finish_scrub(&mut self, cx: &mut Context<Self>) {
        if let Some(fraction) = self.scrubbing.take()
            && self.state.duration > 0.
            && let Some(audio) = &self.audio
        {
            audio.seek_to(f64::from(fraction) * self.state.duration);
            self.state.position = f64::from(fraction) * self.state.duration;
        }
        cx.notify();
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.volume = volume.clamp(0., 1.);
        if let Some(audio) = &self.audio {
            audio.set_volume(f64::from(self.volume) * 100.);
        }
        cx.notify();
    }
}

fn fraction_in(bar: Bounds<Pixels>, x: Pixels) -> f32 {
    if bar.size.width <= px(0.) {
        return 0.;
    }
    ((x - bar.origin.x) / bar.size.width).clamp(0., 1.)
}

fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0)
}

/// Fisher–Yates with a small xorshift: shuffling a queue needs no better.
fn shuffle_in_place<T>(list: &mut [T]) {
    let mut seed = time_seed() | 1;
    for i in (1..list.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        list.swap(i, (seed % (i as u64 + 1)) as usize);
    }
}

pub fn clock(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0. {
        return "0:00".into();
    }
    let whole = seconds as u64;
    let (h, m, s) = (whole / 3600, whole % 3600 / 60, whole % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn round(id: &'static str, icon: IconName, on: bool, big: bool) -> Stateful<Div> {
    let side = if big { 36. } else { 32. };
    let base = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(side))
        .rounded_full()
        .cursor_pointer();
    if big {
        base.bg(color(TEXT))
            .hover(|style| style.bg(color(TEXT).opacity(0.85)))
            .child(Icon::new(icon).small().text_color(color(0x000000)))
    } else {
        base.hover(|style| style.text_color(color(TEXT))).child(
            Icon::new(icon)
                .small()
                .text_color(color(if on { TEXT } else { TEXT_DIM })),
        )
    }
}

/// The track and fill of a slider: 4 pixels tall, white up to `shown` (0 to 1). The bounds are
/// recorded so a click can be turned into a position.
fn slider_track(shown: f32, bounds: Rc<Cell<Bounds<Pixels>>>) -> Div {
    div()
        .relative()
        .w_full()
        .h(px(16.))
        .flex()
        .items_center()
        .child(
            canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(4.))
                .rounded_full()
                .bg(color(TEXT).opacity(0.22))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h_full()
                        .w(relative(shown.clamp(0., 1.)))
                        .rounded_full()
                        .bg(color(TEXT)),
                ),
        )
}

impl Render for MusicPlayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(track) = self.current().cloned() else {
            return div().into_any_element();
        };
        let loading = matches!(self.status, Status::Loading) || self.state.buffering;
        let playing = self.is_playing();
        let duration = self.state.duration;
        let shown = self.scrubbing.unwrap_or(if duration > 0. {
            (self.state.position / duration).clamp(0., 1.) as f32
        } else {
            0.
        });
        let failed = match &self.status {
            Status::Failed(message) => Some(message.clone()),
            _ => None,
        };
        let preparing = matches!(self.status, Status::Loading);

        let seek = div()
            .id("music-seek")
            .flex_1()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|player, event: &MouseDownEvent, _, cx| {
                    player.scrubbing = Some(player.seek_fraction(event.position.x));
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|player, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    player.scrubbing = Some(player.seek_fraction(event.position.x));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|player, _, _, cx| player.finish_scrub(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|player, _, _, cx| player.finish_scrub(cx)),
            )
            .child(slider_track(shown, self.seek_bar.clone()));

        let volume = div()
            .id("music-volume")
            .w(px(96.))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|player, event: &MouseDownEvent, _, cx| {
                    let fraction = fraction_in(player.volume_bar.get(), event.position.x);
                    player.set_volume(fraction, cx);
                }),
            )
            .on_mouse_move(cx.listener(|player, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let fraction = fraction_in(player.volume_bar.get(), event.position.x);
                    player.set_volume(fraction, cx);
                }
            }))
            .child(slider_track(self.volume, self.volume_bar.clone()));

        let time = |seconds: f64, right: bool| {
            div()
                .w(px(44.))
                .text_size(px(11.))
                .text_color(color(TEXT_DIM))
                .when(right, |d| d.text_right())
                .child(clock(seconds))
        };
        let shown_position = shown as f64 * duration;

        let bar = div()
            .w_full()
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .gap_4()
            .px_4()
            .bg(color(0x000000))
            .border_t_1()
            .border_color(color(LINE))
            .block_mouse_except_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .w(relative(0.3))
                    .min_w_0()
                    .child(cover(track.cover.clone(), px(56.), false, "bar"))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(TEXT))
                                    .child(track.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .text_color(color(if failed.is_some() {
                                        crate::design::tokens::DANGER
                                    } else {
                                        TEXT_DIM
                                    }))
                                    .child(failed.clone().unwrap_or_else(|| {
                                        if preparing {
                                            "Loading…".into()
                                        } else {
                                            names(&track.artists)
                                        }
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                round("music-shuffle", IconName::Shuffle, self.shuffle, false)
                                    .on_click(cx.listener(|p, _, _, cx| p.toggle_shuffle(cx))),
                            )
                            .child(
                                round("music-prev", IconName::SkipBack, false, false)
                                    .on_click(cx.listener(|p, _, _, cx| p.previous(cx))),
                            )
                            .child(
                                round(
                                    "music-play",
                                    if loading {
                                        IconName::Loader
                                    } else if playing {
                                        IconName::Pause
                                    } else {
                                        IconName::Play
                                    },
                                    true,
                                    true,
                                )
                                .on_click(cx.listener(|p, _, _, cx| p.toggle(cx))),
                            )
                            .child(
                                round("music-next", IconName::SkipForward, false, false)
                                    .on_click(cx.listener(|p, _, _, cx| p.next(cx))),
                            )
                            .child(
                                round(
                                    "music-repeat",
                                    if self.repeat == Repeat::One {
                                        IconName::Repeat1
                                    } else {
                                        IconName::Repeat
                                    },
                                    self.repeat != Repeat::Off,
                                    false,
                                )
                                .on_click(cx.listener(|p, _, _, cx| p.cycle_repeat(cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .w_full()
                            .max_w(px(560.))
                            .child(time(shown_position, true))
                            .child(seek)
                            .child(time(duration, false)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .w(relative(0.3))
                    .child(
                        round("music-queue", IconName::ListMusic, self.queue_open, false).on_click(
                            cx.listener(|p, _, _, cx| {
                                p.queue_open = !p.queue_open;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        round(
                            "music-mute",
                            if self.volume == 0. {
                                IconName::VolumeX
                            } else {
                                IconName::Volume2
                            },
                            false,
                            false,
                        )
                        .on_click(cx.listener(|p, _, _, cx| {
                            let next = if p.volume == 0. { 1. } else { 0. };
                            p.set_volume(next, cx);
                        })),
                    )
                    .child(volume),
            );

        div()
            .relative()
            .w_full()
            .child(bar)
            .when(self.queue_open, |root| root.child(self.queue_panel(cx)))
            .into_any_element()
    }
}

impl MusicPlayer {
    fn queue_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row = |index: usize, cx: &mut Context<Self>| {
            let track = &self.queue[index];
            let here = index == self.index;
            div()
                .id(SharedString::from(format!("queue-{index}")))
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(color(TEXT).opacity(0.07)))
                .on_click(cx.listener(move |p, _, _, cx| p.jump(index, cx)))
                .child(cover(
                    track.cover.clone(),
                    px(40.),
                    false,
                    &format!("queue-{index}"),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(14.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(TEXT))
                                .child(track.title.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(color(TEXT_DIM))
                                .child(names(&track.artists)),
                        ),
                )
                .when(here, |row| {
                    row.child(Icon::new(IconName::Volume2).small().text_color(color(TEXT)))
                })
        };
        let rows: Vec<_> = (self.index..self.queue.len()).map(|i| row(i, cx)).collect();
        div()
            .id("music-queue-panel")
            .absolute()
            .right(px(8.))
            .bottom(px(BAR_HEIGHT + 8.))
            .w(px(380.))
            .max_h(px(480.))
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .flex()
            .flex_col()
            .p_2()
            .rounded_lg()
            .bg(color(0x121212))
            .border_1()
            .border_color(color(LINE))
            .shadow_lg()
            .block_mouse_except_scroll()
            .child(
                div()
                    .px_2()
                    .py_1p5()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT_FAINT))
                    .child("Now playing and next up"),
            )
            .children(rows)
    }
}
