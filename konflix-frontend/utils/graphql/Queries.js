import { gql } from 'graphql-tag'

const GET_TOP_ANIME = gql`
  query TopAnime($page: Int) {
    topAnime(page: $page) {
      imgUrl
      title
      trailer
      id
    }
  }
`

const GET_RECENT_ANIME = gql`
  query RecentEpisodes($page: Int) {
    recentEpisodes(page: $page) {
      animeId
      name
      episodeId
      episodeNum
      imgUrl
    }
  }
`

const GET_SINGLE_ANIME = gql`
  query AnimeDetails($animeDetailsId: ID) {
    animeDetails(id: $animeDetailsId) {
      id
      genres
      status
      name
      released
      totalEpisodes
      synopsis
      trailer
      type
      imageUrl
      episode_id {
        episodeId
        episodeNumber
      }
    }
  }
`

// Stream resolution only. Metadata is a separate, much faster query
// (GET_SINGLE_ANIME), so the page can render the title and episode list while a
// torrent is still being resolved.
const GET_STREAM_LINK = gql`
  query StreamLink($streamLinkDetailsId: ID) {
    streamLinkDetails(id: $streamLinkDetailsId) {
      url
      streamType
      available
      reason
      release
      animeNameWithEP
    }
  }
`

const GET_SEARCH_ANIME = gql`
  query AnimeSearch($animeSearchId: ID) {
    animeSearch(id: $animeSearchId) {
      imgUrl
      id
      title
    }
  }
`

const GET_RECOMMEDED_ANIME = gql`
  query RecommandedAnime($genre: String) {
    recommandedAnime(genre: $genre) {
      animeTitle
      animeId
      animeImg
    }
  }
`

export {
  GET_TOP_ANIME,
  GET_RECENT_ANIME,
  GET_SINGLE_ANIME,
  GET_STREAM_LINK,
  GET_SEARCH_ANIME,
  GET_RECOMMEDED_ANIME,
}
