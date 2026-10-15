// The caption choices: grouping the API's list by language, and remembering the viewer's language.
// Plain functions, so they can be tested alone.

import type { SubtitleOption } from "../types/media.ts";

/** What the choice is kept under in the browser: a language code, or "off". */
export const SUBTITLE_KEY = "akidev.subtitles";

export interface Language {
  lang: string;
  language: string;
  options: SubtitleOption[];
}

/** One entry per language, in the order the server listed them (English first). */
export function byLanguage(subtitles: SubtitleOption[]): Language[] {
  const groups = new Map<string, Language>();
  for (const option of subtitles) {
    const group = groups.get(option.lang);
    if (group) group.options.push(option);
    else groups.set(option.lang, { lang: option.lang, language: option.language, options: [option] });
  }
  return [...groups.values()];
}

/** What to show for one option: where it came from, and what the file is called. */
export function optionLine(option: SubtitleOption): string {
  return option.label ? `${option.provider} · ${option.label}` : option.provider;
}

type Store = Pick<Storage, "getItem" | "setItem">;

/** The remembered choice: a language code, "off", or null when the viewer has not chosen yet. */
export function readChoice(store?: Store): string | null {
  try {
    return (store ?? localStorage).getItem(SUBTITLE_KEY) || null;
  } catch {
    return null;
  }
}

export function writeChoice(choice: string, store?: Store): void {
  try {
    (store ?? localStorage).setItem(SUBTITLE_KEY, choice);
  } catch {
    // Storage blocked: the choice just does not carry to the next episode.
  }
}

/** The option to switch on for a remembered language, or null when off, unchosen or not offered here. */
export function optionForChoice(subtitles: SubtitleOption[], choice: string | null): SubtitleOption | null {
  if (!choice || choice === "off") return null;
  return subtitles.find((option) => option.lang === choice) ?? null;
}
