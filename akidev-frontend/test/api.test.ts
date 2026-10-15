// Tests for the REST client: the URL it builds, the body it sends, and the errors it turns
// responses into. fetch is replaced with a stub, so nothing here touches a network.

import assert from "node:assert/strict";
import test from "node:test";

import { ApiError, get, post } from "../src/lib/api.ts";

type Call = { url: string; init: RequestInit };

function stub(respond: () => Response | Promise<Response>): Call[] {
  const calls: Call[] = [];
  globalThis.fetch = (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return respond();
  }) as typeof fetch;
  return calls;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

test("a get adds the query and leaves out what is not set", async () => {
  const calls = stub(() => json({ results: [] }));
  await get("/v1/catalogue", { kind: "all", sort: "popularity", genre: undefined, page: 2 });
  assert.equal(calls[0].url, "http://localhost:9000/v1/catalogue?kind=all&sort=popularity&page=2");
  assert.equal(calls[0].init.method, "GET");
});

test("a get with no query has no question mark", async () => {
  const calls = stub(() => json({}));
  await get("/v1/home");
  assert.equal(calls[0].url, "http://localhost:9000/v1/home");
});

test("a post sends its body as JSON", async () => {
  const calls = stub(() => json({ available: false, reason: "none" }));
  await post("/v1/play", { id: "movie-tt0111161", hevc: true });
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.body, '{"id":"movie-tt0111161","hevc":true}');
  assert.deepEqual(calls[0].init.headers, { "Content-Type": "application/json" });
});

test("an error body's message and the status reach the caller", async () => {
  stub(() => json({ error: "busy" }, 503));
  await assert.rejects(post("/v1/play", { id: "x" }), (error: unknown) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.message, "busy");
    assert.equal(error.status, 503);
    return true;
  });
});

test("a failure with no readable body still says what happened", async () => {
  stub(() => new Response("<html>", { status: 502 }));
  await assert.rejects(get("/v1/home"), /returned 502/);
});

test("an unreachable API is reported as such", async () => {
  stub(() => Promise.reject(new TypeError("fetch failed")));
  await assert.rejects(get("/v1/home"), /Could not reach/);
});

test("a timeout is reported as too slow", async () => {
  stub(() => Promise.reject(new DOMException("timed out", "TimeoutError")));
  await assert.rejects(get("/v1/home"), /took too long/);
});
