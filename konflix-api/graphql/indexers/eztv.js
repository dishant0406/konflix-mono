// EZTV indexer.
//
// Series only, and the only source here that can be searched by IMDb id. That
// matters: a title search across indexers leaks badly (knaben returned "The Bad Guys
// Breaking In S01E01" for a Breaking Bad query), whereas an IMDb id identifies the
// show exactly. TVMaze supplies the id, so the chain is
// series -> TVMaze.externals.imdb -> EZTV.
//
// Two verified traps, both of which silently return the wrong thing:
//
//   1. The `search` parameter is IGNORED. `?search=breaking bad` returned
//      torrents_count 1091704 - the entire index - rather than an error or an empty
//      result. Only `imdb_id` filters, so a search without one returns nothing.
//   2. The id must be the BARE form. `?imdb_id=tt0903747` also returned the whole
//      index; `?imdb_id=0903747` returned the real 77 episodes.
//
// Every row reports `seeds: 0`, including rows that demonstrably have peers, and the
// index contains misspellings ("Breaking.Brad.S02E02"). Both are why this source is
// kept as a last resort rather than a primary one: its rows land in the lowest
// health tier and can only be chosen when nothing seeded exists.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, toHexInfoHash } from '../../lib/magnet.js'

const TTL_MS = 10 * 60 * 1000

/**
 * The bare numeric IMDb id, or null.
 *
 * TVMaze reports it as "tt0903747"; EZTV wants "0903747". Anything that is not
 * digits after the prefix is rejected, because sending a malformed id is what
 * returns the whole index.
 */
export function toEztvId(imdbId) {
  const digits = String(imdbId ?? '').replace(/^tt/i, '')
  return /^\d+$/.test(digits) ? digits : null
}

/**
 * Turns an EZTV response into indexer rows.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 */
export function parseSearchResponse(body) {
  const rows = []
  for (const torrent of body?.torrents || []) {
    const infoHash = toHexInfoHash(torrent?.hash)
    const title = torrent?.filename
    if (!infoHash || !title) continue

    rows.push({
      title,
      infoHash,
      // Rebuilt rather than reused: EZTV's magnet carries its own announce list and
      // the hash inside it has to be the normalised one this application keys on.
      magnet: buildMagnet(infoHash, title, config.trackers),
      sizeBytes: Number.isFinite(torrent?.size_bytes) ? torrent.size_bytes : null,
      seeders: Number.isFinite(torrent?.seeds) ? torrent.seeds : 0,
      source: 'eztv',
    })
  }
  return rows
}

export default {
  name: 'eztv',
  types: ['series'],

  // Like SubsPlease, this answers with every episode of a show at once, so it is
  // asked once per title rather than once per episode-number form.
  titleOnly: true,

  /**
   * @param {string} term ignored; EZTV cannot search by title. It is accepted so the
   *   signature matches the contract.
   * @param {{ imdbId?: string }} [options]
   */
  async search(term, options = {}) {
    const imdbId = toEztvId(options.imdbId)
    if (!imdbId) return []

    return cached(`eztv:${imdbId}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(`${config.eztvBaseUrl}/get-torrents`, {
        params: { imdb_id: imdbId, limit: 100 },
        responseType: 'json',
      })

      if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')
      return parseSearchResponse(data)
    })
  },
}
