// AnimeTosho indexer.
//
// AnimeTosho has no JSON API. Search results are an HTML page of rows that look
// like this (verified against a live response):
//
//   <div class="home_list_entry ...">
//     <div class="size" title="Total file size: 1,405,723,690 bytes">1.309 GB</div>
//     <div class="link"><a href="https://animetosho.org/view/slug.n2107283">Title</a></div>
//     <div class="links">
//       <a href="magnet:?xt=urn:btih:...">Magnet</a>
//       <span title="Seeders: 105 / Leechers: 145">[105↑/145↓]</span>
//     </div>
//   </div>
//
// Rows are parsed by splitting on the entry marker rather than with a DOM parser,
// which keeps this dependency-free.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { infoHashFromMagnet, buildMagnet } from '../../lib/magnet.js'

const BASE_URL = 'https://animetosho.org'
const TTL_MS = 10 * 60 * 1000

function decodeEntities(text) {
  return text
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&nbsp;/g, ' ')
    .trim()
}

export function parseSearchPage(html) {
  const rows = []
  for (const chunk of html.split('class="home_list_entry').slice(1)) {
    const titleMatch = /<div class="link"><a href="[^"]*">([^<]*)<\/a>/.exec(chunk)
    const magnetMatch = /href="(magnet:\?[^"]*)"/.exec(chunk)
    if (!titleMatch || !magnetMatch) continue

    const magnet = decodeEntities(magnetMatch[1])
    const infoHash = infoHashFromMagnet(magnet)
    if (!infoHash) continue

    const sizeMatch = /Total file size: ([\d,]+) bytes/.exec(chunk)
    const seederMatch = /Seeders: (\d+)/.exec(chunk)

    rows.push({
      title: decodeEntities(titleMatch[1]),
      infoHash,
      magnet,
      sizeBytes: sizeMatch ? Number.parseInt(sizeMatch[1].replace(/,/g, ''), 10) : null,
      seeders: seederMatch ? Number.parseInt(seederMatch[1], 10) : 0,
      source: 'animetosho',
    })
  }
  return rows
}

export default {
  name: 'animetosho',

  async search(term) {
    if (!term) return []
    return cached(`animetosho:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      // AnimeTosho ignores an `order` parameter (verified: passing order=seeders
      // returns the same order as passing nothing), so results are ranked
      // client-side instead. See scoreRelease.
      const { data } = await http.get(`${BASE_URL}/search`, {
        params: { q: term },
        responseType: 'text',
      })
      return parseSearchPage(data).map((row) => ({
        ...row,
        magnet: buildMagnet(row.infoHash, row.title, config.trackers),
      }))
    })
  },
}
