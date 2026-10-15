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
  return STATUS[status] ?? status.replace(/_/g, " ").toLowerCase();
}

/** "2026 · tv · ★ 75%": the quiet facts under a title. */
export function facts(card: Card): string[] {
  return [
    card.year ? String(card.year) : null,
    card.format?.toLowerCase(),
    card.score ? `★ ${card.score}%` : null,
  ].filter((part): part is string => Boolean(part));
}
