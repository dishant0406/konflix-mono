import test from 'node:test'
import assert from 'node:assert/strict'

import { cached, ttlFor, EMPTY_TTL_MS } from '../lib/cache.js'

test('cached returns the stored value within its TTL', async () => {
  let calls = 0
  const load = async () => {
    calls += 1
    return 'value'
  }

  assert.equal(await cached('ttl-hit', 1000, load), 'value')
  assert.equal(await cached('ttl-hit', 1000, load), 'value')
  assert.equal(calls, 1)
})

test('cached re-loads once the TTL has expired', async () => {
  let calls = 0
  const load = async () => {
    calls += 1
    return calls
  }

  assert.equal(await cached('ttl-expiry', 0, load), 1)
  await new Promise((resolve) => setTimeout(resolve, 5))
  assert.equal(await cached('ttl-expiry', 0, load), 2)
})

test('cached shares one in-flight request between concurrent callers', async () => {
  // A torrent search fans out over several terms and two viewers can open the same
  // episode at once. Without this, each caller would fire its own upstream request.
  let calls = 0
  const load = async () => {
    calls += 1
    await new Promise((resolve) => setTimeout(resolve, 20))
    return 'shared'
  }

  const results = await Promise.all([
    cached('inflight', 1000, load),
    cached('inflight', 1000, load),
    cached('inflight', 1000, load),
  ])

  assert.deepEqual(results, ['shared', 'shared', 'shared'])
  assert.equal(calls, 1)
})

test('cached does not store a failure', async () => {
  let calls = 0
  const load = async () => {
    calls += 1
    if (calls === 1) throw new Error('upstream down')
    return 'recovered'
  }

  await assert.rejects(() => cached('failure', 1000, load), /upstream down/)
  // One bad response must not be replayed for the whole TTL.
  assert.equal(await cached('failure', 1000, load), 'recovered')
  assert.equal(calls, 2)
})

test('cached stays bounded', async () => {
  // Keys include user-supplied search terms, so an unbounded map would grow
  // without limit on a long-running server. The oldest entries are dropped.
  for (let index = 0; index < 600; index += 1) {
    await cached(`bound-${index}`, 60_000, async () => index)
  }

  // The most recent key survives.
  assert.equal(await cached('bound-599', 60_000, async () => 'reloaded'), 599)
  // An evicted key is simply loaded again rather than erroring.
  let reloaded = false
  const value = await cached('bound-0', 60_000, async () => {
    reloaded = true
    return 'fresh'
  })
  assert.equal(value, 'fresh')
  assert.equal(reloaded, true)
})

test('an empty result is cached for the short TTL, a real one for the full TTL', () => {
  // An empty result is usually a fluke - a blocked mirror, a rate limit - and caching
  // it for the full ten minutes blackholes that search term for everyone. A real
  // answer is worth keeping; "nothing found" is worth re-asking.
  const FULL = 10 * 60 * 1000
  assert.equal(ttlFor([], FULL), EMPTY_TTL_MS)
  assert.equal(ttlFor([{ a: 1 }], FULL), FULL)
  // Called with the initial value while the load is still pending, so it must
  // tolerate null and must not shorten the TTL - the dedup of concurrent callers
  // depends on the entry staying alive.
  assert.equal(ttlFor(null, FULL), FULL)
  assert.equal(ttlFor(undefined, FULL), FULL)
  assert.ok(EMPTY_TTL_MS < FULL, 'the empty TTL must be shorter than the full one')
})

test('cached accepts a TTL as a function of the loaded value', async () => {
  let loads = 0
  const load = async () => {
    loads += 1
    return loads === 1 ? [] : ['found']
  }

  const key = `ttl-fn-${Date.now()}`
  const shortTtl = (value) => ttlFor(value, 60 * 60 * 1000)

  // First load returns an empty array, so the entry expires after the short TTL.
  assert.deepEqual(await cached(key, shortTtl, load), [])
  assert.equal(loads, 1)

  // Still inside the short window: served from cache.
  assert.deepEqual(await cached(key, shortTtl, load), [])
  assert.equal(loads, 1, 'a second read inside the TTL must not reload')
})
