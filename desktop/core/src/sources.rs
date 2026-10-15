//! Where titles and releases come from, as data.
//!
//! Indexers move, change their address or die often, so what is searched and how is a JSON
//! document (`sources/sources.json` in the repository) rather than code. The app ships with
//! one copy and fetches a newer one in the background. A downloaded copy is used only if it
//! carries a valid signature, a higher version than the one in use, and passes `validate`;
//! otherwise the app keeps what it has. Nothing here runs downloaded code: a definition only
//! says which requests to make and which fields to read.

use crate::definition::{IndexerDef, ProviderDef};
use crate::model::Kind;
use anyhow::{Context, ensure};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

/// The version of the definition format this build understands. A document with another
/// number is ignored, so an old app is never fed definitions it would misread.
pub const SCHEMA: u64 = 1;

/// Where the signed document is published.
const FEED_URL: &str = "https://sources.akidev.live";

/// The key the published document is signed with. The matching private key is kept by
/// whoever publishes, never in this repository.
const PUBLIC_KEY: &str = "e0776a868d11f70f94e6aaee3a28205cdc15002c246e97811b3100c4d4a04bbe";

const EMBEDDED: &str = include_str!("../../sources/sources.json");

/// A document is a few tens of kilobytes; anything much larger is not one.
const MAX_BYTES: usize = 1 << 20;

const MAX_VERSION: u64 = 1 << 40;

#[derive(Debug, Clone, Deserialize)]
pub struct Sources {
    pub schema: u64,
    /// Rises with every publish. A document that does not beat the one in use is ignored,
    /// so an old copy replayed from a cache cannot take definitions back.
    pub version: u64,
    /// Public trackers added to every magnet. Many indexers return none.
    pub trackers: Vec<String>,
    pub indexers: Vec<IndexerDef>,
    pub providers: Vec<ProviderDef>,
    /// Which providers answer for each kind, in order.
    pub chains: BTreeMap<Kind, Vec<String>>,
}

impl Sources {
    pub fn indexer(&self, id: &str) -> Option<&IndexerDef> {
        self.indexers.iter().find(|indexer| indexer.id == id)
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderDef> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    pub fn chain(&self, kind: Kind) -> &[String] {
        self.chains
            .get(&kind)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Rejects a document that parses but would break a search: the cheapest place to
    /// notice a typo is before it is installed.
    fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.schema == SCHEMA,
            "definitions use format {}, this app reads {SCHEMA}",
            self.schema
        );
        // A version near the top of the range could never be replaced.
        ensure!(self.version <= MAX_VERSION, "the version is out of range");
        ensure!(self.trackers.len() <= 32, "too many trackers");
        ensure!(
            (1..=64).contains(&self.indexers.len()),
            "the number of indexers is out of range"
        );
        ensure!(
            (1..=64).contains(&self.providers.len()),
            "the number of providers is out of range"
        );
        for (index, indexer) in self.indexers.iter().enumerate() {
            ensure!(
                self.indexers[..index]
                    .iter()
                    .all(|other| other.id != indexer.id),
                "indexer {} appears twice",
                indexer.id
            );
            indexer
                .check()
                .with_context(|| format!("indexer {}", indexer.id))?;
        }
        for (index, provider) in self.providers.iter().enumerate() {
            ensure!(
                self.providers[..index]
                    .iter()
                    .all(|other| other.id != provider.id),
                "provider {} appears twice",
                provider.id
            );
            provider
                .check()
                .with_context(|| format!("provider {}", provider.id))?;
        }
        for kind in Kind::ALL {
            let chain = self.chain(kind);
            ensure!(
                (1..=8).contains(&chain.len()),
                "the providers for {} are out of range",
                kind.as_str()
            );
            ensure!(
                chain
                    .iter()
                    .enumerate()
                    .all(|(at, id)| !chain[..at].contains(id)),
                "{} names a provider twice",
                kind.as_str()
            );
            for id in chain {
                let provider = self.provider(id);
                ensure!(
                    provider.is_some(),
                    "{} names an unknown provider {id}",
                    kind.as_str()
                );
                ensure!(
                    provider.is_some_and(|provider| provider.kinds.contains(&kind)),
                    "provider {id} does not serve {}",
                    kind.as_str()
                );
            }
        }
        Ok(())
    }
}

/// Parses and validates a document. Used for the embedded copy, a downloaded one and the
/// publishing tool, so all three accept exactly the same thing.
pub fn parse(bytes: &[u8]) -> anyhow::Result<Sources> {
    ensure!(bytes.len() <= MAX_BYTES, "the document is too large");
    let sources: Sources = serde_json::from_slice(bytes).context("reading the definitions")?;
    sources.validate()?;
    Ok(sources)
}

/// Bytes from hex text. Only hex digits are accepted, so no input can make this slice
/// through a character.
pub fn decode_hex(text: &str) -> anyhow::Result<Vec<u8>> {
    let text = text.trim();
    ensure!(
        text.len().is_multiple_of(2) && text.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "not hex"
    );
    // Every byte is an ASCII digit here, so slicing in twos cannot split a character.
    (0..text.len())
        .step_by(2)
        .map(|at| Ok(u8::from_str_radix(&text[at..at + 2], 16)?))
        .collect()
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Checks a signature made with the publisher's key.
pub fn verify(bytes: &[u8], signature_hex: &str) -> anyhow::Result<()> {
    verify_with(PUBLIC_KEY, bytes, signature_hex)
}

fn verify_with(key_hex: &str, bytes: &[u8], signature_hex: &str) -> anyhow::Result<()> {
    let key: [u8; 32] = decode_hex(key_hex)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("bad key"))?;
    let signature: [u8; 64] = decode_hex(signature_hex)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("bad signature length"))?;
    VerifyingKey::from_bytes(&key)?
        .verify_strict(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| anyhow::anyhow!("the signature does not match"))
}

fn in_use() -> &'static RwLock<Arc<Sources>> {
    static IN_USE: OnceLock<RwLock<Arc<Sources>>> = OnceLock::new();
    IN_USE.get_or_init(|| {
        RwLock::new(Arc::new(
            parse(EMBEDDED.as_bytes()).expect("the embedded definitions are valid"),
        ))
    })
}

/// The definitions in use right now.
pub fn current() -> Arc<Sources> {
    in_use().read().unwrap().clone()
}

/// Makes a downloaded document the one in use, if it is newer. Returns whether it was.
fn adopt(sources: Sources) -> bool {
    let mut slot = in_use().write().unwrap();
    if sources.version <= slot.version {
        return false;
    }
    *slot = Arc::new(sources);
    true
}

const DOCUMENT: &str = "sources.json";
const SIGNATURE: &str = "sources.sig";

/// Uses the copy saved by an earlier run, when it is still valid and newer than the one
/// built in. It is checked again on every start, so a damaged file does no harm.
pub fn load_saved(dir: &Path) {
    load_saved_with(PUBLIC_KEY, dir)
}

fn load_saved_with(key: &str, dir: &Path) {
    let read = || -> anyhow::Result<Sources> {
        let bytes = std::fs::read(dir.join(DOCUMENT))?;
        verify_with(key, &bytes, &std::fs::read_to_string(dir.join(SIGNATURE))?)?;
        parse(&bytes)
    };
    match read() {
        Ok(sources) => {
            tracing::info!("[sources] saved copy, version {}", sources.version);
            adopt(sources);
        }
        Err(error) if dir.join(DOCUMENT).exists() => {
            tracing::warn!("[sources] ignoring the saved copy: {error:#}")
        }
        Err(_) => {}
    }
}

/// Writes a file so that a crash leaves the old one or the new one, never half of one.
fn save(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// How often a running app looks for a newer document.
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// After a failure (offline, a half-published pair of files) it looks again sooner.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Looks for a newer document now and then for as long as the app runs. A failure (offline,
/// the host down) only means the definitions in use stay in use.
pub async fn keep_fresh(dir: std::path::PathBuf) {
    loop {
        let wait = match refresh(&dir).await {
            Ok(_) => CHECK_EVERY,
            Err(error) => {
                tracing::warn!("[sources] not updated: {error:#}");
                RETRY_AFTER
            }
        };
        tokio::time::sleep(wait).await;
    }
}

/// Fetches the published document and, if it is genuine and newer, starts using it and saves
/// it for the next run. Returns the version now in use.
pub async fn refresh(dir: &Path) -> anyhow::Result<u64> {
    refresh_from(FEED_URL, PUBLIC_KEY, dir).await
}

async fn refresh_from(feed: &str, key: &str, dir: &Path) -> anyhow::Result<u64> {
    let client = crate::http::client();
    let fetch = |name: &'static str| async move {
        let response = client
            .get(format!("{feed}/{name}"))
            .send()
            .await?
            .error_for_status()?;
        anyhow::Ok(crate::http::read_capped(response, MAX_BYTES).await?)
    };
    let (document, signature) =
        futures::future::try_join(fetch(DOCUMENT), fetch(SIGNATURE)).await?;
    let signature = std::str::from_utf8(&signature).context("the signature is not text")?;
    verify_with(key, &document, signature)?;
    let sources = parse(&document)?;
    let version = sources.version;
    if version <= current().version {
        return Ok(current().version);
    }
    // Saved before it is used, so what is in use is always what the next run will find.
    std::fs::create_dir_all(dir)?;
    save(&dir.join(DOCUMENT), &document)?;
    save(&dir.join(SIGNATURE), signature.as_bytes())?;
    adopt(sources);
    tracing::info!("[sources] updated to version {version}");
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::{Value, json};

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn key_hex() -> String {
        encode_hex(signing_key().verifying_key().as_bytes())
    }

    fn embedded_json() -> Value {
        serde_json::from_str(EMBEDDED).unwrap()
    }

    /// The embedded document with `change` applied, as bytes.
    fn changed(change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut document = embedded_json();
        change(&mut document);
        serde_json::to_vec(&document).unwrap()
    }

    #[test]
    fn the_embedded_definitions_are_valid_and_complete() {
        let sources = parse(EMBEDDED.as_bytes()).unwrap();
        assert_eq!(sources.indexers.len(), 9);
        assert_eq!(sources.providers.len(), 4);
        assert!(!sources.trackers.is_empty());
        for kind in Kind::ALL {
            assert!(!sources.chain(kind).is_empty());
        }
        // The public key is a real one and not a placeholder.
        assert!(PUBLIC_KEY.bytes().any(|byte| byte != b'0'));
        assert!(
            VerifyingKey::from_bytes(&decode_hex(PUBLIC_KEY).unwrap().try_into().unwrap()).is_ok()
        );
    }

    #[test]
    fn a_document_that_would_break_a_search_is_refused() {
        let refused = |what: &str, bytes: Vec<u8>| {
            assert!(parse(&bytes).is_err(), "{what} was accepted");
        };
        refused("a newer format", changed(|d| d["schema"] = json!(2)));
        refused(
            "a repeated indexer id",
            changed(|d| d["indexers"][1]["id"] = json!("subsplease")),
        );
        refused(
            "an indexer with no host",
            changed(|d| d["indexers"][0]["hosts"] = json!([])),
        );
        refused(
            "a host that is not http",
            changed(|d| d["indexers"][0]["hosts"] = json!(["ftp://x"])),
        );
        refused(
            "a bad pattern",
            changed(|d| d["indexers"][4]["response"]["dedupe"]["avoid"] = json!("(unclosed")),
        );
        refused(
            "a misspelt hole",
            changed(|d| d["indexers"][1]["request"]["query"]["q"] = json!("{tem}")),
        );
        refused(
            "a chain naming nobody",
            changed(|d| d["chains"]["anime"] = json!(["nobody"])),
        );
        refused(
            "a kind with no provider",
            changed(|d| d["chains"]["movie"] = json!([])),
        );
        refused(
            "a provider address that is not http",
            changed(|d| d["providers"][2]["base_url"] = json!("tvmaze")),
        );
        refused("no indexers", changed(|d| d["indexers"] = json!([])));
        refused(
            "an answer reused for ever",
            changed(|d| d["providers"][1]["list"]["ttl_secs"] = json!(u64::MAX)),
        );
        refused(
            "a request that leaves the service",
            changed(|d| {
                d["providers"][2]["search"]["request"]["url"] = json!("https://evil.example/{term}")
            }),
        );
        refused(
            "a request that lets the term change the host",
            changed(|d| d["providers"][2]["search"]["request"]["url"] = json!("{base}{term}")),
        );
        refused(
            "a base address with braces",
            changed(|d| d["providers"][2]["base_url"] = json!("https://x.example/{")),
        );
        refused(
            "pages too small to read in a few requests",
            changed(|d| d["providers"][1]["list"]["paging"]["size"] = json!(1)),
        );
        refused(
            "a page of no rows",
            changed(|d| d["providers"][1]["list"]["paging"]["per_page"] = json!(0)),
        );
        refused(
            "a window of something that cannot be asked",
            changed(|d| d["providers"][1]["list"]["paging"]["window"]["when"] = json!(["gnere"])),
        );
        refused(
            "days that overflow",
            changed(|d| {
                d["providers"][2]["list"]["drop"][1]["after_today"]["grace_days"] =
                    json!(99_999_999)
            }),
        );
        refused(
            "a search that counts pages of no known size",
            changed(|d| {
                d["providers"][3]["search"]
                    .as_object_mut()
                    .unwrap()
                    .remove("per_page");
            }),
        );
        refused(
            "a list that needs the genre nobody asked for",
            changed(|d| d["providers"][2]["list"]["request"]["url"] = json!("{base}/{genre}")),
        );
        refused(
            "a provider twice in a chain",
            changed(|d| d["chains"]["series"] = json!(["cinemeta", "cinemeta"])),
        );
        refused(
            "a bad pattern in a related list",
            changed(|d| {
                d["providers"][0]["detail"]["related"]["drop"][0]["path"] =
                    json!({ "regex": { "from": "type", "pattern": "(" } })
            }),
        );
        refused(
            "an endless rest",
            changed(|d| d["indexers"][0]["cooldown_secs"] = json!(u64::MAX)),
        );
        refused(
            "a version that cannot be replaced",
            changed(|d| d["version"] = json!(u64::MAX)),
        );
        refused(
            "a provider on the wrong kind",
            changed(|d| d["chains"]["series"] = json!(["yts"])),
        );
        refused("not JSON", b"nope".to_vec());
        refused("an oversized document", vec![b' '; MAX_BYTES + 1]);
    }

    #[test]
    fn fields_a_newer_format_adds_are_ignored() {
        let bytes = changed(|d| {
            d["something_new"] = json!(true);
            d["indexers"][0]["something_new"] = json!(1);
        });
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn a_signature_is_valid_only_for_the_exact_document_and_key() {
        let document = b"the document";
        let signature = encode_hex(&signing_key().sign(document).to_bytes());
        assert!(verify_with(&key_hex(), document, &signature).is_ok());
        assert!(verify_with(&key_hex(), b"the document.", &signature).is_err());
        assert!(verify_with(&key_hex(), document, &signature[2..]).is_err());
        assert!(verify_with(&key_hex(), document, "not hex").is_err());
        // Text that is not ASCII is refused, not sliced through.
        assert!(verify_with(&key_hex(), document, "a€").is_err());
        assert!(decode_hex("é9").is_err());
        let other = SigningKey::from_bytes(&[8; 32]);
        let forged = encode_hex(&other.sign(document).to_bytes());
        assert!(verify_with(&key_hex(), document, &forged).is_err());
    }

    /// Serves a signed document, and counts nothing else: only the two files.
    async fn feed(document: Vec<u8>, signature: String) -> String {
        use axum::{Router, routing::get};
        let app = Router::new()
            .route("/sources.json", get(move || async move { document }))
            .route("/sources.sig", get(move || async move { signature }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("akidev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn a_newer_signed_document_is_adopted_saved_and_reloaded_but_never_rolled_back() {
        let dir = temp_dir("sources");
        let in_use = current().version;

        // A genuine, newer document is used and saved.
        let newer = changed(|d| {
            d["version"] = json!(in_use + 10);
            d["trackers"] = json!(["udp://new.example:1"]);
        });
        let signed = encode_hex(&signing_key().sign(&newer).to_bytes());
        let base = feed(newer.clone(), signed.clone()).await;
        assert_eq!(
            refresh_from(&base, &key_hex(), &dir).await.unwrap(),
            in_use + 10
        );
        assert_eq!(current().trackers, ["udp://new.example:1"]);
        assert!(dir.join(DOCUMENT).exists());

        // An older one that is genuine is not taken: a replayed copy cannot undo an update.
        let older = changed(|d| d["version"] = json!(in_use + 5));
        let signed_older = encode_hex(&signing_key().sign(&older).to_bytes());
        let base = feed(older, signed_older).await;
        assert_eq!(
            refresh_from(&base, &key_hex(), &dir).await.unwrap(),
            in_use + 10
        );

        // A document altered after signing is refused, and nothing changes.
        let forged = changed(|d| d["version"] = json!(in_use + 99));
        let base = feed(forged, signed).await;
        assert!(refresh_from(&base, &key_hex(), &dir).await.is_err());
        assert_eq!(current().version, in_use + 10);

        // The saved copy is used by the next run, and a damaged one is ignored.
        load_saved_with(&key_hex(), &dir);
        assert_eq!(current().version, in_use + 10);
        std::fs::write(dir.join(DOCUMENT), b"damaged").unwrap();
        load_saved_with(&key_hex(), &dir);
        assert_eq!(current().version, in_use + 10);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
