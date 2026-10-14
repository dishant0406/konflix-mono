// GraphQL documents are plain strings. The fragments are interpolated at module
// load, which is why they are ordinary template literals and not a tagged template:
// a tag function would receive the un-substituted source and every document would
// contain a literal "${...}".
const gql = String.raw;

export { gql };

// One fragment per shape, so the field list exists in exactly one place and the
// TypeScript types cannot drift from what is actually requested.

const MEDIA_LIST_FIELDS = `
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
  studios { nodes { id name isAnimationStudio siteUrl } }
  nextAiringEpisode { episode airingAt timeUntilAiring }
  startDate { year month day }
  endDate { year month day }
`;

const MEDIA_PAGE_FIELDS = `
  page
  perPage
  total
  hasNextPage
  results { ${MEDIA_LIST_FIELDS} }
`;

export const GET_MEDIA_LIST = gql`
  query MediaList($input: MediaListInput) {
    mediaList(input: $input) { ${MEDIA_PAGE_FIELDS} }
  }
`;

export const GET_MEDIA_SEARCH = gql`
  query MediaSearch($term: String, $input: MediaListInput) {
    mediaSearch(term: $term, input: $input) { ${MEDIA_PAGE_FIELDS} }
  }
`;

export const GET_MEDIA_DETAIL = gql`
  query MediaDetail($id: ID) {
    mediaDetail(id: $id) {
      ${MEDIA_LIST_FIELDS}
      idMal
      description
      trailer { id site }
      relations {
        edges {
          relationType
          node { id type format status episodes meanScore title { romaji english native } coverImage { extraLarge large } startDate { year } }
        }
      }
      recommendations {
        nodes {
          rating
          mediaRecommendation { id type format status episodes meanScore averageScore popularity genres title { romaji english native } coverImage { extraLarge large } startDate { year } }
        }
      }
      characters {
        edges {
          role
          node { id name { full native } image { large } }
          voiceActors { id name { full } image { large } languageV2 }
        }
      }
      externalLinks { url site type }
      streamingEpisodes { title thumbnail url site }
      episodeList { episodeId season number name airdate }
    }
  }
`;

// Metadata for the watch page: the episode list and the title, without the detail
// page's characters and relations, so it arrives as fast as possible.
export const GET_WATCH_METADATA = gql`
  query WatchMetadata($id: ID) {
    mediaDetail(id: $id) {
      id
      title { romaji english native }
      coverImage { extraLarge large }
      bannerImage
      format
      episodes
      status
      nextAiringEpisode { episode }
      genres
      description
      episodeList { episodeId season number name airdate }
    }
  }
`;

// Stream resolution only. It is a separate query because it is much slower: the
// page renders the title and episode list while this is still running.
export const GET_STREAM_LINK = gql`
  query StreamLink($id: ID) {
    streamLinkDetails(id: $id) {
      url
      streamType
      available
      reason
      release
      titleWithEpisode
      episodeNumber
    }
  }
`;
