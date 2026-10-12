import test from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

import { parseRange, streamDirect } from '../graphql/stream/serve.js'
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
