// Media ids.
//
// Every id is self-describing: it names the content type, then the provider's own
// id, then the episode where there is one.
//
//   anime-16498-ep1     an anime, episode 1
//   series-169-s1e5     a series, season 1 episode 5
//   movie-7691          a film, no episode
//   movie-tt32261958    a film, addressed by its IMDb id
//
// The provider id is alphanumeric rather than digits-only because providers do not
// agree on a form: AniList, TVMaze and YTS use numbers, Cinemeta uses IMDb ids
// ("tt32261958"). This mirrors akidev-frontend/src/lib/media-id.ts, and the two must
// agree - the frontend puts the id in the URL and the API parses it back out.
//
// The type is part of the id rather than a separate argument because the ids come
// from different providers and would otherwise collide: TVMaze's show 1 and YTS's
// film 1 are different titles, and both are reachable from the same site. It also
// means the watch page can tell what it is looking at from the URL alone, with no
// second request to find out.

const TYPES = ['anime', 'series', 'movie']

const MEDIA_ID = /^(anime|series|movie)-([A-Za-z0-9]+)(?:-(.*))?$/
const EPISODE_SUFFIX = /^ep(\d+)$/
const SERIES_SUFFIX = /^s(\d+)e(\d+)$/

export const CONTENT_TYPES = TYPES

/**
 * Parses a media id.
 *
 * The episode part is optional, because the same function serves two purposes: a
 * detail page addresses a title ("series-169") and a watch page addresses one
 * episode ("series-169-s1e5"). What is not optional is that the suffix matches the
 * content type - a film has no episode, an anime has no season - so a malformed id
 * is rejected rather than silently reinterpreted.
 *
 * @param {string} id
 * @returns {{ contentType: string, mediaId: string, season: number|null,
 *             episodeNumber: number|null, key: string } | null}
 *   `key` is the title's own id, without the episode - the id the metadata provider
 *   is asked for.
 */
export function parseMediaId(id) {
  const match = MEDIA_ID.exec(String(id ?? ''))
  if (!match) return null

  const [, contentType, mediaId, suffix] = match
  const key = `${contentType}-${mediaId}`

  if (contentType === 'movie') {
    // A film has no episode, so any suffix is a malformed id rather than a film with
    // an episode number.
    return suffix ? null : { contentType, mediaId, season: null, episodeNumber: null, key }
  }

  if (!suffix) return { contentType, mediaId, season: null, episodeNumber: null, key }

  if (contentType === 'anime') {
    const episode = EPISODE_SUFFIX.exec(suffix)
    if (!episode) return null
    return {
      contentType,
      mediaId,
      season: null,
      episodeNumber: Number.parseInt(episode[1], 10),
      key,
    }
  }

  const episode = SERIES_SUFFIX.exec(suffix)
  if (!episode) return null
  return {
    contentType,
    mediaId,
    season: Number.parseInt(episode[1], 10),
    episodeNumber: Number.parseInt(episode[2], 10),
    key,
  }
}

// The title's id, which is what a detail page and a metadata lookup use.
export function mediaKey(contentType, mediaId) {
  return `${contentType}-${mediaId}`
}

// An anime episode id.
export function animeEpisodeId(mediaId, episodeNumber) {
  return `anime-${mediaId}-ep${episodeNumber}`
}

// A series episode id.
export function seriesEpisodeId(mediaId, season, episodeNumber) {
  return `series-${mediaId}-s${season}e${episodeNumber}`
}
