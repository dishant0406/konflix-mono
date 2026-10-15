// Tests for the torrent -> ffmpeg -> browser path.
//
// These run against a real local swarm: a webtorrent seeder and a leecher connected
// through a loopback tracker. That is the only way to exercise the parts that broke
// in practice, because they depend on webtorrent's actual read behaviour:
//
//   - a torrent downloads pieces only when a read touches them (deselect), so
//     ffmpeg must read through the torrent stream rather than a path on disk
//   - ffmpeg must be given the media on stdin for the same reason

import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
import { PassThrough } from 'node:stream'

import WebTorrent from 'webtorrent'
import { Server as TrackerServer } from 'bittorrent-tracker'

import { startHlsTranscode, probeFile } from '../graphql/stream/media.js'

function makeMkv(target) {
  execFileSync('ffmpeg', [
    '-v', 'error', '-y',
    '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
    '-f', 'lavfi', '-i', 'sine=frequency=440',
    '-t', '8', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac',
    target,
  ])
}

// A seeder and a leecher that can only see each other, so the test does not depend
// on the public DHT or on trackers being reachable.
async function localSwarm(dir, sourcePath) {
  const tracker = new TrackerServer({ udp: false, ws: false, stats: false })
  await new Promise((resolve) => tracker.listen(0, '127.0.0.1', resolve))
  const announce = `http://127.0.0.1:${tracker.http.address().port}/announce`

  const options = {
    dht: false,
    lsd: false,
    natUpnp: false,
    natPmp: false,
    utp: false,
    tracker: { announce: [announce] },
  }

  const seeder = new WebTorrent(options)
  const leecher = new WebTorrent(options)

  const torrent = await new Promise((resolve, reject) => {
    // The announce list is passed explicitly because seeding otherwise injects
    // webtorrent's built-in public trackers, which makes the test depend on
    // third-party DNS and leaves those lookups pending after the test finishes.
    const seeded = seeder.seed(sourcePath, { announce: [announce] })
    seeded.on('ready', () => resolve(seeded))
    seeded.on('error', reject)
  })

  const leeched = await new Promise((resolve, reject) => {
    const added = leecher.add(torrent.magnetURI, {
      path: path.join(dir, 'download'),
      deselect: true,
    })
    const timer = setTimeout(() => reject(new Error('metadata timeout')), 30_000)
    added.on('metadata', () => {
      clearTimeout(timer)
      resolve(added)
    })
    added.on('error', reject)
  })

  return {
    file: leeched.files[0],
    async close() {
      await new Promise((resolve) => seeder.destroy(resolve))
      await new Promise((resolve) => leecher.destroy(resolve))
      await new Promise((resolve) => tracker.close(resolve))
    },
  }
}

function runHls(file, dir) {
  return new Promise((resolve, reject) => {
    const ffmpeg = startHlsTranscode(dir, 'pipe:0')
    const source = file.createReadStream()
    source.on('error', reject)
    source.pipe(ffmpeg.stdin)

    let stderr = ''
    ffmpeg.stderr.on('data', (chunk) => {
      stderr += chunk.toString()
    })
    ffmpeg.on('error', reject)
    ffmpeg.on('close', (code) => {
      source.destroy()
      const segments = fs.readdirSync(dir).filter((name) => name.endsWith('.m4s'))
      resolve({ code, bytes: segments.length, stderr })
    })
  })
}

test('transcoding a torrent reads through the download stream', async (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'akidev-remux-'))
  const sourcePath = path.join(dir, 'Show - 01.mkv')
  makeMkv(sourcePath)

  const swarm = await localSwarm(dir, sourcePath)
  t.after(async () => {
    await swarm.close()
    fs.rmSync(dir, { recursive: true, force: true })
  })

  const hlsDir = path.join(dir, 'hls')
  fs.mkdirSync(hlsDir, { recursive: true })
  const result = await runHls(swarm.file, hlsDir)

  // A non-zero exit means ffmpeg gave up on the input. This is what happened when
  // the input was a path on disk: the bytes past the first gap had not been
  // downloaded, and ffmpeg stopped with "File ended prematurely".
  assert.equal(result.code, 0, `ffmpeg failed: ${result.stderr}`)
  assert.ok(result.bytes > 0, 'ffmpeg wrote no segments')
  assert.equal(result.stderr, '', 'ffmpeg reported an error')
})

test('a torrent is not downloaded in full just to be probed', async (t) => {
  // webtorrent selects every piece by default, so adding a torrent downloaded the
  // whole episode (measured: 542MB for one resolve) even though only the header was
  // read. The download option `deselect: true` is what keeps this bounded.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'akidev-deselect-'))

  // Noise rather than a test pattern, so the encoder cannot compress it away and a
  // full download is unmistakable.
  const sourcePath = path.join(dir, 'Show - 01.mkv')
  execFileSync('ffmpeg', [
    '-v', 'error', '-y',
    '-f', 'lavfi', '-i', 'nullsrc=size=640x480:rate=30',
    '-f', 'lavfi', '-i', 'sine=frequency=440',
    '-vf', 'noise=alls=100:allf=t',
    '-t', '4', '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-c:a', 'aac',
    sourcePath,
  ])
  const totalBytes = fs.statSync(sourcePath).size
  assert.ok(totalBytes > 4 * 1024 * 1024, `source is only ${totalBytes} bytes`)

  const swarm = await localSwarm(dir, sourcePath)
  t.after(async () => {
    await swarm.close()
    fs.rmSync(dir, { recursive: true, force: true })
  })

  // Read only the first bytes, as probing does.
  const stream = swarm.file.createReadStream({ start: 0, end: 1024 * 1024 - 1 })
  await new Promise((resolve, reject) => {
    stream.on('data', () => {})
    stream.on('end', resolve)
    stream.on('error', reject)
  })

  // Give the client a moment to continue fetching if it is going to.
  await new Promise((resolve) => setTimeout(resolve, 3000))

  assert.ok(
    swarm.file.downloaded < totalBytes * 0.5,
    `expected a partial download, got ${swarm.file.downloaded} of ${totalBytes} bytes`,
  )
})


test('probing reads through the download stream, not a path on disk', async (t) => {
  // ffprobe used to be given a path. fs-chunk-store creates a full-length file
  // immediately and fills it as pieces arrive, so the path is a file full of holes
  // and ffprobe fails with "File ended prematurely". This was the failure that made
  // a 379-seeder release unusable: fansub MKVs carry font attachments that push the
  // header past whatever prefix was fetched.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'akidev-probe-'))
  const sourcePath = path.join(dir, 'Show - 01.mkv')
  // Comfortably larger than ffprobe's probesize, so "did not download everything"
  // is a meaningful assertion rather than a statement about a tiny fixture.
  execFileSync('ffmpeg', [
    '-v', 'error', '-y',
    '-f', 'lavfi', '-i', 'nullsrc=size=640x480:rate=30',
    '-f', 'lavfi', '-i', 'sine=frequency=440',
    '-vf', 'noise=alls=80:allf=t',
    '-t', '30', '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-c:a', 'aac',
    sourcePath,
  ])

  const swarm = await localSwarm(dir, sourcePath)
  t.after(async () => {
    await swarm.close()
    fs.rmSync(dir, { recursive: true, force: true })
  })

  const info = await probeFile(swarm.file, 30_000)

  assert.equal(info.videoCodec, 'h264')
  assert.equal(info.audioCodec, 'aac')
  assert.ok(info.container.includes('matroska'), `unexpected container ${info.container}`)

  // The whole file must not have been downloaded just to identify it.
  assert.ok(
    swarm.file.length > 20 * 1024 * 1024,
    `fixture is too small to prove anything (${swarm.file.length} bytes)`,
  )
  assert.ok(
    swarm.file.downloaded < swarm.file.length * 0.9,
    `probing downloaded ${swarm.file.downloaded} of ${swarm.file.length} bytes`,
  )
})

test('probing reports a swarm that never delivers bytes', async (t) => {
  // A release with no reachable peers must fail with a clear error rather than
  // hanging the resolve until the overall deadline.
  const file = {
    name: 'Show - 01.mkv',
    length: 1024 * 1024,
    createReadStream() {
      // A stream that never produces data or ends, standing in for a swarm with no
      // reachable peers.
      return new PassThrough()
    },
  }

  await assert.rejects(() => probeFile(file, 300), /Could not read/)
})
