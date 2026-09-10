import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

import { registerAutocompleteTests } from "./editor-autocomplete-scenarios.mjs";

const distribution = process.env.PI_TUI_DIST;
assert.ok(distribution, "PI_TUI_DIST must point to authenticated pi-tui 0.84.2 dist");
const manifest = JSON.parse(await readFile(join(dirname(distribution), "package.json"), "utf8"));
assert.equal(manifest.name, "@earendil-works/pi-tui");
assert.equal(manifest.version, "0.84.2");
assert.equal(
  createHash("sha256")
    .update(await readFile(join(distribution, "components/editor.js")))
    .digest("hex"),
  "a384c140d84e5352605250fab0e1284add133dbdda1e986419c4a0778ffa0853",
  "authenticated components/editor.js SHA-256",
);

const reference = await import(pathToFileURL(join(distribution, "index.js")));
registerAutocompleteTests(reference, "upstream 0.84.2");

const adapter = await import('../index.js');
registerAutocompleteTests(adapter, 'native facade');
