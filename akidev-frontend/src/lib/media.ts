// The shape every tile, row and banner draws from: one flat record per title, which is what
// the API returns. The same fields as the desktop app's `Media`.

import type { Media } from "../types/media.ts";

export type Card = Media;

const STATUS: Record<string, string> = {
  FINISHED: "finished",
  RELEASING: "airing",
  NOT_YET_RELEASED: "upcoming",
  CANCELLED: "cancelled",
  HIATUS: "on hiatus",
};

/** A provider's status word as display text: AniList's "RELEASING" is "airing", TVMaze's "Ended" is "ended". */
export function statusLabel(status: string | null | undefined): string | null {
  if (!status) return null;
  const word = STATUS[status] ?? status.replace(/_/g, " ").toLowerCase();
  return word[0].toUpperCase() + word.slice(1);
}

/** "tv" becomes "TV", "special" becomes "Special", and "movie" is left out: the kind already says film. */
export function formatLabel(format: string | null | undefined): string | null {
  const word = format?.trim().toLowerCase();
  if (!word || word === "movie") return null;
  return word.length <= 3 ? word.toUpperCase() : word[0].toUpperCase() + word.slice(1);
}

/** "2026 · TV · ★ 75%": the quiet facts under a title. */
export function facts(card: Card): string[] {
  return [
    card.year ? String(card.year) : null,
    formatLabel(card.format),
    card.score ? `★ ${card.score}%` : null,
  ].filter((part): part is string => Boolean(part));
}
