import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'

// Imports the server's modules the way the server does, so a missing or renamed
// import fails here instead of at runtime.
//
// `node --check` only parses a file; it does not resolve identifiers. A module that
// referenced an import it no longer had passed every existing test and then crashed
// the server on the first /health request, because nothing loaded that file's
// exported functions under the real entry point.
const root = path.dirname(new URL(import.meta.url).pathname)

function moduleFiles(dir) {
  const found = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules') continue
      found.push(...moduleFiles(full))
    } else if (entry.name.endsWith('.js')) {
      found.push(full)
    }
  }
  return found
}

test('every module loads and exposes what the server imports', async () => {
  const serverRoot = path.join(root, '..')
  const files = [
    ...moduleFiles(path.join(serverRoot, 'graphql')),
    ...moduleFiles(path.join(serverRoot, 'lib')),
  ]

  assert.ok(files.length > 10, 'expected to find the server modules')

  for (const file of files) {
    const module = await import(file)
    assert.ok(module, `${path.relative(serverRoot, file)} did not load`)
  }
})

test('the entry point imports resolve', async () => {
  // The exported functions the HTTP layer calls. Each one is exercised by /health
  // or /graphql, so a missing binding here is a server that cannot start.
  const { ffmpegAvailable, parseProbe } = await import('../graphql/stream/media.js')
  const { enabledIndexers, indexerNames, indexerStatus } = await import('../graphql/indexers/index.js')
  const { provider, providerNames } = await import('../graphql/providers/index.js')
  const { resolveStream } = await import('../graphql/stream/index.js')
  const { serveStream } = await import('../graphql/stream/serve.js')

  for (const [name, value] of Object.entries({
    ffmpegAvailable,
    parseProbe,
    enabledIndexers,
    indexerNames,
    indexerStatus,
    provider,
    providerNames,
    resolveStream,
    serveStream,
  })) {
    assert.ok(value, `${name} is not exported`)
  }

  assert.equal(await ffmpegAvailable(), true, 'ffmpeg must be available')
})


test('every enabled indexer runs and returns rows of the documented shape', async (t) => {
  // A module that imports cleanly can still fail on its first call: a helper that was
  // used but never imported throws a ReferenceError inside the function body, which
  // `node --check` cannot see and which merely loading the module does not trigger.
  //
  // That happened here. Every indexer threw "ttlFor is not defined" on every search
  // while the whole suite stayed green, because no test ever *called* an indexer.
  // This one does, with the upstream stubbed per host so it stays offline.
  const { enabledIndexers } = await import('../graphql/indexers/index.js')
  const { http } = await import('../lib/http.js')

  const indexers = enabledIndexers()
  assert.ok(indexers.length > 0, 'at least one indexer must be enabled')

  const RSS =
    '<rss><channel><item><title>Stub Show - 01 [1080p].mkv</title>' +
    '<nyaa:seeders>5</nyaa:seeders><nyaa:size>1 GiB</nyaa:size>' +
    '<nyaa:infoHash>0123456789abcdef0123456789abcdef01234567</nyaa:infoHash>' +
    '</item></channel></rss>'

  const HASH = '0123456789abcdef0123456789abcdef01234567'

  // Each indexer is answered in the format it asks for. Returning one shape for all of
  // them is how this test first failed: the SubsPlease indexer rejected the RSS body
  // with "not a JSON response", which is exactly the guard it is supposed to have.
  const bodies = {
    'https://nyaa.si/': RSS,
    'https://nyaa.net/': RSS,
    'https://animetosho.org/search': '<div class="home_list_entry"></div>',
    'https://apibay.org/q.php': [{ id: '1', name: 'Stub Show - 01', info_hash: HASH, seeders: '5', size: '100' }],
    'https://subsplease.org/api/': {
      'Stub Show - 01': { episode: '01', downloads: [{ res: '1080', magnet: `magnet:?xt=urn:btih:${HASH}&dn=x&xl=100` }] },
    },
  }

  const originalGet = http.get
  http.get = async (url) => {
    const body = bodies[url]
    if (body === undefined) throw new Error(`unstubbed request to ${url}`)
    return { data: body }
  }
  t.after(() => {
    http.get = originalGet
  })

  for (const indexer of indexers) {
    let rows
    try {
      rows = await indexer.search(`wiring check ${indexer.name}`)
    } catch (error) {
      assert.fail(`indexer "${indexer.name}" threw on its first call: ${error.message}`)
    }
    assert.ok(Array.isArray(rows), `indexer "${indexer.name}" must return an array`)

    for (const row of rows) {
      assert.match(row.infoHash, /^[0-9a-f]{40}$/, `"${indexer.name}" returned a bad info hash`)
      assert.ok(row.title, `"${indexer.name}" returned a row with no title`)
      assert.equal(row.source, indexer.name, `"${indexer.name}" mislabelled its own row`)
    }
  }
})
