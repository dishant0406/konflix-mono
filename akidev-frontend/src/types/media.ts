// The shapes the API returns.
//
// These mirror desktop/server/API.md, which is the contract; nothing here is invented locally.

export type Kind = "anime" | "series" | "movie";

/** One title, flat. Everything a tile, row or banner draws from. */
export interface Media {
  /** The title key, e.g. "anime-16498". */
  id: string;
  kind: Kind;
  title: string;
  poster: string | null;
  banner: string | null;
  description: string | null;
  format: string | null;
  year: number | null;
  episodes: number | null;
  runtime_minutes: number | null;
  /** A provider's own word (FINISHED, RELEASING, Ended, ...), so display text. */
  status: string | null;
  /** 0 to 100. */
  score: number | null;
  popularity: number;
  genres: string[];
  is_adult: boolean;
}

export interface Page {
  results: Media[];
  has_next: boolean;
}

export interface Episode {
  /** Self-describing, e.g. "series-tt0903747-s1e5". */
  id: string;
  season: number;
  number: number;
  name: string | null;
  airdate: string | null;
}

export interface Person {
  name: string;
  role: string | null;
  image: string | null;
}

export interface Trailer {
  site: string;
  id: string;
}

/** A title in full. `episodes` is empty for a film and for an anime, which counts 1..media.episodes. */
export interface Detail {
  media: Media;
  studios: string[];
  trailer: Trailer | null;
  episodes: Episode[];
  cast: Person[];
  related: Media[];
  recommended: Media[];
}

export interface Subtitle {
  label: string;
  lang: string;
  url: string;
}

export type PlayResult =
  | {
      available: true;
      url: string;
      /** "hls" when the container needed converting, "file" when the browser plays it directly. */
      stream_type: "file" | "hls";
      release: string | null;
      source: string | null;
      /** Seconds, when known. */
      duration: number | null;
      subtitles: Subtitle[];
    }
  | { available: false; reason: string };

export interface HomeRow {
  title: string;
  kind: Kind;
  /** Empty when the row's provider failed; the page leaves such a row out. */
  items: Media[];
}

export interface HomeResponse {
  featured: Media[];
  rows: HomeRow[];
}
