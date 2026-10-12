// GraphQL resolvers.
//
// This file only dispatches. Metadata comes from the configured provider, video
// from the stream layer. Adding a provider or indexer never requires a change here.
//
// The Query fields are the root value handed to graphql-http, exported flat rather
// than wrapped in `{ Query: ... }`. graphql-js invokes a root field function as
// `(args, context, info)` - there is no parent argument - so the signatures below
// take `args` first. This is the standard rootValue contract.

import { provider } from '../providers/index.js'
import { resolveStream } from '../stream/index.js'

const Query = {
  topAnime: (args) => provider.topAnime(args.page || 1),

  animeSearch: (args) => provider.animeSearch(args.id, args.page || 1),

  animeDetails: (args) => provider.animeDetails(args.id),

  recentEpisodes: (args) => provider.recentEpisodes(args.page || 1),

  recommandedAnime: (args) => provider.recommandedAnime(args.genre),

  streamLinkDetails: async (args) => {
    const episodeId = args.id
    // resolveStream never throws, but it calls the metadata provider first. A
    // provider failure (a 429 from AniList, a timeout) would otherwise surface its
    // raw error text - including the upstream URL - to the browser.
    let result
    try {
      result = await resolveStream(episodeId)
    } catch (error) {
      console.error(`[streamLinkDetails] ${error.message}`)
      result = { available: false, reason: 'Could not look up this episode. Try again shortly.' }
    }
    const details = result.details || {}
    const episodeNumber = Number.parseInt(episodeId?.split('-ep')[1], 10) || null

    return {
      id: episodeId,
      url: result.url || null,
      streamType: result.streamType || null,
      available: Boolean(result.available),
      reason: result.reason || null,
      release: result.release || null,
      animeNameWithEP:
        details.name && episodeNumber ? `${details.name} Episode ${episodeNumber}` : null,
      ep_num: episodeNumber,
      anime_info: details.synopsis || null,
    }
  },
}

export default Query
