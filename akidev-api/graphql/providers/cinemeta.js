// Cinemeta metadata provider for films and series.
//
// Cinemeta is the catalogue Stremio itself uses, and it is keyless. It answers the
// two questions this application needs that a torrent tracker cannot:
//
//   - what films and shows exist (a catalogue), as opposed to what has been ripped
//   - what each one is (synopsis, cast, rating, trailer, runtime, episode list)
//
// YTS, the previous film provider, is a torrent tracker: its catalogue is the set of
// films it has released. Searching it for a film nobody has ripped returned zero
// results even while the indexers held 277-seeded releases for it, which is why a
// newly released film was unfindable.
//
// Three hosts sit behind one base URL, and the service redirects between them:
//
//   v3-cinemeta.strem.io        the documented entry point
//   cinemeta-live.strem.io      meta
//   cinemeta-catalogs.strem.io  catalogs
//
// axios follows the redirect, so one configured base URL is enough. It is
// configurable for the same reason every other third-party host here is: this
// service has moved before.
//
// Two response shapes are mapped:
//
//   catalog/{type}/{id}.json   { metas: [...] }            one page of cards
//   meta/{type}/{id}.json      { meta: {...} }             one title in full
//
// Ids are IMDb ids ("tt32261958"), which is also the key EZTV requires - so the
// series stream path gets its search key from this provider for free.

import { http } from '../../lib/http.js'
import { config } from '../../lib/config.js'
import { cached, ttlFor } from '../../lib/cache.js'
import { mediaKey, seriesEpisodeId } from '../../lib/mediaId.js'

const TTL_MS = 10 * 60 * 1000

// A catalogue page is the same for everyone and changes daily, so it is cached far
// longer than a search.
const BROWSE_TTL_MS = 6 * 60 * 60 * 1000

// Cinemeta returns 48-50 rows a page and pages with `skip`.
const PAGE_SIZE = 48

const MAX_PER_PAGE = 50

// The catalogues the browse rows are drawn from, by the UI's sort name.
//
// `top` is ordered by popularity and is what a "Trending" or "Popular" row means.
// `year` is ordered by release year, which is the only catalogue that is actually
// ordered by date - `top` leads with whatever is popular, including announcements.
const CATALOGS = {
  trending: 'top',
  popularity: 'top',
  favourites: 'top',
  title: 'top',
  score: 'imdbRating',
  recent: 'year',
  year: 'year',
}

// A catalogue page holds 48 rows. Filtering by genre happens here rather than
// upstream, because Cinemeta 404s on a request that combines `skip` and `genre` in
// separate path segments - verified against the live service. Widening the page and
// filtering in memory is the only form that works.
const FILTER_SCAN_PAGES = 3

function currentYear() {
  return new Date().getFullYear()
}

/**
 * The year a title is from, or null.
 *
 * A series reports a range ("2008-2013") and a film reports a single year
 * ("2026"). Only the first year is wanted: it is what a release name carries.
 */
export function parseReleaseInfo(releaseInfo) {
  const match = /(\d{4})/.exec(String(releaseInfo ?? ''))
  return match ? Number.parseInt(match[1], 10) : null
}

// The end of a range, so a finished series can report when it ended.
function endYear(releaseInfo) {
  const match = /(\d{4})\s*[-–—]\s*(\d{4})/.exec(String(releaseInfo ?? ''))
  return match ? Number.parseInt(match[2], 10) : null
}

// "143 min" -> 143. Cinemeta has no other unit.
export function parseRuntime(runtime) {
  const match = /(\d+)/.exec(String(runtime ?? ''))
  return match ? Number.parseInt(match[1], 10) : null
}

// "9.5" -> 95, so the scale matches AniList's and the two sites show comparable
// scores. Cinemeta omits the field for an unrated title.
function toScore(rating) {
  const value = Number.parseFloat(rating)
  return Number.isFinite(value) ? Math.round(value * 10) : null
}

// A date as the schema's MediaDate. Cinemeta sends ISO timestamps.
function toDate(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(value ?? ''))
  if (!match) return { year: null, month: null, day: null }
  return {
    year: Number.parseInt(match[1], 10),
    month: Number.parseInt(match[2], 10),
    day: Number.parseInt(match[3], 10),
  }
}

// Cinemeta's own status vocabulary, mapped onto the one the frontend renders.
function toStatus(meta) {
  if (meta.type !== 'series') return null
  const status = String(meta.status ?? '').toLowerCase()
  if (status.includes('ended')) return 'FINISHED'
  if (status.includes('cancel')) return 'CANCELLED'
  if (status.includes('running') || status.includes('continuing')) return 'RELEASING'
  return null
}

// A title announced for a date that has not arrived is not watchable, so it is kept
// out of every row.
//
// Two signals, because neither is complete: `released` is an exact date and is
// present for most entries, while `releaseInfo` is the only field an announcement
// carries. Checking both is what stops a 2027 film leading a "New Releases" row -
// the same defect that made the series and anime rows wrong.
export function isReleased(meta, now = Date.now()) {
  const released = Date.parse(String(meta.released ?? ''))
  if (Number.isFinite(released)) return released <= now
  const year = parseReleaseInfo(meta.releaseInfo)
  return year === null || year <= currentYear()
}

// The IMDb id a Cinemeta entry is addressed by.
//
// Both fields are checked because either can be missing, and an entry with neither
// would produce the id "movie-undefined" - which parses under the media-id grammar and
// becomes a routable URL that 404s.
function imdbIdOf(meta) {
  const value = meta?.imdb_id || meta?.id
  return typeof value === 'string' && value ? value : null
}

// An entry with no id cannot be routed to, so it is dropped rather than rendered as a
// card that goes nowhere.
export function usableMetas(metas) {
  return (Array.isArray(metas) ? metas : []).filter((meta) => imdbIdOf(meta) !== null)
}

// The poster Cinemeta publishes is small and the wide background is what the hero
// banner wants, so the two come from different fields.
function toCoverImage(meta) {
  return {
    extraLarge: meta.poster || null,
    large: meta.poster || null,
    color: null,
  }
}

/**
 * Maps a Cinemeta catalog entry onto the MediaList shape the frontend renders.
 *
 * Exported so it can be tested against a captured response without the network.
 */
export function toMediaList(meta) {
  const year = parseReleaseInfo(meta.releaseInfo)

  return {
    id: mediaKey(meta.type === 'series' ? 'series' : 'movie', imdbIdOf(meta)),
    title: {
      // Cinemeta has one name and it is the name releases are published under, so it
      // goes in `english` - the field the UI prefers and the stream layer searches.
      english: meta.name || null,
      romaji: null,
      native: null,
    },
    coverImage: toCoverImage(meta),
    bannerImage: meta.background || null,
    format: meta.type === 'series' ? 'TV' : 'MOVIE',
    season: null,
    seasonYear: year,
    episodes: null,
    duration: parseRuntime(meta.runtime),
    status: toStatus(meta),
    averageScore: toScore(meta.imdbRating),
    meanScore: toScore(meta.imdbRating),
    popularity: Number.isFinite(meta.popularity) ? Math.round(meta.popularity * 100) : 0,
    favourites: 0,
    genres: meta.genres || meta.genre || [],
    source: null,
    // Cinemeta reports a country name ("United States") where every other provider
    // here reports a code ("US"). Rather than mix two vocabularies in one field, the
    // code is left unset.
    countryOfOrigin: null,
    isAdult: false,
    studios: { nodes: [] },
    nextAiringEpisode: null,
    startDate: { year, month: null, day: null },
    endDate: { year: endYear(meta.releaseInfo), month: null, day: null },
  }
}

// A person credit as the character edge the UI renders.
//
// `credits_cast` carries the character name and a photo, `cast` carries names only.
// Both are mapped so a title with only one of them still shows a cast.
function toCharacterEdges(meta) {
  const credited = meta.credits_cast
  if (Array.isArray(credited) && credited.length > 0) {
    return credited.slice(0, 12).map((person) => ({
      role: person.character || null,
      node: {
        id: person.id ? String(person.id) : null,
        name: { full: person.name || null, native: null },
        // Cinemeta returns a TMDB path, not a URL.
        image: { large: person.profile_path ? `https://image.tmdb.org/t/p/w185${person.profile_path}` : null },
      },
      voiceActors: [],
    }))
  }

  return (meta.cast || []).slice(0, 12).map((name) => ({
    role: null,
    node: {
      id: null,
      name: { full: name || null, native: null },
      image: { large: null },
    },
    voiceActors: [],
  }))
}

// The episode list, from the `videos` array on a series.
//
// Season 0 is specials and minisodes - Breaking Bad carries nine of them - and they
// are excluded because the count is what the watch page uses as the episode total.
// Including them reported 71 episodes for a 62-episode show.
export function toEpisodeList(meta) {
  const id = imdbIdOf(meta)
  if (id === null) return []
  return (meta.videos || [])
    .filter((video) => Number.isFinite(video?.season) && Number.isFinite(video?.number))
    .filter((video) => video.season >= 1)
    .map((video) => ({
      season: video.season,
      number: video.number,
      name: video.name || video.title || null,
      airdate: video.released || video.firstAired || null,
      // "S01E05" is the id format the series stream path parses.
      episodeId: seriesEpisodeId(id, video.season, video.number),
    }))
}

/**
 * Maps a Cinemeta meta onto the MediaFull shape the detail page renders.
 */
export function toMediaFull(meta) {
  const episodes = toEpisodeList(meta)
  const trailer = (meta.trailers || [])[0]

  return {
    ...toMediaList(meta),
    // A series reports its episode count here, where the catalog entry has none.
    episodes: episodes.length || null,
    idMal: null,
    description: meta.description || null,
    trailer: trailer?.source ? { id: trailer.source, site: 'YouTube' } : null,
    // Cinemeta carries no relation or recommendation graph. Empty structures rather
    // than null keep the UI's rendering path unchanged, as with TVMaze.
    relations: { edges: [] },
    recommendations: { nodes: [] },
    characters: { edges: toCharacterEdges(meta) },
    externalLinks: meta.imdb_id
      ? [{ url: `https://www.imdb.com/title/${meta.imdb_id}/`, site: 'IMDb', type: 'STREAMING' }]
      : [],
    streamingEpisodes: [],
    // --- fields the stream layer reads, beyond the MediaFull contract ---
    //
    // Not in the schema because the frontend does not render them; resolveStream
    // reads them from the provider's return value.
    year: parseReleaseInfo(meta.releaseInfo),
    imdbId: imdbIdOf(meta),
    episodeList: episodes,
  }
}

// The catalog path for a page of results.
//
// The extra is one path segment and only one: `skip=24` or `genre=Comedy`, never
// both. Combining them in separate segments 404s.
function catalogPath(type, catalog, { skip = 0, genre = null, search = null } = {}) {
  const extra = search
    ? `search=${encodeURIComponent(search)}`
    : genre
      ? `genre=${encodeURIComponent(genre)}`
      : skip > 0
        ? `skip=${skip}`
        : null
  const suffix = extra ? `/${extra}` : ''
  return `/catalog/${type}/${catalog}${suffix}.json`
}

async function query(path) {
  const { data } = await http.get(`${config.cinemetaBaseUrl}${path}`, { responseType: 'json' })
  if (typeof data !== 'object' || data === null) throw new Error('not a JSON response')
  return data
}

// A title that does not exist is a 404, which axios turns into a throw. Returning
// null keeps a missing title from looking like an outage.
async function queryOrNull(path) {
  try {
    return await query(path)
  } catch (error) {
    if (error?.response?.status === 404) return null
    throw error
  }
}

function catalogType(contentType) {
  return contentType === 'series' ? 'series' : 'movie'
}

// One catalogue page, cached. A failure returns an empty page rather than throwing,
// so an outage empties the rows instead of failing the page.
//
// The catch is inside the loader rather than on the result: `cached` returns the
// promise on a miss but the resolved value on a hit, so chaining `.catch` onto it
// works once and then throws "catch is not a function".
async function catalogPage(type, catalog, skip) {
  return cached(`cinemeta:${type}:${catalog}:${skip}`, (value) => ttlFor(value, BROWSE_TTL_MS), async () => {
    try {
      const data = await queryOrNull(catalogPath(type, catalog, { skip }))
      return data?.metas || []
    } catch (error) {
      console.warn(`[cinemeta] catalog ${type}/${catalog}/${skip} failed: ${error.message}`)
      return []
    }
  })
}

function pageVariables(page, perPage) {
  return {
    page: Math.max(1, Number(page) || 1),
    perPage: Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 24)),
  }
}

// Newest first, when the catalogue is the date-ordered one.
function byReleased(a, b) {
  return String(b.released || b.releaseInfo || '').localeCompare(String(a.released || a.releaseInfo || ''))
}

export default {
  name: 'cinemeta',
  types: ['movie', 'series'],

  // --- full shapes, rendered by the frontend -----------------------------------

  async mediaList(filters = {}) {
    const { page, perPage } = pageVariables(filters.page, filters.perPage)
    const type = catalogType(filters.contentType)
    const catalog = CATALOGS[filters.sort] || CATALOGS.popularity
    const genre = filters.genre || filters.genres?.[0]
    const year = Number.parseInt(filters.year, 10)

    // A genre, a year and the date sort are all applied here rather than upstream,
    // so a window of pages is read and the answer is paged within it.
    //
    // A genre and a year need it because Cinemeta 404s on a request that combines
    // `skip` with either. The date catalogue needs it because its ordering is not
    // contiguous: measured, page 1 of `/catalog/movie/year` mixes 2020, 2025 and 2026
    // and page 2 repeats rows from page 1, so upstream `skip` paging produced six
    // duplicate rows across two pages. Reading a window and sorting it is the only
    // form that yields a stable newest-first list.
    const windowed = Boolean(genre) || Number.isFinite(year) || catalog === 'year'

    const offset = windowed ? 0 : (page - 1) * perPage
    const scans = windowed ? FILTER_SCAN_PAGES : Math.ceil(perPage / PAGE_SIZE) + 1

    const pages = await Promise.all(
      Array.from({ length: scans }, (_, index) => catalogPage(type, catalog, offset + index * PAGE_SIZE)),
    )

    const seen = new Set()
    const rows = pages
      .flat()
      // A title can appear on two scanned pages, and a duplicate id in a React list
      // is a rendering bug rather than a cosmetic one.
      .filter((meta) => {
        const id = imdbIdOf(meta)
        if (id === null || seen.has(id)) return false
        seen.add(id)
        return true
      })
      .filter((meta) => isReleased(meta))
      .filter((meta) =>
        genre ? (meta.genres || []).some((value) => value.toLowerCase() === genre.toLowerCase()) : true,
      )
      .filter((meta) => (Number.isFinite(year) ? parseReleaseInfo(meta.releaseInfo) === year : true))

    const sorted = catalog === 'year' ? [...rows].sort(byReleased) : rows


    if (!windowed) {
      // Whether the catalogue continues is decided by the raw pages, before dedupe and
      // the released filter. Comparing the filtered count instead reported "no next
      // page" whenever a page happened to contain a future-dated title, which is
      // common on the popularity catalogue.
      const lastPage = pages[pages.length - 1] || []
      return {
        page,
        perPage,
        // The popularity catalogue offers no total, and reading a wider window to
        // count one would cost several requests for a number nothing displays.
        total: null,
        hasNextPage: lastPage.length >= PAGE_SIZE,
        results: sorted.slice(0, perPage).map(toMediaList),
      }
    }

    // A windowed row is paged in memory, so its pages are contiguous and its count is
    // the size of the window rather than of the whole catalogue.
    const start = (page - 1) * perPage
    return {
      page,
      perPage,
      total: sorted.length,
      hasNextPage: start + perPage < sorted.length,
      results: sorted.slice(start, start + perPage).map(toMediaList),
    }
  },

  async mediaSearch(filters = {}) {
    const term = filters.term
    const type = catalogType(filters.contentType)
    if (!term) {
      return { page: 1, perPage: MAX_PER_PAGE, total: 0, hasNextPage: false, results: [] }
    }

    const metas = await cached(`cinemeta:search:${type}:${term}`, (value) => ttlFor(value, TTL_MS), async () => {
      try {
        const data = await queryOrNull(catalogPath(type, 'top', { search: term }))
        return usableMetas(data?.metas)
      } catch (error) {
        console.warn(`[cinemeta] search ${type}/${term} failed: ${error.message}`)
        return []
      }
    })

    // The search catalogue ignores `skip` and caps its own results, so there is no
    // next page to offer.
    return {
      page: 1,
      perPage: MAX_PER_PAGE,
      total: metas.length,
      hasNextPage: false,
      results: metas.map(toMediaList),
    }
  },

  /**
   * A title in full.
   *
   * @param {string} id an IMDb id.
   * @param {string} [contentType] 'movie' or 'series'. The meta endpoint is keyed by
   *   both id and type, and an IMDb id alone does not say which - so the caller,
   *   which always knows, passes it.
   */
  async mediaDetail(id, contentType) {
    const value = String(id ?? '')
    // Cinemeta ids are IMDb ids, and the type decides which endpoint answers.
    if (!/^tt\d+$/.test(value)) return null

    const type = catalogType(contentType)
    const meta = await cached(`cinemeta:detail:${type}:${value}`, TTL_MS, () =>
      queryOrNull(`/meta/${type}/${value}.json`).then((data) => data?.meta || null),
    )
    return meta ? toMediaFull(meta) : null
  },

  // --- small shapes ------------------------------------------------------------
  //
  // Read by the stream layer: `name`, `titleVariants`, `year` and `imdbId`.

  async animeDetails(id, contentType) {
    const details = await this.mediaDetail(id, contentType)
    if (!details) return null
    return {
      id: details.id,
      name: details.title.english,
      type: details.format,
      status: details.status,
      totalEpisodes: details.episodeList.length || 1,
      released: details.startDate.year,
      genres: details.genres,
      synopsis: details.description,
      imageUrl: details.coverImage.extraLarge,
      trailer: details.trailer?.id ? `https://www.youtube.com/watch?v=${details.trailer.id}` : null,
      titleVariants: [details.title.english].filter(Boolean),
      // Read by the stream layer, not rendered. The year separates a film from its
      // remakes when ranking, and the IMDb id is how EZTV is searched at all.
      year: details.year ?? null,
      imdbId: details.imdbId ?? null,
      episode_id: details.episodeList.map((episode) => ({
        episodeId: episode.episodeId,
        episodeNumber: episode.number,
      })),
    }
  },
}
