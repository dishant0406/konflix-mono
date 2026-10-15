// Tests for the site's catalogue configuration.
//
// The web and the desktop app show the same things in the same order, so these pin the parts a
// slip would silently break: every page maps to a catalogue.

import assert from "node:assert/strict";
import test from "node:test";

import { CATALOGUES, CONTENT_TYPES, catalogueForSlug, genresFor } from "../src/constants/site.ts";

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

test("a genre filter only offers genres the provider has", () => {
  // AniList and Cinemeta spell genres differently; a genre one lacks only ever returns nothing.
  assert.ok(genresFor("anime").includes("Mecha"));
  assert.ok(!genresFor("series").includes("Mecha"));
  assert.ok(genresFor("movie").includes("Documentary"));
  for (const genre of genresFor(null)) {
    assert.ok(genresFor("anime").includes(genre) && genresFor("movie").includes(genre), genre);
  }
});
