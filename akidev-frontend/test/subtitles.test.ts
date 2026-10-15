// Tests for the caption choices: grouping by language and remembering the viewer's language.

import assert from "node:assert/strict";
import test from "node:test";

import { SUBTITLE_KEY, byLanguage, optionForChoice, optionLine, readChoice, writeChoice } from "../src/lib/subtitles.ts";
import type { SubtitleOption } from "../src/types/media.ts";

function option(id: string, lang: string, language: string, patch: Partial<SubtitleOption> = {}): SubtitleOption {
  return { id, provider: "OpenSubtitles", lang, language, label: `${id}.srt`, hearing_impaired: false, url: `https://x/${id}.vtt`, ...patch };
}

const LIST = [option("a", "en", "English"), option("b", "en", "English"), option("c", "fr", "French")];

test("options are grouped by language in the order the server listed them", () => {
  const groups = byLanguage(LIST);
  assert.deepEqual(groups.map((g) => [g.lang, g.language, g.options.length]), [["en", "English", 2], ["fr", "French", 1]]);
});

test("an option is described by provider and file, or the provider alone", () => {
  assert.equal(optionLine(LIST[0]), "OpenSubtitles · a.srt");
  assert.equal(optionLine(option("d", "en", "English", { label: "" })), "OpenSubtitles");
});

test("a remembered language picks its first option; off, unchosen or missing picks none", () => {
  assert.equal(optionForChoice(LIST, "en")?.id, "a");
  assert.equal(optionForChoice(LIST, "fr")?.id, "c");
  assert.equal(optionForChoice(LIST, "de"), null);
  assert.equal(optionForChoice(LIST, "off"), null);
  assert.equal(optionForChoice(LIST, null), null);
});

test("the choice is kept under one key, and a blocked store is not an error", () => {
  const data = new Map<string, string>();
  const store = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
  assert.equal(readChoice(store), null);
  writeChoice("fr", store);
  assert.equal(data.get(SUBTITLE_KEY), "fr");
  assert.equal(readChoice(store), "fr");

  const blocked = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
  };
  assert.equal(readChoice(blocked), null);
  assert.doesNotThrow(() => writeChoice("en", blocked));
});
