import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Formats a number of seconds as MM:SS. Invalid input becomes 00:00. */
export function formatSeconds(totalSeconds: number | null | undefined): string {
  if (typeof totalSeconds !== "number" || !Number.isFinite(totalSeconds) || totalSeconds < 0) {
    return "00:00";
  }
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = Math.floor(totalSeconds % 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** The title to show for a media entry, preferring English. */
export function mediaTitle(title: { english?: string | null; romaji?: string | null; native?: string | null } | null | undefined): string {
  return title?.english || title?.romaji || title?.native || "Untitled";
}

/** The best available poster for a media entry. */
export function mediaPoster(coverImage: { extraLarge?: string | null; large?: string | null } | null | undefined): string {
  return coverImage?.extraLarge || coverImage?.large || "";
}

/** The best available wide image for a hero banner, falling back to the poster. */
export function mediaBanner(media: {
  bannerImage?: string | null;
  coverImage?: { extraLarge?: string | null; large?: string | null } | null;
}): string {
  return media.bannerImage || mediaPoster(media.coverImage);
}
