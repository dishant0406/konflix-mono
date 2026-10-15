//! Finding and fetching a newer version of the desktop app.
//!
//! Releases are published on GitHub, one per platform per version (`desktop-mac-v0.1.4`,
//! `desktop-windows-v0.1.4`). `check` asks GitHub which is newest for this platform and
//! `download` fetches its installer, only from GitHub's own hosts, and keeps it only if its
//! SHA-256 matches the one published next to it. The hash guards against a corrupt download;
//! what makes an installer trustworthy is its code signature, which the app checks before it
//! installs anything (see `desktop/app/src/updater.rs`).

use anyhow::{Context, bail, ensure};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// The GitHub repository releases are published in. It may be renamed; GitHub redirects.
pub const REPO: &str = "dishant0406/konflix-mono";

/// Shown when a release has no page of its own to open.
pub const RELEASES_PAGE: &str = "https://github.com/dishant0406/konflix-mono/releases";

/// Hosts an installer or its hash may be fetched from, and be redirected to.
const ALLOWED_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// No installer is anywhere near this; a host that sends more is not sending one.
const MAX_ASSET_BYTES: u64 = 1 << 30;
const MAX_NOTES_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    /// The platform this build is for, or `None` where there is no updater (Intel Macs, Linux).
    pub fn current() -> Option<Platform> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Platform::Mac)
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Platform::Windows)
        } else {
            None
        }
    }

    fn tag_prefix(self) -> &'static str {
        match self {
            Platform::Mac => "desktop-mac-v",
            Platform::Windows => "desktop-windows-v",
        }
    }

    /// The installer's file name for a version.
    fn asset_name(self, version: &Version) -> String {
        match self {
            Platform::Mac => format!("Akidev-{version}-arm64.dmg"),
            Platform::Windows => format!("Akidev-{version}-x86_64-setup.exe"),
        }
    }
}

/// `major.minor.patch`, nothing else. A tag with a suffix or a missing part is not one of ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.');
        let mut next = || {
            let part = parts.next()?;
            // `parse` accepts a leading plus; a version does not.
            (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse::<u64>().ok())
                .flatten()
        };
        let version = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub tag: String,
    /// The installer's file name, as published.
    pub asset_name: String,
    pub asset_url: String,
    pub sha256_url: String,
    pub size: u64,
    pub release_url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Whether a URL may be downloaded from: https, and one of GitHub's hosts.
pub fn is_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
        && url.username().is_empty()
        && url.password().is_none()
}

/// `is_allowed` for an address that is still text.
pub fn is_allowed_address(address: &str) -> bool {
    Url::parse(address).is_ok_and(|url| is_allowed(&url))
}

/// The newest release for `platform` that is newer than `current`, with the installer and its
/// hash attached. Drafts, prereleases, other platforms and malformed tags never count, and a
/// release that lacks either file (still being published) is passed over.
fn pick(releases: Vec<Release>, platform: Platform, current: Version) -> Option<Update> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix(platform.tag_prefix())?)?;
            (version > current).then_some((version, release))
        })
        .filter_map(|(version, release)| {
            let name = platform.asset_name(&version);
            let hash_name = format!("{name}.sha256");
            let find = |wanted: &str| release.assets.iter().find(|asset| asset.name == wanted);
            let (asset, hash) = (find(&name)?, find(&hash_name)?);
            Some((
                version,
                Update {
                    version: version.to_string(),
                    tag: release.tag_name.clone(),
                    asset_name: name,
                    asset_url: asset.browser_download_url.clone(),
                    sha256_url: hash.browser_download_url.clone(),
                    size: asset.size,
                    release_url: release.html_url.clone(),
                    notes: notes(release.body.as_deref().unwrap_or_default()),
                },
            ))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, update)| update)
}

fn notes(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(MAX_NOTES_CHARS) {
        Some((end, _)) => format!("{}…", body[..end].trim_end()),
        None => body.to_string(),
    }
}

/// Asks GitHub whether a version newer than `current` has been released for this platform.
pub async fn check(current: &str) -> anyhow::Result<Option<Update>> {
    let Some(platform) = Platform::current() else {
        return Ok(None);
    };
    check_at(
        &format!("https://api.github.com/repos/{REPO}/releases?per_page=30"),
        platform,
        current,
    )
    .await
}

async fn check_at(url: &str, platform: Platform, current: &str) -> anyhow::Result<Option<Update>> {
    let current =
        Version::parse(current).context("the running version is not major.minor.patch")?;
    let response = crate::http::client()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    let body = crate::http::read_capped(response, 4 << 20).await?;
    let releases: Vec<Release> = serde_json::from_slice(&body).context("reading the releases")?;
    Ok(pick(releases, platform, current))
}

/// Fetches the installer into `dest_dir` and returns where it is. `on_progress` is called with
/// the bytes received and the total expected. The file appears under its real name only once its
/// hash matches; any failure leaves nothing behind.
pub async fn download(
    update: &Update,
    dest_dir: &Path,
    on_progress: impl FnMut(u64, u64),
) -> anyhow::Result<PathBuf> {
    download_with(update, dest_dir, is_allowed, on_progress).await
}

async fn download_with(
    update: &Update,
    dest_dir: &Path,
    allow: fn(&Url) -> bool,
    mut on_progress: impl FnMut(u64, u64),
) -> anyhow::Result<PathBuf> {
    let name = &update.asset_name;
    ensure!(
        !name.is_empty()
            && !name.starts_with('.')
            && !name.contains(['/', '\\', ':'])
            && update.size <= MAX_ASSET_BYTES,
        "the release lists an installer that cannot be used"
    );
    let parse = |text: &str| -> anyhow::Result<Url> {
        let url = Url::parse(text).context("a download address is not a URL")?;
        ensure!(
            allow(&url),
            "{url} is not a place updates are downloaded from"
        );
        Ok(url)
    };
    let (asset_url, sha256_url) = (parse(&update.asset_url)?, parse(&update.sha256_url)?);

    // The whole download has no deadline, but a connection that goes quiet is dropped. Every
    // redirect must stay on an allowed host too: GitHub sends assets to its own storage.
    let client = reqwest::Client::builder()
        .user_agent(crate::http::USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() < 5 && allow(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("the download was redirected somewhere it may not go")
            }
        }))
        .build()?;

    tokio::fs::create_dir_all(dest_dir).await?;
    let partial = dest_dir.join(format!(".{name}.part"));
    let result = fetch(
        &client,
        asset_url,
        &sha256_url,
        &partial,
        update.size,
        &mut on_progress,
    )
    .await;
    let finished = match result {
        Ok(()) => {
            let target = dest_dir.join(name);
            tokio::fs::rename(&partial, &target)
                .await
                .map(|()| target)
                .map_err(Into::into)
        }
        Err(error) => Err(error),
    };
    if finished.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    finished
}

async fn fetch(
    client: &reqwest::Client,
    asset_url: Url,
    sha256_url: &Url,
    partial: &Path,
    expected_size: u64,
    on_progress: &mut impl FnMut(u64, u64),
) -> anyhow::Result<()> {
    let mut response = client.get(asset_url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(expected_size);
    ensure!(total <= MAX_ASSET_BYTES, "the installer is too large");
    let mut file = tokio::fs::File::create(partial).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = response.chunk().await? {
        received += chunk.len() as u64;
        ensure!(received <= MAX_ASSET_BYTES, "the installer is too large");
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        on_progress(received, total.max(received));
    }
    file.flush().await?;
    drop(file);
    on_progress(received, received);

    let published = client
        .get(sha256_url.clone())
        .send()
        .await?
        .error_for_status()?;
    let published = crate::http::read_capped(published, 4096).await?;
    check_hash(
        &crate::sources::encode_hex(&hasher.finalize()),
        std::str::from_utf8(&published).context("the published hash is not text")?,
    )
}

/// Compares the hash of what was received with the published `<hex>  <file name>` line.
fn check_hash(actual_hex: &str, published: &str) -> anyhow::Result<()> {
    let wanted = published.split_whitespace().next().unwrap_or_default();
    if wanted.len() != 64 || !wanted.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("the published hash is not a SHA-256");
    }
    ensure!(
        wanted.eq_ignore_ascii_case(actual_hex),
        "the download does not match its published hash"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::{StatusCode, header};
    use axum::routing::get;
    use serde_json::json;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn release(tag: &str, draft: bool, prerelease: bool) -> Release {
        let platform = if tag.starts_with("desktop-windows-v") {
            Platform::Windows
        } else {
            Platform::Mac
        };
        let version = Version::parse(tag.rsplit('v').next().unwrap());
        let assets = version
            .map(|version| {
                let name = platform.asset_name(&version);
                vec![
                    Asset {
                        browser_download_url: format!("https://github.com/x/{name}"),
                        name: name.clone(),
                        size: 10,
                    },
                    Asset {
                        browser_download_url: format!("https://github.com/x/{name}.sha256"),
                        name: format!("{name}.sha256"),
                        size: 90,
                    },
                ]
            })
            .unwrap_or_default();
        Release {
            tag_name: tag.into(),
            draft,
            prerelease,
            html_url: format!("https://github.com/x/releases/tag/{tag}"),
            body: Some("Notes".into()),
            assets,
        }
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(v("0.1.10") > v("0.1.9"));
        assert!(v("0.2.0") > v("0.1.99"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert_eq!(v("0.1.3"), v("0.1.3"));
    }

    #[test]
    fn only_plain_major_minor_patch_is_a_version() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-rc1",
            "+1.2.3",
            "1..3",
            "v1.2.3",
            "1.2.-3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_newest_release_for_this_platform_wins() {
        let found = pick(
            vec![
                release("desktop-mac-v0.1.4", false, false),
                release("desktop-mac-v0.1.10", false, false),
                release("desktop-mac-v0.1.9", false, false),
            ],
            Platform::Mac,
            v("0.1.3"),
        )
        .unwrap();
        assert_eq!(found.version, "0.1.10");
        assert_eq!(found.tag, "desktop-mac-v0.1.10");
        assert_eq!(found.asset_name, "Akidev-0.1.10-arm64.dmg");
        assert!(found.sha256_url.ends_with("arm64.dmg.sha256"));
        assert_eq!(found.notes, "Notes");
    }

    #[test]
    fn drafts_prereleases_other_platforms_and_bad_tags_are_skipped() {
        let found = pick(
            vec![
                release("desktop-mac-v0.3.0", true, false),
                release("desktop-mac-v0.2.0", false, true),
                release("desktop-windows-v0.9.0", false, false),
                release("desktop-mac-v0.8.0-rc1", false, false),
                release("desktop-mac-v1.0", false, false),
                release("web-v5.0.0", false, false),
                release("desktop-mac-v0.1.4", false, false),
            ],
            Platform::Mac,
            v("0.1.3"),
        )
        .unwrap();
        assert_eq!(found.version, "0.1.4");
        let windows = pick(
            vec![release("desktop-windows-v0.9.0", false, false)],
            Platform::Windows,
            v("0.1.3"),
        )
        .unwrap();
        assert_eq!(windows.asset_name, "Akidev-0.9.0-x86_64-setup.exe");
    }

    #[test]
    fn nothing_is_offered_when_equal_or_older() {
        let releases = || {
            vec![
                release("desktop-mac-v0.1.3", false, false),
                release("desktop-mac-v0.1.2", false, false),
            ]
        };
        assert_eq!(pick(releases(), Platform::Mac, v("0.1.3")), None);
        assert_eq!(pick(releases(), Platform::Mac, v("0.2.0")), None);
    }

    #[test]
    fn a_release_missing_its_hash_is_not_offered() {
        let mut half = release("desktop-mac-v0.1.9", false, false);
        half.assets.pop();
        let found = pick(
            vec![half, release("desktop-mac-v0.1.4", false, false)],
            Platform::Mac,
            v("0.1.3"),
        )
        .unwrap();
        assert_eq!(found.version, "0.1.4");
    }

    #[test]
    fn notes_are_cut_on_a_character_boundary() {
        assert_eq!(notes("  short \n"), "short");
        let long = "é".repeat(800);
        let cut = notes(&long);
        assert_eq!(cut.chars().count(), MAX_NOTES_CHARS + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn only_github_over_https_is_allowed() {
        let ok = |text: &str| is_allowed(&Url::parse(text).unwrap());
        assert!(ok("https://github.com/a/b/releases/download/t/f.dmg"));
        assert!(ok("https://objects.githubusercontent.com/x"));
        assert!(ok("https://release-assets.githubusercontent.com/x"));
        assert!(!ok("http://github.com/a"));
        assert!(!ok("https://evilgithub.com/a"));
        assert!(!ok("https://github.com.evil.com/a"));
        assert!(!ok("https://user@github.com/a"));
        assert!(!ok("https://raw.githubusercontent.com/a"));
        assert!(!ok("file:///etc/passwd"));
    }

    #[test]
    fn the_published_hash_must_match() {
        let actual = "ab".repeat(32);
        assert!(check_hash(&actual, &format!("{actual}  Akidev.dmg\n")).is_ok());
        assert!(check_hash(&actual, &format!("{}  Akidev.dmg", actual.to_uppercase())).is_ok());
        assert!(check_hash(&actual, &format!("{}  Akidev.dmg", "cd".repeat(32))).is_err());
        assert!(check_hash(&actual, "").is_err());
        assert!(check_hash(&actual, "not a hash").is_err());
        assert!(check_hash(&actual, &"ab".repeat(31)).is_err());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("akidev-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn check_reads_the_release_list() {
        let list = json!([
            {"tag_name": "desktop-mac-v0.1.4", "draft": false, "prerelease": false,
             "html_url": "https://github.com/x/releases/tag/desktop-mac-v0.1.4", "body": null,
             "assets": [
                {"name": "Akidev-0.1.4-arm64.dmg", "browser_download_url": "https://github.com/x/a.dmg", "size": 123},
                {"name": "Akidev-0.1.4-arm64.dmg.sha256", "browser_download_url": "https://github.com/x/a.sha", "size": 90}]}
        ]);
        let base = serve(Router::new().route(
            "/releases",
            get(move || {
                let list = list.clone();
                async move {
                    (
                        [(header::CONTENT_TYPE, "application/json")],
                        list.to_string(),
                    )
                }
            }),
        ))
        .await;
        let url = format!("{base}/releases");
        let update = check_at(&url, Platform::Mac, "0.1.3")
            .await
            .unwrap()
            .unwrap();
        assert_eq!((update.version.as_str(), update.size), ("0.1.4", 123));
        assert_eq!(update.notes, "");
        assert!(
            check_at(&url, Platform::Mac, "0.1.4")
                .await
                .unwrap()
                .is_none()
        );
        assert!(check_at(&url, Platform::Mac, "garbage").await.is_err());
        let down =
            serve(Router::new().route("/releases", get(|| async { StatusCode::FORBIDDEN }))).await;
        assert!(
            check_at(&format!("{down}/releases"), Platform::Mac, "0.1.3")
                .await
                .is_err()
        );
    }

    fn update_for(base: &str, bytes: &[u8]) -> Update {
        Update {
            version: "0.1.4".into(),
            tag: "desktop-mac-v0.1.4".into(),
            asset_name: "Akidev-0.1.4-arm64.dmg".into(),
            asset_url: format!("{base}/asset"),
            sha256_url: format!("{base}/asset.sha256"),
            size: bytes.len() as u64,
            release_url: String::new(),
            notes: String::new(),
        }
    }

    async fn host(bytes: Vec<u8>, published: String) -> String {
        serve(
            Router::new()
                .route(
                    "/asset",
                    get(move || {
                        let bytes = bytes.clone();
                        async move { bytes }
                    }),
                )
                .route(
                    "/asset.sha256",
                    get(move || {
                        let published = published.clone();
                        async move { published }
                    }),
                ),
        )
        .await
    }

    #[tokio::test]
    async fn a_matching_download_is_kept_under_its_name_with_progress() {
        let bytes: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
        let hex = crate::sources::encode_hex(&Sha256::digest(&bytes));
        let base = host(bytes.clone(), format!("{hex}  Akidev-0.1.4-arm64.dmg\n")).await;
        let dir = temp_dir("good");
        let mut last = (0, 0);
        let path = download_with(
            &update_for(&base, &bytes),
            &dir,
            |_| true,
            |done, total| last = (done, total),
        )
        .await
        .unwrap();
        assert_eq!(path, dir.join("Akidev-0.1.4-arm64.dmg"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(last, (bytes.len() as u64, bytes.len() as u64));
        // Nothing else is left in the folder.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_download_that_does_not_match_is_deleted() {
        let bytes = vec![7u8; 5000];
        let base = host(
            bytes.clone(),
            format!("{}  Akidev-0.1.4-arm64.dmg", "00".repeat(32)),
        )
        .await;
        let dir = temp_dir("bad");
        let error = download_with(&update_for(&base, &bytes), &dir, |_| true, |_, _| {})
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("published hash"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn the_real_allowlist_refuses_a_local_server_and_odd_names() {
        let bytes = vec![1u8; 10];
        let base = host(bytes.clone(), String::new()).await;
        let dir = temp_dir("refused");
        let error = download(&update_for(&base, &bytes), &dir, |_, _| {})
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not a place updates are downloaded from")
        );

        let mut odd = update_for(&base, &bytes);
        odd.asset_name = "../evil".into();
        assert!(
            download_with(&odd, &dir, |_| true, |_, _| {})
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
