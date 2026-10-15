//! Debrid (TorBox): a service that already holds the torrent and serves the finished file from a
//! CDN. Playback starts at once and nothing joins a swarm from this server.
//!
//! The CDN link carries the API key as its `token` parameter, so it is never handed to a
//! browser: this server fetches it and relays the bytes, which keeps the key out of the page, the
//! history and every screenshot. The link is re-resolved rather than stored, because these links
//! expire and the service asks that they not be kept.
//!
//! Only releases the service already has are used: a release it would have to fetch cannot be
//! watched now, and adding one spends a separate quota. `createtorrent` is therefore called with
//! `add_only_if_cached`, which refuses anything that is not already there.

use crate::origin::{BoxRead, Origin};
use akidev_core::cache;
use akidev_core::model::Release;
use akidev_core::releases::{VideoFile, pick_video_file};
use anyhow::{Context, bail};
use futures::TryStreamExt;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

const API: &str = "https://api.torbox.app/v1/api/torrents";
const LIST_TTL: Duration = Duration::from_secs(10 * 60);
const LINK_TTL: Duration = Duration::from_secs(60);
/// How long to wait before reading the account listing again, when an add has not shown up yet.
const SETTLE: Duration = Duration::from_millis(500);
/// The smallest budget worth starting a request with.
const MIN_STEP: Duration = Duration::from_millis(250);

pub struct Debrid {
    token: String,
    timeout: Duration,
    cdn_hosts: Vec<String>,
    /// Does not follow redirects: a link that bounces elsewhere has left the allowlist.
    client: reqwest::Client,
    base: String,
    /// Always true outside tests, which cannot serve HTTPS from a port on localhost.
    require_https: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Account {
    id: i64,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    files: Vec<AccountFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccountFile {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    short_name: String,
    #[serde(default)]
    size: u64,
}

/// A file the service can serve, chosen for the episode.
pub struct Found {
    pub release: String,
    pub origin: Origin,
}

impl Debrid {
    pub fn new(token: String, timeout: Duration, cdn_hosts: Vec<String>) -> Option<Arc<Debrid>> {
        Debrid::with_base(token, timeout, cdn_hosts, API.to_string())
    }

    fn with_base(
        token: String,
        timeout: Duration,
        cdn_hosts: Vec<String>,
        base: String,
    ) -> Option<Arc<Debrid>> {
        if token.is_empty() {
            return None;
        }
        let client = reqwest::Client::builder()
            .user_agent(akidev_core::http::USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        Some(Arc::new(Debrid {
            token,
            timeout,
            cdn_hosts,
            client,
            base,
            require_https: true,
        }))
    }

    /// Whether the token is accepted, for `/health`. A rotated or mistyped token looks the same
    /// as a working one until someone tries to watch something.
    pub async fn status(&self) -> bool {
        self.api("mylist", &[("limit", "1".into())], self.timeout)
            .await
            .is_ok()
    }

    /// Takes the token out of anything that is about to be logged: it rides in the query string
    /// of `requestdl`, and reqwest puts the URL into some of its error messages.
    fn redact(&self, message: impl ToString) -> String {
        message.to_string().replace(&self.token, "[REDACTED]")
    }

    fn bounded(&self, left: Duration) -> Duration {
        left.min(self.timeout).max(MIN_STEP)
    }

    /// TorBox answers every route with `{ success, error, detail, data }`.
    fn unwrap(body: Value) -> anyhow::Result<Value> {
        if body.get("success").and_then(Value::as_bool) != Some(true) {
            let reason = body
                .get("detail")
                .or_else(|| body.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("TorBox returned an unsuccessful response");
            bail!("{reason}");
        }
        Ok(body.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn api(
        &self,
        route: &str,
        query: &[(&str, String)],
        left: Duration,
    ) -> anyhow::Result<Value> {
        let response = self
            .client
            .get(format!("{}/{route}", self.base))
            .bearer_auth(&self.token)
            .query(query)
            .timeout(self.bounded(left))
            .send()
            .await
            .map_err(|error| anyhow::anyhow!(self.redact(error.without_url())))?;
        let body: Value = response
            .json()
            .await
            .map_err(|error| anyhow::anyhow!(self.redact(error.without_url())))?;
        Self::unwrap(body)
    }

    async fn add_if_cached(&self, release: &Release, left: Duration) -> bool {
        let form = reqwest::multipart::Form::new()
            .text("magnet", release.magnet.clone())
            .text("add_only_if_cached", "true");
        let sent = self
            .client
            .post(format!("{}/createtorrent", self.base))
            .bearer_auth(&self.token)
            .multipart(form)
            .timeout(self.bounded(left))
            .send()
            .await;
        let outcome = match sent {
            Ok(response) => match response.json::<Value>().await {
                Ok(body) => Self::unwrap(body).map(|_| ()),
                Err(error) => Err(anyhow::anyhow!(error.without_url())),
            },
            Err(error) => Err(anyhow::anyhow!(error.without_url())),
        };
        match outcome {
            Ok(()) => {
                // The listing is cached, so it is dropped, or the new entry would be invisible
                // and every request would add the torrent again.
                cache::put("torbox:mylist", Vec::<Account>::new(), Duration::ZERO);
                true
            }
            Err(error) => {
                // A refusal means the release is not cached, which is the normal case for most
                // candidates, so it is only worth a debug line.
                tracing::debug!(
                    "[debrid] could not add \"{}\": {}",
                    release.title,
                    self.redact(format!("{error:#}"))
                );
                false
            }
        }
    }

    /// Which of these releases the service already holds.
    async fn cached_hashes(
        &self,
        releases: &[Release],
        left: Duration,
    ) -> anyhow::Result<HashSet<String>> {
        let mut hashes: Vec<&str> = releases.iter().map(|r| r.info_hash.as_str()).collect();
        hashes.sort_unstable();
        hashes.dedup();
        if hashes.is_empty() {
            return Ok(HashSet::new());
        }
        let key = format!("torbox:checkcached:{}", hashes.join(","));
        let joined = hashes.join(",");
        let data: Value = cache::cached(
            &key,
            |_| LIST_TTL,
            || async {
                self.api(
                    "checkcached",
                    &[
                        ("hash", joined.clone()),
                        ("format", "object".into()),
                        ("list_files", "true".into()),
                    ],
                    left,
                )
                .await
            },
        )
        .await?;
        Ok(data
            .as_object()
            .map(|found| {
                found
                    .iter()
                    .filter(|(_, entry)| !entry.is_null() && entry.as_bool() != Some(false))
                    .map(|(hash, _)| hash.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Which of these releases the service holds, for showing a viewer which will start at once.
    /// Nothing is cached when the service cannot be reached: that is an unknown, not a no.
    pub async fn holds(&self, releases: &[Release]) -> Option<HashSet<String>> {
        self.cached_hashes(releases, self.timeout).await.ok()
    }

    /// The account's own entry for a torrent: `mylist` is the only route that returns the numeric
    /// torrent and file ids `requestdl` needs.
    async fn account_entry(
        &self,
        info_hash: &str,
        left: Duration,
    ) -> anyhow::Result<Option<Account>> {
        // A generous limit: an entry that falls past the end of the page is indistinguishable
        // from one that is not there, which would silently drop a cached release.
        let list: Vec<Account> = cache::cached(
            "torbox:mylist",
            |_| LIST_TTL,
            || async {
                let data = self
                    .api("mylist", &[("limit", "5000".into())], left)
                    .await?;
                serde_json::from_value::<Vec<Account>>(data).context("reading the account listing")
            },
        )
        .await?;
        Ok(list
            .into_iter()
            .find(|entry| entry.hash.eq_ignore_ascii_case(info_hash) && !entry.files.is_empty()))
    }

    /// The CDN link for one file, valid for a minute. It carries the API key.
    async fn link(&self, torrent_id: i64, file_id: i64, left: Duration) -> anyhow::Result<String> {
        let key = format!("torbox:dl:{torrent_id}:{file_id}");
        let url: String = cache::cached(
            &key,
            |_| LINK_TTL,
            || async {
                let data = self
                    .api(
                        "requestdl",
                        &[
                            ("torrent_id", torrent_id.to_string()),
                            ("file_id", file_id.to_string()),
                            // Required by this route in addition to the header.
                            ("token", self.token.clone()),
                        ],
                        left,
                    )
                    .await?;
                data.as_str()
                    .map(str::to_string)
                    .context("requestdl returned no link")
            },
        )
        .await?;
        Ok(url)
    }

    /// Whether a link may be fetched. The destination does not come from a request, but this
    /// server fetches it, so it is parsed and allowlisted rather than trusted: a prefix check
    /// would accept any host at all.
    pub fn allowed(&self, url: &str) -> bool {
        let Ok(url) = reqwest::Url::parse(url) else {
            return false;
        };
        let scheme_ok = url.scheme() == "https" || (!self.require_https && url.scheme() == "http");
        if !scheme_ok || !url.username().is_empty() || url.password().is_some() {
            return false;
        }
        let Some(host) = url.host_str() else {
            return false;
        };
        host_allowed(host, &self.cdn_hosts)
    }

    /// A reader over one file from `offset`, fetched through this server.
    pub async fn open(
        &self,
        torrent_id: i64,
        file_id: i64,
        offset: u64,
    ) -> anyhow::Result<BoxRead> {
        let url = self.link(torrent_id, file_id, self.timeout).await?;
        if !self.allowed(&url) {
            let host = reqwest::Url::parse(&url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_else(|| "unparseable".into());
            // The host is logged because the provider rotates its CDN domain and this is the
            // only place the new one shows. The URL is not: it carries the API key.
            tracing::error!(
                "[debrid] refused a link outside DEBRID_CDN_HOSTS: host={host}. Set DEBRID_CDN_HOSTS to this domain to allow it."
            );
            bail!("the download service returned an unusable link");
        }
        let mut request = self.client.get(&url);
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| anyhow::anyhow!(self.redact(error.without_url())))?;
        let status = response.status();
        if !(status.as_u16() == 200 && offset == 0 || status.as_u16() == 206) {
            bail!("the download service answered {status}");
        }
        let stream = response
            .bytes_stream()
            .map_err(|error| std::io::Error::other(error.without_url()));
        Ok(Box::new(tokio_util::io::StreamReader::new(Box::pin(
            stream,
        ))))
    }

    /// The best cached copy of the episode the service holds, if any.
    pub async fn find(
        self: &Arc<Self>,
        ranked: &[Release],
        episode: Option<u32>,
        season: Option<u32>,
        max_tries: usize,
        deadline: Instant,
    ) -> Result<Found, String> {
        let left = || deadline.saturating_duration_since(Instant::now());
        if left().is_zero() {
            return Err("Timed out looking for a playable source.".into());
        }
        let cached = match self.cached_hashes(ranked, left()).await {
            Ok(cached) => cached,
            Err(error) => {
                // An outage must not take the app down: "unavailable" lets the relay answer.
                tracing::warn!(
                    "[debrid] checkcached failed: {}",
                    self.redact(format!("{error:#}"))
                );
                return Err("The debrid service could not be reached.".into());
            }
        };
        let usable: Vec<&Release> = ranked
            .iter()
            .filter(|release| cached.contains(&release.info_hash))
            .collect();
        if usable.is_empty() {
            return Err("No cached copy of this episode on the debrid service.".into());
        }

        // Each candidate costs two or three calls, so the loop is bounded.
        for release in usable.into_iter().take(max_tries) {
            if left() < MIN_STEP {
                break;
            }
            match self.file_for(release, episode, season, left()).await {
                Ok(Some(origin)) => {
                    return Ok(Found {
                        release: release.title.clone(),
                        origin,
                    });
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    "[debrid] {}: {}",
                    release.title,
                    self.redact(format!("{error:#}"))
                ),
            }
        }
        if left() < MIN_STEP {
            return Err("The debrid service ran out of time for this episode.".into());
        }
        Err("Cached copies exist, but none holds a file that could be played.".into())
    }

    async fn file_for(
        self: &Arc<Self>,
        release: &Release,
        episode: Option<u32>,
        season: Option<u32>,
        left: Duration,
    ) -> anyhow::Result<Option<Origin>> {
        // A cached release the account does not hold yet has no ids, so it is added first. That
        // is the step that makes the source work on a fresh account at all.
        let mut entry = self.account_entry(&release.info_hash, left).await?;
        if entry.is_none() && self.add_if_cached(release, left).await {
            // The add is asynchronous on the service's side: the listing read straight after can
            // still be the pre-add snapshot, so it is read once more after a short pause.
            entry = self.account_entry(&release.info_hash, left).await?;
            if entry.is_none() && left > SETTLE + MIN_STEP {
                tokio::time::sleep(SETTLE).await;
                cache::put("torbox:mylist", Vec::<Account>::new(), Duration::ZERO);
                entry = self
                    .account_entry(&release.info_hash, left - SETTLE)
                    .await?;
            }
        }
        let Some(entry) = entry else { return Ok(None) };

        let files: Vec<VideoFile> = entry
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| VideoFile {
                index,
                name: if file.name.is_empty() {
                    file.short_name.clone()
                } else {
                    file.name.clone()
                },
                length: file.size,
            })
            .collect();
        // No size floor: an absent size would otherwise reject every file.
        let Some(picked) = pick_video_file(&files, episode, 0, season) else {
            return Ok(None);
        };
        let file = &entry.files[picked.index];
        let name = picked
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&picked.name)
            .to_string();
        Ok(Some(Origin::Debrid {
            debrid: self.clone(),
            torrent_id: entry.id,
            file_id: file.id,
            length: file.size,
            name,
        }))
    }
}

/// Whether a host is one of the allowed domains or a subdomain of one.
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    allowed
        .iter()
        .any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debrid() -> Arc<Debrid> {
        Debrid::new(
            "secret-key".into(),
            Duration::from_secs(5),
            vec!["tb-cdn.io".into(), "tb-cdn.pw".into()],
        )
        .unwrap()
    }

    /// The whole flow against a stand-in for TorBox and its CDN: the cached check, the account
    /// listing that only shows the torrent after it is added, the link, and a ranged read.
    #[tokio::test]
    async fn a_cached_release_is_added_found_and_read_from_an_offset() {
        use axum::extract::{Query, State};
        use axum::http::{HeaderMap, StatusCode, header};
        use axum::response::IntoResponse;
        use axum::routing::{get, post};
        use axum::{Json, Router};
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};

        const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

        #[derive(Clone)]
        struct Mock {
            added: Arc<AtomicBool>,
            port: Arc<std::sync::atomic::AtomicU16>,
        }

        async fn checkcached(headers: HeaderMap) -> Json<Value> {
            assert_eq!(headers[header::AUTHORIZATION], "Bearer secret-key");
            Json(json!({ "success": true, "data": { HASH: { "name": "x" } } }))
        }
        async fn mylist(State(mock): State<Mock>) -> Json<Value> {
            let data = if mock.added.load(Ordering::SeqCst) {
                json!([{ "id": 7, "hash": HASH, "files": [
                    { "id": 1, "name": "Show/Show - 04.mkv", "size": BODY.len() },
                    { "id": 2, "name": "Show/Show - 05.mkv", "size": BODY.len() },
                ]}])
            } else {
                json!([])
            };
            Json(json!({ "success": true, "data": data }))
        }
        async fn createtorrent(State(mock): State<Mock>) -> Json<Value> {
            mock.added.store(true, Ordering::SeqCst);
            Json(json!({ "success": true, "data": {} }))
        }
        async fn requestdl(
            State(mock): State<Mock>,
            Query(query): Query<std::collections::HashMap<String, String>>,
        ) -> Json<Value> {
            // The token rides in the query on this route, and the file asked for is the episode.
            assert_eq!(query["token"], "secret-key");
            assert_eq!(
                (query["torrent_id"].as_str(), query["file_id"].as_str()),
                ("7", "2")
            );
            let port = mock.port.load(Ordering::SeqCst);
            Json(
                json!({ "success": true, "data": format!("http://127.0.0.1:{port}/cdn/file.mkv?token=secret-key") }),
            )
        }
        async fn cdn(headers: HeaderMap) -> impl IntoResponse {
            let from = headers
                .get(header::RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("bytes="))
                .and_then(|value| value.strip_suffix('-'))
                .and_then(|value| value.parse::<usize>().ok());
            match from {
                Some(from) => (StatusCode::PARTIAL_CONTENT, BODY[from..].to_vec()),
                None => (StatusCode::OK, BODY.to_vec()),
            }
        }

        let mock = Mock {
            added: Arc::new(AtomicBool::new(false)),
            port: Arc::new(Default::default()),
        };
        let app = Router::new()
            .route("/checkcached", get(checkcached))
            .route("/mylist", get(mylist))
            .route("/createtorrent", post(createtorrent))
            .route("/requestdl", get(requestdl))
            .route("/cdn/file.mkv", get(cdn))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        mock.port.store(port, Ordering::SeqCst);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let mut debrid = Debrid::with_base(
            "secret-key".into(),
            Duration::from_secs(5),
            vec!["127.0.0.1".into()],
            format!("http://127.0.0.1:{port}"),
        )
        .unwrap();
        Arc::get_mut(&mut debrid).unwrap().require_https = false;

        let release = Release {
            title: "Show - 05 [1080p]".into(),
            info_hash: HASH.into(),
            magnet: format!("magnet:?xt=urn:btih:{HASH}"),
            size_bytes: None,
            seeders: 3,
            source: "t".into(),
        };
        let other = Release {
            info_hash: "b".repeat(40),
            title: "other".into(),
            ..release.clone()
        };
        let found = debrid
            .find(
                &[other, release],
                Some(5),
                None,
                3,
                Instant::now() + Duration::from_secs(10),
            )
            .await
            .map_err(|reason| format!("not found: {reason}"))
            .unwrap();
        assert_eq!(found.release, "Show - 05 [1080p]");
        assert!(
            mock.added.load(Ordering::SeqCst),
            "the cached release should have been added"
        );
        assert_eq!(found.origin.name(), "Show - 05.mkv");
        assert_eq!(found.origin.length(), BODY.len() as u64);

        use tokio::io::AsyncReadExt;
        let mut reader = found.origin.open_at(10).await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes, &BODY[10..]);
        let mut reader = found.origin.open_at(0).await.unwrap();
        let mut all = Vec::new();
        reader.read_to_end(&mut all).await.unwrap();
        assert_eq!(all, BODY);
    }

    #[tokio::test]
    async fn nothing_cached_is_a_reason_not_an_error() {
        use axum::routing::get;
        use axum::{Json, Router};
        let app = Router::new().route(
            "/checkcached",
            get(|| async { Json(serde_json::json!({ "success": true, "data": {} })) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let debrid = Debrid::with_base(
            "secret-key".into(),
            Duration::from_secs(5),
            vec![],
            format!("http://127.0.0.1:{port}"),
        )
        .unwrap();
        let release = Release {
            title: "Show - 77".into(),
            info_hash: "c".repeat(40),
            magnet: String::new(),
            size_bytes: None,
            seeders: 1,
            source: "t".into(),
        };
        let reason = debrid
            .find(
                &[release],
                Some(77),
                None,
                3,
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(reason.contains("No cached copy"), "{reason}");
    }

    #[test]
    fn without_a_token_there_is_no_debrid() {
        assert!(Debrid::new(String::new(), Duration::from_secs(1), vec![]).is_none());
    }

    #[test]
    fn a_link_must_be_https_on_an_allowed_host() {
        let debrid = debrid();
        assert!(debrid.allowed("https://nexus-144.enam.tb-cdn.io/dl/x?token=t"));
        assert!(debrid.allowed("https://tb-cdn.pw/file"));
        assert!(!debrid.allowed("http://nexus.tb-cdn.io/file"));
        assert!(!debrid.allowed("https://evil.com/tb-cdn.io"));
        assert!(!debrid.allowed("https://nottb-cdn.io/file"));
        assert!(!debrid.allowed("https://tb-cdn.io.evil.com/file"));
        assert!(!debrid.allowed("https://user:pass@tb-cdn.io/file"));
        assert!(!debrid.allowed("not a url"));
    }

    #[test]
    fn the_token_never_reaches_a_log_line() {
        let debrid = debrid();
        let message =
            debrid.redact("error sending request for url (https://x/?token=secret-key&a=1)");
        assert!(!message.contains("secret-key"));
        assert!(message.contains("[REDACTED]"));
    }

    #[test]
    fn an_unsuccessful_body_is_an_error_with_the_reason() {
        let failed = serde_json::json!({ "success": false, "detail": "Bad token" });
        assert_eq!(Debrid::unwrap(failed).unwrap_err().to_string(), "Bad token");
        let fine = serde_json::json!({ "success": true, "data": [1] });
        assert_eq!(Debrid::unwrap(fine).unwrap(), serde_json::json!([1]));
    }

    #[test]
    fn a_domain_matches_itself_and_its_subdomains_only() {
        let allowed = vec!["tb-cdn.io".to_string()];
        assert!(host_allowed("tb-cdn.io", &allowed));
        assert!(host_allowed("a.b.tb-cdn.io", &allowed));
        assert!(!host_allowed("xtb-cdn.io", &allowed));
    }
}
