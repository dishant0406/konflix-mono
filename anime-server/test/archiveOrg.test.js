import test from 'node:test'
import assert from 'node:assert/strict'

import {
  downloadUrl,
  identifiesEpisode,
  matchesTitle,
  pickFile,
  playableFiles,
} from '../graphql/stream/sources/archive-org.js'

// Captured from the live API. The item is a real archive.org episode of a regional
// series, and it is the only shape the source is ever handed.
const episodeItem = {
  title: 'INDIA S GOT LATENT S 2 EP 6 Ft. Rakhi Sawant Ashneer Grover Kushagra Srivastava( 720 P HD)',
  identifier: 'india-s-got-latent-s-2-ep-6-ft.-rakhi-sawant',
}

// A real metadata response, trimmed to the formats that matter. The original carries
// 60 entries: two video derivatives plus thumbnails and the item's own metadata files.
const episodeMetadata = {
  files: [
    { name: 'clip.ia.mp4', format: 'h.264 IA', size: '272061938' },
    { name: 'clip.mp4', format: 'MPEG4', size: '272061938' },
    { name: '__ia_thumb.jpg', format: 'Item Tile', size: '5497' },
    { name: 'clip.thumbs/a.jpg', format: 'Thumbnail', size: '2688' },
    { name: 'item_archive.torrent', format: 'Archive BitTorrent', size: '23576' },
    { name: 'item_files.xml', format: 'Metadata', size: '' },
  ],
}

test('playableFiles keeps the video derivatives and drops the item scaffolding', () => {
  const files = playableFiles(episodeMetadata)
  assert.deepEqual(
    files.map((file) => file.name),
    ['clip.ia.mp4', 'clip.mp4'],
    'thumbnails and metadata files must not be offered as video',
  )
})

test('playableFiles prefers the higher quality derivative of the same format', () => {
  const files = playableFiles({
    files: [
      { name: 'low.mp4', format: 'MPEG4', size: '1000000' },
      { name: 'high.mp4', format: 'MPEG4', size: '9000000' },
    ],
  })
  assert.equal(files[0].name, 'high.mp4')
})

test('playableFiles returns nothing for an item with no video', () => {
  // Several search hits are news articles rather than uploads with video.
  assert.deepEqual(playableFiles({ files: [{ name: 'a.jpg', format: 'JPEG', size: '1000' }] }), [])
  assert.deepEqual(playableFiles({}), [])
})

test('matchesTitle accepts every apostrophe spelling and rejects another show', () => {
  assert.equal(matchesTitle(episodeItem.title, ["India's Got Latent"]), true)
  assert.equal(matchesTitle('Indias Got Latent S02E01', ["India's Got Latent"]), true)
  assert.equal(
    matchesTitle("India's Got Latent Row: I&B Ministry Ticks Off OTT Players", ["India's Got Latent"]),
    true,
  )

  // A different show that shares every word but the first must not match.
  assert.equal(matchesTitle("Britain's Got Latent", ["India's Got Latent"]), false)
})

test('identifiesEpisode separates "this episode", "another episode" and "unknown"', () => {
  const title = 'INDIA S GOT LATENT S 2 EP 6 Ft. Rakhi Sawant'
  assert.equal(identifiesEpisode(title, 6, 2), true)
  // A different episode, and a different season of the same episode number.
  assert.equal(identifiesEpisode(title, 5, 2), false)
  assert.equal(identifiesEpisode('SHOW S 1 EP 6', 6, 2), false)
  // Names no episode at all, so it can neither confirm nor deny.
  assert.equal(identifiesEpisode('India s Got Latent Row', 6, 2), null)
})

// The regression this exists for: a search for the show returns news clips and reviews
// alongside the episodes, and a clip holds exactly one video file naming no episode.
// Treating "the item has one video" as proof of identity served a 650MB news package
// as S2E5 - the wrong content, silently.

test('an item naming no episode is refused rather than guessed at', () => {
  const clip = { title: "India's Got Latent Row: I&B Ministry Ticks Off OTT Players" }
  const files = [{ name: 'news_package.mp4' }]
  assert.equal(pickFile(clip, files, 5, 2), null)
  assert.equal(pickFile(clip, files, 6, 2), null)
})

test('an item that names the episode is served even when the file does not', () => {
  // archive.org frequently names the file after the item, and the item carries the
  // episode, so the item title is the identity that matters.
  const files = [{ name: 'video.ia.mp4' }]
  assert.equal(pickFile(episodeItem, files, 6, 2).name, 'video.ia.mp4')

  // Inside a correctly-named item, a file that names a *different* episode is a
  // mislabelled extra and must be skipped rather than served.
  const mixed = [{ name: 'S02E07 extra.mp4' }, { name: 'feature.mp4' }]
  assert.equal(pickFile(episodeItem, mixed, 6, 2).name, 'feature.mp4')

  // A file that does name the requested episode is the better answer than one that
  // does not, so it wins.
  const named = [{ name: 'S02E06 feature.mp4' }, { name: 'feature.mp4' }]
  assert.equal(pickFile(episodeItem, named, 6, 2).name, 'S02E06 feature.mp4')
})

test('a file name is the identity when the item title has none', () => {
  const item = { title: 'India s Got Latent' }
  const files = [{ name: 'INDIA S GOT LATENT S 2 EP 5 Ft. Orry.mp4' }]
  assert.equal(pickFile(item, files, 5, 2).name, 'INDIA S GOT LATENT S 2 EP 5 Ft. Orry.mp4')
  // The same file must not satisfy a different episode.
  assert.equal(pickFile(item, files, 6, 2), null)
})

test('downloadUrl encodes the identifier and the file name', () => {
  const url = downloadUrl('igl-s2-06', 'INDIAS GOT LATENT S2 EP6.mp4')
  assert.equal(url, 'https://archive.org/download/igl-s2-06/INDIAS%20GOT%20LATENT%20S2%20EP6.mp4')
})

// The gate must compare the two sides on equal footing. Normalisation is driven by the
// apostrophe, so it rewrites "India's" to "India s" but leaves "Indias" untouched; the
// two never converge. An item titled with one spelling was therefore rejected when the
// search had found it under another, silently discarding the results the multi-spelling
// search exists to find.

test('matchesTitle compares across apostrophe spellings in both directions', () => {
  const spacedItem = 'INDIA S GOT LATENT S 2 EP 6 Ft. Rakhi Sawant'
  const droppedItem = 'Indias Got Latent S02E01 1080p'
  const apostropheItem = "India's Got Latent S02E01 1080p"

  // Each query spelling must accept each item spelling.
  for (const query of ["India's Got Latent", 'Indias Got Latent', 'India s Got Latent']) {
    assert.equal(matchesTitle(spacedItem, [query]), true, `${query} vs spaced`)
    assert.equal(matchesTitle(droppedItem, [query]), true, `${query} vs dropped`)
    assert.equal(matchesTitle(apostropheItem, [query]), true, `${query} vs apostrophe`)
  }
})

test('matchesTitle tolerates a non-string title', () => {
  // archive.org's `title` field is multi-valued, so it can come back as an array.
  // Reading .toLowerCase off it would throw out of resolve(), which is documented
  // never to throw.
  assert.equal(matchesTitle(['a', 'b'], ['x']), false)
  assert.equal(matchesTitle(null, ['x']), false)
  assert.equal(matchesTitle(undefined, ['x']), false)
})

// Serving a known-wrong episode is worse than serving nothing, which is the invariant
// the whole file is built around.

test('an item whose every file names another episode is refused', () => {
  const item = { title: 'SHOW S 02 EP 06' }
  assert.equal(pickFile(item, [{ name: 'SHOW S02E07.mp4' }], 6, 2), null)
})

// resolve() itself was untested, which is the core of the reported fix: the search,
// the query construction and the deadline are all inside it.

test('resolve() serves the requested episode and refuses a different one', async (t) => {
  const { http } = await import('../lib/http.js')
  const archiveOrg = (await import('../graphql/stream/sources/archive-org.js')).default
  const { forget } = await import('../lib/cache.js')

  const originalGet = http.get
  t.after(() => {
    http.get = originalGet
    forget('archive-org:search:Indias Got Latent')
    forget('archive-org:item:ep-six')
  })

  http.get = async (url) => {
    if (url.includes('advancedsearch')) {
      return { data: { response: { docs: [{ identifier: 'ep-six', title: 'INDIA S GOT LATENT S 2 EP 6' }] } } }
    }
    return {
      data: {
        files: [{ name: 'ep6.ia.mp4', format: 'h.264 IA', size: '90000000' }],
      },
    }
  }

  const titles = ["India's Got Latent"]

  const six = await archiveOrg.resolve({
    episodeNumber: 6,
    season: 2,
    titles,
    terms: ['Indias Got Latent S02E06'],
    ranked: [],
    deadlineMs: 5000,
  })
  assert.equal(six.available, true)
  assert.equal(six.streamType, 'file')
  assert.match(six.url, /^https:\/\/archive\.org\/download\/ep-six\//)

  // The same item must not satisfy a different episode.
  const five = await archiveOrg.resolve({
    episodeNumber: 5,
    season: 2,
    titles,
    terms: ['Indias Got Latent S02E05'],
    ranked: [],
    deadlineMs: 5000,
  })
  assert.equal(five.available, false)
})

test('resolve() never throws, whatever the API returns', async (t) => {
  const { http } = await import('../lib/http.js')
  const archiveOrg = (await import('../graphql/stream/sources/archive-org.js')).default

  const originalGet = http.get
  t.after(() => {
    http.get = originalGet
  })

  for (const response of [
    { data: null },
    { data: {} },
    { data: { response: { docs: null } } },
    { data: { response: { docs: [{ title: 'no identifier' }] } } },
  ]) {
    http.get = async () => response
    const result = await archiveOrg.resolve({
      episodeNumber: 1,
      season: 1,
      titles: ['Whatever'],
      terms: [],
      ranked: [],
      deadlineMs: 2000,
    })
    assert.equal(result.available, false, JSON.stringify(response))
    assert.ok(result.reason, 'a reason must always be returned')
  }

  // A network failure must be reported, not thrown.
  http.get = async () => {
    throw new Error('ETIMEDOUT')
  }
  const failed = await archiveOrg.resolve({
    episodeNumber: 1,
    season: 1,
    titles: ['Whatever'],
    terms: [],
    ranked: [],
    deadlineMs: 2000,
  })
  assert.equal(failed.available, false)
})

test('resolve() returns without searching when it has no title', async () => {
  const archiveOrg = (await import('../graphql/stream/sources/archive-org.js')).default
  const result = await archiveOrg.resolve({
    episodeNumber: 1,
    season: 1,
    titles: [],
    terms: [],
    ranked: [],
    deadlineMs: 1000,
  })
  assert.equal(result.available, false)
  assert.match(result.reason, /no title/i)
})
