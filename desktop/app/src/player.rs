//! Plays a video with libmpv and hands its frames to GPUI.
//!
//! mpv decodes and keeps audio in sync, and renders each frame into memory. GPUI has no
//! video element, so every frame becomes an image the UI draws. One thread owns the mpv
//! render context, because mpv requires every render call to come from the same thread.

use futures::channel::mpsc;
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use libmpv2_sys::*;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// What the controls need to know about playback.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    /// Waiting for the download to catch up.
    pub buffering: bool,
    /// A picture has been decoded, so frames show video and not black.
    pub video_ready: bool,
    /// Playback reached the end of the file.
    pub ended: bool,
    /// 0 to 100.
    pub volume: f64,
    pub muted: bool,
    /// The subtitle and audio track in use, as a short label ("ENG", "Off").
    pub subtitle: String,
    pub audio: String,
    /// The subtitle track in use, if any (mpv's track id).
    pub sid: Option<i64>,
    /// Every subtitle track mpv knows: the file's own and any loaded from outside.
    pub subtitle_list: Vec<SubtitleTrack>,
    /// How many audio tracks the file has, so the controls only offer a choice that exists.
    pub audio_tracks: u32,
    pub error: Option<String>,
}

/// One subtitle track of the loaded video.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleTrack {
    pub id: i64,
    /// As the file spells it ("eng"), empty when it does not say.
    pub lang: String,
    pub title: String,
    /// Loaded by the app from a downloaded file, not part of the video.
    pub external: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            position: 0.,
            duration: 0.,
            paused: false,
            buffering: false,
            video_ready: false,
            ended: false,
            volume: 100.,
            muted: false,
            subtitle: "Off".into(),
            audio: String::new(),
            sid: None,
            subtitle_list: Vec::new(),
            audio_tracks: 0,
            error: None,
        }
    }
}

struct Mpv(*mut mpv_handle);

// mpv's client API is thread-safe.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Drop for Mpv {
    fn drop(&mut self) {
        // By now the render thread has freed its render context.
        unsafe { mpv_terminate_destroy(self.0) };
    }
}

/// Shared between the UI, the render thread and mpv's update callback.
struct Shared {
    state: Mutex<PlayerState>,
    frame: Mutex<Option<Arc<RenderImage>>>,
    /// Target frame size, packed as width << 32 | height.
    size: AtomicU64,
    stop: AtomicBool,
    frame_ready: AtomicBool,
}

/// What mpv's update callback needs: the flag to raise and the player to wake.
struct UpdateTarget {
    shared: Arc<Shared>,
    mpv: *mut mpv_handle,
}

pub struct Player {
    mpv: Arc<Mpv>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

const TIME_POS: u64 = 1;
const DURATION: u64 = 2;
const PAUSE: u64 = 3;
const BUFFERING: u64 = 4;
const VIDEO_READY: u64 = 5;
const ENDED: u64 = 6;
const VOLUME: u64 = 7;
const MUTE: u64 = 8;
const SUBTITLE_TRACK: u64 = 9;
const AUDIO_TRACK: u64 = 10;
const TRACK_COUNT: u64 = 11;

/// Frames are never larger than this, whatever the window: software rendering costs CPU
/// in proportion to its area, and GPUI scales the image up to the window.
pub const MAX_WIDTH: u32 = 1280;

impl Player {
    /// Starts playing `url`, from `start_at` seconds in if given, at the given volume.
    /// `frames` is told when a new frame can be taken.
    pub fn start(
        url: &str,
        start_at: Option<f64>,
        volume: f64,
        muted: bool,
        frames: mpsc::Sender<()>,
    ) -> Result<Player, String> {
        unsafe {
            let handle = mpv_create();
            if handle.is_null() {
                return Err("could not create the video player".into());
            }
            let mpv = Arc::new(Mpv(handle));
            for (name, value) in [
                ("vo", "libmpv"),
                // Decodes on the GPU where it can and copies the picture back for rendering.
                ("hwdec", "auto-copy-safe"),
                ("keep-open", "yes"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                // mpv's own Lua scripts (stats, console, the youtube-dl hook, ...) are not used,
                // since the app draws its own controls. They run on LuaJIT, which writes machine
                // code at run time, and a notarised app (hardened runtime) is killed for that.
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("load-stats-overlay", "no"),
                ("load-console", "no"),
                ("load-auto-profiles", "no"),
                ("load-select", "no"),
                ("load-commands", "no"),
                ("load-context-menu", "no"),
                ("load-positioning", "no"),
                ("config", "no"),
                ("sub-auto", "no"),
                ("slang", "eng,en"),
                // How far ahead mpv reads. The torrent engine fetches only what is read, so
                // this is also how much is downloaded ahead of the playhead.
                ("cache", "yes"),
                ("cache-secs", "40"),
                ("demuxer-max-bytes", "32MiB"),
                ("demuxer-max-back-bytes", "8MiB"),
                ("network-timeout", "60"),
                ("volume-max", "100"),
            ] {
                set_option(&mpv, name, value);
            }
            set_option(&mpv, "volume", &volume.to_string());
            set_option(&mpv, "mute", if muted { "yes" } else { "no" });
            if mpv_initialize(handle) < 0 {
                return Err("could not start the video player".into());
            }
            for (id, name, format) in [
                (TIME_POS, "time-pos", mpv_format_MPV_FORMAT_DOUBLE),
                (DURATION, "duration", mpv_format_MPV_FORMAT_DOUBLE),
                (PAUSE, "pause", mpv_format_MPV_FORMAT_FLAG),
                (BUFFERING, "paused-for-cache", mpv_format_MPV_FORMAT_FLAG),
                (VIDEO_READY, "vo-configured", mpv_format_MPV_FORMAT_FLAG),
                (ENDED, "eof-reached", mpv_format_MPV_FORMAT_FLAG),
                (VOLUME, "volume", mpv_format_MPV_FORMAT_DOUBLE),
                (MUTE, "mute", mpv_format_MPV_FORMAT_FLAG),
                (SUBTITLE_TRACK, "sid", mpv_format_MPV_FORMAT_INT64),
                (AUDIO_TRACK, "aid", mpv_format_MPV_FORMAT_INT64),
                // A subtitle loaded after the file started shows up here, not at file load.
                (TRACK_COUNT, "track-list/count", mpv_format_MPV_FORMAT_INT64),
            ] {
                let name = CString::new(name).expect("static name");
                mpv_observe_property(handle, id, name.as_ptr(), format);
            }

            let shared = Arc::new(Shared {
                state: Mutex::new(PlayerState {
                    volume,
                    muted,
                    ..PlayerState::default()
                }),
                frame: Mutex::new(None),
                size: AtomicU64::new(pack(MAX_WIDTH, MAX_WIDTH * 9 / 16)),
                stop: AtomicBool::new(false),
                frame_ready: AtomicBool::new(false),
            });

            let thread = {
                let (mpv, shared) = (mpv.clone(), shared.clone());
                std::thread::Builder::new()
                    .name("mpv-render".into())
                    .spawn(move || render_loop(&mpv, shared, frames))
                    .map_err(|error| error.to_string())?
            };

            let player = Player {
                mpv,
                shared,
                thread: Some(thread),
            };
            match start_at.filter(|seconds| *seconds > 0.) {
                Some(seconds) => {
                    player.command(&["loadfile", url, "replace", "0", &format!("start={seconds}")])
                }
                None => player.command(&["loadfile", url]),
            }
            Ok(player)
        }
    }

    pub fn state(&self) -> PlayerState {
        self.shared.state.lock().unwrap().clone()
    }

    /// The newest frame, if one has arrived since the last call.
    pub fn take_frame(&self) -> Option<Arc<RenderImage>> {
        self.shared.frame.lock().unwrap().take()
    }

    /// Sets the size frames are rendered at. The video is letterboxed inside it.
    pub fn set_frame_size(&self, width: u32, height: u32) {
        let width = width.clamp(160, MAX_WIDTH) & !1;
        let height = height.clamp(90, MAX_WIDTH) & !1;
        self.shared
            .size
            .store(pack(width, height), Ordering::SeqCst);
    }

    pub fn toggle_pause(&self) {
        self.command(&["cycle", "pause"]);
    }

    pub fn set_pause(&self, paused: bool) {
        self.command(&["set", "pause", if paused { "yes" } else { "no" }]);
    }

    /// Playback speed, 1.0 being normal. A watch party closes a small gap by running a little
    /// faster or slower.
    pub fn set_speed(&self, speed: f64) {
        self.command(&["set", "speed", &speed.to_string()]);
    }

    /// Jumps to an absolute position, in seconds.
    pub fn seek_to(&self, seconds: f64) {
        self.command(&["seek", &seconds.to_string(), "absolute"]);
    }

    pub fn seek_by(&self, seconds: f64) {
        self.command(&["seek", &seconds.to_string(), "relative"]);
    }

    pub fn toggle_mute(&self) {
        self.command(&["cycle", "mute"]);
    }

    pub fn change_volume(&self, delta: f64) {
        self.command(&["add", "volume", &delta.to_string()]);
    }

    /// Switches to the next subtitle track, or off after the last.
    pub fn cycle_subtitles(&self) {
        self.command(&["cycle", "sub"]);
    }

    /// Shows the subtitle track with this id, or none.
    pub fn select_subtitle(&self, id: Option<i64>) {
        match id {
            Some(id) => self.command(&["set", "sid", &id.to_string()]),
            None => self.command(&["set", "sid", "no"]),
        }
    }

    /// Loads a subtitle file and shows it. `title` and `lang` are what mpv reports for it.
    pub fn add_subtitle(&self, path: &str, title: &str, lang: &str) {
        self.command(&["sub-add", path, "select", title, lang]);
    }

    /// Shifts the subtitles by `seconds` from where they are now: later when positive.
    pub fn shift_subtitles(&self, seconds: f64) {
        self.command(&["add", "sub-delay", &seconds.to_string()]);
    }

    pub fn remove_subtitle(&self, id: i64) {
        self.command(&["sub-remove", &id.to_string()]);
    }

    pub fn cycle_audio(&self) {
        self.command(&["cycle", "audio"]);
    }

    fn command(&self, args: &[&str]) {
        let args: Vec<CString> = args
            .iter()
            .filter_map(|arg| CString::new(*arg).ok())
            .collect();
        let mut pointers: Vec<*const c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(std::ptr::null());
        unsafe {
            mpv_command_async(self.mpv.0, 0, pointers.as_mut_ptr());
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        unsafe { mpv_wakeup(self.mpv.0) };
        // Stopping can wait on a network read, so it happens off the UI thread. The mpv handle
        // is destroyed when the last reference, held here, goes.
        let (thread, mpv) = (self.thread.take(), self.mpv.clone());
        std::thread::spawn(move || {
            if let Some(thread) = thread {
                let _ = thread.join();
            }
            drop(mpv);
        });
    }
}

fn pack(width: u32, height: u32) -> u64 {
    u64::from(width) << 32 | u64::from(height)
}

fn set_option(mpv: &Mpv, name: &str, value: &str) {
    let (c_name, c_value) = (
        CString::new(name).expect("static name"),
        CString::new(value).expect("static value"),
    );
    if unsafe { mpv_set_option_string(mpv.0, c_name.as_ptr(), c_value.as_ptr()) } < 0 {
        tracing::warn!("[player] mpv rejected the option {name}={value}");
    }
}

fn property_string(mpv: &Mpv, name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    unsafe {
        let raw = mpv_get_property_string(mpv.0, name.as_ptr());
        if raw.is_null() {
            return None;
        }
        let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
        mpv_free(raw.cast());
        Some(text)
    }
}

/// The subtitle tracks of the loaded file, and how many audio tracks it has.
fn read_tracks(mpv: &Mpv) -> (Vec<SubtitleTrack>, u32) {
    let count: u32 = property_string(mpv, "track-list/count")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let (mut subtitles, mut audio) = (Vec::new(), 0);
    for index in 0..count {
        let field = |name: &str| property_string(mpv, &format!("track-list/{index}/{name}"));
        match field("type").as_deref() {
            Some("sub") => {
                let Some(id) = field("id").and_then(|id| id.parse().ok()) else {
                    continue;
                };
                subtitles.push(SubtitleTrack {
                    id,
                    lang: field("lang").unwrap_or_default(),
                    title: field("title").unwrap_or_default(),
                    external: field("external").as_deref() == Some("yes"),
                });
            }
            Some("audio") => audio += 1,
            _ => {}
        }
    }
    (subtitles, audio)
}

/// "eng" becomes "ENG"; a track with a title and no language uses its title.
fn track_label(mpv: &Mpv, kind: &str) -> Option<String> {
    let language = property_string(mpv, &format!("current-tracks/{kind}/lang"));
    let title = property_string(mpv, &format!("current-tracks/{kind}/title"));
    language.map(|lang| lang.to_uppercase()).or(title)
}

/// Rendering and events, on the one thread mpv allows.
fn render_loop(mpv: &Mpv, shared: Arc<Shared>, mut frames: mpsc::Sender<()>) {
    unsafe {
        let mut context: *mut mpv_render_context = std::ptr::null_mut();
        let mut params = [
            mpv_render_param {
                type_: mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                data: MPV_RENDER_API_TYPE_SW.as_ptr() as *mut c_void,
            },
            mpv_render_param {
                type_: 0,
                data: std::ptr::null_mut(),
            },
        ];
        if mpv_render_context_create(&mut context, mpv.0, params.as_mut_ptr()) < 0 {
            shared.state.lock().unwrap().error = Some("could not start video rendering".into());
            return;
        }
        // The callback only raises a flag and wakes this loop: mpv forbids doing real work in it.
        let target = Box::into_raw(Box::new(UpdateTarget {
            shared: shared.clone(),
            mpv: mpv.0,
        }));
        mpv_render_context_set_update_callback(context, Some(on_update), target.cast());

        let mut scratch = Scratch::default();
        while !shared.stop.load(Ordering::SeqCst) {
            handle_events(mpv, &shared);
            // Until a picture is decoded mpv would render black, so the flag waits.
            let ready = shared.state.lock().unwrap().video_ready;
            if ready
                && shared.frame_ready.swap(false, Ordering::SeqCst)
                && mpv_render_context_update(context)
                    & u64::from(mpv_render_update_flag_MPV_RENDER_UPDATE_FRAME)
                    != 0
            {
                let size = shared.size.load(Ordering::SeqCst);
                let image = scratch.render(context, (size >> 32) as u32, size as u32);
                *shared.frame.lock().unwrap() = Some(image);
                let _ = frames.try_send(());
            }
        }
        // mpv only promises no further callbacks once it is told to stop making them.
        mpv_render_context_set_update_callback(context, None, std::ptr::null_mut());
        mpv_render_context_free(context);
        drop(Box::from_raw(target));
    }
}

unsafe extern "C" fn on_update(target: *mut c_void) {
    // SAFETY: the target outlives the render context, which is freed before it.
    let target = unsafe { &*(target as *const UpdateTarget) };
    target.shared.frame_ready.store(true, Ordering::SeqCst);
    unsafe { mpv_wakeup(target.mpv) };
}

unsafe fn handle_events(mpv: &Mpv, shared: &Shared) {
    loop {
        // Waits briefly: the update callback wakes it as soon as a frame is due.
        let event = unsafe { &*mpv_wait_event(mpv.0, 0.05) };
        match event.event_id {
            id if id == mpv_event_id_MPV_EVENT_NONE => return,
            id if id == mpv_event_id_MPV_EVENT_SHUTDOWN => return,
            id if id == mpv_event_id_MPV_EVENT_PROPERTY_CHANGE => {
                let property = unsafe { &*(event.data as *const mpv_event_property) };
                if property.data.is_null() {
                    // A track property with no value means no track is selected.
                    match event.reply_userdata {
                        SUBTITLE_TRACK => {
                            let mut state = shared.state.lock().unwrap();
                            state.subtitle = "Off".into();
                            state.sid = None;
                        }
                        AUDIO_TRACK => shared.state.lock().unwrap().audio.clear(),
                        _ => {}
                    }
                    continue;
                }
                // Looked up before the state is locked: it asks mpv, which can take a moment.
                let subtitle = (event.reply_userdata == SUBTITLE_TRACK)
                    .then(|| track_label(mpv, "sub").unwrap_or_else(|| "Track".into()));
                let audio = (event.reply_userdata == AUDIO_TRACK)
                    .then(|| track_label(mpv, "audio").unwrap_or_else(|| "Track".into()));
                let tracks = (event.reply_userdata == TRACK_COUNT).then(|| read_tracks(mpv));
                let mut state = shared.state.lock().unwrap();
                unsafe {
                    match event.reply_userdata {
                        TIME_POS => state.position = *(property.data as *const f64),
                        DURATION => state.duration = *(property.data as *const f64),
                        PAUSE => state.paused = *(property.data as *const i32) != 0,
                        BUFFERING => state.buffering = *(property.data as *const i32) != 0,
                        VIDEO_READY => state.video_ready = *(property.data as *const i32) != 0,
                        ENDED => state.ended = *(property.data as *const i32) != 0,
                        VOLUME => state.volume = *(property.data as *const f64),
                        MUTE => state.muted = *(property.data as *const i32) != 0,
                        SUBTITLE_TRACK => state.sid = Some(*(property.data as *const i64)),
                        _ => {}
                    }
                }
                if let Some(subtitle) = subtitle {
                    state.subtitle = subtitle;
                }
                if let Some(audio) = audio {
                    state.audio = audio;
                }
                if let Some((subtitles, audio)) = tracks {
                    state.subtitle_list = subtitles;
                    state.audio_tracks = audio;
                }
            }
            id if id == mpv_event_id_MPV_EVENT_FILE_LOADED => {
                let (subtitles, audio) = read_tracks(mpv);
                let mut state = shared.state.lock().unwrap();
                state.subtitle_list = subtitles;
                state.audio_tracks = audio;
            }
            id if id == mpv_event_id_MPV_EVENT_START_FILE => {
                let mut state = shared.state.lock().unwrap();
                state.ended = false;
                state.error = None;
            }
            id if id == mpv_event_id_MPV_EVENT_END_FILE => {
                let end = unsafe { &*(event.data as *const mpv_event_end_file) };
                if end.reason == mpv_end_file_reason_MPV_END_FILE_REASON_ERROR {
                    let message = unsafe { CStr::from_ptr(mpv_error_string(end.error)) };
                    shared.state.lock().unwrap().error =
                        Some(message.to_string_lossy().into_owned());
                }
            }
            _ => {}
        }
    }
}

/// The buffer mpv renders into. Its start must be 64-byte aligned for mpv's fast path.
#[derive(Default)]
struct Scratch {
    memory: Vec<u8>,
}

impl Scratch {
    unsafe fn render(
        &mut self,
        context: *mut mpv_render_context,
        width: u32,
        height: u32,
    ) -> Arc<RenderImage> {
        let stride = width as usize * 4;
        let length = stride * height as usize;
        if self.memory.len() < length + 64 {
            self.memory = vec![0; length + 64];
        }
        let offset = self.memory.as_ptr().align_offset(64);
        let target = &mut self.memory[offset..offset + length];

        let mut size = [width as i32, height as i32];
        let format = CString::new("bgr0").expect("static format");
        let mut stride_parameter = stride;
        let mut params = [
            mpv_render_param {
                type_: mpv_render_param_type_MPV_RENDER_PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            mpv_render_param {
                type_: mpv_render_param_type_MPV_RENDER_PARAM_SW_FORMAT,
                data: format.as_ptr() as *mut c_void,
            },
            mpv_render_param {
                type_: mpv_render_param_type_MPV_RENDER_PARAM_SW_STRIDE,
                data: (&mut stride_parameter as *mut usize).cast(),
            },
            mpv_render_param {
                type_: mpv_render_param_type_MPV_RENDER_PARAM_SW_POINTER,
                data: target.as_mut_ptr().cast(),
            },
            mpv_render_param {
                type_: 0,
                data: std::ptr::null_mut(),
            },
        ];
        unsafe { mpv_render_context_render(context, params.as_mut_ptr()) };

        // "bgr0" leaves the fourth byte undefined, and GPUI reads it as opacity.
        let mut pixels = target.to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        let image =
            RgbaImage::from_raw(width, height, pixels).expect("buffer holds width * height pixels");
        Arc::new(RenderImage::new(smallvec::smallvec![Frame::new(image)]))
    }
}
