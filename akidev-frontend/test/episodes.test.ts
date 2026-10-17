// Tests for the episode lookups and the status words on the title and watch pages.

import assert from "node:assert/strict";
import test from "node:test";

import { episodeCount, firstPlayable, neighbour, titleLine } from "../src/lib/episodes.ts";
import { facts, statusLabel } from "../src/lib/media.ts";
import type { Detail, Episode, Media } from "../src/types/media.ts";

function media(overrides: Partial<Media>): Media {
  return {
    id: "series-tt1",
    kind: "series",
    title: "Show",
    poster: null,
    banner: null,
    description: null,
    format: null,
    year: null,
    episodes: null,
    runtime_minutes: null,
    status: null,
    score: null,
    popularity: 0,
    genres: [],
    is_adult: false,
    ...overrides,
  };
}

const episode = (season: number, number: number, name: string | null = null): Episode => ({
  id: `series-tt1-s${season}e${number}`,
  season,
  number,
  name,
  airdate: null,
});

function detail(m: Media, episodes: Episode[] = []): Detail {
  return { media: m, studios: [], trailer: null, episodes, cast: [], related: [], recommended: [] };
}

const series = detail(media({}), [episode(2, 1), episode(1, 2, "Second"), episode(1, 1, "Pilot")]);
const anime = detail(media({ id: "anime-16498", kind: "anime", title: "Titan", episodes: 3 }));

test("a film plays itself, a series its first listed episode, an anime episode 1", () => {
  assert.equal(firstPlayable("movie-tt2", detail(media({ id: "movie-tt2", kind: "movie" }))), "movie-tt2");
  assert.equal(firstPlayable("series-tt1", series), "series-tt1-s2e1");
  assert.equal(firstPlayable("series-tt1", detail(media({}))), null);
  assert.equal(firstPlayable("anime-16498", anime), "anime-16498-ep1");
});

test("series neighbours follow season then number, across a season break", () => {
  assert.equal(neighbour(series, "series-tt1-s1e1", 1), "series-tt1-s1e2");
  assert.equal(neighbour(series, "series-tt1-s1e2", 1), "series-tt1-s2e1");
  assert.equal(neighbour(series, "series-tt1-s2e1", -1), "series-tt1-s1e2");
  assert.equal(neighbour(series, "series-tt1-s1e1", -1), null);
  assert.equal(neighbour(series, "series-tt1-s9e9", 1), null);
});

test("anime neighbours stop at the first and last episode", () => {
  assert.equal(neighbour(anime, "anime-16498-ep2", 1), "anime-16498-ep3");
  assert.equal(neighbour(anime, "anime-16498-ep3", 1), null);
  assert.equal(neighbour(anime, "anime-16498-ep1", -1), null);
  assert.equal(neighbour(undefined, "anime-16498-ep1", 1), null);
});

test("a film has no neighbours", () => {
  assert.equal(neighbour(detail(media({ id: "movie-tt2", kind: "movie" })), "movie-tt2", 1), null);
});

test("the episode count is the title's own, or the list's when it has none", () => {
  assert.equal(episodeCount(anime), 3);
  assert.equal(episodeCount(series), 3);
  assert.equal(episodeCount(undefined), 0);
});

test("the player's title line names the season, episode and episode name", () => {
  assert.equal(titleLine(series, "series-tt1-s1e1"), "Show · S1 E1 · Pilot");
  assert.equal(titleLine(series, "series-tt1-s5e5"), "Show · S5 E5");
  assert.equal(titleLine(anime, "anime-16498-ep2"), "Titan · Episode 2");
  assert.equal(titleLine(undefined, "anime-16498-ep2"), "");
});

test("a provider's status becomes display text", () => {
  assert.equal(statusLabel("RELEASING"), "Airing");
  assert.equal(statusLabel("NOT_YET_RELEASED"), "Upcoming");
  assert.equal(statusLabel("FINISHED"), "Finished");
  assert.equal(statusLabel("Ended"), "Ended");
  assert.equal(statusLabel("To Be Determined"), "To be determined");
  assert.equal(statusLabel(null), null);
});

test("the quiet facts under a title", () => {
  assert.deepEqual(facts(media({ year: 2013, format: "TV", score: 85 })), ["2013", "TV", "★ 85%"]);
  assert.deepEqual(facts(media({ year: 1994, format: "MOVIE" })), ["1994"]);
  assert.deepEqual(facts(media({ format: "SPECIAL" })), ["Special"]);
  assert.deepEqual(facts(media({})), []);
});
