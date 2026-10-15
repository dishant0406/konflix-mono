// Tests for the torrents-csv indexer.
//
// It is the only indexer here that holds regional and Indian series, and it is the
// only one with no category field and no safe-search: a query for "porn" returns adult
// rows. That makes the relevance gate the only thing standing between this source and
// adult content on a films and series site, so it is pinned here rather than assumed.

import test from 'node:test'
import assert from 'node:assert/strict'

import { parseSearchResponse, matchesTerm } from '../graphql/indexers/torrents-csv.js'

// Copied from a live response for "Gyaarah Gyaarah" (trackers trimmed in the magnet).
// This is the title that started all of this: knaben, eztv, nyaa and subsplease all
// answer it with nothing, and this source holds it with 71 seeders.
const RESPONSE = {
  torrents: [
    {
      infohash: '419848235f919b6350e764697555cdc782d85bdd',
      name: 'Gyaarah Gyaarah S01 [1080p WEB-DL x264 [AAC Hindi] xDark [MoviesUp]',
      size_bytes: 4705096321,
      created_unix: 1725521388,
      seeders: 71,
      leechers: 14,
      completed: 2787,
      id: 23969,
    },
    {
      infohash: '6eead2db91ebb5255301930c92c71f3e081cf02a',
      name: 'www.5MovieRulz.dad - Gyaarah Gyaarah (2024) 720p S01 EP(01-08) TRUE WEB-DL - AVC',
      size_bytes: 3113713341,
      seeders: 11,
      id: 235363,
    },
  ],
  next: 1094153,
}

test('parseSearchResponse turns every torrent into a row', () => {
  const rows = parseSearchResponse(RESPONSE, 'Gyaarah Gyaarah')
  assert.equal(rows.length, 2)

  const best = rows[0]
  assert.equal(best.title, 'Gyaarah Gyaarah S01 [1080p WEB-DL x264 [AAC Hindi] xDark [MoviesUp]')
  assert.equal(best.infoHash, '419848235f919b6350e764697555cdc782d85bdd')
  assert.equal(best.sizeBytes, 4705096321)
  assert.equal(best.seeders, 71)
  assert.equal(best.source, 'torrents-csv')
})

test('the magnet is rebuilt from the hash, because the API returns none', () => {
  // The API returns an info hash and no magnet, and a DHT-only torrent often announces
  // nowhere, so the shared tracker list is what makes it reachable at all.
  const row = parseSearchResponse(RESPONSE, 'Gyaarah Gyaarah')[0]
  assert.match(row.magnet, /^magnet:\?xt=urn:btih:419848235f919b6350e764697555cdc782d85bdd/)
  assert.match(row.magnet, /tr=udp%3A%2F%2F/)
})

test('a row that does not name the searched term is dropped', () => {
  // This source has no categories and no safe-search, so an unrelated or adult row
  // arrives alongside real results. The gate is the only filter there is.
  assert.equal(matchesTerm('Gyaarah Gyaarah S01 [1080p]', 'Gyaarah Gyaarah'), true)
  assert.equal(matchesTerm('Gyaarah Gyaarah S01 [1080p]', 'Gyaarah'), true)

  assert.equal(matchesTerm('Some Other Show S01 1080p', 'Gyaarah Gyaarah'), false)
  assert.equal(matchesTerm('Gyaarah', 'Gyaarah Gyaarah'), false, 'half a duplicated title is not the title')
  // An empty or whitespace term must not match everything.
  assert.equal(matchesTerm('anything', ''), false)
  assert.equal(matchesTerm('anything', '   '), false)
})

test('adult rows do not survive the gate on an ordinary title query', () => {
  // Verified against the live API: a query for an unrelated word returns adult rows,
  // because the endpoint has no safe-search. They must not pass as results for a real
  // title, which is the whole reason this gate exists.
  const body = {
    torrents: [
      { infohash: 'a'.repeat(40), name: 'Gyaarah Gyaarah S01 1080p', size_bytes: 1, seeders: 5 },
      { infohash: 'b'.repeat(40), name: 'Hot Amateur Video 1080p', size_bytes: 1, seeders: 500 },
    ],
  }

  const rows = parseSearchResponse(body, 'Gyaarah Gyaarah')
  assert.equal(rows.length, 1)
  assert.equal(rows[0].infoHash, 'a'.repeat(40))
})

test('parseSearchResponse tolerates rows it cannot use', () => {
  assert.deepEqual(parseSearchResponse(null, 'x'), [])
  assert.deepEqual(parseSearchResponse({}, 'x'), [])
  // No hash, no name, or a hash that is not a real info hash.
  assert.deepEqual(parseSearchResponse({ torrents: [{ name: 'Show 1080p' }] }, 'Show'), [])
  assert.deepEqual(parseSearchResponse({ torrents: [{ infohash: 'nope', name: 'Show' }] }, 'Show'), [])
  // A missing size or seeder count must not become NaN in the ranking.
  const rows = parseSearchResponse({ torrents: [{ infohash: 'a'.repeat(40), name: 'Show 1080p' }] }, 'Show')
  assert.equal(rows.length, 1)
  assert.equal(rows[0].sizeBytes, null)
  assert.equal(rows[0].seeders, 0)
})
