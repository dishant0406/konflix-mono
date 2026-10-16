//! Usage statistics and error reports, through PostHog, only for a viewer who turned them on.
//!
//! Nothing runs until `Config::analytics` is true, and a build without a project token (a
//! local one) has no way to turn it on at all. What is sent is in the privacy policy: a few
//! named events (the app opened, playback started or failed, an update failed), handled errors,
//! and panics, each with the app version and the platform. The install is a random id made
//! when the viewer first opts in; the connection's address is not kept (`$geoip_disable`).
//!
//! Turning it off is immediate: every event passes `before_send`, which drops it while the
//! switch is off, panics included.

use akidev_core::config::Config;
use posthog_rs::{CaptureExceptionOptions, ClientOptionsBuilder, ErrorTrackingOptionsBuilder, Event};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The PostHog project token, put in by the release workflows. It only lets events in, so it is
/// not a secret.
const TOKEN: Option<&str> = option_env!("POSTHOG_TOKEN");
const HOST: &str = "https://us.i.posthog.com";

static ENABLED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

fn id() -> &'static Mutex<Option<String>> {
    static ID: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ID.get_or_init(|| Mutex::new(None))
}

/// Whether this build can report at all. Settings shows the switch only when it can.
pub fn available() -> bool {
    TOKEN.is_some_and(|token| !token.is_empty())
}

/// Turns reporting on in `config`, giving this install its id the first time.
pub fn enable(config: &mut Config) {
    config.analytics = true;
    if config.analytics_id.is_none() {
        config.analytics_id = Some(Event::new_anon("id").distinct_id().to_string());
    }
}

/// Makes the running app match `config`: starts the client the first time it is on, and keeps
/// the switch every event is checked against.
pub fn apply(config: &Config) {
    let on = available() && config.analytics && config.analytics_id.is_some();
    ENABLED.store(on, Ordering::SeqCst);
    if !on {
        return;
    }
    *id().lock().unwrap() = config.analytics_id.clone();
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(token) = TOKEN else { return };
    let options = ClientOptionsBuilder::default()
        .api_key(token.to_string())
        .host(HOST)
        .error_tracking(
            ErrorTrackingOptionsBuilder::default()
                .capture_panics(true)
                .build()
                .expect("error tracking options are valid"),
        )
        .before_send(|mut event: Event| {
            if !ENABLED.load(Ordering::SeqCst) {
                return None;
            }
            let _ = event.insert_prop("$geoip_disable", true);
            let _ = event.insert_prop("app_version", env!("CARGO_PKG_VERSION"));
            let _ = event.insert_prop("platform", std::env::consts::OS);
            let _ = event.insert_prop("arch", std::env::consts::ARCH);
            Some(event)
        })
        .build();
    let Ok(options) = options else { return };
    if let Err(error) = crate::runtime::block_on(posthog_rs::init_global(options)) {
        tracing::warn!("[analytics] could not start: {error}");
        return;
    }
    let pending: Vec<_> = PENDING.lock().unwrap().drain(..).collect();
    for (context, message) in pending {
        error(&context, &message);
    }
}

/// Records one named event. `properties` is a JSON object, or `Value::Null` for none.
pub fn capture(name: &str, properties: Value) {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(id) = id().lock().unwrap().clone() else { return };
    let mut event = Event::new(name, id.as_str());
    // One person per install is all that is needed; a profile for each would be noise.
    let _ = event.insert_prop("$process_person_profile", false);
    if let Value::Object(map) = properties {
        for (key, value) in map {
            let _ = event.insert_prop(key, value);
        }
    }
    posthog_rs::capture(event);
}

/// An error the app handled but that a developer should know about. `context` says where, and
/// groups the same failure together.
pub fn error(context: &str, message: &str) {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(id) = id().lock().unwrap().clone() else { return };
    let (context, failure) = (context.to_string(), Failure(message.to_string()));
    crate::runtime::spawn(async move {
        let options = CaptureExceptionOptions::new()
            .distinct_id(id)
            .fingerprint(context.clone());
        let Ok(options) = options.property("context", json!(context)) else { return };
        let _ = posthog_rs::capture_exception_with(&failure, options).await;
    });
}

/// Sends what is queued. Called as the app quits, and given only a moment.
pub async fn flush() {
    if STARTED.load(Ordering::SeqCst) {
        let _ = tokio::time::timeout(Duration::from_secs(2), posthog_rs::shutdown()).await;
    }
}

#[derive(Debug)]
struct Failure(String);

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

/// For a failure before the settings are loaded into the app (startup): remembered, and reported
/// by `error` once it can be.
pub fn report_later(context: &str, message: &str) {
    let mut pending = PENDING.lock().unwrap();
    pending.push((context.to_string(), message.to_string()));
}

static PENDING: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
