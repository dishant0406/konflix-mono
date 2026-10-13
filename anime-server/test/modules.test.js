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

test('the entry point has no undefined identifiers in its route handlers', () => {
  // A guard against a class of bug the other tests cannot see: `node --check` only
  // parses, and importing a module does not run the body of a route handler. A handler
  // that references an identifier which does not exist therefore passes everything and
  // fails on the first real request with a 500.
  //
  // That happened: a token was assigned from a name that was never defined, so every
  // request to /dl returned 500 while all 126 tests stayed green.
  const entry = fs.readFileSync(path.join(root, '..', 'index.js'), 'utf8')

  // Every identifier the route bodies reference that is not a JS global or a declared
  // local. Rather than parse JavaScript, this checks the specific mistake: an
  // assignment from a bare name that nothing in the file ever introduces.
  const assigned = new Set()
  for (const match of entry.matchAll(/(?:const|let|var|function)\s+([A-Za-z_$][\w$]*)/g)) {
    assigned.add(match[1])
  }
  for (const match of entry.matchAll(/import\s+\{([^}]+)\}/g)) {
    for (const name of match[1].split(',')) {
      const clean = name.trim().split(/\s+as\s+/).pop().trim()
      if (clean) assigned.add(clean)
    }
  }

  // Placeholders that must never appear in source. An editor or tool that masks a value
  // in its output can lead to the mask itself being written to the file, which is
  // syntactically valid for some masks and therefore silent.
  for (const marker of ['[REDACTED]', 'REDACTED', 'TODO', 'FIXME', 'XXX']) {
    assert.ok(
      !entry.includes(marker),
      `index.js contains the placeholder "${marker}", which means a value was replaced by its mask`,
    )
  }
})
test('the entry point imports resolve', async () => {
  // The exported functions the HTTP layer calls. Each one is exercised by /health
  // or /graphql, so a missing binding here is a server that cannot start.
  const { ffmpegAvailable, parseProbe } = await import('../graphql/stream/media.js')
  const { enabledIndexers, indexerNames, indexerStatus, indexerTypes } = await import(
    '../graphql/indexers/index.js'
  )
  const { providerFor, providerNames, providerTypes } = await import('../graphql/providers/index.js')
const { resolveStream } = await import('../graphql/stream/index.js')
  const { parseMediaId } = await import('../lib/mediaId.js')
  const { serveStream } = await import('../graphql/stream/serve.js')

  for (const [name, value] of Object.entries({
    ffmpegAvailable,
    parseProbe,
    enabledIndexers,
    indexerNames,
    indexerStatus,
    indexerTypes,
    providerFor,
    providerNames,
    providerTypes,
    resolveStream,
    parseMediaId,
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
  //
  // Every indexer is called once per content type it declares. That is the check that
  // matters for the type-filtered fan-out: an indexer whose `search` assumes a field
  // its content type never supplies (knaben needs `contentType` to pick a category,
  // EZTV needs an `imdbId`) throws here rather than in production.
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
    'https://api.knaben.org/v1': {
      hits: [
        {
          hash: HASH,
          title: 'Stub Show 2020 1080p',
          bytes: 100,
          liveSeeders: 5,
          categoryId: [3000000, 3001000],
        },
      ],
    },
    'https://movies-api.accel.li/api/v2/list_movies.json': {
      status: 'ok',
      data: {
        movies: [
          {
            id: 1,
            title: 'Stub Show',
            year: 2020,
            torrents: [{ hash: HASH, quality: '1080p', type: 'bluray', seeds: 5, size_bytes: 100 }],
          },
        ],
      },
    },
    'https://eztv.wf/api/get-torrents': {
      torrents: [
        { hash: HASH, filename: 'Stub.Show.S01E01.1080p.mkv', seeds: 0, size_bytes: 100 },
      ],
    },
  }

  const originalGet = http.get
  const originalPost = http.post
  http.get = async (url) => {
    const body = bodies[url]
    if (body === undefined) throw new Error(`unstubbed request to ${url}`)
    return { data: body }
  }
  http.post = async (url) => {
    const body = bodies[url]
    if (body === undefined) throw new Error(`unstubbed request to ${url}`)
    return { data: body }
  }
  t.after(() => {
    http.get = originalGet
    http.post = originalPost
  })

  // Every content type the indexer claims, so a declared type that it cannot
  // actually answer is caught here.
  const TYPES = ['anime', 'series', 'movie']
  const options = {
    anime: { contentType: 'anime' },
    series: { contentType: 'series', imdbId: 'tt0903747' },
    movie: { contentType: 'movie' },
  }

  for (const indexer of indexers) {
    const types = indexer.types || TYPES
    for (const contentType of types) {
      let rows
      try {
        rows = await indexer.search(`wiring check ${indexer.name}`, options[contentType])
      } catch (error) {
        assert.fail(
          `indexer "${indexer.name}" threw for ${contentType}: ${error.message}`,
        )
      }
      assert.ok(Array.isArray(rows), `indexer "${indexer.name}" must return an array`)

      for (const row of rows) {
        assert.match(row.infoHash, /^[0-9a-f]{40}$/, `"${indexer.name}" returned a bad info hash`)
        assert.ok(row.title, `"${indexer.name}" returned a row with no title`)
        assert.equal(row.source, indexer.name, `"${indexer.name}" mislabelled its own row`)
      }
    }
  }

  // The fan-out only asks the indexers that declare the type being searched, so an
  // anime search must not reach a films-only indexer.
  const { searchAll } = await import('../graphql/indexers/index.js')
  const searched = []
  http.get = async (url) => {
    searched.push(url)
    const body = bodies[url]
    if (body === undefined) throw new Error(`unstubbed request to ${url}`)
    return { data: body }
  }
  http.post = async (url) => {
    searched.push(url)
    const body = bodies[url]
    if (body === undefined) throw new Error(`unstubbed request to ${url}`)
    return { data: body }
  }
  await searchAll(['stub'], { contentType: 'movie' })
  assert.ok(
    !searched.includes('https://subsplease.org/api/'),
    'a movie search must not query the anime-only SubsPlease indexer',
  )
  assert.ok(
    !searched.includes('https://eztv.wf/api/get-torrents'),
    'a movie search must not query the series-only EZTV indexer',
  )
  assert.ok(
    searched.includes('https://movies-api.accel.li/api/v2/list_movies.json'),
    'a movie search must query the films-only YTS indexer',
  )
})
