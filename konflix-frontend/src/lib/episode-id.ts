// Episode ids are owned by the API: "<mediaId>-ep<number>". The frontend only
// carries them around, so both halves are parsed in one place.

export interface ParsedEpisodeId {
  mediaId: string;
  episodeNumber: number;
}

export function parseEpisodeId(episodeId: string | string[] | undefined): ParsedEpisodeId | null {
  const value = Array.isArray(episodeId) ? episodeId[0] : episodeId;
  const match = /^(\d+)-ep(\d+)$/.exec(value || "");
  if (!match) return null;
  return { mediaId: match[1], episodeNumber: Number.parseInt(match[2], 10) };
}

export function buildEpisodeId(mediaId: string, episodeNumber: number): string {
  return `${mediaId}-ep${episodeNumber}`;
}
