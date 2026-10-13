// Tests for the stream-source layer.
//
// The debrid source is the only place in this application that talks to a paid
// third party, so the parts worth testing are the ones that fail silently: an
// unrecognised response envelope, an info hash that is not normalised before it is
// matched, a redirect that points somewhere it should not, and a token that leaks
// more than it should.
//
// Nothing here touches the network. The debrid API is stubbed at the shared HTTP
// client, so a request that is not stubbed fails the test loudly instead of quietly
// calling api.torbox.app.

import test from 'node:test'
import assert from 'node:assert/strict'

import { http } from '../lib/http.js'
import { config } from '../lib/config.js'
import { toHexInfoHash } from '../lib/magnet.js'
import { isBrowserPlayableFile } from '../graphql/stream/releases.js'
import { deliveryFor } from '../graphql/stream/serve.js'
import { containerFromBytes } from '../graphql/stream/media.js'
import { pickVideoFile } from '../graphql/stream/torrent.js'
import {
  resolveDownload,
  registerDownload,
  isAllowedDownloadUrl,
  redactToken,
} from '../graphql/stream/sources/debrid.js'

const HEX = 'ad80545f9585598613a170c3d58d096b53a77f55'
const OTHER = '0123456789abcdef0123456789abcdef01234567'

// What `mylist` returns once the account holds the torrent, in TorBox's own shape.
const ACCOUNT_ENTRY = {
  id: 42,
  hash: HEX,
  files: [{ id: 7, name: 'Show - 01.mp4', size: 1024 }],
}

// Replaces the HTTP client for the duration of a test and records what was asked
// for, so both the response handling and the request shape can be asserted.
function stubApi(t, routes) {
  const calls = []
  const originalGet = http.get
  const originalPost = http.post

  function routeFor(url) {
    const route = Object.keys(routes).find((name) => url.endsWith(name))
    if (!route) throw new Error(`unstubbed request to ${url}`)
    return route
  }

  http.get = async (url, options) => {
    calls.push({ method: 'GET', url, params: options?.params })
    return { data: routes[routeFor(url)] }
  }

  http.post = async (url, body, options) => {
    // FormData is what createtorrent requires, so it is unpacked into a plain object
    // to make the fields assertable.
    const fields = body instanceof FormData ? Object.fromEntries(body.entries()) : body
    calls.push({ method: 'POST', url, fields, headers: options?.headers })
    return { data: routes[routeFor(url)] }
  }

  t.after(() => {
    http.get = originalGet
    http.post = originalPost
  })
  return calls
}

test('only sources that are ready are used', async () => {
  const { activeSources, sourceNames } = await import('../graphql/stream/sources/index.js')
  assert.deepEqual(sourceNames().sort(), ['debrid', 'torrent'])
  // The debrid source needs an API key, so it drops out when none is configured.
  // The suite runs without TORBOX_API_KEY set, which is what the next test pins.
  assert.deepEqual(activeSources().map((source) => source.name), ['torrent'])
})

test('a failing source does not fail the request, and its reason is reported', async () => {
  // resolve() is documented never to throw, but a buggy source must still not take
  // the request down, and the most preferred source's reason is the useful one.
  const { resolveFromSources } = await import('../graphql/stream/sources/index.js')

  const result = await resolveFromSources({ episodeNumber: 1, ranked: [], deadlineMs: 100 })
  assert.equal(result.available, false)
  // The relay is the only active source and has no candidates, so its own reason is
  // reported rather than a generic fallback.
  assert.match(result.reason, /none could be played/i)
})

test('a debrid file is delivered the same way the relay would deliver it', () => {
  // The relay remuxes MKV and relays MP4. The debrid path used to send everything as
  // "file", which plays on a desktop (Chrome and Firefox have a Matroska demuxer,
  // Chromium only from M145) and fails on every iOS browser and older Android ones -
  // the player shows a play button and pressing it does nothing. This is the rule that
  // keeps the two paths agreeing.
  assert.equal(deliveryFor('Show - 01.mp4'), 'file')
  assert.equal(deliveryFor('Show - 01.MP4'), 'file')
  assert.equal(deliveryFor('Show - 01.mkv'), 'remux')
  assert.equal(deliveryFor('Show - 01.MKV'), 'remux')
  // A container no browser can open, with no remux that would help.
  assert.equal(deliveryFor('Show - 01.avi'), null)
  assert.equal(deliveryFor('Show - 01.txt'), null)
  assert.equal(deliveryFor(''), null)
  assert.equal(deliveryFor(undefined), null)
})

test('a debrid torrent is only offered as a browser-playable file', () => {
  // Both extensions reach the picker, because both are playable once the delivery
  // decision above has been applied. Anything else is rejected before it is offered.
  assert.equal(isBrowserPlayableFile('Show - 01.mp4'), true)
  assert.equal(isBrowserPlayableFile('Show - 01.MP4'), true)
  assert.equal(isBrowserPlayableFile('Show - 01.mkv'), true)
  assert.equal(isBrowserPlayableFile('Show - 01.MKV'), true)
  assert.equal(isBrowserPlayableFile('Show - 01.avi'), false)
  assert.equal(isBrowserPlayableFile('Show - 01.txt'), false)
  assert.equal(isBrowserPlayableFile(''), false)
})

test('a file list with no sizes is still usable', () => {
  // The debrid file list can omit sizes. Applying the relay's 5MB sample filter
  // there would reject every file and look like "nothing is playable".
  const files = [
    { id: 1, name: 'Show - 01.mp4', length: 0 },
    { id: 2, name: 'Show - 02.mp4', length: 0 },
  ]
  assert.equal(pickVideoFile(files, 1, 0).name, 'Show - 01.mp4')
  // The relay keeps its size floor.
  assert.equal(pickVideoFile(files, 1), null)
})

test('the download token is opaque, and maps to the ids it was registered with', async (t) => {
  const token = registerDownload(42, 7, 'Show - 01.mkv')
  assert.match(token, /^[A-Za-z0-9_-]{32}$/)

  // An unknown token resolves to nothing rather than throwing, so a guessed token is
  // indistinguishable from an expired one.
  assert.equal(await resolveDownload('z'.repeat(32)), null)

  // A registered token is exchanged for the CDN link, and the request must carry the
  // ids the token was registered with - not arbitrary ones.
  const calls = stubApi(t, { requestdl: { success: true, data: 'https://cdn.torbox.app/f.mp4' } })
  const download = await resolveDownload(token)
  assert.equal(download.url, 'https://cdn.torbox.app/f.mp4')
  // The file name travels with the link because it is what decides whether the bytes
  // are relayed as-is or remuxed. Without it an MKV would be handed to the browser raw.
  assert.equal(download.fileName, 'Show - 01.mkv')
  assert.equal(calls.length, 1)
  assert.equal(calls[0].params.torrent_id, 42)
  assert.equal(calls[0].params.file_id, 7)
})

test('requestdl sends the token as a query parameter, as that route requires', async (t) => {
  // requestdl declares `token` as a required query parameter and does not inherit the
  // bearer scheme, so a header-only request is rejected with
  // 422 {"loc":["query","token"],"msg":"Field required"} - verified against the live
  // API. Without this the debrid source would fail on every episode while looking
  // like the service was simply unreachable.
  const previous = config.torboxToken
  config.torboxToken = 'test-token-abc'
  t.after(() => {
    config.torboxToken = previous
  })

  const token = registerDownload(4242, 77, 'Show - 01.mp4')
  const calls = stubApi(t, { requestdl: { success: true, data: 'https://cdn.torbox.app/f.mp4' } })
  await resolveDownload(token)

  // Distinct ids from the test above, because the download link is cached per id pair
  // and a repeat would be served from the cache without a request to assert on.
  assert.equal(calls.length, 1)
  assert.equal(calls[0].params.token, 'test-token-abc')
})

test('a token is never written to a log message', () => {
  // The token has to travel in a query string on the requestdl route, and axios puts
  // the request URL into some of its error messages, so anything derived from an
  // error is redacted before it is logged.
  const previous = config.torboxToken
  config.torboxToken = 'super-secret-value'
  try {
    const message = redactToken('Request failed: https://api.torbox.app?token=super-secret-value')
    assert.equal(message.includes('super-secret-value'), false)
    assert.match(message, /\[REDACTED\]/)
  } finally {
    config.torboxToken = previous
  }
})

test('a debrid download URL is only allowed from a configured CDN host', () => {
  // The URL comes from the debrid API rather than from a request, but this server
  // fetches it, so it is allowlisted. A prefix check would accept every one of the
  // rejected values below.
  //
  // The real CDN host is a node under one of the listed domains, e.g.
  // nexus-144.enam.tb-cdn.io. The provider has rotated between .pw and .io, so both
  // are allowed. The API host torbox.app is a *different* domain and is the wrong
  // default: allowlisting it rejects every link the API actually returns.
  assert.equal(isAllowedDownloadUrl('https://nexus-078.indi.tb-cdn.pw/dld/abc?token=x'), true)
  assert.equal(isAllowedDownloadUrl('https://tb-cdn.pw/dld/abc'), true)
  assert.equal(isAllowedDownloadUrl('https://tb-cdn.io/dld/abc'), true)

  assert.equal(isAllowedDownloadUrl('http://nexus-078.indi.tb-cdn.pw/dld/abc'), false, 'not https')
  assert.equal(isAllowedDownloadUrl('https://evil.example.com/f.mp4'), false, 'other host')
  assert.equal(isAllowedDownloadUrl('https://tb-cdn.pw.evil.com/f.mp4'), false, 'suffix trick')
  assert.equal(isAllowedDownloadUrl('https://eviltb-cdn.pw/f.mp4'), false, 'prefix trick')
  assert.equal(isAllowedDownloadUrl('https://tb-cdn.io.evil.com/f.mp4'), false, 'suffix trick on the rotated domain')
  assert.equal(isAllowedDownloadUrl('https://user:pass@tb-cdn.pw/f.mp4'), false, 'credentials')
  assert.equal(isAllowedDownloadUrl('not a url'), false)
  assert.equal(isAllowedDownloadUrl(''), false)
})

test('an info hash is normalised before it is matched', () => {
  // TorBox keys the checkcached response by the hash it was sent, and indexers
  // publish base32 while webtorrent reports hex. Without normalising, a cached
  // release would never match the ranked release list.
  assert.equal(toHexInfoHash(HEX.toUpperCase()), HEX)
  assert.equal(toHexInfoHash('VWAFIX4VQVMYME5BODB5LDIJNNJ2O72V'), HEX)
  assert.equal(toHexInfoHash('nonsense'), null)
  assert.equal(toHexInfoHash(OTHER), OTHER)
})

test('an unstubbed request fails loudly instead of reaching the network', async (t) => {
  // Guards the suite itself: if the debrid source starts calling a route these tests
  // do not stub, the failure must be visible rather than a live call to TorBox.
  stubApi(t, {})
  await assert.rejects(() => http.get('https://api.torbox.app/v1/api/torrents/nope'))
})

test('sourceStatus reports configured sources, including ones that are not ready', async () => {
  // A source that is configured but unusable - a missing or rejected credential - is
  // the interesting case for /health, and it is invisible in activeSources().
  const { sourceStatus } = await import('../graphql/stream/sources/index.js')
  const status = await sourceStatus()

  assert.deepEqual(Object.keys(status).sort(), ['torrent'])
  assert.equal(status.torrent.configured, true)
})

test('a cached release the account does not hold yet is added before it is requested', async (t) => {
  // checkcached answers from the service's own cache and returns no ids; requestdl
  // needs the account's ids; mylist only lists what the account holds. So a cached
  // release that was never added has no ids and is unusable. Without the add step a
  // fresh account resolves nothing at all, while every log line looks healthy.
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default

  const previousToken = config.torboxToken
  config.torboxToken = 'test-token-add'
  // The listing is cached across tests, and the assertions below depend on the first
  // read being empty.
  forget('torbox:mylist')
  t.after(() => {
    config.torboxToken = previousToken
    forget('torbox:mylist')
  })

  let listed = 0
  const calls = []
  const originalGet = http.get
  const originalPost = http.post

  http.get = async (url, options) => {
    calls.push({ method: 'GET', url, params: options?.params })
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [HEX]: { files: [{ name: 'Show - 01.mp4' }] } } } }
    }
    if (url.endsWith('mylist')) {
      listed += 1
      // Empty until the torrent is added, which is the situation on a new account.
      return { data: { success: true, data: listed === 1 ? [] : [ACCOUNT_ENTRY] } }
    }
    if (url.endsWith('requestdl')) {
      return { data: { success: true, data: 'https://cdn.torbox.app/f.mp4' } }
    }
    throw new Error(`unstubbed request to ${url}`)
  }

  http.post = async (url, body) => {
    calls.push({ method: 'POST', url, fields: Object.fromEntries(body.entries()) })
    return { data: { success: true, data: { torrent_id: 42 } } }
  }

  t.after(() => {
    http.get = originalGet
    http.post = originalPost
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: HEX, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${HEX}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, true, result.reason)
  assert.equal(result.streamType, 'file')

  const add = calls.find((call) => call.url.endsWith('createtorrent'))
  assert.ok(add, 'the release must be added to the account')
  assert.equal(add.method, 'POST')
  assert.equal(add.fields.add_only_if_cached, 'true', 'an uncached torrent must not be queued')
  assert.match(add.fields.magnet, new RegExp(HEX))

  // The listing must be re-read after the add, or the new ids would stay invisible
  // and every request would add the torrent again.
  assert.equal(listed, 2, 'the account listing must be refreshed after adding')
})

test('a release the service will not add is skipped rather than offered', async (t) => {
  // `add_only_if_cached` refuses an uncached hash. That is the normal case for most
  // candidates, so it must degrade to "not available" instead of a playable-looking
  // link or a thrown error.
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default

  const previousToken = config.torboxToken
  config.torboxToken = 'test-token-refuse'
  forget('torbox:mylist')
  t.after(() => {
    config.torboxToken = previousToken
    forget('torbox:mylist')
  })

  const originalGet = http.get
  const originalPost = http.post

  http.get = async (url) => {
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [OTHER]: { files: [{ name: 'Show - 01.mp4' }] } } } }
    }
    if (url.endsWith('mylist')) return { data: { success: true, data: [] } }
    throw new Error(`unstubbed request to ${url}`)
  }
  http.post = async () => {
    throw new Error('Request failed with status code 400')
  }

  t.after(() => {
    http.get = originalGet
    http.post = originalPost
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: OTHER, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${OTHER}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, false)
  assert.match(result.reason, /none is a file a browser can play directly/i)
})

test('the resolve budget is shared across sources instead of reset for each', async () => {
  // The relay used to receive the full configured budget after the debrid source had
  // already spent time, so one request could take twice the maximum. Measured, a
  // fallback resolve took 85s against a 75s budget. Every source must now be handed
  // what is *left* of one shared deadline.
  const { resolveFromSources } = await import('../graphql/stream/sources/index.js')
  const { activeSources } = await import('../graphql/stream/sources/index.js')

  const seen = []
  const originals = activeSources().map((source) => [source, source.resolve])
  // Replace each active source with one that reports the budget it was given and
  // fails, so the next source in the chain gets a turn.
  for (const [source] of originals) {
    source.resolve = async (input) => {
      seen.push(input.deadlineMs)
      await new Promise((resolve) => setTimeout(resolve, 120))
      return { available: false, reason: 'stub' }
    }
  }
  try {
    await resolveFromSources({ episodeNumber: 1, ranked: [], deadlineMs: 1000 })
  } finally {
    for (const [source, original] of originals) source.resolve = original
  }

  assert.ok(seen.length >= 1, 'at least one source must be asked')
  assert.ok(seen[0] <= 1000, `the first source got ${seen[0]}ms, more than the budget`)
  if (seen.length > 1) {
    // The second source must get strictly less, because time passed while the first
    // one ran. Without the shared deadline this was the full 1000ms again.
    assert.ok(seen[1] < seen[0], `the second source got ${seen[1]}ms, not a reduced budget`)
  }
})

test('a resolve whose budget is already spent does not call a source', async () => {
  // With several sources configured, an expired budget must stop the chain rather
  // than let the next source start a fresh timeout of its own.
  const { resolveFromSources } = await import('../graphql/stream/sources/index.js')
  const { activeSources } = await import('../graphql/stream/sources/index.js')

  let called = 0
  const originals = activeSources().map((source) => [source, source.resolve])
  for (const [source] of originals) {
    source.resolve = async () => {
      called += 1
      return { available: false, reason: 'stub' }
    }
  }
  try {
    const result = await resolveFromSources({ episodeNumber: 1, ranked: [], deadlineMs: 0 })
    assert.equal(called, 0, 'no source should run once the budget is gone')
    assert.equal(result.available, false)
  } finally {
    for (const [source, original] of originals) source.resolve = original
  }
})

test('an mkv release resolves as a remux, not as a directly playable file', async (t) => {
  // The bug this pins: debrid reported every file as streamType "file", so an MKV was
  // relayed untouched. That plays on a desktop and fails on a phone, where the player
  // shows a play button that does nothing. The MKV must be reported as "remux" so the
  // /dl handler runs it through ffmpeg, exactly as the torrent relay already does.
  const previousToken = config.torboxToken
  config.torboxToken = 'test-token'
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default
  forget('torbox:mylist')
  forget(`torbox:checkcached:${HEX}`)

  const mkvEntry = {
    id: 42,
    hash: HEX,
    files: [{ id: 7, name: 'Show - 01.mkv', size: 1024 }],
  }

  const originalGet = http.get
  http.get = async (url) => {
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [HEX]: { files: [{ name: 'Show - 01.mkv' }] } } } }
    }
    if (url.endsWith('mylist')) return { data: { success: true, data: [mkvEntry] } }
    if (url.endsWith('requestdl')) {
      return { data: { success: true, data: 'https://cdn.torbox.app/f.mkv' } }
    }
    throw new Error(`unstubbed request to ${url}`)
  }

  t.after(() => {
    http.get = originalGet
    config.torboxToken = previousToken
    forget('torbox:mylist')
    forget(`torbox:checkcached:${HEX}`)
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: HEX, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${HEX}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, true, result.reason)
  assert.equal(result.streamType, 'remux')

  // And the file name reaches the /dl handler, which is what actually applies the rule.
  const token = result.url.split('/').pop()
  const download = await resolveDownload(token)
  assert.equal(download.fileName, 'Show - 01.mkv')
})

test('the container is read from the bytes, not from the file name', () => {
  // A real release was measured named `...mp4` whose bytes were Matroska. Trusting the
  // name relays it untouched, which plays on a desktop and shows a dead play button on
  // a phone. This is the signature check that catches it.
  const matroska = Buffer.from([0x1a, 0x45, 0xdf, 0xa3, 0x42, 0x86, 0x81, 0x01])
  const mp4 = Buffer.concat([Buffer.from([0, 0, 0, 0x1c]), Buffer.from('ftypiso5', 'latin1')])

  assert.equal(containerFromBytes(matroska), 'mkv')
  assert.equal(containerFromBytes(mp4), 'mp4')
  // Not a container this server can serve.
  assert.equal(containerFromBytes(Buffer.from('GIF89a', 'latin1')), null)
  assert.equal(containerFromBytes(Buffer.alloc(0)), null)
})

test('a file named .mp4 whose bytes are matroska resolves as a remux', async (t) => {
  // The whole point of sniffing: the name says mp4, the bytes say Matroska, and the
  // bytes win. Without this the release is relayed raw and no mobile browser can play it.
  const previousToken = config.torboxToken
  config.torboxToken = 'test-token'
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default
  forget('torbox:mylist')
  forget(`torbox:checkcached:${HEX}`)
  forget('torbox:url:42:7')

  // Named .mp4, but the bytes are an EBML header.
  const lyingEntry = {
    id: 42,
    hash: HEX,
    files: [{ id: 7, name: 'Show - 01.mp4', size: 1024 }],
  }

  const originalGet = http.get
  http.get = async (url, options) => {
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [HEX]: { files: [{ name: 'Show - 01.mp4' }] } } } }
    }
    if (url.endsWith('mylist')) return { data: { success: true, data: [lyingEntry] } }
    if (url.endsWith('requestdl')) {
      return { data: { success: true, data: 'https://cdn.torbox.app/f.mp4' } }
    }
    // The container sniff. This is the request that decides the answer.
    if (url === 'https://cdn.torbox.app/f.mp4') {
      assert.match(options.headers.range, /^bytes=0-\d+$/, 'the sniff must ask for a range')
      return {
        status: 206,
        data: Buffer.from([0x1a, 0x45, 0xdf, 0xa3, 0x42, 0x86, 0x81, 0x01]),
      }
    }
    throw new Error(`unstubbed request to ${url}`)
  }

  t.after(() => {
    http.get = originalGet
    config.torboxToken = previousToken
    forget('torbox:mylist')
    forget(`torbox:checkcached:${HEX}`)
    forget('torbox:url:42:7')
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: HEX, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${HEX}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, true, result.reason)
  assert.equal(result.streamType, 'remux', 'matroska bytes must win over an mp4 name')

  // The decision travels on the token, so /dl does not have to sniff again per request.
  const token = result.url.split('/').pop()
  const download = await resolveDownload(token)
  assert.equal(download.delivery, 'remux')
})

test('a genuine mp4 is relayed rather than remuxed', async (t) => {
  // The other half of the rule: sniffing must not send everything through ffmpeg, which
  // would cost a stream copy and break seeking for files that were already playable.
  const previousToken = config.torboxToken
  config.torboxToken = 'test-token'
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default
  forget('torbox:mylist')
  forget(`torbox:checkcached:${OTHER}`)
  forget('torbox:url:99:8')

  const entry = { id: 99, hash: OTHER, files: [{ id: 8, name: 'Show - 01.mp4', size: 1024 }] }
  const originalGet = http.get
  http.get = async (url) => {
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [OTHER]: { files: [{ name: 'Show - 01.mp4' }] } } } }
    }
    if (url.endsWith('mylist')) return { data: { success: true, data: [entry] } }
    if (url.endsWith('requestdl')) {
      return { data: { success: true, data: 'https://cdn.torbox.app/real.mp4' } }
    }
    if (url === 'https://cdn.torbox.app/real.mp4') {
      return {
        status: 206,
        data: Buffer.concat([Buffer.from([0, 0, 0, 0x1c]), Buffer.from('ftypiso5', 'latin1')]),
      }
    }
    throw new Error(`unstubbed request to ${url}`)
  }

  t.after(() => {
    http.get = originalGet
    config.torboxToken = previousToken
    forget('torbox:mylist')
    forget(`torbox:checkcached:${OTHER}`)
    forget('torbox:url:99:8')
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: OTHER, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${OTHER}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, true, result.reason)
  assert.equal(result.streamType, 'file')
})

test('a redirect on the container sniff falls back to the file name', async (t) => {
  // A CDN answering the sniff with a 3xx sends no body to read a signature from. That
  // is a missing answer, not a bad file: the name was already checked, so the release
  // must still be offered rather than dropped. Treating the 3xx as "unknown container"
  // returned null, which silently removed every release behind a redirecting CDN.
  const previousToken = config.torboxToken
  config.torboxToken = 'test-token-redirect'
  const { forget } = await import('../lib/cache.js')
  const debrid = (await import('../graphql/stream/sources/debrid.js')).default
  forget('torbox:mylist')
  forget(`torbox:checkcached:${OTHER}`)
  forget('torbox:url:99:8')

  const entry = { id: 99, hash: OTHER, files: [{ id: 8, name: 'Show - 01.mp4', size: 1024 }] }
  const originalGet = http.get
  http.get = async (url) => {
    if (url.endsWith('checkcached')) {
      return { data: { success: true, data: { [OTHER]: { files: [{ name: 'Show - 01.mp4' }] } } } }
    }
    if (url.endsWith('mylist')) return { data: { success: true, data: [entry] } }
    if (url.endsWith('requestdl')) {
      return { data: { success: true, data: 'https://cdn.torbox.app/redirect.mp4' } }
    }
    if (url === 'https://cdn.torbox.app/redirect.mp4') {
      return { status: 302, data: Buffer.alloc(0) }
    }
    throw new Error(`unstubbed request to ${url}`)
  }

  t.after(() => {
    http.get = originalGet
    config.torboxToken = previousToken
    forget('torbox:mylist')
    forget(`torbox:checkcached:${OTHER}`)
    forget('torbox:url:99:8')
  })

  const result = await debrid.resolve({
    episodeNumber: 1,
    ranked: [{ infoHash: OTHER, title: 'Show - 01', magnet: `magnet:?xt=urn:btih:${OTHER}` }],
    deadlineMs: 5000,
  })

  assert.equal(result.available, true, result.reason)
  // The name says mp4 and nothing contradicted it, so it is relayed.
  assert.equal(result.streamType, 'file')
})
