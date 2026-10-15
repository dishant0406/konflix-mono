// SubsPlease indexer.
//
// A JSON API, and the only source here that is both anime-specific and fast. One
// request returns *every* episode of a show, each with a magnet per resolution:
//
//   {
//     "Bocchi the Rock! - 12": {
//       "episode": "12",
//       "downloads": [{ "res": "1080", "magnet": "magnet:?xt=urn:btih:...&xl=1448606898&dn=..." }]
//     }
//   }
//
// That is why it is worth having: the other indexers are searched once per episode
// number form, and this one answers all of them at once, so it is queried with the
// show title only (`titleOnly`) instead of once per search term.
//
// It is one release group's feed rather than a general index: it covers currently
// airing SubsPlease releases and nothing older, so it is an addition to the other
// indexers and never a replacement.
//
// Two quirks, both verified against a live response:
//   - `res` is a string ("1080"), not a number.
//   - The magnet carries `xl`, the exact size in bytes, which the indexer contract
//     wants as `sizeBytes`. That is also the only place the size appears.

import { http } from '../../lib/http.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { infoHashFromMagnet } from '../../lib/magnet.js'

const ENDPOINT = 'https://subsplease.org/api/'
const TTL_MS = 10 * 60 * 1000

// The release name is the magnet's display name, which is exactly the form the
// release parser and ranker expect ("[SubsPlease] Show - 12 (1080p) [ABC].mkv").
function displayName(magnet) {
  const match = /[?&]dn=([^&]*)/.exec(magnet || '')
  if (!match) return null
  try {
    return decodeURIComponent(match[1])
  } catch {
    return null
  }
}

function sizeFromMagnet(magnet) {
  const match = /[?&]xl=(\d+)/.exec(magnet || '')
  return match ? Number.parseInt(match[1], 10) : null
}

/**
 * Turns a SubsPlease response into indexer rows.
 *
 * Exported so it can be tested against a captured response without the network.
 *
 * @param {object} body the parsed JSON response.
 * @returns {Array<{title: string, infoHash: string, magnet: string, sizeBytes: number|null,
 *                  seeders: number, source: string}>}
 */
export function parseSearchResponse(body) {
  const rows = []
  if (!body || typeof body !== 'object') return rows

  for (const entry of Object.values(body)) {
    if (!entry || !Array.isArray(entry.downloads)) continue

    for (const download of entry.downloads) {
      const magnet = download?.magnet
      if (!magnet) continue

      const infoHash = infoHashFromMagnet(magnet)
      const title = displayName(magnet)
      if (!infoHash || !title) continue

      rows.push({
        title,
        infoHash,
        magnet,
        sizeBytes: sizeFromMagnet(magnet),
        // The feed reports no seeder counts. Reporting 0 is honest and keeps these
        // releases in the lowest health tier, so they cannot outrank a release with a
        // real seeder count for the same episode.
        seeders: 0,
        source: 'subsplease',
      })
    }
  }
  return rows
}

export default {
name: 'subsplease',
  types: ['anime'],

  // Search this one with the show title alone: it returns every episode, so querying
  // it once per episode-number form would repeat identical work 4 times over.
  titleOnly: true,

  async search(term) {
    if (!term) return []
    return cached(`subsplease:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      const { data } = await http.get(ENDPOINT, {
        params: { f: 'search', s: term, tz: 'UTC' },
        responseType: 'json',
      })

      // A blocked or replaced host answers 200 with an HTML block page, and axios
      // leaves a body it cannot parse as a string rather than throwing. Throwing here
      // means the failure is reported instead of being cached as "no results" for the
      // whole TTL.
      if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')

      return parseSearchResponse(data)
    })
  },
}
