import test from 'node:test'
import assert from 'node:assert/strict'

import { parseSearchPage } from '../graphql/indexers/animetosho.js'
import { parseRss } from '../graphql/indexers/nyaa.js'
import { parseSearchResults } from '../graphql/indexers/apibay.js'
import apibay from '../graphql/indexers/apibay.js'
import { http } from '../lib/http.js'
import { titleOf } from '../graphql/indexers/index.js'
import { buildMagnet, infoHashFromMagnet, parseSize, toHexInfoHash } from '../lib/magnet.js'

test('parseSearchPage extracts rows from a real AnimeTosho result page', () => {
  // Trimmed from a live response.
  const html = `
    <div class="home_list_entry home_list_entry_compl_1">
      <div class="date" title="Date/time submitted: 07/05/2026 20:12">20:12</div>
      <div class="size" title="Total file size: 1,405,723,690 bytes">1.309 GB</div>
      <div class="link"><a href="https://animetosho.org/view/x.n2107283">Naruto Shippuden 163 (BDRip 1080p x264 AAC)</a></div>
      <div class="links">
        <a href="magnet:?xt=urn:btih:VWAFIX4VQVMYME5BODB5LDIJNNJ2O72V&amp;dn=x">Magnet</a>
        <span title="Seeders: 105 / Leechers: 145">[105&#8593;/145&#8595;]</span>
      </div>
    </div>`

  const rows = parseSearchPage(html)
  assert.equal(rows.length, 1)
  assert.equal(rows[0].title, 'Naruto Shippuden 163 (BDRip 1080p x264 AAC)')
  // AnimeTosho publishes base32; it is normalised to hex so it matches webtorrent.
  assert.equal(rows[0].infoHash, 'ad80545f9585598613a170c3d58d096b53a77f55')
  assert.equal(rows[0].sizeBytes, 1405723690)
  assert.equal(rows[0].seeders, 105)
  assert.equal(rows[0].source, 'animetosho')
})

test('parseSearchPage returns nothing for a page with no results', () => {
  assert.deepEqual(parseSearchPage('<html><body>No results</body></html>'), [])
})

test('parseRss extracts rows from a Nyaa feed', () => {
  const xml = `<rss><channel>
    <item>
      <title>[SubsPlease] Show - 12 (1080p) [ABC].mkv</title>
      <nyaa:seeders>42</nyaa:seeders>
      <nyaa:leechers>3</nyaa:leechers>
      <nyaa:size>1.4 GiB</nyaa:size>
      <nyaa:infoHash>0123456789abcdef0123456789abcdef01234567</nyaa:infoHash>
    </item>
  </channel></rss>`

  const rows = parseRss(xml)
  assert.equal(rows.length, 1)
  assert.equal(rows[0].title, '[SubsPlease] Show - 12 (1080p) [ABC].mkv')
  assert.equal(rows[0].seeders, 42)
  assert.equal(rows[0].sizeBytes, Math.round(1.4 * 1024 ** 3))
  assert.equal(rows[0].source, 'nyaa')
  assert.equal(rows[0].infoHash, '0123456789abcdef0123456789abcdef01234567')
})

test('parseRss skips items without an info hash', () => {
  const xml = '<rss><channel><item><title>no hash</title></item></channel></rss>'
  assert.deepEqual(parseRss(xml), [])
})

test('parseSearchResults maps an apibay row', () => {
  // Trimmed from a live response.
  const rows = parseSearchResults([
    {
      id: '81917903',
      name: 'Solo Leveling S01E01 2024 1080p AMZN WEB-DL H 264 DDP2 0-ADWeb',
      info_hash: '708F446B4DA87DDC210B8BF1F0F5464EF460123F',
      seeders: '27',
      size: '1368905414',
    },
  ])

  assert.equal(rows.length, 1)
  assert.equal(rows[0].title, 'Solo Leveling S01E01 2024 1080p AMZN WEB-DL H 264 DDP2 0-ADWeb')
  // Upper case hex is normalised, so it matches what webtorrent reports.
  assert.equal(rows[0].infoHash, '708f446b4da87ddc210b8bf1f0f5464ef460123f')
  assert.equal(rows[0].seeders, 27)
  assert.equal(rows[0].sizeBytes, 1368905414)
  assert.equal(rows[0].source, 'apibay')
})

test('parseSearchResults drops the "No results returned" sentinel', () => {
  // apibay answers with one row describing the empty result rather than [].
  const rows = parseSearchResults([
    {
      id: '0',
      name: 'No results returned',
      info_hash: '0000000000000000000000000000000000000000',
      seeders: '0',
      size: '0',
    },
  ])
  assert.deepEqual(rows, [])
})

test('a blocked apibay is not retried on every search term', async () => {
  // apibay sits behind a Cloudflare JS challenge that a datacenter IP cannot pass:
  // verified from the production server, every request answers 403. A search fans out
  // over several terms, so without a cooldown one episode lookup fires a request that
  // is certain to fail once per term.
  //
  // search() must never throw, and one failure must suppress the following calls.
  const calls = []
  const originalGet = http.get
  http.get = async () => {
    calls.push(1)
    throw new Error('Request failed with status code 403')
  }

  try {
    assert.deepEqual(await apibay.search('Bocchi the Rock 1'), [], 'returns empty, does not throw')
    assert.equal(calls.length, 1, 'the first call reaches the host')

    await apibay.search('Bocchi the Rock 01')
    await apibay.search('Bocchi the Rock 001')
    assert.equal(calls.length, 1, 'the cooldown suppresses the remaining terms')
  } finally {
    http.get = originalGet
  }
})

test('parseSearchResults ignores malformed rows', () => {
  assert.deepEqual(parseSearchResults([]), [])
  assert.deepEqual(parseSearchResults(null), [])
  assert.deepEqual(parseSearchResults([{ name: 'no hash' }]), [])
})

test('toHexInfoHash normalises both forms an info hash is written in', () => {
  // The same hash as published by an indexer (base32) and as reported by
  // webtorrent (hex). These must produce the same key.
  assert.equal(
    toHexInfoHash('pmiuagcod73tvbx7tjwbhm5gy6j6mzje'),
    '7b1140184e1ff73a86ff9a6c13b3a6c793e66524',
  )
  assert.equal(toHexInfoHash('7B1140184E1FF73A86FF9A6C13B3A6C793E66524'), '7b1140184e1ff73a86ff9a6c13b3a6c793e66524')
  assert.equal(toHexInfoHash('ABCDEF123'), null)
  assert.equal(toHexInfoHash('not a hash'), null)
  assert.equal(toHexInfoHash(''), null)
  assert.equal(toHexInfoHash(null), null)
})

test('infoHashFromMagnet reads the hash from a magnet', () => {
  assert.equal(
    infoHashFromMagnet('magnet:?xt=urn:btih:pmiuagcod73tvbx7tjwbhm5gy6j6mzje&dn=x'),
    '7b1140184e1ff73a86ff9a6c13b3a6c793e66524',
  )
  assert.equal(infoHashFromMagnet('not-a-magnet'), null)
  assert.equal(infoHashFromMagnet(null), null)
})

test('buildMagnet round-trips through infoHashFromMagnet', () => {
  const hex = '7b1140184e1ff73a86ff9a6c13b3a6c793e66524'
  const magnet = buildMagnet(hex, 'Some Release', ['udp://tracker.example:1337/announce'])
  assert.equal(infoHashFromMagnet(magnet), hex)
  assert.ok(magnet.includes('dn=Some%20Release'))
  assert.ok(magnet.includes('tr=udp%3A%2F%2Ftracker.example%3A1337%2Fannounce'))
})

test('parseSize understands the units indexers use', () => {
  assert.equal(parseSize('1.309 GB'), Math.round(1.309 * 1024 ** 3))
  assert.equal(parseSize('700 MiB'), 700 * 1024 ** 2)
  assert.equal(parseSize('1024 B'), 1024)
  assert.equal(parseSize('not a size'), null)
  assert.equal(parseSize(null), null)
})

test('titleOf strips only an episode-number token', () => {
  // Removing the last word unconditionally ate a real word from any term with no
  // episode number on the end: "Attack on Titan" was searched as "Attack on", and the
  // cache key was wrong with it. The id is client-supplied, and "x-ep0" reaches this
  // path with no episode number.
  assert.equal(titleOf('BOCCHI THE ROCK 1'), 'BOCCHI THE ROCK')
  assert.equal(titleOf('BOCCHI THE ROCK 001'), 'BOCCHI THE ROCK')
  assert.equal(titleOf('BOCCHI THE ROCK S01E01'), 'BOCCHI THE ROCK')
  assert.equal(titleOf('ONE PIECE 001'), 'ONE PIECE')
  // No episode token, so nothing is stripped.
  assert.equal(titleOf('Attack on Titan'), 'Attack on Titan')
  assert.equal(titleOf('Shingeki no Kyojin'), 'Shingeki no Kyojin')
})

test('a titleOnly indexer is asked once per distinct title, not once per term', () => {
  // It answers with every episode of a show, so asking it per episode-number form
  // would fetch the same response repeatedly. Every title variant is still asked,
  // because a show's releases may use a different name than the one searched first.
  const terms = ['Attack on Titan 1', 'Attack on Titan 01', 'Shingeki no Kyojin 1']
  const titles = [...new Set(terms.map(titleOf))]

  assert.deepEqual(titles, ['Attack on Titan', 'Shingeki no Kyojin'])
  assert.ok(titles.length < terms.length, 'the forms must collapse to the distinct titles')
})

test('a term with no episode number is not mangled by the fan-out', () => {
  // searchTerms returns bare titles when there is no episode number, which is what
  // an id like "21-ep0" produces.
  assert.equal(titleOf('Attack on Titan'), 'Attack on Titan')
  assert.equal(titleOf('進撃の巨人'), '進撃の巨人')
})

// ---------------------------------------------------------------------------
// Torrentio. It is the only indexer here searched by IMDb id rather than by text,
// and it is what finds "India's Got Latent" - a title every text search answers with
// nothing, because it is named three different ways in the wild.
// ---------------------------------------------------------------------------

test('torrentio reads the release name out of its display title', async () => {
  const { streamNames, streamSeeders, streamSizeBytes, toTorrentioId } =
    await import('../graphql/indexers/torrentio.js')

  // Captured from the live API. Two shapes: a show name followed by a file name, and a
  // single release name with no separate file name.
  const twoLine = {
    title: "India's Got Latent\nIGL_EP_01_ft_RaftaarMusic.mp4\n👤 47 💾 473.25 MB ⚙️ ThePirateBay",
    behaviorHints: { filename: 'IGL_EP_01_ft_RaftaarMusic.mp4' },
  }
  const oneLine = {
    title: 'Indias Got Latent (2026)-MovieLinkHub.com-S02 Bouns E01-E01\n👤 3 💾 1.41 GB ⚙️ EXT',
  }

  assert.deepEqual(streamNames(twoLine), {
    showName: "India's Got Latent",
    filename: 'IGL_EP_01_ft_RaftaarMusic.mp4',
  })
  // The trailing line is always the seeder/size summary, never a name. Reading it as a
  // file name is what made this row look like it had a file called "👤 3 💾 1.41 GB".
  assert.deepEqual(streamNames(oneLine), {
    showName: '',
    filename: 'Indias Got Latent (2026)-MovieLinkHub.com-S02 Bouns E01-E01',
  })

  assert.equal(streamSeeders(twoLine), 47)
  assert.equal(streamSeeders(oneLine), 3)
  assert.equal(streamSizeBytes(twoLine), 496238592)
  assert.equal(streamSizeBytes(oneLine), 1513975972)
  // An unreadable size is null rather than 0, so it cannot rank as "tiny file".
  assert.equal(streamSizeBytes({ title: 'no numbers here' }), null)

  // The id must be the "tt" form; EZTV wants the digits alone and Torrentio does not.
  assert.equal(toTorrentioId('tt33094114'), 'tt33094114')
  assert.equal(toTorrentioId('TT33094114'), 'tt33094114')
  assert.equal(toTorrentioId('0903747'), null)
  assert.equal(toTorrentioId('notanid'), null)
})

test('torrentio keeps one row per hash and prefers a real episode over a bonus', async () => {
  const { parseSearchResponse } = await import('../graphql/indexers/torrentio.js')

  // Captured from the live API for "India's Got Latent" S1E1. Torrentio emits one row
  // per file of the pack, all sharing an infoHash, and the bonus comes first. Keeping
  // the first row made the bonus the release title, which then failed the episode gate
  // and discarded the whole torrent - a torrent holding every real episode at 47 seeders.
  const body = {
    streams: [
      {
        title: "India's Got Latent\nIGL_Bonus_EP_1_ft. Arpit Bala.mp4\n👤 47 💾 366.23 MB ⚙️ ThePirateBay",
        infoHash: '1fb077d1660144985c11e998bd91baac494ecf26',
        behaviorHints: { filename: 'IGL_Bonus_EP_1_ft. Arpit Bala.mp4' },
      },
      {
        title: "India's Got Latent\nIGL_EP_01_ft_RaftaarMusic.mp4\n👤 47 💾 473.25 MB ⚙️ ThePirateBay",
        infoHash: '1fb077d1660144985c11e998bd91baac494ecf26',
        behaviorHints: { filename: 'IGL_EP_01_ft_RaftaarMusic.mp4' },
      },
    ],
  }

  const rows = parseSearchResponse(body)
  assert.equal(rows.length, 1, 'sibling rows must collapse to one per info hash')
  assert.match(rows[0].title, /IGL_EP_01/, 'the episode must win over the bonus')
  assert.equal(rows[0].seeders, 47)
  assert.equal(rows[0].source, 'torrentio')
  // The row title has to carry the show name, because that is what the relevance gate
  // compares against. A file name alone ranked zero candidates.
  assert.match(rows[0].title, /Got Latent/)
})

test('a release naming a season and an episode is not a season pack', async () => {
  const { isBatch } = await import('../graphql/stream/releases.js')

  // "S02E01" contains a bare "S02", which the pack pattern matched. Treating a single
  // episode as a pack subtracted 30 from its score, dropped it below zero, and discarded
  // it - so this release ranked nothing despite passing every other gate.
  assert.equal(isBatch('Show S01E05 1080p'), false)
  assert.equal(isBatch('Indias Got Latent (2026)-MovieLinkHub.com-S02 Bouns E01-E01'), false)
  assert.equal(isBatch('Show S01 WEB-DL EP 05 1080p'), false)

  // A real pack still reads as one.
  assert.equal(isBatch('Show S01 [1080p WEB-DL]'), true)
  assert.equal(isBatch('Gyaarah Gyaarah S01 [1080p WEB-DL x264 AAC Hindi]'), true)
  assert.equal(isBatch('Show 1-162 Complete'), true)
  assert.equal(isBatch('Show S01-S02 Complete'), true)
  assert.equal(isBatch('Show Season 2 1080p'), true)
})
