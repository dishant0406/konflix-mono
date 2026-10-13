// Knaben indexer.
//
// A Prowlarr-maintained meta-search across many trackers, and the only source here
// that covers anime, series and films from one endpoint. Measured from the
// production server it answers in ~0.2s and returns, per row, the info hash, a ready
// magnet, a live seeder count and the byte size.
//
// It is asked for one content type at a time through `categories`, which is the
// API's own taxonomy. The ids are not documented anywhere, so they were mapped by
// probing:
//
//   2000000  series        3000000  films        6000000  anime
//   5000000  adult         <- must never be requested or returned
//
// Two behaviours were verified against the live API and shape this file:
//
//   1. `hide_unsafe` is accepted and IGNORED. With it set to true, a query for
//      "porn" still returned 26 of 30 rows in the 5xxxxxx category. The filter
//      below is therefore mandatory, not belt-and-braces.
//   2. `categoryId` is an ARRAY, e.g. [6001000, 6000000, 3008000, 3000000]. Anime
//      releases are tagged with the film and series categories as well, so a
//      single-value comparison matches nothing and the unsafe check has to scan
//      every entry.
//
// Title matching is also unreliable: "Breaking Bad S01E01" returned
// "The Bad Guys Breaking In S01E01". Relevance filtering downstream is what rejects
// that, so this file deliberately does not try to rank.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, toHexInfoHash } from '../../lib/magnet.js'

const TTL_MS = 10 * 60 * 1000

// The API's category ids, by the content type this application knows about.
const CATEGORIES = {
  anime: 6000000,
  series: 2000000,
  movie: 3000000,
}

// The adult tree. Any row carrying one of these is dropped regardless of what was
// asked for, because the API's own filter does not work.
const UNSAFE_PREFIX = '5'

export function isUnsafe(categoryId) {
  const ids = Array.isArray(categoryId) ? categoryId : [categoryId]
  return ids.some((id) => String(id ?? '').startsWith(UNSAFE_PREFIX))
}

/**
 * Turns a knaben response into indexer rows.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 */
export function parseSearchResponse(body) {
  const rows = []
  for (const hit of body?.hits || []) {
    if (isUnsafe(hit?.categoryId)) continue

    const infoHash = toHexInfoHash(hit?.hash)
    const title = hit?.title
    if (!infoHash || !title) continue

    rows.push({
      title,
      infoHash,
      // The API sends a magnet, but it carries no trackers of its own and many of
      // its torrents announce nowhere, so the magnet is rebuilt from the hash with
      // the shared tracker list. That also guarantees the hash in the magnet is the
      // normalised one this application uses as a key everywhere else.
      magnet: buildMagnet(infoHash, title, config.trackers),
      sizeBytes: Number.isFinite(hit.bytes) ? hit.bytes : null,
      // Reported as null for some rows, which must not become NaN in the ranking.
      seeders: Number.isFinite(hit.liveSeeders) ? hit.liveSeeders : 0,
      source: 'knaben',
    })
  }
  return rows
}

export default {
  name: 'knaben',
  types: ['anime', 'series', 'movie'],

  /**
   * @param {string} term the search term.
   * @param {{ contentType?: string }} [options]
   */
  async search(term, options = {}) {
    if (!term) return []

    const category = CATEGORIES[options.contentType]
    // An unset category means "everything", which would return adult rows for an
    // ordinary query. Searching nothing is the honest alternative.
    if (!category) return []

    return cached(`knaben:${category}:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.post(
        config.knabenBaseUrl,
        {
          search_type: '100%',
          search_field: 'title',
          query: term,
          size: 50,
          categories: [category],
          order_by: 'seeders',
          order_direction: 'desc',
        },
        { headers: { 'Content-Type': 'application/json' } },
      )
      return parseSearchResponse(data)
    })
  },
}
