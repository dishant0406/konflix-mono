// AniList GraphQL provider.
//
// AniList is the default metadata source: it is a real, documented, rate limited
// API (30 requests/minute) with no authentication required. Every function below
// returns the shape declared in graphql/typeDefs/schema.graphql, so swapping this
// file for another provider changes nothing else in the codebase.

import { http } from '../../lib/http.js'
import { cached } from '../../lib/cache.js'

const ENDPOINT = 'https://graphql.anilist.co'
const TTL_MS = 10 * 60 * 1000

async function query(document, variables) {
  const { data } = await http.post(ENDPOINT, { query: document, variables })
  if (data.errors) {
    throw new Error(`AniList: ${data.errors.map((error) => error.message).join('; ')}`)
  }
  return data.data
}

// AniList descriptions are HTML. The schema declares a plain String, and the
// frontend renders it directly, so tags and entities have to go.
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

const MEDIA_FIELDS = `
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
`

function toAnime(media) {
  return {
    id: String(media.id),
    title: media.title.english || media.title.romaji,
    trailer: trailerUrl(media.trailer),
    imgUrl: media.coverImage.extraLarge || media.coverImage.large,
  }
}

export default {
  name: 'anilist',

  async topAnime(page) {
    const data = await cached(`anilist:top:${page}`, TTL_MS, () =>
      query(
        `query ($page: Int) {
          Page(page: $page, perPage: 24) {
            media(type: ANIME, sort: TRENDING_DESC) { ${MEDIA_FIELDS} }
          }
        }`,
        { page: page || 1 },
      ),
    )
    return data.Page.media.map(toAnime)
  },

  async animeSearch(term, page) {
    if (!term) return []
    const data = await cached(`anilist:search:${term}:${page}`, TTL_MS, () =>
      query(
        `query ($search: String, $page: Int) {
          Page(page: $page, perPage: 24) {
            media(type: ANIME, search: $search) { ${MEDIA_FIELDS} }
          }
        }`,
        { search: term, page: page || 1 },
      ),
    )
    return data.Page.media.map(toAnime)
  },

  async animeDetails(id) {
    const data = await cached(`anilist:details:${id}`, TTL_MS, () =>
      query(`query ($id: Int) { Media(id: $id, type: ANIME) { ${MEDIA_FIELDS} } }`, {
        id: Number(id),
      }),
    )
    const media = data.Media
    if (!media) return null

    const total = episodeCount(media)
    return {
      id: String(media.id),
      name: media.title.english || media.title.romaji,
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
        episodeId: `${media.id}-ep${index + 1}`,
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
        animeId: String(entry.media.id),
        episodeId: `${entry.media.id}-ep${entry.episode}`,
        name: entry.media.title.english || entry.media.title.romaji,
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
            media(type: ANIME, genre: $genre, sort: TRENDING_DESC) {
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
      animeId: String(media.id),
      animeTitle: media.title.english || media.title.romaji,
      animeImg: media.coverImage.extraLarge || media.coverImage.large,
    }))
  },
}
