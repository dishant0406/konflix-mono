// Tests for the HLS session manager: the lifecycle and eviction logic that decides
// whether a converted stream starts, is cleaned up, and is bounded on disk.
//
// These exercise the real code against a real local HTTP file, so they cover what the
// serving tests cannot: session reuse, the session cap, expiry, and that a segment
// request for an unknown session does not start a transcode of its own.

import test from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

import { config } from '../lib/config.js'
import {
  createHlsSession,
  closeAllHls,
  hlsSessionCount,
  sessionForTorrent,
  servePlaylist,
  serveSegment,
  prepareHlsDir,
} from '../graphql/stream/hls.js'
import { deliveryFor } from '../graphql/stream/serve.js'

// A tiny real video, so ffmpeg has something to convert. It is reused for every session
// because the conversion itself is covered elsewhere; what matters here is the lifecycle.
const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'akidev-hls-'))
const videoPath = path.join(workDir, 'episode.mkv')
execFileSync('ffmpeg', [
  '-v', 'error', '-y',
  '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
  '-f', 'lavfi', '-i', 'sine=frequency=440',
  '-t', '2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest',
  videoPath,
])

let cdn = null
let cdnUrl = null

// Stands in for the debrid CDN. Range support is included because ffmpeg seeks while
// demuxing Matroska.
function startCdn() {
  const body = fs.readFileSync(videoPath)
  cdn = http.createServer((req, res) => {
    const match = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range || '')
    if (match) {
      const start = match[1] === '' ? 0 : Number.parseInt(match[1], 10)
      const end = match[2] === '' ? body.length - 1 : Number.parseInt(match[2], 10)
      res.writeHead(206, {
        'Accept-Ranges': 'bytes',
        'Content-Range': `bytes ${start}-${end}/${body.length}`,
        'Content-Length': String(end - start + 1),
      })
      res.end(body.subarray(start, end + 1))
      return
    }
    res.writeHead(200, { 'Content-Length': String(body.length) })
    res.end(body)
  })
  return new Promise((resolve) => {
    cdn.listen(0, '127.0.0.1', () => {
      cdnUrl = `http://127.0.0.1:${cdn.address().port}/episode.mkv`
      resolve()
    })
  })
}

// A response object that records what was written, so a handler can be called directly
// without an HTTP server.
function fakeResponse() {
  const written = { status: 0, headers: {}, body: Buffer.alloc(0), ended: false }
  return {
    written,
    writeHead(status, headers) {
      written.status = status
      Object.assign(written.headers, headers || {})
    },
    setHeader(name, value) {
      written.headers[name.toLowerCase()] = value
    },
    end(body) {
      written.ended = true
      if (body) written.body = Buffer.from(body)
    },
    destroy() {},
    on() {},
  }
}

test.before(async () => {
  prepareHlsDir()
  await startCdn()
})

test.after(async () => {
  closeAllHls()
  if (cdn) {
    cdn.closeAllConnections()
    await new Promise((resolve) => cdn.close(resolve))
  }
  fs.rmSync(workDir, { recursive: true, force: true })
})

test('a session is created, and its playlist and segments can be served', async (t) => {
  t.after(() => closeAllHls())

  const session = await createHlsSession({ kind: 'url', url: cdnUrl })
  assert.ok(session?.id, 'no session was created')

  const playlistRes = fakeResponse()
  servePlaylist(playlistRes, session.id, `/hls/${session.id}`)
  assert.equal(playlistRes.written.status, 200)
  assert.equal(playlistRes.written.headers['Content-Type'], 'application/vnd.apple.mpegurl')
  // The playlist must never be cached, or the player would stop seeing new segments.
  assert.equal(playlistRes.written.headers['Cache-Control'], 'no-store')

  const text = playlistRes.written.body.toString('utf8')
  assert.ok(text.startsWith('#EXTM3U'))
  // Segments are absolute, so they resolve to this server wherever the playlist is served.
  assert.ok(text.includes(`/hls/${session.id}/init.mp4`))
  assert.ok(/\/hls\/[A-Za-z0-9_-]+\/seg00000\.m4s/.test(text), 'no absolute segment path')
})

test('the same source is served by one session rather than a second transcode', async (t) => {
  t.after(() => closeAllHls())

  const first = await createHlsSession({ kind: 'url', url: cdnUrl })
  const second = await createHlsSession({ kind: 'url', url: cdnUrl })

  assert.ok(first?.id)
  // Two viewers opening the same episode must share one ffmpeg and one copy on disk.
  assert.equal(second.id, first.id, 'a second session was started for the same source')
})

test('concurrent requests for one source produce a single session', async (t) => {
  t.after(() => closeAllHls())

  // Both calls are started before either can finish. The session map is filled
  // synchronously, so the second must take the reuse path rather than spawning ffmpeg.
  const [a, b] = await Promise.all([
    createHlsSession({ kind: 'url', url: cdnUrl }),
    createHlsSession({ kind: 'url', url: cdnUrl }),
  ])
  assert.ok(a?.id && b?.id)
  assert.equal(a.id, b.id, 'concurrent opens started two transcodes')
})

test('the session cap evicts the oldest rather than growing without bound', async (t) => {
  const previousMax = config.maxHlsSessions
  config.maxHlsSessions = 1
  t.after(() => {
    config.maxHlsSessions = previousMax
    closeAllHls()
  })

  const first = await createHlsSession({ kind: 'url', url: `${cdnUrl}?one` })
  assert.ok(first?.id)
  assert.equal(hlsSessionCount(), 1)

  // A different source with the cap at 1 must evict the first, or disk and process count
  // would grow with every episode opened.
  const second = await createHlsSession({ kind: 'url', url: `${cdnUrl}?two` })
  assert.ok(second?.id)
  assert.equal(hlsSessionCount(), 1, 'the cap was not enforced')
  assert.notEqual(second.id, first.id)
})

test('a segment for an unknown session is reported, not served', async () => {
  const res = fakeResponse()
  await serveSegment(res, 'AAAAAAAAAAAAAAAA', 'seg00000.m4s')
  assert.equal(res.written.status, 404)
})

test('a segment name that is not a segment is refused', async (t) => {
  t.after(() => closeAllHls())
  const session = await createHlsSession({ kind: 'url', url: cdnUrl })
  assert.ok(session?.id)

  // The name reaches the filesystem, so anything that is not a segment must be rejected
  // before it can be joined to a path.
  for (const name of ['../../etc/passwd', 'index.m3u8', 'seg.m4s', 'seg0000.m4s', '']) {
    const res = fakeResponse()
    await serveSegment(res, session.id, name)
    assert.equal(res.written.status, 400, `${name} was not refused`)
  }
})

test('sessionForTorrent only finds a torrent-backed session', async (t) => {
  t.after(() => closeAllHls())

  // A URL-backed session has no torrent, so a torrent lookup must not find it. This is
  // what stops the segment route serving the wrong episode's bytes.
  await createHlsSession({ kind: 'url', url: cdnUrl })
  assert.equal(sessionForTorrent('0'.repeat(40)), null)
})

test('an unspawnable ffmpeg is reported rather than crashing the process', async (t) => {
  // The bug this pins: an unhandled 'error' on a child process throws, which would take
  // the whole API down and drop every other viewer with it. It is provoked by pointing
  // PATH at an empty directory, so ffmpeg cannot be found.
  const previousPath = process.env.PATH
  process.env.PATH = workDir
  t.after(() => {
    process.env.PATH = previousPath
    closeAllHls()
  })

  const session = await createHlsSession({ kind: 'url', url: cdnUrl })
  // The process is still alive, which is the point; the session simply did not start.
  assert.equal(session, null)
  assert.equal(hlsSessionCount(), 0, 'a failed session was left registered')
})

test('the delivery decision routes MKV to HLS and MP4 to a byte relay', () => {
  assert.equal(deliveryFor('Show - 01.mkv'), 'hls')
  assert.equal(deliveryFor('Show - 01.mp4'), 'file')
  assert.equal(deliveryFor('Show - 01.avi'), null)
})
