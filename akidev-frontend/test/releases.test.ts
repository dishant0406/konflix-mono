// Tests for how releases are worded, grouped and matched to what is playing.

import assert from "node:assert/strict";
import test from "node:test";

import { groupReleases, playingRelease, releaseFacts, sizeLabel } from "../src/lib/releases.ts";
import type { Release } from "../src/types/media.ts";

function release(patch: Partial<Release>): Release {
  return {
    id: "a".repeat(40),
    title: "Show S01E01 1080p WEB-DL x264 AAC",
    size_bytes: 1_400_000_000,
    seeders: 66,
    source: "knaben",
    resolution: 1080,
    codec: "h264",
    audio: "compressed",
    pack: false,
    uncommon: false,
    cached: null,
    ...patch,
  };
}

test("a release is worded in reading order, without what the name did not say", () => {
  assert.deepEqual(releaseFacts(release({})), ["1080p", "H.264", "1.4 GB", "66 seeders"]);
  assert.deepEqual(
    releaseFacts(release({ resolution: 2160, codec: "hevc", audio: "lossless", pack: true, seeders: 1 })),
    ["4K", "HEVC", "Lossless", "1.4 GB", "1 seeder", "Season pack"],
  );
  assert.deepEqual(releaseFacts(release({ resolution: null, codec: null, size_bytes: null, seeders: 0 })), ["No seeders"]);
});

test("sizes use decimal units and round small ones to megabytes", () => {
  assert.equal(sizeLabel(1_400_000_000), "1.4 GB");
  assert.equal(sizeLabel(734_003_200), "734 MB");
  assert.equal(sizeLabel(10), "1 MB");
  assert.equal(sizeLabel(null), null);
  assert.equal(sizeLabel(0), null);
});

test("uncommon formats follow the rest, each group keeping the server's order", () => {
  const list = [
    release({ id: "1" }),
    release({ id: "2", uncommon: true }),
    release({ id: "3" }),
    release({ id: "4", uncommon: true }),
  ];
  const { common, other } = groupReleases(list);
  assert.deepEqual(common.map((r) => r.id), ["1", "3"]);
  assert.deepEqual(other.map((r) => r.id), ["2", "4"]);
});

test("the playing release is the picked one, else the one the play request named by title", () => {
  const list = [release({ id: "1", title: "A" }), release({ id: "2", title: "B" })];
  assert.equal(playingRelease(list, "2", "A"), "2");
  assert.equal(playingRelease(list, null, "B"), "2");
  assert.equal(playingRelease(list, null, "gone"), null);
  assert.equal(playingRelease(list, null, null), null);
});
