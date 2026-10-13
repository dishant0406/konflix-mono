// Release parsing and ranking.
//
// Everything here is a pure function over indexer results, so it can be tested
// without touching the network or the torrent client.
//
// Indexer titles look like:
//   [SubsPlease] One Piece - 1080 (1080p) [ABC].mkv
//   Naruto Shippuden 163 Remaster (BDRip 1080p x265 FLAC Multi) - Ryujin
//   NARUTO.S01.1080p.CR.WEB-DL.AAC2.0.H.264-Kitsune
//   One Piece - Box 05 [BDRip 1080p x264 AC3]

const VIDEO_EXTENSIONS = /\.(mkv|mp4|avi|m4v|mov|webm)$/i

export function isVideoFile(name) {
  return VIDEO_EXTENSIONS.test(name)
}

export function parseResolution(title) {
  // Anchored so the digits are not part of a longer number, and so that trailing
  // tags such as the "10" in "1080p10bit" are not mistaken for the resolution.
  const match = /(?:^|[^\d])(\d{3,4})p/i.exec(title)
  return match ? Number.parseInt(match[1], 10) : null
}

export function parseCodec(title) {
  if (/\b(x265|h\.?265|hevc)\b/i.test(title)) return 'hevc'
  if (/\b(x264|h\.?264|avc)\b/i.test(title)) return 'h264'
  if (/\b(av1)\b/i.test(title)) return 'av1'
  return null
}

// Substring matching is intentional here: release names write these without word
// boundaries ("AAC2.0", "DTS-HD", "x264-10bit").
export function parseAudio(title) {
  if (/(flac|dts|truehd|ac3|eac3|ddp)/i.test(title)) return 'lossless'
  if (/(aac|mp3|opus)/i.test(title)) return 'compressed'
  return null
}

// Returns the episode number a release refers to, or null when the release is a
// batch/season pack that does not identify a single episode.
// Words that introduce a collection or a part number rather than an episode
// number, for example "Season 01 Part 1 [1080p]". "Special" is included because
// "Show S01 Special 1 1080p" is a special numbered 1, not episode 1.
const PART_WORDS = /\b(?:part|vol\.?|volume|season|set|box|cour|movies?|films?|specials?)\s*$/i

export function parseEpisode(title) {
  const patterns = [
    // SxxExx, the standard naming for WEB-DL releases. Must come first, because
    // "S01E1158" has no word boundary before the E for the pattern below.
    /\bS\d{1,2}E(\d{1,4})\b/i,
    /\bE(?:P)?\.?\s?(\d{1,4})\b/i,
    /\bEpisode\s+(\d{1,4})\b/i,
    /\s-\s(\d{1,4})(?:\s|\[|\(|\.|$)/,
    /第\s?(\d{1,4})\s?[话話]/,
    /\b(\d{1,4})\s*(?:v\d)?\s*\[/,
    // Fansub style with the episode number bare before a bracketed quality tag:
    // "[HatSubs] One Piece 1104 (WEB 1080p)". Without this the release parsed as
    // "no episode number", which made episode 1104 an eligible candidate for
    // episode 1. The lookbehind keeps decimals ("5.1") from matching, and the
    // lookahead keeps resolutions ("1080p") from matching.
    /(?<![\d.-])\b(\d{1,4})(?![\dpP])\s*(?:v\d)?\s*[\[(]/,
    // The bare form, where the number is followed by a space and a word:
    // "Naruto Shippuden 163 Remaster (BDRip 1080p)" and "One Piece 1000 WEB-DL".
    // Without this the release parsed as "no episode number", which put it in the
    // fallback bucket where it could be served for any episode. Letters that
    // introduce a collection ("S01", "Part 1") are excluded by the lookbehind.
    /(?<![\d.\-(Ss])\b(\d{1,4})(?![\dpP])\s+/,
  ]
  for (const pattern of patterns) {
    const match = pattern.exec(title)
    if (!match) continue
    // "One Piece - Season 01 Part 1 [1080p]" is a collection, not episode 1, so a
    // number introduced by one of these words is not treated as an episode.
    if (PART_WORDS.test(title.slice(0, match.index))) continue
    return Number.parseInt(match[1], 10)
  }
  return null
}

// The season a release names, or null.
//
// Series releases use "S01E05", and the season is part of the identity: "S02E05" is
// a different episode from "S01E05". The anime path ignores the season because its
// episode ids are a flat count, but the series path matches on both.
//
// A bare season token is matched as well as a full "SxxExx", because a season *pack*
// names only the season ("Gyaarah Gyaarah S01 [1080p ...]"). Reading that as "no
// season" is what let a season-2 pack satisfy a request for a season-1 episode.
//
// The digits are bounded so a resolution ("1080p") and a year ("S2020") are not read
// as a season: "\b" fails after the "20" of "S2020" because the next character is
// another digit, and "Season 1" is matched through its word.
export function parseSeason(title) {
  const match = /\bS(?:eason\s*)?(\d{1,2})(?:E\d{1,4})?\b/i.exec(words(title))
  return match ? Number.parseInt(match[1], 10) : null
}

// Release names separate words with spaces, dots and underscores. Underscore is a
// regex word character, so "\bmovie\b" would not match "_Movie_"; normalising the
// separators first makes the word-boundary checks below behave as intended.
function words(title) {
  return (title || '').replace(/[._]+/g, ' ')
}

export function isBatch(title) {
  // An episode range ("Show 1-162", "[1-79]") is a batch just as much as the word
  // "batch" is, and picking one episode out of a range is not reliable.
  //
  // Every separator release groups actually use is included, not just the hyphen:
  // "[Erai-raws] Show - 01 ~ 12 [1080p]" is a 12-episode batch, and reading it as
  // episode 1 meant a whole season was offered as a single episode.
  return /\b(batch|complete|season\s*\d+|box\s*\d+|set\s*\d+|\bs\d{2}\b|\[s\d+\]|\d{1,4}\s*[-~–—]\s*\d{1,4}\b)/i.test(
    words(title),
  )
}

// A release that is part of the franchise but not the series itself: a movie, an
// OVA, a recap, or a trailer. These name no episode number, so without this check
// they pass the episode filter for free and can outrank real episodes - searching
// "Naruto 1" returned "Naruto Shippuden - The Movie" first.
export function isExtra(title) {
  // Plurals are included because release names use them freely ("Naruto Movies",
  // "OVAs", "Specials").
  return /\b(movies?|films?|ovas?|onas?|specials?|recaps?|extras?|trailers?|promos?|previews?|compilations?|nced|ncop|openings?|endings?)\b/i.test(
    words(title),
  )
}

// A film is often named without the word "movie" ("One Piece - Film Red 2022",
// "Baron Omatsuri and the Secret Island (2005)"). A year in brackets is the signal:
// episode releases do not carry one, and this is only consulted for releases that
// name no episode at all.
export function isLikelyMovie(title) {
  return /\((?:19|20)\d{2}\)/.test(words(title))
}

// Codecs a browser can play. HEVC and AV1 are absent on purpose: they fail in most
// browsers, and fixing them would need a real transcode rather than a stream copy,
// so they are excluded from the candidate list entirely.
export function isPlayableCodec(title) {
  const codec = parseCodec(title)
  return codec === null || codec === 'h264'
}

// Whether a *file name* is a container a browser can play without this server
// touching it.
//
// This is the check used by the debrid source, where there is no transcode to fall back
// on because the bytes go straight to the browser. MKV is accepted alongside MP4:
// it is not a web format, but it was verified playing directly in Chrome, H.264 and
// HEVC alike, which is the only thing that matters here. What is inside the
// container is not checked, because a file name does not reliably carry the codecs
// and a wrong guess only costs one playback attempt.
export function isBrowserPlayableFile(name) {
  return /\.(mp4|mkv)$/i.test(name || '')
}

// The quality signals a release carries, independent of whether it is the right shape
// for the request. Shared by the episode scorer and the pack scorer so the two cannot
// drift apart on what "better quality" means.
function qualityScore(title) {
  let score = 0

  // 1080p is the sweet spot for streaming: high quality without the very large
  // files of 2160p, which take much longer to buffer from a torrent.
  const resolution = parseResolution(title)
  if (resolution === 1080) score += 30
  else if (resolution === 720) score += 20
  else if (resolution === 2160) score += 15
  else if (resolution === 480) score += 10
  else if (resolution !== null && resolution < 480) score -= 10

  const codec = parseCodec(title)
  if (codec === 'h264') score += 15

  const audio = parseAudio(title)
  if (audio === 'compressed') score += 10
  else if (audio === 'lossless') score -= 5 // needs an audio transcode

  if (/\b(web-?dl|web-?rip|bdrip|bluray)\b/i.test(title)) score += 5

  return score
}

// Ranks a release for a specific episode. Higher is better.
//
// Reached by the `named` and `fallback` groups in rankReleases, so a release naming no
// episode is scored too - which is what the batch and no-episode penalties below are
// for. The season-pack group has its own scorer (see scorePacks).
export function scoreRelease(release) {
  const title = release.title
  const batch = isBatch(title)

  let score = qualityScore(title)

  // A release that names no episode is only acceptable as a single-file torrent,
  // which openRelease verifies. A batch is a last resort, because picking the
  // right file out of one is not reliable.
  if (parseEpisode(title) === null) score += batch ? -10 : 0

  if (batch) score -= 30
  else score += 20

  // Seeders only break ties, so a well-seeded 480p release never beats a 1080p one.
  score += Math.min(release.seeders || 0, 100) / 100

  return score
}

export function normalizeTitle(text) {
  return (text || '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s]/gu, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

// Release names usually begin with group and quality tags such as
// "[SubsPlease]" or "[Group] ". Those are stripped before comparing titles.
// This runs on the raw title, because normalizing removes the brackets.
function stripLeadingTags(release) {
  return (release || '').replace(/^(\[[^\]]*\]\s*)+/, '').trim()
}

// A release for the wrong show must never be chosen. Searching for "Naruto" also
// returns "Boruto - Naruto Next Generations", and searching for "One Piece"
// returns "One Piece Fan Letter", so plain containment is not enough.
//
// The gate is: the requested title must appear at the start of the release name,
// followed by a word boundary. Everything after it (episode number, quality tags,
// fansub group) is free-form.
//
// Single-word titles are additionally allowed to appear inside a bracketed group
// tag, because some groups prefix the show name: "[Naruto] Episode 1".
export function isRelevant(releaseTitle, animeTitle) {
  const wanted = normalizeTitle(animeTitle)
  if (!wanted) return true

  const raw = releaseTitle || ''
  const release = normalizeTitle(stripLeadingTags(raw))
  if (release === wanted || release.startsWith(`${wanted} `)) return true

  // Group-prefixed naming, for example "[Naruto] Episode 1".
  if (!wanted.includes(' ') && new RegExp(`\\[${wanted}\\]`, 'i').test(raw)) return true

  return false
}

// A release that names an episode must never be served as a film. Every series
// release carries an SxxExx or a season token, so their presence is the signal.
//
// This is the check that matters most for the film path: searching for a film by
// title also returns its series, and a series episode is not a film.
//
// A bare number is deliberately NOT enough on its own. A film's title is full of bare
// numbers - the sequel number ("Dhamaal 4", "Stree 2", "Bhool Bhulaiyaa 3") and the
// year ("Singham Again 2024") - and parseEpisode matches both. Filtering on it threw
// away 7 of 10 real films, which is why searching for a film reported "no source"
// while the indexers held 73-seeded releases for it.
//
// So a bare number counts as an episode only when the release does NOT name a year.
// The year is what distinguishes the two cases, and it is why the film's own year is
// passed in:
//
//   "Dhamaal 4 (2026) 720p WEBRip"  year present -> the 4 is a sequel number
//   "Fargo - 05 1080p WEB-DL"       no year      -> the 05 is an episode
//   "Singham Again 2024 1080p"      year present -> the 2024 is the year
//   "Fargo.S01E01.1080p"            SxxExx       -> rejected outright
//
// A release that names no year and no SxxExx but does carry a bare number is treated
// as an episode. That is the conservative choice: serving an episode as a film is
// worse than reporting no source. The trailing-number form is checked separately
// because parseEpisode's bare pattern needs a word after the number, so it misses
// "Fargo 05" - while a resolution tag ("Dangal 1080p") must still be admitted.
//
// @param {string} title the release name.
// @param {number|null} [year] the film's own release year, when known.
export function isEpisodeRelease(title, year = null) {
  if (/\bS\d{1,2}E\d{1,4}\b/i.test(words(title))) return true
  if (isBatch(title)) return true
  // A release naming a year is a film release, whatever numbers else it carries.
  if (year !== null && parseYear(title) !== null) return false
  if (parseEpisode(title) !== null) return true
  return /(?:^|\s)\d{1,4}\s*$/.test(words(title))
}

// The year a release names, or null.
//
// A film is identified by title *and* year - "Dangal" alone is ambiguous, and
// remakes share titles. The year appears as "(2016)" or "[2016]" in scene naming and
// bare in others ("Dangal 2016 1080p").
export function parseYear(title) {
  const match = /[([]?((?:19|20)\d{2})[)\]]?/.exec(words(title))
  return match ? Number.parseInt(match[1], 10) : null
}

// Ranks a release for a film. Higher is better.
//
// Differs from scoreRelease in what it rewards: a film is one large file, so 2160p
// is not the liability it is for an episode and is scored above 1080p, and there is
// no episode to match.
export function scoreMovieRelease(release, year) {
  const title = release.title
  let score = 0

  const releaseYear = parseYear(title)
  // The right year is the strongest signal available that this is the right film
  // rather than a remake, a sequel or an unrelated title.
  if (year && releaseYear === year) score += 40
  else if (year && releaseYear !== null) score -= 10

  const resolution = parseResolution(title)
  if (resolution === 2160) score += 30
  else if (resolution === 1080) score += 28
  else if (resolution === 720) score += 18
  else if (resolution === 480) score += 8
  else if (resolution !== null && resolution < 480) score -= 10

  const codec = parseCodec(title)
  if (codec === 'h264') score += 15

  const audio = parseAudio(title)
  if (audio === 'compressed') score += 10
  else if (audio === 'lossless') score -= 5 // needs an audio transcode

  if (/\b(web-?dl|web-?rip|bdrip|bluray|remux)\b/i.test(title)) score += 5

  score += Math.min(release.seeders || 0, 100) / 100

  return score
}

/**
 * Ranks releases for a film.
 *
 * Only releases that pass the relevance gate and do not look like an episode are
 * considered. A release naming the requested year is ordered first, so a remake is
 * not served in place of the film that was asked for.
 *
 * @param {Array} releases indexer rows.
 * @param {number} year the film's release year, or null when unknown.
 * @param {string} title the film title, used by the relevance gate.
 * @param {string[]} titles every title the film is known by.
 */
export function rankMovieReleases(releases, year, title, titles) {
  const known = titles?.length ? titles : [title]

  const usable = releases
    .filter((release) => isPlayableCodec(release.title))
    .filter((release) => matchesAnyTitle(release.title, known))
    // A series episode is not a film, and this is a hard filter: without it a
    // season pack or a stray episode outranks the film itself. The year is passed
    // because it is what distinguishes a sequel number from an episode number.
    .filter((release) => !isEpisodeRelease(release.title, year))

  const yearMatches = usable.filter((release) => parseYear(release.title) === year)
  const others = usable.filter((release) => !yearMatches.includes(release))

  // The year decides the order, but quality and seeders order within each group.
  const byScore = (group) =>
    group
      .map((release) => ({ ...release, score: scoreMovieRelease(release, year) }))
      .filter((release) => release.score > 0)
      .sort((a, b) => {
        const tier = healthTier(a.seeders || 0) - healthTier(b.seeders || 0)
        if (tier !== 0) return tier
        return b.score - a.score
      })

  // Only when the year is known: with no year there is nothing to prefer, and
  // treating every release as "not the year" would empty the first group anyway.
  if (!year) return byScore(usable)

  return [...byScore(yearMatches), ...byScore(others)]
}

// Rejects a release that names an episode other than the one requested, and for a
// series also rejects a release from a different season.
//
// Serving the wrong episode silently is worse than serving nothing, so this is a
// hard filter rather than a score penalty. For a series the season is part of the
// episode's identity: "S02E05" must not satisfy a request for "S01E05".
export function matchesEpisode(releaseTitle, episodeNumber, season) {
  const episode = parseEpisode(releaseTitle)
  if (episode !== null && episode !== episodeNumber) return false
  if (season === undefined || season === null) return true

  const releaseSeason = parseSeason(releaseTitle)
  // A release naming no season is accepted: some groups omit it, and the episode
  // number alone is still a real signal.
  if (releaseSeason === null) return true
  return releaseSeason === season
}

// Seeders decide whether a release can be played at all, but they were only a
// tie-break, so a 2-seeder release outranked a 379-seeder one for the same episode
// purely because it named its codec. Releases are therefore grouped by how likely
// they are to stream, and quality (scoreRelease) only orders releases within a
// group. The thresholds are the ones the Stremio/Torrentio ecosystem publishes.
function healthTier(seeders) {
  if (seeders >= 50) return 0 // healthy: streams smoothly
  if (seeders >= 5) return 1 // seeded: works
  if (seeders >= 1) return 2 // barely seeded
  return 3 // no seeders reported
}

function scoreAll(releases) {
  return releases
    .map((release) => ({ ...release, score: scoreRelease(release) }))
    .filter((release) => release.score > 0)
    .sort((a, b) => {
      const tier = healthTier(a.seeders || 0) - healthTier(b.seeders || 0)
      if (tier !== 0) return tier
      return b.score - a.score
    })
}

// Whether a release belongs to any of the titles a show is known by.
//
// A show has several names and releases use them inconsistently: an English title
// ("Attack on Titan") and its romaji ("Shingeki no Kyojin") both appear, so
// checking a single name rejects correct releases. A title with no comparable
// characters (a CJK-only name against ASCII release names) cannot gate anything,
// so it is ignored and the episode number alone decides.
export function matchesAnyTitle(releaseTitle, animeTitles) {
  const wanted = (animeTitles || []).map(normalizeTitle).filter(Boolean)
  if (wanted.length === 0) return true

  if (wanted.some((title) => isRelevant(releaseTitle, title))) return true

  // Nothing was comparable, so there is no title signal to use.
  return !wanted.some((title) => /[a-z]/.test(title))
}

// Whether a season pack holds the requested episode.
//
// A pack names the season instead of the episode, so it can only be used when its
// season is the one being watched. Regional and Indian series are released almost
// entirely as packs ("Gyaarah Gyaarah S01 [1080p ...]", "Panchayat S02 Complete ..."),
// so without this a series that exists only as a pack reports "no torrent found" while
// the indexers hold it with 71 seeders.
//
// A pack spanning several seasons is refused outright. It is many times the size of the
// one being watched, and its file names restart at 01 for each season, so "S03E05.mkv"
// is indistinguishable from "S01E05.mkv" by number alone. The anime path has no season
// at all to disambiguate with, so a later season's pack could not be mapped onto it
// either: "Shippuden S02" holds episodes 33-53 but names them 01-21, and reading file
// "01" out of it would serve episode 33 as episode 1.
function packSeasonMatches(title, season) {
  // Both separators a range is written with: "S01-S05" and "S01 S05". Underscores and
  // dots have already become spaces by this point, so "Show.S01.S05.Complete" arrives
  // as "Show S01 S05 Complete" and has to be caught by the space form.
  if (/S\d{1,2}(?:\s*[-~–—]\s*S?|\s+S)\d{1,2}\b/i.test(words(title))) return false

  const releaseSeason = parseSeason(title)
  if (releaseSeason === null) return false
  // Only a first-season pack can be mapped onto the anime path, whose episode ids are a
  // flat count starting where season 1 does.
  if (season === null || season === undefined) return releaseSeason === 1
  return releaseSeason === season
}

// Scores season packs for the last-resort group.
//
// Deliberately not scoreRelease: that scorer applies a -30 batch penalty, which is
// right when a pack competes with a real episode but would drive a perfectly good pack
// below zero and drop it from the list entirely - the exact bug this group exists to
// fix. The group is already ordered last, so the batch penalty has nothing left to say.
// What is scored here is the quality the pack actually carries, and the size is
// subtracted so a pack covering a few episodes is preferred over one covering five
// seasons.
//
// There is no positive floor and none is applied: the terse regional release names this
// group exists for ("Show S01") carry no quality tag at all, and filtering on a positive
// score would drop exactly those. Every release here already passed the title, codec and
// season gates, so a low score means "no quality information", not "unusable".
function scorePacks(packs) {
  const SIZE_CAP = 64 * 1024 ** 3
  return packs
    .map((release) => ({
      ...release,
      score:
        qualityScore(release.title) +
        Math.min(release.seeders || 0, 100) / 100 -
        Math.min(release.sizeBytes || 0, SIZE_CAP) / SIZE_CAP,
    }))
    .sort((a, b) => {
      const tier = healthTier(a.seeders || 0) - healthTier(b.seeders || 0)
      if (tier !== 0) return tier
      return b.score - a.score
    })
}

export function rankReleases(releases, episodeNumber, animeTitle, animeTitles, season) {
  const titles = animeTitles?.length ? animeTitles : [animeTitle]

  // When no title can be compared against a release name, relevance passes
  // everything, so the episode number becomes the only reliable signal and is
  // therefore mandatory.
  const requireNamedEpisode = !titles.some((title) => /[a-z]/i.test(normalizeTitle(title)))

  const usable = releases
    .filter((release) => isPlayableCodec(release.title))
    .filter((release) => matchesAnyTitle(release.title, titles))
    .filter((release) => matchesEpisode(release.title, episodeNumber, season))
    .filter((release) => !requireNamedEpisode || parseEpisode(release.title) === episodeNumber)

  // A release that names the requested episode is always the right answer, so those
  // are ordered first. This is what stops a movie or a trailer from being served for
  // a specific episode: they name no episode number, so they can never outrank one.
  const named = usable.filter((release) => parseEpisode(release.title) === episodeNumber)

  // Fallbacks name no episode at all and are neither a movie/OVA nor a collection.
  // openRelease still has to confirm the torrent holds exactly one video file, so a
  // wrong pick is rejected there rather than played.
  const fallback = usable.filter(
    (release) =>
      !named.includes(release) &&
      !isExtra(release.title) &&
      !isBatch(release.title) &&
      !isLikelyMovie(release.title),
  )

  // A season pack is a last resort, ordered after a release that names the episode and
  // after a single-file fallback. It is admitted only when its season matches, which
  // is what keeps a season-2 pack from satisfying a request for season 1 - the season
  // gate in matchesEpisode already rejected a mismatched one, and this repeats the
  // check because a pack is the one case where the wrong season is silently playable.
  //
  // Extras are excluded for the same reason the fallback group excludes them: a specials
  // pack ("Show S01 Specials 1080p") names no episode, so it would otherwise be offered
  // for a real episode of that season.
  const packs = usable.filter(
    (release) =>
      !named.includes(release) &&
      !fallback.includes(release) &&
      !isExtra(release.title) &&
      !isLikelyMovie(release.title) &&
      packSeasonMatches(release.title, season),
  )

  return [...scoreAll(named), ...scoreAll(fallback), ...scorePacks(packs)]
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
const MAX_TERMS = 8

// Collects the terms for a set of titles, applying the cap per title variant.
//
// The cap is per variant rather than over the whole list, and for the current form
// counts it does not bind: a series has at most 5 forms and an anime at most 4. It is
// kept as a bound for a future form list rather than as the thing limiting the fan-out
// today. What actually bounds the requests is the number of title variants a provider
// supplies (AniList gives up to 3) multiplied by the form count, times the number of
// indexers - which is why the fan-out is issued in bounded batches and the indexer
// timeout exists.
//
// Capping the whole list instead is what this replaced: the first variant consumed the
// budget and the rest lost their season and bare-title forms, which are the forms a
// pack-only series is found by.
function collectTerms(bases, formsFor) {
  const terms = []
  for (const base of bases) {
    let added = 0
    for (const form of formsFor()) {
      const term = form ? `${base} ${form}` : base
      if (terms.includes(term)) continue
      terms.push(term)
      added += 1
      if (added >= MAX_TERMS) break
    }
  }
  return terms
}

function episodeForms(episodeNumber) {
  const number = String(episodeNumber)
  // A three-digit episode is left alone by padStart, so long-running shows such as
  // ONE PIECE (1104) keep their real number instead of being truncated.
  return [number, number.padStart(2, '0'), number.padStart(3, '0'), `S01E${number.padStart(2, '0')}`]
}

export function searchTerms(animeTitles, episodeNumber) {
  const bases = normalizeBases(animeTitles)
return collectTerms(bases, () => (episodeNumber ? episodeForms(episodeNumber) : ['']))
}

// Series search terms.
//
// The episode number is written with a season ("S01E05") or without it ("05"), and
// indexers match literally, so both forms are searched. The season is included
// because series releases almost always carry it, and without it the search returns
// every season of the show mixed together.
//
// The season alone and the bare title are searched last, and they are what make a
// season-pack-only series findable. Regional and Indian series are released almost
// entirely as packs ("Gyaarah Gyaarah S01 [1080p ...]", "Mirzapur S01 Hindi 1080p"),
// and knaben requires every word of a term to match, so "Mirzapur S01E01" returns 0
// hits where "Mirzapur S01" returns 15 and "Mirzapur" returns 100. They come last so
// the episode-specific forms still decide the ordering whenever they return anything.
export function seriesSearchTerms(titles, season, episodeNumber) {
  const bases = normalizeBases(titles)
  const seasonNumber = Number.parseInt(season, 10) || 1
  const episode = Number.parseInt(episodeNumber, 10)
  if (!Number.isFinite(episode)) return bases

  const seasonTag = `S${String(seasonNumber).padStart(2, '0')}`

  return collectTerms(bases, () => [
    `${seasonTag}E${String(episode).padStart(2, '0')}`,
    String(episode).padStart(2, '0'),
    String(episode),
    seasonTag,
    '',
  ])
}

// Film search terms.
//
// The year is what distinguishes a film from its remakes and from its series, so it
// is searched as part of the term. It is optional: YTS is searched by title and
// matches loosely, so a term without the year still finds the film.
export function movieSearchTerms(titles, year) {
  const bases = normalizeBases(titles)
  return collectTerms(bases, () => (year ? [`${year}`, ''] : ['']))
}

// The comparable form of every title a work is known by.
//
// Unicode-aware (\p{L}\p{N}) so a CJK-only title survives instead of being stripped to
// nothing by an ASCII-only \w.
function normalizeBases(titles) {
  return (Array.isArray(titles) ? titles : [titles])
    .map((title) => (title || '').replace(/[^\p{L}\p{N}\s-]/gu, ' ').replace(/\s+/g, ' ').trim())
    .filter(Boolean)
}
