// AniList GraphQL provider.
//
// AniList is the default metadata source: it is a real, documented, rate limited
// API (30 requests/minute) with no authentication required.
//
// Two families of functions live here:
//
//   topAnime / animeSearch / animeDetails / recentEpisodes / recommandedAnime
//     The small shapes declared in schema.graphql. Used by the stream layer and
//     kept because they are the stable contract that layer depends on.
//
//   mediaList / mediaSearch / mediaDetail
//     The full AniList `Media` object, normalised into the shape the frontend
//     renders. AniList is the source of truth for those fields, so this is a
//     near-passthrough rather than a translation - which is why the UI can render
//     AniList's banners, scores, studios, relations and recommendations directly.

import { http } from '../../lib/http.js'
import { cached } from '../../lib/cache.js'
import { animeEpisodeId, mediaKey } from '../../lib/mediaId.js'

const ENDPOINT = 'https://graphql.anilist.co'
const TTL_MS = 10 * 60 * 1000

// AniList caps a page at 50 and the frontend never asks for more.
const MAX_PER_PAGE = 50

// The sort is an enum upstream, so it is mapped from a short name rather than
// passed through. An unknown value would be a 400 from AniList, which would look
// like our API breaking.
const SORTS = {
  trending: 'TRENDING_DESC',
  popularity: 'POPULARITY_DESC',
  score: 'SCORE_DESC',
  favourites: 'FAVOURITES_DESC',
  recent: 'START_DATE_DESC',
  title: 'TITLE_ROMAJI',
}

// AniList's filter arguments are enums, so a UI value has to be mapped. The maps
// also accept the enum itself, because the GraphQL schema types these as plain
// strings and a client could send either form.
const FORMATS = { tv: 'TV', movie: 'MOVIE', ova: 'OVA', ona: 'ONA', special: 'SPECIAL', music: 'MUSIC' }
const STATUSES = {
  'currently-airing': 'RELEASING',
  'finished-airing': 'FINISHED',
  'not-yet-aired': 'NOT_YET_RELEASED',
  cancelled: 'CANCELLED',
  hiatus: 'HIATUS',
}
const SEASONS = { winter: 'WINTER', spring: 'SPRING', summer: 'SUMMER', fall: 'FALL' }

async function query(document, variables) {
  const { data } = await http.post(ENDPOINT, { query: document, variables })
  if (data.errors) {
    throw new Error(`AniList: ${data.errors.map((error) => error.message).join('; ')}`)
  }
  return data.data
}

// AniList descriptions are HTML. The small shapes declare a plain String and the
// frontend renders them directly, so tags and entities have to go.
export function stripHtml(html) {
  if (!html) return null
  return html
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&quot;/g, '"')
    .replace(/&#039;/g, "'")
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&mdash;/g, '—')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function trailerUrl(trailer) {
  if (!trailer || trailer.site !== 'youtube' || !trailer.id) return null
  return `https://www.youtube.com/watch?v=${trailer.id}`
}

// AniList reports `episodes: null` for shows that are still airing, so the
// currently airing episode is the best available count.
export function episodeCount(media) {
  if (media.episodes) return media.episodes
  if (media.nextAiringEpisode) return Math.max(media.nextAiringEpisode.episode - 1, 0)
  return 0
}

// A short display title. AniList gives three; the English one is the most useful
// to a viewer and the romaji is the reliable fallback.
export function displayTitle(title) {
  if (!title) return 'Untitled'
  return title.english || title.romaji || title.native || 'Untitled'
}

// The fields the UI renders for a card, a carousel or a hero banner.
const LIST_FIELDS = `
  id
  title { romaji english native }
  coverImage { extraLarge large color }
  bannerImage
  format
  season
  seasonYear
  episodes
  duration
  status
  averageScore
  meanScore
  popularity
  favourites
  genres
  source
  countryOfOrigin
  isAdult
  studios(isMain: true) { nodes { id name isAnimationStudio siteUrl } }
  nextAiringEpisode { episode airingAt timeUntilAiring }
  startDate { year month day }
  endDate { year month day }
`

// The detail page adds the fields only it needs. Kept separate so a card grid does
// not pay for 12 characters and 12 recommendations per item.
const DETAIL_FIELDS = `
  ${LIST_FIELDS}
  idMal
  description(asHtml: false)
  trailer { id site }
  relations {
    edges {
      relationType
      node {
        id
        type
        format
        status
        episodes
        meanScore
        title { romaji english native }
        coverImage { extraLarge large }
        startDate { year }
      }
    }
  }
  recommendations(sort: RATING_DESC, perPage: 12) {
    nodes {
      rating
      mediaRecommendation {
        id
        format
        status
        episodes
        meanScore
        averageScore
        popularity
        genres
        title { romaji english native }
        coverImage { extraLarge large }
        startDate { year }
      }
    }
  }
  characters(sort: [ROLE, RELEVANCE], perPage: 12) {
    edges {
      role
      node { id name { full native } image { large } }
      voiceActors(language: JAPANESE) { id name { full } image { large } languageV2 }
    }
  }
  externalLinks { url site type }
  streamingEpisodes { title thumbnail url site }
`

const PAGE_INFO = 'pageInfo { total currentPage lastPage hasNextPage perPage }'

function pageVariables(page, perPage) {
  return {
    page: Math.max(1, Number(page) || 1),
    perPage: Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 24)),
  }
}

// AniList returns null for most of these on some titles, and the UI reads them
// directly, so the shape is fixed here rather than left to the caller.
export function toMediaList(media) {
  return {
    // The id names its content type, so the frontend can route from the URL alone.
    id: mediaKey('anime', media.id),
    title: media.title,
    coverImage: media.coverImage,
    bannerImage: media.bannerImage || null,
    format: media.format || null,
    season: media.season || null,
    seasonYear: media.seasonYear || null,
    episodes: media.episodes ?? null,
    duration: media.duration ?? null,
    status: media.status || null,
    averageScore: media.averageScore ?? null,
    meanScore: media.meanScore ?? null,
    popularity: media.popularity ?? 0,
    favourites: media.favourites ?? 0,
    genres: media.genres || [],
    source: media.source || null,
    countryOfOrigin: media.countryOfOrigin || null,
    isAdult: Boolean(media.isAdult),
    studios: media.studios || { nodes: [] },
    nextAiringEpisode: media.nextAiringEpisode || null,
    startDate: media.startDate || { year: null, month: null, day: null },
    endDate: media.endDate || { year: null, month: null, day: null },
  }
}

export function toMediaFull(media) {
  return {
    ...toMediaList(media),
    idMal: media.idMal ?? null,
    description: stripHtml(media.description),
    trailer: media.trailer || null,
    relations: media.relations || { edges: [] },
    recommendations: media.recommendations || { nodes: [] },
    characters: media.characters || { edges: [] },
    externalLinks: media.externalLinks || [],
    streamingEpisodes: media.streamingEpisodes || [],
  }
}

function toPage(data, page, perPage) {
  const info = data.Page.pageInfo
  return {
    page: info.currentPage,
    perPage: info.perPage,
    total: info.total,
    hasNextPage: Boolean(info.hasNextPage),
    // Some rows carry no cover image, which would render as broken cards.
    results: data.Page.media.filter((media) => media?.coverImage?.large).map(toMediaList),
  }
}


// AniList treats an explicitly null filter as "match nothing" rather than "no
// filter". Sending every argument with a null default therefore returned zero
// results for every filtered query, which is why only the filters that were
// actually set are put into the document.
const MEDIA_FILTERS = {
  genre: { type: 'String', arg: 'genre' },
  genres: { type: '[String]', arg: 'genre_in' },
  format: { type: 'MediaFormat', arg: 'format' },
  status: { type: 'MediaStatus', arg: 'status' },
  season: { type: 'MediaSeason', arg: 'season' },
  year: { type: 'Int', arg: 'seasonYear' },
  search: { type: 'String', arg: 'search' },
}

function mediaFilterArgs(filters) {
  const declarations = []
  const args = []
  const variables = {}

  for (const [name, spec] of Object.entries(MEDIA_FILTERS)) {
    const value = filters[name]
    if (value === undefined || value === null || value === '') continue
    if (Array.isArray(value) && value.length === 0) continue
    declarations.push(`$${name}: ${spec.type}`)
    args.push(`${spec.arg}: $${name}`)
    variables[name] = value
  }

  return { declarations, args, variables }
}

function normaliseFilters(filters) {
  const normalised = {}
  if (filters.genre) normalised.genre = filters.genre
  if (filters.genres?.length) normalised.genres = filters.genres
  if (filters.format) normalised.format = FORMATS[filters.format] || filters.format
  if (filters.status) normalised.status = STATUSES[filters.status] || filters.status
  if (filters.season) normalised.season = SEASONS[filters.season] || filters.season
  if (filters.year) normalised.year = Number(filters.year) || undefined
  if (filters.term) normalised.search = filters.term
  return normalised
}

async function mediaPage(filters) {
  const { page, perPage } = pageVariables(filters.page, filters.perPage)
  const sort = SORTS[filters.sort] || SORTS.popularity
  const { declarations, args, variables } = mediaFilterArgs(normaliseFilters(filters))

  const document = `query ($page: Int, $perPage: Int, $sort: [MediaSort]${declarations
    .map((declaration) => `, ${declaration}`)
    .join('')}) {
    Page(page: $page, perPage: $perPage) {
      ${PAGE_INFO}
      media(type: ANIME, sort: $sort, isAdult: false${args
        .map((arg) => `, ${arg}`)
        .join('')}) { ${LIST_FIELDS} }
    }
  }`

  const key = `anilist:mediaPage:${sort}:${JSON.stringify(variables)}:${page}:${perPage}`
  const data = await cached(key, TTL_MS, () =>
    query(document, { page, perPage, sort, ...variables }),
  )
  return toPage(data, page, perPage)
}

export default {
  name: 'anilist',
  types: ['anime'],

  // --- full media shapes, rendered by the frontend -----------------------------

  async mediaList(filters) {
    return mediaPage(filters || {})
  },

  async mediaSearch(filters) {
    // With no term AniList returns the whole catalogue, which is not what an
    // empty search box means. An empty page is the honest answer.
    if (!filters?.term) {
      return {
        page: 1,
        perPage: Math.min(MAX_PER_PAGE, Math.max(1, Number(filters?.perPage) || 14)),
        total: 0,
        hasNextPage: false,
        results: [],
      }
    }
    return mediaPage(filters)
  },

  async mediaDetail(id) {
    if (!/^\d+$/.test(String(id ?? ''))) return null
    const data = await cached(`anilist:mediaDetail:${id}`, TTL_MS, () =>
      query(`query ($id: Int) { Media(id: $id, type: ANIME) { ${DETAIL_FIELDS} } }`, {
        id: Number(id),
      }),
    )
    return data.Media ? toMediaFull(data.Media) : null
  },

  // --- small shapes, used by the stream layer ----------------------------------

  async topAnime(page) {
    const data = await cached(`anilist:top:${page}`, TTL_MS, () =>
      query(
        `query ($page: Int) {
          Page(page: $page, perPage: 24) {
            media(type: ANIME, sort: TRENDING_DESC, isAdult: false) {
              id
              title { romaji english }
              coverImage { extraLarge large }
              trailer { id site }
            }
          }
        }`,
        { page: page || 1 },
      ),
    )
    return data.Page.media.map((media) => ({
      id: mediaKey('anime', media.id),
      title: displayTitle(media.title),
      trailer: trailerUrl(media.trailer),
      imgUrl: media.coverImage.extraLarge || media.coverImage.large,
    }))
  },

  async animeSearch(term, page) {
    if (!term) return []
    const data = await cached(`anilist:search:${term}:${page}`, TTL_MS, () =>
      query(
        `query ($search: String, $page: Int) {
          Page(page: $page, perPage: 24) {
            media(type: ANIME, search: $search, isAdult: false) {
              id
              title { romaji english }
              coverImage { extraLarge large }
              trailer { id site }
            }
          }
        }`,
        { search: term, page: page || 1 },
      ),
    )
    return data.Page.media.map((media) => ({
      id: mediaKey('anime', media.id),
      title: displayTitle(media.title),
      trailer: trailerUrl(media.trailer),
      imgUrl: media.coverImage.extraLarge || media.coverImage.large,
    }))
  },

  async animeDetails(id) {
    const data = await cached(`anilist:details:${id}`, TTL_MS, () =>
      query(
        `query ($id: Int) {
          Media(id: $id, type: ANIME) {
            id
            title { romaji english native }
            coverImage { extraLarge large }
            trailer { id site }
            genres
            status
            format
            episodes
            nextAiringEpisode { episode }
            startDate { year }
            description(asHtml: false)
          }
        }`,
        { id: Number(id) },
      ),
    )
    const media = data.Media
    if (!media) return null

    const total = episodeCount(media)
    return {
      id: mediaKey('anime', media.id),
      name: displayTitle(media.title),
      type: media.format || 'ANIME',
      status: media.status,
      totalEpisodes: total,
      released: media.startDate.year,
      genres: media.genres,
      synopsis: stripHtml(media.description),
      imageUrl: media.coverImage.extraLarge || media.coverImage.large,
      trailer: trailerUrl(media.trailer),
      titleVariants: [media.title.english, media.title.romaji, media.title.native].filter(Boolean),
      episode_id: Array.from({ length: total }, (_, index) => ({
        episodeId: animeEpisodeId(media.id, index + 1),
        episodeNumber: index + 1,
      })),
    }
  },

  async recentEpisodes(page) {
    const data = await cached(`anilist:recent:${page}`, 5 * 60 * 1000, () =>
      query(
        `query ($page: Int) {
          Page(page: $page, perPage: 50) {
            airingSchedules(notYetAired: false, sort: TIME_DESC) {
              episode
              media { id title { romaji english } coverImage { extraLarge large } }
            }
          }
        }`,
        { page: page || 1 },
      ),
    )

    const seen = new Set()
    const episodes = []
    for (const entry of data.Page.airingSchedules) {
      const key = `${entry.media.id}-${entry.episode}`
      if (seen.has(key)) continue
      seen.add(key)
      episodes.push({
        animeId: mediaKey('anime', entry.media.id),
        episodeId: animeEpisodeId(entry.media.id, entry.episode),
        name: displayTitle(entry.media.title),
        episodeNum: entry.episode,
        subOrDub: 'sub',
        imgUrl: entry.media.coverImage.extraLarge || entry.media.coverImage.large,
      })
    }
    return episodes
  },

  async recommandedAnime(genre) {
    if (!genre) return []
    const data = await cached(`anilist:genre:${genre}`, TTL_MS, () =>
      query(
        `query ($genre: String) {
          Page(page: 1, perPage: 12) {
            media(type: ANIME, genre: $genre, sort: TRENDING_DESC, isAdult: false) {
              id
              title { romaji english }
              coverImage { extraLarge large }
            }
          }
        }`,
        { genre },
      ),
    )
    return data.Page.media.map((media) => ({
      animeId: mediaKey('anime', media.id),
      animeTitle: displayTitle(media.title),
      animeImg: media.coverImage.extraLarge || media.coverImage.large,
    }))
  },
}
