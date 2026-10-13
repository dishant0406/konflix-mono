// ffprobe/ffmpeg wrappers.
//
// Both read their input from stdin, never from a path on disk. A torrent is only
// downloaded piece by piece: fs-chunk-store creates a full-length file immediately
// and fills it in as pieces arrive, so a path handed to ffprobe or ffmpeg is a file
// full of holes. That produced two distinct failures in practice:
//
//   - ffprobe reported "File ended prematurely" on any release whose header ran
//     past the bytes fetched, which is common for fansub MKVs carrying font
//     attachments (measured: a 12-attachment MKV needs more than 4MB).
//   - ffmpeg stopped at the first gap in the middle of a remux.
//
// Reading through the torrent's stream instead makes webtorrent fetch each piece as
// the tool consumes it.

import { execFile, spawn } from 'node:child_process'

import { config } from '../../lib/config.js'

// Enough header for any real release, while still bounded: a header is read once per
// candidate, and an unbounded read would download an entire episode to probe it.
const PROBE_SIZE_BYTES = 20 * 1024 * 1024

// Reads only what is needed to identify the streams, from stdin.
const PROBE_ARGS = [
  '-v', 'error',
  '-show_entries', 'format=format_name,duration:stream=codec_type,codec_name',
  '-of', 'json',
  // Bounded so ffprobe cannot download most of a file just to identify it.
  '-probesize', String(PROBE_SIZE_BYTES),
  '-i', 'pipe:0',
]

// Streams the torrent's bytes into a child process's stdin.
//
// `pipe()` only attaches an error handler to the source, not the destination. When
// the child exits first - which it does as soon as ffprobe has identified the
// streams, or when it is killed on timeout - the next write to its stdin raises
// EPIPE on a stream nobody is listening to, and an uncaught error like that takes
// the whole server down. The destination is therefore handled explicitly.
//
// The source is a webtorrent file stream, which is built on `streamx` and has no
// `unpipe` method. Calling it here threw `TypeError: source.unpipe is not a
// function` inside the error handler, which is itself uncaught, so the crash the
// handler exists to prevent happened anyway. Destroying the source is what actually
// stops the writes.
export function pipeInto(source, destination) {
  destination.on('error', () => {
    if (typeof source.unpipe === 'function') source.unpipe(destination)
    source.destroy()
  })
  source.pipe(destination)
}

function collect(process) {  return new Promise((resolve, reject) => {
    let stdout = ''
    let stderr = ''
    process.stdout.on('data', (chunk) => {
      stdout += chunk
    })
    process.stderr.on('data', (chunk) => {
      stderr += chunk
    })
    process.on('error', reject)
    process.on('close', (code) => {
      if (code !== 0) reject(new Error(stderr.trim() || `ffprobe exited with code ${code}`))
      else resolve(stdout)
    })
  })
}

// Runs ffprobe against a torrent file, reading it through the download stream.
export async function probeFile(file, timeoutMs = config.probeTimeoutMs) {
  const ffprobe = spawn('ffprobe', PROBE_ARGS, { stdio: ['pipe', 'pipe', 'pipe'] })

  const source = file.createReadStream()
  source.on('error', () => ffprobe.kill('SIGKILL'))
  pipeInto(source, ffprobe.stdin)

  // A swarm with no reachable peers would otherwise wait forever.
  const timer = setTimeout(() => {
    source.destroy()
    ffprobe.kill('SIGKILL')
  }, timeoutMs)

  try {
    const stdout = await collect(ffprobe)
    return parseProbe(stdout)
  } catch (error) {
    throw new Error(`Could not read "${file.name}": ${error.message}`)
  } finally {
    clearTimeout(timer)
    source.destroy()
  }
}

export function parseProbe(stdout) {
  const parsed = JSON.parse(stdout)
  const streams = parsed.streams || []
  const video = streams.find((stream) => stream.codec_type === 'video')
  const audio = streams.find((stream) => stream.codec_type === 'audio')

  return {
    container: parsed.format?.format_name || null,
    durationSeconds: Number.parseFloat(parsed.format?.duration) || null,
    videoCodec: video?.codec_name || null,
    audioCodec: audio?.codec_name || null,
  }
}

// A browser can play the file directly only if the container is MP4 and both
// codecs are ones every browser ships. Everything else is remuxed or rejected.
export function isDirectlyPlayable(info) {
  const containers = (info.container || '').split(',')
  if (!containers.includes('mp4')) return false
  if (info.videoCodec !== 'h264') return false
  return ['aac', 'mp3', null].includes(info.audioCodec)
}

// The container a file really is, read from its first bytes.
//
// The file name cannot be trusted for this. A real release was measured naming its file
// `.mp4` while the bytes were Matroska, which is precisely the case that plays on a
// desktop and fails on a phone. The container signature is in the first few bytes, so
// sniffing costs one small range request rather than the whole file.
//
//   EBML (1A 45 DF A3)   Matroska / WebM
//   "ftyp" at offset 4   ISO base media (MP4 and relatives)
//
// @returns {'mp4'|'mkv'|null} null when the bytes are not a recognised container.
export function containerFromBytes(buffer) {
  if (buffer.length >= 4 && buffer[0] === 0x1a && buffer[1] === 0x45 && buffer[2] === 0xdf && buffer[3] === 0xa3) {
    return 'mkv'
  }
  if (buffer.length >= 8 && buffer.toString('latin1', 4, 8) === 'ftyp') return 'mp4'
  return null
}

// A remux (stream copy) is cheap and keeps the original quality. It can fix the
// container and re-encode audio, but it cannot change the video codec, so HEVC and
// AV1 releases are rejected rather than transcoded.
export function canRemux(info) {
  return info.videoCodec === 'h264'
}

// These arguments produce a fragmented MP4 that can be streamed while it is being
// written, which is what makes piped playback possible.
//
// The input is stdin rather than a file path, because a torrent is only downloaded
// piece by piece: handing ffmpeg a path makes it read a file whose remaining bytes
// have not been fetched yet, and it stops at the first gap with "File ended
// prematurely". Reading through webtorrent's stream instead fetches each piece as
// ffmpeg consumes it.
const REMUX_ARGS = [
  '-nostdin',
  '-v', 'error',
  '-i', 'pipe:0',
  '-map', '0:v:0',
  '-map', '0:a:0?',
  '-c:v', 'copy',
  '-c:a', 'aac',
  '-b:a', '192k',
  '-ac', '2',
  '-movflags', 'frag_keyframe+empty_moov+default_base_moof',
  '-f', 'mp4',
  'pipe:1',
]

export function startRemux() {
  return spawn('ffmpeg', REMUX_ARGS, { stdio: ['pipe', 'pipe', 'pipe'] })
}

// Checked at startup so a missing ffmpeg is reported once, clearly, instead of
// surfacing as a broken stream later.
export function ffmpegAvailable() {
  return new Promise((resolve) => {
    execFile('ffmpeg', ['-version'], { timeout: 5000 }, (error) => resolve(!error))
  })
}
