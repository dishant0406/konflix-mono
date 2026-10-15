// Serves a non-playable container (MKV) as HLS, which every browser can play.
//
// Why not a single fragmented MP4, which is what this used to send:
//
//   iOS refuses any media source that does not answer a byte-range request with a real
//   206 and a Content-Length. Its first request is `Range: bytes=0-1`, and if the reply
//   is not a range response it abandons that source - the player shows a play button and
//   pressing it does nothing. A live fragmented MP4 cannot answer that: ffmpeg writes it
//   as it goes, so there is no total length to advertise and no byte range to serve.
//   Desktop browsers do not check, which is why this played everywhere except a phone.
//
//   HLS sidesteps the problem instead of fighting it. The playlist is a small text file
//   and every segment is a complete file on disk, so each response is an ordinary 200
//   with a Content-Length. Seeking also works properly: the player can jump to any
//   segment already produced, where the old live pipe could only ever play forwards.
//
// Cost: ffmpeg runs for the whole episode rather than being torn down with the response,
// and the episode is written to disk (roughly the size of the source, since the video is
// stream-copied rather than re-encoded). Both are bounded - see config.hlsSessionTtlMs
// and config.maxHlsSessions.
//
// Nothing is pre-generated. ffmpeg starts when the first request arrives and produces
// segments while the player watches, so the first segment is ready in a few seconds and
// no disk or CPU is spent on episodes nobody opens.

import { createHash, randomBytes } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

import { config } from '../../lib/config.js'
import { cacheDir } from '../../lib/cacheDir.js'
import { pipeInto, startHlsTranscode } from './media.js'
import { beginStream, cacheSizeBytes, endStream } from './torrent.js'

const sessions = new Map()
const sessionsByKey = new Map()

const SEGMENT_PATTERN = /^seg\d{5}\.m4s$/

function sessionDir(id) {
  return path.join(cacheDir, 'hls', id)
}

// The playlist is rewritten by ffmpeg while it works, so a read can catch a partial
// line. Every consumer here only matches complete, known lines, which makes a torn
// read harmless.
function readPlaylist(id) {
  try {
    return fs.readFileSync(path.join(sessionDir(id), 'index.m3u8'), 'utf8')
  } catch {
    return null
  }
}

function listedSegments(playlist) {
  if (!playlist) return []
  return playlist
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => SEGMENT_PATTERN.test(line))
}

function isFinished(playlist) {
  return Boolean(playlist && playlist.includes('#EXT-X-ENDLIST'))
}

// Sums the segment durations ffmpeg has declared so far. This is what the player shows as
// a length before the whole episode has been read, so it is worth the parse.
function durationFrom(playlist) {
  const matches = (playlist || '').match(/#EXTINF:([\d.]+)/g)
  if (!matches) return null
  return matches.reduce((total, entry) => total + Number.parseFloat(entry.slice(8)), 0)
}

// Kills ffmpeg and deletes its output. Called on every exit path, because a session that
// is never destroyed leaves both a process and a directory behind.
function destroySession(id) {
  const session = sessions.get(id)
  if (!session) return

  sessions.delete(id)
  if (session.key) sessionsByKey.delete(session.key)
  clearTimeout(session.timer)

  session.process.kill('SIGKILL')

  // The parent's ends of the child's pipes outlive a kill, and an open pipe is an active
  // handle: it keeps the event loop alive forever. That is what stopped the test process
  // from ever exiting, and it would equally hold the API open on shutdown. stdout is not
  // piped for a transcode, hence the null checks.
  for (const stream of [session.process.stdin, session.process.stdout, session.process.stderr]) {
    if (stream) stream.destroy()
  }

  if (session.source?.destroy) session.source.destroy()
  if (session.infoHash) endStream(session.infoHash)

  // The directory is removed without awaiting: this runs from request handlers and from
  // a timer, and a failure here only leaves scratch files that startup clears anyway.
  fs.rm(sessionDir(id), { recursive: true, force: true }, () => {})
}

export function hlsSessionCount() {
  return sessions.size
}

/**
 * The live session for a torrent, if there is one.
 *
 * Used by the segment route, which must serve an existing session or report it as expired
 * rather than starting a new transcode - see serveStreamSegment for why.
 */
export function sessionForTorrent(infoHash) {
  return sessionsByKey.get(`t:${infoHash}`) || null
}

// The live session for a stream token, if there is one. Used by the segment route, which
// must serve an existing session rather than start a new transcode.
export function sessionForStream(token) {
  return sessionsByKey.get(`s:${token}`) || null
}

// A stable key for a source that is identified by a token rather than by its URL. The
// token is already a random, opaque identifier, so it can be used directly.
export function streamKey(token) {
  return `s:${token}`
}

// Polls until the predicate accepts, or the timeout passes. Used instead of fs.watch
// because ffmpeg rewrites the playlist many times per second and a watcher would wake far
// more often than the answer can change.
function waitFor(id, predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs
  return new Promise((resolve) => {
    const check = () => {
      const playlist = readPlaylist(id)
      if (playlist && predicate(playlist)) {
        resolve(playlist)
        return
      }
      // A session that was destroyed while waiting must stop polling immediately.
      if (!sessions.has(id) || Date.now() >= deadline) {
        resolve(null)
        return
      }
      setTimeout(check, 200)
    }
    check()
  })
}

function scheduleExpiry(session) {
  clearTimeout(session.timer)
  session.timer = setTimeout(() => {
    // One more grace period while a viewer is still fetching, so a paused player is not
    // killed mid-episode. Anything past this is an abandoned session.
    if (Date.now() - session.lastUsed >= config.hlsSessionTtlMs) destroySession(session.id)
    else scheduleExpiry(session)
  }, config.hlsSessionTtlMs)
}

function oldestSession() {
  let oldest = null
  for (const session of sessions.values()) {
    if (!oldest || session.lastUsed < oldest.lastUsed) oldest = session
  }
  return oldest
}

function makeRoom() {
  while (sessions.size >= config.maxHlsSessions) {
    const oldest = oldestSession()
    if (!oldest) return
    destroySession(oldest.id)
  }
}

// Bounds disk as well as session count, because the count is not a byte limit: a 1080p
// episode is roughly 1.4GB and the video is stream-copied, so three of them is over 4GB
// on top of whatever the torrent cache holds. Both live under the same cache directory,
// so the budget is measured against the whole of it.
//
// The session being created is excluded from eviction, so an episode larger than the
// whole budget still plays - it just cannot be joined by another.
async function enforceDiskBudget(keepId) {
  let total = await cacheSizeBytes()
  while (total > config.maxCacheBytes) {
    let oldest = null
    for (const session of sessions.values()) {
      if (session.id === keepId) continue
      if (!oldest || session.lastUsed < oldest.lastUsed) oldest = session
    }
    // Nothing left to reclaim. The remaining session is allowed to exceed the budget
    // rather than being destroyed, because destroying it would not free the disk anyway.
    if (!oldest) return
    destroySession(oldest.id)
    total = await cacheSizeBytes()
  }
}

/**
 * Starts (or reuses) an HLS session for a source.
 *
 * Sessions are keyed by their input, so two viewers opening the same episode share one
 * ffmpeg and one copy on disk rather than starting a second transcode.
 *
 * @param {{ kind: 'torrent', infoHash: string }
 *   | { kind: 'url', url: string }
 *   | { kind: 'stream', key: string, open: () => Promise<import('node:stream').Readable> }} source
 * @returns {Promise<{ id: string, duration: number|null } | null>} null when the source
 *   could not be turned into playable segments.
 */
export async function createHlsSession(source) {
  const key =
    source.kind === 'torrent'
      ? `t:${source.infoHash}`
      : source.kind === 'stream'
        ? source.key
        : `u:${createHash('sha1').update(source.url).digest('hex')}`

  const existing = sessionsByKey.get(key)
  if (existing) {
    // Reusing a session extends its life: it is still being watched.
    existing.lastUsed = Date.now()
    scheduleExpiry(existing)
    return { id: existing.id, duration: existing.duration }
  }

  makeRoom()

  const id = randomBytes(12).toString('base64url')
  const dir = sessionDir(id)
  fs.mkdirSync(dir, { recursive: true })

  const session = {
    id,
    key,
    dir,
    lastUsed: Date.now(),
    duration: null,
    // Set for torrent sources so the swarm stays alive for the whole episode and the
    // eviction timer cannot reclaim a torrent that is still being read.
    infoHash: source.kind === 'torrent' ? source.infoHash : null,
    source: null,
    process: null,
    timer: null,
  }

  // A torrent is read through its download stream, because the file on disk is sparse
  // until the pieces arrive. A `stream` source is the same idea for a network file: the
  // bytes are fetched by this server's HTTP client, which is the one client known to work
  // against the CDN, rather than by ffmpeg. Only a plain `url` is handed to ffmpeg.
  let torrentSession = null
  if (source.kind === 'torrent') {
    torrentSession = beginStream(source.infoHash)
    if (!torrentSession) {
      fs.rm(dir, { recursive: true, force: true }, () => {})
      return null
    }
  }

  const piped = source.kind === 'torrent' || source.kind === 'stream'
  const ffmpeg = startHlsTranscode(dir, piped ? 'pipe:0' : source.url)

  session.process = ffmpeg
  // Registered before anything is awaited, so the handlers below can always find the
  // session to destroy, and so it is excluded from its own eviction pass.
  sessions.set(id, session)
  sessionsByKey.set(key, session)

  // Attached synchronously, with no await in between. A child that cannot be spawned at
  // all (ffmpeg missing, or fd exhaustion) emits 'error' on the next tick, and an
  // unhandled 'error' on an EventEmitter throws - which here means the whole API process
  // dies and takes every other viewer with it. `await enforceDiskBudget` used to sit
  // between the spawn and these handlers, and that window is exactly where the crash
  // happened. 'close' does not fire when the spawn itself failed, so 'error' is also the
  // only thing that cleans the session up in that case. stderr is attached here too, so a
  // failure during the awaits below is still explained rather than silent.
  ffmpeg.stderr.on('data', (chunk) => {
    const message = chunk.toString().trim()
    if (message) console.warn(`[hls] ${message}`)
  })

  ffmpeg.on('error', (error) => {
    console.warn(`[hls] could not start ffmpeg: ${error.message}`)
    destroySession(id)
  })

  // A transcode that dies must not leave a session that never produces segments: every
  // request for it would wait out its timeout. Destroying it here makes the failure
  // immediate and visible.
  ffmpeg.on('close', (code) => {
    if (code !== 0 && !isFinished(readPlaylist(id))) {
      console.warn(`[hls] ffmpeg exited with code ${code}`)
      destroySession(id)
    }
  })

  if (piped) {
    // Opened lazily, so the fetch begins only once ffmpeg is listening.
    session.source =
      source.kind === 'torrent'
        ? torrentSession.file.createReadStream()
        : await source.open()

    if (!session.source) {
      console.warn('[hls] could not open the source')
      destroySession(id)
      return null
    }
    // pipeInto only guards the destination. An unhandled 'error' on the source stream
    // would take the process down the same way a spawn failure does.
    session.source.on('error', () => destroySession(id))
    pipeInto(session.source, ffmpeg.stdin)
  }

  await enforceDiskBudget(id)

  // The session may already be gone: a spawn failure emits 'error' while this await is
  // still in flight, and destroySession has run. Scheduling expiry now would attach a
  // timer to a session that is no longer tracked, so nothing would ever clear it - and a
  // pending timer is an active handle, which keeps the process alive until it fires.
  if (!sessions.has(id)) return null

  scheduleExpiry(session)

  const ready = await waitFor(id, (playlist) => Boolean(playlist), config.hlsStartTimeoutMs)
  if (!ready) {
    console.warn('[hls] no playlist was produced in time')
    destroySession(id)
    return null
  }

  session.duration = durationFrom(ready)
  return { id, duration: session.duration }
}

/** Whether a session is still alive, and whether it has finished producing segments. */
export function hlsState(id) {
  const session = sessions.get(id)
  if (!session) return null
  const playlist = readPlaylist(id)
  session.lastUsed = Date.now()
  return {
    session,
    playlist,
    finished: isFinished(playlist),
    duration: durationFrom(playlist),
  }
}

/**
 * Rewrites ffmpeg's relative segment names into absolute paths under `prefix`.
 *
 * This is not cosmetic. A player resolves relative names against the playlist's own URL,
 * so a playlist served at `/stream/<hash>` (no filename) makes `seg00000.m4s` resolve to
 * `/stream/seg00000.m4s` - a different route entirely, which answers 400. That is exactly
 * what happened: the debrid path worked because its URL ends in `index.m3u8`, and the
 * torrent path did not. Absolute paths make the playlist correct wherever it is served.
 */
export function absolutisePlaylist(playlist, prefix) {
  return playlist
    .replace(/#EXT-X-MAP:URI="init\.mp4"/, `#EXT-X-MAP:URI="${prefix}/init.mp4"`)
    .split('\n')
    .map((line) => (SEGMENT_PATTERN.test(line.trim()) ? `${prefix}/${line.trim()}` : line))
    .join('\n')
}

/**
 * The playlist for a session, or null when the session is unknown.
 *
 * `prefix` is the absolute path the playlist's own route is mounted at, e.g.
 * `/hls/<id>` or `/stream/<hash>`.
 */
export function servePlaylist(res, id, prefix) {
  const state = hlsState(id)
  if (!state?.playlist) {
    res.writeHead(404, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'This stream has expired. Reload the episode.' }))
    return
  }

  const body = Buffer.from(absolutisePlaylist(state.playlist, prefix), 'utf8')
  res.writeHead(200, {
    'Content-Type': 'application/vnd.apple.mpegurl',
    'Content-Length': String(body.length),
    // The playlist grows as ffmpeg works, so a cached copy would freeze the stream.
    'Cache-Control': 'no-store',
  })
  res.end(body)
}

// A segment is served as an ordinary complete file, which is the whole point: this is the
// response iOS was refusing before.
//
// The last segment listed in the playlist may still be being written, so it is only
// served once ffmpeg has moved past it or stopped. Serving it early would hand the player
// a truncated fragment.
export async function serveSegment(res, id, name) {
  // The name is matched against a fixed pattern before it reaches the filesystem, so it
  // cannot escape the session directory.
  if (!SEGMENT_PATTERN.test(name) && name !== 'init.mp4') {
    res.writeHead(400, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Invalid segment.' }))
    return
  }

  const state = hlsState(id)
  if (!state) {
    res.writeHead(404, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'This stream has expired. Reload the episode.' }))
    return
  }

  if (name !== 'init.mp4') {
    const listed = listedSegments(state.playlist)
    const index = listed.indexOf(name)
    const isLast = index === listed.length - 1
    if (index === -1 || (isLast && !state.finished)) {
      const ready = await waitFor(
        id,
        (playlist) => {
          const names = listedSegments(playlist)
          const position = names.indexOf(name)
          return position !== -1 && (position < names.length - 1 || isFinished(playlist))
        },
        config.hlsSegmentTimeoutMs,
      )
      if (!ready) {
        res.writeHead(404, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'That segment is not available yet.' }))
        return
      }
    }
  }

  const file = path.join(sessionDir(id), name)
  let size
  try {
    size = fs.statSync(file).size
  } catch {
    res.writeHead(404, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'That segment is not available yet.' }))
    return
  }

  res.writeHead(200, {
    'Content-Type': 'video/mp4',
    'Content-Length': String(size),
    'Cache-Control': 'private, max-age=3600',
  })
  const stream = fs.createReadStream(file)
  stream.on('error', () => res.destroy())
  res.on('close', () => stream.destroy())
  stream.pipe(res)
}

// Called at startup, like the torrent cache: HLS output is scratch data and everything in
// it can be produced again.
export function prepareHlsDir() {
  fs.rmSync(path.join(cacheDir, 'hls'), { recursive: true, force: true })
  fs.mkdirSync(path.join(cacheDir, 'hls'), { recursive: true })
}

export function closeAllHls() {
  for (const id of [...sessions.keys()]) destroySession(id)
}
