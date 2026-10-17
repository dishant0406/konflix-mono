//! Plays music with libmpv: no window, no picture, just the sound.
//!
//! One mpv instance lives as long as the player and is pointed at each track in turn, so a track
//! change is a `loadfile`, not a new engine. A thread reads mpv's events into a state the UI polls.

use libmpv2_sys::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// What the controls need to know about playback.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    /// Waiting for the network to catch up.
    pub buffering: bool,
    /// The track played to its end.
    pub ended: bool,
    pub error: Option<String>,
}

struct Mpv(*mut mpv_handle);

// mpv's client API is thread-safe.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe { mpv_terminate_destroy(self.0) };
    }
}

struct Shared {
    state: Mutex<AudioState>,
    stop: AtomicBool,
}

pub struct AudioPlayer {
    mpv: Arc<Mpv>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

const TIME_POS: u64 = 1;
const DURATION: u64 = 2;
const PAUSE: u64 = 3;
const BUFFERING: u64 = 4;
const ENDED: u64 = 5;

impl AudioPlayer {
    pub fn new(volume: f64) -> Result<AudioPlayer, String> {
        unsafe {
            let handle = mpv_create();
            if handle.is_null() {
                return Err("could not create the audio player".into());
            }
            let mpv = Arc::new(Mpv(handle));
            for (name, value) in [
                ("vo", "null"),
                ("video", "no"),
                ("audio-display", "no"),
                // Keeps the last track loaded at its end, so the end is seen as `eof-reached`.
                ("keep-open", "yes"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                // As with the video player: no Lua scripts (a notarised app is killed for the JIT)
                // and no help from youtube-dl, since the address is already known.
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
                ("cache", "yes"),
                ("cache-secs", "120"),
                ("network-timeout", "30"),
                ("volume-max", "100"),
            ] {
                set_option(&mpv, name, value);
            }
            set_option(&mpv, "volume", &volume.to_string());
            if mpv_initialize(handle) < 0 {
                return Err("could not start the audio player".into());
            }
            for (id, name, format) in [
                (TIME_POS, "time-pos", mpv_format_MPV_FORMAT_DOUBLE),
                (DURATION, "duration", mpv_format_MPV_FORMAT_DOUBLE),
                (PAUSE, "pause", mpv_format_MPV_FORMAT_FLAG),
                (BUFFERING, "paused-for-cache", mpv_format_MPV_FORMAT_FLAG),
                (ENDED, "eof-reached", mpv_format_MPV_FORMAT_FLAG),
            ] {
                let name = CString::new(name).expect("static name");
                mpv_observe_property(handle, id, name.as_ptr(), format);
            }
            let shared = Arc::new(Shared {
                state: Mutex::new(AudioState::default()),
                stop: AtomicBool::new(false),
            });
            let thread = {
                let (mpv, shared) = (mpv.clone(), shared.clone());
                std::thread::Builder::new()
                    .name("mpv-audio".into())
                    .spawn(move || event_loop(&mpv, &shared))
                    .map_err(|error| error.to_string())?
            };
            Ok(AudioPlayer {
                mpv,
                shared,
                thread: Some(thread),
            })
        }
    }

    /// Starts playing `url`. `user_agent` is what YouTube's address was found with and expects.
    pub fn load(&self, url: &str, user_agent: Option<&str>) {
        {
            let mut state = self.shared.state.lock().unwrap();
            *state = AudioState {
                buffering: true,
                ..AudioState::default()
            };
        }
        if let Some(agent) = user_agent {
            self.set_property("user-agent", agent);
        }
        self.set_property("pause", "no");
        self.command(&["loadfile", url, "replace"]);
    }

    pub fn state(&self) -> AudioState {
        self.shared.state.lock().unwrap().clone()
    }

    pub fn toggle_pause(&self) {
        self.command(&["cycle", "pause"]);
    }

    pub fn seek_to(&self, seconds: f64) {
        self.command(&["seek", &seconds.to_string(), "absolute"]);
    }

    pub fn set_pause(&self, paused: bool) {
        self.set_property("pause", if paused { "yes" } else { "no" });
    }

    /// Playback speed, 1.0 being normal. A watch party closes a small gap by running a little
    /// faster or slower.
    pub fn set_speed(&self, speed: f64) {
        self.set_property("speed", &speed.to_string());
    }

    /// 0 to 100.
    pub fn set_volume(&self, volume: f64) {
        self.set_property("volume", &volume.clamp(0., 100.).to_string());
    }

    /// Plays on, when the track was paused (at its end, say).
    pub fn resume(&self) {
        self.set_property("pause", "no");
    }

    pub fn stop(&self) {
        self.command(&["stop"]);
    }

    fn set_property(&self, name: &str, value: &str) {
        if let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) {
            unsafe { mpv_set_property_string(self.mpv.0, name.as_ptr(), value.as_ptr()) };
        }
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

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        unsafe { mpv_wakeup(self.mpv.0) };
        // Stopping can wait on a network read, so it happens off the UI thread.
        let (thread, mpv) = (self.thread.take(), self.mpv.clone());
        std::thread::spawn(move || {
            if let Some(thread) = thread {
                let _ = thread.join();
            }
            drop(mpv);
        });
    }
}

fn set_option(mpv: &Mpv, name: &str, value: &str) {
    let (c_name, c_value) = (
        CString::new(name).expect("static name"),
        CString::new(value).expect("static value"),
    );
    if unsafe { mpv_set_option_string(mpv.0, c_name.as_ptr(), c_value.as_ptr()) } < 0 {
        tracing::warn!("[audio] mpv rejected the option {name}={value}");
    }
}

fn event_loop(mpv: &Mpv, shared: &Shared) {
    while !shared.stop.load(Ordering::SeqCst) {
        let event = unsafe { &*mpv_wait_event(mpv.0, 0.25) };
        match event.event_id {
            id if id == mpv_event_id_MPV_EVENT_SHUTDOWN => return,
            id if id == mpv_event_id_MPV_EVENT_PROPERTY_CHANGE => {
                let property = unsafe { &*(event.data as *const mpv_event_property) };
                if property.data.is_null() {
                    continue;
                }
                let mut state = shared.state.lock().unwrap();
                unsafe {
                    match event.reply_userdata {
                        TIME_POS => state.position = *(property.data as *const f64),
                        DURATION => state.duration = *(property.data as *const f64),
                        PAUSE => state.paused = *(property.data as *const i32) != 0,
                        BUFFERING => state.buffering = *(property.data as *const i32) != 0,
                        ENDED => state.ended = *(property.data as *const i32) != 0,
                        _ => {}
                    }
                }
            }
            id if id == mpv_event_id_MPV_EVENT_START_FILE => {
                let mut state = shared.state.lock().unwrap();
                state.ended = false;
                state.error = None;
            }
            id if id == mpv_event_id_MPV_EVENT_FILE_LOADED => {
                shared.state.lock().unwrap().buffering = false;
            }
            id if id == mpv_event_id_MPV_EVENT_END_FILE => {
                let end = unsafe { &*(event.data as *const mpv_event_end_file) };
                if end.reason == mpv_end_file_reason_MPV_END_FILE_REASON_ERROR {
                    let message = unsafe { CStr::from_ptr(mpv_error_string(end.error)) };
                    let mut state = shared.state.lock().unwrap();
                    state.buffering = false;
                    state.error = Some(message.to_string_lossy().into_owned());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live: needs the network and yt-dlp, and plays (silently) for a few seconds.
    // `cargo test -p akidev audio -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn plays_a_track_found_by_ytdlp() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let music = akidev_core::music::Music::new("yt-dlp");
        let found = runtime.block_on(music.resolve("dQw4w9WgXcQ")).unwrap();
        let agent = found
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
            .map(|(_, value)| value.clone());
        let player = AudioPlayer::new(0.).unwrap();
        player.load(&found.url, agent.as_deref());
        std::thread::sleep(std::time::Duration::from_secs(6));
        let state = player.state();
        println!("{state:?}");
        assert!(state.error.is_none(), "{:?}", state.error);
        assert!(state.duration > 60., "no duration: {state:?}");
        assert!(state.position > 1., "did not advance: {state:?}");
        player.toggle_pause();
        player.seek_to(30.);
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(player.state().paused);
        assert!(player.state().position >= 29.);
    }
}
