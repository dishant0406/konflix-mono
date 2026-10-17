// Tests for the caption choices: grouping by language and remembering the viewer's language.

import assert from "node:assert/strict";
import test from "node:test";

import { SUBTITLE_KEY, byLanguage, isSplitDisc, optionForChoice, optionLine, preferRelease, readChoice, shiftCues, writeChoice } from "../src/lib/subtitles.ts";
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

test("a disc of a split film is recognised and ordinary names are not", () => {
  assert.equal(isSplitDisc("The_Shawshank_Redemption_(1994).CD1.aFLDp.srt"), true);
  assert.equal(isSplitDisc("The Shawshank Redemption SBC AC3 aFLDp CD2.eng.srt"), true);
  assert.equal(isSplitDisc("movie.part 2.srt"), true);
  assert.equal(isSplitDisc("Breaking.Bad.S01E01.720p.HDTV.x264-BiA.srt"), false);
  assert.equal(isSplitDisc("Spartacus.2010.srt"), false);
});

test("the subtitles for the played release come first, split discs last, languages stay apart", () => {
  const list = [
    option("Movie.CD2", "en", "English"),
    option("Movie.DVDRip.XviD-AAA", "en", "English"),
    option("Movie.1080p.BluRay.x264-BBB", "en", "English"),
    option("x.BBB", "fr", "French"),
    option("y", "fr", "French"),
  ];
  const ranked = preferRelease(list, "Movie.2000.1080p.BluRay.x264-BBB");
  assert.deepEqual(ranked.map((o) => o.id), ["Movie.1080p.BluRay.x264-BBB", "Movie.DVDRip.XviD-AAA", "Movie.CD2", "x.BBB", "y"]);
  assert.deepEqual(preferRelease(list, null).map((o) => o.id).slice(0, 2), ["Movie.DVDRip.XviD-AAA", "Movie.1080p.BluRay.x264-BBB"]);
});

test("moving the cues shifts both ends and never goes below zero", () => {
  const cues = [{ startTime: 1, endTime: 2 }, { startTime: 0.1, endTime: 0.5 }];
  shiftCues(cues, -0.3);
  assert.deepEqual(cues.map((c) => [Math.round(c.startTime * 10) / 10, Math.round(c.endTime * 10) / 10]), [[0.7, 1.7], [0, 0.2]]);
});
