#!/usr/bin/env node
// Harvest M3 core behavior from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-m3-core.mjs
//
// The fixture records source-file digests, never the local oracle path.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fuzzyPath = join(dist, "fuzzy.js");
const keybindingsPath = join(dist, "keybindings.js");
const [{ fuzzyMatch, fuzzyFilter }, keybindingsMod] = await Promise.all([
  import(pathToFileURL(fuzzyPath).href),
  import(pathToFileURL(keybindingsPath).href),
]);
const { KeybindingsManager, TUI_KEYBINDINGS, getKeybindings, setKeybindings } = keybindingsMod;
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: {
      "fuzzy.js": digest(fuzzyPath),
      "keybindings.js": digest(keybindingsPath),
    },
  },
  fuzzyMatch: [],
  fuzzyFilter: [],
  keybindings: [],
  globalKeybindings: {},
};

const fuzzyMatchCases = [
  ["", "anything"],
  ["hello", "hello"],
  ["hlo", "hello"],
  ["helo", "hello"],
  ["hllo", "hello"],
  ["create", "CreateFile"],
  ["cf", "CreateFile"],
  ["ile", "CreateFile"],
  ["xyz", "CreateFile"],
  ["a1", "1abc"],
  ["1a", "1abc"],
  ["ab12", "12ab"],
  ["12ab", "ab12"],
  ["a1b", "a1b9c"],
  ["longerquerythan", "short"],
  ["f", "config file"],
  ["s", "settings"],
  ["完", "完成作业"],
  ["ab", "AB"],
  ["sp", "some/path"],
  ["wrld", "hello-world"],
  ["h", ""],
  ["🎉", "x🎉y"],
  ["🎉", "🎉"],
  ["fé", "FÉlicitations"],
  ["bc", `a\ufeffb-c`],
];
for (const [query, text] of fuzzyMatchCases) {
  const result = fuzzyMatch(query, text);
  out.fuzzyMatch.push({
    query,
    text,
    matches: result.matches,
    score: result.score.toPrecision(17),
  });
}

const fuzzyFilterCases = [
  ["", ["config", "cache-lru-file", "cfg"]],
  ["cfg", ["config", "cache-lru-file", "cfg"]],
  ["set/col", ["set-color", "col-set", "settings columns", "colors"]],
  ["set col", ["set-color", "col-set", "settings columns", "colors"]],
  ["cfg", ["cfg", "config", "cache-lru-file"]],
  ["zzz", ["config", "cfg"]],
  ["mi", ["mi", "im", "mix", "mid"]],
  ["\ufeff", ["first", "second"]],
  ["a", ["alpha", "atom", "beta", "algebra"]],
];
for (const [query, items] of fuzzyFilterCases) {
  out.fuzzyFilter.push({
    query,
    items,
    result: fuzzyFilter(items, query, (item) => item),
  });
}

const definitions = Object.entries(TUI_KEYBINDINGS).map(([id, definition]) => ({
  id,
  defaultKeys: definition.defaultKeys,
  description: definition.description ?? "",
}));

function snapshot(label, userBindings, checks) {
  const userObject = Object.fromEntries(userBindings);
  const manager = new KeybindingsManager(TUI_KEYBINDINGS, userObject);
  const keys = Object.keys(TUI_KEYBINDINGS).map((id) => [id, manager.getKeys(id)]);
  const resolved = Object.entries(manager.getResolvedBindings());
  return {
    label,
    userBindings,
    definitions,
    keys,
    resolved,
    conflicts: manager.getConflicts(),
    checks: checks.map(({ data, id }) => ({ data, id, matches: manager.matches(data, id) })),
  };
}

out.keybindings.push(snapshot("defaults", [], [
  { data: "\x1b[A", id: "tui.select.up" },
  { data: "\x1b[B", id: "tui.select.down" },
  { data: "\r", id: "tui.select.confirm" },
  { data: "\x1b", id: "tui.select.cancel" },
  { data: "x", id: "tui.select.confirm" },
  { data: "\x17", id: "tui.editor.deleteWordBackward" },
  { data: "\x1b[A", id: "tui.nonexistent.binding" },
  { data: "\x01", id: "tui.editor.cursorLineStart" },
  { data: "\x02", id: "tui.editor.cursorLeft" },
]));

out.keybindings.push(snapshot("rebind", [
  ["tui.select.up", ["ctrl+k", "ctrl+k"]],
  ["tui.editor.cursorUp", ["ctrl+p"]],
], [
  { data: "\x1b[A", id: "tui.select.up" },
  { data: "\x0b", id: "tui.select.up" },
  { data: "\x10", id: "tui.editor.cursorUp" },
  { data: "\x1b[A", id: "tui.editor.cursorUp" },
]));

out.keybindings.push(snapshot("conflicts-in-user-order", [
  ["tui.select.up", ["ctrl+k"]],
  ["tui.editor.cursorUp", ["ctrl+k"]],
  ["tui.input.submit", ["ctrl+m"]],
], [
  { data: "\x0b", id: "tui.select.up" },
  { data: "\x0b", id: "tui.editor.cursorUp" },
]));

out.keybindings.push(snapshot("unknown-ignored", [
  ["tui.unknown.action", ["ctrl+z"]],
], [
  { data: "\x1a", id: "tui.unknown.action" },
]));

const replacementManager = new KeybindingsManager(TUI_KEYBINDINGS, {
  "tui.select.cancel": ["escape"],
});
replacementManager.setUserBindings({ "tui.select.cancel": ["ctrl+c"] });
out.keybindings.push({
  ...snapshot("set-user-bindings-replaces", [["tui.select.cancel", ["ctrl+c"]]], [
    { data: "\x1b", id: "tui.select.cancel" },
    { data: "\x03", id: "tui.select.cancel" },
  ]),
  observedUserBindings: Object.entries(replacementManager.getUserBindings()),
});

const sharedManager = new KeybindingsManager(TUI_KEYBINDINGS);
setKeybindings(sharedManager);
const retainedManager = getKeybindings();
const before = retainedManager.matches("\x1b[A", "tui.select.up");
sharedManager.setUserBindings({ "tui.select.up": "ctrl+k" });
out.globalKeybindings = {
  sameIdentity: retainedManager === getKeybindings(),
  before,
  afterOldKey: retainedManager.matches("\x1b[A", "tui.select.up"),
  afterNewKey: retainedManager.matches("\x0b", "tui.select.up"),
  resolved: retainedManager.getResolvedBindings()["tui.select.up"],
};

const fixturePath = join(root, "crates/pie-core/tests/fixtures/m3-core.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log(`wrote M3 core fixture: ${out.fuzzyMatch.length} fuzzyMatch, ${out.fuzzyFilter.length} fuzzyFilter, ${out.keybindings.length} keybinding scenarios`);
