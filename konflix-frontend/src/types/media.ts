// The media shapes the API returns.
//
// These mirror graphql/typeDefs/schema.graphql in anime-server. The API is the
// contract; nothing here is invented locally.

export type MediaFormat = "TV" | "MOVIE" | "OVA" | "ONA" | "SPECIAL" | "MUSIC";
export type MediaStatus = "RELEASING" | "FINISHED" | "NOT_YET_RELEASED" | "CANCELLED" | "HIATUS";
export type MediaSeason = "WINTER" | "SPRING" | "SUMMER" | "FALL";

export interface MediaTitle {
  romaji: string | null;
  english: string | null;
  native: string | null;
}

export interface MediaCoverImage {
  extraLarge: string | null;
  large: string | null;
  color: string | null;
}

export interface MediaDate {
  year: number | null;
  month: number | null;
  day: number | null;
}

export interface MediaStudio {
  id: string | null;
  name: string | null;
  isAnimationStudio: boolean | null;
  siteUrl: string | null;
}

export interface MediaNextAiringEpisode {
  episode: number | null;
  airingAt: number | null;
  timeUntilAiring: number | null;
}

export interface MediaList {
  id: string;
  title: MediaTitle | null;
  coverImage: MediaCoverImage | null;
  bannerImage: string | null;
  format: MediaFormat | null;
  season: MediaSeason | null;
  seasonYear: number | null;
  episodes: number | null;
  duration: number | null;
  status: MediaStatus | null;
  averageScore: number | null;
  meanScore: number | null;
  popularity: number | null;
  favourites: number | null;
  genres: string[] | null;
  source: string | null;
  countryOfOrigin: string | null;
  isAdult: boolean | null;
  studios: { nodes: MediaStudio[] | null } | null;
  nextAiringEpisode: MediaNextAiringEpisode | null;
  startDate: MediaDate | null;
  endDate: MediaDate | null;
}

export interface MediaPage {
  page: number | null;
  perPage: number | null;
  total: number | null;
  hasNextPage: boolean | null;
  results: MediaList[] | null;
}

export interface MediaRelationNode {
  id: string | null;
  type: string | null;
  format: MediaFormat | null;
  status: MediaStatus | null;
  episodes: number | null;
  meanScore: number | null;
  title: MediaTitle | null;
  coverImage: MediaCoverImage | null;
  startDate: MediaDate | null;
}

export interface MediaFull extends MediaList {
  idMal: number | null;
  description: string | null;
  trailer: { id: string | null; site: string | null } | null;
  relations: { edges: { relationType: string | null; node: MediaRelationNode | null }[] | null } | null;
  recommendations: { nodes: { rating: number | null; mediaRecommendation: MediaRelationNode | null }[] | null } | null;
  characters: {
    edges:
      | {
          role: string | null;
          node: { id: string | null; name: { full: string | null; native: string | null } | null; image: { large: string | null } | null } | null;
          voiceActors: { id: string | null; name: { full: string | null; native: string | null } | null; image: { large: string | null } | null; languageV2: string | null }[] | null;
        }[]
      | null;
  } | null;
  externalLinks: { url: string | null; site: string | null; type: string | null }[] | null;
  streamingEpisodes: { title: string | null; thumbnail: string | null; url: string | null; site: string | null }[] | null;
}

export interface StreamLink {
  id: string | null;
  url: string | null;
  /** "hls" when the container needed converting, "file" when the browser plays it directly. */
  streamType: string | null;
  available: boolean | null;
  reason: string | null;
  release: string | null;
  animeNameWithEP: string | null;
  ep_num: number | null;
}
