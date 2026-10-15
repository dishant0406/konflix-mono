// Tests for the host -> site mapping.
//
// This is what replaced the build-time SITE argument: one build serves every hostname,
// and the Host header is the only thing that decides which site renders. A wrong mapping
// serves the anime catalogue on the TV host, which looks like a working site with the
// wrong content - the exact failure the two-build setup could not have.
//
// Plain node:test against plain functions, so no test framework or browser is involved.

import assert from "node:assert/strict";
import test from "node:test";

import { siteForHost, siteUrl, SITE_DOMAIN } from "../src/constants/site.ts";

test("each site answers on its own subdomain", () => {
  assert.equal(siteForHost(`anime.${SITE_DOMAIN}`)?.contentType, "anime");
  assert.equal(siteForHost(`tv.${SITE_DOMAIN}`)?.contentType, "movie");
});

test("the bare zone belongs to no site, so it is the picker", () => {
  assert.equal(siteForHost(SITE_DOMAIN), null);
  assert.equal(siteForHost(`www.${SITE_DOMAIN}`), null);
  assert.equal(siteForHost(""), null);
  assert.equal(siteForHost(undefined), null);
  assert.equal(siteForHost(null), null);
});

test("the host is matched case-insensitively and trimmed", () => {
  // Host headers arrive as sent, and a proxy may pass them through unchanged.
  assert.equal(siteForHost(`ANIME.${SITE_DOMAIN}`)?.contentType, "anime");
  assert.equal(siteForHost(`  tv.${SITE_DOMAIN}  `)?.contentType, "movie");
});

test("a lookalike host is not a site", () => {
  // A suffix match would treat an attacker-controlled host as our own, which is how a
  // host header becomes an open redirect or a cache-poisoning vector.
  assert.equal(siteForHost(`anime.${SITE_DOMAIN}.evil.com`), null);
  assert.equal(siteForHost(`evil-anime.${SITE_DOMAIN}`), null);
  assert.equal(siteForHost(`anime.example.com`), null);
});

test("the dev ports map to the two sites", () => {
  assert.equal(siteForHost("localhost:3000")?.contentType, "anime");
  assert.equal(siteForHost("localhost:3001")?.contentType, "movie");
  assert.equal(siteForHost("localhost:3002"), null);
});

test("a site's canonical URL uses its own subdomain", () => {
  assert.equal(siteUrl(siteForHost(`anime.${SITE_DOMAIN}`)!), `https://anime.${SITE_DOMAIN}`);
  assert.equal(siteUrl(siteForHost(`tv.${SITE_DOMAIN}`)!), `https://tv.${SITE_DOMAIN}`);
});

test("the two sites differ in the copy the UI renders", () => {
  const anime = siteForHost(`anime.${SITE_DOMAIN}`)!;
  const tv = siteForHost(`tv.${SITE_DOMAIN}`)!;

  assert.notEqual(anime.title, tv.title);
  assert.notEqual(anime.itemLabel, tv.itemLabel);
  assert.notEqual(anime.attribution, tv.attribution);
  // A site's search spans its own catalogues, and the TV site has two.
  assert.deepEqual(anime.searchContentTypes, ["anime"]);
  assert.deepEqual(tv.searchContentTypes, ["series", "movie"]);
});
