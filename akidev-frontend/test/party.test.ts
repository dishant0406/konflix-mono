// Tests for the watch party's messages and the rules that keep a guest in step. The same cases
// as desktop/party/src/{protocol,sync}.rs, so the two apps agree.

import assert from "node:assert/strict";
import test from "node:test";

import { cleanName, guestState } from "../src/lib/party/connection.ts";
import { parseMessage } from "../src/lib/party/protocol.ts";
import { Clock, fix, positionNow, targetOf } from "../src/lib/party/sync.ts";

test("messages are read the way the desktop app writes them", () => {
  assert.deepEqual(
    parseMessage('{"t":"state","item":{"kind":"video","id":"movie-tt1"},"playing":true,"position":12.5,"at":1700000000000}'),
    { t: "state", item: { kind: "video", id: "movie-tt1" }, playing: true, position: 12.5, at: 1700000000000 },
  );
  assert.deepEqual(parseMessage('{"t":"pause"}'), { t: "pause" });
  assert.deepEqual(parseMessage('{"t":"seek","position":3.0}'), { t: "seek", position: 3 });
  assert.deepEqual(parseMessage('{"t":"state","item":{"kind":"music","id":"abc","title":"A","artist":"B"},"playing":false,"position":0,"at":1}'), {
    t: "state",
    item: { kind: "music", id: "abc", title: "A", artist: "B", cover: null },
    playing: false,
    position: 0,
    at: 1,
  });
  assert.deepEqual(parseMessage('{"t":"members","members":[{"name":"Hana","host":true,"state":"ready"}]}'), {
    t: "members",
    members: [{ name: "Hana", host: true, state: "ready" }],
  });
  assert.deepEqual(parseMessage('{"t":"status","state":"loading","id":"movie-tt1"}'), { t: "status", state: "loading", id: "movie-tt1" });
  assert.deepEqual(parseMessage('{"t":"status","state":"ready","id":null}'), { t: "status", state: "ready", id: null });
});

test("anything else is turned away", () => {
  for (const text of ["", "nonsense", '{"t":"status","state":"asleep","id":null}', '{"t":"status","trouble":true}', "[]", "null", '{"t":"unknown"}', '{"t":"seek"}', '{"t":"seek","position":"3"}', '{"t":"open","item":{"kind":"video","id":"x"}}', '{"t":"state","item":{"kind":"x"},"playing":true,"position":1,"at":1}']) {
    assert.equal(parseMessage(text), null, text);
  }
});

test("a small gap is closed by changing speed and a big one by seeking", () => {
  assert.deepEqual(fix(10, 10.05, true), { seek: null, speed: 1 });
  assert.deepEqual(fix(10, 10.5, true), { seek: null, speed: 1.05 });
  assert.deepEqual(fix(10.5, 10, true), { seek: null, speed: 0.95 });
  assert.deepEqual(fix(10, 14, true), { seek: 14, speed: 1 });
  assert.deepEqual(fix(14, 10, true), { seek: 10, speed: 1 });
});

test("a paused party is matched exactly at normal speed", () => {
  assert.deepEqual(fix(10, 10.5, false), { seek: 10.5, speed: 1 });
  assert.deepEqual(fix(10, 10.1, false), { seek: null, speed: 1 });
});

test("the clock is read from the quickest round trip", () => {
  const clock = new Clock();
  // The host's clock is 5000 ms ahead. A slow trip, then a fast one.
  clock.sample(1000, 6300, 1600);
  clock.sample(2000, 7010, 2020);
  assert.equal(clock.hostTime(3000), 8000);
});

test("a state is moved on by the time it took to arrive", () => {
  const clock = new Clock();
  clock.sample(1000, 6010, 1020);
  // The host said "10.0 seconds" at its 7000; it is now its 7400 (this device's 2400).
  const target = targetOf({ item: null, playing: true, position: 10, at: 7000 }, clock, 2400, 0);
  assert.ok(Math.abs(target.playback.position - 10.4) < 0.001);
  assert.equal(targetOf({ item: null, playing: false, position: 10, at: 7000 }, clock, 2400, 0).playback.position, 10);
  // And on from there as time passes here.
  assert.ok(Math.abs(positionNow(target, 2000) - 12.4) < 0.001);
});

test("a guest is loading what it has not yet said it is ready for", () => {
  // Nothing playing: whatever the guest said stands.
  assert.equal(guestState("ready", null, null), "ready");
  // Something is playing and the guest has said nothing, or said it of the one before.
  assert.equal(guestState("ready", null, "b"), "loading");
  assert.equal(guestState("ready", "a", "b"), "loading");
  // Said of this one.
  assert.equal(guestState("ready", "b", "b"), "ready");
  assert.equal(guestState("loading", "b", "b"), "loading");
  assert.equal(guestState("trouble", "b", "b"), "trouble");
});

test("a clock that is far out cannot send a guest far away", () => {
  // No reading yet, and the host's clock is ten minutes behind this one.
  const target = targetOf({ item: null, playing: true, position: 10, at: 0 }, new Clock(), 600_000, 0);
  assert.equal(target.playback.position, 20);
});

test("a guest's name is kept short and plain", () => {
  assert.equal(cleanName("  Sam \n"), "Sam");
  assert.equal(cleanName(""), "Guest");
  assert.equal(cleanName("x".repeat(100)).length, 24);
});
