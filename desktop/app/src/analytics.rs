//! Usage statistics and error reports, through PostHog.
//!
//! Always on in a release build that carries a project token, and nowhere else (a local build has
//! no token, so it never reports). What is sent is in the privacy policy: screens opened, titles
//! played and whether that worked and how fast, handled errors and panics, each with the app
//! version and the platform. An install is a random id made at the first start; the connection's
//! address is not kept (`$geoip_disable`), and what was typed into a search is never sent.
//!
//! It must not cost the viewer anything. Capturing is a push onto a queue that the SDK's own
//! thread sends in batches, so nothing here waits for the network, the client starts in the
//! background, and the one wait (on quit) is capped at a second.

use akidev_core::config::Config;
use posthog_rs::{CaptureExceptionOptions, ClientOptionsBuilder, ErrorTrackingOptionsBuilder, Event};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// The PostHog project token, put in by the release workflows. It only lets events in, so it is
/// not a secret.
const TOKEN: Option<&str> = option_env!("POSTHOG_TOKEN");
const HOST: &str = "https://us.i.posthog.com";

/// Events asked for before the client is up (the first screen, mostly) wait here, so none is lost.
const WAITING_MAX: usize = 64;
/// A failure in a loop must not become a flood. Errors past this many in a run are not sent.
const ERRORS_MAX: u32 = 25;

enum State {
    /// Not started, or no token: events are dropped.
    Off,
    Starting(Vec<Event>),
    Ready,
}

static STATE: Mutex<State> = Mutex::new(State::Off);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static INSTALL: OnceLock<String> = OnceLock::new();
static OPENED: OnceLock<Instant> = OnceLock::new();

/// A new random id, for an install or a session.
pub fn new_id() -> String {
    Event::new_anon("id").distinct_id().to_string()
}

/// Starts reporting under `install`. Returns at once: the client is made on the runtime, and
/// what was asked for meanwhile is sent when it is up.
pub fn start(install: String, first_run: bool) {
    let Some(token) = TOKEN.filter(|token| !token.is_empty()) else {
        return;
    };
    if INSTALL.set(install).is_err() {
        return;
    }
    let _ = OPENED.set(Instant::now());
    *STATE.lock().unwrap() = State::Starting(Vec::new());
    let session = new_id();
    crate::runtime::spawn(async move {
        let options = ClientOptionsBuilder::default()
            .api_key(token.to_string())
            .host(HOST)
            .error_tracking(
                ErrorTrackingOptionsBuilder::default()
                    .capture_panics(true)
                    .build()
                    .expect("error tracking options are valid"),
            )
            .before_send(move |mut event: Event| {
                let _ = event.insert_prop("$geoip_disable", true);
                let _ = event.insert_prop("$session_id", session.as_str());
                let _ = event.insert_prop("app_version", env!("CARGO_PKG_VERSION"));
                let _ = event.insert_prop("platform", std::env::consts::OS);
                let _ = event.insert_prop("arch", std::env::consts::ARCH);
                Some(event)
            })
            .build();
        let started = match options {
            Ok(options) => posthog_rs::init_global(options).await.is_ok(),
            Err(_) => false,
        };
        let waiting = {
            let mut state = STATE.lock().unwrap();
            let waiting = match std::mem::replace(&mut *state, State::Off) {
                State::Starting(events) => events,
                _ => Vec::new(),
            };
            if started {
                *state = State::Ready;
            }
            waiting
        };
        if !started {
            tracing::warn!("[analytics] could not start");
            return;
        }
        capture("app_opened", json!({ "first_run": first_run }));
        for event in waiting {
            posthog_rs::capture(event);
        }
    });
}

/// Gives this install its id the first time, and returns it with whether it was the first.
pub fn install_id(config: &mut Config) -> (String, bool) {
    match &config.analytics_id {
        Some(id) => (id.clone(), false),
        None => {
            let id = new_id();
            config.analytics_id = Some(id.clone());
            (id, true)
        }
    }
}

/// Records one named event. `properties` is a JSON object, or `Value::Null` for none.
pub fn capture(name: &str, properties: Value) {
    let Some(id) = INSTALL.get() else { return };
    let mut event = Event::new(name, id.as_str());
    // One person per install is all that is needed; a profile for each would be noise.
    let _ = event.insert_prop("$process_person_profile", false);
    if let Value::Object(map) = properties {
        for (key, value) in map {
            let _ = event.insert_prop(key, value);
        }
    }
    let mut state = STATE.lock().unwrap();
    match &mut *state {
        State::Off => {}
        State::Starting(waiting) => {
            if waiting.len() < WAITING_MAX {
                waiting.push(event);
            }
        }
        State::Ready => {
            drop(state);
            posthog_rs::capture(event);
        }
    }
}

/// A screen the viewer opened. PostHog treats `$screen` as a page view.
pub fn screen(name: &str, properties: Value) {
    let mut properties = properties;
    if let Value::Object(map) = &mut properties {
        map.insert("$screen_name".into(), json!(name));
    } else {
        properties = json!({ "$screen_name": name });
    }
    capture("$screen", properties);
}

/// An error the app handled but that a developer should know about. `context` says where, and
/// groups the same failure together.
pub fn error(context: &str, message: &str) {
    if !matches!(*STATE.lock().unwrap(), State::Ready | State::Starting(_)) {
        return;
    }
    let Some(id) = INSTALL.get().cloned() else { return };
    if ERRORS.fetch_add(1, Ordering::Relaxed) >= ERRORS_MAX {
        return;
    }
    let (context, failure) = (context.to_string(), Failure(message.to_string()));
    crate::runtime::spawn(async move {
        // The client may still be starting; a moment's wait beats losing the first error.
        for _ in 0..20 {
            if matches!(*STATE.lock().unwrap(), State::Ready) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let options = CaptureExceptionOptions::new()
            .distinct_id(id)
            .fingerprint(context.clone());
        let Ok(options) = options.property("context", json!(context)) else { return };
        let _ = posthog_rs::capture_exception_with(&failure, options).await;
    });
}

/// Records how long the app was open, then sends what is queued. Called as the app quits, and
/// given only a moment.
pub async fn finish() {
    if !matches!(*STATE.lock().unwrap(), State::Ready) {
        return;
    }
    let seconds = OPENED.get().map_or(0, |opened| opened.elapsed().as_secs());
    capture("app_closed", json!({ "session_seconds": seconds }));
    let _ = tokio::time::timeout(Duration::from_secs(1), posthog_rs::shutdown()).await;
}

#[derive(Debug)]
struct Failure(String);

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

/// Sends every `tracing::error!` the app itself logs, so a failure that is only logged is not
/// invisible. Other crates' errors, and the SDK's own, are left out (the SDK reporting its own
/// trouble would be a loop).
pub struct ErrorLogs;

impl<S: tracing::Subscriber> Layer<S> for ErrorLogs {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() != tracing::Level::ERROR || !meta.target().starts_with("akidev") {
            return;
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        error(meta.target(), &message.0);
    }
}

struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

/// The screen a route is, with what identifies it (a title's id; for a search only how long
/// the words were, never the words).
pub fn screen_of(route: &crate::route::Route) {
    use crate::route::Route;
    match route {
        Route::Home => screen("home", Value::Null),
        Route::Browse { kind, term } if term.is_empty() => screen(
            "browse",
            json!({ "kind": kind.map(|kind| format!("{kind:?}").to_lowercase()) }),
        ),
        Route::Browse { kind, term } => screen(
            "search",
            json!({
                "kind": kind.map(|kind| format!("{kind:?}").to_lowercase()),
                "term_length": term.chars().count(),
            }),
        ),
        Route::Title { id } => screen("title", json!({ "title_id": id })),
        Route::Watch { id } => screen("watch", json!({ "title_id": id })),
        Route::Settings => screen("settings", Value::Null),
    }
}
