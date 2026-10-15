// GraphQL resolvers.
//
// This file only dispatches. Metadata comes from the provider configured for the
// requested content type, video from the stream layer. Adding a provider or an
// indexer never requires a change here.
//
// The Query fields are the root value handed to graphql-http, exported flat rather
// than wrapped in `{ Query: ... }`. graphql-js invokes a root field function as
// `(args, context, info)` - there is no parent argument - so the signatures below
// take `args` first. This is the standard rootValue contract.

import { providerFor, withFallback } from '../providers/index.js'
import { resolveStream } from '../stream/index.js'
import { parseMediaId } from '../../lib/mediaId.js'

// The three media fields take the content type from the input, defaulting to anime
// so a client that predates content types keeps its behaviour exactly.
function typeOf(input) {
  return input?.contentType || 'anime'
}

const Query = {
  mediaList: (args) => {
    const input = args.input || {}
    return withFallback(typeOf(input), 'mediaList', [input])
  },

  mediaSearch: (args) => {
    const input = { ...(args.input || {}), term: args.term }
    return withFallback(typeOf(input), 'mediaSearch', [input])
  },

  mediaDetail: (args) => {
    const input = args.input || {}
    // A detail request carries either a full media id or a bare provider id. Both
    // are accepted so an existing link keeps working; a full id wins because it is
    // the only form that says which provider to ask.
    const parsed = parseMediaId(args.id)
    if (parsed) return withFallback(parsed.contentType, 'mediaDetail', [parsed.mediaId, parsed.contentType])
    return withFallback(typeOf(input), 'mediaDetail', [args.id, typeOf(input)])
  },

  // --- the anime shapes, kept because the anime site and the stream layer use them

  topAnime: (args) => providerFor('anime').topAnime(args.page || 1),

  animeSearch: (args) => providerFor('anime').animeSearch(args.id, args.page || 1),

  animeDetails: (args) => providerFor('anime').animeDetails(args.id),

  recentEpisodes: (args) => providerFor('anime').recentEpisodes(args.page || 1),

  recommandedAnime: (args) => providerFor('anime').recommandedAnime(args.genre),

  streamLinkDetails: async (args) => {
    const id = args.id

    // resolveStream never throws, but it calls the metadata provider first. A
    // provider failure (a 429 from AniList, a timeout) would otherwise surface its
    // raw error text - including the upstream URL - to the browser.
    let result
    try {
      result = await resolveStream(id)
    } catch (error) {
      console.error(`[streamLinkDetails] ${error.message}`)
      result = { available: false, reason: 'Could not look up this title. Try again shortly.' }
    }

    const details = result.details || {}
    // The episode number comes out of the id, which is the only place it is known for
    // a series: a film has none, and the provider reports a count rather than a
    // current episode.
    const episodeNumber = parseMediaId(id)?.episodeNumber ?? null

    return {
      id,
      url: result.url || null,
      streamType: result.streamType || null,
      available: Boolean(result.available),
      reason: result.reason || null,
      release: result.release || null,
      titleWithEpisode:
        details.name && episodeNumber ? `${details.name} Episode ${episodeNumber}` : null,
      episodeNumber,
    }
  },
}
export default Query
