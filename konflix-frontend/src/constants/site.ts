// Site configuration.
//
// One codebase builds two sites. `SITE` selects which, and everything that differs
// between them - the copy, the row titles, the genres, the content type sent to the
// API - is declared here rather than scattered as conditionals.
//
// `SITE` is a *build* argument, not a runtime variable, so each build is a plain
// static app with no host detection and no per-request branching. The API address is
// identical for both, which is why one image cannot serve both only in the sense that
// Next bakes NEXT_PUBLIC_* at build time - the value is the same either way, so a
// second build is all that is needed.

export type ContentType = "anime" | "series" | "movie";

export interface SiteConfig {
  /** Which catalogue this site browses. Sent to the API on every query. */
  contentType: ContentType;
  appName: string;
  title: string;
  description: string;
  /** The word for one item, used in empty states and labels. */
  itemLabel: string;
  itemLabelPlural: string;
  /**
   * Rows on the home page, in order. Each row names the catalogue it draws from, so
   * a site with two catalogues can interleave them.
   */
  rows: { title: string; sort?: string; genre?: string; contentType?: ContentType }[];
  /** Row titles for the detail page. */
  relatedTitle: string;
  recommendedTitle: string;
  /** Genre names the search filter offers. These are the provider's own values. */
  genres: string[];
  /** Filter values for the search page. */
  formats: { value: string; label: string }[];
  statuses: { value: string; label: string }[];
  sorts: { value: string; label: string }[];
  /**
   * Whether the Type, Status, Season and Year filters apply.
   *
   * They are AniList-only concepts: TVMaze has no format or status filter, and YTS
   * ignores the year parameter. Offering them elsewhere would be a control that
   * silently does nothing.
   */
  supportsAdvancedFilters: boolean;
  /**
   * The catalogues a search should span, in order.
   *
   * Searching has to cover every catalogue the site carries, because the user has no
   * idea which provider holds a title: on the TV site "Breaking Bad" is a series and
   * "Dangal" is a film, and typing either into the search box should find it. A single
   * content type would silently answer from one catalogue only.
   *
   * The first entry is also the catalogue a browse with no term uses, so it doubles as
   * the site's default. It is normally the same as `contentType`.
   */
  searchContentTypes: ContentType[];
  /** The attribution the metadata provider requires, if any. */
  attribution: string;
}

const ANIME: SiteConfig = {
  contentType: "anime",
  appName: "Konflix",
  title: "Konflix | Anime Streaming",
  description: "Browse and stream anime, with metadata from AniList.",
  itemLabel: "anime",
  itemLabelPlural: "anime",
  rows: [
    { title: "Trending Now", sort: "trending" },
    { title: "Newly Released", sort: "recent" },
    { title: "Top Rated", sort: "score" },
    { title: "Action", genre: "Action", sort: "trending" },
    { title: "Romance", genre: "Romance", sort: "popularity" },
  ],
  relatedTitle: "Related",
  recommendedTitle: "Recommended",
  genres: [
    "Action", "Adventure", "Comedy", "Drama", "Ecchi", "Fantasy", "Horror",
    "Mahou Shoujo", "Mecha", "Music", "Mystery", "Psychological", "Romance",
    "Sci-Fi", "Slice of Life", "Sports", "Supernatural", "Thriller",
  ],
  formats: [
    { value: "tv", label: "TV" },
    { value: "movie", label: "Movie" },
    { value: "ova", label: "OVA" },
    { value: "ona", label: "ONA" },
    { value: "special", label: "Special" },
    { value: "music", label: "Music" },
  ],
  statuses: [
    { value: "currently-airing", label: "Currently Airing" },
    { value: "finished-airing", label: "Finished Airing" },
    { value: "not-yet-aired", label: "Not Yet Aired" },
  ],
  sorts: [
    { value: "popularity", label: "Most Popular" },
    { value: "trending", label: "Trending" },
    { value: "score", label: "Highest Score" },
    { value: "favourites", label: "Most Favourited" },
    { value: "recent", label: "Newest" },
    { value: "title", label: "Title A-Z" },
  ],
  supportsAdvancedFilters: true,
  searchContentTypes: ["anime"],
  attribution: "Metadata from AniList.",
};

const TV: SiteConfig = {
  // Films are the default for this site, but the rows below interleave series, so
  // the page stays useful when either source is having a bad day.
  contentType: "movie",
  appName: "Konflix TV",
  title: "Konflix TV | Movies and Series",
  description: "Browse and stream films and series, Hollywood and Bollywood.",
  itemLabel: "title",
  itemLabelPlural: "titles",
  rows: [
    { title: "New Releases", sort: "recent", contentType: "movie" },
    { title: "Popular Series", sort: "popularity", contentType: "series" },
    { title: "Most Watched Films", sort: "popularity", contentType: "movie" },
    { title: "New Series", sort: "recent", contentType: "series" },
    { title: "Action", genre: "action", sort: "popularity", contentType: "movie" },
    { title: "Drama Series", genre: "Drama", sort: "score", contentType: "series" },
  ],
  relatedTitle: "More Like This",
  recommendedTitle: "Recommended",
  // YTS genre values are lowercase, unlike AniList's.
  genres: [
    "action", "adventure", "animation", "biography", "comedy", "crime",
    "documentary", "drama", "family", "fantasy", "history", "horror",
    "music", "mystery", "romance", "sci-fi", "sport", "thriller", "war",
  ],
  formats: [
    { value: "movie", label: "Film" },
    { value: "series", label: "Series" },
  ],
  statuses: [
    { value: "currently-airing", label: "Currently Airing" },
    { value: "finished-airing", label: "Finished" },
  ],
  sorts: [
    { value: "popularity", label: "Most Watched" },
    { value: "score", label: "Highest Rated" },
    { value: "recent", label: "Newest" },
    { value: "title", label: "Title A-Z" },
  ],
  supportsAdvancedFilters: false,
  searchContentTypes: ["series", "movie"],
  attribution:
    "Film metadata from YTS. Series metadata from TVMaze, licensed CC BY-SA.",
};

export const SITE: SiteConfig = process.env.NEXT_PUBLIC_SITE === "tv" ? TV : ANIME;
