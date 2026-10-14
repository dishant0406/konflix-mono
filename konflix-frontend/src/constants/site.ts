// Site configuration.
//
// One codebase serves three hostnames. `siteForHost` decides which from the request's
// host, and everything that differs between the sites - the copy, the row titles, the
// genres, the content type sent to the API - is declared here rather than scattered as
// conditionals.
//
// The host is read per request, not baked at build time, so there is exactly one
// frontend build and one frontend container. The bare zone resolves to no site, which
// is how the service picker gets its own page.

export type ContentType = "anime" | "series" | "movie";

/**
 * The zone the sites are served from.
 *
 * One value for the whole deployment rather than a per-site one, which is what lets a
 * single build serve every hostname. NEXT_PUBLIC_ so the middleware and the browser
 * bundle can both read it; the value is identical everywhere, so nothing about it is
 * per-site.
 */
export const SITE_DOMAIN = process.env.NEXT_PUBLIC_SITE_DOMAIN || "konflix.live";

export interface SiteConfig {
  /** The subdomain this site answers on, e.g. "anime" for anime.konflix.live. */
  subdomain: string;
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
  subdomain: "anime",
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
  subdomain: "tv",
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
    { title: "Action", genre: "Action", sort: "popularity", contentType: "movie" },
    { title: "Drama Series", genre: "Drama", sort: "score", contentType: "series" },
  ],
  relatedTitle: "More Like This",
  recommendedTitle: "Recommended",
  // Cinemeta's own genre values, title-case. They are matched case-insensitively by
  // the provider, and they are the values sent as the filter, so they must be the
  // provider's spelling rather than a display name.
  genres: [
    "Action", "Adventure", "Animation", "Biography", "Comedy", "Crime",
    "Documentary", "Drama", "Family", "Fantasy", "History", "Horror",
    "Music", "Mystery", "Romance", "Sci-Fi", "Sport", "Thriller", "War",
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
    "Film and series metadata from Cinemeta. Series metadata also from TVMaze, licensed CC BY-SA.",
};

/**
 * The hostnames each site answers on.
 *
 * The canonical subdomain for each site, plus the dev-server ports, so both sites are
 * reachable locally without an /etc/hosts entry. `siteForHost` normalises the request
 * host down to a bare hostname before looking it up, so the port suffix is part of the
 * key rather than something stripped.
 */
const HOSTS: Record<string, SiteConfig> = {
  [`anime.${SITE_DOMAIN}`]: ANIME,
  "localhost:3000": ANIME,
  [`tv.${SITE_DOMAIN}`]: TV,
  "localhost:3001": TV,
};

/**
 * Marks a request the middleware has rewritten to the service picker.
 *
 * A rewrite changes the URL the render sees, so by the time the layout runs the host is
 * the server's own address rather than the one the browser sent, and the picker host
 * would resolve to whichever site that address happens to map to. The middleware sets
 * this header instead of relying on the host surviving the rewrite.
 */
export const PICKER_HEADER = "x-konflix-picker";

/** Which site a request host belongs to, or null for the service picker. */
export function siteForHost(host: string | null | undefined): SiteConfig | null {
  if (!host) return null;
  return HOSTS[host.trim().toLowerCase()] ?? null;
}

/** The canonical origin of a site, used to link between the two. */
export function siteUrl(site: SiteConfig): string {
  return `https://${site.subdomain}.${SITE_DOMAIN}`;
}

/**
 * The label for each site on the picker.
 *
 * Declared next to the sites rather than derived from them, because the picker needs a
 * blurb and a poster that a site's own config has no reason to carry.
 */
export const PICKER_SITES: {
  site: SiteConfig;
  blurb: string;
  /** A few titles to show as a preview, looked up live by the picker. */
  genre: string;
}[] = [
  { site: ANIME, blurb: "Series and films from Japan, subbed and dubbed.", genre: "Action" },
  { site: TV, blurb: "Hollywood and Bollywood films, plus series.", genre: "Action" },
];
