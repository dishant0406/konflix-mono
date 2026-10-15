// Episode lookups shared by the title and watch pages. Plain functions over the API's
// `Detail`, so they carry no React and can be tested alone.

import type { Detail } from "../types/media.ts";
import { animeEpisodeId, parseMediaId } from "./media-id.ts";

/** What plays when Play is pressed: the film itself, or the first episode. */
export function firstPlayable(key: string, detail: Detail): string | null {
  const parsed = parseMediaId(key);
  if (!parsed) return null;
  if (parsed.contentType === "movie") return key;
  if (parsed.contentType === "series") return detail.episodes[0]?.id ?? null;
  return animeEpisodeId(parsed.mediaId, 1);
}

/** How many episodes a title shows: an anime's count, or a series' list when it has no count. */
export function episodeCount(detail: Detail | null | undefined): number {
  return detail ? (detail.media.episodes ?? detail.episodes.length) : 0;
}

/** The episode before (-1) or after (1) this one, if there is one. */
export function neighbour(detail: Detail | null | undefined, id: string | undefined, delta: number): string | null {
  const parsed = parseMediaId(id);
  if (!detail || !parsed || parsed.contentType === "movie") return null;

  if (parsed.contentType === "anime") {
    const next = (parsed.episodeNumber ?? 0) + delta;
    return next >= 1 && next <= (detail.media.episodes ?? 0) ? animeEpisodeId(parsed.mediaId, next) : null;
  }

  const episodes = [...detail.episodes].sort((a, b) => a.season - b.season || a.number - b.number);
  const at = episodes.findIndex((episode) => episode.id === id);
  return at === -1 ? null : (episodes[at + delta]?.id ?? null);
}

/** "Title · S1 E5 · Name", or "Title · Episode 5", or just the title for a film. */
export function titleLine(detail: Detail | null | undefined, id: string | undefined): string {
  if (!detail) return "";
  const title = detail.media.title;
  const parsed = parseMediaId(id);
  const name = detail.episodes.find((episode) => episode.id === id)?.name;
  if (parsed?.season && parsed.episodeNumber) {
    return `${title} · S${parsed.season} E${parsed.episodeNumber}${name ? ` · ${name}` : ""}`;
  }
  return parsed?.episodeNumber ? `${title} · Episode ${parsed.episodeNumber}` : title;
}
