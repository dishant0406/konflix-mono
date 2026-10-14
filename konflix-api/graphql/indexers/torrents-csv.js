// torrents-csv indexer.
//
// A plain JSON API over a scrape of the whole DHT, and the only source here that holds
// regional and Indian series. Measured from the production server it answers in ~0.3s
// and returned 4 rows (best 71 seeders) for "Gyaarah Gyaarah", which knaben, eztv,
// nyaa and subsplease all answer with nothing.
//
// Two things shape this file:
//
//   1. It is searched with the title alone. The endpoint matches a query as a whole
//      phrase and there is no episode syntax: "Gyaarah Gyaarah S01E01" returns 0 rows
//      where "Gyaarah Gyaarah" returns 4. That is why the term is stripped to its
//      title before it is sent.
//   2. It has no category field and no safe-search - a query for "porn" returns adult
//      rows - so nothing upstream can be trusted to filter. The gate below is what
//      stands in for a category, and it is sufficient here for a reason worth stating:
//      the term is never user input. It is always a title from a metadata provider
//      (Cinemeta, TVMaze, YTS, AniList), reached through a catalogue lookup, so an
//      adult row would have to name a catalogue title to survive the gate.
//
// Every row reports a real seeder count and byte size, which is more than several of
// the other indexers manage.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, toHexInfoHash } from '../../lib/magnet.js'
import { normalizeTitle } from '../stream/releases.js'

const TTL_MS = 10 * 60 * 1000

// How many rows to ask for. The API caps this itself; asking for more than it will
// serve is harmless, and a title like "Breaking Bad" has 40+ season packs alone.
const SIZE = 100

/**
 * Whether a row is plausibly a result for the term that was searched.
 *
 * This exists because the API has no category field and no safe-search, so an
 * unrelated or adult row arrives alongside real results - a query for "porn" returns
 * adult rows. The ranking gate downstream (matchesAnyTitle) covers the ordinary case,
 * but it deliberately passes everything when a title has no comparable characters (a
 * CJK-only name), and this source has no category to fall back on the way knaben does.
 *
 * The rule is whole-term containment: the release name must contain the searched term.
 * It is deliberately weaker than the ranking gate downstream (`isRelevant`, which
 * requires the title at the *start* of the release name), so it can only ever remove a
 * row that the ranking would have rejected anyway - it cannot hide a real result.
 *
 * Exported so it can be tested against a captured response.
 *
 * @param {string} name the release name.
 * @param {string} term the term that was searched.
 */
export function matchesTerm(name, term) {
  const wanted = normalizeTitle(term)
  if (!wanted) return false
  return normalizeTitle(name).includes(wanted)
}

/**
 * Turns a torrents-csv response into indexer rows.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 * @param {string} term the term that was searched, used by the relevance gate.
 */
export function parseSearchResponse(body, term) {
  const rows = []
  for (const torrent of body?.torrents || []) {
    const infoHash = toHexInfoHash(torrent?.infohash)
    const title = torrent?.name
    if (!infoHash || !title) continue
    if (!matchesTerm(title, term)) continue

    rows.push({
      title,
      infoHash,
      // The API returns no magnet, so it is built from the hash with the shared
      // tracker list. DHT-only torrents often announce nowhere, and without trackers
      // we would never find peers.
      magnet: buildMagnet(infoHash, title, config.trackers),
      sizeBytes: Number.isFinite(torrent?.size_bytes) ? torrent.size_bytes : null,
      seeders: Number.isFinite(torrent?.seeders) ? torrent.seeders : 0,
      source: 'torrents-csv',
    })
  }
  return rows
}

export default {
  name: 'torrents-csv',
  types: ['anime', 'series', 'movie'],

  // Searched with the title alone: the endpoint matches a query as a whole phrase and
  // has no episode syntax, so "Gyaarah Gyaarah S01E01" returns 0 rows where
  // "Gyaarah Gyaarah" returns 4. Asking it once per distinct title rather than once
  // per episode-number form is both more accurate and fewer requests.
  titleOnly: true,

  /**
   * @param {string} term the search term, already reduced to a title.
   */
  async search(term) {
    if (!term) return []

    return cached(`torrents-csv:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(`${config.torrentsCsvBaseUrl}/service/search`, {
        params: { q: term, size: SIZE },
        responseType: 'json',
      })

      // A blocked or replaced host answers 200 with an HTML page, and axios leaves a
      // body it cannot parse as a string rather than throwing. Throwing here means the
      // failure is reported instead of being cached as "no results" for the whole TTL.
      if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')

      return parseSearchResponse(data, term)
    })
  },
}
