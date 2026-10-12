// Stream source registry.
//
// A "source" is where the video bytes come from. The rest of the application only
// knows the contract below, so adding one is a new file and a key in the map.
//
//   name                          key used in STREAM_SOURCES
//   ready()                       -> boolean, false when the source is not usable
//                                    (missing credentials, for example)
//   resolve({ episodeNumber, ranked, deadlineMs })
//                                 -> { available: false, reason }
//                                 -> { available: true, url, streamType, release }
//
// resolve() must never throw. Sources go down and get blocked; a failing source
// should reduce what is playable, not fail the request. `ranked` is the candidate
// release list, best first, and `deadlineMs` is the time budget left for the whole
// resolve.

import { config } from '../../../lib/config.js'
import torrent from './torrent.js'
import debrid from './debrid.js'

const sources = { torrent, debrid }

export function sourceNames() {
  return Object.keys(sources)
}

// The sources to try, in order, skipping any that are not usable right now.
export function activeSources() {
  return config.streamSources
    .map((name) => sources[name])
    .filter((source) => source && source.ready())
}

/**
 * Whether each *configured* source is usable, for /health.
 *
 * Every configured source is reported, including ones that are not active: a source
 * that is configured but not ready is the interesting case (a missing credential),
 * and it would otherwise be invisible.
 *
 * @returns {Promise<Record<string, object>>} one entry per configured source.
 */
export async function sourceStatus() {
  const entries = await Promise.all(
    config.streamSources.map(async (name) => {
      const source = sources[name]
      if (!source) return [name, { configured: false, error: 'Unknown stream source.' }]
      if (!source.ready()) return [name, { configured: false }]
      // A source without its own probe reports only that it is configured. Nothing
      // is checked against the network, so this stays cheap enough for a health poll.
      if (typeof source.status !== 'function') return [name, { configured: true }]
      try {
        return [name, { configured: true, ...(await source.status()) }]
      } catch (error) {
        return [name, { configured: true, reachable: false, error: error?.message || 'check failed' }]
      }
    }),
  )
  return Object.fromEntries(entries)
}

/**
 * Asks each configured source in turn for a playable stream.
 *
 * `deadlineMs` is the budget for the *whole* resolve, not for each source: it is
 * turned into one absolute deadline here and every source is handed what is left of
 * it. Sources therefore never need to know about each other.
 *
 * @param {{ episodeNumber: number, ranked: Array, deadlineMs: number }} input
 * @returns {Promise<{ available: boolean }>} the first source that succeeds, or the
 *   last reason when none does.
 */
export async function resolveFromSources(input) {
  const active = activeSources()
  if (active.length === 0) {
    // Distinguish "nothing configured" from "configured but unusable", because the
    // second is a missing API key and the first is a mistake in the environment.
    const reason = config.streamSources.length
      ? `No usable stream source. Configured: ${config.streamSources.join(', ')}.`
      : 'No stream source is configured.'
    return { available: false, reason }
  }

  // The budget is shared across the sources rather than handed to each of them in
  // turn. Passing the same number to every source would let each one start a fresh
  // deadline of its own, so a request could take twice the configured maximum before
  // the last source gave up - which is exactly what a slow fallback used to do.
  const budget = Number.isFinite(input.deadlineMs) ? input.deadlineMs : config.resolveDeadlineMs
  const deadline = Date.now() + budget

  const fallbackReason = 'No stream source could serve this episode.'
  let reason = fallbackReason

  for (const source of active) {
    const remaining = deadline - Date.now()
    if (remaining <= 0) {
      // Out of time before this source was even asked. Keep the more specific reason
      // from the source that did run, if there is one.
      if (reason === fallbackReason) reason = 'Timed out looking for a playable source.'
      break
    }

    let result
    try {
      result = await source.resolve({ ...input, deadlineMs: remaining })
    } catch (error) {
      // A source is documented never to throw, so this is a bug in that source
      // rather than an expected failure. It must still not fail the request: the
      // next source may well have an answer.
      console.error(`[stream:${source.name}] threw: ${error?.message || error}`)
      continue
    }

    if (result?.available) return { ...result, source: source.name }

    // The first source is the most preferred one, so its reason describes the most
    // relevant failure. Later sources are fallbacks and report something vaguer
    // ("none could be played"), which is less useful to show than "nothing cached".
    if (reason === fallbackReason && result?.reason) reason = result.reason
  }

  return { available: false, reason }
}
