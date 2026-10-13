// YTS indexer.
//
// Films only. YTS is the one source here with a real Indian catalogue, which is what
// makes a Bollywood search work at all, and it is the only indexer that publishes a
// seeder count that is actually meaningful (knaben does too; EZTV reports 0 for
// everything).
//
// The host is mid-migration. The old yts.mx no longer resolves; the live base is
// movies-api.accel.li, which YTS's own API response names as the successor. Because
// it has moved more than once it is a config value, not a constant.
//
// YTS returns no magnet, only a `hash`, so the magnet is built from it.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, toHexInfoHash } from '../../lib/magnet.js'

const TTL_MS = 10 * 60 * 1000

// How many rows to ask for. The API's own maximum is 50.
const LIMIT = 50

/**
 * Turns a YTS response into indexer rows, one per torrent within each film.
 *
 * A film carries several torrents (720p, 1080p, 2160p) and each is a separate
 * choice for the viewer, so they are separate rows. The film's title and year are
 * prefixed onto the torrent's own quality label, because the ranking and the
 * relevance gate work on the release name and YTS does not provide one.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 */
export function parseSearchResponse(body) {
  const rows = []
  for (const movie of body?.data?.movies || []) {
    const title = movie?.title
    if (!title) continue

    for (const torrent of movie?.torrents || []) {
      const infoHash = toHexInfoHash(torrent?.hash)
      if (!infoHash) continue

      // Shaped like a scene release name so the existing parser reads the
      // resolution and the year out of it: "Dangal (2016) 1080p BluRay x264".
      const name = [title, `(${movie.year})`, torrent.quality, torrent.type].filter(Boolean).join(' ')

      rows.push({
        title: name,
        infoHash,
        magnet: buildMagnet(infoHash, name, config.trackers),
        // YTS sends a human-readable size ("2.6 GB"); `size_bytes` is not always
        // present, so the ranking falls back to the title when it is missing.
        sizeBytes: Number.isFinite(torrent?.size_bytes) ? torrent.size_bytes : null,
        seeders: Number.isFinite(torrent?.seeds) ? torrent.seeds : 0,
        source: 'yts',
      })
    }
  }
  return rows
}

export default {
  name: 'yts',
  types: ['movie'],

  async search(term) {
    if (!term) return []

    return cached(`yts:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(`${config.ytsBaseUrl}/list_movies.json`, {
        params: { query_term: term, limit: LIMIT },
        responseType: 'json',
      })

      // A blocked or replaced host answers 200 with an HTML block page, and axios
      // leaves a body it cannot parse as a string rather than throwing. Throwing
      // means the failure is reported instead of being cached as "no results".
      if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')

      return parseSearchResponse(data)
    })
  },
}
