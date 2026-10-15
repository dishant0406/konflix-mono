//! The Internet Archive, as a last-resort source.
//!
//! It is not a torrent index and not a debrid service: it is a public library that stores the
//! file and serves it over plain HTTPS. It is here for one measured reason: it is the only place a
//! regional series can be found when no torrent indexer has it at all. Its own h264 derivative has
//! real range support, so this source joins no swarm, downloads nothing and converts nothing: it
//! returns a direct URL and the bytes go from archive.org to the viewer.
//!
//! It is tried last, so it only ever answers when nothing else can.

use akidev_core::cache;
use akidev_core::http::client;
use akidev_core::releases::{is_bonus, parse_episode, parse_season, title_variants};
use anyhow::{Context, bail};
use serde_json::Value;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(30 * 60);
/// How many items a search may return before the per-item lookups begin. Each costs a request.
const MAX_ITEMS: usize = 6;
/// How many title spellings are searched. The bare-title forms come first.
const MAX_QUERIES: usize = 5;
/// Formats archive.org derives from an upload, best first. A derivative beats the original
/// because the original may be a format no browser plays, while these are h264 in MP4.
const PLAYABLE_FORMATS: [&str; 6] = [
    "h.264 IA",
    "MPEG4",
    "h.264",
    "512Kb MPEG4",
    "Ogg Video",
    "WebM",
];
/// The shortest a file can be and still be an episode rather than a clip. Measured on the items
/// this source returns: real episodes are 2000s and up, the clip was 74s.
const MIN_EPISODE_SECONDS: f64 = 300.;
/// The fallback when an item reports no duration.
const MIN_EPISODE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveFile {
    pub name: String,
    pub size: u64,
    pub format: String,
    pub seconds: Option<f64>,
}

pub struct Found {
    pub title: String,
    pub url: String,
}

/// Whether a search result is plausibly the title that was asked for. Compared with every
/// non-alphanumeric character removed from both sides, which is what makes "India's Got Latent",
/// "Indias Got Latent" and "India s Got Latent" equal: archive.org stores an item under whichever
/// spelling its uploader used.
pub fn matches_title(name: &str, titles: &[String]) -> bool {
    let wanted: Vec<String> = titles
        .iter()
        .flat_map(|title| title_variants(title))
        .collect();
    if wanted.is_empty() {
        return true;
    }
    let squash = |text: &str| -> String {
        text.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    };
    let item = squash(name);
    wanted.iter().any(|title| item.contains(&squash(title)))
}

/// The files worth offering for one item, best first. archive.org lists every derivative beside
/// the original, the thumbnails and its own metadata files; only a playable format with a size is
/// a candidate.
pub fn playable_files(metadata: &Value) -> Vec<ArchiveFile> {
    let mut found: Vec<ArchiveFile> = metadata
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| {
                    let format = file.get("format")?.as_str()?.to_string();
                    let name = file.get("name")?.as_str()?.to_string();
                    // Sizes and lengths come as strings, and sometimes not at all.
                    let number = |key: &str| -> Option<f64> {
                        let value = file.get(key)?;
                        value
                            .as_f64()
                            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
                    };
                    let size = number("size").filter(|size| *size > 0.)? as u64;
                    if name.is_empty() || !PLAYABLE_FORMATS.contains(&format.as_str()) {
                        return None;
                    }
                    Some(ArchiveFile {
                        name,
                        size,
                        format,
                        seconds: number("length"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let rank = |format: &str| {
        PLAYABLE_FORMATS
            .iter()
            .position(|f| *f == format)
            .unwrap_or(usize::MAX)
    };
    found.sort_by(|a, b| {
        rank(&a.format)
            .cmp(&rank(&b.format))
            .then(b.size.cmp(&a.size))
    });
    found
}

/// Whether a file is long enough to be an episode. Duration is preferred and size is the
/// fallback, because a duration is only reported for some items.
pub fn is_episode_length(file: &ArchiveFile) -> bool {
    match file.seconds {
        Some(seconds) => seconds >= MIN_EPISODE_SECONDS,
        None => file.size >= MIN_EPISODE_BYTES,
    }
}

/// `Some(true)` when a name identifies the episode asked for, `Some(false)` when it names a
/// different one, `None` when it names no episode and so cannot be checked. The season is only
/// enforced when one is given: an anime's later seasons restart at episode 1.
pub fn identifies_episode(name: &str, episode: u32, season: Option<u32>) -> Option<bool> {
    if let Some(named) = parse_episode(name) {
        if named != episode {
            return Some(false);
        }
        if let (Some(wanted), Some(named_season)) = (season, parse_season(name))
            && named_season != wanted
        {
            return Some(false);
        }
        return Some(true);
    }
    // No episode number: a season alone can only rule a name out, never confirm it.
    if let (Some(wanted), Some(named_season)) = (season, parse_season(name))
        && named_season != wanted
    {
        return Some(false);
    }
    None
}

/// Picks the file to serve, or `None` when the episode cannot be confirmed. Serving the wrong
/// episode silently is worse than serving nothing, so an item is only accepted when its title or
/// the file itself names the requested episode. That is what stops commentary being served as an
/// episode: a news clip holds one video that names no episode.
pub fn pick_file<'a>(
    item_title: &str,
    files: &'a [ArchiveFile],
    episode: Option<u32>,
    season: Option<u32>,
) -> Option<&'a ArchiveFile> {
    if files.is_empty() || is_bonus(item_title) {
        return None;
    }
    // A film has no episode number, so nothing can be checked and the best file wins.
    let Some(episode) = episode else {
        return files.first();
    };
    let mut usable = files.iter().filter(|file| !is_bonus(&file.name));
    match identifies_episode(item_title, episode, season) {
        Some(false) => None,
        Some(true) => {
            usable.find(|file| identifies_episode(&file.name, episode, season) != Some(false))
        }
        None => usable.find(|file| identifies_episode(&file.name, episode, season) == Some(true)),
    }
}

pub fn download_url(base: &str, identifier: &str, name: &str) -> String {
    format!("{base}/download/{}/{}", encode(identifier), encode(name))
}

/// Percent-encodes a path segment.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// archive.org identifiers are alphanumeric with dots, dashes and underscores.
fn identifier_ok(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

async fn search_items(
    base: &str,
    term: &str,
    left: Duration,
) -> anyhow::Result<Vec<(String, String)>> {
    let response = client()
        .get(format!("{base}/advancedsearch.php"))
        .query(&[
            ("q", format!("title:(\"{term}\") AND mediatype:movies")),
            // `fl[]` repeated: the indexed form is accepted silently and returns rows with no
            // identifier, which made a matched title look like nothing found.
            ("fl[]", "identifier".to_string()),
            ("fl[]", "title".to_string()),
            ("rows", (MAX_ITEMS * 3).to_string()),
            ("output", "json".to_string()),
        ])
        .timeout(left.max(Duration::from_millis(250)))
        .send()
        .await?
        .error_for_status()?;
    let data: Value = response.json().await?;
    let docs = data
        .pointer("/response/docs")
        .and_then(Value::as_array)
        .context("not a search response")?;
    Ok(docs
        .iter()
        .filter_map(|doc| {
            let identifier = doc.get("identifier")?.as_str()?;
            identifier_ok(identifier).then(|| {
                (
                    identifier.to_string(),
                    doc.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            })
        })
        .collect())
}

async fn item_metadata(base: &str, identifier: &str, left: Duration) -> anyhow::Result<Value> {
    let response = client()
        .get(format!("{base}/metadata/{}", encode(identifier)))
        .timeout(left.max(Duration::from_millis(250)))
        .send()
        .await?
        .error_for_status()?;
    let data: Value = response.json().await?;
    if !data.is_object() {
        bail!("not a metadata response");
    }
    Ok(data)
}

/// Looks the title up directly rather than choosing among indexer results, which is what lets it
/// answer when the indexers found nothing at all.
pub async fn find(
    base: &str,
    titles: &[String],
    terms: &[String],
    episode: Option<u32>,
    season: Option<u32>,
    deadline: Instant,
) -> Result<Found, String> {
    let left = || deadline.saturating_duration_since(Instant::now());

    // archive.org's phrase search matches the whole title and its items are named after the show,
    // so searching the show name and reading the episode out of the results is both more accurate
    // and far fewer requests than searching per episode. Every spelling is kept: each apostrophe
    // spelling returns a different set of items, and the union is what finds the episodes.
    let mut candidates: Vec<String> = Vec::new();
    for query in titles.iter().chain(terms) {
        if !query.is_empty() && !candidates.contains(query) {
            candidates.push(query.clone());
        }
    }
    // A title-only form has no digits at all.
    let (bare, rest): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|query| !query.chars().any(|c| c.is_ascii_digit()));
    let queries: Vec<String> = bare.into_iter().chain(rest).take(MAX_QUERIES).collect();
    if queries.is_empty() {
        return Err("No title to look up on archive.org.".into());
    }

    let mut failures: Vec<String> = Vec::new();
    // Every (item, file) pair that identifies the episode, so the best can be chosen and not
    // merely the first found.
    let mut found: Vec<(String, String, ArchiveFile)> = Vec::new();

    for query in &queries {
        if left().is_zero() {
            break;
        }
        let docs = cache::cached(
            &format!("archive-org:search:{query}"),
            |docs: &Vec<(String, String)>| {
                if docs.is_empty() {
                    cache::EMPTY_TTL
                } else {
                    TTL
                }
            },
            || search_items(base, query, left()),
        )
        .await;
        let docs = match docs {
            Ok(docs) => docs,
            Err(error) => {
                failures.push(format!("{query}: {error:#}"));
                continue;
            }
        };

        // Every item is checked: returning on the first match served a 74-second clip for an
        // episode whose real upload was listed second.
        for (identifier, title) in docs.iter().take(MAX_ITEMS) {
            if left().is_zero() {
                failures.push("deadline reached".into());
                break;
            }
            if !matches_title(title, std::slice::from_ref(query)) {
                continue;
            }
            let metadata = cache::cached(
                &format!("archive-org:item:{identifier}"),
                |_| TTL,
                || item_metadata(base, identifier, left()),
            )
            .await;
            let metadata = match metadata {
                Ok(metadata) => metadata,
                Err(error) => {
                    failures.push(format!("{identifier}: {error:#}"));
                    continue;
                }
            };
            let files = playable_files(&metadata);
            if let Some(file) = pick_file(title, &files, episode, season) {
                found.push((identifier.clone(), title.clone(), file.clone()));
            }
        }

        // Stop once a real episode has been found. A clip does not stop the search.
        if found.iter().any(|(_, _, file)| is_episode_length(file)) {
            break;
        }
    }

    // A file too short to be an episode is only used when nothing longer exists.
    let full: Vec<&(String, String, ArchiveFile)> = found
        .iter()
        .filter(|(_, _, file)| is_episode_length(file))
        .collect();
    let pool: Vec<&(String, String, ArchiveFile)> = if full.is_empty() {
        found.iter().collect()
    } else {
        full
    };
    let chosen = pool.into_iter().max_by(|a, b| {
        a.2.seconds
            .unwrap_or(0.)
            .total_cmp(&b.2.seconds.unwrap_or(0.))
            .then(a.2.size.cmp(&b.2.size))
    });
    if let Some((identifier, title, file)) = chosen {
        return Ok(Found {
            title: title.clone(),
            url: download_url(base, identifier, &file.name),
        });
    }
    if !failures.is_empty() {
        tracing::warn!("[archive-org] {}", failures.join(" | "));
    }
    Err(format!("Nothing on archive.org for \"{}\".", queries[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(name: &str) -> ArchiveFile {
        ArchiveFile {
            name: name.into(),
            size: 400 << 20,
            format: "h.264 IA".into(),
            seconds: Some(1400.),
        }
    }

    #[test]
    fn three_spellings_of_one_title_are_the_same_title() {
        let titles = vec!["India's Got Latent".to_string()];
        assert!(matches_title("INDIA S GOT LATENT S 2 EP 6", &titles));
        assert!(matches_title("Indias Got Latent (2026)", &titles));
        assert!(!matches_title("Some Other Show", &titles));
        assert!(matches_title("anything", &[]));
    }

    #[test]
    fn only_a_playable_format_with_a_size_is_offered_best_first() {
        let metadata = json!({ "files": [
            { "name": "a.mkv", "format": "Matroska", "size": "100" },
            { "name": "a.mp4", "format": "MPEG4", "size": "300", "length": "1400" },
            { "name": "a_512kb.mp4", "format": "512Kb MPEG4", "size": "50" },
            { "name": "a.ia.mp4", "format": "h.264 IA", "size": "200" },
            { "name": "empty.mp4", "format": "MPEG4", "size": "0" },
            { "name": "thumb.jpg", "format": "JPEG", "size": "9" },
        ]});
        let names: Vec<String> = playable_files(&metadata)
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, ["a.ia.mp4", "a.mp4", "a_512kb.mp4"]);
    }

    #[test]
    fn a_clip_is_not_an_episode() {
        let mut clip = file("clip.mp4");
        clip.seconds = Some(74.);
        assert!(!is_episode_length(&clip));
        assert!(is_episode_length(&file("ep.mp4")));
        let mut unknown = file("x.mp4");
        unknown.seconds = None;
        unknown.size = 6 << 20;
        assert!(!is_episode_length(&unknown));
        unknown.size = 188 << 20;
        assert!(is_episode_length(&unknown));
    }

    #[test]
    fn a_name_that_names_another_episode_is_ruled_out() {
        assert_eq!(identifies_episode("Show S 2 EP 5", 5, None), Some(true));
        assert_eq!(identifies_episode("Show EP 6", 5, None), Some(false));
        assert_eq!(identifies_episode("Show S 1 EP 5", 5, Some(2)), Some(false));
        assert_eq!(identifies_episode("Show", 5, None), None);
        assert_eq!(identifies_episode("Show S 1", 5, Some(2)), Some(false));
    }

    #[test]
    fn an_item_that_names_no_episode_needs_a_file_that_does() {
        let files = vec![file("news_package.mp4")];
        // One video proves nothing about which episode it is.
        assert!(pick_file("India's Got Latent Row", &files, Some(5), None).is_none());
        let files = vec![file("show_ep_5.mp4")];
        assert!(pick_file("India's Got Latent", &files, Some(5), None).is_some());
    }

    #[test]
    fn an_item_for_the_right_episode_skips_mislabelled_files() {
        let files = vec![file("show_ep_6.mp4"), file("show_ep_5.mp4")];
        let picked = pick_file("Show EP 5", &files, Some(5), None).unwrap();
        assert_eq!(picked.name, "show_ep_5.mp4");
        assert!(pick_file("Show EP 6", &files[..1], Some(5), None).is_none());
    }

    #[test]
    fn a_bonus_is_not_the_episode_it_is_numbered_as() {
        let files = vec![file("bonus_ep_4.mp4")];
        assert!(pick_file("Show Bonus Ep 04", &files, Some(4), None).is_none());
    }

    #[test]
    fn a_film_takes_the_best_file() {
        let files = vec![file("film.ia.mp4"), file("film.mp4")];
        assert_eq!(
            pick_file("Some Film", &files, None, None).unwrap().name,
            "film.ia.mp4"
        );
    }

    #[test]
    fn the_download_url_is_encoded() {
        assert_eq!(
            download_url("https://archive.org", "igl-s2", "IGL EP 5 (x).mp4"),
            "https://archive.org/download/igl-s2/IGL%20EP%205%20%28x%29.mp4"
        );
    }

    #[test]
    fn an_identifier_is_a_plain_token() {
        assert!(identifier_ok("igl-s2_06.v1"));
        assert!(!identifier_ok("../etc"));
        assert!(!identifier_ok("a b"));
        assert!(!identifier_ok(""));
    }
}
