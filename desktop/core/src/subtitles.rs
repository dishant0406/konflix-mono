//! Subtitles from outside the video: which exist for an episode or a film, and the file itself as
//! WebVTT, which a browser and a media player both read.
//!
//! Where they come from is data (`sources::SubtitleDef`, `subtitles` in `sources.json`), the way
//! indexers and providers are, so a service that moves or a new one is a change to the definitions
//! and not a release. This module is the one engine that follows them.
//!
//! A subtitle has to be for this episode of this show, so a service that is keyed by IMDb id is only
//! asked when there is one. An anime has none: its title is looked up by name where the definition
//! says how, and only when the show is short and is not a later season, because such a service
//! numbers the episodes of a long or later-season show differently and the subtitles it gave would
//! be for another episode. Better none than the wrong ones.

use crate::cache::{EMPTY_TTL, cached};
use crate::calendar::now_ms;
use crate::definition::{Method, SubtitleDef};
use crate::expression::{Env, Row, evaluate, matches, pattern, select};
use crate::http::client;
use crate::model::Kind;
use crate::sources;
use crate::template::Fill;
use anyhow::{Context, bail};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(30 * 60);
/// What an answer or a subtitle file may weigh. A real subtitle is a few hundred kilobytes.
const MAX_ANSWER_BYTES: usize = 4 << 20;
const MAX_FILE_BYTES: usize = 3 << 20;
/// The most subtitles kept per language, and in all, so a long list is a menu and not a wall.
const PER_LANGUAGE: usize = 6;
const MOST: usize = 80;

/// What a subtitle is looked up for.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub kind: Kind,
    pub imdb_id: Option<String>,
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// How many episodes the show has, when it is known.
    pub total_episodes: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Srt,
    Ass,
    Vtt,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Subtitle {
    /// Names the source and the subtitle in it, for asking for the file.
    pub id: String,
    /// The `name` of the source.
    pub provider: String,
    /// `en`, `pt-BR`, `zh-TW`: two letters where there are two, and the region where it matters.
    pub lang: String,
    /// "English", "Portuguese (Brazil)".
    pub language: String,
    /// What the viewer reads: usually the release it was made for.
    pub label: String,
    pub url: String,
    pub format: Option<Format>,
    pub hearing_impaired: bool,
}

// ---------------------------------------------------------------------------------------------
// Languages
// ---------------------------------------------------------------------------------------------

/// (codes the sources use, the code shown, the name). The three-letter forms come in two
/// spellings for some languages ("fre" and "fra"), and some sources use a region.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["en", "eng"], "en", "English"),
    (&["es", "spa"], "es", "Spanish"),
    (&["fr", "fre", "fra"], "fr", "French"),
    (&["de", "ger", "deu"], "de", "German"),
    (&["it", "ita"], "it", "Italian"),
    (&["pt", "por"], "pt", "Portuguese"),
    (&["pob", "pt-br", "ptbr"], "pt-BR", "Portuguese (Brazil)"),
    (&["ru", "rus"], "ru", "Russian"),
    (&["ja", "jpn"], "ja", "Japanese"),
    (&["ko", "kor"], "ko", "Korean"),
    (&["zh", "chi", "zho", "zhc"], "zh", "Chinese"),
    (
        &["zht", "zh-tw", "zh-hant"],
        "zh-TW",
        "Chinese (Traditional)",
    ),
    (&["ar", "ara"], "ar", "Arabic"),
    (&["hi", "hin"], "hi", "Hindi"),
    (&["tr", "tur"], "tr", "Turkish"),
    (&["pl", "pol"], "pl", "Polish"),
    (&["nl", "dut", "nld"], "nl", "Dutch"),
    (&["sv", "swe"], "sv", "Swedish"),
    (&["no", "nor", "nob"], "no", "Norwegian"),
    (&["da", "dan"], "da", "Danish"),
    (&["fi", "fin"], "fi", "Finnish"),
    (&["el", "gre", "ell"], "el", "Greek"),
    (&["he", "heb"], "he", "Hebrew"),
    (&["hu", "hun"], "hu", "Hungarian"),
    (&["cs", "cze", "ces"], "cs", "Czech"),
    (&["ro", "rum", "ron"], "ro", "Romanian"),
    (&["bg", "bul"], "bg", "Bulgarian"),
    (&["uk", "ukr"], "uk", "Ukrainian"),
    (&["vi", "vie"], "vi", "Vietnamese"),
    (&["th", "tha"], "th", "Thai"),
    (&["id", "ind"], "id", "Indonesian"),
    (&["ms", "may", "msa"], "ms", "Malay"),
    (&["bn", "ben"], "bn", "Bengali"),
    (&["ta", "tam"], "ta", "Tamil"),
    (&["te", "tel"], "te", "Telugu"),
    (&["ur", "urd"], "ur", "Urdu"),
    (&["fa", "per", "fas"], "fa", "Persian"),
    (&["hr", "hrv"], "hr", "Croatian"),
    (&["sr", "srp"], "sr", "Serbian"),
    (&["sl", "slv"], "sl", "Slovenian"),
    (&["sk", "slo", "slk"], "sk", "Slovak"),
    (&["ca", "cat"], "ca", "Catalan"),
    (&["eu", "baq", "eus"], "eu", "Basque"),
    (&["gl", "glg"], "gl", "Galician"),
    (&["et", "est"], "et", "Estonian"),
    (&["lv", "lav"], "lv", "Latvian"),
    (&["lt", "lit"], "lt", "Lithuanian"),
    (&["is", "ice", "isl"], "is", "Icelandic"),
];

/// The code and the name to show for a code a source used, or `None` for a language that is not
/// known (the subtitle is then left out: a menu entry nobody can read is no help).
pub fn language(code: &str) -> Option<(&'static str, &'static str)> {
    let wanted = code.trim().to_lowercase().replace('_', "-");
    LANGUAGES
        .iter()
        .find(|(codes, _, _)| codes.contains(&wanted.as_str()))
        .map(|(_, shown, name)| (*shown, *name))
}

// ---------------------------------------------------------------------------------------------
// Asking
// ---------------------------------------------------------------------------------------------

/// A host that just failed is not asked again for a while.
fn down_until() -> &'static Mutex<HashMap<String, Instant>> {
    static DOWN: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    DOWN.get_or_init(Default::default)
}

fn usable(host: &str) -> bool {
    down_until()
        .lock()
        .unwrap()
        .get(host)
        .is_none_or(|until| *until <= Instant::now())
}

fn imdb(id: Option<&str>) -> Option<String> {
    static SHAPE: OnceLock<Regex> = OnceLock::new();
    let shape = SHAPE.get_or_init(|| Regex::new(r"^tt[0-9]{5,}$").unwrap());
    let value = id?.trim().to_lowercase();
    shape.is_match(&value).then_some(value)
}

async fn get_json(url: &str, limit: Duration) -> anyhow::Result<Value> {
    let body = tokio::time::timeout(limit, async {
        let response = client().get(url).send().await?.error_for_status()?;
        crate::http::read_capped(response, MAX_ANSWER_BYTES).await
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out"))??;
    serde_json::from_slice(&body).context("not a JSON answer")
}

/// The IMDb id of an anime, found by its name where the definition says how, and only for a
/// show where the service's numbering would match (see the module's note).
async fn looked_up(def: &SubtitleDef, query: &Query, limit: Duration) -> Option<String> {
    let lookup = def
        .lookup
        .as_ref()
        .filter(|l| l.kinds.contains(&query.kind))?;
    if let Some(longest) = lookup.max_episodes
        && query.total_episodes.is_none_or(|total| total > longest)
    {
        return None;
    }
    if let Some(skip) = &lookup.skip_title
        && pattern(skip).ok()?.is_match(&query.title)
    {
        return None;
    }
    // Filled in before anything is awaited: the filler is not `Send`.
    let url = {
        let hole = |name: &str| (name == "term").then(|| query.title.clone());
        let fill = Fill {
            kind: query.kind,
            sort: None,
            hole: &hole,
        };
        fill.address(&lookup.url)?
    };
    let key = format!("subtitles:lookup:{url}");
    let value = cached(
        &key,
        |value: &Value| {
            if value.is_null() {
                EMPTY_TTL
            } else {
                Duration::from_secs(24 * 3600)
            }
        },
        || async { get_json(&url, limit).await },
    )
    .await
    .ok()?;
    let first = select(&value, &lookup.rows).into_iter().next()?;
    let found = evaluate(
        &lookup.value,
        Row {
            value: first,
            parent: None,
            kind: query.kind,
        },
    )?;
    imdb(Some(&found))
}

/// The query for one episode or film, from the title's details.
pub fn query_for(detail: &crate::model::Detail, parsed: &crate::media_id::MediaId) -> Query {
    Query {
        kind: parsed.kind,
        imdb_id: detail.imdb_id.clone(),
        title: detail.media.title.clone(),
        year: detail.media.year.and_then(|year| u32::try_from(year).ok()),
        season: parsed.season,
        episode: parsed.episode,
        total_episodes: detail.media.episodes,
    }
}

/// The path and the query string of the request, or `None` when the query lacks something it needs.
fn request_for(
    def: &SubtitleDef,
    query: &Query,
    imdb: Option<String>,
) -> Option<(String, Vec<(String, String)>)> {
    let hole = |name: &str| match name {
        "imdb" => imdb.clone(),
        "term" => Some(query.title.clone()).filter(|title| !title.is_empty()),
        "year" => query.year.map(|year| year.to_string()),
        // An anime has no seasons of its own, and a looked-up show is asked for by its first.
        "season" => Some(query.season.unwrap_or(1).to_string()),
        "episode" => Some(query.episode.unwrap_or(1).to_string()),
        _ => None,
    };
    let fill = Fill {
        kind: query.kind,
        sort: None,
        hole: &hole,
    };
    let Value::String(path) = fill.value(&def.request.path)? else {
        return None;
    };
    let params = def
        .request
        .query
        .iter()
        .map(|(name, value)| match fill.value(value)? {
            Value::String(text) => Some((name.clone(), text)),
            other => Some((name.clone(), other.to_string())),
        })
        .collect::<Option<Vec<_>>>()?;
    Some((path, params))
}

/// What one source has for the query.
async fn ask(def: &SubtitleDef, query: &Query, limit: Duration) -> anyhow::Result<Vec<Subtitle>> {
    let imdb = match imdb(query.imdb_id.as_deref()) {
        Some(id) => Some(id),
        None => looked_up(def, query, limit).await,
    };
    // A request that needs what the query lacks has nothing to ask.
    let Some((path, params)) = request_for(def, query, imdb) else {
        return Ok(Vec::new());
    };
    let hosts: Vec<&String> = def.hosts.iter().filter(|host| usable(host)).collect();
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let key = format!(
        "v{}:subtitles:{}:{path}:{params:?}",
        sources::current().version,
        def.id
    );
    let document = cached(
        &key,
        |document: &Value| {
            let empty = select(document, "").is_empty();
            if empty { EMPTY_TTL } else { TTL }
        },
        || async {
            let mut last = None;
            for host in &hosts {
                let url = format!("{host}{path}");
                let builder = match def.request.method {
                    Method::Get => client().get(&url),
                    Method::Post => client().post(&url),
                };
                let result = tokio::time::timeout(limit, async {
                    let response = builder.query(&params).send().await?.error_for_status()?;
                    crate::http::read_capped(response, MAX_ANSWER_BYTES).await
                })
                .await
                .map_err(|_| anyhow::anyhow!("timed out"))
                .and_then(|inner| inner);
                match result.and_then(|body| {
                    serde_json::from_slice::<Value>(&body).context("not a JSON answer")
                }) {
                    Ok(document) => return Ok(document),
                    Err(error) => {
                        down_until().lock().unwrap().insert(
                            (*host).clone(),
                            Instant::now() + Duration::from_secs(def.cooldown_secs),
                        );
                        tracing::debug!("[subtitles:{}] {host}: {error:#}", def.id);
                        last = Some(error);
                    }
                }
            }
            Err(last.expect("there was at least one host"))
        },
    )
    .await?;
    Ok(read(def, &document, query.kind))
}

/// The subtitles in an answer, as the definition says to read them. A row without an address or
/// a language this program knows is skipped.
pub fn read(def: &SubtitleDef, document: &Value, kind: Kind) -> Vec<Subtitle> {
    let env = Env {
        now_ms: now_ms(),
        active: &[],
    };
    let fields = &def.response.fields;
    let mut found = Vec::new();
    for value in select(document, &def.response.rows) {
        let row = Row {
            value,
            parent: None,
            kind,
        };
        if def
            .response
            .drop
            .iter()
            .any(|condition| matches(condition, row, &env))
        {
            continue;
        }
        let (Some(id), Some(url), Some(code)) = (
            evaluate(&fields.id, row),
            evaluate(&fields.url, row),
            evaluate(&fields.lang, row),
        ) else {
            continue;
        };
        let Some((lang, language)) = language(&code) else {
            continue;
        };
        let format = fields
            .format
            .as_ref()
            .and_then(|expression| evaluate(expression, row))
            .and_then(|text| match text.to_lowercase().as_str() {
                "srt" => Some(Format::Srt),
                "ass" | "ssa" => Some(Format::Ass),
                "vtt" | "webvtt" => Some(Format::Vtt),
                _ => None,
            });
        let hearing_impaired = fields
            .hearing_impaired
            .as_ref()
            .and_then(|expression| evaluate(expression, row))
            .is_some_and(|text| !matches!(text.to_lowercase().as_str(), "" | "false" | "0" | "no"));
        found.push(Subtitle {
            id: format!("{}:{id}", def.id),
            provider: def.name.clone(),
            lang: lang.to_string(),
            language: language.to_string(),
            label: fields
                .label
                .as_ref()
                .and_then(|expression| evaluate(expression, row))
                .unwrap_or_else(|| language.to_string()),
            url,
            format,
            hearing_impaired,
        });
    }
    found
}

/// Every subtitle the sources have for the query: grouped by language (English first, then
/// alphabetically by name), a handful of each, so it can be a menu.
pub async fn find(query: &Query, limit: Duration) -> Vec<Subtitle> {
    let sources = sources::current();
    let serving: Vec<&SubtitleDef> = sources
        .subtitles
        .iter()
        .filter(|def| def.kinds.contains(&query.kind))
        .collect();
    let answers = futures::future::join_all(serving.iter().map(|def| async move {
        match ask(def, query, limit).await {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!("[subtitles:{}] {error:#}", def.id);
                Vec::new()
            }
        }
    }))
    .await;
    arrange(answers.into_iter().flatten().collect())
}

/// One language after another, English first. Within a language the order the sources gave is
/// kept, and a subtitle whose address repeats one already listed is dropped.
pub fn arrange(found: Vec<Subtitle>) -> Vec<Subtitle> {
    let mut seen = std::collections::HashSet::new();
    let mut by_language: Vec<(String, String, Vec<Subtitle>)> = Vec::new();
    for subtitle in found {
        if !seen.insert(subtitle.url.clone()) {
            continue;
        }
        match by_language
            .iter_mut()
            .find(|(lang, _, _)| *lang == subtitle.lang)
        {
            Some((_, _, list)) => list.push(subtitle),
            None => by_language.push((
                subtitle.lang.clone(),
                subtitle.language.clone(),
                vec![subtitle],
            )),
        }
    }
    by_language.sort_by(|a, b| {
        (a.0 != "en")
            .cmp(&(b.0 != "en"))
            .then_with(|| a.1.cmp(&b.1))
    });
    by_language
        .into_iter()
        .flat_map(|(_, _, mut list)| {
            // Files for one disc of a film split in two start their clock again at that disc,
            // so they run an hour out against a whole film: they go last, the rest keep order.
            list.sort_by_key(|subtitle| split_disc(&subtitle.label));
            list.into_iter().take(PER_LANGUAGE)
        })
        .take(MOST)
        .collect()
}

/// Whether a file name says it is one disc or part of a film that was shared in pieces.
fn split_disc(name: &str) -> bool {
    let name = name.to_lowercase();
    let tokens: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    tokens.iter().any(|token| {
        ["cd", "disc", "disk", "part", "pt"].iter().any(|word| {
            token
                .strip_prefix(word)
                .is_some_and(|n| n.parse::<u8>().is_ok())
        })
    }) || tokens.windows(2).any(|pair| {
        ["cd", "disc", "disk", "part", "pt"].contains(&pair[0]) && pair[1].parse::<u8>().is_ok()
    })
}

/// Words that say which cut a subtitle was timed to: the group, the source, the resolution.
fn release_words(name: &str) -> std::collections::HashSet<String> {
    const NOISE: [&str; 12] = [
        "srt", "ass", "vtt", "sub", "eng", "en", "english", "the", "and", "of", "a", "subs",
    ];
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| word.len() > 1 && !NOISE.contains(word))
        .map(str::to_string)
        .collect()
}

/// Puts the subtitles made for the release being played first within each language, so the
/// first pick is timed to the same cut of the film. A subtitle that shares words with the
/// release (its group, "bluray", "1080p") scores up; a split-disc file scores down. The order
/// among equals stays as the sources gave it.
pub fn prefer_release(list: &mut [Subtitle], release: &str) {
    let wanted = release_words(release);
    let score = |subtitle: &Subtitle| -> i64 {
        let shared = release_words(&subtitle.label).intersection(&wanted).count() as i64;
        shared - if split_disc(&subtitle.label) { 100 } else { 0 }
    };
    // Stable, and only inside a language: the list is already one language after another.
    let mut start = 0;
    while start < list.len() {
        let end = list[start..]
            .iter()
            .position(|subtitle| subtitle.lang != list[start].lang)
            .map_or(list.len(), |n| start + n);
        list[start..end].sort_by_key(|subtitle| std::cmp::Reverse(score(subtitle)));
        start = end;
    }
}

// ---------------------------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------------------------

/// Whether a subtitle file may be fetched from this address: HTTPS, on a host some source named,
/// with no credentials in it.
pub fn file_allowed(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    sources::current().subtitles.iter().any(|def| {
        def.file_hosts
            .iter()
            .any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
    })
}

/// A subtitle file as WebVTT, from its address. Refused unless the address is allowed (see
/// `file_allowed`), and every redirect is held to the same rule.
pub async fn fetch(url: &str) -> anyhow::Result<String> {
    if !file_allowed(url) {
        bail!("that subtitle is not on a host this program fetches from");
    }
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(crate::http::USER_AGENT)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 || !file_allowed(attempt.url().as_str()) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .expect("the subtitle client builds")
    });
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = crate::http::read_capped(response, MAX_FILE_BYTES).await?;
    to_vtt(&bytes)
}

/// Text from the bytes of a file, whatever it was saved as: UTF-8 (with or without a byte order
/// mark), UTF-16, or else Windows-1252, which is what most older subtitle files are.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    for (mark, big_endian) in [([0xFF, 0xFE], false), ([0xFE, 0xFF], true)] {
        if let Some(rest) = bytes.strip_prefix(&mark) {
            let units: Vec<u16> = rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    if big_endian {
                        u16::from_be_bytes(*pair)
                    } else {
                        u16::from_le_bytes(*pair)
                    }
                })
                .collect();
            return String::from_utf16_lossy(&units);
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| windows_1252(*byte)).collect(),
    }
}

fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        other => other as char,
    }
}

/// Subtitle bytes as WebVTT. SRT and ASS are converted and WebVTT is cleaned and kept.
pub fn to_vtt(bytes: &[u8]) -> anyhow::Result<String> {
    let text = decode(bytes).replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim_start_matches('\u{feff}');
    let vtt = if text.trim_start().starts_with("WEBVTT") {
        text.to_string()
    } else if text.contains("[Script Info]") || text.contains("Dialogue:") {
        ass_to_vtt(text)
    } else {
        srt_to_vtt(text)
    };
    if !vtt.contains("-->") {
        bail!("no subtitles could be read from the file");
    }
    Ok(vtt)
}

/// Overrides in braces ("{\\an8}") and the markup that some files carry in an SRT, other than the
/// few tags WebVTT shares (`<i>`, `<b>`, `<u>`).
fn clean_text(line: &str) -> String {
    static BRACES: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    let braces = BRACES.get_or_init(|| Regex::new(r"\{[^}]*\}").unwrap());
    let tags = TAGS.get_or_init(|| Regex::new(r"</?(?:font|span|div|p|br)[^>]*>").unwrap());
    let line = braces.replace_all(line, "");
    let line = tags.replace_all(&line, "");
    line.replace('&', "&amp;")
        .replace("&amp;amp;", "&amp;")
        .replace('<', "&lt;")
        // The three tags WebVTT shares are given back.
        .replace("&lt;i>", "<i>")
        .replace("&lt;/i>", "</i>")
        .replace("&lt;b>", "<b>")
        .replace("&lt;/b>", "</b>")
        .replace("&lt;u>", "<u>")
        .replace("&lt;/u>", "</u>")
}

fn srt_to_vtt(text: &str) -> String {
    static TIME: OnceLock<Regex> = OnceLock::new();
    let time = TIME.get_or_init(|| {
        Regex::new(
            r"^\s*(\d{1,2}:\d{2}:\d{2})[,.](\d{1,3})\s*-->\s*(\d{1,2}:\d{2}:\d{2})[,.](\d{1,3})",
        )
        .unwrap()
    });
    let stamp = |clock: &str, millis: &str| {
        let (hours, rest) = clock.split_once(':').unwrap_or(("0", clock));
        format!("{:0>2}:{rest}.{:0<3}", hours, millis)
    };
    let mut out = String::from("WEBVTT\n\n");
    for block in text.split("\n\n") {
        let mut lines = block.lines().filter(|line| !line.trim().is_empty());
        let mut first = lines.next();
        // The cue number above the time is left out: it means nothing in WebVTT.
        if let Some(line) = first
            && !line.contains("-->")
        {
            first = lines.next();
        }
        let Some(first) = first else { continue };
        let Some(found) = time.captures(first) else {
            continue;
        };
        out.push_str(&format!(
            "{} --> {}\n",
            stamp(&found[1], &found[2]),
            stamp(&found[3], &found[4])
        ));
        for line in lines {
            out.push_str(&clean_text(line));
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

fn ass_to_vtt(text: &str) -> String {
    // "Dialogue: layer,start,end,style,name,marginL,marginR,marginV,effect,text": the columns are
    // named by the Format line of the Events section, and the text is everything after the last.
    let mut columns: Vec<String> = Vec::new();
    let mut cues: Vec<(f64, String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(format) = line.strip_prefix("Format:") {
            let names: Vec<String> = format
                .split(',')
                .map(|name| name.trim().to_lowercase())
                .collect();
            if names.contains(&"start".to_string()) {
                columns = names;
            }
        } else if let Some(dialogue) = line.strip_prefix("Dialogue:") {
            let count = columns.len().max(10);
            let parts: Vec<&str> = dialogue.splitn(count, ',').collect();
            let at = |name: &str, fallback: usize| {
                columns
                    .iter()
                    .position(|column| column == name)
                    .unwrap_or(fallback)
            };
            let (Some(start), Some(end), Some(words)) = (
                parts.get(at("start", 1)),
                parts.get(at("end", 2)),
                parts.get(at("text", count - 1)),
            ) else {
                continue;
            };
            let (Some(from), Some(to)) = (ass_time(start), ass_time(end)) else {
                continue;
            };
            let words = words
                .replace("\\N", "\n")
                .replace("\\n", "\n")
                .replace("\\h", " ");
            let words: Vec<String> = words
                .lines()
                .map(clean_text)
                .filter(|line| !line.trim().is_empty())
                .collect();
            if words.is_empty() {
                continue;
            }
            cues.push((from.0, format!("{} --> {}", from.1, to.1), words.join("\n")));
        }
    }
    cues.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = String::from("WEBVTT\n\n");
    for (_, times, words) in cues {
        out.push_str(&format!("{times}\n{words}\n\n"));
    }
    out
}

/// "0:01:02.34" as seconds and as WebVTT's "00:01:02.340".
fn ass_time(text: &str) -> Option<(f64, String)> {
    let mut parts = text.trim().split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds = parts.next()?;
    let (whole, hundredths) = seconds.split_once('.').unwrap_or((seconds, "0"));
    let whole: u64 = whole.parse().ok()?;
    let millis: u64 = format!("{hundredths:0<3}")[..3].parse().ok()?;
    let total = hours as f64 * 3600. + minutes as f64 * 60. + whole as f64 + millis as f64 / 1000.;
    Some((
        total,
        format!("{hours:02}:{minutes:02}:{whole:02}.{millis:03}"),
    ))
}

#[cfg(test)]
mod tests {
    fn named(label: &str, lang: &str) -> Subtitle {
        Subtitle {
            id: label.into(),
            provider: "p".into(),
            lang: lang.into(),
            language: lang.into(),
            label: label.into(),
            url: format!("https://x/{label}"),
            format: None,
            hearing_impaired: false,
        }
    }

    #[test]
    fn a_disc_of_a_split_film_is_recognised_and_other_names_are_not() {
        assert!(split_disc("The_Shawshank_Redemption_(1994).CD1.aFLDp.srt"));
        assert!(split_disc(
            "The Shawshank Redemption SBC AC3 aFLDp CD2.eng.srt"
        ));
        assert!(split_disc("movie.part 2.srt"));
        assert!(split_disc("movie disc2.srt"));
        assert!(!split_disc("Breaking.Bad.S01E01.720p.HDTV.x264-BiA.srt"));
        assert!(!split_disc("Spartacus.2010.srt"));
    }

    #[test]
    fn split_disc_files_go_last_and_the_played_release_goes_first() {
        let arranged = arrange(vec![
            named("Movie.CD2.srt", "en"),
            named("Movie.DVDRip.XviD-AAA.srt", "en"),
            named("Movie.1080p.BluRay.x264-BBB.srt", "en"),
        ]);
        assert_eq!(arranged[2].label, "Movie.CD2.srt");
        let mut list = arranged;
        prefer_release(&mut list, "Movie.2000.1080p.BluRay.x264-BBB");
        assert_eq!(list[0].label, "Movie.1080p.BluRay.x264-BBB.srt");
        assert_eq!(list[2].label, "Movie.CD2.srt");
    }

    #[test]
    fn preferring_a_release_keeps_languages_apart() {
        let mut list = vec![
            named("a.srt", "en"),
            named("b.BBB.srt", "en"),
            named("c.BBB.srt", "fr"),
            named("d.srt", "fr"),
        ];
        prefer_release(&mut list, "x.BBB");
        let langs: Vec<_> = list.iter().map(|s| s.lang.as_str()).collect();
        assert_eq!(langs, ["en", "en", "fr", "fr"]);
        assert_eq!(list[0].label, "b.BBB.srt");
        assert_eq!(list[2].label, "c.BBB.srt");
    }
    use super::*;
    use serde_json::json;

    fn opensubtitles() -> SubtitleDef {
        sources::current()
            .subtitles
            .iter()
            .find(|def| def.id == "opensubtitles")
            .expect("the definitions have OpenSubtitles")
            .clone()
    }

    #[test]
    fn a_languages_codes_are_normalised_and_an_unknown_one_is_not_guessed() {
        assert_eq!(language("eng"), Some(("en", "English")));
        assert_eq!(language("EN"), Some(("en", "English")));
        assert_eq!(language("fre"), Some(("fr", "French")));
        assert_eq!(language("fra"), Some(("fr", "French")));
        assert_eq!(language("pob"), Some(("pt-BR", "Portuguese (Brazil)")));
        assert_eq!(language("zh_TW"), Some(("zh-TW", "Chinese (Traditional)")));
        assert_eq!(language("xx"), None);
        assert_eq!(language(""), None);
    }

    #[test]
    fn an_answer_is_read_the_way_the_definition_says() {
        let answer = json!({ "subtitles": [
            { "id": "30949", "url": "https://subs5.strem.io/en/download/a.srt", "lang": "eng",
              "subtitleFileName": "Show.S01E01.srt", "movieReleaseName": "Show S01" },
            { "id": "1", "url": "https://subs5.strem.io/b.srt", "lang": "pob", "movieReleaseName": "Show S01" },
            { "id": "2", "url": "https://subs5.strem.io/c.srt", "lang": "xxx" },
            { "id": "3", "lang": "eng" },
        ]});
        let found = read(&opensubtitles(), &answer, Kind::Series);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "opensubtitles:30949");
        assert_eq!(
            (found[0].lang.as_str(), found[0].language.as_str()),
            ("en", "English")
        );
        assert_eq!(found[0].label, "Show.S01E01.srt");
        assert_eq!(found[0].provider, "OpenSubtitles");
        assert_eq!(found[1].lang, "pt-BR");
        assert_eq!(found[1].label, "Show S01");
    }

    fn subtitle(lang: &str, language: &str, url: &str) -> Subtitle {
        Subtitle {
            id: url.into(),
            provider: "p".into(),
            lang: lang.into(),
            language: language.into(),
            label: "l".into(),
            url: url.into(),
            format: None,
            hearing_impaired: false,
        }
    }

    #[test]
    fn languages_are_grouped_english_first_and_each_is_capped() {
        let mut found = vec![
            subtitle("fr", "French", "u1"),
            subtitle("de", "German", "u2"),
        ];
        for n in 0..10 {
            found.push(subtitle("en", "English", &format!("e{n}")));
        }
        found.push(subtitle("fr", "French", "u1")); // the same file again
        let list = arrange(found);
        let langs: Vec<&str> = list.iter().map(|s| s.lang.as_str()).collect();
        assert_eq!(langs.iter().filter(|l| **l == "en").count(), PER_LANGUAGE);
        assert_eq!(langs[0], "en");
        // By language name after English: French, German.
        let rest: Vec<&str> = langs.iter().copied().filter(|l| *l != "en").collect();
        assert_eq!(rest, ["fr", "de"]);
    }

    #[test]
    fn an_srt_becomes_webvtt() {
        let srt = "1\n00:00:01,500 --> 00:00:03,000\nHello, <i>there</i>.\n\n2\n00:01:05,042 --> 01:00:00,100\n{\\an8}Line one\nLine two & more\n\n";
        let vtt = to_vtt(srt.as_bytes()).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n00:00:01.500 --> 00:00:03.000\nHello, <i>there</i>.\n"));
        assert!(vtt.contains("00:01:05.042 --> 01:00:00.100\nLine one\nLine two &amp; more\n"));
        assert!(!vtt.contains("{\\an8}"));
        assert!(!vtt.contains(",042"));
    }

    #[test]
    fn an_ass_file_becomes_webvtt_in_time_order() {
        let ass = "[Script Info]\nTitle: x\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:10.00,0:00:12.50,Default,,0,0,0,,Second {\\i1}cue{\\i0}\\Nover two lines\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,First, with a comma\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\p1}m 0 0 l 1 1{\\p0}\n";
        let vtt = to_vtt(ass.as_bytes()).unwrap();
        let first = vtt.find("First, with a comma").unwrap();
        let second = vtt.find("Second").unwrap();
        assert!(first < second);
        assert!(vtt.contains("00:00:10.000 --> 00:00:12.500\nSecond cue\nover two lines"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:02.000\nFirst, with a comma"));
    }

    #[test]
    fn webvtt_is_kept_and_text_that_is_no_subtitle_is_refused() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n";
        assert_eq!(to_vtt(vtt.as_bytes()).unwrap(), vtt);
        assert!(to_vtt(b"<html>not a subtitle</html>").is_err());
        assert!(to_vtt(b"").is_err());
    }

    #[test]
    fn bytes_are_decoded_whatever_they_were_saved_as() {
        assert_eq!(decode("héllo".as_bytes()), "héllo");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        // "é" in Windows-1252, and a curly quote in its high range.
        assert_eq!(decode(&[b'c', b'a', b'f', 0xE9, 0x92]), "café\u{2019}");
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("hé".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode(&utf16), "hé");
    }

    #[test]
    fn only_https_files_on_a_listed_host_are_fetched() {
        assert!(file_allowed("https://subs5.strem.io/en/download/a.srt"));
        assert!(!file_allowed("http://subs5.strem.io/a.srt"));
        assert!(!file_allowed("https://evil.example/strem.io"));
        assert!(!file_allowed("https://strem.io.evil.example/a.srt"));
        assert!(!file_allowed("https://u:p@subs5.strem.io/a.srt"));
        assert!(!file_allowed("file:///etc/passwd"));
        assert!(!file_allowed("not a url"));
    }

    #[test]
    fn a_later_season_or_a_long_anime_is_not_looked_up() {
        let def = opensubtitles();
        let lookup = def.lookup.expect("OpenSubtitles looks anime up");
        let skip = pattern(lookup.skip_title.as_deref().unwrap()).unwrap();
        for later in [
            "Attack on Titan Season 3",
            "My Hero Academia 2nd Season",
            "Show Part 2",
            "Show Final Season",
            "Bleach Cour 2",
        ] {
            assert!(skip.is_match(later), "{later}");
        }
        for first in [
            "Attack on Titan",
            "Death Note",
            "Demon Slayer: Kimetsu no Yaiba",
        ] {
            assert!(!skip.is_match(first), "{first}");
        }
        assert_eq!(lookup.max_episodes, Some(30));
    }

    fn query(kind: Kind, imdb: Option<&str>) -> Query {
        Query {
            kind,
            imdb_id: imdb.map(str::to_string),
            title: "Show".into(),
            year: Some(2008),
            season: Some(2),
            episode: Some(5),
            total_episodes: Some(12),
        }
    }

    #[test]
    fn the_request_is_filled_in_for_each_kind_and_empty_without_an_id() {
        let def = opensubtitles();
        let ask = |kind, imdb| request_for(&def, &query(kind, imdb), imdb.map(str::to_string));
        assert_eq!(
            ask(Kind::Movie, Some("tt0111161")).unwrap().0,
            "/subtitles/movie/tt0111161.json"
        );
        assert_eq!(
            ask(Kind::Series, Some("tt0903747")).unwrap().0,
            "/subtitles/series/tt0903747:2:5.json"
        );
        // An anime is asked for as the first season: its episodes number on from one.
        assert_eq!(
            ask(Kind::Anime, Some("tt2560140")).unwrap().0,
            "/subtitles/series/tt2560140:1:5.json"
        );
        // Nothing to ask without an id, rather than asking for "tt" or for everything.
        assert!(ask(Kind::Series, None).is_none());
    }

    #[test]
    fn an_imdb_id_has_to_look_like_one() {
        assert_eq!(imdb(Some(" TT0903747 ")).as_deref(), Some("tt0903747"));
        for bad in ["", "0903747", "tt12", "tt09a3747", "../../x"] {
            assert_eq!(imdb(Some(bad)), None, "{bad}");
        }
        assert_eq!(imdb(None), None);
    }
}
