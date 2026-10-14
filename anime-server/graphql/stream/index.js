// Stream resolution.
//
// This is the volatile layer of the application. It is deliberately isolated
// behind one function so that when indexers or codecs change, only this directory
// needs to change.
//
// Flow:
//   media id -> title -> indexer search -> rank releases
//     -> hand the candidates to the configured stream source
//     -> return a playable URL, or an explicit "nothing available"
//
// Where the bytes come from is decided by the source, not here. See
// sources/index.js: the torrent source relays them through this server, the debrid
// source points the browser straight at a CDN.
//
// Three content types share this path. They differ only in how the id is parsed and
// how releases are ranked:
//
//   anime   "<id>-ep<n>"        flat episode number, no season
//   series  "<id>-s<n>e<m>"     episode number and season
//   movie   "<id>"              no episode at all; ranked by title and year

import { config } from '../../lib/config.js'
import { parseMediaId } from '../../lib/mediaId.js'
import { searchAll } from '../indexers/index.js'
import {
  movieSearchTerms,
  rankMovieReleases,
  rankReleases,
  searchTerms,
  seriesSearchTerms,
} from './releases.js'
import { resolveFromSources } from './sources/index.js'
import { providersFor } from '../providers/index.js'

// How many ranked candidates are offered to the sources.
//
// More than the relay would try, because the debrid source only needs one of them
// to be cached and checking a hash is cheap, while opening a torrent is not.
const CANDIDATES = 10

// Finds the candidates for one request: search terms, the indexer fan-out and the
// ranking. The three content types differ only in the terms and the ranking.
async function findReleases(parsed, details) {
  const titles = details.titleVariants?.length ? details.titleVariants : [details.name]
  const { contentType, episodeNumber, season } = parsed
  // EZTV and Torrentio are searched by IMDb id rather than by text, which the provider
  // supplies. Both are passed on every search and simply ignored by the indexers that do
  // not need them. The season and episode travel too, because Torrentio takes them in
  // its path: "series/tt33094114:2:5" is the request that returns episode 5.
  const options = { contentType, imdbId: details.imdbId, season, episodeNumber }

  if (contentType === 'movie') {
    const terms = movieSearchTerms(titles, details.year)
    const releases = await searchAll(terms, options)
    return { terms, ranked: rankMovieReleases(releases, details.year, details.name, titles) }
  }

  const terms =
    contentType === 'series'
      ? seriesSearchTerms(titles, season, episodeNumber)
      : searchTerms(titles, episodeNumber)
  const releases = await searchAll(terms, options)
  return { terms, ranked: rankReleases(releases, episodeNumber, details.name, titles, season) }
}

// The provider function that looks a title up, by content type.
//
// Every provider answers animeDetails, which is the small shape the stream layer
// reads: `name`, `titleVariants`, `year` and `imdbId`. It is deliberately not
// mediaDetail, whose MediaFull shape names the title under `title.english` and
// carries no year - reading that here made every series search for the literal
// string "undefined".
//
// Providers are tried in configured order, so a title the first one does not know is
// still looked up in the next. The content type is passed because an id alone does
// not say which endpoint answers: an IMDb id is a film or a series, and only the
// caller knows which.
async function detailsFor(contentType, mediaId) {
  for (const provider of providersFor(contentType)) {
    try {
      const details = await provider.animeDetails(mediaId, contentType)
      if (details) return details
    } catch (error) {
      console.warn(`[stream] ${provider.name}.animeDetails failed: ${error.message}`)
    }
  }
  return null
}

/**
 * Finds a playable source for one episode or film.
 *
 * Never throws: if no release can be played, it returns `{ available: false }`
 * with a reason, so the UI can show an honest message instead of a broken player.
 *
 * @param {string} id a media id, e.g. "anime-16498-ep1". See lib/mediaId.js.
 */
export async function resolveStream(id) {
  const parsed = parseMediaId(id)
  if (!parsed) {
    return { available: false, reason: 'Unrecognised media id.' }
  }

  // A title without an episode is a detail page, not a watch page. A film is the
  // exception: it has no episode number by definition, so its absence is expected.
  if (parsed.contentType !== 'movie' && parsed.episodeNumber === null) {
    return { available: false, reason: 'No episode was requested.' }
  }

  const details = await detailsFor(parsed.contentType, parsed.mediaId)
  if (!details) {
    return { available: false, reason: 'Unknown title.' }
  }

  const { terms, ranked: allRanked } = await findReleases(parsed, details)
  const ranked = allRanked.slice(0, Math.max(config.maxCandidates, CANDIDATES))

  // Sources are asked even when no indexer found anything. A source is not necessarily
  // an indexer-driven one: archive.org answers by title, and the case it exists for is
  // precisely the one where every torrent indexer has nothing. Returning early here
  // would make that source unreachable exactly when it is needed.
  const result = await resolveFromSources({
    episodeNumber: parsed.episodeNumber,
    // The season matters when choosing a file out of a torrent that holds a whole
    // show: "S02E05" is a different episode from "S01E05". Ranking already uses it;
    // this carries it into the file pick.
    season: parsed.season,
    // Every name the work is known by, plus the search terms, because a source that
    // looks a title up directly (archive.org) has nothing else to go on when the
    // indexers returned nothing.
    titles: details.titleVariants?.length ? details.titleVariants : [details.name],
    terms,
    ranked,
    deadlineMs: config.resolveDeadlineMs,
  })

  if (!result.available) console.warn(`[stream] ${id} (${parsed.contentType}): ${result.reason}`)

  // When no indexer found anything at all, say so. The sources then report their own
  // "nothing here" reasons, which are true but misleading on their own: they describe
  // the fallbacks rather than the actual problem, which is that the release could not
  // be found anywhere. The reason still names the first term, because that is what a
  // person would search for by hand to check.
  if (!result.available && ranked.length === 0) {
    return { ...result, reason: `No torrent found for "${terms[0]}".`, details }
  }

  return { ...result, details }
}
