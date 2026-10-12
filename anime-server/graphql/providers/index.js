// Metadata provider registry.
//
// To add a provider: create a file next to this one that default-exports an object
// with these five async functions, then add it to the map below. Nothing else in
// the codebase changes.
//
//   topAnime(page)             -> [{ id, title, trailer, imgUrl }]
//   animeSearch(term, page)    -> [{ id, title, trailer, imgUrl }]
//   animeDetails(id)           -> { id, name, type, status, totalEpisodes,
//                                   released, genres, synopsis, imageUrl,
//                                   episode_id: [{ episodeId, episodeNumber }],
//                                   trailer }
//   recentEpisodes(page)       -> [{ animeId, episodeId, name, episodeNum,
//                                    subOrDub, imgUrl }]
//   recommandedAnime(genre)    -> [{ animeId, animeTitle, animeImg }]
//
// `episodeId` must be opaque to callers and parseable by stream.js, because the
// frontend puts it in the URL and hands it back to streamLinkDetails.

import { config } from '../../lib/config.js'
import anilist from './anilist.js'
import kitsu from './kitsu.js'

const providers = { anilist, kitsu }

export const provider = providers[config.metadataProvider] || anilist

export function providerNames() {
  return Object.keys(providers)
}
