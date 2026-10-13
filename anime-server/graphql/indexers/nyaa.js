// Nyaa indexer.
//
// Nyaa is a plain RSS feed, which is the cleanest possible source. The canonical
// host (nyaa.si) is blocked on many networks - it answers 503 through a filtering
// proxy - so several mirrors are tried in order and the first that returns results
// is used. See NYAA_BASE_URLS.
//
// A mirror returning an error page instead of a feed parses to zero rows, which is
// treated the same as a network failure so the next mirror is tried.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { buildMagnet, parseSize, toHexInfoHash } from '../../lib/magnet.js'

const TTL_MS = 10 * 60 * 1000

function tag(item, name) {
  const match = new RegExp(`<${name}[^>]*>([\\s\\S]*?)</${name}>`).exec(item)
  if (!match) return null
  return match[1]
    .replace(/<!\[CDATA\[|\]\]>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .trim()
}

export function parseRss(xml) {
  const rows = []
  for (const item of xml.split('<item>').slice(1)) {
    const title = tag(item, 'title')
    const infoHash = toHexInfoHash(tag(item, 'nyaa:infoHash'))
    if (!title || !infoHash) continue

    rows.push({
      title,
      // Normalised to hex so it matches what webtorrent reports.
      infoHash: toHexInfoHash(infoHash),
      sizeBytes: parseSize(tag(item, 'nyaa:size')),
      seeders: Number.parseInt(tag(item, 'nyaa:seeders') || '0', 10) || 0,
      source: 'nyaa',
    })
  }
  return rows
}

// A mirror that just failed is not retried for every search term: a search fans out
// over several terms, and a blocked mirror would otherwise be hit once per term.
// The cooldown is short because a proxy block can be transient.
const mirrorCooldownMs = 60 * 1000
const mirrorDownUntil = new Map()

function mirrorUsable(baseUrl) {
  return (mirrorDownUntil.get(baseUrl) || 0) <= Date.now()
}

function markMirrorDown(baseUrl) {
  mirrorDownUntil.set(baseUrl, Date.now() + mirrorCooldownMs)
}

// A filtering proxy can answer 200 with an HTML block page rather than an error,
// which would otherwise look like "no results" on every search. A real response is
// an RSS document, so that is what is checked.
function looksLikeRss(body) {
  return /<rss[\s>]/.test(body || '')
}

async function searchMirror(baseUrl, term) {
  const { data } = await http.get(`${baseUrl}/`, {
    // c=1_0 is all of "Anime", where c=1_2 is only its "English-translated"
    // subcategory. Measured against the same query: 23 items vs 18, and 37 vs 22. It is
    // a strict superset at identical latency, so the narrower category only loses rows.
    params: { page: 'rss', q: term, c: '1_0', f: '0' },
    responseType: 'text',
  })
  if (!looksLikeRss(data)) throw new Error('not an RSS feed')
  return parseRss(data)
}

export default {
name: 'nyaa',
  types: ['anime'],

  async search(term) {
    if (!term) return []

    // If every mirror is cooling down there is nothing to try, so the empty result
    // is returned without being cached. Caching it would blackhole this term for the
    // full TTL even though the mirrors may recover in seconds.
    if (!config.nyaaBaseUrls.some(mirrorUsable)) return []

    return cached(`nyaa:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      for (const baseUrl of config.nyaaBaseUrls) {
        if (!mirrorUsable(baseUrl)) continue

        let rows
        try {
          rows = await searchMirror(baseUrl, term)
        } catch (error) {
          // A blocked mirror (503 from a filtering proxy) fails on every term, so
          // it is taken out of rotation briefly instead of being retried each time.
          console.warn(`[indexer:nyaa] ${baseUrl}: ${error.message}`)
          markMirrorDown(baseUrl)
          continue
        }

        // A valid feed with no matching rows is a real empty result; it is returned
        // rather than treated as a broken mirror.
        return rows.map((row) => ({
          ...row,
          magnet: buildMagnet(row.infoHash, row.title, config.trackers),
        }))
      }
      return []
    })
  },
}
