// Serves video bytes to the browser.
//
// Two paths, chosen by what ffprobe reported:
//
//   direct  the file is already an MP4 with browser-safe codecs. Bytes are sent
//           from the downloaded pieces with HTTP range support, which is what
//           makes seeking work.
//
//   remux   the container is not playable (typically MKV). ffmpeg stream-copies
//           it into a fragmented MP4 and we pipe that to the client.
//
// Because we serve our own endpoint, the frontend needs no CORS proxy at all.
//
// One response holds exactly one reference on the torrent, so the eviction timer
// can never destroy a torrent that is still being read.

import { beginStream, endStream } from './torrent.js'
import { isDirectlyPlayable, canRemux, startRemux, pipeInto } from './media.js'

// Plain Node response helpers. Using res.writeHead/res.end rather than Express
// helpers keeps this module usable from any HTTP server, which is also what makes
// it testable without spinning up Express.
// Directly playable in a browser, by file name alone.
//
// The container decides this, and the container is visible in the extension, so this
// needs no probe: an MP4 is played as-is and an MKV is remuxed. That matters because
// the alternative - probing the file - costs a fetch of up to the probe size, which
// was measured at 1.5-3.3s per episode against the debrid CDN.
//
// Codec is deliberately not checked here. Every mainstream release is H.264, and the
// codecs are not visible in the name, so a probe would be the only way to find the
// exception. An unsupported codec (HEVC, AV1) therefore reaches the player and is
// reported by it rather than being caught here. The torrent relay makes the same
// trade-off, and its probe is a side effect of having the bytes locally anyway.
export function isDirectlyPlayableFile(name) {
  return /\.mp4$/i.test(name || '')
}

// How the bytes for a file should be sent to a browser.
//
//   "file"  the browser can play it as it is, so the bytes are relayed with range
//           support and seeking works.
//   "remux" the container is not one a browser can play (MKV), so ffmpeg stream-copies
//           it into a fragmented MP4 on the way out.
//   null    nothing can play it.
//
// The reason this exists in one place is that getting it wrong is invisible on a
// desktop and fatal on a phone. An MKV served as-is plays in Chrome and Firefox, which
// have a Matroska demuxer (Chromium only from M145), and fails on every iOS browser and
// on older Android ones - the player shows a play button and pressing it does nothing.
// The debrid source used to send every file as "file", which is exactly that bug.
export function deliveryFor(name) {
  if (isDirectlyPlayableFile(name)) return 'file'
  if (/\.mkv$/i.test(name || '')) return 'remux'
  return null
}

function sendError(res, status, message) {
  const body = JSON.stringify({ error: message })
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': String(Buffer.byteLength(body)),
  })
  res.end(body)
}

export function parseRange(header, size) {
  const match = /^bytes=(\d*)-(\d*)$/.exec(header || '')
  if (!match) return null

  const [, rawStart, rawEnd] = match
  if (rawStart === '' && rawEnd === '') return null

  if (rawStart === '') {
    const length = Number.parseInt(rawEnd, 10)
    // "bytes=-0" asks for zero bytes, which is not a satisfiable range.
    if (length === 0) return null
    return { start: Math.max(size - length, 0), end: size - 1 }
  }

  const start = Number.parseInt(rawStart, 10)
  const end = rawEnd === '' ? size - 1 : Math.min(Number.parseInt(rawEnd, 10), size - 1)
  if (start > end || start >= size) return null
  return { start, end }
}

function pipeRange(res, session, range) {
  const stream = session.file.createReadStream(range)
  stream.on('error', () => res.destroy())
  res.on('close', () => stream.destroy())
  stream.pipe(res)
}

export function streamDirect(req, res, session) {
  const size = session.file.length
  const range = parseRange(req.headers.range, size)

  // A range header we cannot satisfy must be answered with 416, never with the
  // whole file.
  if (req.headers.range && !range) {
    res.writeHead(416, {
      'Accept-Ranges': 'bytes',
      'Content-Type': 'video/mp4',
      'Content-Range': `bytes */${size}`,
    })
    res.end()
    return
  }

  if (!range) {
    res.writeHead(200, {
      'Accept-Ranges': 'bytes',
      'Content-Type': 'video/mp4',
      'Content-Length': String(size),
      // Without a cache header a repeat view is guaranteed to re-fetch from the
      // swarm. "private" keeps it out of shared caches, which must never hold one
      // viewer's video for another. Browser support for replaying a partially
      // fetched video is uneven, so this is a cheap improvement rather than a fix.
      'Cache-Control': 'private, max-age=3600',
    })
    pipeRange(res, session, {})
    return
  }

  res.writeHead(206, {
    'Accept-Ranges': 'bytes',
    'Content-Type': 'video/mp4',
    'Content-Range': `bytes ${range.start}-${range.end}/${size}`,
    'Content-Length': String(range.end - range.start + 1),
    'Cache-Control': 'private, max-age=3600',
  })
  pipeRange(res, session, range)
}

// Remuxes an MKV into a fragmented MP4 and pipes it to the response.
//
// `source` is whatever the bytes come from - the torrent's download stream, or the
// debrid CDN's response body - because the reason to remux is the container, not the
// origin. The relay and the debrid path both end up here for MKV releases.
export function streamRemux(res, source) {
  res.setHeader('Content-Type', 'video/mp4')
  // ffmpeg writes a fragmented MP4 as it goes, so there is no total length to
  // advertise and no way to seek backwards. A player must therefore start from the
  // beginning, which is what "Accept-Ranges: none" tells it.
  res.setHeader('Accept-Ranges', 'none')
  res.setHeader('Cache-Control', 'no-store')

  const ffmpeg = startRemux()

  source.on('error', () => res.destroy())
  pipeInto(source, ffmpeg.stdin)

  ffmpeg.stdout.pipe(res)
  ffmpeg.stderr.on('data', (chunk) => {
    console.warn(`[remux] ${chunk.toString().trim()}`)
  })
  ffmpeg.on('error', () => res.destroy())

  res.on('close', () => {
    source.destroy()
    ffmpeg.kill('SIGKILL')
  })
  ffmpeg.on('close', () => {
    if (!res.writableEnded) res.end()
  })
}

/**
 * Streams one episode to the response.
 *
 * The response is a `video/mp4` body that a plain <video> element can play, so no
 * player library or HLS manifest is involved.
 */
export function serveStream(req, res, infoHash) {
  const session = beginStream(infoHash)
  if (!session) {
    sendError(res, 404, 'Stream expired. Reload the episode to start it again.')
    return
  }

  // The reference is released once, when the response is finished, whatever
  // path it took.
  res.on('close', () => endStream(infoHash))

  if (isDirectlyPlayable(session.info)) {
    streamDirect(req, res, session)
    return
  }

  if (canRemux(session.info)) {
    // Bytes are pulled from the torrent as ffmpeg consumes them, so the source is the
    // download stream rather than a path on disk.
    streamRemux(res, session.file.createReadStream())
    return
  }

  sendError(res, 415, `Unsupported video codec "${session.info.videoCodec}".`)
}
