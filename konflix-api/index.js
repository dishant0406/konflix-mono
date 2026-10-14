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
import { providersFor, providerNames, providerTypes } from './graphql/providers/index.js'
import { enabledIndexers, indexerNames, indexerStatus, indexerTypes } from './graphql/indexers/index.js'
import { serveStream, serveStreamSegment } from './graphql/stream/serve.js'
import { servePlaylist, serveSegment, prepareHlsDir, closeAllHls, hlsSessionCount, sessionForStream, streamKey, createHlsSession } from './graphql/stream/hls.js'
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
app.get('/stream/:infoHash/:segment?', async (req, res, next) => {
  const infoHash = req.params.infoHash.toLowerCase()
  // The hash is an untrusted path segment. It is only ever used as a map key, but
  // validating the shape keeps malformed requests from reaching the stream layer.
  // Only the hex form is accepted: every hash is normalised to hex when a release is
  // indexed (lib/magnet.js), so a base32 hash could never match a live session.
  if (!/^[0-9a-f]{40}$/.test(infoHash)) {
    res.status(400).json({ error: 'Invalid torrent info hash.' })
    return
  }
  try {
    // The bare path answers with MP4 bytes or the HLS playlist; a trailing name is one
    // HLS segment. Express 4 does not catch a rejected promise, so this is awaited
    // inside a try rather than left to the framework.
    if (req.params.segment) await serveStreamSegment(res, infoHash, req.params.segment)
    else await serveStream(req, res, infoHash)
  } catch (error) {
    next(error)
  }
})

// Serves the HLS output for a session, which is how a non-playable container (MKV) is
// delivered to the browser. The playlist lists segments with relative names, so the
// player resolves them against this same path.
//
// A session id is an opaque token minted when the episode was resolved. It is validated
// by shape before it is used as a directory name.
app.get('/hls/:id/:file', async (req, res, next) => {
  const { id, file } = req.params
  if (!/^[A-Za-z0-9_-]{16}$/.test(id)) {
    res.status(400).json({ error: 'Invalid stream id.' })
    return
  }
  try {
    // A segment that has not been written yet is awaited, so this can reject.
    if (file === 'index.m3u8') servePlaylist(res, id, `/hls/${id}`)
    else await serveSegment(res, id, file)
  } catch (error) {
    next(error)
  }
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
    const entry = await resolveDownload(token)
    // Unknown and expired tokens are the same answer, so a token cannot be probed.
    if (!entry) {
      res.status(404).json({ error: 'This download link has expired. Reload the episode.' })
      return
    }

    if (entry.delivery === 'hls') {
      const hls = await createHlsSession({
        kind: 'stream',
        key: streamKey(token),
        // The bytes are fetched by this server's HTTP client and piped to ffmpeg, rather
        // than letting ffmpeg fetch the URL itself. The client is the one known to work
        // against this CDN, and it is the same mechanism the torrent path uses.
        open: async () => {
          const upstream = await fetchDownload(entry.url, {})
          if (typeof upstream === 'number') {
            console.warn(`[hls] the download service answered ${upstream}`)
            return null
          }
          return upstream.data
        },
      })
      if (!hls) {
        res.status(502).json({ error: 'This episode could not be prepared for playback.' })
        return
      }
      servePlaylist(res, hls.id, `/dl/${token}`)
      return
    }

    if (!isAllowedDownloadUrl(entry.url)) {
      // The hostname is logged because the provider rotates its CDN domain, and this
      // is the only place the new one becomes visible. The URL itself is not logged:
      // it carries the API key in its query string.
      let host = 'unparseable'
      try {
        host = new URL(entry.url).hostname
      } catch {
        // Leave the placeholder; the link was malformed, which the check already
        // treats as a refusal.
      }
      console.error(
        `[dl] refused a link outside DEBRID_CDN_HOSTS: host=${host}. ` +
          'Set DEBRID_CDN_HOSTS to this domain to allow it.',
      )
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

    const upstream = await fetchDownload(entry.url, headers)
    if (typeof upstream === 'number') {
      console.warn(`[dl] the download service answered ${upstream}`)
      res.status(502).json({ error: 'The download service could not serve this file.' })
      return
    }

    const passthrough = ['content-length', 'content-range', 'accept-ranges']
    for (const name of passthrough) {
      const value = upstream.headers[name]
      if (value) res.setHeader(name, value)
    }
    // Seeking depends on this being advertised, and the CDN does not always send it.
    if (!upstream.headers['accept-ranges']) res.setHeader('Accept-Ranges', 'bytes')
    // A relayed stream keeps the CDN's own type,
    // which is not reliable (`application/octet-stream` is common). A video type is
    // therefore sent whenever the CDN does not name one. Media elements treat
    // octet-stream as "no type given" and sniff the container, so this is for players
    // and tooling that trust the header rather than a fix for playback.
    const upstreamType = upstream.headers['content-type']
    res.setHeader(
      'Content-Type',
      upstreamType && !upstreamType.includes('octet-stream') ? upstreamType : 'video/mp4',
    )
    res.setHeader('Cache-Control', 'private, max-age=3600')

    res.writeHead(upstream.status)
    upstream.data.on('error', () => res.destroy())
    res.on('close', () => upstream.data.destroy())
    upstream.data.pipe(res)
  } catch (error) {
    next(error)
  }
})

// One HLS segment (or the init file) for a debrid-backed stream. A segment request must
// never start a session of its own: if the session had expired, doing so would begin a
// fresh transcode from the start of the episode and then time out waiting for a segment
// the new playlist will not reach for minutes.
app.get('/dl/:token/:segment', async (req, res, next) => {
  const { segment } = req.params
  // An untrusted path segment, so the shape is validated before it is used as a key.
  const token = req.params.token
  if (!/^[A-Za-z0-9_-]{32}$/.test(token)) {
    res.status(400).json({ error: 'Invalid download token.' })
    return
  }
  try {
    const entry = await resolveDownload(token)
    if (!entry || entry.delivery !== 'hls') {
      res.status(404).json({ error: 'This stream has expired. Reload the episode.' })
      return
    }
    const session = sessionForStream(token)
    if (!session) {
      res.status(404).json({ error: 'This stream has expired. Reload the episode.' })
      return
    }
    await serveSegment(res, session.id, segment)
  } catch (error) {
    next(error)
  }
})

// Fetches an allowlisted CDN link and returns its response, or the status code when
// the CDN refused. Returning a number rather than throwing keeps the "the service
// said no" case separate from "the request failed", which are reported differently.
//
// A 3xx is refused rather than followed, because following one would leave the
// allowlist behind.
async function fetchDownload(url, headers) {
  const upstream = await http.get(url, {
    headers,
    responseType: 'stream',
    // No timeout on the body. axios applies its timeout to the whole response,
    // including the stream, so a fixed value would abort an episode partway through.
    // The request only has to reach the CDN, which the connect timeout bounds.
    timeout: 0,
    validateStatus: null,
    maxRedirects: 0,
  })

  // A redirect, or an error page from the CDN, is not a body a video element can play.
  if (upstream.status >= 300) {
    upstream.data.destroy()
    return upstream.status
  }

  return upstream
}

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
      // The provider chain per content type, so a misconfigured site is visible here
      // rather than at the first request for a film. The first name answers unless it
      // has nothing, in which case the next one does.
      metadataProviders: Object.fromEntries(
        ['anime', 'series', 'movie'].map((type) => [type, providersFor(type).map((p) => p.name)]),
      ),
      availableMetadataProviders: providerNames(),
      providerTypes: providerTypes(),
      configuredStreamSources: config.streamSources,
      activeStreamSources: activeSources().map((source) => source.name),
      availableStreamSources: sourceNames(),
      sources,
      // What is actually searched, as opposed to what is registered. Reporting every
      // registered indexer here would claim animetosho is in use when the default
      // configuration deliberately leaves it out.
      enabledIndexers: enabledIndexers().map((indexer) => indexer.name),
      availableIndexers: indexerNames(),
      indexerTypes: indexerTypes(),
      indexers,
      ffmpeg,
      activeTorrents: sessionCount(),
      activeHlsSessions: hlsSessionCount(),
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
prepareHlsDir()

const server = app.listen(config.port, () => {
  console.log(`Konflix API listening on ${config.publicUrl}`)
  console.log(`  GraphQL  ${config.publicUrl}/graphql`)
  console.log(`  Health   ${config.publicUrl}/health`)
  if (config.publicUrl.includes('localhost')) {
    console.warn('  PUBLIC_URL is still localhost: stream URLs will not work for remote viewers.')
  }
  console.log(
    `  Metadata providers: ${['anime', 'series', 'movie']
      .map((type) => `${type}=${providersFor(type).map((p) => p.name).join('+')}`)
      .join(', ')}`,
  )
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
      closeAllHls()
      await closeAll()
    } catch (error) {
      console.error(`[server] shutdown: ${error.message}`)
    }
    process.exit(0)
  })
}
