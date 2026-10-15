import assert from "node:assert/strict";
import { test } from "node:test";

import { pick } from "../src/lib/desktop-release.ts";

const release = (tag: string, files: string[], extra = {}) => ({
  tag_name: tag,
  draft: false,
  prerelease: false,
  html_url: `https://example.test/${tag}`,
  assets: files.map((name) => ({ name, browser_download_url: `https://example.test/dl/${name}` })),
  ...extra,
});

const list = [
  release("desktop-mac-v0.1.4", ["Akidev-0.1.4-arm64.dmg", "Akidev-0.1.4-arm64.dmg.sha256"]),
  release("desktop-windows-v0.1.3", ["Akidev-0.1.3-x86_64.zip", "Akidev-0.1.3-x86_64-setup.exe"]),
  release("desktop-mac-v0.1.3", ["Akidev-0.1.3-arm64.dmg"]),
];

test("each platform gets its own newest release and its own file", () => {
  assert.deepEqual(pick(list, "windows"), {
    version: "0.1.3",
    url: "https://example.test/dl/Akidev-0.1.3-x86_64-setup.exe",
  });
  assert.equal(pick(list, "mac")?.url, "https://example.test/dl/Akidev-0.1.4-arm64.dmg");
});

test("a draft, a prerelease and a platform with no release are skipped", () => {
  assert.equal(pick([release("desktop-windows-v9", [], { draft: true })], "windows"), null);
  assert.equal(pick([release("desktop-windows-v9", [], { prerelease: true })], "windows"), null);
  assert.equal(pick(list.slice(0, 1), "windows"), null);
});

test("a release without the file still links to its page", () => {
  assert.equal(pick([release("desktop-windows-v1.0.0", [])], "windows")?.url, "https://example.test/desktop-windows-v1.0.0");
});
