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
// number, for example "Season 01 Part 1 [1080p]".
const PART_WORDS = /\b(?:part|vol\.?|volume|season|set|box|cour|movies?|films?)\s*$/i

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
// browsers, and fixing them would need a real transcode rather than a cheap remux,
// so they are excluded from the candidate list entirely.
export function isPlayableCodec(title) {
  const codec = parseCodec(title)
  return codec === null || codec === 'h264'
}

// Whether a *file name* is a container a browser can play without this server
// touching it.
//
// This is the check used by the debrid source, where there is no remux to fall back
// on because the bytes go straight to the browser. MKV is accepted alongside MP4:
// it is not a web format, but it was verified playing directly in Chrome, H.264 and
// HEVC alike, which is the only thing that matters here. What is inside the
// container is not checked, because a file name does not reliably carry the codecs
// and a wrong guess only costs one playback attempt.
export function isBrowserPlayableFile(name) {
  return /\.(mp4|mkv)$/i.test(name || '')
}

// Ranks a release for a specific episode. Higher is better.
//
// Only releases that name the requested episode reach this point (see
// rankReleases), so the episode match is not scored here.
export function scoreRelease(release) {
  const title = release.title
  let score = 0

  const batch = isBatch(title)

  // A release that names no episode is only acceptable as a single-file torrent,
  // which openRelease verifies. A batch is a last resort, because picking the
  // right file out of one is not reliable.
  if (parseEpisode(title) === null) score += batch ? -10 : 0

  if (batch) score -= 30
  else score += 20

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

// Rejects a release that names an episode other than the one requested.
// Serving the wrong episode silently is worse than serving nothing, so this is a
// hard filter rather than a score penalty.
export function matchesEpisode(releaseTitle, episodeNumber) {
  const episode = parseEpisode(releaseTitle)
  return episode === null || episode === episodeNumber
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

export function rankReleases(releases, episodeNumber, animeTitle, animeTitles) {
  const titles = animeTitles?.length ? animeTitles : [animeTitle]

  // When no title can be compared against a release name, relevance passes
  // everything, so the episode number becomes the only reliable signal and is
  // therefore mandatory.
  const requireNamedEpisode = !titles.some((title) => /[a-z]/i.test(normalizeTitle(title)))

  const usable = releases
    .filter((release) => isPlayableCodec(release.title))
    .filter((release) => matchesAnyTitle(release.title, titles))
    .filter((release) => matchesEpisode(release.title, episodeNumber))
    .filter((release) => !requireNamedEpisode || parseEpisode(release.title) === episodeNumber)

  // A release that names the requested episode is always the right answer, so those
  // are ordered first. This is what stops a movie or a trailer from being served for
  // a specific episode: they name no episode number, so they can never outrank one.
  const named = usable.filter((release) => parseEpisode(release.title) === episodeNumber)

  // Fallbacks name no episode at all and are neither a movie/OVA nor a season pack.
  // openRelease still has to confirm the torrent holds exactly one video file, so a
  // wrong pick is rejected there rather than played.
  const fallback = usable.filter(
    (release) =>
      !named.includes(release) &&
      !isExtra(release.title) &&
      !isBatch(release.title) &&
      !isLikelyMovie(release.title),
  )

  return [...scoreAll(named), ...scoreAll(fallback)]
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
// terms are fanned out and the results merged. The list is capped because each term
// costs one request per indexer.
const MAX_TERMS = 8

function episodeForms(episodeNumber) {
  const number = String(episodeNumber)
  // A three-digit episode is left alone by padStart, so long-running shows such as
  // ONE PIECE (1104) keep their real number instead of being truncated.
  return [number, number.padStart(2, '0'), number.padStart(3, '0'), `S01E${number.padStart(2, '0')}`]
}

export function searchTerms(animeTitles, episodeNumber) {
  const bases = (Array.isArray(animeTitles) ? animeTitles : [animeTitles])
    // Unicode-aware so a CJK-only title survives instead of being stripped to
    // nothing by an ASCII-only \w.
    .map((title) => (title || '').replace(/[^\p{L}\p{N}\s-]/gu, ' ').replace(/\s+/g, ' ').trim())
    .filter(Boolean)

  const terms = []
  for (const base of bases) {
    const forms = episodeNumber ? episodeForms(episodeNumber) : ['']
    for (const form of forms) {
      const term = form ? `${base} ${form}` : base
      if (!terms.includes(term)) terms.push(term)
      if (terms.length >= MAX_TERMS) return terms
    }
  }
  return terms
}
