// Tests for the site's catalogue configuration.
//
// The web and the desktop app show the same things in the same order, so these pin the parts a
// slip would silently break: every page maps to a catalogue, and every row names one.

import assert from "node:assert/strict";
import test from "node:test";

import { CATALOGUES, CONTENT_TYPES, HOME_ROWS, SORTS, catalogueForSlug, genresFor } from "../src/constants/site.ts";

test("each catalogue page is found by its slug", () => {
  assert.equal(catalogueForSlug("anime")?.kind, "anime");
  assert.equal(catalogueForSlug("series")?.kind, "series");
  assert.equal(catalogueForSlug("films")?.kind, "movie");
  assert.equal(catalogueForSlug("movie"), undefined);
  assert.equal(catalogueForSlug("settings"), undefined);
});

test("the three catalogues are all offered, in the top bar's order", () => {
  assert.deepEqual(
    CATALOGUES.map((page) => page.label),
    ["Anime", "Series", "Films"],
  );
  assert.deepEqual(CONTENT_TYPES, ["anime", "series", "movie"]);
});

test("every home row draws from a real catalogue and a real sort", () => {
  const sorts = new Set(SORTS.map((sort) => sort.value));
  for (const row of HOME_ROWS) {
    assert.ok(CONTENT_TYPES.includes(row.kind), row.title);
    assert.ok(sorts.has(row.sort), row.title);
  }
});

test("the home page mixes all three catalogues", () => {
  assert.deepEqual(new Set(HOME_ROWS.map((row) => row.kind)), new Set(CONTENT_TYPES));
});

test("a genre filter only offers genres the provider has", () => {
  // AniList and Cinemeta spell genres differently; a genre one lacks only ever returns nothing.
  assert.ok(genresFor("anime").includes("Mecha"));
  assert.ok(!genresFor("series").includes("Mecha"));
  assert.ok(genresFor("movie").includes("Documentary"));
  for (const genre of genresFor(null)) {
    assert.ok(genresFor("anime").includes(genre) && genresFor("movie").includes(genre), genre);
  }
});

test("the home rows used for a genre are offered in that catalogue", () => {
  for (const row of HOME_ROWS.filter((each) => each.genre)) {
    assert.ok(genresFor(row.kind).includes(row.genre as string), row.title);
  }
});
