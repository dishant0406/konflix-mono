// The Pirate Bay (apibay) indexer.
//
// A plain JSON API, which is the easiest source to consume. It is not anime
// specific, so it returns more noise than the other two; the relevance and episode
// filters in stream/releases.js are what keep that noise out of the results, and
// they were verified against a real adult-content result set before this was added.
//
// It is worth having because it reports live seeder counts and covers recent
// releases, which the other indexers do not: AnimeTosho stopped ingesting new
// torrents in May 2026, so on its own it has nothing for a currently airing show.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { toHexInfoHash, buildMagnet } from '../../lib/magnet.js'

const ENDPOINT = 'https://apibay.org/q.php'
const TTL_MS = 10 * 60 * 1000

// Returned instead of an empty list when nothing matches.
const NO_RESULTS_HASH = '0000000000000000000000000000000000000000'

export function parseSearchResults(rows) {
  const parsed = []
  for (const row of Array.isArray(rows) ? rows : []) {
    if (!row || row.name === 'No results returned') continue

    const infoHash = toHexInfoHash(row.info_hash)
    if (!infoHash || infoHash === NO_RESULTS_HASH) continue

    parsed.push({
      title: row.name,
      infoHash,
      sizeBytes: Number.parseInt(row.size, 10) || null,
      seeders: Number.parseInt(row.seeders, 10) || 0,
      source: 'apibay',
    })
  }
  return parsed
}

export default {
  name: 'apibay',

  async search(term) {
    if (!term) return []
    return cached(`apibay:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(ENDPOINT, {
        // category 0 is "all"; filtering server-side loses the anime rows, which
        // TPB files under a general video category.
        params: { q: term, cat: '0' },
        responseType: 'json',
      })
      return parseSearchResults(data).map((row) => ({
        ...row,
        magnet: buildMagnet(row.infoHash, row.title, config.trackers),
      }))
    })
  },
}
