// archive.org stream source.
//
// The Internet Archive is not a torrent index and not a debrid service: it is a public
// library that stores the file itself and serves it over plain HTTPS. That makes it a
// genuinely different kind of source, and it is here for one measured reason - it is
// the only place a regional or Indian series can be found when no torrent indexer has
// it at all.
//
// Measured from the production server for "India's Got Latent", which every configured
// indexer answers with nothing:
//
//   knaben 0   torrents-csv 0   eztv 0   nyaa 0   subsplease []
//
// archive.org holds the episodes, and its own detail page already exposes a browser
// playable h264 derivative with real range support:
//
//   GET .../download/<id>/<name>.mp4
//     Range: bytes=0-1023        -> 206, Content-Range: bytes 0-1023/197190368
//     Range: bytes=90000000-...  -> 206
//     Accept-Ranges: bytes, Access-Control-Allow-Origin: *
//
// So this source does not join a swarm, does not download anything, and does not remux.
// It returns a direct URL and the bytes go straight from archive.org to the viewer.
//
// It is deliberately the last source tried: see STREAM_SOURCES ordering. Everything
// here reports zero seeders, so it can never outrank a real torrent, and it should only
// ever answer when nothing else can.

import { config } from '../../../lib/config.js'
import { http } from '../../../lib/http.js'
import { cached, ttlFor } from '../../../lib/cache.js'
import { parseEpisode, parseSeason, titleVariants } from '../releases.js'

const TTL_MS = 30 * 60 * 1000

// How many items the search may return before the per-item detail lookups begin. Each
// item costs one extra request, so this bounds the work as well as the result count.
const MAX_ITEMS = 6

// How many title spellings are searched. The bare-title forms are tried first, so this
// covers every apostrophe spelling of the show name before falling back to the
// episode-bearing terms.
const MAX_QUERIES = 5

// Formats archive.org derives from an upload, best first. A derivative is preferred
// over the original because the original may be an MKV or a format no browser plays,
// while these are h264 in MP4 - the one combination every device handles.
//
// "h.264 IA" is archive.org's own browser-streamable derivative and is what its own
// player uses. "MPEG4" is the plain MP4. The rest are older fallbacks that are still
// better than an unplayable original.
const PLAYABLE_FORMATS = ['h.264 IA', 'MPEG4', 'h.264', '512Kb MPEG4', 'Ogg Video', 'WebM']

/**
 * Whether a search result is plausibly the title that was asked for.
 *
 * The search endpoint is a phrase match, so it is reasonably tight, but it also returns
 * commentary, reviews and news clips about a show ("India's Got Latent Row: I&B Ministry
 * Ticks Off OTT Players"). Those are rejected by the episode gate below, not here.
 *
 * The comparison is spelling-insensitive on *both* sides. archive.org stores an item
 * under whichever spelling the uploader used, and each apostrophe spelling returns a
 * different set of items, so an item surfaced by one query may be titled with another
 * ("INDIA S GOT LATENT S 2 EP 6" against the query "Indias Got Latent"). Normalising
 * only the wanted title rejected those items outright, which silently discarded the
 * very results the multi-spelling search exists to find.
 *
 * Exported so it can be tested against a captured response.
 *
 * @param {string} name the item title.
 * @param {string[]} titles every spelling of the title being looked for.
 */
export function matchesTitle(name, titles) {
  const wanted = (titles || []).flatMap(titleVariants)
  if (wanted.length === 0) return true

  // Compared with every non-alphanumeric character removed from both sides. That is
  // what makes the three spellings of one title equal: "India's Got Latent",
  // "Indias Got Latent" and "India s Got Latent" all reduce to "indiasgotlatent".
  //
  // Comparing the normalised forms instead does not work, because normalisation is
  // driven by the apostrophe: it rewrites "India's" to "India s" but leaves "Indias"
  // untouched, so the two never converge and an item titled with one spelling is
  // rejected even when the search found it under another.
  const squash = (text) =>
    String(text || '')
      .toLowerCase()
      .replace(/[^\p{L}\p{N}]/gu, '')

  const item = squash(name)
  return wanted.some((title) => item.includes(squash(title)))
}

/**
 * The files worth offering for one item, best first.
 *
 * archive.org lists every derivative alongside the original, the thumbnails and the
 * item's own metadata files. Only a playable format with a real size is a candidate.
 *
 * Exported so it can be tested against a captured response.
 *
 * @param {object} metadata an archive.org /metadata/<id> response.
 */
export function playableFiles(metadata) {
  const files = Array.isArray(metadata?.files) ? metadata.files : []
  const candidates = []

  for (const file of files) {
    const format = String(file?.format || '')
    const name = String(file?.name || '')
    const size = Number(file?.size)
    if (!name || !Number.isFinite(size) || size <= 0) continue
    if (!PLAYABLE_FORMATS.includes(format)) continue
    candidates.push({ name, size, format })
  }

  // Best format first, then largest: an item often carries both a 480p and a 720p
  // derivative, and the larger one is the higher quality.
  return candidates.sort((a, b) => {
    const rank = PLAYABLE_FORMATS.indexOf(a.format) - PLAYABLE_FORMATS.indexOf(b.format)
    if (rank !== 0) return rank
    return b.size - a.size
  })
}

/**
 * Whether a name identifies the episode that was asked for.
 *
 * Returns true when it names this episode, false when it names a different one, and
 * null when it names no episode at all and therefore cannot be checked.
 *
 * The season is only enforced when the caller supplies one. The anime path does not:
 * its episode ids are a flat count with no season, so an anime request for episode 5
 * is satisfied by an item titled "S 2 EP 5". That is deliberate and matches the torrent
 * path, where the season is likewise absent for anime - an anime's later seasons are
 * separate catalogue entries whose episodes restart at 1, so treating a named season as
 * authoritative would refuse every episode above season 1.
 *
 * Exported so it can be tested against a captured response.
 *
 * @param {string} name an item title or a file name.
 * @param {number} episodeNumber the episode being watched.
 * @param {number|null} season the season being watched, when known.
 */
export function identifiesEpisode(name, episodeNumber, season) {
  const named = parseEpisode(name)
  if (named !== null) {
    if (named !== episodeNumber) return false
    if (season !== null && season !== undefined) {
      const namedSeason = parseSeason(name)
      if (namedSeason !== null && namedSeason !== season) return false
    }
    return true
  }

  // No episode number. A season on its own is still an identity for a pack, but not
  // for one episode inside it, so it can only rule a name out, never confirm it.
  if (season !== null && season !== undefined) {
    const namedSeason = parseSeason(name)
    if (namedSeason !== null && namedSeason !== season) return false
  }
  return null
}

/**
 * Picks the file to serve, or null when the episode cannot be confirmed.
 *
 * Serving the wrong episode silently is worse than serving nothing, so an item is only
 * accepted when its title or the file itself actually names the requested episode.
 *
 * This is the check that stops commentary being served as an episode. A search for
 * "India's Got Latent" returns news clips and reviews alongside the episodes, and a
 * clip holds exactly one video file that names no episode - so "the item has one video,
 * so it must be the episode" admitted a 650MB news package as S2E5. Holding one file
 * proves nothing about which episode it is.
 *
 * Exported so it can be tested against a captured response.
 *
 * @param {{ title: string }} item the search result.
 * @param {Array<{ name: string }>} files the item's playable files, best first.
 * @param {number} episodeNumber the episode being watched, or null for a film.
 * @param {number|null} season the season being watched, when known.
 */
export function pickFile(item, files, episodeNumber, season) {
  if (files.length === 0) return null

  // A film has no episode number, so nothing can be checked and the best file wins.
  if (episodeNumber === null || episodeNumber === undefined) return files[0]

  const itemVerdict = identifiesEpisode(item?.title || '', episodeNumber, season)
  if (itemVerdict === false) return null
  if (itemVerdict === true) {
    // The item is the right episode, so a file that names a different one is a
    // mislabelled extra and is skipped. When every file names a different episode the
    // item is inconsistent with itself, and serving the first one anyway would be
    // serving a known-wrong episode - so nothing is returned.
    return files.find((file) => identifiesEpisode(file.name, episodeNumber, season) !== false) || null
  }

  // The item title names no episode, so the file name is the only remaining identity.
  return files.find((file) => identifiesEpisode(file.name, episodeNumber, season) === true) || null
}

/**
 * Builds the direct download URL for a file.
 *
 * The /download/ path redirects to a storage node, and the redirect is followed by the
 * browser or the player. It is the same URL archive.org's own player uses.
 *
 * @param {string} identifier the item identifier.
 * @param {string} name the file name inside the item.
 */
export function downloadUrl(identifier, name) {
  return `${config.archiveOrgBaseUrl}/download/${encodeURIComponent(identifier)}/${encodeURIComponent(name)}`
}

async function searchItems(term, timeoutMs) {
  const { data } = await http.get(`${config.archiveOrgBaseUrl}/advancedsearch.php`, {
    params: {
      q: `title:("${term}") AND mediatype:movies`,
      // The fields must be sent as `fl[]` repeated, which is axios's default array
      // serialization. Forcing indexed form produced `fl=identifier&fl=title`, which
      // archive.org accepts silently while returning rows with no identifier - every
      // row was then skipped as unusable, and the source reported "nothing found" for
      // a title it had actually matched.
      'fl[]': ['identifier', 'title'],
      rows: MAX_ITEMS * 3,
      output: 'json',
    },
    responseType: 'json',
    timeout: timeoutMs,
  })

  const docs = data?.response?.docs
  if (!Array.isArray(docs)) throw new Error('not a search response')
  // An identifier is used as a URL path segment, so it is restricted to the characters
  // archive.org itself issues. encodeURIComponent already makes traversal impossible;
  // this is the cheap second check, and it also drops the rows that come back with a
  // non-string identifier.
  return docs.filter((doc) => typeof doc?.identifier === 'string' && IDENTIFIER.test(doc.identifier))
}

// archive.org identifiers are lowercase alphanumeric with dots, dashes and underscores.
const IDENTIFIER = /^[A-Za-z0-9._-]+$/

async function itemMetadata(identifier, timeoutMs) {
  const { data } = await http.get(`${config.archiveOrgBaseUrl}/metadata/${encodeURIComponent(identifier)}`, {
    responseType: 'json',
    timeout: timeoutMs,
  })
  if (!data || typeof data !== 'object') throw new Error('not a metadata response')
  return data
}

export default {
  name: 'archive-org',

  ready() {
    return true
  },

  /**
   * `ranked` is accepted because every source receives it, and ignored: this source
   * looks a title up directly rather than choosing among indexer results, which is what
   * lets it answer when the indexers found nothing at all.
   *
   * @param {{ episodeNumber: number, season: number|null, titles: string[], terms: string[],
   *           deadlineMs: number }} input
   * @returns {Promise<{ available: boolean, url?: string, streamType?: string,
   *                     release?: string, reason?: string }>}
   */
  async resolve({ episodeNumber, season = null, titles = [], terms = [], deadlineMs }) {
    const deadline = Date.now() + deadlineMs
    const left = () => deadline - Date.now()

    // What to search for, best first.
    //
    // archive.org's phrase search matches the whole title, and its items are named
    // after the show ("INDIA S GOT LATENT S 2 EP 5 Ft. Orry"), so searching the show
    // name and reading the episode out of the results is both more accurate and far
    // fewer requests than searching per episode number.
    //
    // Every spelling is kept, because each apostrophe spelling returns a *different*
    // set of items on archive.org - measured, the union is what finds the episodes.
    // Capping this list is what made the source report "nothing found" for a title it
    // had actually matched.
    const candidates = [...new Set([...titles, ...terms].filter(Boolean))]
    // A title-only form is one with no digits at all. Testing for a season or episode
    // pattern is not enough: "Indias Got Latent S02E05" has no standalone digit run for
    // a word-boundary pattern to catch, so it was counted as a title and pushed the
    // bare spaced spelling ("India s Got Latent") - the one that actually finds the
    // episodes - past the cap.
    const bare = candidates.filter((query) => !/\d/.test(query))
    const rest = candidates.filter((query) => !bare.includes(query))
    const queries = [...bare, ...rest].slice(0, MAX_QUERIES)
    if (queries.length === 0) {
      return { available: false, reason: 'No title to look up on archive.org.' }
    }

    const failures = []

    for (const query of queries) {
      if (left() <= 0) break

let docs
      try {
        docs = await cached(`archive-org:search:${query}`, (value) => ttlFor(value, TTL_MS), () =>
          searchItems(query, Math.max(left(), 1)),
        )
      } catch (error) {
        failures.push(`${query}: ${error.message}`)
        continue
      }

      // One search returns many items; each is checked until one holds the episode.
      // The search is cached, so the per-item lookups below are the only work a repeat
      // view pays for, and those are cached too.
      for (const doc of docs.slice(0, MAX_ITEMS)) {
        if (left() <= 0) {
          failures.push('deadline reached')
          break
        }
        if (!matchesTitle(doc.title, [query])) continue

let metadata
        try {
          metadata = await cached(`archive-org:item:${doc.identifier}`, (value) => ttlFor(value, TTL_MS), () =>
            itemMetadata(doc.identifier, Math.max(left(), 1)),
          )
        } catch (error) {
          failures.push(`${doc.identifier}: ${error.message}`)
          continue
        }

        const files = playableFiles(metadata)
        if (files.length === 0) continue

        // The item title identifies the episode when the file name does not, and vice
        // versa. A name that identifies neither is refused rather than guessed at.
        const match = pickFile(doc, files, episodeNumber, season)
        if (!match) continue

        return {
          available: true,
          url: downloadUrl(doc.identifier, match.name),
          // Always a file: archive.org serves an MP4 derivative with real range
          // support, so there is nothing to convert and no playlist to build.
          streamType: 'file',
          release: doc.title,
        }
      }
    }

    if (failures.length > 0) console.warn(`[stream:archive-org] ${failures.join(' | ')}`)
    return { available: false, reason: `Nothing on archive.org for "${queries[0]}".` }
  },
}
