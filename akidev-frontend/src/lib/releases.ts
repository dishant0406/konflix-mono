// How the source picker words and orders a release. Plain functions over the API's `Release`,
// so they carry no React and can be tested alone.

import type { Release } from "../types/media.ts";

export function resolutionLabel(resolution: Release["resolution"]): string | null {
  if (!resolution) return null;
  return resolution >= 2160 ? "4K" : `${resolution}p`;
}

export function codecLabel(codec: Release["codec"]): string | null {
  switch (codec) {
    case "h264":
      return "H.264";
    case "hevc":
      return "HEVC";
    case "av1":
      return "AV1";
    default:
      return null;
  }
}

/** Only the notable case is named: compressed audio is what every release has, so it says nothing. */
export function audioLabel(audio: Release["audio"]): string | null {
  return audio === "lossless" ? "Lossless" : null;
}

/** "1.4 GB" or "700 MB". Decimal units, as the indexers and the file managers show them. */
export function sizeLabel(bytes: number | null): string | null {
  if (bytes === null || !Number.isFinite(bytes) || bytes <= 0) return null;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.max(1, Math.round(bytes / 1e6))} MB`;
}

export function seedersLabel(seeders: number): string {
  if (seeders <= 0) return "No seeders";
  return seeders === 1 ? "1 seeder" : `${seeders} seeders`;
}

/** The quiet badges under a title, in reading order, leaving out what the name did not say. */
export function releaseFacts(release: Release): string[] {
  return [
    resolutionLabel(release.resolution),
    codecLabel(release.codec),
    audioLabel(release.audio),
    sizeLabel(release.size_bytes),
    seedersLabel(release.seeders),
    release.pack ? "Season pack" : null,
  ].filter((fact): fact is string => fact !== null);
}

/** The server already puts the best first; the formats that may not decode follow, in the same order. */
export function groupReleases(releases: Release[]): { common: Release[]; other: Release[] } {
  return {
    common: releases.filter((release) => !release.uncommon),
    other: releases.filter((release) => release.uncommon),
  };
}

/**
 * Which release is playing: the one the viewer picked, or else the one whose title the play
 * request reported (it names the release by title, not by id).
 */
export function playingRelease(releases: Release[], picked: string | null, playedTitle: string | null): string | null {
  if (picked) return picked;
  if (!playedTitle) return null;
  return releases.find((release) => release.title === playedTitle)?.id ?? null;
}
