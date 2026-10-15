//! Release parsing and ranking.
//!
//! Everything here is a pure function over indexer results, so it can be tested
//! without touching the network or the torrent client.
//!
//! Indexer titles look like:
//!   [SubsPlease] One Piece - 1080 (1080p) [ABC].mkv
//!   Naruto Shippuden 163 Remaster (BDRip 1080p x265 FLAC Multi) - Ryujin
//!   NARUTO.S01.1080p.CR.WEB-DL.AAC2.0.H.264-Kitsune
//!   One Piece - Box 05 [BDRip 1080p x264 AC3]
//!
//! The patterns are the ones the Node implementation used, and they depend on
//! JavaScript's regex semantics: `\b`, `\w` and `\d` are ASCII-only there, while the
//! `regex` crate defaults to Unicode. `pattern` therefore pins both to ASCII, so a
//! letter such as "é" next to a number is a boundary exactly as it was.
//! The two patterns that used look-behind are matched by hand in `bare_number`.

use std::sync::LazyLock;

use regex::Regex;

use crate::model::Release;

/// Compiles a pattern written with JavaScript's ASCII `\b` and `\d`.
fn pattern(source: &str) -> Regex {
    let source = source.replace(r"\b", r"(?-u:\b)").replace(r"\d", "[0-9]");
    Regex::new(&source).expect("release pattern is valid")
}

static VIDEO_EXTENSIONS: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\.(mkv|mp4|avi|m4v|mov|webm)$"));

pub fn is_video_file(name: &str) -> bool {
    VIDEO_EXTENSIONS.is_match(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Hevc,
    H264,
    Av1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audio {
    Lossless,
    Compressed,
}

static RESOLUTION: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)(?:^|[^\d])(\d{3,4})p"));

pub fn parse_resolution(title: &str) -> Option<u32> {
    // Anchored so the digits are not part of a longer number, and so that trailing
    // tags such as the "10" in "1080p10bit" are not mistaken for the resolution.
    RESOLUTION
        .captures(title)
        .and_then(|found| found[1].parse().ok())
}

static HEVC: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\b(x265|h\.?265|hevc)\b"));
static H264: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\b(x264|h\.?264|avc)\b"));
static AV1: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\b(av1)\b"));

pub fn parse_codec(title: &str) -> Option<Codec> {
    if HEVC.is_match(title) {
        Some(Codec::Hevc)
    } else if H264.is_match(title) {
        Some(Codec::H264)
    } else if AV1.is_match(title) {
        Some(Codec::Av1)
    } else {
        None
    }
}

static LOSSLESS_AUDIO: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)(flac|dts|truehd|ac3|eac3|ddp)"));
static COMPRESSED_AUDIO: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)(aac|mp3|opus)"));

/// Substring matching is intentional here: release names write these without word
/// boundaries ("AAC2.0", "DTS-HD", "x264-10bit").
pub fn parse_audio(title: &str) -> Option<Audio> {
    if LOSSLESS_AUDIO.is_match(title) {
        Some(Audio::Lossless)
    } else if COMPRESSED_AUDIO.is_match(title) {
        Some(Audio::Compressed)
    } else {
        None
    }
}

/// Words that introduce a collection or a part number rather than an episode
/// number, for example "Season 01 Part 1 [1080p]". "Special" is included because
/// "Show S01 Special 1 1080p" is a special numbered 1, not episode 1.
static PART_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)\b(?:part|vol\.?|volume|season|set|box|cour|movies?|films?|specials?)\s*$")
});

static EPISODE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // SxxExx, the standard naming for WEB-DL releases. Must come first, because
        // "S01E1158" has no word boundary before the E for the pattern below.
        pattern(r"(?i)\bS\d{1,2}E(\d{1,4})\b"),
        pattern(r"(?i)\bE(?:P)?\.?\s?(\d{1,4})\b"),
        pattern(r"(?i)\bEpisode\s+(\d{1,4})\b"),
        pattern(r"\s-\s(\d{1,4})(?:\s|\[|\(|\.|$)"),
        pattern(r"第\s?(\d{1,4})\s?[话話]"),
        pattern(r"\b(\d{1,4})\s*(?:v\d)?\s*\["),
    ]
});

/// The first number in `title` that the JS look-behind/look-ahead patterns accepted.
///
/// Both of those patterns had the shape `(?<![forbidden])\b(\d{1,4})(?![\dpP])` and
/// then a tail: a number is a whole run of at most four digits, preceded by a
/// character that is neither a word character nor in `forbidden`, and not followed by
/// a `p` (so "1080p" is never a number). `brackets` selects the tail: a bracket after
/// optional spaces and an optional "v2" revision (fansub style, "1104 (WEB 1080p)"), or
/// else at least one space (the bare form, "163 Remaster").
///
/// Returns the byte index the number starts at, and its value.
fn bare_number(title: &str, forbidden: &[u8], brackets: bool) -> Option<(usize, u32)> {
    let bytes = title.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let run = start..end;
        start = end;

        // A longer run cannot be a number: the look-ahead rejects every shorter prefix.
        if run.len() > 4 {
            continue;
        }
        if run.start > 0 {
            let before = bytes[run.start - 1];
            let word = before.is_ascii_alphanumeric() || before == b'_';
            if word || forbidden.contains(&before) {
                continue;
            }
        }
        let rest = &title[run.end..];
        if rest.starts_with(['p', 'P']) {
            continue;
        }
        let tail_matches = if brackets {
            let rest = rest.trim_start_matches(char::is_whitespace);
            let rest = match rest.strip_prefix('v') {
                Some(revision) if revision.starts_with(|c: char| c.is_ascii_digit()) => {
                    revision[1..].trim_start_matches(char::is_whitespace)
                }
                _ => rest,
            };
            rest.starts_with(['[', '('])
        } else {
            rest.starts_with(char::is_whitespace)
        };
        if tail_matches {
            return title[run.clone()]
                .parse()
                .ok()
                .map(|number| (run.start, number));
        }
    }
    None
}

/// Tries the episode pattern at `step` (in priority order) against the title.
fn episode_candidate(step: usize, title: &str) -> Option<(usize, u32)> {
    match step {
        0..=5 => EPISODE_PATTERNS[step].captures(title).and_then(|found| {
            let whole = found.get(0).expect("group 0 always exists");
            found[1].parse().ok().map(|number| (whole.start(), number))
        }),
        // Fansub style with the episode number bare before a bracketed quality tag:
        // "[HatSubs] One Piece 1104 (WEB 1080p)". Without this the release parsed as
        // "no episode number", which made episode 1104 an eligible candidate for
        // episode 1. The look-behind keeps decimals ("5.1") from matching, and the
        // look-ahead keeps resolutions ("1080p") from matching.
        6 => bare_number(title, b".-", true),
        // The bare form, where the number is followed by a space and a word:
        // "Naruto Shippuden 163 Remaster (BDRip 1080p)" and "One Piece 1000 WEB-DL".
        // Without this the release parsed as "no episode number", which put it in the
        // fallback bucket where it could be served for any episode. Letters that
        // introduce a collection ("S01", "Part 1") are excluded by the look-behind.
        _ => bare_number(title, b".-(Ss", false),
    }
}

/// Returns the episode number a release refers to, or None when the release is a
/// batch/season pack that does not identify a single episode.
pub fn parse_episode(title: &str) -> Option<u32> {
    // Release names and file names both separate words with underscores, and a word
    // boundary does not exist around an underscore because it is a regex word character.
    // Without this, "IGL_EP_01_ft_RaftaarMusic.mp4" parsed as *no* episode number, so a
    // torrent holding the right episode looked like one that named none - which made the
    // whole Season 1 pack unusable and is why this show reported "no source" while a
    // 47-seeder copy of every episode sat in the indexers.
    //
    // Only the underscore is replaced. Dots must be left alone, because they are what
    // keeps a decimal together: rewriting them to spaces turns "AAC2.0" into "AAC2 0" and
    // "5.1" into "5 1", and the bare-number pattern then reads that trailing digit as an
    // episode number. parse_season and is_batch normalise both separators through
    // words(), which is safe for them because neither has a bare-number pattern.
    let title = title.replace('_', " ");
    for step in 0..8 {
        let Some((start, number)) = episode_candidate(step, &title) else {
            continue;
        };
        // "One Piece - Season 01 Part 1 [1080p]" is a collection, not episode 1, so a
        // number introduced by one of these words is not treated as an episode.
        if PART_WORDS.is_match(&title[..start]) {
            continue;
        }
        return Some(number);
    }
    None
}

static SEASON: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)\bS(?:eason\s*)?\s*(\d{1,2})(?:\s*(?:EP|E|Episode)\s*\d{1,4})?\b")
});

/// The season a release names, or None.
///
/// Series releases use "S01E05", and the season is part of the identity: "S02E05" is
/// a different episode from "S01E05". The anime path ignores the season because its
/// episode ids are a flat count, but the series path matches on both.
///
/// A bare season token is matched as well as a full "SxxExx", because a season *pack*
/// names only the season ("Gyaarah Gyaarah S01 [1080p ...]"). Reading that as "no
/// season" is what let a season-2 pack satisfy a request for a season-1 episode.
///
/// The digits are bounded so a resolution ("1080p") and a year ("S2020") are not read
/// as a season: "\b" fails after the "20" of "S2020" because the next character is
/// another digit, and "Season 1" is matched through its word.
///
/// Spaces are allowed around every part of the token, because archive.org writes the
/// season out longhand and with gaps: "INDIA S GOT LATENT S 2 EP 6", "Panchayat S 03 E 1 8",
/// "Mirzapur. S 01. E 01". Reading those as "no season" is not a cosmetic problem - a
/// release whose season cannot be read passes the season gate vacuously, so a season-2
/// file would satisfy a request for a season-1 episode.
pub fn parse_season(title: &str) -> Option<u32> {
    SEASON
        .captures(&words(title))
        .and_then(|found| found[1].parse().ok())
}

static SEPARATORS: LazyLock<Regex> = LazyLock::new(|| pattern(r"[._]+"));

/// Release names separate words with spaces, dots and underscores. Underscore is a
/// regex word character, so "\bmovie\b" would not match "_Movie_"; normalising the
/// separators first makes the word-boundary checks below behave as intended.
fn words(title: &str) -> String {
    SEPARATORS.replace_all(title, " ").into_owned()
}

// An episode range ("Show 1-162", "[1-79]") is a batch just as much as the word
// "batch" is, and picking one episode out of a range is not reliable.
//
// Every separator release groups actually use is included, not just the hyphen:
// "[Erai-raws] Show - 01 ~ 12 [1080p]" is a 12-episode batch, and reading it as
// episode 1 meant a whole season was offered as a single episode.
static RANGE: LazyLock<Regex> = LazyLock::new(|| pattern(r"\d{1,4}\s*[-~–—]\s*\d{1,4}\b"));
static SEASON_TOKEN: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bs\d{1,2}\b|\[s\d+\]"));
static NAMES_AN_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\bs\d{1,2}\b[\s\S]*\b(?:e|ep|episode)\s?\d{1,4}\b"));
static BATCH_WORDS: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b(batch|complete|season\s*\d+|box\s*\d+|set\s*\d+)\b"));

pub fn is_batch(title: &str) -> bool {
    let text = words(title);

    // A bare season token ("Show S01 [1080p]") is how a season *pack* is named. It is
    // only a pack signal when the name does not also name an episode, though: "S02E01"
    // contains "S02" but is a single episode, and treating it as a pack subtracted 30
    // from its score, which dropped it below zero and discarded it. That is what made
    // "Indias Got Latent (2026)-MovieLinkHub.com-S02 Bouns E01-E01" rank no candidates
    // at all, even though every other gate accepted it.
    //
    // "Also names an episode" is tested loosely - a season token followed by an episode
    // token anywhere later in the name - because real release names insert words between
    // them ("S02 Bouns E01-E01", "S01 WEB-DL EP 05"). Requiring them to be adjacent
    // missed those, and the season token was then read as a pack.
    if BATCH_WORDS.is_match(&text) || RANGE.is_match(&text) {
        return true;
    }
    SEASON_TOKEN.is_match(&text) && !NAMES_AN_EPISODE.is_match(&text)
}

static EXTRA: LazyLock<Regex> = LazyLock::new(|| {
    // Plurals are included because release names use them freely ("Naruto Movies",
    // "OVAs", "Specials").
    pattern(
        r"(?i)\b(movies?|films?|ovas?|onas?|specials?|recaps?|extras?|trailers?|promos?|previews?|compilations?|nced|ncop|openings?|endings?)\b",
    )
});

/// A release that is part of the franchise but not the series itself: a movie, an
/// OVA, a recap, or a trailer. These name no episode number, so without this check
/// they pass the episode filter for free and can outrank real episodes - searching
/// "Naruto 1" returned "Naruto Shippuden - The Movie" first.
pub fn is_extra(title: &str) -> bool {
    EXTRA.is_match(&words(title))
}

static BONUS: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)\b(bonus|discarded|deleted|behind\s*the\s*scenes|bloopers?|outtakes?)\b")
});

/// Whether a release is a bonus/extra rather than the numbered episode it claims.
///
/// "Bonus" is deliberately not part of is_extra above, because the two cases are not the
/// same. A release titled "Indias Got Latent ( 2026) S 02 Bonus Ep 04" parses as episode
/// 4 and names season 2, so every existing gate accepted it and it was served as the
/// real episode 4 - an episode that does not exist in that upload. A bonus is separate
/// content that happens to be numbered, so it must never satisfy a request for the
/// episode with that number.
///
/// "Discarded" and "Deleted" are the same shape: extras numbered like episodes.
pub fn is_bonus(title: &str) -> bool {
    BONUS.is_match(&words(title))
}

static BRACKETED_YEAR: LazyLock<Regex> = LazyLock::new(|| pattern(r"\((?:19|20)\d{2}\)"));

/// A film is often named without the word "movie" ("One Piece - Film Red 2022",
/// "Baron Omatsuri and the Secret Island (2005)"). A year in brackets is the signal:
/// episode releases do not carry one, and this is only consulted for releases that
/// name no episode at all.
pub fn is_likely_movie(title: &str) -> bool {
    BRACKETED_YEAR.is_match(&words(title))
}

/// Codecs the old browser player could handle. HEVC and AV1 are absent on purpose:
/// they failed in most browsers and were excluded from the candidate list entirely.
/// Kept because it still gates `rank_releases` and `rank_movie_releases`, so the
/// candidate list stays the same one the Node server produced.
pub fn is_playable_codec(title: &str) -> bool {
    matches!(parse_codec(title), None | Some(Codec::H264))
}

static SOURCE_TAG: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b(web-?dl|web-?rip|bdrip|bluray)\b"));

/// The quality signals a release carries, independent of whether it is the right shape
/// for the request. Shared by the episode scorer and the pack scorer so the two cannot
/// drift apart on what "better quality" means.
fn quality_score(title: &str) -> f64 {
    let mut score = 0.0;

    // 1080p is the sweet spot for streaming: high quality without the very large
    // files of 2160p, which take much longer to buffer from a torrent.
    match parse_resolution(title) {
        Some(1080) => score += 30.0,
        Some(720) => score += 20.0,
        Some(2160) => score += 15.0,
        Some(480) => score += 10.0,
        Some(resolution) if resolution < 480 => score -= 10.0,
        _ => {}
    }

    if parse_codec(title) == Some(Codec::H264) {
        score += 15.0;
    }

    match parse_audio(title) {
        Some(Audio::Compressed) => score += 10.0,
        Some(Audio::Lossless) => score -= 5.0, // needs an audio transcode
        None => {}
    }

    if SOURCE_TAG.is_match(title) {
        score += 5.0;
    }

    score
}

/// Seeders only break ties, so a well-seeded 480p release never beats a 1080p one.
fn seeder_tiebreak(seeders: u32) -> f64 {
    f64::from(seeders.min(100)) / 100.0
}

/// Ranks a release for a specific episode. Higher is better.
///
/// Reached by the named and fallback groups in `rank_releases`, so a release naming no
/// episode is scored too - which is what the batch and no-episode penalties below are
/// for. The season-pack group has its own scorer (see `score_packs`).
pub fn score_release(release: &Release) -> f64 {
    let title = &release.title;
    let batch = is_batch(title);

    let mut score = quality_score(title);

    // A release that names no episode is only acceptable as a single-file torrent,
    // which the stream layer verifies. A batch is a last resort, because picking the
    // right file out of one is not reliable.
    if parse_episode(title).is_none() && batch {
        score -= 10.0;
    }

    if batch {
        score -= 30.0;
    } else {
        score += 20.0;
    }

    score + seeder_tiebreak(release.seeders)
}

static NON_WORD: LazyLock<Regex> = LazyLock::new(|| pattern(r"[^\p{L}\p{N}\s]"));
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| pattern(r"\s+"));

pub fn normalize_title(text: &str) -> String {
    let lowered = text.to_lowercase();
    let spaced = NON_WORD.replace_all(&lowered, " ");
    WHITESPACE.replace_all(&spaced, " ").trim().to_string()
}

const APOSTROPHES: [char; 2] = ['\'', '\u{2019}'];

/// Every spelling of a title that a release name might use.
///
/// An apostrophe is written three different ways in practice and the variants do not
/// match each other:
///
///   "India's Got Latent"   the catalogue spelling
///   "Indias Got Latent"    scene naming, which drops the apostrophe entirely
///   "India s Got Latent"   what normalize_title produces, because it turns every
///                          non-alphanumeric character into a space
///
/// Only the last form was ever compared, so the first two - the ones real releases
/// actually use - were rejected by the relevance gate, and the search terms built from
/// them ("India s Got Latent S02E01") matched nothing at any indexer. A title with an
/// apostrophe was therefore unusable end to end: "no torrent found" while releases with
/// the correct name sat in the indexers.
///
/// Titles without an apostrophe have exactly one variant, so nothing changes for them.
pub fn title_variants(text: &str) -> Vec<String> {
    let mut forms = vec![normalize_title(text)];
    if text.contains(APOSTROPHES) {
        let dropped: String = text.chars().filter(|c| !APOSTROPHES.contains(c)).collect();
        let spaced: String = text
            .chars()
            .map(|c| if APOSTROPHES.contains(&c) { ' ' } else { c })
            .collect();
        for form in [normalize_title(&dropped), normalize_title(&spaced)] {
            if !forms.contains(&form) {
                forms.push(form);
            }
        }
    }
    forms.retain(|form| !form.is_empty());
    forms
}

static LEADING_TAGS: LazyLock<Regex> = LazyLock::new(|| pattern(r"^(\[[^\]]*\]\s*)+"));

/// Release names usually begin with group and quality tags such as
/// "[SubsPlease]" or "[Group] ". Those are stripped before comparing titles.
/// This runs on the raw title, because normalizing removes the brackets.
fn strip_leading_tags(release: &str) -> &str {
    let rest = LEADING_TAGS
        .find(release)
        .map_or(release, |tags| &release[tags.end()..]);
    rest.trim()
}

/// A release for the wrong show must never be chosen. Searching for "Naruto" also
/// returns "Boruto - Naruto Next Generations", and searching for "One Piece"
/// returns "One Piece Fan Letter", so plain containment is not enough.
///
/// The gate is: the requested title must appear at the start of the release name,
/// followed by a word boundary. Everything after it (episode number, quality tags,
/// fansub group) is free-form.
///
/// Single-word titles are additionally allowed to appear inside a bracketed group
/// tag, because some groups prefix the show name: "[Naruto] Episode 1".
///
/// Every spelling of the title is tried, because a release that drops an apostrophe
/// ("Indias Got Latent") is the correct release for "India's Got Latent" and would
/// otherwise be rejected here.
pub fn is_relevant(release_title: &str, anime_title: &str) -> bool {
    let wanted = title_variants(anime_title);
    if wanted.is_empty() {
        return true;
    }

    let release = normalize_title(strip_leading_tags(release_title));
    let lowered_raw = release_title.to_lowercase();

    wanted.iter().any(|title| {
        if release == *title || release.starts_with(&format!("{title} ")) {
            return true;
        }
        // Group-prefixed naming, for example "[Naruto] Episode 1". A normalised title
        // holds only letters, digits and spaces, so it is safe to search for literally.
        !title.contains(' ') && lowered_raw.contains(&format!("[{title}]"))
    })
}

static SERIES_EPISODE: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bS\d{1,2}E\d{1,4}\b"));
static TRAILING_NUMBER: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?:^|\s)\d{1,4}\s*$"));

/// A release that names an episode must never be served as a film. Every series
/// release carries an SxxExx or a season token, so their presence is the signal.
///
/// This is the check that matters most for the film path: searching for a film by
/// title also returns its series, and a series episode is not a film.
///
/// A bare number is deliberately NOT enough on its own. A film's title is full of bare
/// numbers - the sequel number ("Dhamaal 4", "Stree 2", "Bhool Bhulaiyaa 3") and the
/// year ("Singham Again 2024") - and parse_episode matches both. Filtering on it threw
/// away 7 of 10 real films, which is why searching for a film reported "no source"
/// while the indexers held 73-seeded releases for it.
///
/// So a bare number counts as an episode only when the release does NOT name a year.
/// The year is what distinguishes the two cases, and it is why the film's own year is
/// passed in:
///
///   "Dhamaal 4 (2026) 720p WEBRip"  year present -> the 4 is a sequel number
///   "Fargo - 05 1080p WEB-DL"       no year      -> the 05 is an episode
///   "Singham Again 2024 1080p"      year present -> the 2024 is the year
///   "Fargo.S01E01.1080p"            SxxExx       -> rejected outright
///
/// A release that names no year and no SxxExx but does carry a bare number is treated
/// as an episode. That is the conservative choice: serving an episode as a film is
/// worse than reporting no source. The trailing-number form is checked separately
/// because parse_episode's bare pattern needs a word after the number, so it misses
/// "Fargo 05" - while a resolution tag ("Dangal 1080p") must still be admitted.
///
/// `year` is the film's own release year, when known.
pub fn is_episode_release(title: &str, year: Option<u32>) -> bool {
    let text = words(title);
    if SERIES_EPISODE.is_match(&text) {
        return true;
    }
    if is_batch(title) {
        return true;
    }
    // A release naming a year is a film release, whatever numbers else it carries.
    if year.is_some() && parse_year(title).is_some() {
        return false;
    }
    if parse_episode(title).is_some() {
        return true;
    }
    TRAILING_NUMBER.is_match(&text)
}

static YEAR: LazyLock<Regex> = LazyLock::new(|| pattern(r"[(\[]?((?:19|20)\d{2})[)\]]?"));

/// The year a release names, or None.
///
/// A film is identified by title *and* year - "Dangal" alone is ambiguous, and
/// remakes share titles. The year appears as "(2016)" or "[2016]" in scene naming and
/// bare in others ("Dangal 2016 1080p").
pub fn parse_year(title: &str) -> Option<u32> {
    YEAR.captures(&words(title))
        .and_then(|found| found[1].parse().ok())
}

static FILM_SOURCE_TAG: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)\b(web-?dl|web-?rip|bdrip|bluray|remux)\b"));

/// Ranks a release for a film. Higher is better.
///
/// Differs from score_release in what it rewards: a film is one large file, so 2160p
/// is not the liability it is for an episode and is scored above 1080p, and there is
/// no episode to match.
pub fn score_movie_release(release: &Release, year: Option<u32>) -> f64 {
    let title = &release.title;
    let mut score = 0.0;

    let release_year = parse_year(title);
    // The right year is the strongest signal available that this is the right film
    // rather than a remake, a sequel or an unrelated title.
    if let Some(year) = year.filter(|year| *year != 0) {
        if release_year == Some(year) {
            score += 40.0;
        } else if release_year.is_some() {
            score -= 10.0;
        }
    }

    match parse_resolution(title) {
        Some(2160) => score += 30.0,
        Some(1080) => score += 28.0,
        Some(720) => score += 18.0,
        Some(480) => score += 8.0,
        Some(resolution) if resolution < 480 => score -= 10.0,
        _ => {}
    }

    if parse_codec(title) == Some(Codec::H264) {
        score += 15.0;
    }

    match parse_audio(title) {
        Some(Audio::Compressed) => score += 10.0,
        Some(Audio::Lossless) => score -= 5.0, // needs an audio transcode
        None => {}
    }

    if FILM_SOURCE_TAG.is_match(title) {
        score += 5.0;
    }

    score + seeder_tiebreak(release.seeders)
}

/// Seeders decide whether a release can be played at all, but they were only a
/// tie-break, so a 2-seeder release outranked a 379-seeder one for the same episode
/// purely because it named its codec. Releases are therefore grouped by how likely
/// they are to stream, and quality (score_release) only orders releases within a
/// group. The thresholds are the ones the Stremio/Torrentio ecosystem publishes.
fn health_tier(seeders: u32) -> u32 {
    match seeders {
        50.. => 0,   // healthy: streams smoothly
        5..=49 => 1, // seeded: works
        1..=4 => 2,  // barely seeded
        0 => 3,      // no seeders reported
    }
}

/// Orders by health tier first, then by score, best first. The sort is stable, so
/// releases that tie keep the order the indexers returned them in.
fn sort_by_health_then_score(scored: &mut [(&Release, f64)]) {
    scored.sort_by(|a, b| {
        health_tier(a.0.seeders)
            .cmp(&health_tier(b.0.seeders))
            .then_with(|| b.1.total_cmp(&a.1))
    });
}

/// Ranks releases for a film.
///
/// Only releases that pass the relevance gate and do not look like an episode are
/// considered. A release naming the requested year is ordered first, so a remake is
/// not served in place of the film that was asked for.
///
/// `year` is the film's release year, or None when unknown. `titles` is every title the
/// film is known by, and `title` is used when it is empty.
pub fn rank_movie_releases(
    releases: &[Release],
    year: Option<u32>,
    title: &str,
    titles: &[String],
) -> Vec<Release> {
    let known = known_titles(title, titles);

    let usable: Vec<&Release> = releases
        .iter()
        .filter(|release| is_playable_codec(&release.title))
        .filter(|release| matches_any_title(&release.title, &known))
        // A series episode is not a film, and this is a hard filter: without it a
        // season pack or a stray episode outranks the film itself. The year is passed
        // because it is what distinguishes a sequel number from an episode number.
        .filter(|release| !is_episode_release(&release.title, year))
        .collect();

    // The year decides the order, but quality and seeders order within each group.
    let by_score = |group: Vec<&Release>| -> Vec<Release> {
        let mut scored: Vec<(&Release, f64)> = group
            .into_iter()
            .map(|release| (release, score_movie_release(release, year)))
            .filter(|(_, score)| *score > 0.0)
            .collect();
        sort_by_health_then_score(&mut scored);
        scored
            .into_iter()
            .map(|(release, _)| release.clone())
            .collect()
    };

    // Only when the year is known: with no year there is nothing to prefer, and
    // treating every release as "not the year" would empty the first group anyway.
    let Some(year) = year.filter(|year| *year != 0) else {
        return by_score(usable);
    };

    let (year_matches, others): (Vec<&Release>, Vec<&Release>) = usable
        .into_iter()
        .partition(|release| parse_year(&release.title) == Some(year));
    let mut ranked = by_score(year_matches);
    ranked.extend(by_score(others));
    ranked
}

/// The titles to compare against: every known title, or the single title when none.
fn known_titles(title: &str, titles: &[String]) -> Vec<String> {
    if titles.is_empty() {
        vec![title.to_string()]
    } else {
        titles.to_vec()
    }
}

/// Rejects a release that names an episode other than the one requested, and for a
/// series also rejects a release from a different season.
///
/// Serving the wrong episode silently is worse than serving nothing, so this is a
/// hard filter rather than a score penalty. For a series the season is part of the
/// episode's identity: "S02E05" must not satisfy a request for "S01E05".
///
/// A bonus is rejected for the same reason and by the same rule. It is numbered like an
/// episode and often names the season too, so nothing else here would catch it.
pub fn matches_episode(release_title: &str, episode: u32, season: Option<u32>) -> bool {
    if is_bonus(release_title) {
        return false;
    }
    if parse_episode(release_title).is_some_and(|named| named != episode) {
        return false;
    }
    let Some(season) = season else {
        return true;
    };

    // A release naming no season is season 1 by convention: a show with more than one
    // season always labels them, and it is season 1 that is written without one. So it
    // can satisfy a season-1 request, but never a later one.
    //
    // Accepting it for any season is what served the first-season pack of "India's Got
    // Latent" for a request for season 2 - the pack is titled with the show name alone,
    // so its season cannot be read, and its files are named "IGL_EP_05" with no season
    // either. Nothing downstream could tell it apart from a season-2 copy, and the wrong
    // episode played. Refusing is the same choice this file makes everywhere else: a
    // wrong episode served silently is worse than an honest "no source".
    match parse_season(release_title) {
        None => season == 1,
        Some(named) => named == season,
    }
}

/// Whether a release belongs to any of the titles a show is known by.
///
/// A show has several names and releases use them inconsistently: an English title
/// ("Attack on Titan") and its romaji ("Shingeki no Kyojin") both appear, so
/// checking a single name rejects correct releases. A title with no comparable
/// characters (a CJK-only name against ASCII release names) cannot gate anything,
/// so it is ignored and the episode number alone decides.
pub fn matches_any_title(release_title: &str, anime_titles: &[String]) -> bool {
    let wanted: Vec<String> = anime_titles
        .iter()
        .flat_map(|title| title_variants(title))
        .collect();
    if wanted.is_empty() {
        return true;
    }

    if wanted.iter().any(|title| is_relevant(release_title, title)) {
        return true;
    }

    // Nothing was comparable, so there is no title signal to use.
    !wanted.iter().any(|title| has_ascii_letter(title))
}

fn has_ascii_letter(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
}

static MULTI_SEASON: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)S\d{1,2}(?:\s*[-~–—]\s*S?|\s+S)\d{1,2}\b"));

/// Whether a season pack holds the requested episode.
///
/// A pack names the season instead of the episode, so it can only be used when its
/// season is the one being watched. Regional and Indian series are released almost
/// entirely as packs ("Gyaarah Gyaarah S01 [1080p ...]", "Panchayat S02 Complete ..."),
/// so without this a series that exists only as a pack reports "no torrent found" while
/// the indexers hold it with 71 seeders.
///
/// A pack spanning several seasons is refused outright. It is many times the size of the
/// one being watched, and its file names restart at 01 for each season, so "S03E05.mkv"
/// is indistinguishable from "S01E05.mkv" by number alone. The anime path has no season
/// at all to disambiguate with, so a later season's pack could not be mapped onto it
/// either: "Shippuden S02" holds episodes 33-53 but names them 01-21, and reading file
/// "01" out of it would serve episode 33 as episode 1.
fn pack_season_matches(title: &str, season: Option<u32>) -> bool {
    // Both separators a range is written with: "S01-S05" and "S01 S05". Underscores and
    // dots have already become spaces by this point, so "Show.S01.S05.Complete" arrives
    // as "Show S01 S05 Complete" and has to be caught by the space form.
    if MULTI_SEASON.is_match(&words(title)) {
        return false;
    }

    let Some(release_season) = parse_season(title) else {
        return false;
    };
    // Only a first-season pack can be mapped onto the anime path, whose episode ids are a
    // flat count starting where season 1 does.
    release_season == season.unwrap_or(1)
}

/// Scores season packs for the last-resort group.
///
/// Deliberately not score_release: that scorer applies a -30 batch penalty, which is
/// right when a pack competes with a real episode but would drive a perfectly good pack
/// below zero and drop it from the list entirely - the exact bug this group exists to
/// fix. The group is already ordered last, so the batch penalty has nothing left to say.
/// What is scored here is the quality the pack actually carries, and the size is
/// subtracted so a pack covering a few episodes is preferred over one covering five
/// seasons.
///
/// There is no positive floor and none is applied: the terse regional release names this
/// group exists for ("Show S01") carry no quality tag at all, and filtering on a positive
/// score would drop exactly those. Every release here already passed the title, codec and
/// season gates, so a low score means "no quality information", not "unusable".
fn score_packs(packs: Vec<&Release>) -> Vec<&Release> {
    const SIZE_CAP: u64 = 64 * 1024 * 1024 * 1024;
    let mut scored: Vec<(&Release, f64)> = packs
        .into_iter()
        .map(|release| {
            let size = release.size_bytes.unwrap_or(0).min(SIZE_CAP);
            let score = quality_score(&release.title) + seeder_tiebreak(release.seeders)
                - size as f64 / SIZE_CAP as f64;
            (release, score)
        })
        .collect();
    sort_by_health_then_score(&mut scored);
    scored.into_iter().map(|(release, _)| release).collect()
}

fn score_all(releases: Vec<&Release>) -> Vec<&Release> {
    let mut scored: Vec<(&Release, f64)> = releases
        .into_iter()
        .map(|release| (release, score_release(release)))
        .filter(|(_, score)| *score > 0.0)
        .collect();
    sort_by_health_then_score(&mut scored);
    scored.into_iter().map(|(release, _)| release).collect()
}

/// Ranks releases for one episode: releases naming it first, then single-file
/// fallbacks, then season packs. `season` is None on the anime path.
///
/// `titles` is every title the show is known by, and `title` is used when it is empty.
pub fn rank_releases(
    releases: &[Release],
    episode: u32,
    title: &str,
    titles: &[String],
    season: Option<u32>,
) -> Vec<Release> {
    let titles = known_titles(title, titles);

    // When no title can be compared against a release name, relevance passes
    // everything, so the episode number becomes the only reliable signal and is
    // therefore mandatory.
    let require_named_episode = !titles
        .iter()
        .any(|title| has_ascii_letter(&normalize_title(title)));

    let usable: Vec<&Release> = releases
        .iter()
        .filter(|release| is_playable_codec(&release.title))
        .filter(|release| matches_any_title(&release.title, &titles))
        .filter(|release| matches_episode(&release.title, episode, season))
        .filter(|release| !require_named_episode || parse_episode(&release.title) == Some(episode))
        .collect();

    // A release that names the requested episode is always the right answer, so those
    // are ordered first. This is what stops a movie or a trailer from being served for
    // a specific episode: they name no episode number, so they can never outrank one.
    let is_named = |release: &Release| parse_episode(&release.title) == Some(episode);

    // Fallbacks name no episode at all and are neither a movie/OVA nor a collection.
    // The stream layer still has to confirm the torrent holds exactly one video file, so
    // a wrong pick is rejected there rather than played.
    let is_fallback = |release: &Release| {
        !is_named(release)
            && !is_extra(&release.title)
            && !is_batch(&release.title)
            && !is_likely_movie(&release.title)
    };

    // A season pack is a last resort, ordered after a release that names the episode and
    // after a single-file fallback. It is admitted only when its season matches, which
    // is what keeps a season-2 pack from satisfying a request for season 1 - the season
    // gate in matches_episode already rejected a mismatched one, and this repeats the
    // check because a pack is the one case where the wrong season is silently playable.
    //
    // Extras are excluded for the same reason the fallback group excludes them: a specials
    // pack ("Show S01 Specials 1080p") names no episode, so it would otherwise be offered
    // for a real episode of that season.
    let is_pack = |release: &Release| {
        !is_named(release)
            && !is_fallback(release)
            && !is_extra(&release.title)
            && !is_likely_movie(&release.title)
            && pack_season_matches(&release.title, season)
    };

    let named: Vec<&Release> = usable.iter().copied().filter(|r| is_named(r)).collect();
    let fallback: Vec<&Release> = usable.iter().copied().filter(|r| is_fallback(r)).collect();
    let packs: Vec<&Release> = usable.iter().copied().filter(|r| is_pack(r)).collect();

    let mut ranked = score_all(named);
    ranked.extend(score_all(fallback));
    ranked.extend(score_packs(packs));
    ranked.into_iter().cloned().collect()
}

// Builds the search terms used against the indexers.
//
// Release groups write the episode number differently and the indexers match
// literally, so a single term finds a fraction of the releases that exist. Measured
// against both indexers, no single form wins:
//
//   "Bocchi the Rock 1"     -> 29 rows,  0 usable
//   "Bocchi the Rock 001"   -> more rows, 7 usable
//   "Jujutsu Kaisen 5"      -> 20 rows,  2 usable
//   "Jujutsu Kaisen S01E05" -> more rows, 31 usable
//
// Searching every form is what turns "no playable source" into a real one, so the
// terms are fanned out and the results merged.
const MAX_TERMS: usize = 8;

/// Collects the terms for a set of titles, applying the cap per title variant.
///
/// The cap is per variant rather than over the whole list, and for the current form
/// counts it does not bind: a series has at most 5 forms and an anime at most 4. It is
/// kept as a bound for a future form list rather than as the thing limiting the fan-out
/// today. What actually bounds the requests is the number of title variants a provider
/// supplies (AniList gives up to 3) multiplied by the form count, times the number of
/// indexers - which is why the fan-out is issued in bounded batches and the indexer
/// timeout exists.
///
/// Capping the whole list instead is what this replaced: the first variant consumed the
/// budget and the rest lost their season and bare-title forms, which are the forms a
/// pack-only series is found by.
fn collect_terms(bases: &[String], forms: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for base in bases {
        let mut added = 0;
        for form in forms {
            let term = if form.is_empty() {
                base.clone()
            } else {
                format!("{base} {form}")
            };
            if terms.contains(&term) {
                continue;
            }
            terms.push(term);
            added += 1;
            if added >= MAX_TERMS {
                break;
            }
        }
    }
    terms
}

fn episode_forms(episode: u32) -> Vec<String> {
    // A three-digit episode is left alone by padding, so long-running shows such as
    // ONE PIECE (1104) keep their real number instead of being truncated.
    vec![
        episode.to_string(),
        format!("{episode:02}"),
        format!("{episode:03}"),
        format!("S01E{episode:02}"),
    ]
}

/// Anime search terms: the title with the episode number in every form releases use,
/// or the bare title when there is no episode (None, or 0).
pub fn search_terms(titles: &[String], episode: Option<u32>) -> Vec<String> {
    let bases = normalize_bases(titles);
    let forms = match episode {
        Some(episode) if episode != 0 => episode_forms(episode),
        _ => vec![String::new()],
    };
    collect_terms(&bases, &forms)
}

/// Series search terms.
///
/// The episode number is written with a season ("S01E05") or without it ("05"), and
/// indexers match literally, so both forms are searched. The season is included
/// because series releases almost always carry it, and without it the search returns
/// every season of the show mixed together.
///
/// The season alone and the bare title are searched last, and they are what make a
/// season-pack-only series findable. Regional and Indian series are released almost
/// entirely as packs ("Gyaarah Gyaarah S01 [1080p ...]", "Mirzapur S01 Hindi 1080p"),
/// and knaben requires every word of a term to match, so "Mirzapur S01E01" returns 0
/// hits where "Mirzapur S01" returns 15 and "Mirzapur" returns 100. They come last so
/// the episode-specific forms still decide the ordering whenever they return anything.
pub fn series_search_terms(
    titles: &[String],
    season: Option<u32>,
    episode: Option<u32>,
) -> Vec<String> {
    let bases = normalize_bases(titles);
    let Some(episode) = episode else {
        return bases;
    };
    let season_tag = format!("S{:02}", season.filter(|season| *season != 0).unwrap_or(1));

    let forms = [
        format!("{season_tag}E{episode:02}"),
        format!("{episode:02}"),
        episode.to_string(),
        season_tag,
        String::new(),
    ];
    collect_terms(&bases, &forms)
}

/// Film search terms.
///
/// The year is what distinguishes a film from its remakes and from its series, so it
/// is searched as part of the term. It is optional: YTS is searched by title and
/// matches loosely, so a term without the year still finds the film.
pub fn movie_search_terms(titles: &[String], year: Option<u32>) -> Vec<String> {
    let bases = normalize_bases(titles);
    let forms = match year {
        Some(year) if year != 0 => vec![year.to_string(), String::new()],
        _ => vec![String::new()],
    };
    collect_terms(&bases, &forms)
}

static NOT_TERM_CHARACTER: LazyLock<Regex> = LazyLock::new(|| pattern(r"[^\p{L}\p{N}\s'-]"));

fn collapse_spaces(text: &str) -> String {
    WHITESPACE.replace_all(text, " ").trim().to_string()
}

/// The comparable form of every title a work is known by.
///
/// Unicode-aware (\p{L}\p{N}) so a CJK-only title survives instead of being stripped to
/// nothing by an ASCII-only \w.
///
/// An apostrophe is emitted in all three spellings, because both the indexers and
/// archive.org match a query literally. Measured against archive.org, each spelling of
/// one show returns a different set and the union is what finds the episodes:
///
///   "Indias Got Latent"      -> the bonus episode and one S2 episode
///   "India's Got Latent"     -> a news clip and a collection
///   "India s Got Latent"     -> the two S2 episodes that actually play
///
/// Searching one spelling only is what made an apostrophe title unfindable. Every other
/// character is treated exactly as before.
fn normalize_bases(titles: &[String]) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    for title in titles {
        let raw = collapse_spaces(&NOT_TERM_CHARACTER.replace_all(title, " "));
        if raw.is_empty() {
            continue;
        }
        for form in [raw.replace('\'', ""), raw.clone(), raw.replace('\'', " ")] {
            let base = collapse_spaces(&form);
            if !base.is_empty() && !bases.contains(&base) {
                bases.push(base);
            }
        }
    }
    bases
}

/// Video files smaller than this are samples, not the episode.
pub const MIN_VIDEO_BYTES: u64 = 5 * 1024 * 1024;

/// One file inside a torrent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFile {
    /// Position in the torrent's file list.
    pub index: usize,
    pub name: String,
    pub length: u64,
}

/// Chooses which file inside the torrent is the episode we want.
///
/// `min_bytes` excludes samples. A caller whose file list has no sizes passes 0, so a
/// missing size does not silently reject every file; the extension and episode checks
/// are what matter there.
///
/// `episode` may be None, which means the torrent is a film: a film has no episode
/// number, so the largest video file is the feature and everything else (samples,
/// extras, a trailer) is smaller than it.
///
/// `season` narrows the match for a series, where a single torrent can hold every
/// season. A file naming a *different* season is rejected outright: the season is part
/// of the episode's identity, and a season pack is a case this server can now choose,
/// so "S02E01" must never satisfy a request for "S01E01".
pub fn pick_video_file(
    files: &[VideoFile],
    episode: Option<u32>,
    min_bytes: u64,
    season: Option<u32>,
) -> Option<&VideoFile> {
    let mut videos: Vec<&VideoFile> = files
        .iter()
        .filter(|file| is_video_file(&file.name) && file.length >= min_bytes)
        .collect();
    if videos.is_empty() {
        return None;
    }

    let Some(episode) = episode else {
        // Largest first, so a sample or an extra cannot be mistaken for the feature.
        videos.sort_by_key(|file| std::cmp::Reverse(file.length));
        return videos.first().copied();
    };

    // A file that names a season other than the one requested is not this episode,
    // whatever its number says. This is a hard filter rather than a narrowing one: it
    // applies even when it would leave nothing, because serving the wrong season is
    // worse than serving nothing.
    //
    // A file naming no season is still accepted, because the season is only known from
    // the release title and this function never sees it. A pack titled "Show S02" whose
    // files are named "Show - 05.mkv" is a real shape, and rejecting those would turn a
    // correct season-2 pack into no source at all. Whether an unlabelled *release* may
    // satisfy a later season is decided in matches_episode, which does see the title.
    let in_season = |file: &VideoFile| match (season, parse_season(&file.name)) {
        (Some(wanted), Some(named)) => named == wanted,
        _ => true,
    };

    let matching: Vec<&VideoFile> = videos
        .iter()
        .copied()
        .filter(|file| parse_episode(&file.name) == Some(episode) && in_season(file))
        .collect();

    if !matching.is_empty() {
        // A bonus is numbered like an episode and so matches the same way, but it is
        // separate content. In the "India's Got Latent" Season 1 pack, "IGL_Bonus_EP_1" and
        // "IGL_EP_01" both parse as episode 1, and the bonus was listed first - so it was
        // served in place of the episode. A real episode is preferred, and the bonus is
        // only used when the pack genuinely holds nothing else for that number.
        let real = matching.iter().copied().find(|file| !is_bonus(&file.name));
        return real.or_else(|| matching.first().copied());
    }

    // A single-file torrent does not have to name its episode, but if it does name
    // one it must be the one requested. Holding exactly one file proves nothing
    // about which episode it is, and serving the wrong episode silently is worse
    // than serving nothing.
    if let [only] = videos.as_slice()
        && in_season(only)
    {
        return if parse_episode(&only.name).is_none() {
            Some(*only)
        } else {
            None
        };
    }

    // A batch that does not name the episode cannot be resolved safely.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(title: &str, seeders: u32, info_hash: &str) -> Release {
        Release {
            title: title.to_string(),
            info_hash: info_hash.to_string(),
            magnet: String::new(),
            size_bytes: None,
            seeders,
            source: "test".to_string(),
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn hashes(ranked: &[Release]) -> Vec<&str> {
        ranked
            .iter()
            .map(|release| release.info_hash.as_str())
            .collect()
    }

    fn file(name: &str, length: u64) -> VideoFile {
        VideoFile {
            index: 0,
            name: name.to_string(),
            length,
        }
    }

    fn hash(letter: char) -> String {
        letter.to_string().repeat(40)
    }

    // --- releases.test.js ---------------------------------------------------------

    #[test]
    fn is_video_file_recognises_video_containers_only() {
        assert!(is_video_file("episode.mkv"));
        assert!(is_video_file("episode.MP4"));
        assert!(!is_video_file("readme.txt"));
        assert!(!is_video_file("subs.ass"));
    }

    #[test]
    fn parse_resolution_reads_common_release_formats() {
        assert_eq!(
            parse_resolution("[SubsPlease] Show - 01 (1080p)"),
            Some(1080)
        );
        assert_eq!(parse_resolution("Show 720p WEB-DL"), Some(720));
        assert_eq!(parse_resolution("Show 480p"), Some(480));
        assert_eq!(parse_resolution("Show 2160p"), Some(2160));
        assert_eq!(parse_resolution("Show no resolution"), None);
    }

    #[test]
    fn parse_resolution_is_not_confused_by_digits_after_the_p() {
        // "10bit" tags follow the resolution with no separator.
        assert_eq!(parse_resolution("Show 720p10bit"), Some(720));
        assert_eq!(parse_resolution("Show 1080p10bit x264"), Some(1080));
    }

    #[test]
    fn parse_codec_detects_hevc_and_h264() {
        assert_eq!(parse_codec("Show 1080p x265 FLAC"), Some(Codec::Hevc));
        assert_eq!(parse_codec("Show 1080p HEVC"), Some(Codec::Hevc));
        assert_eq!(parse_codec("Show 1080p x264 AC3"), Some(Codec::H264));
        assert_eq!(parse_codec("Show 1080p H.264"), Some(Codec::H264));
        assert_eq!(parse_codec("Show 1080p AV1"), Some(Codec::Av1));
        assert_eq!(parse_codec("Show 1080p"), None);
    }

    #[test]
    fn parse_audio_separates_lossless_from_compressed() {
        assert_eq!(parse_audio("Show FLAC"), Some(Audio::Lossless));
        assert_eq!(parse_audio("Show DTS-HD"), Some(Audio::Lossless));
        assert_eq!(parse_audio("Show AAC2.0"), Some(Audio::Compressed));
        assert_eq!(parse_audio("Show"), None);
    }

    #[test]
    fn parse_episode_reads_episode_numbers_from_several_naming_styles() {
        assert_eq!(parse_episode("[SubsPlease] Show - 12 (1080p)"), Some(12));
        assert_eq!(parse_episode("Show Episode 7"), Some(7));
        assert_eq!(parse_episode("Show E05"), Some(5));
        assert_eq!(parse_episode("Show EP.102"), Some(102));
        assert_eq!(parse_episode("Show 第12话"), Some(12));
    }

    #[test]
    fn parse_episode_returns_none_when_the_release_names_no_episode() {
        // A resolution must never be mistaken for an episode number: "Show - 1080p" is
        // not episode 1080.
        assert_eq!(parse_episode("Show - 1080p"), None);
        assert_eq!(parse_episode("Show 1080p WEB-DL"), None);
        assert_eq!(
            parse_episode("NARUTO.S01.1080p.CR.WEB-DL.AAC2.0.H.264-Kitsune"),
            None
        );
    }

    #[test]
    fn is_batch_identifies_season_packs_and_boxes() {
        assert!(is_batch("Show S01 1080p"));
        assert!(is_batch("Show Batch 1080p"));
        assert!(is_batch("Show - Box 05 [BDRip]"));
        assert!(!is_batch("Show - 12 (1080p)"));
    }

    #[test]
    fn score_release_prefers_1080p_h264_and_penalises_batches() {
        let right = score_release(&release("[SubsPlease] Show - 12 (1080p) [ABC]", 10, ""));
        let lowres = score_release(&release("[Group] Show - 12 (480p x264)", 1, ""));
        let batch = score_release(&release("[Group] Show S01 1080p x264", 99, ""));

        assert!(right > lowres, "1080p must beat 480p");
        assert!(right > batch, "a single episode must beat a season pack");
    }

    #[test]
    fn matches_episode_rejects_a_release_for_a_different_episode() {
        assert!(matches_episode("Show S01E12 1080p", 12, None));
        assert!(!matches_episode("Show S01E11 1080p", 12, None));
        // A release that names no episode is allowed through; the stream layer decides
        // whether it can be used safely.
        assert!(matches_episode("Show S01 1080p x264", 12, None));
    }

    #[test]
    fn rank_releases_drops_unplayable_releases_and_orders_the_rest() {
        let ranked = rank_releases(
            &[
                release("Show - 12 (1080p x265 HEVC)", 500, "a"),
                release("Show - 12 (1080p x264 AAC)", 5, "b"),
                release("Show - 11 (1080p x264 AAC)", 50, "c"),
            ],
            12,
            "Show",
            &[],
            None,
        );

        // the HEVC release and the wrong episode are both filtered out
        assert_eq!(hashes(&ranked), ["b"]);
    }

    #[test]
    fn search_terms_fans_out_over_the_ways_releases_write_an_episode_number() {
        // Releases use "1", "01", "001" and "S01E01" interchangeably and the indexers
        // match literally, so one form finds a fraction of what exists. Measured live:
        // "Bocchi the Rock 1" ranked 0 usable releases while the full fan-out ranked 7.
        let terms = search_terms(&strings(&["Bocchi the Rock!"]), Some(1));
        assert_eq!(
            terms,
            [
                "Bocchi the Rock 1",
                "Bocchi the Rock 01",
                "Bocchi the Rock 001",
                "Bocchi the Rock S01E01",
            ]
        );
    }

    #[test]
    fn search_terms_strips_punctuation_and_keeps_cjk_titles() {
        assert_eq!(
            search_terms(&strings(&["Kaguya-sama: Love is War"]), Some(3)),
            [
                "Kaguya-sama Love is War 3",
                "Kaguya-sama Love is War 03",
                "Kaguya-sama Love is War 003",
                "Kaguya-sama Love is War S01E03",
            ]
        );
        // An ASCII-only \w would strip this title to nothing and search for the episode
        // number alone.
        assert_eq!(
            search_terms(&strings(&["進撃の巨人"]), Some(1))[0],
            "進撃の巨人 1"
        );
    }

    #[test]
    fn search_terms_does_not_truncate_a_long_running_episode_number() {
        // ONE PIECE is past episode 1100; padding must leave a three-digit number alone.
        assert_eq!(
            search_terms(&strings(&["One Piece"]), Some(1104)),
            ["One Piece 1104", "One Piece S01E1104"]
        );
    }

    #[test]
    fn search_terms_covers_every_title_variant_and_stays_bounded() {
        let terms = search_terms(
            &strings(&["Attack on Titan", "Shingeki no Kyojin"]),
            Some(1),
        );
        assert!(terms.contains(&"Attack on Titan 1".to_string()));
        assert!(terms.contains(&"Shingeki no Kyojin 1".to_string()));
        // Each term costs one request per indexer, so the list is capped.
        assert!(
            terms.len() <= 8,
            "expected at most 8 terms, got {}",
            terms.len()
        );
    }

    #[test]
    fn search_terms_falls_back_to_the_bare_title_without_an_episode_number() {
        assert_eq!(search_terms(&strings(&["Show"]), None), ["Show"]);
        assert!(search_terms(&[], Some(1)).is_empty());
    }

    #[test]
    fn parse_episode_handles_the_sxxexx_naming_used_by_web_dl_releases() {
        assert_eq!(
            parse_episode("One Piece S01E1158 1080p CR WEB-DL AAC2.0 H 264-VARYG"),
            Some(1158)
        );
        assert_eq!(parse_episode("Show S02E07 1080p"), Some(7));
    }

    #[test]
    fn is_relevant_rejects_a_different_show_that_merely_contains_the_title() {
        // These all contain the searched title but are different shows.
        assert!(!is_relevant(
            "Boruto - Naruto Next Generations Box Set 1",
            "Naruto"
        ));
        assert!(is_relevant("One Piece Fan Letter S01E01", "One Piece"));
        assert!(is_relevant(
            "One Piece S01E1158 1080p CR WEB-DL",
            "One Piece"
        ));
    }

    #[test]
    fn is_relevant_accepts_the_naming_styles_real_indexers_use() {
        assert!(is_relevant(
            "[SubsPlease] One Piece - 1080 (1080p)",
            "One Piece"
        ));
        assert!(is_relevant(
            "[Group] Naruto Shippuden 163 (BDRip 1080p)",
            "Naruto Shippuden"
        ));
        assert!(is_relevant(
            "NARUTO.S01.1080p.CR.WEB-DL.H.264-Kitsune",
            "Naruto"
        ));
        assert!(is_relevant("[Naruto] Episode 1 [1080p]", "Naruto"));
        assert!(!is_relevant("Bleach S01E01 1080p", "Naruto"));
    }

    #[test]
    fn rank_releases_never_returns_a_release_for_a_different_episode() {
        // Serving the wrong episode silently is worse than serving nothing.
        let ranked = rank_releases(
            &[release("Naruto S01E11 1080p x264 AAC", 500, "wrong")],
            12,
            "Naruto",
            &[],
            None,
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn rank_releases_keeps_the_requested_episode() {
        let ranked = rank_releases(
            &[release("Naruto S01E12 1080p x264 AAC", 5, "right")],
            12,
            "Naruto",
            &[],
            None,
        );
        assert_eq!(hashes(&ranked), ["right"]);
    }

    #[test]
    fn is_extra_matches_words_separated_by_underscores() {
        // "_" is a regex word character, so a plain \bmovie\b never matched this title.
        assert!(is_extra("[DB]_Naruto_Shippuuden_Movie_[75F57621]"));
        assert!(!is_extra("Show.S01E01.1080p.WEB-DL"));
    }

    #[test]
    fn is_batch_treats_an_episode_range_as_a_batch() {
        assert!(is_batch("Naruto [1-162 of 220] [BDRip] [800p]"));
        assert!(is_batch("Naruto Shippuden 1-79 HD [Multi-Sub]"));
        assert!(!is_batch("Show - 12 (1080p)"));
    }

    #[test]
    fn a_sequel_number_after_movie_is_not_an_episode_number() {
        assert_eq!(
            parse_episode("[Believe] Naruto Shippuden the Movie 1 (BD 1080p x264)"),
            None
        );
    }

    #[test]
    fn parse_episode_reads_the_bare_episode_number_fansub_releases_use() {
        // "[HatSubs] One Piece 1104 (WEB 1080p)" previously parsed as "no episode", which
        // made episode 1104 an eligible fallback candidate for episode 1.
        assert_eq!(
            parse_episode("[HatSubs] One Piece 1104 (WEB 1080p) [F8FA71E1].mkv"),
            Some(1104)
        );
        assert_eq!(parse_episode("[Group] Show 001 (1080p) [ABC].mkv"), Some(1));
    }

    #[test]
    fn parse_episode_is_not_fooled_by_audio_channels_or_bit_depths() {
        assert_eq!(parse_episode("Show 5.1 (Dual Audio) [1080p]"), None);
        assert_eq!(parse_episode("[Group] Show 720p10bit x264"), None);
    }

    #[test]
    fn parse_episode_reads_the_bare_show_nn_word_form() {
        // This is the shape documented at the top of the module, and it previously
        // parsed as None, which put the release in the fallback bucket where it could be
        // served for any episode.
        assert_eq!(
            parse_episode("Naruto Shippuden 163 Remaster (BDRip 1080p x265 FLAC Multi) - Ryujin"),
            Some(163)
        );
        assert_eq!(parse_episode("One Piece 1000 WEB-DL 1080p AAC"), Some(1000));
        assert_eq!(parse_episode("Show 12 1080p x264"), Some(12));
        // Collections must still not read as an episode.
        assert_eq!(parse_episode("Show S01 1080p"), None);
        assert_eq!(parse_episode("Show Part 2 1080p"), None);
    }

    #[test]
    fn matches_any_title_accepts_a_release_named_with_any_known_title_variant() {
        // A show has several names and releases use them inconsistently. Checking only
        // the English title rejected every correct release of this show.
        let variants = strings(&["Attack on Titan", "Shingeki no Kyojin", "進撃の巨人"]);
        assert!(matches_any_title(
            "[SubsPlease] Shingeki no Kyojin - 01 (1080p)",
            &variants
        ));
        assert!(matches_any_title("Attack on Titan S01E01 1080p", &variants));
        assert!(matches_any_title("進撃の巨人 S01E01 1080p", &variants));
        assert!(!matches_any_title("Bleach S01E01 1080p", &variants));
    }

    #[test]
    fn rank_releases_uses_every_title_variant() {
        let ranked = rank_releases(
            &[release(
                "[SubsPlease] Shingeki no Kyojin - 01 (1080p)",
                10,
                "romaji",
            )],
            1,
            "Attack on Titan",
            &strings(&["Attack on Titan", "Shingeki no Kyojin"]),
            None,
        );
        assert_eq!(hashes(&ranked), ["romaji"]);
    }

    #[test]
    fn rank_releases_enforces_the_episode_number_when_no_title_is_comparable() {
        // A CJK-only title cannot be matched against ASCII release names, so relevance
        // has to pass everything. The episode number is then the only signal available,
        // which means a wrong release for the SAME episode number cannot be detected.
        // That is a real limitation, and this test pins the behaviour that matters: a
        // release naming a different episode is still rejected.
        let ranked = rank_releases(
            &[
                release("Some Other Show S01E05 1080p x264", 99, "wrongepisode"),
                release("進撃の巨人 S01E01 1080p x264", 5, "right"),
            ],
            1,
            "進撃の巨人",
            &strings(&["進撃の巨人"]),
            None,
        );
        assert_eq!(hashes(&ranked), ["right"]);
    }

    #[test]
    fn matches_any_title_accepts_everything_when_no_title_is_comparable() {
        // Documented limitation: with only a CJK title there is nothing to gate on.
        assert!(matches_any_title(
            "Any Show 1 1080p",
            &strings(&["進撃の巨人"])
        ));
        assert!(!matches_any_title(
            "Any Show 1 1080p",
            &strings(&["Attack on Titan"])
        ));
    }

    #[test]
    fn is_likely_movie_spots_films_named_without_the_word_movie() {
        // "One Piece - Film Red 2022" was ranked as a fallback for episode 1 because it
        // names no episode number.
        assert!(is_likely_movie(
            "One Piece: Baron Omatsuri and the Secret Island (2005) [1080p]"
        ));
        assert!(is_likely_movie("Naruto the Movie 1 (2004)"));
        assert!(!is_likely_movie(
            "[SubsPlease] One Piece - 001 (1080p) [ABC]"
        ));
        assert!(!is_likely_movie("NARUTO.S01.1080p.CR.WEB-DL.H.264"));
    }

    #[test]
    fn rank_releases_never_falls_back_to_a_film_named_without_the_word_movie() {
        let ranked = rank_releases(
            &[release(
                "One Piece: Baron Omatsuri and the Secret Island (2005) [1080p]",
                99,
                "film",
            )],
            1,
            "One Piece",
            &[],
            None,
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn parse_episode_ignores_part_and_season_numbers() {
        // "One Piece - Season 01 Part 1 [1080p]" is a collection. Reading the trailing
        // "1" as episode 1 made a whole season look like the requested episode.
        assert_eq!(
            parse_episode("One Piece - Season 01 Part 1 [1080p] [x264] [Multi-Audio]"),
            None
        );
        assert_eq!(parse_episode("Show Part 2 [1080p]"), None);
        assert_eq!(parse_episode("Show Vol.1 [720p]"), None);
        assert_eq!(parse_episode("Show S01 [1080p]"), None);
        // A real episode number is still read.
        assert_eq!(parse_episode("One Piece - 001 [1080p]"), Some(1));
    }

    #[test]
    fn rank_releases_keeps_single_file_fallbacks_behind_named_episodes() {
        // A release that names no episode can still be the right file (many single-episode
        // torrents are simply named after the show), but it must never outrank a release
        // that names the episode.
        let ranked = rank_releases(
            &[
                release("Naruto [1080p x264]", 50, "unnamed"),
                release("Naruto - 001 [1080p x264 AAC]", 2, "named"),
            ],
            1,
            "Naruto",
            &[],
            None,
        );
        assert_eq!(hashes(&ranked), ["named", "unnamed"]);
    }

    #[test]
    fn is_extra_recognises_movies_ovas_and_trailers() {
        assert!(is_extra("Naruto Shippuden - The Movie (BDRip 1080p x264)"));
        assert!(is_extra(
            "NARUTO OVAs (1-11) + Special [English Subbed] 480p"
        ));
        assert!(is_extra(
            "Naruto - Coming to FUNimation! (1080p) [Official Trailer]"
        ));
        assert!(!is_extra("[SubsPlease] Naruto - 12 (1080p) [ABC]"));
    }

    #[test]
    fn rank_releases_prefers_a_real_episode_over_a_movie_of_the_same_show() {
        // Searching "Naruto 1" surfaced "Naruto Shippuden - The Movie" first, because a
        // release that names no episode number still collected the episode bonus.
        let ranked = rank_releases(
            &[
                release(
                    "[uP] Naruto Shippuden - The Movie (BDRip 1080p x264 AC3 Multi)",
                    40,
                    "movie",
                ),
                release("[Group] Naruto - 001 (1080p x264 AAC)", 5, "episode"),
            ],
            1,
            "Naruto",
            &[],
            None,
        );
        assert_eq!(hashes(&ranked), ["episode"]);
    }

    #[test]
    fn rank_releases_does_not_fall_back_to_a_movie_when_no_episode_is_named() {
        let ranked = rank_releases(
            &[
                release(
                    "Naruto the Movie - Ninja Clash in the Land of Snow (1080p x264)",
                    99,
                    "movie",
                ),
                release("Naruto Movies + EXTRA 1080p", 99, "pack"),
            ],
            1,
            "Naruto",
            &[],
            None,
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn rank_releases_orders_by_stream_health_before_quality() {
        // A 2-seeder release previously outranked a 379-seeder one for the same episode
        // because it named its codec and seeders were only a tie-break. Health decides
        // whether a release can be played at all, so it is the primary ordering.
        let ranked = rank_releases(
            &[
                release("Show - 12 (1080p x264 AAC)", 2, "barely"),
                release("Show - 12 (1080p x265)", 379, "healthy"),
                release("Show - 12 (1080p x264 AAC)", 40, "seeded"),
            ],
            12,
            "Show",
            &[],
            None,
        );
        // The 379-seeder release is unplayable (HEVC) and is filtered out entirely.
        assert_eq!(hashes(&ranked), ["seeded", "barely"]);
    }

    #[test]
    fn rank_releases_prefers_a_healthy_release_over_a_better_quality_unseeded_one() {
        let ranked = rank_releases(
            &[
                release("Show - 12 (1080p x264 AAC)", 0, "dead"),
                release("Show - 12 (720p x264 AAC)", 60, "healthy"),
            ],
            12,
            "Show",
            &[],
            None,
        );
        // 1080p scores higher, but the 720p release has real seeders, so it is tried first.
        assert_eq!(hashes(&ranked), ["healthy", "dead"]);
    }

    #[test]
    fn rank_releases_keeps_unseeded_releases_as_a_last_resort() {
        // An indexer can report zero seeders for a swarm that is in fact alive, so these
        // are not discarded.
        let ranked = rank_releases(
            &[release("Show - 12 (1080p x264 AAC)", 0, "only")],
            12,
            "Show",
            &[],
            None,
        );
        assert_eq!(hashes(&ranked), ["only"]);
    }

    #[test]
    fn rank_releases_never_returns_a_release_for_a_different_show() {
        let ranked = rank_releases(
            &[
                release(
                    "Boruto - Naruto Next Generations Box Set 1 [1080p]",
                    999,
                    "x",
                ),
                release("Naruto S01E01 1080p x264 AAC", 2, "y"),
            ],
            1,
            "Naruto",
            &[],
            None,
        );
        assert_eq!(hashes(&ranked), ["y"]);
    }

    #[test]
    fn is_batch_recognises_every_range_separator_release_groups_use() {
        // "01 ~ 12" is a 12-episode batch. It was read as episode 1, so a whole season was
        // ranked as a single episode and offered for episode 1 - which is worse than
        // finding nothing, because it looks like it worked.
        assert!(is_batch(
            "[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]"
        ));
        assert!(is_batch("Show 01 ~ 12 [1080p]"));
        assert!(is_batch("Show 1~12"));
        assert!(is_batch("Show 1–12"), "en dash");
        assert!(is_batch("Show 1—12"), "em dash");
        assert!(is_batch("Show 1-12"));
        // A single episode must not be caught by any of the above.
        assert!(!is_batch("Show - 12 (1080p)"));
        assert!(!is_batch("Show S01E01 [1080p]"));
    }

    #[test]
    fn a_range_release_is_penalised_like_the_batch_it_is() {
        // Once "01 ~ 12" is recognised as a range, the batch penalty applies to it, so a
        // real single episode wins at equal seeder health. Before the fix the range was not
        // seen as a batch at all, so it carried no penalty.
        let titles = strings(&["Bocchi the Rock"]);
        let releases = [
            release(
                "[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]",
                10,
                &hash('a'),
            ),
            release(
                "[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv",
                10,
                &hash('b'),
            ),
        ];

        let ranked = rank_releases(&releases, 1, &titles[0], &titles, None);
        assert_eq!(
            ranked[0].title,
            "[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv"
        );

        // Seeder health is still the first key, by design: a batch with far more peers is
        // more likely to play than a single episode with almost none.
        let seeded = [
            release(
                "[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]",
                56,
                &hash('a'),
            ),
            release(
                "[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv",
                12,
                &hash('b'),
            ),
        ];
        assert_eq!(
            rank_releases(&seeded, 1, &titles[0], &titles, None)[0].seeders,
            56
        );
    }

    // Season packs: regional and Indian series are released almost entirely as packs, and
    // each of these pins a way a pack was once mishandled.

    #[test]
    fn parse_season_reads_a_bare_season_token_as_well_as_sxxexx() {
        // A pack names only the season, which is what made a season-2 pack look like it
        // named no season at all - and therefore satisfy a request for season 1.
        assert_eq!(
            parse_season("Gyaarah Gyaarah S01 [1080p WEB-DL x264]"),
            Some(1)
        );
        assert_eq!(
            parse_season("Panchayat S02 Complete Season 2 1080p"),
            Some(2)
        );
        assert_eq!(parse_season("Show.Season.3.1080p"), Some(3));

        // The digits must not be read out of a resolution or a year.
        assert_eq!(parse_season("Show 1080p WEB-DL"), None);
        assert_eq!(parse_season("Show S2020 1080p"), None);
        assert_eq!(parse_season("Show 05 1080p"), None);
    }

    #[test]
    fn a_season_pack_is_offered_only_for_the_season_it_covers() {
        let titles = strings(&["Panchayat"]);

        let season_one = [release("Panchayat S01 1080p WEB-DL", 30, &hash('a'))];
        assert_eq!(
            rank_releases(&season_one, 1, &titles[0], &titles, Some(1)).len(),
            1,
            "season 1 pack, season 1 request"
        );

        let season_two = [release("Panchayat S02 Complete 1080p", 30, &hash('b'))];
        assert_eq!(
            rank_releases(&season_two, 1, &titles[0], &titles, Some(1)).len(),
            0,
            "season 2 pack must not satisfy season 1"
        );

        // The same pack is correct for its own season.
        assert_eq!(
            rank_releases(&season_two, 1, &titles[0], &titles, Some(2)).len(),
            1
        );
    }

    #[test]
    fn a_pack_is_ordered_behind_a_release_that_names_the_episode() {
        // A release naming the episode is always the better answer: it is the one episode
        // rather than a whole season to reach it from.
        let titles = strings(&["Gyaarah Gyaarah"]);
        let releases = [
            release(
                "Gyaarah Gyaarah S01 [1080p WEB-DL x264 AAC Hindi]",
                71,
                &hash('a'),
            ),
            release("Gyaarah Gyaarah S01E01 1080p WEB-DL", 2, &hash('b')),
        ];

        let ranked = rank_releases(&releases, 1, &titles[0], &titles, Some(1));
        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].info_hash,
            hash('b'),
            "the named episode wins despite far fewer seeders"
        );
        assert_eq!(ranked[1].info_hash, hash('a'));
    }

    #[test]
    fn a_pack_is_a_last_resort_behind_a_single_file_fallback_too() {
        let titles = strings(&["Show"]);
        let releases = [
            release("Show S01 1080p x264", 50, &hash('a')),
            release("Show 1080p WEB-DL x264", 50, &hash('b')),
        ];

        let ranked = rank_releases(&releases, 1, &titles[0], &titles, Some(1));
        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].info_hash,
            hash('b'),
            "the release naming no episode is not a pack and comes first"
        );
    }

    #[test]
    fn the_anime_path_admits_only_a_first_season_pack() {
        // An anime episode id is a flat count with no season, so a later season's pack
        // cannot be mapped onto it: Shippuden S02 holds episodes 33-53 but names its files
        // 01-21, so reading file "01" out of it would serve episode 33 as episode 1.
        let titles = strings(&["Naruto Shippuden"]);
        let first = [release("Naruto Shippuden S01 1080p", 20, &hash('a'))];
        let second = [release("Naruto Shippuden S02 1080p", 20, &hash('b'))];

        assert_eq!(rank_releases(&first, 1, &titles[0], &titles, None).len(), 1);
        assert_eq!(
            rank_releases(&second, 1, &titles[0], &titles, None).len(),
            0
        );
    }

    #[test]
    fn a_series_search_asks_for_the_season_and_the_title_not_only_the_episode() {
        // knaben requires every word of a term to match, so an episode-bearing term finds
        // nothing for a pack-only series. Measured: "Mirzapur S01E01" returns 0 hits where
        // "Mirzapur S01" returns 15 and "Mirzapur" returns 100.
        let terms = series_search_terms(&strings(&["Mirzapur"]), Some(1), Some(1));
        let position = |wanted: &str| terms.iter().position(|term| term == wanted);
        assert!(position("Mirzapur S01E01").is_some(), "got {terms:?}");
        assert!(position("Mirzapur S01").is_some(), "got {terms:?}");
        assert!(position("Mirzapur").is_some(), "got {terms:?}");
        // The episode-specific forms still come first, so they decide the ordering whenever
        // they return anything.
        assert!(position("Mirzapur S01E01") < position("Mirzapur S01"));
    }

    // "the title term is derived without eating a real word" (titleOf) belongs to the
    // indexers module, not to this one, and is ported with it.

    #[test]
    fn a_pack_spanning_several_seasons_is_refused() {
        // Its file names restart at 01 for each season, so "S03E05.mkv" is indistinguishable
        // from "S01E05.mkv" by number alone, and for anime there is no season to check at all.
        let titles = strings(&["Show"]);
        let spanning = [release("Show S01-S05 Complete 1080p x264", 50, &hash('a'))];
        assert_eq!(
            rank_releases(&spanning, 5, &titles[0], &titles, Some(3)).len(),
            0,
            "series"
        );
        assert_eq!(
            rank_releases(&spanning, 5, &titles[0], &titles, None).len(),
            0,
            "anime"
        );
        assert_eq!(
            rank_releases(&spanning, 1, &titles[0], &titles, Some(1)).len(),
            0
        );
    }

    #[test]
    fn an_extras_pack_is_not_offered_as_an_episode() {
        // A specials pack names no episode, so without this it lands in the pack group and is
        // offered for a real episode of that season.
        let titles = strings(&["Show"]);
        let specials = [release("Show S01 Specials 1080p x264", 50, &hash('a'))];
        assert_eq!(
            rank_releases(&specials, 1, &titles[0], &titles, Some(1)).len(),
            0
        );

        let ovas = [release("Show S01 OVA 1080p", 50, &hash('b'))];
        assert_eq!(
            rank_releases(&ovas, 1, &titles[0], &titles, Some(1)).len(),
            0
        );
    }

    #[test]
    fn a_pack_with_no_quality_tag_is_still_offered() {
        // The terse regional release names this group exists for carry no quality tag at all,
        // so a pack must not be dropped for scoring low. It is the last resort, not an
        // unusable release.
        let titles = strings(&["Show"]);
        let bare = [release("Show S01", 0, &hash('a'))];
        assert_eq!(
            rank_releases(&bare, 1, &titles[0], &titles, Some(1)).len(),
            1
        );
    }

    #[test]
    fn every_title_variant_keeps_its_season_and_title_forms() {
        // A show is known by several names and releases use them inconsistently. Capping the
        // whole term list meant the first variant consumed the budget and the rest lost their
        // season and bare-title forms - the forms a pack-only series is found by.
        let terms = series_search_terms(
            &strings(&["Shingeki no Kyojin", "Attack on Titan"]),
            Some(1),
            Some(1),
        );
        for wanted in [
            "Shingeki no Kyojin S01",
            "Attack on Titan S01",
            "Attack on Titan",
        ] {
            assert!(terms.contains(&wanted.to_string()), "got {terms:?}");
        }

        // An anime has three variants from AniList, and the Japanese one is last.
        let anime = search_terms(
            &strings(&["Attack on Titan", "Shingeki no Kyojin", "進撃の巨人"]),
            Some(1),
        );
        assert!(
            anime.iter().any(|term| term.starts_with("進撃の巨人")),
            "got {anime:?}"
        );
    }

    #[test]
    fn a_multi_season_pack_is_refused_in_every_form_it_is_written() {
        // Release names write a season range with a hyphen, a tilde, or a space, and
        // underscores and dots have already become spaces by the time this runs. All of them
        // mean the same thing: the file names inside restart at 01 for each season, so a file
        // number alone cannot say which season it belongs to.
        let titles = strings(&["Show"]);
        for title in [
            "Show S01-S05 Complete 1080p x264",
            "Show.S01.S05.Complete.1080p",
            "Show S01 S05 1080p",
            "Show S01~S05 Complete",
        ] {
            let releases = [release(title, 50, &hash('a'))];
            assert_eq!(
                rank_releases(&releases, 5, &titles[0], &titles, None).len(),
                0,
                "{title} (anime)"
            );
            assert_eq!(
                rank_releases(&releases, 5, &titles[0], &titles, Some(3)).len(),
                0,
                "{title} (series)"
            );
        }
    }

    #[test]
    fn a_special_numbered_1_is_not_episode_1() {
        // "Show S01 Special 1 1080p" parses to episode 1 and would otherwise land in the
        // named group and outrank the real episode on seeders or resolution.
        let titles = strings(&["Show"]);
        let special = [release("Show S01 Special 1 1080p x264", 99, &hash('a'))];
        assert_eq!(
            rank_releases(&special, 1, &titles[0], &titles, Some(1)).len(),
            0
        );
    }

    // A title with an apostrophe is written three ways and the variants do not match each
    // other: the catalogue spelling ("India's Got Latent"), the scene spelling that drops
    // the apostrophe ("Indias Got Latent"), and the spaced form that normalize_title
    // produces ("India s Got Latent").

    #[test]
    fn an_apostrophe_title_matches_every_spelling_a_release_uses() {
        let titles = strings(&["India's Got Latent"]);
        for release in [
            "INDIA S GOT LATENT S 2 EP 5 Ft. Orry Archana Puran Singh",
            "INDIA S GOT LATENT S 2 EP 6 Ft. Rakhi Sawant",
            "Indias Got Latent S02E01 1080p",
            "Indias.Got.Latent.S02E01.1080p.WEB-DL",
            "India's Got Latent S02E01 1080p",
        ] {
            assert!(matches_any_title(release, &titles), "{release}");
        }
    }

    #[test]
    fn the_apostrophe_variants_do_not_make_the_gate_permissive() {
        let titles = strings(&["India's Got Latent"]);
        // A different show that shares every word except the first must still be rejected.
        assert!(!matches_any_title("Britain s Got Latent Problems", &titles));
        assert!(!matches_any_title("Britain's Got Latent", &titles));
        assert!(!matches_any_title("Got Latent", &titles));
    }

    #[test]
    fn an_apostrophe_title_is_searched_in_every_spelling() {
        let terms = series_search_terms(&strings(&["India's Got Latent"]), Some(2), Some(1));
        // The apostrophe is dropped in the spelling real releases use, so a term without
        // it must exist; without this every indexer returned zero rows for the show.
        assert!(
            terms.contains(&"Indias Got Latent S02E01".to_string()),
            "{terms:?}"
        );
        assert!(
            terms.contains(&"Indias Got Latent".to_string()),
            "{terms:?}"
        );
        // The spaced form is kept too: archive.org names its items that way.
        assert!(
            terms.contains(&"India s Got Latent".to_string()),
            "{terms:?}"
        );
    }

    #[test]
    fn a_title_without_an_apostrophe_is_unaffected() {
        assert_eq!(title_variants("Panchayat"), ["panchayat"]);
        assert_eq!(title_variants("Bocchi the Rock!"), ["bocchi the rock"]);
    }

    // archive.org writes the season out longhand and with gaps: "INDIA S GOT LATENT S 2 EP 6",
    // "Panchayat S 03 E 1 8", "Mirzapur. S 01. E 01". A release whose season cannot be read
    // passes the season gate vacuously, so a season-2 file would satisfy a season-1 request.

    #[test]
    fn parse_season_reads_the_spaced_season_form_archive_org_uses() {
        assert_eq!(
            parse_season("INDIA S GOT LATENT S 2 EP 5 Ft. Orry"),
            Some(2)
        );
        assert_eq!(parse_season("Panchayat S 03 E 1 8"), Some(3));
        assert_eq!(parse_season("Mirzapur. S 01. E 01.720p. HEVC"), Some(1));
        assert_eq!(
            parse_season("Gyaarah. Gyaarah. S 01 E 01. Episode. 1.1080p"),
            Some(1)
        );
    }

    #[test]
    fn the_spaced_season_form_still_refuses_a_resolution_and_a_year() {
        // The widening must not turn "1080p" or "S2020" into a season.
        assert_eq!(parse_season("Show 1080p WEB-DL"), None);
        assert_eq!(parse_season("Show S2020 1080p"), None);
        assert_eq!(
            parse_season("Show S1 1080p"),
            Some(1),
            "a bare S1 is still a season"
        );
    }

    #[test]
    fn a_spaced_season_file_is_rejected_for_the_wrong_season() {
        let titles = strings(&["India s Got Latent"]);
        let pack = [release(
            "INDIA S GOT LATENT S 2 EP 5 Ft. Orry",
            3,
            &hash('a'),
        )];
        // The release names episode 5 of season 2, so it is not episode 1 of season 1.
        assert!(!matches_episode(&pack[0].title, 1, Some(1)));
        assert!(matches_episode(&pack[0].title, 5, Some(2)));
        assert_eq!(
            rank_releases(&pack, 1, &titles[0], &titles, Some(1)).len(),
            0
        );
        assert_eq!(
            rank_releases(&pack, 5, &titles[0], &titles, Some(2)).len(),
            1
        );
    }

    #[test]
    fn pick_video_file_prefers_a_real_episode_over_a_bonus_of_the_same_number() {
        // The real Season 1 pack of "India's Got Latent". "IGL_Bonus_EP_1" and "IGL_EP_01"
        // both parse as episode 1, and the bonus is listed first - so the bonus was served in
        // place of the episode. The pack holds every real episode at 47 seeders.
        let pack: Vec<VideoFile> = [
            "IGL/IGL_Bonus_EP_1_ft. Arpit Bala.mp4",
            "IGL/IGL_EP_01_ft_RaftaarMusic.mp4",
            "IGL/IGL_EP_02_ft_GamerFleet_JokeSingh.mp4",
            "IGL/IGL_EP_05_ft_KunalKamra_AtulKhatri.mp4",
            "IGL/IGL_BONUS_EP_Discarded_1_ft_Aakash_Gupta.mp4",
        ]
        .iter()
        .map(|name| file(name, 500 * 1024 * 1024))
        .collect();

        let name = |episode| {
            pick_video_file(&pack, Some(episode), 0, Some(1))
                .unwrap()
                .name
                .clone()
        };
        assert!(
            name(1).contains("IGL_EP_01"),
            "the episode must win over the bonus"
        );
        assert!(name(2).contains("IGL_EP_02"));
        assert!(name(5).contains("IGL_EP_05"));

        // A pack holding only a bonus still serves it, because there is nothing else it
        // could be - refusing outright would turn a playable file into no source at all.
        let bonus_only = [file(
            "IGL/IGL_Bonus_EP_1_ft. Arpit Bala.mp4",
            500 * 1024 * 1024,
        )];
        let picked = pick_video_file(&bonus_only, Some(1), 0, Some(1)).unwrap();
        assert!(picked.name.contains("Bonus_EP_1"));
    }

    #[test]
    fn an_unlabelled_release_cannot_satisfy_a_later_season() {
        // The real defect. The Season 1 pack of "India's Got Latent" is titled with the show
        // name alone - it names no season - and its files are named "IGL_EP_05" with no season
        // either. It was therefore accepted for a request for season 2, and S1E5 played in
        // place of S2E5. The real S2E5 exists and is a different episode.
        let titles = strings(&["india s got latent", "indias got latent"]);
        let unlabelled_pack = Release {
            title: "India's Got Latent".to_string(),
            info_hash: "1fb077d1660144985c11e998bd91baac494ecf26".to_string(),
            magnet: "magnet:?xt=urn:btih:1fb077d1660144985c11e998bdbaac494ecf26".to_string(),
            size_bytes: Some(20358144983),
            seeders: 45,
            source: "knaben".to_string(),
        };
        let pack = [unlabelled_pack.clone()];

        // Season 1 is what an unlabelled release means, so it still answers there.
        assert_eq!(
            rank_releases(&pack, 5, "India's Got Latent", &titles, Some(1)).len(),
            1
        );
        // Season 2 must not fall back to it.
        assert_eq!(
            rank_releases(&pack, 5, "India's Got Latent", &titles, Some(2)).len(),
            0
        );

        // A pack that does label its season is unaffected.
        let labelled = [Release {
            title: "Show S02 1080p WEB-DL".to_string(),
            info_hash: hash('a'),
            ..unlabelled_pack
        }];
        assert_eq!(
            rank_releases(&labelled, 5, "Show", &strings(&["show"]), Some(2)).len(),
            1
        );

        // The anime path passes no season, so nothing is narrowed there.
        assert_eq!(
            rank_releases(&pack, 5, "India's Got Latent", &titles, None).len(),
            1
        );
    }

    // --- contentTypes.test.js ------------------------------------------------------

    #[test]
    fn a_film_whose_title_contains_a_number_is_not_rejected_as_an_episode() {
        // The reported failure: searching for "Dhamaal 4" found 73-seeded releases and then
        // threw all of them away, so the film reported "no source" while the indexers held
        // plenty. The cause was parse_episode matching the sequel number (and the year) and
        // is_episode_release treating that as a series episode. It hit Indian cinema hardest
        // because it is sequel-heavy, but any film with a number in its name was affected.
        let releases = [
            release("Dhamaal 4 (2026) 720p WEBRip x264 -YTS", 73, "1"),
            release("Dhamaal 4 (2026) 1080p WEBRip 5.1 x264 -YTS", 41, "2"),
            release("Singham Again 2024 1080p WEB-DL", 11, "3"),
            release("Stree 2 (2024) 1080p WEB-DL", 5, "4"),
            release("Kalki 2898 AD (2024) 1080p", 52, "5"),
            release("Bhool Bhulaiyaa 3 (2024) 1080p WEBRip", 12, "6"),
        ];
        for release in &releases {
            assert!(
                !is_episode_release(&release.title, Some(2026)),
                "{} is a film and must not be read as an episode",
                release.title
            );
        }

        let ranked =
            rank_movie_releases(&releases, Some(2026), "Dhamaal 4", &strings(&["Dhamaal 4"]));
        assert_eq!(
            ranked.len(),
            2,
            "both Dhamaal 4 releases must survive the filter"
        );
        assert!(
            ranked[0].title.contains("720p"),
            "the better-seeded release comes first"
        );
    }

    #[test]
    fn a_series_release_is_still_rejected_even_when_it_names_a_year() {
        // The loosening above must not let a series episode through. Every series release
        // carries an SxxExx, which is what the gate now keys on.
        for title in [
            "Breaking.Bad.S02E02.1080p.WEB.mkv",
            "Fargo.S01E01.1080p.BluRay",
            "Show.2024.S03E10.1080p",
        ] {
            assert!(
                is_episode_release(title, None),
                "{title} must be rejected as a film"
            );
        }

        // And a season pack, which names no episode at all.
        for title in [
            "Breaking Bad S01 COMPLETE 1080p",
            "Show Season 2 1080p",
            "Show 01-12 [1080p]",
        ] {
            assert!(
                is_episode_release(title, None),
                "{title} must be rejected as a film"
            );
        }
    }

    #[test]
    fn a_film_release_is_not_mistaken_for_a_series_episode() {
        // This is the regression that matters most for the film path: searching a title
        // also returns its series, and a series episode is not the film.
        let releases = [
            release("Breaking.Bad.S02E02.1080p.WEB.mkv", 500, "1"),
            release("Breaking Bad S01 COMPLETE 1080p", 400, "1"),
            release("Breaking Bad (2008) 1080p BluRay x264", 40, "2"),
        ];
        let ranked = rank_movie_releases(
            &releases,
            Some(2008),
            "Breaking Bad",
            &strings(&["Breaking Bad"]),
        );
        assert_eq!(ranked.len(), 1, "only the film itself may be ranked");
        assert!(ranked[0].title.contains("2008"));
        assert!(!is_episode_release(&ranked[0].title, None));
    }

    #[test]
    fn a_series_episode_with_no_sxxexx_is_still_rejected_as_a_film() {
        // The gate keys on a bare number only when the release names no year. Without that
        // half, a series episode named "Fargo - 05 1080p" - which starts with the film's
        // title and so passes the relevance filter - was ranked as the film itself.
        for title in [
            "Fargo - 05 1080p WEB-DL",
            "Fargo 05 1080p WEB-DL",
            "Fargo 05",
            "Show - 112 1080p",
        ] {
            assert!(
                is_episode_release(title, Some(1996)),
                "{title} must be rejected as a film"
            );
        }

        // And it cannot reach the ranking, which is where the damage would be done.
        let ranked = rank_movie_releases(
            &[
                release("Fargo - 05 1080p WEB-DL", 500, "1"),
                release("Fargo (1996) 1080p BluRay x264", 20, "2"),
            ],
            Some(1996),
            "Fargo",
            &strings(&["Fargo"]),
        );
        assert_eq!(ranked.len(), 1, "only the film may be ranked");
        assert!(ranked[0].title.contains("1996"));
    }

    #[test]
    fn a_release_naming_a_year_is_a_film_release_even_when_it_also_names_a_number() {
        // Both halves of the rule, on the same title: with the year known the sequel number
        // is admitted, and without it the number is treated as an episode because there is
        // nothing to compare against.
        assert!(!is_episode_release(
            "Dhamaal 4 (2026) 720p WEBRip x264 -YTS",
            Some(2026)
        ));
        assert!(!is_episode_release(
            "Singham Again 2024 1080p WEB-DL",
            Some(2024)
        ));
        // No year to compare, so the conservative answer is "an episode".
        assert!(is_episode_release(
            "Dhamaal 4 (2026) 720p WEBRip x264 -YTS",
            None
        ));
    }

    #[test]
    fn a_film_release_naming_the_wrong_year_is_ranked_below_one_that_matches() {
        let releases = [
            release("Dangal (2013) 1080p", 1, "1"),
            release("Dangal (2016) 720p", 1, "2"),
        ];
        let ranked = rank_movie_releases(&releases, Some(2016), "Dangal", &strings(&["Dangal"]));
        assert_eq!(ranked.len(), 2);
        assert!(
            ranked[0].title.contains("2016"),
            "the matching year must come first"
        );
    }

    #[test]
    fn parse_season_and_parse_year_read_the_forms_releases_actually_use() {
        assert_eq!(parse_season("Show S02E05 1080p"), Some(2));
        assert_eq!(parse_season("Show.S02E05.1080p.mkv"), Some(2));
        assert_eq!(parse_season("Show 05 1080p"), None);
        assert_eq!(parse_year("Dangal (2016) 1080p"), Some(2016));
        assert_eq!(parse_year("Dangal 2016 1080p"), Some(2016));
        assert_eq!(parse_year("Dangal 1080p"), None);
    }

    #[test]
    fn search_terms_carry_the_season_for_a_series_and_the_year_for_a_film() {
        let series = series_search_terms(&strings(&["Breaking Bad"]), Some(1), Some(1));
        assert!(
            series.contains(&"Breaking Bad S01E01".to_string()),
            "got {series:?}"
        );

        let film = movie_search_terms(&strings(&["Dangal"]), Some(2016));
        assert!(film.contains(&"Dangal 2016".to_string()), "got {film:?}");
        // The year is optional, so a term without it is searched too.
        assert!(film.contains(&"Dangal".to_string()));
    }

    #[test]
    fn pick_video_file_prefers_the_requested_season_inside_a_multi_season_torrent() {
        let files = [
            file("Show S01E05 1080p.mkv", 2_000_000_000),
            file("Show S02E05 1080p.mkv", 2_000_000_000),
            file("Show S03E05 1080p.mkv", 2_000_000_000),
        ];
        let picked = |season| {
            pick_video_file(&files, Some(5), MIN_VIDEO_BYTES, season)
                .unwrap()
                .name
                .as_str()
        };

        assert_eq!(picked(Some(2)), "Show S02E05 1080p.mkv");
        assert_eq!(picked(Some(3)), "Show S03E05 1080p.mkv");
        // Without a season the first match wins, which is the old behaviour.
        assert_eq!(picked(None), "Show S01E05 1080p.mkv");
    }

    #[test]
    fn pick_video_file_still_accepts_a_release_that_names_no_season() {
        let files = [
            file("Show - 05 1080p.mkv", 2_000_000_000),
            file("Show - 06 1080p.mkv", 2_000_000_000),
        ];
        // The season is only used to narrow an ambiguous list, so an unnamed release is
        // still a match rather than being rejected.
        let picked = pick_video_file(&files, Some(5), MIN_VIDEO_BYTES, Some(2)).unwrap();
        assert_eq!(picked.name, "Show - 05 1080p.mkv");
    }

    // --- sources.test.js -----------------------------------------------------------

    #[test]
    fn a_file_list_with_no_sizes_is_still_usable() {
        // A file list can omit sizes. Applying the 5MB sample filter there would reject
        // every file and look like "nothing is playable".
        let files = [file("Show - 01.mp4", 0), file("Show - 02.mp4", 0)];
        assert_eq!(
            pick_video_file(&files, Some(1), 0, None).unwrap().name,
            "Show - 01.mp4"
        );
        // The size floor still applies by default.
        assert_eq!(
            pick_video_file(&files, Some(1), MIN_VIDEO_BYTES, None),
            None
        );
    }

    // --- derived from the JS comments, beyond the ported suite ---------------------

    #[test]
    fn pick_video_file_takes_the_largest_file_for_a_film_and_the_first_on_a_tie() {
        let files = [
            file("sample.mkv", 6 * 1024 * 1024),
            file("film.mkv", 900 * 1024 * 1024),
            file("also.mkv", 900 * 1024 * 1024),
            file("readme.txt", 1 << 40),
        ];
        let picked = pick_video_file(&files, None, MIN_VIDEO_BYTES, None).unwrap();
        assert_eq!(picked.name, "film.mkv");
    }

    #[test]
    fn pick_video_file_refuses_a_lone_file_that_names_another_episode_or_season() {
        let size = 500 * 1024 * 1024;
        let wrong_episode = [file("Show - 07.mkv", size)];
        assert_eq!(pick_video_file(&wrong_episode, Some(5), 0, None), None);
        let unnamed = [file("Show.mkv", size)];
        assert!(pick_video_file(&unnamed, Some(5), 0, None).is_some());
        let wrong_season = [file("Show S03.mkv", size)];
        assert_eq!(pick_video_file(&wrong_season, Some(5), 0, Some(1)), None);
        let batch = [file("Show - 01.mkv", size), file("Show - 02.mkv", size)];
        assert_eq!(pick_video_file(&batch, Some(5), 0, None), None);
    }

    #[test]
    fn parse_episode_reads_underscore_separated_file_names() {
        assert_eq!(parse_episode("IGL/IGL_EP_01_ft_RaftaarMusic.mp4"), Some(1));
        assert_eq!(parse_episode("IGL_Bonus_EP_1_ft. Arpit Bala.mp4"), Some(1));
    }

    #[test]
    fn parse_episode_bare_patterns_follow_the_look_behind_rules() {
        // A revision suffix between the number and the bracket.
        assert_eq!(parse_episode("[Group] Show 12v2 [1080p]"), Some(12));
        assert_eq!(parse_episode("[Group] Show 12 v2 (1080p)"), Some(12));
        // Five digits is not a number, whatever follows it.
        assert_eq!(parse_episode("Show 12345 [x]"), None);
        assert_eq!(parse_episode("Show 12345 Remaster"), None);
        // A hyphen, dot or bracket directly before the number disqualifies the bare form.
        assert_eq!(parse_episode("Show-12 (x)"), None);
        assert_eq!(parse_episode("Show.12 (x)"), None);
        assert_eq!(parse_episode("Show (12 Remaster)"), None);
        // A non-ASCII letter is not a word character in JavaScript, so it is a boundary.
        assert_eq!(parse_episode("Showé12 (x)"), Some(12));
        // An episode-looking number inside a word never counts.
        assert_eq!(parse_episode("x264 [x]"), None);
        // A collection word only skips a number it directly introduces.
        assert_eq!(parse_episode("Show Season 2 Remaster 15 (x)"), Some(15));
    }

    #[test]
    fn is_relevant_tolerates_leading_tags_and_normalises_case() {
        assert!(is_relevant("[A][B]  one piece - 12", "One Piece"));
        assert!(!is_relevant("[A] One Pieceman - 12", "One Piece"));
        assert!(is_relevant("anything", ""));
    }

    #[test]
    fn movie_ranking_does_not_need_a_year() {
        let releases = [release("Dangal 1080p BluRay x264", 10, "1")];
        let ranked = rank_movie_releases(&releases, None, "Dangal", &[]);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn series_search_terms_without_an_episode_return_only_the_bases() {
        assert_eq!(
            series_search_terms(&strings(&["Show"]), Some(2), None),
            ["Show"]
        );
        assert_eq!(
            series_search_terms(&strings(&["Show"]), None, Some(5)),
            ["Show S01E05", "Show 05", "Show 5", "Show S01", "Show"]
        );
    }

    #[test]
    fn health_tiers_follow_the_published_thresholds() {
        assert_eq!(
            [0, 1, 4, 5, 49, 50, 1000].map(health_tier),
            [3, 2, 2, 1, 1, 0, 0]
        );
    }
}
