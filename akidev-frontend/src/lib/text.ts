/** Descriptions from AniList carry markup (<br>, <i>); a plain-text line has no use for it. */
export function stripTags(text: string): string {
  return text.replace(/<br\s*\/?>/gi, " ").replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}
