// Metadata provider registry.
//
// A provider declares which content types it can serve and implements one contract
// per type. The rest of the codebase asks for a type, never for a provider.
//
//   types                       ['anime'] | ['series'] | ['movie'] | several
//
//   --- the small shapes, used by the stream layer ---
//   topAnime(page)              -> [{ id, title, trailer, imgUrl }]
//   animeSearch(term, page)     -> [{ id, title, trailer, imgUrl }]
//   animeDetails(id)            -> { id, name, type, status, totalEpisodes,
//                                    released, genres, synopsis, imageUrl,
//                                    episode_id: [{ episodeId, episodeNumber }],
//                                    trailer, titleVariants }
//   recentEpisodes(page)        -> [{ animeId, episodeId, name, episodeNum,
//                                     subOrDub, imgUrl }]
//   recommandedAnime(genre)     -> [{ animeId, animeTitle, animeImg }]
//
//   --- the full shapes, rendered by the frontend ---
//   mediaList(filters)          -> MediaPage
//   mediaSearch(filters)        -> MediaPage
//   mediaDetail(id, type)       -> MediaFull
//
// A provider only has to implement the functions for the types it declares. The
// anime provider implements all of them; the series and movie providers implement
// mediaList/mediaSearch/mediaDetail plus a details lookup used by the stream layer.
//
// A type may name several providers, most-preferred first (see config.providers).
// `withFallback` walks that list, so an outage or a gap in one provider degrades the
// answer instead of emptying it.
//
// `episodeId` must be opaque to callers and parseable by stream.js, because the
// frontend puts it in the URL and hands it back to streamLinkDetails. See
// lib/mediaId.js for the two forms.

import { config } from '../../lib/config.js'
import anilist from './anilist.js'
import cinemeta from './cinemeta.js'
import kitsu from './kitsu.js'
import tvmaze from './tvmaze.js'
import yts from './yts.js'

const providers = { anilist, cinemeta, kitsu, tvmaze, yts }

// The providers configured for a content type, in preference order.
//
// A type may name several ("series:cinemeta+tvmaze"), and one that does not declare
// the requested type is skipped rather than answered with - a provider asked for a
// type it does not serve would return the wrong catalogue, which is worse than the
// default one.
//
// `providerFor` (below) is the single-provider accessor the anime fields use, and it
// keeps the documented fallback to the anime provider so a request without a
// contentType behaves exactly as it did before content types existed.
export function providersFor(contentType = 'anime') {
  const usable = (name) => {
    const provider = providers[name]
    return provider?.types?.includes(contentType) ? provider : null
  }

  const configured = (config.providers[contentType] || []).map(usable).filter(Boolean)
  if (configured.length > 0) return configured

  // Nothing usable was configured. A missing entry for a content type is a
  // misconfiguration, and answering it with the anime provider would render anime
  // rows on the films site - a silent wrong answer. There is no honest provider for a
  // type that has none configured, so the chain is empty and the caller answers with
  // nothing.
  if (!(contentType in config.providers)) return []

  return [usable(config.providers.anime?.[0]) || anilist]
}

/**
 * The provider configured for a content type.
 *
 * The first in preference order, or the anime provider when nothing is configured -
 * the anime fields call this and must always have something to call. Callers that can
 * fall back should use `withFallback` instead.
 *
 * @param {string} [contentType] 'anime', 'series' or 'movie'.
 */
export function providerFor(contentType = 'anime') {
  return providersFor(contentType)[0] || anilist
}

// A title is "missing" when the provider answered with nothing: no detail, a page
// with no results, or a record with no id to route to. All mean the same thing to a
// caller - ask the next provider.
//
// The `results` key is checked by presence rather than by truthiness, because a page
// whose `results` is null is still a page that found nothing. Testing `Array.isArray`
// first would judge it non-empty and stop the chain, silently defeating the fallback.
export function isEmpty(result) {
  if (result === null || result === undefined) return true
  if ('results' in result) return !Array.isArray(result.results) || result.results.length === 0
  // A detail record with no id cannot be linked to, so it is not an answer.
  if ('id' in result) return !result.id
  return false
}

// What a chain answers with when every provider was empty or failed.
//
// A detail lookup answers null, which the resolvers already handle as "not found". A
// list or a search answers an empty page, because the frontend reads `results` off it
// and null there would be a render crash rather than an empty row.
export function emptyFor(method) {
  return method === 'mediaDetail'
    ? null
    : { page: 1, perPage: 0, total: 0, hasNextPage: false, results: [] }
}

/**
 * Calls `method` on each configured provider until one answers with something.
 *
 * This is what makes the provider list a fallback chain rather than a single choice.
 * It matters most for films: Cinemeta is a catalogue and YTS is a torrent tracker, so
 * a title the first knows about but has no release for is still findable, and a title
 * the first is missing is still searchable in the second.
 *
 * A provider that throws is treated as an empty answer, because an outage upstream
 * must not fail the request while another provider can answer it.
 *
 * @param {string} contentType
 * @param {string} method the provider function to call.
 * @param {Array} args arguments for that function.
 */
export async function withFallback(contentType, method, args) {
  const candidates = providersFor(contentType)

  for (const provider of candidates) {
    const fn = provider[method]
    if (typeof fn !== 'function') continue
    try {
      const result = await fn.apply(provider, args)
      if (!isEmpty(result)) return result
    } catch (error) {
      console.warn(`[providers] ${provider.name}.${method} failed: ${error.message}`)
    }
  }

  return emptyFor(method)
}

export function providerNames() {
  return Object.keys(providers)
}

// Which types each provider serves, for /health. Makes a misconfiguration visible
// at boot instead of at the first request for a film.
export function providerTypes() {
  return Object.fromEntries(
    Object.entries(providers).map(([name, provider]) => [name, provider.types || []]),
  )
}
