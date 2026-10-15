import assert from "node:assert/strict";
import test from "node:test";

import { stripTags } from "../src/lib/text.ts";

test("markup from a provider's description is reduced to one line of text", () => {
  assert.equal(stripTags("A boy<br>sets out.<br/><i>Really</i>  sets   out"), "A boy sets out. Really sets out");
  assert.equal(stripTags("  plain  "), "plain");
  assert.equal(stripTags(""), "");
});
