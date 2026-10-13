// Tests for merging search results across catalogues.
//
// This is the behaviour that was broken: the TV site's search box sent no content type,
// so the API answered from the anime catalogue and a series search returned nothing.
// These pin the merge that replaced it.
//
// Plain node:test against plain functions, so no test framework or browser is involved.

import assert from "node:assert/strict";
import test from "node:test";

import { interleaveResults, mergeSearchPages, preferTitleMatches } from "../src/lib/merge-search.ts";
import type { MediaList, MediaPage } from "../src/types/media.ts";

function item(id: string): MediaList {
  return { id } as MediaList;
}

function page(results: MediaList[]): MediaPage {
  return { page: 1, perPage: 24, total: results.length, hasNextPage: false, results };
}

test("results from two catalogues are interleaved, not concatenated", () => {
  // Concatenating would put every series above every film, so the first page of a
  // film-heavy search would show only series.
  const merged = interleaveResults([[item("series-1"), item("series-2")], [item("movie-1"), item("movie-2")]], 10);
  assert.deepEqual(
    merged.map((entry) => entry.id),
    ["series-1", "movie-1", "series-2", "movie-2"],
  );
});

test("a catalogue that returns nothing does not consume any of the page", () => {
  const merged = interleaveResults([[item("series-1"), item("series-2")], []], 10);
  assert.deepEqual(
    merged.map((entry) => entry.id),
    ["series-1", "series-2"],
  );
});

test("the merge stops at the requested page size", () => {
  const merged = interleaveResults(
    [[item("a"), item("b"), item("c"), item("d")], [item("e"), item("f"), item("g"), item("h")]],
    3,
  );
  assert.equal(merged.length, 3);
  assert.deepEqual(
    merged.map((entry) => entry.id),
    ["a", "e", "b"],
  );
});

test("a longer catalogue keeps contributing after a shorter one is exhausted", () => {
  const merged = interleaveResults([[item("series-1")], [item("movie-1"), item("movie-2"), item("movie-3")]], 10);
  assert.deepEqual(
    merged.map((entry) => entry.id),
    ["series-1", "movie-1", "movie-2", "movie-3"],
  );
});

test("no results at all yields an empty page rather than an error", () => {
  assert.deepEqual(interleaveResults([[], []], 10), []);
  const merged = mergeSearchPages([page([]), page([])], 24);
  assert.deepEqual(merged.results, []);
  assert.equal(merged.total, 0);
});

test("a single catalogue is returned in its own relevance order", () => {
  // Re-ranking within one catalogue would override the provider's own relevance.
  const merged = mergeSearchPages([page([item("a"), item("b"), item("c")]), page([])], 24);
  assert.deepEqual(
    merged.results?.map((entry) => entry.id),
    ["a", "b", "c"],
  );
});

test("a merged search never advertises a next page", () => {
  // It is merged here from one upstream page per catalogue, so there is no upstream
  // page to advance to. Reporting true would offer a Next button that does nothing.
  const merged = mergeSearchPages([page([item("a")]), page([item("b")])], 24);
  assert.equal(merged.hasNextPage, false);
});

test("an undefined page is treated as empty, so a failing catalogue does not blank the search", () => {
  const merged = mergeSearchPages([page([item("a")]), undefined], 24);
  assert.deepEqual(
    merged.results?.map((entry) => entry.id),
    ["a"],
  );
});

test("a title that starts with the term is moved ahead of an approximate match", () => {
  // Measured: searching "dangal" returned TVMaze's fuzzy match "D.N.Angel" first, above
  // the film actually called Dangal. TVMaze's own search is fuzzy, so the order it
  // returns is not the order a user reads.
  const results = [named("series-1", "D.N.Angel"), named("movie-7691", "Dangal")];
  assert.deepEqual(
    preferTitleMatches(results, "dangal").map((entry) => entry.id),
    ["movie-7691", "series-1"],
  );
});

test("the term match is case and punctuation insensitive", () => {
  const results = [named("a", "Some Other Show"), named("b", "Game of Thrones")];
  assert.deepEqual(
    preferTitleMatches(results, "game of thrones").map((entry) => entry.id),
    ["b", "a"],
  );
});

test("a partial title still matches the term, so a qualified title is not demoted", () => {
  // "Dangal (2016)" should still count as a match for "dangal".
  const results = [named("a", "Unrelated"), named("b", "Dangal (2016)")];
  assert.equal(preferTitleMatches(results, "dangal")[0].id, "b");
});

test("the ordering is a stable partition, so nothing is dropped or re-sorted", () => {
  const results = [named("a", "Zebra"), named("b", "Aardvark"), named("c", "Zulu")];
  assert.deepEqual(
    preferTitleMatches(results, "zzz").map((entry) => entry.id),
    ["a", "b", "c"],
    "with no match the original order must survive",
  );
});

test("an empty term leaves the results untouched", () => {
  const results = [named("a", "Anything")];
  assert.deepEqual(
    preferTitleMatches(results, "").map((entry) => entry.id),
    ["a"],
  );
});

function named(id: string, title: string): MediaList {
  return { id, title: { english: title, romaji: null, native: null } } as MediaList;
}

test("a short unrelated title is not treated as a match for a longer term", () => {
  // "DANG!" begins with the first four letters of "dangal" but is a different title, so
  // a plain prefix test on the title would wrongly promote it.
  const results = [named("a", "DANG!"), named("b", "Dangal")];
  assert.deepEqual(
    preferTitleMatches(results, "dangal").map((entry) => entry.id),
    ["b", "a"],
  );
});
