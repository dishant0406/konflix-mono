import test from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
import { Readable } from 'node:stream'

import { parseRange, streamDirect, deliveryFor } from '../graphql/stream/serve.js'
import { startHlsTranscode } from '../graphql/stream/media.js'
import { absolutisePlaylist } from '../graphql/stream/hls.js'
import { pickVideoFile, overLimits } from '../graphql/stream/torrent.js'

test('parseRange handles the range forms a browser sends', () => {
  assert.deepEqual(parseRange('bytes=0-1023', 5000), { start: 0, end: 1023 })
  assert.deepEqual(parseRange('bytes=1000-', 5000), { start: 1000, end: 4999 })
  assert.deepEqual(parseRange('bytes=-500', 5000), { start: 4500, end: 4999 })
  assert.deepEqual(parseRange('bytes=0-99999', 5000), { start: 0, end: 4999 })
})

test('parseRange rejects ranges it cannot satisfy', () => {
  assert.equal(parseRange(undefined, 5000), null)
  assert.equal(parseRange('bytes=99999-', 5000), null)
  assert.equal(parseRange('bytes=abc', 5000), null)
  assert.equal(parseRange('items=0-10', 5000), null)
})

test('pickVideoFile chooses the file matching the episode', () => {
  const files = [
    { name: 'Show - 01.mkv', length: 300 * 1024 * 1024 },
    { name: 'Show - 02.mkv', length: 300 * 1024 * 1024 },
  ]
  assert.equal(pickVideoFile(files, 2).name, 'Show - 02.mkv')
})

test('pickVideoFile accepts a single-file torrent that names no episode', () => {
  const files = [{ name: 'Show S01E05 1080p.mkv', length: 300 * 1024 * 1024 }]
  assert.equal(pickVideoFile(files, 5).name, 'Show S01E05 1080p.mkv')
})

test('pickVideoFile refuses a single file that names a different episode', () => {
  // Holding exactly one video file proves nothing about which episode it is, so a
  // file naming a different episode must be rejected rather than served.
  const single = [{ name: 'Show - 05.mkv', length: 300 * 1024 * 1024 }]
  assert.equal(pickVideoFile(single, 12), null)
  assert.equal(pickVideoFile(single, 5).name, 'Show - 05.mkv')
  // A single file that names no episode is still usable.
  const unnamed = [{ name: 'Show.mkv', length: 300 * 1024 * 1024 }]
  assert.equal(pickVideoFile(unnamed, 12).name, 'Show.mkv')
})

test('pickVideoFile refuses a batch that does not name the episode', () => {
  const files = [
    { name: 'Show - 01.mkv', length: 300 * 1024 * 1024 },
    { name: 'Show - 02.mkv', length: 300 * 1024 * 1024 },
  ]
  assert.equal(pickVideoFile(files, 7), null)
})

test('pickVideoFile ignores samples and non-video files', () => {
  const files = [
    { name: 'Show - 01.mkv', length: 300 * 1024 * 1024 },
    { name: 'Show - 01-sample.mkv', length: 1024 },
    { name: 'readme.txt', length: 10 },
  ]
  assert.equal(pickVideoFile(files, 1).name, 'Show - 01.mkv')
})

// A stand-in for a webtorrent file: the same `length` and
// `createReadStream({ start, end })` contract with inclusive bounds, backed by a
// local file. That contract was verified directly against webtorrent's own
// implementation, so these HTTP tests stay hermetic and fast.
function fileFromDisk(filePath) {
  return {
    name: path.basename(filePath),
    length: fs.statSync(filePath).size,
    createReadStream(range = {}) {
      return fs.createReadStream(filePath, range)
    },
  }
}

test('streamDirect serves full and partial content over real HTTP', async (t) => {
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'konflix-serve-'))
  const videoPath = path.join(workDir, 'episode.mp4')
  execFileSync('ffmpeg', [
    '-v', 'error', '-y',
    '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
    '-f', 'lavfi', '-i', 'sine=frequency=440',
    '-t', '3', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest',
    videoPath,
  ])

  const file = fileFromDisk(videoPath)
  const session = {
    file,
    info: { container: 'mov,mp4', videoCodec: 'h264', audioCodec: 'aac' },
  }

  const server = http.createServer((req, res) => streamDirect(req, res, session))
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const base = `http://127.0.0.1:${server.address().port}`

  t.after(async () => {
    server.closeAllConnections()
    await new Promise((resolve) => server.close(resolve))
    fs.rmSync(workDir, { recursive: true, force: true })
  })

  // Full request
  const full = await fetch(base, { headers: { Connection: 'close' } })
  assert.equal(full.status, 200)
  assert.equal(full.headers.get('accept-ranges'), 'bytes')
  assert.equal(full.headers.get('content-type'), 'video/mp4')
  assert.equal(Number(full.headers.get('content-length')), file.length)
  const fullBody = Buffer.from(await full.arrayBuffer())
  assert.equal(fullBody.length, file.length)
  assert.equal(fullBody.slice(4, 8).toString(), 'ftyp')

  // Range request: this is what makes seeking work.
  const ranged = await fetch(base, {
    headers: { Range: 'bytes=100-199', Connection: 'close' },
  })
  assert.equal(ranged.status, 206)
  assert.equal(ranged.headers.get('content-range'), `bytes 100-199/${file.length}`)
  assert.equal(Number(ranged.headers.get('content-length')), 100)
  const rangedBody = Buffer.from(await ranged.arrayBuffer())
  assert.equal(rangedBody.length, 100)
  assert.ok(rangedBody.equals(fullBody.slice(100, 200)), 'range bytes must match the full body')

  // Open-ended range
  const tail = await fetch(base, {
    headers: { Range: `bytes=${file.length - 50}-`, Connection: 'close' },
  })
  assert.equal(tail.status, 206)
  assert.equal(Number(tail.headers.get('content-length')), 50)

  // An unsatisfiable range must be a 416, never a silent full-file response.
  const bad = await fetch(base, {
    headers: { Range: `bytes=${file.length + 10}-`, Connection: 'close' },
  })
  assert.equal(bad.status, 416)
  assert.equal(bad.headers.get('content-range'), `bytes */${file.length}`)
})

test('overLimits bounds disk as well as torrent count', () => {
  const entry = (bytes) => ({ file: { length: bytes } })
  const limits = { maxTorrents: 2, maxCacheBytes: 1000 }

  assert.equal(overLimits([], limits), false)
  // Within both limits.
  assert.equal(overLimits([entry(100)], limits), false)
  // Over the torrent count.
  assert.equal(overLimits([entry(1), entry(1)], limits), true)
  // Under the torrent count but over the byte budget: counting torrents alone does
  // not bound disk, because one episode can be larger than the whole budget.
  assert.equal(overLimits([entry(1000)], limits), true)
})

// The debrid path serves a remote file, so its transcode input is a CDN URL rather
// than a torrent's download stream. That is the difference between the two paths, and
// it is the one that was broken twice: an MKV was first relayed as-is, which plays on a
// desktop and shows a dead play button on a phone, and then piped as a live fragmented
// MP4, which iOS refuses because it cannot answer a byte-range request.
//
// This runs the whole thing for real - a file on disk, served over HTTP, turned into HLS
// by the same function production calls - and then asserts the two properties iOS
// actually checks.
test('an mkv from a URL becomes HLS that a mobile browser can fetch', async (t) => {
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'konflix-serve-hls-'))
  const mkvPath = path.join(workDir, 'episode.mkv')
  const hlsDir = path.join(workDir, 'out')

  execFileSync('ffmpeg', [
    '-v', 'error', '-y',
    '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
    '-f', 'lavfi', '-i', 'sine=frequency=440',
    '-t', '4', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest',
    mkvPath,
  ])

  // Stands in for the CDN.
  const cdn = http.createServer((req, res) => {
    const body = fs.readFileSync(mkvPath)
    res.writeHead(200, { 'Content-Type': 'application/octet-stream' })
    res.end(body)
  })
  await new Promise((resolve) => cdn.listen(0, '127.0.0.1', resolve))

  const url = `http://127.0.0.1:${cdn.address().port}/episode.mkv`
  fs.mkdirSync(hlsDir, { recursive: true })

  const { code, stderr } = await new Promise((resolve, reject) => {
    const ffmpeg = startHlsTranscode(hlsDir, url)
    let err = ''
    ffmpeg.stderr.on('data', (chunk) => {
      err += chunk.toString()
    })
    ffmpeg.on('close', (code) => resolve({ code, stderr: err }))
    ffmpeg.on('error', reject)
  })

  t.after(async () => {
    cdn.closeAllConnections()
    await new Promise((resolve) => cdn.close(resolve))
    fs.rmSync(workDir, { recursive: true, force: true })
  })

  assert.equal(code, 0, `ffmpeg failed: ${stderr}`)

  const playlist = fs.readFileSync(path.join(hlsDir, 'index.m3u8'), 'utf8')
  const segments = fs.readdirSync(hlsDir).filter((name) => name.endsWith('.m4s')).sort()
  assert.ok(segments.length > 0, 'no segments were written')

  // The property iOS checks first, and the reason the live pipe could never work: every
  // segment is a complete file that can be served with a real length.
  const segment = fs.statSync(path.join(hlsDir, segments[0]))
  assert.ok(segment.size > 0, 'the first segment is empty')

  // And the codecs inside it are ones a phone can play. The playlist is probed rather
  // than a bare segment: an fMP4 fragment has no moov of its own and is only parseable
  // through the init segment the playlist declares. This also proves the playlist is
  // well-formed, which is the first thing the browser fetches.
  const probed = JSON.parse(
    execFileSync('ffprobe', [
      '-v', 'error',
      '-show_entries', 'format=format_name:stream=codec_type,codec_name',
      '-of', 'json',
      path.join(hlsDir, 'index.m3u8'),
    ]).toString(),
  )
  const video = probed.streams.find((stream) => stream.codec_type === 'video')
  assert.equal(video.codec_name, 'h264')
})

// The delivery decision is what routes an MKV away from the raw byte relay. Getting it
// wrong is invisible on a desktop and fatal on a phone, so it is pinned here.
test('an mkv is routed to HLS rather than relayed as bytes', () => {
  assert.equal(deliveryFor('Show - 01.mkv'), 'hls')
  assert.equal(deliveryFor('Show - 01.MKV'), 'hls')
  assert.equal(deliveryFor('Show - 01.mp4'), 'file')
  assert.equal(deliveryFor('Show - 01.avi'), null)
})

// A playlist served at a URL with no file name must still resolve its segments.
//
// This is the bug that made the torrent path fail while the debrid path worked: hls.js
// resolves relative segment names against the playlist's own URL, so `/stream/<hash>`
// turned `seg00000.m4s` into `/stream/seg00000.m4s` - a different route, which rejects it
// with 400. The debrid path was unaffected because its URL ends in `index.m3u8`. Nothing
// about that difference is obvious, which is why it is pinned here.
test('a playlist makes its segments absolute, wherever it is served', () => {
  const playlist = [
    '#EXTM3U',
    '#EXT-X-VERSION:7',
    '#EXT-X-MAP:URI="init.mp4"',
    '#EXTINF:6.006000,',
    'seg00000.m4s',
    '#EXTINF:6.006000,',
    'seg00001.m4s',
    '#EXT-X-ENDLIST',
    '',
  ].join('\n')

  const rewritten = absolutisePlaylist(playlist, '/stream/abc123')

  // The init and every segment must be reachable from the playlist's own URL, so that a
  // player cannot resolve one to a different route.
  assert.ok(rewritten.includes('#EXT-X-MAP:URI="/stream/abc123/init.mp4"'))
  assert.ok(rewritten.includes('/stream/abc123/seg00000.m4s'))
  assert.ok(rewritten.includes('/stream/abc123/seg00001.m4s'))
  // No relative segment name may survive, which is the actual failure mode.
  assert.ok(!/^seg\d+\.m4s$/m.test(rewritten), 'a relative segment name survived')
  // Everything else must be passed through untouched.
  assert.ok(rewritten.includes('#EXT-X-ENDLIST'))
  assert.ok(rewritten.startsWith('#EXTM3U'))
})

test('absolutising a playlist does not mangle the rest of it', () => {
  const playlist = '#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-PLAYLIST-TYPE:EVENT\n#EXTINF:6.006000,\nseg00000.m4s\n'
  const rewritten = absolutisePlaylist(playlist, '/hls/xyz')
  const lines = rewritten.split('\n')
  assert.equal(lines[0], '#EXTM3U')
  assert.equal(lines[1], '#EXT-X-TARGETDURATION:6')
  assert.equal(lines[2], '#EXT-X-PLAYLIST-TYPE:EVENT')
  assert.equal(lines[3], '#EXTINF:6.006000,')
  assert.equal(lines[4], '/hls/xyz/seg00000.m4s')
})
