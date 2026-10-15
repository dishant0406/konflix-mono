//! Asking a provider: filling in a request, sending it and checking what comes back.

use super::disk;
use crate::cache::{EMPTY_TTL, peek, put};
use crate::calendar::now_ms;
use crate::definition::{Check, Method, ProviderDef, ProviderRequest};
use crate::expression::{Env, Row, matches, scalar, select};
use crate::http::{client, read_capped};
use crate::model::{Kind, Sort};
use crate::sources;
use crate::template::Fill;
use futures::FutureExt as _;
use futures::future::{BoxFuture, Shared};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// A full day of a TV schedule is tens of megabytes of JSON; nothing legitimate is larger.
const MAX_BYTES: usize = 32 << 20;

/// What a request is about: the values its holes are filled with.
pub struct Asking<'a> {
    pub def: &'a ProviderDef,
    pub kind: Kind,
    pub sort: Option<Sort>,
    pub id: Option<&'a str>,
    pub term: Option<&'a str>,
    pub page: usize,
    /// How many rows the service is asked for.
    pub per_page: usize,
    pub skip: usize,
    pub genre: Option<&'a str>,
    pub year: Option<i32>,
}

impl<'a> Asking<'a> {
    pub fn new(def: &'a ProviderDef, kind: Kind) -> Asking<'a> {
        Asking {
            def,
            kind,
            sort: None,
            id: None,
            term: None,
            page: 1,
            per_page: 0,
            skip: 0,
            genre: None,
            year: None,
        }
    }

    fn hole(&self, name: &str) -> Option<String> {
        let def = self.def;
        match name {
            "base" => Some(def.base_url.clone()),
            "type" => def.types.get(&self.kind).cloned(),
            "sort" => self
                .sort
                .and_then(|sort| def.sorts.get(sort.as_str()))
                .or_else(|| def.sorts.get("default"))
                .cloned(),
            "id" => self.id.map(str::to_string),
            "term" => self
                .term
                .map(str::to_string)
                .filter(|term| !term.is_empty()),
            "page" => Some(self.page.to_string()),
            "per_page" => (self.per_page > 0).then(|| self.per_page.to_string()),
            "skip" => (self.skip > 0).then(|| self.skip.to_string()),
            "genre" => self.genre.map(str::to_string),
            "genre_lower" => self.genre.map(str::to_lowercase),
            "year" => self.year.map(|year| year.to_string()),
            _ => None,
        }
    }

    /// The request with its holes filled, or `None` when it needs something that is missing.
    pub fn prepare(&self, request: &ProviderRequest) -> Option<Prepared> {
        let hole = |name: &str| self.hole(name);
        let fill = Fill {
            kind: self.kind,
            sort: self.sort.map(Sort::as_str),
            hole: &hole,
        };
        let mut query = Vec::new();
        for (name, value) in &request.query {
            let (name, optional) = match name.strip_suffix('?') {
                Some(name) => (name, true),
                None => (name.as_str(), false),
            };
            match fill.value(value) {
                Some(Value::String(text)) => query.push((name.to_string(), text)),
                Some(other) => query.push((name.to_string(), other.to_string())),
                None if optional => {}
                None => return None,
            }
        }
        let json = match &request.json {
            Some(body) => Some(fill.value(body)?),
            None => None,
        };
        Some(Prepared {
            method: request.method,
            // The service's own address is not data to encode, only what goes after it.
            url: fill.address(&request.url.replace("{base}", &self.def.base_url))?,
            query,
            json,
        })
    }
}

/// A request ready to send.
pub struct Prepared {
    method: Method,
    url: String,
    query: Vec<(String, String)>,
    json: Option<Value>,
}

impl Prepared {
    #[cfg(test)]
    pub fn json_for_test(&self) -> Value {
        self.json.clone().unwrap_or(Value::Null)
    }

    #[cfg(test)]
    pub fn url_for_test(&self) -> String {
        self.url.clone()
    }

    #[cfg(test)]
    pub fn query_for_test(&self) -> Vec<(String, String)> {
        self.query.clone()
    }

    fn key(&self, provider: &str) -> String {
        // The definitions' version is part of it, so an update is not answered from an answer
        // read with the old one.
        format!(
            "{provider}:v{}:{:?}:{}:{:?}:{:?}",
            sources::current().version,
            self.method,
            self.url,
            self.query,
            self.json
        )
    }
}

type Answer = Result<Option<Arc<Value>>, Arc<anyhow::Error>>;

/// The requests being made right now, by key. A page asks for many lists at once and several
/// of them need the same catalogue page, so a second ask for one that is on its way waits for
/// that one instead of making it again.
fn in_flight() -> &'static Mutex<HashMap<String, Shared<BoxFuture<'static, Answer>>>> {
    static FLIGHTS: OnceLock<Mutex<HashMap<String, Shared<BoxFuture<'static, Answer>>>>> =
        OnceLock::new();
    FLIGHTS.get_or_init(Default::default)
}

/// The answer to a request, or `None` when the service has nothing there (a 404). It is shared
/// rather than copied, because some are tens of megabytes. It is reused for `ttl`, and for a
/// short while only when it holds no rows at `rows`.
///
/// With `keep` the answer is also kept on disk. One from an earlier run that is not older than
/// a day is returned at once, and when it is past its `ttl` it is refreshed in the background
/// for the next time.
pub async fn get(
    provider: &str,
    kind: Kind,
    request: Prepared,
    check: Option<&Check>,
    ttl: Duration,
    rows: &str,
    keep: bool,
) -> anyhow::Result<Option<Arc<Value>>> {
    let key = request.key(provider);
    if let Some(answer) = peek::<Option<Arc<Value>>>(&key) {
        return Ok(answer);
    }
    let keep = keep && disk::enabled();
    match if keep { disk::read(&key) } else { None } {
        Some(kept) if kept.age <= ttl => {
            let body = Arc::new(kept.body);
            put(&key, Some(body.clone()), ttl - kept.age);
            Ok(Some(body))
        }
        Some(kept) => {
            // Shown now, and replaced when the fresh one arrives. If it never does, it is asked
            // for again the next time rather than trusted for long.
            let body = Arc::new(kept.body);
            put(&key, Some(body.clone()), STALE_FOR);
            let refresh = flight(&key, request, check, kind, ttl, rows, keep);
            tokio::spawn(async move {
                let _ = refresh.await;
            });
            Ok(Some(body))
        }
        None => flight(&key, request, check, kind, ttl, rows, keep)
            .await
            .map_err(|error| anyhow::anyhow!("{error:#}")),
    }
}

/// How long an answer that is being shown past its time is trusted before it is asked for again.
const STALE_FOR: Duration = Duration::from_secs(20);

/// The request being made for `key`: the one already on its way, or a new one.
fn flight(
    key: &str,
    request: Prepared,
    check: Option<&Check>,
    kind: Kind,
    ttl: Duration,
    rows: &str,
    keep: bool,
) -> Shared<BoxFuture<'static, Answer>> {
    in_flight()
        .lock()
        .unwrap()
        .entry(key.to_string())
        .or_insert_with(|| {
            let (check, rows, key) = (check.cloned(), rows.to_string(), key.to_string());
            async move {
                let answer = send(request, check, kind).await.map_err(Arc::new);
                if let Ok(body) = &answer {
                    let empty = body
                        .as_ref()
                        .is_none_or(|body| select(body, &rows).is_empty());
                    put(&key, body.clone(), if empty { EMPTY_TTL } else { ttl });
                    if let (true, false, Some(body)) = (keep, empty, body) {
                        disk::write(&key, body);
                    }
                }
                in_flight().lock().unwrap().remove(&key);
                answer
            }
            .boxed()
            .shared()
        })
        .clone()
}

/// Makes the request.
async fn send(
    request: Prepared,
    check: Option<Check>,
    kind: Kind,
) -> anyhow::Result<Option<Arc<Value>>> {
    let builder = match request.method {
        Method::Get => client().get(&request.url),
        Method::Post => client().post(&request.url),
    };
    let mut builder = builder.query(&request.query);
    if let Some(body) = &request.json {
        builder = builder.json(body);
    }
    let response = builder.send().await?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let bytes = read_capped(response, MAX_BYTES).await?;
    // A service that reports an error in its own words is read before the status is, so the
    // words are what is shown.
    let body: Option<Value> = serde_json::from_slice(&bytes).ok();
    if let (Some(body), Some(check)) = (&body, &check) {
        failed(check, body, kind)?;
    }
    anyhow::ensure!(status.is_success(), "HTTP status {status}");
    body.map(|body| Some(Arc::new(body)))
        .ok_or_else(|| anyhow::anyhow!("not a JSON response"))
}

/// An error if the answer says it failed.
fn failed(check: &Check, body: &Value, kind: Kind) -> anyhow::Result<()> {
    let row = Row {
        value: body,
        parent: None,
        kind,
    };
    let env = Env {
        now_ms: now_ms(),
        active: &[],
    };
    let reported = check
        .error
        .as_deref()
        .is_some_and(|path| select(body, path).iter().any(|value| !value.is_null()))
        || check.ok.as_ref().is_some_and(|ok| !matches(ok, row, &env));
    if !reported {
        return Ok(());
    }
    let message = check
        .message
        .as_deref()
        .and_then(|path| select(body, path).into_iter().find_map(scalar))
        .unwrap_or_else(|| "the service reported an error".to_string());
    anyhow::bail!("{message}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(value: Value) -> Check {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_answer_that_says_it_failed_is_an_error_with_its_own_words() {
        let by_error = check(json!({ "error": "errors", "message": "errors.0.message" }));
        let failure = json!({ "errors": [{ "message": "Too Many Requests" }] });
        let error = failed(&by_error, &failure, Kind::Anime).unwrap_err();
        assert_eq!(error.to_string(), "Too Many Requests");
        assert!(failed(&by_error, &json!({ "data": {} }), Kind::Anime).is_ok());
        assert!(failed(&by_error, &json!({ "errors": null }), Kind::Anime).is_ok());

        let by_status = check(
            json!({ "ok": { "path": "status", "equals": "ok" }, "message": "status_message" }),
        );
        assert!(failed(&by_status, &json!({ "status": "ok" }), Kind::Movie).is_ok());
        let error = failed(
            &by_status,
            &json!({ "status": "error", "status_message": "Query was empty" }),
            Kind::Movie,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Query was empty");
        let error = failed(&by_status, &json!({}), Kind::Movie).unwrap_err();
        assert_eq!(error.to_string(), "the service reported an error");
    }

    /// A local server that counts the requests made to it and answers each after a pause.
    async fn counting_server(body: &'static str) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use axum::{Router, routing::get};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        let app = Router::new().route(
            "/rows",
            get(move || {
                let counted = counted.clone();
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    body
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}/rows"), hits)
    }

    fn request(url: &str) -> Prepared {
        Prepared {
            method: Method::Get,
            url: url.to_string(),
            query: vec![("t".to_string(), url.to_string())],
            json: None,
        }
    }

    async fn ask(url: &str, keep: bool) -> Option<Arc<Value>> {
        get(
            "test",
            Kind::Movie,
            request(url),
            None,
            Duration::from_secs(600),
            "rows",
            keep,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn the_same_request_made_together_is_made_once() {
        let _held = crate::sources::definitions_change_lock().lock().await;
        let (url, hits) = counting_server(r#"{"rows":[1]}"#).await;
        let answers = futures::future::join_all((0..6).map(|_| ask(&url, false))).await;
        assert!(
            answers
                .iter()
                .all(|answer| answer.as_deref() == Some(&json!({"rows":[1]})))
        );
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        // And afterwards it is answered from memory.
        ask(&url, false).await;
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_answer_from_an_earlier_run_is_shown_at_once_and_refreshed_when_it_is_old() {
        use std::sync::atomic::Ordering;
        let _held = crate::sources::definitions_change_lock().lock().await;
        let dir = std::env::temp_dir().join(format!("akidev-fetch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        super::disk::start(dir.clone());

        // Not past its time: used as it is, with no request at all.
        let (fresh_url, fresh_hits) = counting_server(r#"{"rows":["new"]}"#).await;
        let key = request(&fresh_url).key("test");
        super::disk::write_aged(&key, &json!({"rows":["kept"]}), Duration::from_secs(60));
        assert_eq!(
            ask(&fresh_url, true).await.as_deref(),
            Some(&json!({"rows":["kept"]}))
        );
        assert_eq!(fresh_hits.load(Ordering::SeqCst), 0);

        // Past its time but not too old: shown at once, then replaced behind the scenes.
        let (old_url, old_hits) = counting_server(r#"{"rows":["new"]}"#).await;
        let key = request(&old_url).key("test");
        super::disk::write_aged(
            &key,
            &json!({"rows":["kept"]}),
            Duration::from_secs(3 * 3600),
        );
        let started = std::time::Instant::now();
        assert_eq!(
            ask(&old_url, true).await.as_deref(),
            Some(&json!({"rows":["kept"]}))
        );
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "it waited for the network"
        );
        for _ in 0..40 {
            if super::disk::read(&key).is_some_and(|kept| kept.body == json!({"rows":["new"]})) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(old_hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            super::disk::read(&key).unwrap().body,
            json!({"rows":["new"]})
        );

        // Too old to be shown: the network is waited for.
        let (stale_url, stale_hits) = counting_server(r#"{"rows":["new"]}"#).await;
        let key = request(&stale_url).key("test");
        super::disk::write_aged(
            &key,
            &json!({"rows":["kept"]}),
            Duration::from_secs(30 * 3600),
        );
        assert_eq!(
            ask(&stale_url, true).await.as_deref(),
            Some(&json!({"rows":["new"]}))
        );
        assert_eq!(stale_hits.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
