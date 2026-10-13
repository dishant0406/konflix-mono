// Kitsu provider.
//
// The second metadata provider. It exists to prove that the provider map is real:
// setting METADATA_PROVIDER=kitsu swaps the whole metadata layer without touching
// the schema, the resolvers, or the frontend.
//
// Kitsu's episode endpoint does not support filtering by airing status, so recent
// episodes are read from the show's own episode list rather than a global feed.

import { http } from '../../lib/http.js'
import { cached } from '../../lib/cache.js'
import { animeEpisodeId, mediaKey } from '../../lib/mediaId.js'

const ENDPOINT = 'https://kitsu.io/api/edge'
const TTL_MS = 10 * 60 * 1000

async function get(path, params) {
  const { data } = await http.get(`${ENDPOINT}${path}`, { params })
  return data
}

function attributes(entry) {
  return entry.attributes
}

function title(entry) {
  const a = attributes(entry)
  return a.canonicalTitle || a.titles?.en || a.titles?.en_jp
}

function poster(entry) {
  const image = attributes(entry).posterImage
  return image?.large || image?.original || null
}

function trailerUrl(entry) {
  const id = attributes(entry).youtubeVideoId
  return id ? `https://www.youtube.com/watch?v=${id}` : null
}

function toAnime(entry) {
  return {
    id: mediaKey('anime', entry.id),
    title: title(entry),
    trailer: trailerUrl(entry),
    imgUrl: poster(entry),
  }
}

// Kitsu reports `episodeCount: null` for every long-running show (verified live:
// ONE PIECE, Detective Conan and Naruto Shippuuden all return null), so the
// attribute alone cannot be trusted. The episodes collection reports the real
// total in `meta.count`, which is correct even when the attribute is null.
async function episodeTotal(id) {
  const data = await cached(`kitsu:episodeTotal:${id}`, TTL_MS, () =>
    get(`/anime/${id}/episodes`, { 'page[limit]': 1 }),
  )
  return data.meta?.count || 0
}

// --- full media shapes, rendered by the frontend ------------------------------
//
// Kitsu stores the same facts AniList does but spells them differently and
// spreads them across `attributes`, `included` and `meta`. These helpers do that
// translation so both providers produce one shape and the UI never branches on
// which provider is configured.

const MAX_PER_PAGE = 20 // Kitsu rejects page[limit] above 20 with a 400.

// Kitsu's own status vocabulary, mapped to the values the rest of the app uses.
// AniList and Kitsu do not share status names, and the frontend must not have to
// know which provider answered.
const STATUS_FROM_KITSU = {
  current: 'RELEASING',
  finished: 'FINISHED',
  tba: 'NOT_YET_RELEASED',
  unreleased: 'NOT_YET_RELEASED',
  upcoming: 'NOT_YET_RELEASED',
}

const STATUS_TO_KITSU = {
  'currently-airing': 'current',
  'finished-airing': 'finished',
  'not-yet-aired': 'tba',
  cancelled: 'finished',
  hiatus: 'current',
}

// Kitsu has no season column, so it is derived from the start month the way the
// seasonal calendars divide the year.
function seasonOf(isoDate) {
  if (!isoDate) return null
  const month = Number(isoDate.slice(5, 7))
  if (!month) return null
  if (month <= 3) return 'WINTER'
  if (month <= 6) return 'SPRING'
  if (month <= 9) return 'SUMMER'
  return 'FALL'
}

function yearOf(isoDate) {
  return isoDate ? Number(isoDate.slice(0, 4)) || null : null
}

function dateOf(isoDate) {
  if (!isoDate) return { year: null, month: null, day: null }
  return {
    year: yearOf(isoDate),
    month: Number(isoDate.slice(5, 7)) || null,
    day: Number(isoDate.slice(8, 10)) || null,
  }
}

function hostnameOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return 'link'
  }
}

function ratingToScore(averageRating) {
  const value = Number.parseFloat(averageRating)
  return Number.isFinite(value) ? Math.round(value) : null
}

function toMediaList(entry, genres = []) {
  const a = attributes(entry)
  const image = a.posterImage || {}
  return {
    id: mediaKey('anime', entry.id),
    title: {
      romaji: a.titles?.en_jp || a.canonicalTitle || null,
      english: a.titles?.en || a.canonicalTitle || null,
      native: a.titles?.ja_jp || null,
    },
    coverImage: { extraLarge: image.original || image.large || null, large: image.large || null, color: null },
    bannerImage: a.coverImage?.large || null,
    format: a.subtype ? a.subtype.toUpperCase() : null,
    season: seasonOf(a.startDate),
    seasonYear: yearOf(a.startDate),
    episodes: a.episodeCount ?? null,
    duration: a.episodeLength ?? null,
    status: STATUS_FROM_KITSU[a.status] || (a.status ? a.status.toUpperCase() : null),
    averageScore: ratingToScore(a.averageRating),
    meanScore: null,
    popularity: a.userCount ?? 0,
    favourites: a.favoritesCount ?? 0,
    genres,
    source: null,
    countryOfOrigin: 'JP',
    isAdult: Boolean(a.nsfw),
    // Kitsu exposes producers and studios through separate relationships that
    // would cost another request per card; the UI hides the row when empty.
    studios: { nodes: [] },
    nextAiringEpisode: null,
    startDate: dateOf(a.startDate),
    endDate: dateOf(a.endDate),
  }
}

function toMediaFull(entry, included) {
  const a = attributes(entry)
  const byType = (type) => (included || []).filter((item) => item.type === type)
  const animeById = new Map(byType('anime').map((item) => [item.id, item]))
  const genres = byType('categories').map((category) => category.attributes.title)

  const relations = byType('mediaRelationships')
    .map((relationship) => {
      const destination = relationship.relationships?.destination?.data
      const node = destination && animeById.get(destination.id)
      if (!node) return null
      const attrs = attributes(node)
      const image = attrs.posterImage || {}
      return {
        relationType: relationship.attributes.role || null,
        node: {
          id: node.id,
          type: 'ANIME',
          format: attrs.subtype ? attrs.subtype.toUpperCase() : null,
          status: attrs.status ? attrs.status.toUpperCase() : null,
          episodes: attrs.episodeCount ?? null,
          meanScore: ratingToScore(attrs.averageRating),
          // AniList returns these on the same node shape, so Kitsu fills them from
          // its own attributes rather than leaving the key absent.
          averageScore: ratingToScore(attrs.averageRating),
          popularity: attrs.userCount ?? 0,
          genres: [],
          title: {
            romaji: attrs.titles?.en_jp || attrs.canonicalTitle || null,
            english: attrs.titles?.en || attrs.canonicalTitle || null,
            native: attrs.titles?.ja_jp || null,
          },
          coverImage: { extraLarge: image.original || image.large || null, large: image.large || null, color: null },
          startDate: dateOf(attrs.startDate),
        },
      }
    })
    .filter(Boolean)

  // Kitsu has no recommendations endpoint, so the relation list doubles as one.
  // It is the closest honest equivalent and avoids a second round trip per page.
  const recommendations = relations.slice(0, 12).map((edge) => ({
    rating: null,
    mediaRecommendation: edge.node,
  }))

  const characters = byType('characters').map((character) => {
    const attrs = character.attributes || {}
    const image = attrs.image || {}
    return {
      role: null,
      node: {
        id: character.id,
        name: { full: attrs.canonicalName || attrs.name || null, native: attrs.names?.ja_jp || null },
        image: { large: image.large || image.original || null },
      },
      // Kitsu stores castings separately from characters; an extra request per
      // character is not worth it, and the UI hides the row when empty.
      voiceActors: [],
    }
  })

  return {
    ...toMediaList(entry, genres),
    idMal: null,
    description: a.synopsis || a.description || null,
    trailer: a.youtubeVideoId ? { id: a.youtubeVideoId, site: 'youtube' } : null,
    relations: { edges: relations },
    recommendations: { nodes: recommendations },
    characters: { edges: characters },
    externalLinks: byType('streamingLinks')
      .filter((link) => link.attributes?.url)
      .map((link) => ({
        url: link.attributes.url,
        // Kitsu puts the service name in the URL rather than in an attribute, and
        // the UI shows this string as the link label.
        site: hostnameOf(link.attributes.url),
        type: 'STREAMING',
      })),
    streamingEpisodes: [],
  }
}

async function toPage(data, page, perPage) {
  const included = data.included || []
  const categoryById = new Map(
    included.filter((item) => item.type === 'categories').map((item) => [item.id, item.attributes?.title]),
  )

  const entries = data.data.filter((entry) => entry.attributes?.posterImage?.large)

  // Kitsu reports `episodeCount: null` for every long-running show (verified live:
  // One Piece, Detective Conan and Naruto Shippuuden all return null), so those
  // rows would render with no episode list at all. The episodes collection knows
  // the real total, so it is fetched for just the rows that need it.
  const missing = entries.filter((entry) => !entry.attributes?.episodeCount)
  const counts = new Map(
    await Promise.all(
      missing.map(async (entry) => [entry.id, await episodeTotal(entry.id).catch(() => 0)]),
    ),
  )

  const results = entries.map((entry) => {
    const genres = (entry.relationships?.categories?.data || [])
      .map((reference) => categoryById.get(reference.id))
      .filter(Boolean)
    const media = toMediaList(entry, genres)
    if (media.episodes === null) media.episodes = counts.get(entry.id) || null
    return media
  })

  const total = data.meta?.count ?? results.length
  return {
    page,
    perPage,
    total,
    hasNextPage: page * perPage < total,
    results,
  }
}

export default {
  name: 'kitsu',
  types: ['anime'],

  // --- full media shapes, rendered by the frontend -----------------------------

  async mediaList({ sort, genre, genres, format, status, season, year, page, perPage } = {}) {
    const limit = Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 14))
    const currentPage = Math.max(1, Number(page) || 1)
    const params = {
      'page[limit]': limit,
      'page[offset]': (currentPage - 1) * limit,
      include: 'categories',
    }
    if (sort === 'trending' || sort === 'popularity') params.sort = '-userCount'
    else if (sort === 'score') params.sort = '-averageRating'
    else if (sort === 'favourites') params.sort = '-favoritesCount'
    else if (sort === 'recent') params.sort = '-startDate'
    else if (sort === 'title') params.sort = 'canonicalTitle'
    // Kitsu cannot OR several categories in one request, so only the first is
    // applied. Returning fewer results is better than ignoring the filter.
    if (genre) params['filter[categories]'] = genre
    else if (genres?.length) params['filter[categories]'] = genres[0]
    if (format) params['filter[subtype]'] = format.toLowerCase()
    if (status) params['filter[status]'] = STATUS_TO_KITSU[status] || status
    if (season) params['filter[season]'] = season.toLowerCase()
    if (year) params['filter[year]'] = year

    const data = await cached(`kitsu:mediaList:${JSON.stringify(params)}`, TTL_MS, () =>
      get('/anime', params),
    )
    return toPage(data, currentPage, limit)
  },

  async mediaSearch({ term, format, status, season, year, genres, page, perPage } = {}) {
    const limit = Math.min(MAX_PER_PAGE, Math.max(1, Number(perPage) || 14))
    const currentPage = Math.max(1, Number(page) || 1)
    if (!term) {
      return { page: 1, perPage: limit, total: 0, hasNextPage: false, results: [] }
    }
    const params = {
      'filter[text]': term,
      'page[limit]': limit,
      'page[offset]': (currentPage - 1) * limit,
      include: 'categories',
    }
    if (format) params['filter[subtype]'] = format.toLowerCase()
    if (status) params['filter[status]'] = STATUS_TO_KITSU[status] || status
    if (season) params['filter[season]'] = season.toLowerCase()
    if (year) params['filter[year]'] = year
    if (genres?.length) params['filter[categories]'] = genres[0]

    const data = await cached(`kitsu:mediaSearch:${JSON.stringify(params)}`, TTL_MS, () =>
      get('/anime', params),
    )
    return toPage(data, currentPage, limit)
  },

  async mediaDetail(id) {
    // The id reaches here straight from the client. Kitsu ids are numeric, and
    // interpolating anything else into the path would let a caller reach a
    // different endpoint. The AniList provider has the same guard.
    if (!/^\d+$/.test(String(id ?? ''))) return null
    const data = await cached(`kitsu:mediaDetail:${id}`, TTL_MS, () =>
      get(`/anime/${id}`, {
        include: 'categories,mediaRelationships.destination,characters.character,streamingLinks',
      }),
    )
    return data.data ? toMediaFull(data.data, data.included) : null
  },

  // --- small shapes, used by the stream layer ----------------------------------

  async topAnime(page) {
    const data = await cached(`kitsu:top:${page}`, TTL_MS, () =>
      get('/anime', {
        sort: '-userCount',
        'page[limit]': 24,
        'page[offset]': ((page || 1) - 1) * 24,
      }),
    )
    return data.data.map(toAnime)
  },

  async animeSearch(term, page) {
    if (!term) return []
    const data = await cached(`kitsu:search:${term}:${page}`, TTL_MS, () =>
      get('/anime', {
        'filter[text]': term,
        'page[limit]': 24,
        'page[offset]': ((page || 1) - 1) * 24,
      }),
    )
    return data.data.map(toAnime)
  },

  async animeDetails(id) {
    const data = await cached(`kitsu:details:${id}`, TTL_MS, () =>
      get(`/anime/${id}`, { include: 'categories' }),
    )
    const entry = data.data
    if (!entry) return null

    const total = attributes(entry).episodeCount || (await episodeTotal(entry.id))
    return {
      id: mediaKey('anime', entry.id),
      name: title(entry),
      type: attributes(entry).subtype?.toUpperCase() || 'ANIME',
      status: attributes(entry).status?.toUpperCase() || null,
      totalEpisodes: total,
      released: Number.parseInt(attributes(entry).startDate, 10) || null,
      genres: (data.included || []).map((category) => category.attributes.title),
      synopsis: attributes(entry).synopsis,
      imageUrl: poster(entry),
      trailer: trailerUrl(entry),
      titleVariants: [
        attributes(entry).titles?.en,
        attributes(entry).titles?.en_jp,
        attributes(entry).canonicalTitle,
        attributes(entry).titles?.ja_jp,
      ].filter(Boolean),
      episode_id: Array.from({ length: total }, (_, index) => ({
        episodeId: animeEpisodeId(entry.id, index + 1),
        episodeNumber: index + 1,
      })),
    }
  },

// Kitsu has no global "recently aired" endpoint, so the newest episodes are read
  // from the episodes collection. It is sorted by air date descending, and an air
  // date in the future is a scheduled release, so those rows are dropped.
  async recentEpisodes(page) {
    const offset = ((page || 1) - 1) * 20
    const data = await cached(`kitsu:recent:${page}`, 5 * 60 * 1000, () =>
      get('/episodes', {
        sort: '-airdate',
        include: 'media',
        'page[limit]': 20,
        'page[offset]': offset,
      }),
    )

    const mediaById = new Map((data.included || []).map((media) => [media.id, media]))
    const today = new Date().toISOString().slice(0, 10)

    const episodes = []
    for (const entry of data.data || []) {
      const media = mediaById.get(entry.relationships?.media?.data?.id)
      if (!media) continue

      // An episode with no air date is not known to have aired.
      const airDate = entry.attributes.airdate
      if (!airDate || airDate > today) continue

      episodes.push({
        animeId: mediaKey('anime', media.id),
        episodeId: animeEpisodeId(media.id, entry.attributes.number),
        name: title(media),
        episodeNum: entry.attributes.number,
        subOrDub: 'sub',
        imgUrl: poster(media),
      })
    }
    return episodes
  },

  async recommandedAnime(genre) {
    if (!genre) return []
    const data = await cached(`kitsu:genre:${genre}`, TTL_MS, () =>
      get('/anime', { 'filter[categories]': genre, 'page[limit]': 12 }),
    )
    return data.data.map((entry) => ({
      animeId: mediaKey('anime', entry.id),
      animeTitle: title(entry),
      animeImg: poster(entry),
    }))
  },
}
