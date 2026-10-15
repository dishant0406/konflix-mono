import test from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { parseProbe, isDirectlyPlayable, canRemux, startHlsTranscode } from '../graphql/stream/media.js'

// Production code always probes through a download stream, so there is no
// path-based probe to import. These tests need to inspect finished files on disk
// (a generated fixture, a remuxed output), so they run ffprobe directly and share
// the production parser.
async function probe(filePath) {
  const stdout = execFileSync('ffprobe', [
    '-v', 'error',
    '-show_entries', 'format=format_name,duration:stream=codec_type,codec_name',
    '-of', 'json',
    filePath,
  ])
  return parseProbe(stdout.toString())
}

// These tests exercise real ffmpeg/ffprobe against generated files. They are the
// part of the pipeline that cannot be checked by reading code.
const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'akidev-media-'))

function makeFile(name, args) {
  const target = path.join(workDir, name)
  execFileSync('ffmpeg', ['-v', 'error', '-y', ...args, target], { stdio: 'pipe' })
  return target
}

const mp4 = makeFile('h264-aac.mp4', [
  '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
  '-f', 'lavfi', '-i', 'sine=frequency=440',
  '-t', '2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest',
])

const mkv = makeFile('h264-aac.mkv', [
  '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
  '-f', 'lavfi', '-i', 'sine=frequency=440',
  '-t', '2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest',
])

const mkvNoAudio = makeFile('h264-noaudio.mkv', [
  '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=10',
  '-t', '2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p',
])

test('probe reads container, codecs and duration', async () => {
  const info = await probe(mp4)
  assert.equal(info.videoCodec, 'h264')
  assert.equal(info.audioCodec, 'aac')
  assert.ok(info.container.includes('mp4'))
  assert.ok(info.durationSeconds > 1)
})

test('probe reports matroska for an mkv', async () => {
  const info = await probe(mkv)
  assert.ok(info.container.includes('matroska'))
})

test('an h264/aac mp4 is served directly', async () => {
  assert.equal(isDirectlyPlayable(await probe(mp4)), true)
})

test('an mkv is not served directly but can be remuxed', async () => {
  const info = await probe(mkv)
  assert.equal(isDirectlyPlayable(info), false)
  assert.equal(canRemux(info), true)
})

test('hevc cannot be remuxed and must be rejected', () => {
  assert.equal(canRemux({ videoCodec: 'hevc' }), false)
  assert.equal(canRemux({ videoCodec: 'av1' }), false)
  assert.equal(canRemux({ videoCodec: 'h264' }), true)
})

// startHlsTranscode reads its input from stdin when the source is a torrent, because a
// torrent's bytes arrive incrementally and ffmpeg must pull them through the download
// stream rather than open a file that is still incomplete. These tests feed it a real
// file on stdin, exactly as the relay does.
function transcodeToHls(filePath, dir) {
  return new Promise((resolve, reject) => {
    const ffmpeg = startHlsTranscode(dir, 'pipe:0')
    const source = fs.createReadStream(filePath)
    source.on('error', reject)
    source.pipe(ffmpeg.stdin)
    let stderr = ''
    ffmpeg.stderr.on('data', (chunk) => {
      stderr += chunk.toString()
    })
    ffmpeg.on('close', (code) => {
      source.destroy()
      resolve({ code, stderr })
    })
    ffmpeg.on('error', reject)
  })
}

function hlsOutput(dir) {
  const playlist = fs.readFileSync(path.join(dir, 'index.m3u8'), 'utf8')
  const segments = fs
    .readdirSync(dir)
    .filter((name) => name.endsWith('.m4s'))
    .sort()
  return { playlist, segments, dir }
}

test('startHlsTranscode turns an mkv into a playable HLS stream', async () => {
  const dir = path.join(workDir, 'hls-mkv')
  fs.mkdirSync(dir, { recursive: true })

  const { code, stderr } = await transcodeToHls(mkv, dir)
  assert.equal(code, 0, `ffmpeg failed: ${stderr}`)

  const { playlist, segments } = hlsOutput(dir)
  // A playlist the player can act on, and at least one real segment to fetch.
  assert.ok(playlist.startsWith('#EXTM3U'), 'not a playlist')
  assert.ok(playlist.includes('#EXT-X-MAP:URI="init.mp4"'), 'no init segment declared')
  assert.ok(playlist.includes('#EXT-X-ENDLIST'), 'a finished stream must be marked so')
  assert.ok(segments.length > 0, 'no segments were written')

  // The only check that matters: the stream is a container and codecs a phone can play.
  // The playlist is probed rather than a bare segment, because an fMP4 fragment has no
  // moov and is not independently parseable - it is only meaningful through the init
  // segment the playlist declares. Probing the playlist also proves it is well-formed,
  // which is what the browser actually fetches first.
  const info = await probe(path.join(dir, 'index.m3u8'))
  assert.equal(info.videoCodec, 'h264')
  assert.equal(info.audioCodec, 'aac')
})

test('startHlsTranscode handles a file with no audio track', async () => {
  const dir = path.join(workDir, 'hls-noaudio')
  fs.mkdirSync(dir, { recursive: true })

  const { code, stderr } = await transcodeToHls(mkvNoAudio, dir)
  assert.equal(code, 0, `ffmpeg failed: ${stderr}`)
  assert.ok(hlsOutput(dir).segments.length > 0)
})

// The playlist must describe the whole episode, not just the first fragment, or the
// player has nothing to seek within. `event` makes it append-only; the alternative
// would be a sliding window that drops what has already been played.
test('the playlist declares a duration the player can seek within', async () => {
  const dir = path.join(workDir, 'hls-duration')
  fs.mkdirSync(dir, { recursive: true })

  const { code } = await transcodeToHls(mkv, dir)
  assert.equal(code, 0)

  const { playlist } = hlsOutput(dir)
  const total = (playlist.match(/#EXTINF:([\d.]+)/g) || []).reduce(
    (sum, entry) => sum + Number.parseFloat(entry.slice(8)),
    0,
  )
  assert.ok(total > 1, `playlist declares ${total}s, expected the episode length`)
})

test.after(() => {
  fs.rmSync(workDir, { recursive: true, force: true })
})
