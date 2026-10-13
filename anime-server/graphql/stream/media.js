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
import { USER_AGENT } from '../../lib/http.js'

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

// Re-packages a file as HLS: fragmented MP4 segments plus a growing playlist.
//
// This replaces the single live fragmented MP4 that used to be piped straight to the
// player. That stream could not be played on iOS at all - see the comment at the top of
// hls.js for why - and segments fix it by making every response a complete file with a
// length, which is what the player insists on.
//
// The video is stream-copied, so quality is untouched and the work is I/O bound: measured
// at 6 seconds of content per second of wall clock on a 2 vCPU box. Audio is re-encoded
// to AAC because browser support for FLAC and DTS in MP4 is not universal.
//
// `input` is either a URL or `pipe:0`. A torrent must be read through its download stream
// rather than from its path: the file on disk is sparse until the pieces arrive, and
// ffmpeg reading a path stops at the first gap with "File ended prematurely".
//
// `-hls_playlist_type event` makes the playlist append-only and seekable, which is what
// lets the player jump to any segment already produced instead of only playing forwards.
export function startHlsTranscode(dir, input) {
  // ffmpeg identifies itself as "Lavf/<version>" by default, which CDNs and anti-bot
  // layers commonly reject outright - the request then fails with no body and no useful
  // stderr, so the transcode never produces a playlist. Sending the same agent this
  // server uses for its own requests keeps the two consistent. The option only exists for
  // a network input, so a torrent read through `pipe:0` must not be given it.
  const inputArgs = input === 'pipe:0' ? [] : ['-user_agent', USER_AGENT]

  return spawn(
    'ffmpeg',
    [
      '-nostdin',
      '-v', 'error',
      ...inputArgs,
      '-i', input,
      '-map', '0:v:0',
      '-map', '0:a:0?',
      '-c:v', 'copy',
      '-c:a', 'aac',
      '-b:a', '192k',
      '-ac', '2',
      '-f', 'hls',
      '-hls_time', '6',
      '-hls_playlist_type', 'event',
      '-hls_list_size', '0',
      '-hls_segment_type', 'fmp4',
      '-hls_flags', 'independent_segments',
      '-hls_fmp4_init_filename', 'init.mp4',
      '-hls_segment_filename', `${dir}/seg%05d.m4s`,
      `${dir}/index.m3u8`,
    ],
    { stdio: ['pipe', 'ignore', 'pipe'] },
  )
}

// Checked at startup so a missing ffmpeg is reported once, clearly, instead of
// surfacing as a broken stream later.
export function ffmpegAvailable() {
  return new Promise((resolve) => {
    execFile('ffmpeg', ['-version'], { timeout: 5000 }, (error) => resolve(!error))
  })
}
