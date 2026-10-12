// Debrid stream source (TorBox).
//
// A debrid service joins the swarm on its own infrastructure and hands back a
// direct HTTPS link to the finished file.
//
// Flow:
//   ranked releases -> which are already cached? -> request a link for one
//     -> hand the browser a short /dl link on this server
//
// Why the bytes are relayed rather than redirected:
//
//   The obvious design is to redirect the browser straight at the CDN. It cannot be
//   used. The `token` query parameter on a TorBox CDN link *is the API key* (verified
//   against the live API: the string is identical, and the CDN rejects a request
//   without it). A redirect would therefore publish the account's API key into the
//   page, the browser history, and every screenshot, and anyone holding it can read
//   and modify the whole account. So this server fetches the CDN URL itself, where
//   the key stays, and streams the bytes to the viewer with HTTP range support.
//
//   That means video bytes do cross this server, so the bandwidth saving of a debrid
//   redirect is not realised. What debrid still buys is real: no swarm membership,
//   no torrent disk cache, no ffmpeg, no peer connectivity requirement, and playback
//   that starts immediately instead of after a swarm search.
//
// Two more things are deliberate:
//
//   - Only cached releases are used. A release the service has not already fetched
//     cannot be watched now, and adding an uncached one both waits for a full
//     download and spends a separate 60-per-hour quota. `createtorrent` is therefore
//     called with `add_only_if_cached`, which asks the service to add the torrent
//     *only* if it is already cached, and to refuse otherwise.
//   - The CDN URL is re-resolved on every request rather than stored, because these
//     links expire and the service asks that they not be kept.
//
// Contract: https://api-docs.torbox.app (OpenAPI) and TorBox-App/torbox-sdk-js.

import { randomBytes } from 'node:crypto'

import { config } from '../../../lib/config.js'
import { http } from '../../../lib/http.js'
import { cached, forget } from '../../../lib/cache.js'
import { toHexInfoHash, buildMagnet } from '../../../lib/magnet.js'
import { isBrowserPlayableFile } from '../releases.js'
import { pickVideoFile } from '../torrent.js'

const API = 'https://api.torbox.app/v1/api/torrents'
const CACHE_TTL_MS = 10 * 60 * 1000

// How long to wait before re-reading the account listing, when the add has not shown
// up yet. The service accepts the add before it appears in the listing.
const SETTLE_MS = 500

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

// TorBox answers every route with { success, error, detail, data }. A failure is
// reported in the body as well as the status code, so both are checked.
function unwrap(body) {
  if (!body || body.success !== true) {
    throw new Error(body?.detail || body?.error || 'TorBox returned an unsuccessful response')
  }
  return body.data
}

// Every route authenticates with `Authorization: Bearer <token>`, so the token is
// sent as a header rather than a query parameter wherever the API allows it: a
// header cannot end up in a URL, a log line, or a redirect.
//
// `requestdl` is the exception. It declares `token` as a *required query parameter*
// and does not inherit the bearer scheme, and it answers a header-only request with
// `422 {"loc":["query","token"],"msg":"Field required"}`. That was verified against
// the live API, so the caller must pass it explicitly in `params`.
function authHeader() {
  return { Authorization: 'Bearer ' + config.torboxToken }
}
// The smallest remaining budget worth starting a request with.
//
// A request given 1ms cannot succeed, so starting it only delays the fallback source
// and pushes the request past its deadline. Anything under this is treated as "no
// budget left" and the caller gives up instead.
const MIN_STEP_MS = 250

function boundedTimeout(timeoutMs) {
  return Math.max(MIN_STEP_MS, Math.min(config.debridTimeoutMs, timeoutMs))
}

async function api(path, params, timeoutMs = config.debridTimeoutMs) {
  const { data } = await http.get(`${API}/${path}`, {
    params,
    headers: authHeader(),
    timeout: boundedTimeout(timeoutMs),
  })
  return unwrap(data)
}

// `createtorrent` is a POST that takes `multipart/form-data`, not a JSON body, and
// is the only route here that is not a GET.
async function apiPost(path, fields, timeoutMs = config.debridTimeoutMs) {
  const form = new FormData()
  for (const [key, value] of Object.entries(fields)) form.append(key, String(value))

  const { data } = await http.post(`${API}/${path}`, form, {
    headers: authHeader(),
    timeout: boundedTimeout(timeoutMs),
  })
  return unwrap(data)
}

// The token ends up in a query string on the requestdl route, and axios puts the
// request URL into some of its error messages. Anything derived from an error is
// passed through here before it is logged, so a rotated secret cannot be written to
// the server log by a failing request.
export function redactToken(message) {
  const token = config.torboxToken
  if (!token) return String(message)
  return String(message).split(token).join('[REDACTED]')
}

/**
 * Which of these releases TorBox already holds.
 *
 * `format=object` is used even though `list` is faster, because the object form is
 * the one that can return file names, which is what allows a playable MP4 to be
 * chosen over an MKV before spending a request on it.
 *
 * @returns {Promise<Set<string>>} hex info hashes that are cached.
 */
async function cachedHashes(releases, timeoutMs) {
  const hashes = releases.map((release) => release.infoHash).filter(Boolean)
  if (hashes.length === 0) return new Set()

  const data = await cached(`torbox:checkcached:${[...hashes].sort().join(',')}`, CACHE_TTL_MS, () =>
    api('checkcached', { hash: hashes.join(','), format: 'object', list_files: true }, timeoutMs),
  )

  const found = new Set()
  for (const [hash, entry] of Object.entries(data || {})) {
    if (!entry) continue
    // The key is the hash as it was sent, so it is normalised to match the releases.
    const normalised = toHexInfoHash(hash)
    if (normalised) found.add(normalised)
  }
  return found
}

// The account's own entry for a torrent. `mylist` is the only route that returns the
// numeric torrent and file ids that requestdl needs.
async function accountEntry(infoHash, timeoutMs) {
  // A generous limit because the response is one page of every torrent the account
  // holds, and an entry that falls past the end is indistinguishable from one that is
  // not there at all - which would silently drop a cached release.
  const list = await cached('torbox:mylist', CACHE_TTL_MS, () =>
    api('mylist', { limit: 5000 }, timeoutMs),
  )
  const torrent = (list || []).find((entry) => toHexInfoHash(entry.hash) === infoHash)
  if (!torrent?.files?.length) return null

  return {
    torrentId: torrent.id,
    files: torrent.files.map((file) => ({
      id: file.id,
      name: file.name || file.short_name || '',
      // `length` is the field the shared file picker reads; TorBox calls it `size`.
      length: file.size ?? 0,
    })),
  }
}

/**
 * Asks the service to add a torrent that `checkcached` says it already holds.
 *
 * `checkcached` answers from the service's own cache and returns no ids, and
 * `requestdl` needs the account's ids. `mylist` only contains torrents the account
 * holds, so a cached release that was never added has no ids and cannot be
 * downloaded. Adding it with `add_only_if_cached` closes that gap: it is accepted
 * because the data is already there, so nothing is fetched and no wait is involved,
 * and an uncached hash is refused rather than queued.
 *
 * The listing is cached for ten minutes, so it is invalidated afterwards; otherwise
 * the new entry would be invisible and every request would add the torrent again.
 *
 * @returns {Promise<boolean>} whether the account now holds the torrent.
 */
async function addIfCached(release, timeoutMs) {
  const magnet = release.magnet || buildMagnet(release.infoHash, release.title, config.trackers)
  try {
    await apiPost('createtorrent', { magnet, add_only_if_cached: true }, timeoutMs)
    forget('torbox:mylist')
    return true
  } catch (error) {
    // A refusal means the release is not actually cached, which is the normal case
    // for most candidates. It is not an error worth surfacing, so the reason is
    // logged and the caller moves to the next release.
    console.warn(`[stream:debrid] could not add "${release.title}": ${redactToken(error.message)}`)
    return false
  }
}

// --- /dl link registry -------------------------------------------------------
//
// The browser gets an opaque token, not the TorBox ids, so the account cannot be
// used as a generic link oracle by anyone who can guess an id pair.

const LINK_TTL_MS = 3 * 60 * 60 * 1000
const MAX_LINKS = 500
const links = new Map()

// A /dl token is a bearer credential: whoever holds it can watch the episode. It is
// therefore generated from a CSPRNG rather than Math.random, which is not
// cryptographically random and would make tokens guessable.
export function registerDownload(torrentId, fileId) {
  // Expired entries are dropped first, so a burst of new links cannot evict a token
  // that someone is still watching with.
  const now = Date.now()
  for (const [existing, entry] of links) {
    if (entry.expiresAt <= now) links.delete(existing)
  }
  while (links.size >= MAX_LINKS) links.delete(links.keys().next().value)

  const token = randomBytes(24).toString('base64url')
  links.set(token, { torrentId, fileId, expiresAt: now + LINK_TTL_MS })
  return token
}

/**
 * Whether a URL from the debrid API may be fetched.
 *
 * The destination does not come from the request, but this server does fetch it, so
 * it is parsed and allowlisted rather than trusted. A prefix check ("starts with
 * https://") would accept any host at all.
 */
export function isAllowedDownloadUrl(value) {
  let target
  try {
    target = new URL(value)
  } catch {
    return false
  }

  if (target.protocol !== 'https:') return false
  // Credentials in the URL would be sent to the host as part of the request.
  if (target.username || target.password) return false

  return config.debridCdnHosts.some(
    (host) => target.hostname === host || target.hostname.endsWith(`.${host}`),
  )
}

/**
 * Turns a /dl token into the CDN URL to fetch.
 *
 * @returns {Promise<string|null>} null when the token is unknown or expired.
 */
export async function resolveDownload(token) {
  const entry = links.get(token)
  if (!entry) return null
  if (entry.expiresAt <= Date.now()) {
    links.delete(token)
    return null
  }

  try {
    // The link expires and the service asks that it not be stored, so it is resolved
    // per request and cached only briefly, well inside their own window.
    return await cached(`torbox:dl:${entry.torrentId}:${entry.fileId}`, 60 * 1000, () =>
      api('requestdl', {
        torrent_id: entry.torrentId,
        file_id: entry.fileId,
        // Required by this route in addition to the header. See api() above.
        token: config.torboxToken,
      }),
    )
  } catch (error) {
    console.warn(`[stream:debrid] requestdl failed: ${redactToken(error.message)}`)
    return null
  }
}

export default {
  name: 'debrid',

  ready() {
    return Boolean(config.torboxToken)
  },

  /**
   * Whether the configured token is accepted by the API.
   *
   * `ready()` only knows that a token is set. A rotated, expired, or mistyped token
   * looks identical to a working one until someone tries to watch something, so
   * /health checks it against the API and reports what actually came back.
   */
  async status() {
    if (!config.torboxToken) return { configured: false }
    try {
      const account = await api('mylist', { limit: 1 }, config.debridTimeoutMs)
      return { configured: true, reachable: true, torrents: Array.isArray(account) ? account.length : 0 }
    } catch (error) {
      return { configured: true, reachable: false, error: redactToken(error.message) }
    }
  },

  async resolve({ episodeNumber, ranked, deadlineMs }) {
    if (!config.torboxToken) {
      return { available: false, reason: 'The debrid source is not configured.' }
    }

    // The caller passes whatever is left of the overall resolve budget. Every step
    // below is bounded by it, because otherwise a slow debrid API would spend the
    // whole budget and the fallback source would never get a turn.
    const deadline = Date.now() + (deadlineMs || config.debridTimeoutMs)
    const left = () => Math.max(0, deadline - Date.now())

    let cachedSet
    try {
      cachedSet = await cachedHashes(ranked, left())
    } catch (error) {
      // A debrid outage must not take the app down: reporting "unavailable" lets the
      // next configured source (usually the torrent relay) answer instead.
      console.warn(`[stream:debrid] checkcached failed: ${redactToken(error.message)}`)
      return { available: false, reason: 'The debrid service could not be reached.' }
    }

    const usable = ranked.filter((release) => cachedSet.has(release.infoHash))
    if (usable.length === 0) {
      return { available: false, reason: 'No cached copy of this episode on the debrid service.' }
    }

    // Each candidate costs two or three API calls, so the loop is bounded the same way
    // the relay bounds its own. Unbounded, a long list of cached releases that all fail
    // file selection would spend the whole resolve budget here and leave the fallback
    // source nothing to work with.
    for (const release of usable.slice(0, config.maxCandidates)) {
      if (left() <= 0) break
      try {
        const link = await linkFor(release, episodeNumber, left())
        if (link) return { ...link, release: release.title }
      } catch (error) {
        console.warn(`[stream:debrid] ${release.title}: ${redactToken(error.message)}`)
      }
    }

    // Running out of budget is a different failure from having tried everything, and
    // reporting it as a file problem would send someone looking for the wrong cause.
    if (left() <= 0) {
      return { available: false, reason: 'The debrid service ran out of time for this episode.' }
    }

    return {
      available: false,
      reason: 'Cached copies exist, but none is a file a browser can play directly.',
    }
  },
}

async function linkFor(release, episodeNumber, timeoutMs) {
  // A cached release the account does not hold yet has no ids, so it is added first.
  // This is the step that makes the source work at all on a fresh account: without
  // it, `mylist` is empty and no release can ever be resolved.
  let entry = await accountEntry(release.infoHash, timeoutMs)
  if (!entry && (await addIfCached(release, timeoutMs))) {
    // The add is asynchronous on the service's side: the listing that is read back
    // immediately after it can still be the pre-add snapshot. Reading it once more
    // after a short pause is the difference between serving the episode and skipping a
    // release that was successfully added.
    entry = await accountEntry(release.infoHash, timeoutMs)
    if (!entry && timeoutMs > SETTLE_MS + MIN_STEP_MS) {
      await sleep(SETTLE_MS)
      forget('torbox:mylist')
      entry = await accountEntry(release.infoHash, timeoutMs - SETTLE_MS)
    }
  }
  if (!entry) return null

  const playable = entry.files.filter((file) => isBrowserPlayableFile(file.name))
  // No size floor: the file list comes from the debrid API, where an absent size
  // would otherwise reject every file and look like "nothing is playable".
  const file = pickVideoFile(playable, episodeNumber, 0)
  if (!file) return null

  const dlToken = registerDownload(entry.torrentId, file.id)
  return {
    available: true,
    url: `${config.publicUrl}/dl/${dlToken}`,
    // The browser plays the file directly, so it is a plain progressive source.
    streamType: 'file',
  }
}
