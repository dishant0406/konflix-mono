// Tests for the play helpers: HEVC detection and the message for a failed play request.

import assert from "node:assert/strict";
import test from "node:test";

import { ApiError } from "../src/lib/api.ts";
import { canPlayHevc, playErrorMessage } from "../src/lib/play.ts";

test("a busy server is explained, not quoted", () => {
  assert.equal(playErrorMessage(new ApiError("busy", 503)), "The server is busy, try again in a moment.");
});

test("any other failure keeps its own message", () => {
  assert.equal(playErrorMessage(new ApiError("No such title.", 404)), "No such title.");
  assert.equal(playErrorMessage(new Error("Could not reach the Akidev API.")), "Could not reach the Akidev API.");
  assert.equal(playErrorMessage("odd"), "The API did not respond.");
});

test("without MediaSource the browser is not offered HEVC, and it is asked once when there is", () => {
  assert.equal(canPlayHevc(), false);

  const asked: string[] = [];
  (globalThis as { MediaSource?: unknown }).MediaSource = {
    isTypeSupported: (type: string) => {
      asked.push(type);
      return true;
    },
  };
  assert.equal(canPlayHevc(), true);
  assert.equal(canPlayHevc(), true);
  assert.deepEqual(asked, ['video/mp4; codecs="hvc1.1.6.L120.90"']);
});
