//! The music player: one for the whole app, so what is playing carries on as screens change, and
//! the bar along the bottom that shows it.

use crate::audio::{AudioPlayer, AudioState};
use crate::design::tokens::{LINE, TEXT, TEXT_DIM, TEXT_FAINT, color};
use crate::music_ui::{cover, names};
use crate::party::{Party, PartyEvent, PartyHandle, Role};
use akidev_core::music::{Credit, Music, Track};
use akidev_party::{Item, Message, Playback, Readiness, fix};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the playback state is read from mpv.
const TICK: Duration = Duration::from_millis(250);
/// Previous goes back a track only when the track is this far in or less.
const RESTART_AFTER: f64 = 3.;
/// How often a host tells its party where the music is while it plays.
const PARTY_BEAT: Duration = Duration::from_secs(2);
/// A guest seeks to catch up no more often than this, so a seek has time to land.
const CATCH_UP_EVERY: Duration = Duration::from_secs(1);
/// A jump of the position bigger than this, between two looks, is a seek.
const SEEK_JUMP: f64 = 1.;
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

/// Stops the music where it is, so a video can have the sound to itself.
pub fn pause(cx: &mut App) {
    let player = hub(cx).player.clone();
    player.update(cx, |player, cx| player.pause(cx));
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

/// What a host last told its party, to know when to tell it again.
#[derive(Default)]
struct PartySync {
    /// When it was last told, or `None` to tell it at the next look.
    told: Option<Instant>,
    /// The track, and where it was, at the last look, and whether it was playing.
    looked: Option<(String, Instant, f64, bool)>,
}

/// The party, found when the window has made it (this player is made first), and what is watched
/// of it.
struct Joined {
    party: Entity<Party>,
    _requests: Subscription,
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
    /// The track whose "what plays next" has been asked for, so each is asked for once.
    radio_for: Option<String>,
    party: Option<Joined>,
    party_sync: PartySync,
    /// The speed a guest was last set to, so it is only set when it changes.
    speed: f64,
    last_catch_up: Instant,
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
            radio_for: None,
            party: None,
            party_sync: PartySync::default(),
            speed: 1.,
            last_catch_up: Instant::now(),
        }
    }

    /// The party, once the window has made it.
    fn party(&mut self, cx: &mut Context<Self>) -> Option<Entity<Party>> {
        if self.party.is_none() {
            let party = cx.try_global::<PartyHandle>()?.0.clone();
            let requests = cx.subscribe(&party, |player, _, event: &PartyEvent, cx| {
                player.on_party_request(event, cx)
            });
            self.party = Some(Joined {
                party,
                _requests: requests,
            });
        }
        self.party.as_ref().map(|joined| joined.party.clone())
    }

    fn role(&self, cx: &App) -> Role {
        self.party
            .as_ref()
            .map_or(Role::None, |joined| joined.party.read(cx).role())
    }

    /// A guest cannot choose what plays: that is the host's.
    fn blocked(&self, cx: &mut Context<Self>) -> bool {
        if self.role(cx) != Role::Guest {
            return false;
        }
        crate::route::toast("Only the host chooses what plays.", cx);
        true
    }

    pub fn current(&self) -> Option<&Track> {
        self.queue.get(self.index)
    }

    pub fn is_playing(&self) -> bool {
        self.current().is_some() && !self.state.paused && !matches!(self.status, Status::Failed(_))
    }

    pub fn play(&mut self, tracks: Vec<Track>, start: usize, cx: &mut Context<Self>) {
        if self.blocked(cx) {
            return;
        }
        self.set_queue(tracks, start, self.shuffle);
        self.load_current(cx);
    }

    pub fn play_shuffled(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        if tracks.is_empty() || self.blocked(cx) {
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

    /// Autoplay: when the last track of the queue starts, what YouTube Music plays after it is
    /// added, so the music does not stop. Not while repeating (the queue loops), and not for a
    /// guest, whose music is the host's choice.
    fn autoplay(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.current() else { return };
        if self.index + 1 != self.queue.len()
            || self.repeat != Repeat::Off
            || self.role(cx) == Role::Guest
            || self.radio_for.as_deref() == Some(track.id.as_str())
        {
            return;
        }
        let id = track.id.clone();
        self.radio_for = Some(id.clone());
        let music = hub(cx).music.clone();
        crate::task::load(
            cx,
            async move { music.radio(&id).await },
            |player, result, cx| {
                // Nothing more to play after this one, when it fails: what happened before there
                // was autoplay.
                let Ok(songs) = result else { return };
                let add = |list: &mut Vec<Track>| {
                    let known: std::collections::HashSet<String> =
                        list.iter().map(|track| track.id.clone()).collect();
                    list.extend(songs.iter().filter(|t| !known.contains(&t.id)).cloned());
                };
                add(&mut player.queue);
                if let Some(ordered) = &mut player.ordered {
                    add(ordered);
                }
                cx.notify();
            },
        );
    }

    /// Finds the audio of the current track and starts it.
    fn load_current(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.current().cloned() else {
            return;
        };
        self.autoplay(cx);
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
        self.party_tick(cx);
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
                // The host says what comes after a track.
                _ if self.role(cx) == Role::Guest => {}
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

    /// A host tells its party what is playing; a guest is kept where the host is.
    fn party_tick(&mut self, cx: &mut Context<Self>) {
        let Some(party) = self.party(cx) else {
            return;
        };
        let role = party.read(cx).role();
        // A guest tells the host how it is doing with the track, while the track is what the party
        // is playing.
        if let (Role::Guest, Some(track)) = (role, self.current())
            && matches!(party.read(cx).item(), Some(Item::Music { .. }))
        {
            let state = if matches!(self.status, Status::Failed(_)) || self.state.error.is_some() {
                Readiness::Trouble
            } else if matches!(self.status, Status::Loading)
                || self.state.buffering
                || self.state.duration <= 0.
            {
                Readiness::Loading
            } else {
                Readiness::Ready
            };
            let id = track.id.clone();
            party.update(cx, |party, _| party.set_status(state, Some(id)));
        }
        match role {
            Role::Host => self.tell_party(&party, cx),
            Role::Guest => self.follow_party(&party, cx),
            Role::None => {}
        }
        if role != Role::Guest && self.speed != 1. {
            self.speed = 1.;
            if let Some(audio) = &self.audio {
                audio.set_speed(1.);
            }
        }
    }

    /// Tells the party the track and where it is when that has changed (a new track, paused,
    /// played, seeked), and every couple of seconds while it plays.
    fn tell_party(&mut self, party: &Entity<Party>, cx: &mut Context<Self>) {
        let Some(track) = self.current().cloned() else {
            return;
        };
        // Playing means the sound is moving: a host that is still loading or buffering is not.
        let (position, playing) = (
            self.state.position,
            !self.state.paused && !self.state.buffering,
        );
        let item = Item::Music {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: names(&track.artists),
            cover: track.cover.clone(),
        };
        // A track that is not playing and is not what the party is playing has nothing to say:
        // it must not take the party back from the film that is.
        if !playing && party.read(cx).item() != Some(&item) {
            return;
        }
        let now = Instant::now();
        let changed = match &self.party_sync.looked {
            Some((id, at, was, was_playing)) => {
                let expected = if *was_playing {
                    was + at.elapsed().as_secs_f64()
                } else {
                    *was
                };
                *id != track.id
                    || *was_playing != playing
                    || (position - expected).abs() > SEEK_JUMP
            }
            None => true,
        };
        self.party_sync.looked = Some((track.id.clone(), now, position, playing));
        let due = self
            .party_sync
            .told
            .is_none_or(|told| playing && told.elapsed() >= PARTY_BEAT);
        if !changed && !due {
            return;
        }
        let heartbeat = !changed && self.party_sync.told.is_some();
        self.party_sync.told = Some(now);
        let playback = Playback {
            item: Some(item),
            playing,
            position,
        };
        party.update(cx, |party, _| party.publish(playback, heartbeat));
    }

    /// Plays the track the host plays, where the host is.
    fn follow_party(&mut self, party: &Entity<Party>, cx: &mut Context<Self>) {
        let Some(target) = party.read(cx).target().cloned() else {
            return;
        };
        let Some(Item::Music {
            id,
            title,
            artist,
            cover,
        }) = target.playback.item.clone()
        else {
            // The party is playing something else, which has its own player.
            if let Some(audio) = &self.audio
                && !self.state.paused
            {
                audio.set_pause(true);
            }
            return;
        };
        if self.current().map(|track| track.id.as_str()) != Some(id.as_str()) {
            self.set_queue(
                vec![Track {
                    id,
                    title,
                    artists: vec![Credit {
                        id: None,
                        name: artist,
                    }],
                    album: None,
                    duration: None,
                    explicit: false,
                    cover,
                }],
                0,
                false,
            );
            self.load_current(cx);
            return;
        }
        // Until the track has loaded there is no position to correct.
        let Some(audio) = &self.audio else {
            return;
        };
        if self.state.duration <= 0. {
            return;
        }
        let playing = target.playback.playing;
        let change = fix(self.state.position, target.position_now(), playing);
        if let Some(to) = change.seek
            && self.last_catch_up.elapsed() >= CATCH_UP_EVERY
        {
            audio.seek_to(to);
            self.last_catch_up = Instant::now();
        }
        if (change.speed - self.speed).abs() > f64::EPSILON {
            audio.set_speed(change.speed);
            self.speed = change.speed;
        }
        if playing == self.state.paused {
            audio.set_pause(!playing);
        }
    }

    /// A host does what a guest asked for, while the music is what the party is playing.
    fn on_party_request(&mut self, event: &PartyEvent, cx: &mut Context<Self>) {
        let Some(audio) = &self.audio else { return };
        let is_music = self
            .party
            .as_ref()
            .is_some_and(|joined| matches!(joined.party.read(cx).item(), Some(Item::Music { .. })));
        if !is_music {
            return;
        }
        match event {
            PartyEvent::Play => audio.set_pause(false),
            PartyEvent::Pause => audio.set_pause(true),
            PartyEvent::Seek(to) => {
                let end = if self.state.duration > 0. {
                    self.state.duration
                } else {
                    *to
                };
                audio.seek_to(to.clamp(0., end));
            }
        }
        // The party hears of it at the next look.
        self.party_sync.told = None;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        // A guest asks the host, and the music follows when the host does it.
        if self.role(cx) == Role::Guest {
            if let Some(joined) = &self.party {
                let message = if self.state.paused {
                    Message::Play
                } else {
                    Message::Pause
                };
                joined.party.read(cx).ask(message);
            }
            return;
        }
        if let Some(audio) = &self.audio {
            audio.toggle_pause();
        }
        cx.notify();
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        if let Some(audio) = &self.audio
            && !self.state.paused
        {
            audio.set_pause(true);
            cx.notify();
        }
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        if self.blocked(cx) {
            return;
        }
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
        if self.blocked(cx) {
            return;
        }
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
        if self.blocked(cx) {
            return;
        }
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
        if self.blocked(cx) {
            return;
        }
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
        if self.blocked(cx) {
            return;
        }
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
        {
            let to = f64::from(fraction) * self.state.duration;
            if self.role(cx) == Role::Guest {
                if let Some(joined) = &self.party {
                    joined.party.read(cx).ask(Message::Seek { position: to });
                }
            } else if let Some(audio) = &self.audio {
                audio.seek_to(to);
                self.state.position = to;
            }
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
