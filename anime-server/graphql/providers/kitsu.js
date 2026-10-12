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
    id: entry.id,
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

export default {
  name: 'kitsu',

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
      id: entry.id,
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
        episodeId: `${entry.id}-ep${index + 1}`,
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
        animeId: media.id,
        episodeId: `${media.id}-ep${entry.attributes.number}`,
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
      animeId: entry.id,
      animeTitle: title(entry),
      animeImg: poster(entry),
    }))
  },
}
