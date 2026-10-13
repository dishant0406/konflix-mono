// Torrent indexer registry.
//
// To add an indexer: create a file next to this one that default-exports an object
// with `name`, `types` and `async search(term)`, then add it to the map below.
// Enable it by listing its key in TORRENT_INDEXERS.
//
//   types                       content types this indexer can serve
//   search(term) -> [{ title, infoHash, magnet, sizeBytes, seeders, source }]
//
// search() must never throw. Indexers go down and get blocked; a failing indexer
// should reduce the number of results, not fail the request.
//
// `types` is what keeps a movie search from querying an anime feed and vice versa.
// A search only asks the indexers that declare the type being searched.

import { config } from '../../lib/config.js'
import animetosho from './animetosho.js'
import apibay from './apibay.js'
import eztv from './eztv.js'
import knaben from './knaben.js'
import nyaa from './nyaa.js'
import subsplease from './subsplease.js'
import torrentsCsv from './torrents-csv.js'
import yts from './yts.js'

const indexers = { animetosho, apibay, eztv, knaben, nyaa, subsplease, 'torrents-csv': torrentsCsv, yts }

// The indexers enabled for a content type, in configured order.
//
// An indexer with no `types` is treated as covering everything, so an indexer added
// before this field existed keeps working.
export function enabledIndexers(contentType) {
  return config.indexers
    .map((name) => indexers[name])
    .filter((indexer) => indexer && (!contentType || !indexer.types || indexer.types.includes(contentType)))
}

export function indexerNames() {
  return Object.keys(indexers)
}

// Which types each indexer serves, for /health.
export function indexerTypes() {
  return Object.fromEntries(
    Object.entries(indexers).map(([name, indexer]) => [name, indexer.types || []]),
  )
}

// A search fans out over every (indexer, term) pair, which is 24 requests for one
// episode. /graphql is unauthenticated, so an unbounded fan-out would let a handful
// of viewers open hundreds of sockets at once and get the server's IP rate limited by
// the indexers.
//
// The cap is deliberately set to the full fan-out: measured, a low cap costs real
// time because a 45ms apibay request ends up queued behind a 1.5s nyaa one, and the
// per-indexer timeout below is what actually bounds the tail. The cap stays as a
// guard against a configuration with many more indexers or terms than this one.
const MAX_CONCURRENT_REQUESTS = 24

// The show title without the episode or season token: "Show 001" -> "Show",
// "Show S01" -> "Show". Used for indexers that answer with every episode at once.
//
// Only a token that actually looks like an episode number or a season is stripped.
// Removing the last word unconditionally ate a real word from any term that had no
// episode number on the end, so "Attack on Titan" was searched as "Attack on".
export function titleOf(term) {
  return (term || '').replace(/\s+(?:S\d{1,2}E\d{1,4}|S\d{1,2}|\d{1,4})$/, '').trim() || term
}

// A single slow indexer must not hold the whole search open. Every normal request
// here is well under a second; the timeout only bounds the tail, so a hung mirror
// costs its own slot rather than the response.
function withTimeout(promise, timeoutMs) {
  let timer
  const expiry = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${timeoutMs}ms`)), timeoutMs)
  })
  return Promise.race([promise, expiry]).finally(() => clearTimeout(timer))
}

async function inBatches(items, size, run) {
  const results = []
  for (let index = 0; index < items.length; index += size) {
    results.push(...(await Promise.all(items.slice(index, index + size).map(run))))
  }
  return results
}

export async function searchAll(terms, options = {}) {
  // One term or many; a fan-out over several search forms is the common case.
  const queries = (Array.isArray(terms) ? terms : [terms]).filter(Boolean)
  if (queries.length === 0) return []

  const active = enabledIndexers(options.contentType)

  // An indexer that returns every episode for a title (SubsPlease, EZTV) is asked
  // once per distinct title, not once per episode-number form: asking it per form
  // would fetch the same response several times over. Every title variant is still
  // searched, because a show is known by several names and releases use them
  // inconsistently - searching only the first would miss the romaji form of a show
  // whose releases are named in romaji.
  const pairs = active.flatMap((indexer) =>
    indexer.titleOnly
      ? [...new Set(queries.map(titleOf))].map((term) => ({ indexer, term }))
      : queries.map((term) => ({ indexer, term })),
  )

  const results = await inBatches(pairs, MAX_CONCURRENT_REQUESTS, async ({ indexer, term }) => {
    try {
      return await withTimeout(
        indexer.search(term, options),
        config.indexerTimeoutMs,
      )
    } catch (error) {
      console.warn(`[indexer:${indexer.name}] ${error.message}`)
      return []
    }
  })

  const seen = new Set()
  const merged = []
  for (const row of results.flat()) {
    if (seen.has(row.infoHash)) continue
    seen.add(row.infoHash)
    merged.push(row)
  }
  return merged
}

// Per-indexer reachability, used by /health so failures are visible before users
// report them.
export async function indexerStatus(term = 'one piece', options = {}) {
  const active = enabledIndexers(options.contentType)
  const status = {}
  await Promise.all(
    active.map(async (indexer) => {
      try {
        const rows = await indexer.search(term, options)
        status[indexer.name] = { ok: true, results: rows.length }
      } catch (error) {
        status[indexer.name] = { ok: false, error: error.message }
      }
    }),
  )
  return status
}
