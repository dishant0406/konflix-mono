// Tests for the SubsPlease indexer.
//
// It is the only indexer that answers with every episode of a show from a single
// request, which is what makes it fast, and the only one whose size and release name
// live inside the magnet rather than in their own fields. Both are easy to get wrong
// in a way that looks like "no results".

import test from 'node:test'
import assert from 'node:assert/strict'

import { parseSearchResponse } from '../graphql/indexers/subsplease.js'
import { toHexInfoHash } from '../lib/magnet.js'

// Copied from a live response (trackers trimmed), including the batch entry the feed
// returns first. The magnets are real, so the base32 info hashes in them are too.
const RESPONSE = {
  'Bocchi the Rock! - 01-12': {
    episode: '01-12',
    downloads: [
      {
        res: '480',
        magnet:
          'magnet:?xt=urn:btih:H4VYWCNWYT3JQZWIAZOK55AZHKNOWYVB&dn=%5BSubsPlease%5D%20Bocchi%20the%20Rock%21%20%2801-12%29%20%28480p%29%20%5BBatch%5D&xl=4531637597',
      },
    ],
  },
  'Bocchi the Rock! - 12': {
    episode: '12',
    downloads: [
      {
        res: '480',
        magnet:
          'magnet:?xt=urn:btih:MEFC72FJALE4NCYMM22QUC46C5WJL3FE&dn=%5BSubsPlease%5D%20Bocchi%20the%20Rock%21%20-%2012%20%28480p%29%20%5B45C286B3%5D.mkv&xl=378370977',
      },
      {
        res: '1080',
        magnet:
          'magnet:?xt=urn:btih:34QO22RGZNDOSHT4KDA26MYKIQW4O4ZM&dn=%5BSubsPlease%5D%20Bocchi%20the%20Rock%21%20-%2012%20%281080p%29%20%5BCA5333CB%5D.mkv&xl=1448606898',
      },
    ],
  },
}

test('parseSearchResponse turns every resolution into a row', () => {
  const rows = parseSearchResponse(RESPONSE)
  assert.equal(rows.length, 3)

  const best = rows.find((row) => row.title.includes('1080p'))
  // The release name comes from the magnet's display name, which is the form the
  // release parser and ranker expect. Without it the row would be unusable.
  assert.equal(best.title, '[SubsPlease] Bocchi the Rock! - 12 (1080p) [CA5333CB].mkv')
  // The size only exists inside the magnet, as `xl`.
  assert.equal(best.sizeBytes, 1448606898)
  assert.equal(best.source, 'subsplease')
  // The feed reports no seeder counts, and claiming otherwise would let these
  // releases outrank ones with a real count.
  assert.equal(best.seeders, 0)
})

test('parseSearchResponse normalises the info hash out of the magnet', () => {
  // The feed publishes base32 and webtorrent reports hex, so this is the conversion
  // that lets a release found here match a torrent that is later opened.
  const row = parseSearchResponse(RESPONSE).find((r) => r.title.includes('1080p'))
  assert.equal(row.infoHash, toHexInfoHash('34QO22RGZNDOSHT4KDA26MYKIQW4O4ZM'))
  assert.match(row.infoHash, /^[0-9a-f]{40}$/)
})

test('parseSearchResponse keeps the batch entry distinct from a single episode', () => {
  // The batch names a range ("01-12"). It must survive parsing as its own row so the
  // release parser can classify it as a batch and keep it out of episode ranking.
  const batch = parseSearchResponse(RESPONSE).find((row) => row.title.includes('Batch'))
  assert.ok(batch)
  assert.equal(batch.sizeBytes, 4531637597)
})

test('parseSearchResponse ignores anything that is not a download list', () => {
  assert.deepEqual(parseSearchResponse(null), [])
  assert.deepEqual(parseSearchResponse('a string'), [])
  assert.deepEqual(parseSearchResponse({}), [])
  // A download with no magnet cannot be turned into a torrent, so it is skipped
  // rather than emitting a row with no hash.
  assert.deepEqual(parseSearchResponse({ X: { downloads: [{ res: '1080' }] } }), [])
  assert.deepEqual(parseSearchResponse({ X: { downloads: 'nope' } }), [])
})
