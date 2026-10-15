// What the site is made of: the three catalogues, their genres and sorts, and the home page's
// rows. Mirrors the desktop app (desktop/app/src/{home,browse}.rs), so the two show the same
// things in the same order.
//
// There is one site. The anime and TV hostnames only redirect to it (see src/middleware.ts).

export type ContentType = "anime" | "series" | "movie";

/** The zone the site is served from. NEXT_PUBLIC_ so the middleware and the browser can read it. */
export const SITE_DOMAIN = process.env.NEXT_PUBLIC_SITE_DOMAIN || "akidev.live";

export const CONTENT_TYPES: ContentType[] = ["anime", "series", "movie"];

/** The attribution the metadata providers require. */
export const ATTRIBUTION =
  "Metadata from AniList, Cinemeta and TVMaze (CC BY-SA). Akidev stores no media: playback links are resolved from public sources when you press play.";

export interface CataloguePage {
  /** The kind this page browses, or null for all three. */
  kind: ContentType | null;
  /** The URL segment. */
  slug: string;
  /** The tab in the top bar, and the page heading. */
  label: string;
}

export const CATALOGUES: CataloguePage[] = [
  { kind: "anime", slug: "anime", label: "Anime" },
  { kind: "series", slug: "series", label: "Series" },
  { kind: "movie", slug: "films", label: "Films" },
];

export function catalogueForSlug(slug: string): CataloguePage | undefined {
  return CATALOGUES.find((page) => page.slug === slug);
}

/** The small label over a title: "ANIME", "SERIES", "FILM". */
export const KIND_LABEL: Record<ContentType, string> = {
  anime: "ANIME",
  series: "SERIES",
  movie: "FILM",
};

export interface Sort {
  value: string;
  label: string;
}

export const SORTS: Sort[] = [
  { value: "popularity", label: "Popular" },
  { value: "trending", label: "Trending" },
  { value: "score", label: "Top rated" },
  { value: "recent", label: "Newest" },
  { value: "title", label: "A–Z" },
];

const ANIME_GENRES = [
  "Action", "Adventure", "Comedy", "Drama", "Ecchi", "Fantasy", "Horror", "Mahou Shoujo",
  "Mecha", "Music", "Mystery", "Psychological", "Romance", "Sci-Fi", "Slice of Life",
  "Sports", "Supernatural", "Thriller",
];

const SCREEN_GENRES = [
  "Action", "Adventure", "Animation", "Biography", "Comedy", "Crime", "Documentary", "Drama",
  "Family", "Fantasy", "History", "Horror", "Music", "Mystery", "Romance", "Sci-Fi", "Sport",
  "Thriller", "War",
];

const SHARED_GENRES = [
  "Action", "Adventure", "Comedy", "Drama", "Fantasy", "Horror", "Mystery", "Romance",
  "Sci-Fi", "Thriller",
];

/**
 * The genres worth offering, in the providers' own spellings. AniList and Cinemeta name their
 * genres differently, and a genre the catalogue lacks would only ever return nothing.
 */
export function genresFor(kind: ContentType | null): string[] {
  if (kind === "anime") return ANIME_GENRES;
  if (kind) return SCREEN_GENRES;
  return SHARED_GENRES;
}

export interface HomeRow {
  title: string;
  kind: ContentType;
  sort: string;
  genre?: string;
}

/** The rows, in order. Each draws from one catalogue, so the page mixes all three. */
export const HOME_ROWS: HomeRow[] = [
  { title: "Trending Anime", kind: "anime", sort: "trending" },
  { title: "New Releases", kind: "movie", sort: "recent" },
  { title: "Popular Series", kind: "series", sort: "popularity" },
  { title: "Newly Released Anime", kind: "anime", sort: "recent" },
  { title: "Most Watched Films", kind: "movie", sort: "popularity" },
  { title: "New Series", kind: "series", sort: "recent" },
  { title: "Top Rated Anime", kind: "anime", sort: "score" },
  { title: "Action Films", kind: "movie", sort: "popularity", genre: "Action" },
  { title: "Drama Series", kind: "series", sort: "score", genre: "Drama" },
  { title: "Romance Anime", kind: "anime", sort: "popularity", genre: "Romance" },
];
