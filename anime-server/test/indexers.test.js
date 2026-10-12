import test from 'node:test'
import assert from 'node:assert/strict'

import { parseSearchPage } from '../graphql/indexers/animetosho.js'
import { parseRss } from '../graphql/indexers/nyaa.js'
import { parseSearchResults } from '../graphql/indexers/apibay.js'
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
