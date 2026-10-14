// Torrentio indexer.
//
// Torrentio is the indexer Stremio itself uses, and it is here for one measured reason
// that no other source in this repository can match: it finds a title by **IMDb id**,
// with the season and episode in the path, instead of by guessing search terms.
//
// "India's Got Latent" is the case that motivated it. Every text-search indexer here
// answers that title with nothing at all - knaben 0, torrents-csv 0, eztv 0, nyaa 0,
// subsplease [] - because the show is named three different ways in the wild
// ("India's Got Latent", "Indias Got Latent", "INDIA S GOT LATENT") and knaben requires
// every word of a query to match. Torrentio answers the same title with the complete
// first season at 47 seeders, and the two season-2 episodes that exist.
//
//   GET /stream/series/tt33094114:1:1.json  -> 3 rows, 47 seeders
//   GET /stream/series/tt33094114:2:1.json  -> 1 row,  3 seeders
//
// The response shape is the Stremio addon protocol, not an indexer's JSON:
//
//   { streams: [ { name, title, infoHash, fileIdx, behaviorHints: { filename } } ] }
//
// `title` is a display string with the release name, the filename, the seeder count and
// the tracker embedded in it and separated by newlines:
//
//   "India's Got Latent\nIGL_EP_01_ft_RaftaarMusic.mp4\n👤 47 💾 473.25 MB ⚙️ ThePirateBay"
//
// So the filename comes from behaviorHints when it is present and from the second line
// otherwise, and the seeder count is read out of the display string. Both are parsed
// defensively: a row whose numbers cannot be read is still usable, because the filename
// and the hash are what the rest of the pipeline actually needs.
//
// Two limits worth stating. Torrentio reports one row per *file* of a multi-file
// torrent, all sharing an infoHash, so the rows are collapsed to one per hash - the
// per-file choice is made later, by pickVideoFile, which reads the real file list from
// the torrent itself rather than trusting a display string. And it is a public
// third-party instance with no published rate limit, so it is asked once per title
// rather than once per search term.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, toHexInfoHash } from '../../lib/magnet.js'
import { isBonus } from '../stream/releases.js'

const TTL_MS = 10 * 60 * 1000

/**
 * The IMDb id in the bare form Torrentio expects, or null.
 *
 * It accepts the "tt" prefix, unlike EZTV which wants the digits alone. Anything that
 * is not an id is rejected rather than sent, because a malformed id returns an empty
 * stream list that is indistinguishable from "no releases".
 */
export function toTorrentioId(imdbId) {
  const value = String(imdbId ?? '').trim()
  return /^tt\d{5,}$/i.test(value) ? value.toLowerCase() : null
}

/**
 * The show name and file name for one stream row.
 *
 * The display title is newline-separated, and it comes in two shapes:
 *
 *   "India's Got Latent\nIGL_EP_01_ft_RaftaarMusic.mp4\n👤 47 💾 473.25 MB ⚙️ ThePirateBay"
 *   "Indias Got Latent (2026)-MovieLinkHub.com-S02 Bouns E01-E01\n👤 3 💾 1.41 GB ⚙️ EXT"
 *
 * The trailing line is always the seeder/size/tracker summary, so it is dropped first;
 * it is not a name of anything. What remains is either two lines (show name, then file
 * name) or one (a release name with no separate file name).
 *
 * Reading the second line without that filter is what made a single-name row look like
 * it had a file called "👤 3 💾 1.41 GB ⚙️ EXT".
 *
 * `behaviorHints.filename` is preferred when present, because it is the structured field.
 *
 * @param {object} stream one entry of the response's `streams` array.
 */
export function streamNames(stream) {
  const hinted = stream?.behaviorHints?.filename
  const filename = typeof hinted === 'string' && hinted.trim() ? hinted.trim() : ''

  const lines = String(stream?.title || '')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line && !/👤|💾/.test(line))

  if (filename) return { showName: lines[0] || '', filename }
  if (lines.length >= 2) return { showName: lines[0], filename: lines[1] }
  return { showName: '', filename: lines[0] || '' }
}

/**
 * The seeder count out of a display string, or 0.
 *
 * The format is "👤 47 💾 473.25 MB ⚙️ ThePirateBay" and is not a documented contract,
 * so a missing or unreadable count degrades to 0 - which places the release in the
 * lowest health tier rather than discarding it.
 */
export function streamSeeders(stream) {
  const match = /👤\s*(\d+)/.exec(String(stream?.title || ''))
  return match ? Number.parseInt(match[1], 10) : 0
}

/**
 * The byte size out of a display string, or null.
 *
 * The size arrives as "473.25 MB" / "1.41 GB", so the unit has to be applied. An
 * unreadable size is null rather than 0: a size is used to rank and to budget disk, and
 * a wrong 0 would read as "tiny file" and could be preferred over a real episode.
 */
export function streamSizeBytes(stream) {
  const match = /💾\s*([\d.]+)\s*(KB|MB|GB|TB)/i.exec(String(stream?.title || ''))
  if (!match) return null

  const units = { kb: 1024, mb: 1024 ** 2, gb: 1024 ** 3, tb: 1024 ** 4 }
  const value = Number.parseFloat(match[1])
  const scale = units[match[2].toLowerCase()]
  return Number.isFinite(value) && scale ? Math.round(value * scale) : null
}

/**
 * Turns a Torrentio response into indexer rows, one per info hash.
 *
 * Torrentio emits one row per *file* of a multi-file torrent, all sharing an infoHash.
 * They are collapsed to a single row per hash, and the file choice is left to
 * pickVideoFile, which reads the torrent's real file list rather than trusting a
 * display string.
 *
 * The row title is the show name followed by the file name, and both halves are needed:
 *
 *   - the file name is where the episode number lives ("IGL_EP_01_ft_RaftaarMusic.mp4"),
 *     so without it the release names no episode and the episode gate has nothing to
 *     match on;
 *   - the show name is what the relevance gate compares against the requested title, so
 *     without it the release looks like it belongs to a different work entirely and is
 *     discarded. A file name alone does not contain the show name, which is why a row
 *     built from the file name alone ranked zero candidates.
 *
 * Which of the sibling rows is kept also matters, because the row title is what those
 * gates read. Keeping the first one is what picked "IGL_Bonus_EP_1_ft. Arpit Bala.mp4"
 * out of the Season 1 pack, and a bonus then failed the episode gate and discarded the
 * whole torrent - which holds every real episode at 47 seeders. So a non-bonus file is
 * preferred over a bonus one.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 */
export function parseSearchResponse(body) {
  const rows = new Map()

  for (const stream of body?.streams || []) {
    const infoHash = toHexInfoHash(stream?.infoHash)
    if (!infoHash) continue

    const { showName, filename } = streamNames(stream)
    if (!filename) continue

    const title = showName && !filename.includes(showName) ? `${showName} ${filename}` : filename

    const existing = rows.get(infoHash)
    // First row wins, except that a bonus never displaces a real episode.
    if (existing && !(isBonus(existing.title) && !isBonus(filename))) continue

    rows.set(infoHash, {
      title,
      infoHash,
      // Rebuilt rather than reused: Torrentio returns no magnet, and the hash inside a
      // magnet has to be the normalised one this application keys on.
      magnet: buildMagnet(infoHash, title, config.trackers),
      sizeBytes: streamSizeBytes(stream),
      seeders: streamSeeders(stream),
      source: 'torrentio',
    })
  }

  return [...rows.values()]
}

export default {
  name: 'torrentio',
  types: ['series', 'movie'],

  // Searched by IMDb id, not by term, so the term itself is irrelevant. This is the
  // flag searchAll uses to ask once per request instead of once per search term.
  byId: true,

  /**
   * @param {{ imdbId?: string, contentType?: string, season?: number|null,
   *           episodeNumber?: number|null }} [options]
   */
  async search(term, options = {}) {
    const imdbId = toTorrentioId(options.imdbId)
    // No id means no answer from this indexer. Returning [] rather than throwing is the
    // documented contract, and it keeps a title without an IMDb id from failing the
    // whole search.
    if (!imdbId) return []

    const { contentType, season, episodeNumber } = options
    if (contentType !== 'series' && contentType !== 'movie') return []

    // A film has no season or episode, and its path is the id alone.
    const path =
      contentType === 'movie'
        ? `movie/${imdbId}`
        : `series/${imdbId}:${season ?? 1}:${episodeNumber ?? 1}`

    return cached(`torrentio:${path}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(`${config.torrentioBaseUrl}/stream/${path}.json`, {
        responseType: 'json',
      })

      // A blocked or replaced host answers 200 with an HTML page, and axios leaves a
      // body it cannot parse as a string rather than throwing. Throwing here means the
      // failure is reported instead of being cached as "no results" for the whole TTL.
      if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')

      return parseSearchResponse(data)
    })
  },
}
