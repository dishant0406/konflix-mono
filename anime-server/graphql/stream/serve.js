// Serves video bytes to the browser.
//
// Two paths, chosen by what ffprobe reported about the file:
//
//   direct  the file is already an MP4 with browser-safe codecs. Bytes are sent from the
//           downloaded pieces with HTTP range support, which is what makes seeking work.
//
//   hls     the container is not playable (typically MKV). It is re-packaged as HLS:
//           fragmented MP4 segments plus a growing playlist, which every browser
//           including iOS can play and seek within.
//
// The hls path used to be a single fragmented MP4 piped live to the response. That played
// on a desktop and showed a dead play button on iOS, because iOS refuses a media source
// that cannot answer a byte-range request with a real 206 and a length. Segments are
// complete files, so each response is an ordinary 200 with a Content-Length. hls.js has
// the full reasoning.
//
// Both paths live behind one URL shape, so a player only needs the URL it was given:
//
//   /stream/<hash>              MP4 bytes, or the HLS playlist
//   /stream/<hash>/<segment>    one HLS segment
//
// Because we serve our own endpoint, the frontend needs no CORS proxy at all.

import { beginStream, endStream } from './torrent.js'
import { isDirectlyPlayable, canRemux } from './media.js'
import { createHlsSession, servePlaylist, serveSegment, sessionForTorrent } from './hls.js'

// Directly playable in a browser, by file name alone.
//
// The container decides this, and the container is visible in the extension, so this
// needs no probe: an MP4 is played as-is and an MKV is re-packaged. That matters because
// the alternative - probing the file - costs a fetch of up to the probe size, which was
// measured at 1.5-3.3s per episode against the debrid CDN.
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
//   "hls"   the container is not one a browser can play (MKV), so it is re-packaged as
//           HLS segments.
//   null    nothing can play it.
//
// The reason this exists in one place is that getting it wrong is invisible on a desktop
// and fatal on a phone. An MKV served as-is plays in Chrome and Firefox, which have a
// Matroska demuxer (Chromium only from M145), and fails on every iOS browser and on older
// Android ones - the player shows a play button and pressing it does nothing. The debrid
// source used to send every file as "file", which is exactly that bug.
export function deliveryFor(name) {
  if (isDirectlyPlayableFile(name)) return 'file'
  if (/\.mkv$/i.test(name || '')) return 'hls'
  return null
}

export function sendError(res, status, message) {
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

  // A range header we cannot satisfy must be answered with 416, never with the whole
  // file.
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
      // Without a cache header a repeat view is guaranteed to re-fetch from the swarm.
      // "private" keeps it out of shared caches, which must never hold one viewer's video
      // for another. Browser support for replaying a partially fetched video is uneven,
      // so this is a cheap improvement rather than a fix.
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

/**
 * Streams one episode to the response.
 *
 * Either a directly playable MP4 with range support, or an HLS playlist for anything
 * else. A plain <video> element plays the first; hls.js plays the second.
 */
export async function serveStream(req, res, infoHash) {
  const session = beginStream(infoHash)
  if (!session) {
    sendError(res, 404, 'Stream expired. Reload the episode to start it again.')
    return
  }

  if (isDirectlyPlayable(session.info)) {
    // The reference is held for as long as the response is being read.
    res.on('close', () => endStream(infoHash))
    streamDirect(req, res, session)
    return
  }

  // The torrent is not directly playable. This reference is dropped because the HLS
  // session below takes its own for the whole transcode, which outlives this response.
  endStream(infoHash)

  if (!canRemux(session.info)) {
    sendError(res, 415, `Unsupported video codec "${session.info.videoCodec}".`)
    return
  }

  const hls = await createHlsSession({ kind: 'torrent', infoHash })
  if (!hls) {
    sendError(res, 502, 'This episode could not be prepared for playback.')
    return
  }
  servePlaylist(res, hls.id, `/stream/${infoHash}`)
}

/**
 * One HLS segment (or the init file) for a torrent-backed session.
 *
 * The session is looked up by the torrent it was built from rather than started here. A
 * segment request must never create a session: if the session had expired, doing so would
 * start a fresh transcode from the beginning of the episode, take a torrent reference for
 * the whole session lifetime, write a full episode to disk, and then time out waiting for
 * a segment the new playlist will not reach for minutes. Worse, that new session could
 * evict a session another viewer is watching, which would then do the same thing.
 *
 * So an expired session is reported as expired, and the player reloads the episode, which
 * re-resolves the stream and starts a session deliberately.
 */
export async function serveStreamSegment(res, infoHash, name) {
  const session = sessionForTorrent(infoHash)
  if (!session) {
    sendError(res, 404, 'Stream expired. Reload the episode to start it again.')
    return
  }
  await serveSegment(res, session.id, name)
}
