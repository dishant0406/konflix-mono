// The shape every tile, row and banner draws from: one flat record per title, built from the
// GraphQL list shape. The same fields as the desktop app's `Media`.

import type { ContentType } from "@/constants/site";
import { parseMediaId } from "@/lib/media-id";
import { mediaPoster, mediaTitle } from "@/lib/utils";
import type { MediaList, MediaRelationNode } from "@/types/media";

export interface Card {
  /** The title key, e.g. "anime-16498". */
  id: string;
  kind: ContentType;
  title: string;
  poster: string | null;
  banner: string | null;
  year: number | null;
  format: string | null;
  /** 0 to 100. */
  score: number | null;
  description: string | null;
  genres: string[];
}

export function toCard(media: MediaList & { description?: string | null }): Card | null {
  const parsed = parseMediaId(media.id);
  if (!parsed) return null;
  return {
    id: parsed.key,
    kind: parsed.contentType,
    title: mediaTitle(media.title),
    poster: mediaPoster(media.coverImage) || null,
    banner: media.bannerImage || null,
    year: media.seasonYear ?? media.startDate?.year ?? null,
    format: media.format ? media.format.toLowerCase() : null,
    score: media.averageScore || media.meanScore || null,
    description: media.description ?? null,
    genres: media.genres ?? [],
  };
}

export function toCards(list: (MediaList | null | undefined)[] | null | undefined): Card[] {
  return (list ?? []).flatMap((media) => {
    const card = media ? toCard(media) : null;
    return card ? [card] : [];
  });
}

/** A relation or recommendation is a partial record; this fills in what a tile needs. */
export function relationCard(node: MediaRelationNode): Card | null {
  const parsed = parseMediaId(node.id ?? undefined);
  if (!parsed) return null;
  return {
    id: parsed.key,
    kind: parsed.contentType,
    title: mediaTitle(node.title),
    poster: mediaPoster(node.coverImage) || null,
    banner: null,
    year: node.startDate?.year ?? null,
    format: node.format ? node.format.toLowerCase() : null,
    score: node.meanScore ?? null,
    description: null,
    genres: [],
  };
}

/** "2026 · tv · ★ 75%": the quiet facts under a title. */
export function facts(card: Card): string[] {
  return [
    card.year ? String(card.year) : null,
    card.format,
    card.score ? `★ ${card.score}%` : null,
  ].filter((part): part is string => Boolean(part));
}
