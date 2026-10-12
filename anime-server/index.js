// Konflix API server.
//
// Exposes a GraphQL endpoint for metadata and one HTTP route for video bytes.
// Both are served from the same process, which is what lets the browser play a
// torrent without any CORS proxy.

import fs from 'node:fs'
import { fileURLToPath } from 'node:url'

import cors from 'cors'
import express from 'express'
import { buildSchema, GraphQLError, specifiedRules } from 'graphql'
import { createHandler } from 'graphql-http/lib/use/express'

import { config } from './lib/config.js'
import { http } from './lib/http.js'
import { prepareCacheDir } from './lib/cacheDir.js'
import { provider, providerNames } from './graphql/providers/index.js'
import { enabledIndexers, indexerNames, indexerStatus } from './graphql/indexers/index.js'
import { serveStream } from './graphql/stream/serve.js'
import { sessionCount, cacheSizeBytes, closeAll } from './graphql/stream/torrent.js'
import { activeSources, sourceNames, sourceStatus } from './graphql/stream/sources/index.js'
import { resolveDownload, isAllowedDownloadUrl } from './graphql/stream/sources/debrid.js'
import { ffmpegAvailable } from './graphql/stream/media.js'
import resolvers from './graphql/resolvers/resolvers.js'

const typeDefs = fs.readFileSync(
  fileURLToPath(new URL('./graphql/typeDefs/schema.graphql', import.meta.url)),
  { encoding: 'utf8' },
)

const app = express()
app.use(cors())

// Bounds the request body. GraphQL queries here are tiny, so anything large is a
// mistake or an attempt to exhaust memory.
app.use(express.json({ limit: '64kb' }))

// GraphQL over HTTP, using the reference implementation rather than a framework.
// `rootValue` is the resolver map for the Query type.
const schema = buildSchema(typeDefs)

// Every root field is a network call to a rate limited upstream, and /graphql is
// public, so one document must not be able to fan out into unbounded work. The
// field count is bounded by a static validation rule rather than by a cost model,
// which is enough here because the schema is flat and has no pagination nesting.
const MAX_ROOT_FIELDS = 10

function limitRootFields(context) {
  return {
    Document: {
      enter(node) {
        const operation = node.definitions.find(
          (definition) => definition.kind === 'OperationDefinition',
        )
        const selections = operation?.selectionSet?.selections || []
        if (selections.length > MAX_ROOT_FIELDS) {
          context.reportError(
            new GraphQLError(`Too many root fields. This API accepts at most ${MAX_ROOT_FIELDS}.`),
          )
        }
      },
    },
  }
}

app.all(
  '/graphql',
  createHandler({
    schema,
    // graphql-http passes this straight to graphql-js, which expects the field
    // resolvers themselves, not a { Query: ... } wrapper.
    rootValue: resolvers,
    validationRules: [...specifiedRules, limitRootFields],
  }),
)

// Video bytes, relayed from the torrent swarm. A plain GET so a <video> element can
// use it directly.
app.get('/stream/:infoHash', (req, res) => {
  const infoHash = req.params.infoHash.toLowerCase()
  // The hash is an untrusted path segment. It is only ever used as a map key, but
  // validating the shape keeps malformed requests from reaching the stream layer.
  // Only the hex form is accepted: every hash is normalised to hex when a release is
  // indexed (lib/magnet.js), so a base32 hash could never match a live session.
  if (!/^[0-9a-f]{40}$/.test(infoHash)) {
    res.status(400).json({ error: 'Invalid torrent info hash.' })
    return
  }
  serveStream(req, res, infoHash)
})

// Serves a debrid file to the browser.
//
// The debrid API key is only ever sent server-side, so it cannot reach the browser:
// the token is opaque and maps to ids held in memory. This server then fetches the
// CDN URL itself, because that URL carries the API key as its `token` parameter.
//
// The response mirrors the torrent relay: real 206 range responses so seeking works,
// which is what the CDN itself supports (verified: `Accept-Ranges: bytes`).
app.get('/dl/:token', async (req, res, next) => {
  // An untrusted path segment, so the shape is validated before it is used as a key.
  const token = req.params.token
  if (!/^[A-Za-z0-9_-]{32}$/.test(token)) {
    res.status(400).json({ error: 'Invalid download token.' })
    return
  }

  try {
    const url = await resolveDownload(token)
    // Unknown and expired tokens are the same answer, so a token cannot be probed.
    if (!url) {
      res.status(404).json({ error: 'This download link has expired. Reload the episode.' })
      return
    }

    if (!isAllowedDownloadUrl(url)) {
      console.error('[dl] refusing a download link outside DEBRID_CDN_HOSTS')
      res.status(502).json({ error: 'The download service returned an unusable link.' })
      return
    }

    // Range and conditional headers are forwarded so the viewer's player can seek and
    // resume. Everything else is dropped: this is a fetch of a known URL, not an open
    // proxy, and forwarding arbitrary headers would leak the viewer's cookies.
    const headers = {}
    for (const name of ['range', 'if-range', 'if-modified-since', 'if-none-match']) {
      if (req.headers[name]) headers[name] = req.headers[name]
    }

    const upstream = await http.get(url, {
      headers,
      responseType: 'stream',
      // No timeout on the body. axios applies its timeout to the whole response,
      // including the stream, so a fixed value would abort an episode partway
      // through. The request only has to reach the CDN, which the connect timeout
      // below bounds.
      timeout: 0,
      validateStatus: null,
    })

    // A 3xx would otherwise be followed silently into a host we did not allowlist.
    if (upstream.status >= 300 && upstream.status < 400) {
      upstream.data.destroy()
      res.status(502).json({ error: 'The download service returned a redirect.' })
      return
    }

    // An error from the CDN is its own HTML page, which would be handed to the video
    // element as a body it cannot play. Report it as an error instead.
    if (upstream.status >= 400) {
      upstream.data.destroy()
      console.warn(`[dl] the download service answered ${upstream.status}`)
      res.status(502).json({ error: 'The download service could not serve this file.' })
      return
    }

    const passthrough = ['content-length', 'content-range', 'accept-ranges', 'content-type']
    for (const name of passthrough) {
      const value = upstream.headers[name]
      if (value) res.setHeader(name, value)
    }
    // Seeking depends on this being advertised, and the CDN does not always send it.
    if (!upstream.headers['accept-ranges']) res.setHeader('Accept-Ranges', 'bytes')
    res.setHeader('Cache-Control', 'private, max-age=3600')

    res.writeHead(upstream.status)
    upstream.data.on('error', () => res.destroy())
    res.on('close', () => upstream.data.destroy())
    upstream.data.pipe(res)
  } catch (error) {
    next(error)
  }
})

// Reports what is actually reachable right now, so a broken indexer, a missing
// ffmpeg or an unconfigured stream source is visible before a user hits it.
//
// Express 4 does not catch a rejected promise from an async handler, so a failure
// here would leave the request open with no response and an unhandled rejection.
app.get('/health', async (req, res, next) => {
  try {
    const [ffmpeg, indexers, sources] = await Promise.all([
      ffmpegAvailable(),
      req.query.deep === '1' ? indexerStatus() : Promise.resolve(null),
      sourceStatus(),
    ])

    res.json({
      status: 'ok',
      metadataProvider: provider.name,
      availableMetadataProviders: providerNames(),
      configuredStreamSources: config.streamSources,
      activeStreamSources: activeSources().map((source) => source.name),
      availableStreamSources: sourceNames(),
      sources,
      // What is actually searched, as opposed to what is registered. Reporting every
      // registered indexer here would claim animetosho is in use when the default
      // configuration deliberately leaves it out.
      enabledIndexers: enabledIndexers().map((indexer) => indexer.name),
      availableIndexers: indexerNames(),
      indexers,
      ffmpeg,
      activeTorrents: sessionCount(),
      cacheBytes: await cacheSizeBytes(),
    })
  } catch (error) {
    next(error)
  }
})

// Any error that reaches here is a bug or a malformed request. Express's default
// handler renders an HTML page containing the stack trace, which leaks internals,
// so this returns a generic JSON message instead and keeps the detail in the log.
// eslint-disable-next-line no-unused-vars -- Express identifies error handlers by arity.
app.use((error, req, res, next) => {
  console.error(`[server] ${req.method} ${req.path}: ${error.message}`)
  // Once a response has started - a video already streaming, a redirect already
  // written - there is nothing to replace it with, and calling res.json() here
  // would itself throw. The connection is dropped instead.
  if (res.headersSent) {
    res.destroy()
    return
  }
  const status = error.status || error.statusCode || 500
  res.status(status).json({ error: 'The server could not process that request.' })
})

prepareCacheDir()

const server = app.listen(config.port, () => {
  console.log(`Konflix API listening on ${config.publicUrl}`)
  console.log(`  GraphQL  ${config.publicUrl}/graphql`)
  console.log(`  Health   ${config.publicUrl}/health`)
  if (config.publicUrl.includes('localhost')) {
    console.warn('  PUBLIC_URL is still localhost: stream URLs will not work for remote viewers.')
  }
  console.log(`  Metadata provider: ${provider.name}`)
  console.log(`  Stream sources:    ${activeSources().map((source) => source.name).join(', ') || 'none'}`)
  console.log(`  Torrent indexers:  ${enabledIndexers().map((indexer) => indexer.name).join(', ') || 'none'}`)
})

// Bounds how long a connection may sit idle and how many are accepted at once. Both
// defaults are generous for API traffic and only matter under load: without them a
// stalled viewer can hold a socket for the platform's own timeout (minutes), and
// enough of them exhaust the process.
server.keepAliveTimeout = 30000
server.headersTimeout = 35000
server.maxConnections = 500

// Shut down cleanly so active torrents are removed and the cache does not grow.
//
// A failure here must still exit: a shutdown that hangs leaves the process holding
// the port and its sockets, and an unhandled rejection inside a signal handler
// would stop process.exit from ever running.
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, async () => {
    server.close()
    try {
      await closeAll()
    } catch (error) {
      console.error(`[server] shutdown: ${error.message}`)
    }
    process.exit(0)
  })
}
