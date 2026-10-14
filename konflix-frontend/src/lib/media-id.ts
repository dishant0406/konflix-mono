// Media ids, shared by every route and query.
//
// Every id is self-describing: it names the content type, then the provider's own
// id, then the episode where there is one.
//
//   anime-16498-ep1     an anime, episode 1
//   series-169-s1e5     a series, season 1 episode 5
//   movie-7691          a film, no episode
//   movie-tt32261958    a film, addressed by its IMDb id
//   series-169          a title, for a detail page
//
// The provider id is alphanumeric rather than digits-only because providers do not
// agree on a form: AniList, TVMaze and YTS use numbers, Cinemeta uses IMDb ids
// ("tt32261958"). The type is part of the id because the ids come from different
// providers and would otherwise collide: TVMaze's show 1 and YTS's film 1 are
// different titles. It also means a page can tell what it is looking at from the URL
// alone.
//
// This mirrors konflix-api/lib/mediaId.js. The API is the contract, so the two must
// agree; they are kept deliberately small so that stays obvious.

export type ContentType = "anime" | "series" | "movie";

const MEDIA_ID = /^(anime|series|movie)-([A-Za-z0-9]+)(?:-(.*))?$/;
const ANIME_SUFFIX = /^ep(\d+)$/;
const SERIES_SUFFIX = /^s(\d+)e(\d+)$/;

export interface ParsedMediaId {
  contentType: ContentType;
  /** The provider's own id, without the content type. */
  mediaId: string;
  season: number | null;
  episodeNumber: number | null;
  /** The title's id, without the episode. This is what a detail page uses. */
  key: string;
}

export function parseMediaId(id: string | string[] | undefined): ParsedMediaId | null {
  const value = Array.isArray(id) ? id[0] : id;
  const match = MEDIA_ID.exec(value || "");
  if (!match) return null;

  const [, contentType, mediaId, suffix] = match as unknown as [
    string,
    ContentType,
    string,
    string | undefined,
  ];
  const key = `${contentType}-${mediaId}`;

  // A film has no episode, so any suffix is a malformed id rather than a film with an
  // episode number.
  if (contentType === "movie") {
    return suffix ? null : { contentType, mediaId, season: null, episodeNumber: null, key };
  }

  if (!suffix) return { contentType, mediaId, season: null, episodeNumber: null, key };

  if (contentType === "anime") {
    const episode = ANIME_SUFFIX.exec(suffix);
    if (!episode) return null;
    return { contentType, mediaId, season: null, episodeNumber: Number.parseInt(episode[1], 10), key };
  }

  const episode = SERIES_SUFFIX.exec(suffix);
  if (!episode) return null;
  return {
    contentType,
    mediaId,
    season: Number.parseInt(episode[1], 10),
    episodeNumber: Number.parseInt(episode[2], 10),
    key,
  };
}

/** The title's id, which a detail page and a metadata lookup use. */
export function mediaKey(contentType: ContentType, mediaId: string | number): string {
  return `${contentType}-${mediaId}`;
}

/** An anime episode id. */
export function animeEpisodeId(mediaId: string | number, episodeNumber: number): string {
  return `anime-${mediaId}-ep${episodeNumber}`;
}

/** A series episode id. */
export function seriesEpisodeId(mediaId: string | number, season: number, episodeNumber: number): string {
  return `series-${mediaId}-s${season}e${episodeNumber}`;
}
