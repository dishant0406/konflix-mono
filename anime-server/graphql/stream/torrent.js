// Torrent engine.
//
// Owns the webtorrent client, the set of torrents we are currently serving, and
// the decision of which file inside a torrent to play.
//
// Downloads are streamed on demand: webtorrent only fetches the pieces a read
// actually touches, so playback starts without waiting for the whole file. That
// also means we must keep the torrent alive while it is being watched, and evict
// it afterwards so disk and sockets stay bounded.

import path from 'node:path'
import fs from 'node:fs'
import WebTorrent from 'webtorrent'

import { config } from '../../lib/config.js'
import { cacheDir } from '../../lib/cacheDir.js'
import { isBonus, isVideoFile, parseEpisode, parseSeason } from './releases.js'
import { probeFile } from './media.js'

// The webtorrent client is created on first use rather than at import time.
// Importing this module must not open sockets, start a DHT node, or bind a listen
// port, otherwise simply loading the code (in tests, or on a server that never
// streams anything) would leave live network handles behind.
let client = null

function getClient() {
  if (!client) client = new WebTorrent()
  return client
}

// infoHash -> { torrent, file, info, lastUsed, activeStreams, timer }
const active = new Map()

// infoHash -> Promise, so two simultaneous requests for one episode share a
// single torrent instead of racing to create two.
const pendingOpens = new Map()

// Files smaller than this are samples, not episodes.
const MIN_VIDEO_BYTES = 5 * 1024 * 1024

function addTorrent(magnet, expectedInfoHash, timeoutMs = config.metadataTimeoutMs) {
  return new Promise((resolve, reject) => {
    const torrent = getClient().add(magnet, {
      path: path.join(cacheDir, `t${Date.now()}-${Math.random().toString(36).slice(2, 8)}`),
      // webtorrent selects every piece of a torrent by default, which downloads the
      // whole episode instead of the parts being watched. Measured: one 25 minute
      // resolve left 542MB on disk. Deselecting up front means pieces are fetched
      // only when a read actually touches them, which is the on-demand behaviour
      // this server is built around.
      deselect: true,
    })

    const timer = setTimeout(() => {
      torrent.destroy({ destroyStore: true }, () => {})
      reject(new Error('Timed out waiting for peers'))
    }, timeoutMs)

    // Held in a variable so it can be detached once metadata arrives. A listener
    // left attached would destroy the torrent on any later error, including one
    // raised while a viewer is reading it.
    const onError = (error) => {
      clearTimeout(timer)
      torrent.destroy({ destroyStore: true }, () => {})
      reject(error)
    }
    torrent.once('error', onError)

    // 'metadata' fires as soon as the file list is known, before any pieces are
    // downloaded. That is all we need in order to pick a file and probe it.
    torrent.once('metadata', () => {
      clearTimeout(timer)
      torrent.removeListener('error', onError)

      // The map key comes from the release; every later lookup (serving, eviction)
      // uses torrent.infoHash. They must be the same string, or a torrent is added
      // under one form and can never be found - and therefore never evicted - under
      // the other. infoHash is only populated once metadata has arrived, which is
      // why this is checked here rather than straight after add().
      if (torrent.infoHash !== expectedInfoHash) {
        torrent.destroy({ destroyStore: true }, () => {})
        reject(new Error('Torrent info hash does not match the release'))
        return
      }

      resolve(torrent)
    })
  })
}

// Chooses which file inside the torrent is the episode we want.
//
// `minBytes` excludes samples. The debrid source passes 0 because its file list
// comes from the debrid API, where a missing size must not silently reject every
// file; the extension and episode checks are what matter there.
//
// `episodeNumber` may be null, which means the torrent is a film: a film has no
// episode number, so the largest video file is the feature and everything else
// (samples, extras, a trailer) is smaller than it.
//
// `season` narrows the match for a series, where a single torrent can hold every
// season. A file naming a *different* season is rejected outright: the season is part
// of the episode's identity, and a season pack is a case this server can now choose,
// so "S02E01" must never satisfy a request for "S01E01".
export function pickVideoFile(files, episodeNumber, minBytes = MIN_VIDEO_BYTES, season = null) {
  const videos = files.filter((file) => isVideoFile(file.name) && file.length >= minBytes)
  if (videos.length === 0) return null

  if (episodeNumber === null || episodeNumber === undefined) {
    // Largest first, so a sample or an extra cannot be mistaken for the feature.
    return [...videos].sort((a, b) => b.length - a.length)[0]
  }

  // A file that names a season other than the one requested is not this episode,
  // whatever its number says. This is a hard filter rather than a narrowing one: it
  // applies even when it would leave nothing, because serving the wrong season is
  // worse than serving nothing.
  //
  // A file naming no season is still accepted, because the season is only known from
  // the release title and this function never sees it. A pack titled "Show S02" whose
  // files are named "Show - 05.mkv" is a real shape, and rejecting those would turn a
  // correct season-2 pack into no source at all. Whether an unlabelled *release* may
  // satisfy a later season is decided in matchesEpisode, which does see the title.
  const inSeason = (file) => {
    if (season === null || season === undefined) return true
    const named = parseSeason(file.name)
    return named === null || named === season
  }

  const matching = videos.filter(
    (file) => parseEpisode(file.name) === episodeNumber && inSeason(file),
  )

  if (matching.length > 0) {
    // A bonus is numbered like an episode and so matches the same way, but it is
    // separate content. In the "India's Got Latent" Season 1 pack, "IGL_Bonus_EP_1" and
    // "IGL_EP_01" both parse as episode 1, and the bonus was listed first - so it was
    // served in place of the episode. A real episode is preferred, and the bonus is
    // only used when the pack genuinely holds nothing else for that number.
    const real = matching.filter((file) => !isBonus(file.name))
    return (real.length > 0 ? real : matching)[0]
  }

  // A single-file torrent does not have to name its episode, but if it does name
  // one it must be the one requested. Holding exactly one file proves nothing
  // about which episode it is, and serving the wrong episode silently is worse
  // than serving nothing.
  if (videos.length === 1 && inSeason(videos[0])) {
    return parseEpisode(videos[0].name) === null ? videos[0] : null
  }

  // A batch that does not name the episode cannot be resolved safely.
  return null
}

function scheduleEviction(entry) {
  clearTimeout(entry.timer)
  entry.timer = setTimeout(() => {
    evict(entry.torrent.infoHash)
  }, config.torrentTtlMs)
}

function evict(infoHash) {
  const entry = active.get(infoHash)
  if (!entry) return
  // An in-flight response is reading from this torrent; destroying it would
  // truncate playback. Let the next idle check handle it instead.
  if (entry.activeStreams > 0) {
    scheduleEviction(entry)
    return
  }
  clearTimeout(entry.timer)
  active.delete(infoHash)
  // destroyStore removes the downloaded files along with the torrent.
  entry.torrent.destroy({ destroyStore: true }, () => {})
}

// Makes room for a new torrent. An idle torrent is preferred, but if every torrent
// is currently streaming the oldest one is released anyway: exceeding the cap would
// grow sockets and disk without bound, and a viewer whose torrent is dropped simply
// gets the same "reload the episode" response as any other expired stream.
//
// Both limits are checked, because counting torrents alone does not bound disk: a
// 1080p episode is roughly 1.4GB, and an HLS transcode reads the whole file, so twelve
// concurrent torrents can occupy well past 15GB.
//
// Takes an array rather than the live Map: a Map iterator has no `length`, so
// passing `active.values()` here silently disabled the torrent-count limit.
export function overLimits(entries, limits) {
  let bytes = 0
  for (const entry of entries) bytes += entry.file.length
  return entries.length >= limits.maxTorrents || bytes >= limits.maxCacheBytes
}

function evictOldest() {
  let oldest = null
  for (const entry of active.values()) {
    // A torrent with a live response reading from it must never be destroyed: the
    // reader's stream would error and playback would truncate mid-episode. An idle
    // entry is always preferred.
    if (!oldest) {
      oldest = entry
      continue
    }
    const idleBeatsBusy = oldest.activeStreams > 0 && entry.activeStreams === 0
    if (idleBeatsBusy || (oldest.activeStreams === entry.activeStreams && entry.lastUsed < oldest.lastUsed)) {
      oldest = entry
    }
  }

  // Nothing idle to release. Dropping a torrent that is being watched would break an
  // active viewer, so the limit is allowed to be exceeded until a stream finishes
  // and its idle timer reclaims it.
  if (!oldest || oldest.activeStreams > 0) return false

  const infoHash = oldest.torrent.infoHash
  const entry = active.get(infoHash)
  if (!entry) return false
  clearTimeout(entry.timer)
  active.delete(infoHash)
  entry.torrent.destroy({ destroyStore: true }, () => {})
  return true
}

function makeRoom() {
  // A single torrent can be larger than the whole budget, so this stops as soon as
  // the map is empty rather than looping forever. The entries are materialised
  // because overLimits needs an array, not the Map's iterator.
  while (overLimits([...active.values()], config)) {
    if (!evictOldest()) return
  }
}

/**
 * Makes a torrent ready for playback.
 *
* @param {{ infoHash: string, magnet: string, title: string }} release
 * @param {number} episodeNumber
 * @param {number} timeoutMs
 * @param {number|null} season
 * @returns {Promise<{ torrent: object, file: object, info: object }>} the live session
 */
export async function openRelease(release, episodeNumber, timeoutMs, season = null) {
  // Concurrent requests for the same episode must share one torrent, so the
  // pending open is stored before it finishes.
  const pending = pendingOpens.get(release.infoHash)
  if (pending) return pending

  const opening = openNewRelease(release, episodeNumber, timeoutMs, season).finally(() => {
    pendingOpens.delete(release.infoHash)
  })
  pendingOpens.set(release.infoHash, opening)
  return opening
}

async function openNewRelease(release, episodeNumber, timeoutMs, season = null) {
  const existing = active.get(release.infoHash)
  if (existing) {
    existing.lastUsed = Date.now()
    scheduleEviction(existing)
    return existing
  }

  makeRoom()

  // The caller passes whatever is left of its overall budget. Each waiting step is
  // clamped to both its own configured limit and the time still left, so one slow
  // swarm can neither exceed its share nor push the whole request past the deadline.
  const startedAt = Date.now()
  const left = () => (timeoutMs ? Math.max(1, timeoutMs - (Date.now() - startedAt)) : undefined)
  const torrent = await addTorrent(
    release.magnet,
    release.infoHash,
    timeoutMs ? Math.min(config.metadataTimeoutMs, left()) : undefined,
  )

  // From here on the torrent exists, so every failure path must destroy it.
  // Otherwise it keeps holding sockets and disk, and because it was never added
  // to `active` the LRU eviction can never reclaim it.
  try {
    const file = pickVideoFile(torrent.files, episodeNumber, undefined, season)
    if (!file) {
      throw new Error(`No playable video file in "${release.title}"`)
    }

    // Probed through the download stream, not a path: only the pieces ffprobe
    // actually reads are fetched, and the whole file does not have to be present.
    const info = await probeFile(
      file,
      timeoutMs ? Math.min(config.probeTimeoutMs, left()) : config.probeTimeoutMs,
    )

    const entry = {
      torrent,
      file,
      info,
      lastUsed: Date.now(),
      activeStreams: 0,
      timer: null,
    }
    active.set(release.infoHash, entry)
    scheduleEviction(entry)
    return entry
  } catch (error) {
    active.delete(release.infoHash)
    torrent.destroy({ destroyStore: true }, () => {})
    throw error
  }
}

// Marks a response as reading from this torrent, so eviction leaves it alone.
export function beginStream(infoHash) {
  const entry = active.get(infoHash)
  if (!entry) return null
  entry.activeStreams += 1
  entry.lastUsed = Date.now()
  scheduleEviction(entry)
  return entry
}

export function endStream(infoHash) {
  const entry = active.get(infoHash)
  if (!entry) return
  entry.activeStreams = Math.max(0, entry.activeStreams - 1)
  entry.lastUsed = Date.now()
  scheduleEviction(entry)
}

export async function closeAll() {
  for (const entry of active.values()) clearTimeout(entry.timer)
  active.clear()
  pendingOpens.clear()
  if (!client) return
  await new Promise((resolve) => client.destroy(resolve))
  client = null
}

// A cheap readiness check for /health.
export function sessionCount() {
  return active.size
}

// Reported by /health. The scan is asynchronous and bounded: /health is
// unauthenticated, and a synchronous recursive walk of a cache that can hold many
// gigabytes would block every in-flight video response on the same event loop.
export async function cacheSizeBytes(maxEntries = 2000) {
  if (!fs.existsSync(cacheDir)) return 0

  let total = 0
  let seen = 0
  const queue = [cacheDir]

  while (queue.length > 0 && seen < maxEntries) {
    const target = queue.shift()
    let entries
    try {
      entries = await fs.promises.readdir(target, { withFileTypes: true })
    } catch {
      // A torrent being evicted can disappear mid-scan; ignore it.
      continue
    }

    for (const entry of entries) {
      seen += 1
      if (seen > maxEntries) break
      const entryPath = path.join(target, entry.name)
      if (entry.isDirectory()) {
        queue.push(entryPath)
        continue
      }
      try {
        const stats = await fs.promises.stat(entryPath)
        total += stats.size
      } catch {
        // Disappeared mid-scan.
      }
    }
  }

  return total
}
