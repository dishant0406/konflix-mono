import test from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { parseProbe, isDirectlyPlayable, canRemux, startRemux } from '../graphql/stream/media.js'

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
const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'konflix-media-'))

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

// startRemux reads its input from stdin, because a torrent's bytes arrive
// incrementally and ffmpeg must pull them through the download stream rather than
// open a file that is still incomplete. These tests feed it a real file on stdin.
function remuxFile(filePath) {
  return new Promise((resolve, reject) => {
    const ffmpeg = startRemux()
    const source = fs.createReadStream(filePath)
    source.on('error', reject)
    source.pipe(ffmpeg.stdin)

    const chunks = []
    ffmpeg.stdout.on('data', (chunk) => chunks.push(chunk))
    ffmpeg.on('close', (code) => resolve({ code, output: Buffer.concat(chunks) }))
    ffmpeg.on('error', reject)
  })
}

test('startRemux produces a fragmented mp4 from an mkv', async () => {
  const { code, output } = await remuxFile(mkv)
  assert.equal(code, 0)
  assert.ok(output.length > 0, 'remux produced no bytes')

  // Confirm the output really is an mp4 by probing it.
  const remuxed = path.join(workDir, 'remuxed.mp4')
  fs.writeFileSync(remuxed, output)
  const info = await probe(remuxed)
  assert.equal(info.videoCodec, 'h264')
  assert.ok(info.container.includes('mp4'))
})

test('startRemux handles a file with no audio track', async () => {
  const { code, output } = await remuxFile(mkvNoAudio)
  assert.equal(code, 0)
  assert.ok(output.length > 0)
})

test.after(() => {
  fs.rmSync(workDir, { recursive: true, force: true })
})
