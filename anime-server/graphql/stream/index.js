// Stream resolution.
//
// This is the volatile layer of the application. It is deliberately isolated
// behind one function so that when indexers or codecs change, only this directory
// needs to change.
//
// Flow:
//   episode id -> anime title -> indexer search -> rank releases
//     -> hand the candidates to the configured stream source
//     -> return a playable URL, or an explicit "nothing available"
//
// Where the bytes come from is decided by the source, not here. See
// sources/index.js: the torrent source relays them through this server, the debrid
// source points the browser straight at a CDN.

import { config } from '../../lib/config.js'
import { searchAll } from '../indexers/index.js'
import { rankReleases, searchTerms } from './releases.js'
import { resolveFromSources } from './sources/index.js'
import { provider } from '../providers/index.js'

// episodeId format is owned by the metadata provider: "<mediaId>-ep<number>".
export function parseEpisodeId(episodeId) {
  const match = /^(\d+)-ep(\d+)$/.exec(episodeId || '')
  if (!match) return null
  return { mediaId: match[1], episodeNumber: Number.parseInt(match[2], 10) }
}

// How many ranked candidates are offered to the sources.
//
// More than the relay would try, because the debrid source only needs one of them
// to be cached and checking a hash is cheap, while opening a torrent is not.
const CANDIDATES = 10

/**
 * Finds a playable source for an episode.
 *
 * Never throws: if no release can be played, it returns `{ available: false }`
 * with a reason, so the UI can show an honest message instead of a broken player.
 */
export async function resolveStream(episodeId) {
  const parsed = parseEpisodeId(episodeId)
  if (!parsed) {
    return { available: false, reason: 'Unrecognised episode id.' }
  }

  const details = await provider.animeDetails(parsed.mediaId)
  if (!details) {
    return { available: false, reason: 'Unknown anime.' }
  }

  const titles = details.titleVariants?.length ? details.titleVariants : [details.name]
  // Releases write the episode number in several ways and the indexers match
  // literally, so every form is searched and the results merged. Searching only the
  // first form was measured to be barely faster and to pick a worse release: for
  // Bocchi it ranked a 2-seeder release above a 56-seeder one, because the better
  // release only appears under another spelling of the episode number.
  const terms = searchTerms(titles, parsed.episodeNumber)
  const releases = await searchAll(terms)
  const ranked = rankReleases(releases, parsed.episodeNumber, details.name, titles).slice(
    0,
    Math.max(config.maxCandidates, CANDIDATES),
  )

  if (ranked.length === 0) {
    return { available: false, reason: `No torrent found for "${terms[0]}".`, details }
  }

  const result = await resolveFromSources({
    episodeNumber: parsed.episodeNumber,
    ranked,
    deadlineMs: config.resolveDeadlineMs,
  })

  if (!result.available) console.warn(`[stream] ${episodeId}: ${result.reason}`)

  return { ...result, details }
}
