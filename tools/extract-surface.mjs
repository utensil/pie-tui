#!/usr/bin/env node
// extract-surface.mjs — harvest the public export surface of the pinned @earendil-works/pi-tui
// build into tools/surface-manifest.json. Run from repo root:
//   PI_TUI_DIST=<path-to-dist> node tools/extract-surface.mjs
// The manifest records version + source digest so oracle drift is detectable (AGENTS.md).
import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set; pass the path to pi-tui's dist/ directory");
  process.exit(64);
}
const pkgPath = join(dirname(dist), "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else yield p;
  }
}

const entries = [];
for (const file of walk(dist)) {
  if (!file.endsWith(".d.ts") || file.endsWith(".d.ts.map")) continue;
  const rel = file.slice(dist.length + 1);
  if (rel.endsWith("/index.d.ts") && rel !== "index.d.ts") continue; // nested module barrels only if referenced
  const text = readFileSync(file, "utf8");
  // Join multi-line `export {...}` statements: scan char-wise with brace depth.
  let i = 0;
  while (i < text.length) {
    const start = text.indexOf("export", i);
    if (start === -1) break;
    const lineStart = text.lastIndexOf("\n", start) + 1;
    const before = text.slice(lineStart, start).trim();
    if (before !== "" && !before.startsWith("*")) { i = start + 6; continue; }
    let depth = 0, end = -1, inStr = null;
    for (let j = start; j < text.length; j++) {
      const c = text[j];
      if (inStr) {
        if (c === "\\") { j++; continue; }
        if (c === inStr) inStr = null;
        continue;
      }
      if (c === "'" || c === '"' || c === "`") { inStr = c; continue; }
      if (c === "{") depth++;
      else if (c === "}") {
        depth--;
        if (depth === 0) { end = text.indexOf(";", j); break; }
      } else if (depth === 0 && c === ";" && !text.slice(start, j).includes("{")) {
        end = j; break;
      }
      if (c === "\n" && depth === 0 && !text.slice(start, j + 1).includes("export ")) { /* keep scanning */ }
    }
    if (end === -1) end = text.length;
    entries.push({ rel, stmt: text.slice(start, end + 1).replace(/\s+/g, " ").trim() });
    i = end + 1;
  }
}

const manifest = {
  generator: "tools/extract-surface.mjs",
  reference: {
    package: pkg.name,
    version: pkg.version,
    sourceDigest: createHash("sha256")
      .update(readFileSync(join(dist, "index.d.ts")))
      .digest("hex")
      .slice(0, 12),
    exportedStatementCount: entries.length,
  },
  note:
    "Each entry is one export statement from the reference .d.ts tree. Porting status is kept " +
    "in tools/surface-coverage.json (separate so re-extraction never clobbers progress).",
  statements: entries,
};

writeFileSync(join(root, "tools/surface-manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`manifest written: ${entries.length} export statements from ${pkg.name}@${pkg.version}`);
