#!/usr/bin/env node
// Harvest terminal capability priority/cache behavior from pinned pi-tui.
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-terminal-capabilities.mjs
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const imagePath = join(dist, "terminal-image.js");
const image = await import(pathToFileURL(imagePath).href);
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const keys = [
  "TERM_PROGRAM", "TERMINAL_EMULATOR", "TERM", "COLORTERM", "TMUX",
  "KITTY_WINDOW_ID", "GHOSTTY_RESOURCES_DIR", "WEZTERM_PANE", "WARP_SESSION_ID",
  "WARP_TERMINAL_SESSION_UUID", "ITERM_SESSION_ID", "WT_SESSION",
];
const original = Object.fromEntries(keys.map((key) => [key, process.env[key]]));

function setEnv(env) {
  for (const key of keys) delete process.env[key];
  for (const [key, value] of Object.entries(env)) process.env[key] = value;
}

const scenarios = [
  ["unknown", {}, false],
  ["unknown-truecolor", { COLORTERM: "TRUECOLOR" }, false],
  ["unknown-24bit", { COLORTERM: "24Bit" }, false],
  ["tmux-env-blocks-kitty", { TMUX: "socket", KITTY_WINDOW_ID: "1", COLORTERM: "truecolor" }, true],
  ["tmux-wins-over-screen", { TMUX: "socket", TERM: "screen-256color" }, true],
  ["tmux-term-probe-false", { TERM: "TmUx-256Color", TERM_PROGRAM: "iTerm.app" }, false],
  ["screen-blocks-kitty", { TERM: "screen-256color", KITTY_WINDOW_ID: "1", COLORTERM: "truecolor" }, true],
  ["kitty-env", { KITTY_WINDOW_ID: "1" }, false],
  ["kitty-program", { TERM_PROGRAM: "KiTtY" }, false],
  ["ghostty-program", { TERM_PROGRAM: "Ghostty" }, false],
  ["ghostty-term", { TERM: "xterm-ghostty" }, false],
  ["ghostty-env", { GHOSTTY_RESOURCES_DIR: "/opt/ghostty" }, false],
  ["wezterm-env", { WEZTERM_PANE: "3" }, false],
  ["wezterm-program", { TERM_PROGRAM: "WezTerm" }, false],
  ["warp-program", { TERM_PROGRAM: "WarpTerminal" }, false],
  ["warp-session", { WARP_SESSION_ID: "s" }, false],
  ["warp-uuid", { WARP_TERMINAL_SESSION_UUID: "u" }, false],
  ["iterm-session", { ITERM_SESSION_ID: "s" }, false],
  ["iterm-program", { TERM_PROGRAM: "iTerm.app" }, false],
  ["windows-terminal", { WT_SESSION: "s" }, false],
  ["vscode", { TERM_PROGRAM: "vscode" }, false],
  ["alacritty", { TERM_PROGRAM: "Alacritty" }, false],
  ["jetbrains", { TERMINAL_EMULATOR: "JetBrains-JediTerm", COLORTERM: "truecolor" }, false],
  ["kitty-wins-over-iterm", { KITTY_WINDOW_ID: "1", ITERM_SESSION_ID: "s" }, false],
  ["empty-tmux-is-false", { TMUX: "", TERM_PROGRAM: "kitty" }, true],
];

const cases = [];
for (const [label, env, probeResult] of scenarios) {
  setEnv(env);
  let probeCalls = 0;
  const result = image.detectCapabilities(() => {
    probeCalls += 1;
    return probeResult;
  });
  cases.push({ label, env, probeResult, probeCalls, result });
}

setEnv({ TERM_PROGRAM: "kitty" });
image.resetCapabilitiesCache();
const first = image.getCapabilities();
const second = image.getCapabilities();
setEnv({ TERM_PROGRAM: "iTerm.app" });
const stale = image.getCapabilities();
image.resetCapabilitiesCache();
const refreshed = image.getCapabilities();
const explicit = { images: null, trueColor: false, hyperlinks: true };
image.setCapabilities(explicit);
const overridden = image.getCapabilities();
const cache = {
  first,
  firstSecondSame: first === second,
  stale,
  firstStaleSame: first === stale,
  refreshed,
  refreshedSameAsFirst: refreshed === first,
  overridden,
  overrideSameIdentity: overridden === explicit,
};

for (const key of keys) {
  if (original[key] === undefined) delete process.env[key];
  else process.env[key] = original[key];
}
image.resetCapabilitiesCache();

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fixturePath = join(root, "crates/pie-term/tests/fixtures/terminal-capabilities.json");
writeFileSync(fixturePath, `${JSON.stringify({
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: { "terminal-image.js": digest(imagePath) },
    platform: process.platform,
  },
  cases,
  cache,
}, null, 2)}\n`);
console.log(`wrote terminal capability fixture: ${cases.length} cases`);
