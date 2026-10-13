// YTS metadata provider for films.
//
// Chosen over TMDB because it is keyless, which keeps the whole stack free of
// accounts, and because its catalogue has real Indian coverage - the one thing a
// Hollywood-only source cannot provide. TMDB would give richer metadata (real
// trailers, images, credits) but needs a key, forbids commercial use and mandates an
// attribution string; the provider map means it can be added later as one file
// without touching anything else.
//
// The host is mid-migration. The old yts.mx no longer resolves; the live base is
// movies-api.accel.li, which the API's own response names as the successor. It has
// moved more than once, so it is a config value.
//
// Three behaviours were verified against the live API and shape this file:
//
//   1. An unknown id answers 200 with `data.movie.title === null` and `id: 0`, not a
//      404. Returning that to the UI would render a blank page rather than a
//      "not found", so it is rejected explicitly.
//   2. The `year` parameter is IGNORED. `?year=2024` returned films from 2018. A
//      browse by year is therefore filtered here, over one page of results.
//   3. `movie_suggestions.json` returns at most four related films, which is what
//      fills the recommendations row.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { mediaKey } from '../../lib/mediaId.js'

const TTL_MS = 10 * 60 * 1000

// The API's own maximum.
const MAX_PER_PAGE = 50

// A year filter cannot be pushed upstream, so it is applied over one full page.
const YEAR_SCAN_LIMIT = 50

// YTS sort keys, mapped from the UI's short names. An unknown value is a 400 from
// the API, which would look like our API breaking.
const SORTS = {
  trending: 'download_count',
  popularity: 'download_count',
  score: 'rating',
  favourites: 'like_count',
  recent: 'year',
  title: 'title',
  year: 'year',
  rating: 'rating',
}

function toScore(rating) {
  return Number.isFinite(rating) ? Math.round(rating * 10) : null
}

/**
 * Maps a YTS movie onto the MediaList shape the frontend renders.
 *
 * Exported so it can be tested against a captured response without the network.
 */
export function toMediaList(movie) {
  return {
    id: mediaKey('movie', movie.id),
    title: {
      english: movie.title_english || movie.title || null,
      romaji: null,
      native: movie.title || null,
    },
    coverImage: {
      extraLarge: movie.large_cover_image || null,
      large: movie.medium_cover_image || null,
      color: null,
    },
    // YTS publishes a real wide image, so the hero banner works here.
    bannerImage: movie.background_image_original || movie.background_image || null,
    // YTS has no format field: everything in it is a film.
    format: 'MOVIE',
    season: null,
    seasonYear: Number.isFinite(movie.year) ? movie.year : null,
    episodes: 1,
    duration: Number.isFinite(movie.runtime) ? movie.runtime : null,
    status: 'FINISHED',
    averageScore: toScore(movie.rating),
    meanScore: toScore(movie.rating),
    popularity: Number.isFinite(movie.like_count) ? movie.like_count : 0,
    favourites: Number.isFinite(movie.like_count) ? movie.like_count : 0,
    genres: movie.genres || [],
    source: null,
    countryOfOrigin: movie.language || null,
    isAdult: false,
    studios: { nodes: [] },
    nextAiringEpisode: null,
    startDate: { year: Number.isFinite(movie.year) ? movie.year : null, month: null, day: null },
    endDate: { year: null, month: null, day: null },
  }
}

/**
 * Maps a YTS movie detail onto the MediaFull shape.
 *
 * The cast is returned as `characters`, which is the same slot the anime site uses
 * for voice actors, so the detail page renders it without a special case.
 */
export function toMediaFull(movie, suggestions = []) {
  return {
    ...toMediaList(movie),
    idMal: null,
    description: movie.description_full || movie.summary || null,
    trailer: movie.yt_trailer_code ? { id: movie.yt_trailer_code, site: 'youtube' } : null,
    relations: { edges: [] },
    recommendations: {
      nodes: suggestions.slice(0, 12).map((entry) => ({
        rating: null,
        mediaRecommendation: {
          ...toMediaList(entry),
          type: 'ANIME',
          meanScore: toScore(entry.rating),
        },
      })),
    },
    characters: {
      edges: (movie.cast || []).slice(0, 12).map((person) => ({
        role: person.character_name || null,
        node: {
          id: person.imdb_code ? String(person.imdb_code) : null,
          name: { full: person.name || null, native: null },
          image: { large: person.url_small_image || null },
        },
        voiceActors: [],
      })),
    },
    externalLinks: movie.url ? [{ url: movie.url, site: 'YTS', type: 'STREAMING' }] : [],
    streamingEpisodes: [],
    // --- read by the stream layer, not rendered ---
    //
    // The year identifies the film among its remakes, and the IMDb id is passed to
    // the indexers (only EZTV uses it, and only for series).
    year: Number.isFinite(movie.year) ? movie.year : null,
    imdbId: movie.imdb_code || null,
  }
}

// YTS answers 200 with a placeholder movie for an unknown id, so a missing title is
// the signal that the film does not exist.
export function isEmptyMovie(movie) {
  return !movie || !movie.title || !movie.id
}

async function get(path, params) {
  const { data } = await http.get(`${config.ytsBaseUrl}/${path}`, { params, responseType: 'json' })
  if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')
  if (data.status !== 'ok') throw new Error(data.status_message || 'YTS returned an error')
  return data.data || {}
}

function pageVariables(page, perPage) {
  return {
    page: Math.max(1, Number(page) || 1),
    perPage: Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 24)),
  }
}

export default {
  name: 'yts',
  types: ['movie'],

  // --- full shapes, rendered by the frontend -----------------------------------

  async mediaList(filters = {}) {
    const { page, perPage } = pageVariables(filters.page, filters.perPage)
    const genre = filters.genre || filters.genres?.[0]
    const year = Number.parseInt(filters.year, 10)

    // A year cannot be pushed upstream, so a wider page is fetched and filtered here.
    const limit = Number.isFinite(year) ? YEAR_SCAN_LIMIT : perPage

    const data = await cached(
      `yts:list:${filters.sort || ''}:${genre || ''}:${year || ''}:${page}:${limit}`,
      TTL_MS,
      () =>
        get('list_movies.json', {
          sort_by: SORTS[filters.sort] || SORTS.popularity,
          order_by: 'desc',
          limit,
          page,
          ...(genre ? { genre: genre.toLowerCase() } : {}),
          // Excludes the long tail of unreleased and low-rated entries from a browse
          // row. Applied only when browsing, never when searching for a title.
          minimum_rating: 5,
        }),
    )

    const movies = data.movies || []
    const filtered = Number.isFinite(year) ? movies.filter((movie) => movie.year === year) : movies
    const total = Number.isFinite(year) ? filtered.length : data.movie_count || 0

    return {
      page,
      perPage,
      total,
      // A filtered year has no meaningful next page: the upstream page was widened
      // and the remainder of the catalogue was never read.
      hasNextPage: Number.isFinite(year) ? false : page * perPage < total,
      results: filtered.map(toMediaList),
    }
  },

  async mediaSearch(filters = {}) {
    const term = filters.term
    if (!term) {
      return { page: 1, perPage: MAX_PER_PAGE, total: 0, hasNextPage: false, results: [] }
    }

    const { page, perPage } = pageVariables(filters.page, filters.perPage)
    const data = await cached(`yts:search:${term}:${page}:${perPage}`, (value) => ttlFor(value, TTL_MS), () =>
      get('list_movies.json', { query_term: term, limit: perPage, page }),
    )

    // YTS answers a query that matches nothing with `movie_count: 0` and no movies
    // key at all, which is a normal empty result rather than an error.
    const movies = data.movies || []
    const total = data.movie_count || 0

    return {
      page,
      perPage,
      total,
      hasNextPage: page * perPage < total,
      results: movies.map(toMediaList),
    }
  },

  async mediaDetail(id) {
    if (!/^\d+$/.test(String(id ?? ''))) return null

    const data = await cached(`yts:detail:${id}`, TTL_MS, () =>
      get('movie_details.json', { movie_id: id, with_cast: 'true', with_images: 'true' }),
    )
    if (isEmptyMovie(data.movie)) return null

    // The suggestions row is a nicety, so a failure there must not fail the page.
    const suggestions = await cached(`yts:suggestions:${id}`, TTL_MS, () =>
      get('movie_suggestions.json', { movie_id: id }).catch(() => ({})),
    )

    return toMediaFull(data.movie, suggestions.movies || [])
  },

  // --- small shapes ------------------------------------------------------------
  //
  // The movies site uses mediaList/mediaSearch/mediaDetail. This exists so the
  // stream layer's details lookup has one shape to read whichever provider answered.

  async animeDetails(id) {
    const details = await this.mediaDetail(id)
    if (!details) return null
    return {
      id: details.id,
      name: details.title.english,
      type: 'MOVIE',
      status: details.status,
      totalEpisodes: 1,
      released: details.startDate.year,
      genres: details.genres,
      synopsis: details.description,
      imageUrl: details.coverImage.extraLarge,
      trailer: details.trailer?.id ? `https://www.youtube.com/watch?v=${details.trailer.id}` : null,
      titleVariants: [details.title.english, details.title.native].filter(Boolean),
      // Read by the stream layer, not rendered. The year is what separates a film
      // from its remakes when ranking releases.
      year: details.year ?? null,
      imdbId: details.imdbId ?? null,
      episode_id: [],
    }
  },
}
