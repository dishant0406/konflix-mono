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
//   mediaDetail(id)             -> MediaFull
//
// A provider only has to implement the functions for the types it declares. The
// anime provider implements all of them; the series and movie providers implement
// mediaList/mediaSearch/mediaDetail plus a details lookup used by the stream layer.
//
// `episodeId` must be opaque to callers and parseable by stream.js, because the
// frontend puts it in the URL and hands it back to streamLinkDetails. See
// parseEpisodeId there for the two forms.

import { config } from '../../lib/config.js'
import anilist from './anilist.js'
import kitsu from './kitsu.js'
import tvmaze from './tvmaze.js'
import yts from './yts.js'

const providers = { anilist, kitsu, tvmaze, yts }

/**
 * The provider configured for a content type.
 *
 * Falls back to the anime provider so a request without a contentType keeps
 * behaving exactly as it did before content types existed.
 *
 * @param {string} [contentType] 'anime', 'series' or 'movie'.
 */
export function providerFor(contentType = 'anime') {
  const configured = config.providers[contentType]
  const provider = configured ? providers[configured] : null
  // A provider configured for a type it does not declare would answer with the
  // wrong catalogue, which is worse than answering with the default one.
  if (provider?.types?.includes(contentType)) return provider
  return providers[config.providers.anime] || anilist
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
