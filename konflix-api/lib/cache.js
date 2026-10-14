// A tiny in-memory TTL cache.
//
// Metadata providers are rate limited (AniList allows 30 requests per minute) and
// torrent indexers are slow and unfriendly to hammering. Without this, every page
// load would fire a fresh round of upstream requests.
//
// Two things matter beyond storing a value:
//
//   - A torrent search now fans out over several search forms, and two viewers can
//     open the same episode at once. Storing the in-flight promise means concurrent
//     callers for one key share a single upstream request instead of stampeding it.
//   - The map must be bounded. Keys include user-supplied search terms, so an
//     unbounded map grows without limit on a long-running server.

const store = new Map()

// A TTL may be a fixed number or a function of the loaded value.
function toNumber(ttlMs, value) {
  return typeof ttlMs === 'function' ? ttlMs(value) : ttlMs
}

// Enough for every provider, indexer and search term a busy server sees in a
// window, while staying far below anything that would matter in memory.
const MAX_ENTRIES = 500

function remember(key, entry) {
  // Re-inserting moves the key to the end, which makes the first key the oldest.
  store.delete(key)
  store.set(key, entry)

  while (store.size > MAX_ENTRIES) {
    const oldest = store.keys().next().value
    store.delete(oldest)
  }
}

// The stored value is always a promise, so a hit and a miss are awaited the same
// way and concurrent callers for one key share a single upstream request.
//
// `ttlMs` may be a function of the loaded value, which is how an empty result gets a
// shorter life than a real one (see ttlFor).
export function cached(key, ttlMs, load) {
  const hit = store.get(key)
  if (hit && hit.expiresAt > Date.now()) return hit.value

  const entry = { value: null, expiresAt: Date.now() + toNumber(ttlMs, null) }

  entry.value = Promise.resolve()
    .then(load)
    .then(
      (value) => {
        entry.value = value
        // The TTL starts when the load finishes, not when it starts. Otherwise a
        // load that outlives its own TTL would already be expired on arrival, and a
        // second caller would start a duplicate request, defeating the dedup above.
        entry.expiresAt = Date.now() + toNumber(ttlMs, value)
        return value
      },
      (error) => {
        // A failure must not be cached, or one bad response would be replayed for
        // the whole TTL. Only this entry is removed, so a newer load for the same
        // key is left alone.
        if (store.get(key) === entry) store.delete(key)
        throw error
      },
    )

  remember(key, entry)
  return entry.value
}

// Drops a cached value so the next read goes upstream. Needed when something other
// than this process changes the data behind a key: the debrid source adds a torrent
// to the account, and the account listing it just read is now stale.
export function forget(key) {
  store.delete(key)
}

// How long a cached *empty* indexer result stays valid.
//
// Much shorter than a normal TTL on purpose. An empty result is usually a fluke - a
// blocked mirror, a rate limit, a transient failure - and caching it for the full ten
// minutes blackholes that search term for everyone until it expires. A real answer is
// worth keeping; "nothing found" is worth re-asking.
export const EMPTY_TTL_MS = 60 * 1000

// Picks the TTL for a loaded value: the short one when there is nothing in it.
export function ttlFor(value, fullTtlMs) {
  return Array.isArray(value) && value.length === 0 ? EMPTY_TTL_MS : fullTtlMs
}
