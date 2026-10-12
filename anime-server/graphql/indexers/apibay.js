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

// The host sits behind a Cloudflare JS challenge, which a datacenter IP can never
// satisfy: verified from the production server, every request answers 403 with the
// "Just a moment..." interstitial, and a browser User-Agent does not change it. It
// works from a residential connection, so it stays enabled.
//
// Without a cooldown every search retries a request that is certain to fail, once per
// search term. A search fans out over several terms, so a failing indexer would be
// hit that many times on every episode lookup. The cooldown makes one failure suppress
// the rest.
const COOLDOWN_MS = 5 * 60 * 1000
let downUntil = 0

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

    // While the host is known to be refusing us, the empty result is returned without
    // being cached: caching it would blackhole this term for the full TTL even though
    // the block may lift in seconds.
    if (downUntil > Date.now()) return []

    try {
      return await cached(`apibay:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
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
    } catch (error) {
      // search() must never throw. The cooldown is set here rather than in the caller
      // so it applies to every caller, and the first failure is still reported once.
      downUntil = Date.now() + COOLDOWN_MS
      console.warn(`[indexer:apibay] ${error.message} (pausing for ${COOLDOWN_MS / 60000}m)`)
      return []
    }
  },
}
