// What the music API (desktop/server/API.md, /v1/music) sends, and the addresses built from it.

const BASE = (process.env.NEXT_PUBLIC_API_URL || "http://localhost:9000").replace(/\/+$/, "");

export interface Credit {
  id: string | null;
  name: string;
}

export interface Track {
  /** The YouTube video id, which is also what the stream is asked for by. */
  id: string;
  title: string;
  artists: Credit[];
  album: Credit | null;
  /** "3:49", as YouTube Music writes it. */
  duration: string | null;
  explicit: boolean;
  cover: string | null;
}

export interface AlbumCard {
  id: string;
  title: string;
  artist: string;
  year: string | null;
  kind: string | null;
  cover: string | null;
}

export interface ArtistCard {
  id: string;
  name: string;
  subscribers: string | null;
  cover: string | null;
}

export interface MusicSearch {
  songs: Track[];
  albums: AlbumCard[];
  artists: ArtistCard[];
}

export interface Album {
  id: string;
  title: string;
  kind: string | null;
  year: string;
  duration: string;
  artists: Credit[];
  description: string | null;
  cover: string | null;
  tracks: Track[];
}

export interface Artist {
  id: string;
  name: string;
  description: string | null;
  subscribers: string | null;
  cover: string | null;
  songs: Track[];
  albums: AlbumCard[];
  singles: AlbumCard[];
  related: ArtistCard[];
}

/** Where the audio element gets a track. The server finds the audio and passes it on. */
export function streamUrl(trackId: string): string {
  return `${BASE}/v1/music/stream/${encodeURIComponent(trackId)}`;
}

export const MUSIC_ROUTE = "/music";

export const musicSearchPath = (term: string) => `${MUSIC_ROUTE}?q=${encodeURIComponent(term)}`;
export const albumPath = (id: string) => `${MUSIC_ROUTE}/album/${encodeURIComponent(id)}`;
export const artistPath = (id: string) => `${MUSIC_ROUTE}/artist/${encodeURIComponent(id)}`;

/** Seconds as "3:05", or "1:02:03" past an hour. */
export function clock(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const whole = Math.floor(seconds);
  const h = Math.floor(whole / 3600);
  const m = Math.floor((whole % 3600) / 60);
  const s = String(whole % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export function names(credits: Credit[]): string {
  return credits.map((credit) => credit.name).join(", ");
}
