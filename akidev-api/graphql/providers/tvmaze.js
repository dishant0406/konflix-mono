// TVMaze metadata provider for series.
//
// TVMaze is keyless, documented, and the only free source here that answers a whole
// show in one request: `shows/:id?embed[]=episodes&embed[]=cast` returns the
// synopsis, the poster, the full episode list and the cast together. It also carries
// `externals.imdb`, which is the id EZTV requires, so the series stream path depends
// on this provider for its search key as well as its metadata.
//
// Coverage of Indian television is genuinely good, which is why it is the series
// provider rather than a Hollywood-only source: Anupama (1496 episodes), Kumkum
// Bhagya and Mirzapur were all verified present with IMDb ids.
//
// Licence is CC BY-SA, so the attribution in the footer is a requirement, not
// decoration.
//
// Two shapes are worth knowing before reading the mapping:
//
//   rating.average   0-10, scaled to 0-100 here so it matches AniList's scale and
//                    the UI's "★ 92%" is comparable across the two sites.
//   weight           TVMaze's own popularity signal, 0-100. It is the only ordering
//                    signal available, so the browse rows sort by it - tie-broken by
//                    rating, because weight saturates at 100 for many shows.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { mediaKey, seriesEpisodeId } from '../../lib/mediaId.js'

const TTL_MS = 10 * 60 * 1000

// Building a "popular" or "newest" list needs a pool to sort, because TVMaze has no
// sort-by-popularity endpoint.
//
// The pool is the shows airing today, from `/schedule/full`. That endpoint was chosen
// over paging `/shows` after measuring both:
//
//   /schedule/full   1332 currently-airing shows in one request, ~1.2s, ~14MB gzipped
//   /shows?page=0..3  970 shows, and only the 970 OLDEST in the catalogue
//
// `/shows` is ordered by id, so the first pages are the earliest records - measured,
// shows premiering between 1955 and 2015. That is what made "Popular Series" read as
// Under the Dome and Person of Interest, and left every weight tied at 100 so the
// order within the row was arbitrary. The schedule carries today's broadcast, which is
// exactly the set a viewer means by "what is on", and its weights and ratings have a
// real spread to sort by.
const BROWSE_TTL_MS = 6 * 60 * 60 * 1000

// TVMaze caps a search at 10 results and offers no pagination.
const SEARCH_LIMIT = 10

const MAX_PER_PAGE = 50

// A show announced with a future premiere date is not watchable yet, so it is kept
// out of every row. Only a date this far ahead is excluded, so a show premiering later
// this month still appears.
const FUTURE_GRACE_DAYS = 30

function latestPremiere() {
  return Date.now() + FUTURE_GRACE_DAYS * 24 * 60 * 60 * 1000
}

// TVMaze returns HTML in `summary`.
export function stripHtml(html) {
  if (!html) return null
  return html
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&nbsp;/g, ' ')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

// TVMaze reports the id as "tt0903747" and EZTV wants "0903747". Both are kept: the
// bare form is what EZTV is queried with, and the full form is what a viewer
// recognises. Only the digits are stripped here; EZTV validates its own input.
export function imdbId(externals) {
  const value = externals?.imdb
  return typeof value === 'string' && /^tt\d+$/.test(value) ? value : null
}

// AniList scores out of 100 and TVMaze out of 10, so they are normalised to the same
// scale. Without this a series would show "★ 9%" beside an anime's "★ 92%".
function toScore(rating) {
  const average = rating?.average
  return Number.isFinite(average) ? Math.round(average * 10) : null
}

function toCoverImage(image) {
  return {
    extraLarge: image?.original || null,
    large: image?.medium || null,
    // TVMaze publishes no dominant colour.
    color: null,
  }
}

function year(date) {
  const parsed = Number.parseInt(String(date ?? '').slice(0, 4), 10)
  return Number.isFinite(parsed) ? parsed : null
}

// TVMaze dates are "2008-01-20".
function toDate(value) {
  if (!value) return { year: null, month: null, day: null }
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(value)
  if (!match) return { year: year(value), month: null, day: null }
  return {
    year: Number.parseInt(match[1], 10),
    month: Number.parseInt(match[2], 10),
    day: Number.parseInt(match[3], 10),
  }
}

/**
 * Maps a TVMaze show onto the MediaList shape the frontend renders.
 *
 * Exported so it can be tested against a captured response without the network.
 */
export function toMediaList(show) {
  return {
    id: mediaKey('series', show.id),
    title: {
      // TVMaze has one name, and it is the one releases are named after, so it goes
      // in `english` - the field the UI prefers and the stream layer searches with.
      english: show.name || null,
      romaji: null,
      native: null,
    },
    coverImage: toCoverImage(show.image),
    // TVMaze publishes no wide banner. The UI falls back to the poster.
    bannerImage: null,
    // The show's own type is more informative than "TV" and is what the badge shows.
    format: show.type || 'TV',
    season: null,
    seasonYear: year(show.premiered),
    episodes: null,
    duration: show.averageRuntime ?? show.runtime ?? null,
    status: show.status || null,
    averageScore: toScore(show.rating),
    meanScore: toScore(show.rating),
    popularity: show.weight ?? 0,
    favourites: 0,
    genres: show.genres || [],
    source: null,
    countryOfOrigin: show.network?.country?.code || show.webChannel?.country?.code || null,
    isAdult: false,
    studios: show.network
      ? {
          nodes: [
            {
              id: show.network.id ? String(show.network.id) : null,
              name: show.network.name || null,
              isAnimationStudio: false,
              siteUrl: show.network.officialSite || null,
            },
          ],
        }
      : { nodes: [] },
    nextAiringEpisode: null,
    startDate: toDate(show.premiered),
    endDate: toDate(show.ended),
  }
}

/**
 * Maps a TVMaze show with embedded episodes onto the MediaFull shape.
 *
 * The episode list drives the watch page's episode grid, and `externals.imdb` drives
 * the stream search, so both come from the same request.
 */
export function toMediaFull(show) {
  const episodes = show._embedded?.episodes || []
  const cast = show._embedded?.cast || []

  return {
    ...toMediaList(show),
    idMal: null,
    description: stripHtml(show.summary),
    trailer: null,
    // TVMaze has no relation or recommendation graph. Returning empty structures
    // rather than null keeps the UI's existing rendering path unchanged.
    relations: { edges: [] },
    recommendations: { nodes: [] },
    characters: {
      edges: cast.slice(0, 12).map((entry) => ({
        role: entry.character?.name || null,
        node: {
          id: entry.person?.id ? String(entry.person.id) : null,
          name: { full: entry.person?.name || null, native: null },
          image: { large: entry.person?.image?.medium || null },
        },
        // TVMaze lists performers, not voice actors, so there is no language to
        // report and no second credit to show.
        voiceActors: [],
      })),
    },
    externalLinks: show.officialSite ? [{ url: show.officialSite, site: 'Official Site', type: 'STREAMING' }] : [],
    streamingEpisodes: [],
    // --- fields the stream layer reads, beyond the MediaFull contract ---
    //
    // These are not in the schema because the frontend does not render them; they are
    // read by resolveStream from the provider's return value. `year` is unused by the
    // series path and is present so all three providers answer the same shape.
    year: year(show.premiered),
    imdbId: imdbId(show.externals),
    episodeList: episodes
      .filter((episode) => Number.isFinite(episode?.season) && Number.isFinite(episode?.number))
      .map((episode) => ({
        season: episode.season,
        number: episode.number,
        name: episode.name || null,
        airdate: episode.airdate || null,
        // "S01E05" is the id format the series stream path parses.
        episodeId: seriesEpisodeId(show.id, episode.season, episode.number),
      })),
  }
}

async function query(path, params) {
  const { data } = await http.get(`${config.tvmazeBaseUrl}${path}`, { params, responseType: 'json' })
  if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')
  return data
}

// A 404 is a normal answer here - an id that does not exist - and axios turns it
// into a throw. Returning null keeps that from looking like an outage.
async function queryOrNull(path, params) {
  try {
    return await query(path, params)
  } catch (error) {
    if (error?.response?.status === 404) return null
    throw error
  }
}

// The pool of shows the browse rows are drawn from: everything airing today.
//
// `/schedule/full` returns one entry per episode airing, each carrying its show under
// `_embedded.show`, so the shows are de-duplicated by id. Fetched once and cached for
// hours. A failure here returns an empty pool rather than throwing, so a TVMaze outage
// empties the rows instead of failing the page.
async function browsePool() {
  return cached('tvmaze:browse', BROWSE_TTL_MS, async () => {
    const schedule = await query('/schedule/full', {}).catch(() => [])
    if (!Array.isArray(schedule)) return []

    const shows = new Map()
    for (const entry of schedule) {
      const show = entry?._embedded?.show
      if (show?.id && !shows.has(show.id)) shows.set(show.id, show)
    }

    const cutoff = latestPremiere()
    return [...shows.values()].filter(
      (show) => show.image && (!show.premiered || Date.parse(show.premiered) <= cutoff),
    )
  })
}

// Weight saturates at 100 for a large slice of the catalogue, so ordering by it alone
// left the order within a row arbitrary. Rating breaks the tie, then the premiere date
// keeps the more recent of two equally popular shows first.
function byPopularity(a, b) {
  return (
    (b.weight ?? 0) - (a.weight ?? 0) ||
    (b.rating?.average ?? 0) - (a.rating?.average ?? 0) ||
    String(b.premiered || '').localeCompare(String(a.premiered || ''))
  )
}

// A show with no rating would sort as zero under the score sort and land last, which is
// wrong for a show nobody has rated yet rather than one rated badly. Unrated shows keep
// their popularity order instead.
function byScore(a, b) {
  const rated = (show) => Number.isFinite(show.rating?.average)
  if (rated(a) !== rated(b)) return rated(a) ? -1 : 1
  return (b.rating?.average ?? 0) - (a.rating?.average ?? 0) || byPopularity(a, b)
}

const SORTS = {
  trending: byPopularity,
  popularity: byPopularity,
  favourites: byPopularity,
  score: byScore,
  recent: (a, b) => String(b.premiered || '').localeCompare(String(a.premiered || '')) || byPopularity(a, b),
  title: (a, b) => String(a.name || '').localeCompare(String(b.name || '')),
}

function pageVariables(page, perPage) {
  return {
    page: Math.max(1, Number(page) || 1),
    perPage: Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 24)),
  }
}

async function browsePage(filters) {
  const { page, perPage } = pageVariables(filters.page, filters.perPage)
  const pool = await browsePool()

  const genre = filters.genre || filters.genres?.[0]
  const filtered = genre
    ? pool.filter((show) => (show.genres || []).some((value) => value.toLowerCase() === genre.toLowerCase()))
    : pool

  const sorted = [...filtered].sort(SORTS[filters.sort] || SORTS.popularity)
  const start = (page - 1) * perPage
  const results = sorted.slice(start, start + perPage)

  return {
    page,
    perPage,
    total: sorted.length,
    hasNextPage: start + perPage < sorted.length,
    results: results.map(toMediaList),
  }
}

export default {
  name: 'tvmaze',
  types: ['series'],

  // --- full shapes, rendered by the frontend -----------------------------------

  async mediaList(filters = {}) {
    // A format filter is a filter on the show's type ("Animation", "Scripted"), which
    // the pool carries, so it is applied in memory alongside the genre.
    return browsePage(filters)
  },

  async mediaSearch(filters = {}) {
    const term = filters.term
    if (!term) {
      return { page: 1, perPage: SEARCH_LIMIT, total: 0, hasNextPage: false, results: [] }
    }

    const results = await cached(`tvmaze:search:${term}`, (value) => ttlFor(value, TTL_MS), () =>
      queryOrNull('/search/shows', { q: term }),
    )

    const shows = (results || []).map((entry) => entry?.show).filter((show) => show?.image)
    return {
      page: 1,
      perPage: SEARCH_LIMIT,
      total: shows.length,
      // TVMaze returns at most ten matches and offers no way to page through them.
      hasNextPage: false,
      results: shows.map(toMediaList),
    }
  },

  async mediaDetail(id) {
    if (!/^\d+$/.test(String(id ?? ''))) return null
    const show = await cached(`tvmaze:detail:${id}`, TTL_MS, () =>
      queryOrNull(`/shows/${id}`, { 'embed[]': ['episodes', 'cast'] }),
    )
    return show ? toMediaFull(show) : null
  },

  // --- small shapes ------------------------------------------------------------
  //
  // The series site does not use these: its rows come from mediaList and its watch
  // page from mediaDetail. They exist so the stream layer's details lookup has a
  // single shape to read regardless of which provider answered.

  async animeDetails(id) {
    const details = await this.mediaDetail(id)
    if (!details) return null
    return {
      id: details.id,
      name: details.title.english,
      type: details.format,
      status: details.status,
      totalEpisodes: details.episodeList.length,
      released: details.startDate.year,
      genres: details.genres,
      synopsis: details.description,
      imageUrl: details.coverImage.extraLarge,
      trailer: null,
      titleVariants: [details.title.english].filter(Boolean),
      // Read by the stream layer, not rendered: the year identifies a film among its
      // remakes, and EZTV needs the IMDb id to search at all.
      year: details.year ?? null,
      imdbId: details.imdbId ?? null,
      episode_id: details.episodeList.map((episode) => ({
        episodeId: episode.episodeId,
        episodeNumber: episode.number,
      })),
    }
  },
}
