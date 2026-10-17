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

const NOISE = new Set(["srt", "ass", "vtt", "sub", "eng", "en", "english", "the", "and", "of", "subs"]);

function words(name: string): Set<string> {
  return new Set(
    name
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter((word) => word.length > 1 && !NOISE.has(word)),
  );
}

/** Whether a file name says it is one disc or part of a film that was shared in pieces: such a file
 * starts its clock again at that disc, so against a whole film it is out by the first disc's length. */
export function isSplitDisc(name: string): boolean {
  const tokens = name.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
  const labels = ["cd", "disc", "disk", "part", "pt"];
  return tokens.some(
    (token, index) =>
      labels.some((label) => new RegExp(`^${label}\\d$`).test(token)) ||
      (labels.includes(token) && /^\d$/.test(tokens[index + 1] ?? "")),
  );
}

/** The list with, inside each language, the files made for the release being played first (its
 * group, "bluray", "1080p" shared with the file's name) and split-disc files last. The order among
 * equals, and of the languages, stays as the server gave it. */
export function preferRelease(subtitles: SubtitleOption[], release: string | null): SubtitleOption[] {
  const wanted = words(release ?? "");
  const score = (option: SubtitleOption) => {
    let shared = 0;
    for (const word of words(option.label)) if (wanted.has(word)) shared += 1;
    return shared - (isSplitDisc(option.label) ? 100 : 0);
  };
  return byLanguage(subtitles).flatMap((group) =>
    group.options
      .map((option, index) => ({ option, index, score: score(option) }))
      .sort((a, b) => b.score - a.score || a.index - b.index)
      .map((entry) => entry.option),
  );
}

/** Moves every cue of a track by `seconds`, later when positive. */
export function shiftCues(cues: { startTime: number; endTime: number }[], seconds: number): void {
  for (const cue of cues) {
    cue.startTime = Math.max(0, cue.startTime + seconds);
    cue.endTime = Math.max(0, cue.endTime + seconds);
  }
}
