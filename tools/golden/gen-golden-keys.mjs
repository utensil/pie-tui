// gen-golden-keys.mjs — harvest parseKey/matchesKey vectors from pinned pi-tui.
// PI_TUI_DIST=... node tools/golden/gen-golden-keys.mjs
import { writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const dist = process.env.PI_TUI_DIST;
if (!dist) { console.error("PI_TUI_DIST not set"); process.exit(64); }
const keys = await import("file://" + join(dist, "keys.js"));
const q = String.raw;
const inputs = [
  ...["up","down","left","right","home","end","insert","delete","pageUp","pageDown","clear","f1","f2","f3","f4","f5","f6","f7","f8","f9","f10","f11","f12"]
    .flatMap(k => [keys.LEGACY && null].filter(Boolean)),
];
// Build corpus programmatically from well-known sequences (kept explicit for review):
const seq = q`\x1b`;
const inputs2 = [
  "\x1b[A","\x1b[B","\x1b[C","\x1b[D","\x1bOA","\x1bOB","\x1bOC","\x1bOD","\x1bOH","\x1bOF",
  "\x1b[3~","\x1b[5~","\x1b[6~","\x1b[1~","\x1b[4~","\x1b[7~","\x1b[8~","\x1b[2~",
  "\x1b[[5~","\x1b[[6~","\x1bOP","\x1bOQ","\x1bOR","\x1bOS","\x1b[11~","\x1b[15~","\x1b[17~","\x1b[24~",
  "\x1b[a","\x1b[b","\x1b[c","\x1b[d","\x1bOa","\x1bOb","\x1bOc","\x1bOd","\x1b[e","\x1bOE","\x1b[2$","\x1b[3^","\x1b[5$","\x1b[8^",
  "\x1b[Z","\t","\r","\n","\0"," ","\x7f","\x08","\x1b","\x1c","\x1d","\x1f",
  "\x1b\x1b","\x1b\x1c","\x1b\x1d","\x1b\x1f","\x1b\r","\x1b ","\x1b\x7f","\x1b\b","\x1bB","\x1bF","\x1bp","\x1bn",
  "a","z","A","Z","0","9","+","?","\\","_","-",":",
  "\x01","\x05","\x19","\x07","\x02\x03",
  "\x1b[97;2u","\x1b[97;5u","\x1b[99u","\x1b[13;2u","\x1b[57414;3u","\x1b[27u","\x1b[32;5u",
  "\x1b[1;2A","\x1b[1;5C","\x1b[1;3D","\x1b[1;2H","\x1b[1;3F","\x1b[3;5~","\x1b[2;3~",
  "\x1b[27;2;99~","\x1b[27;4;115~","\x1b[13;4u","\x1b[97:65;2u","\x1b[57441u","\x1b[57399u","x","",
];
const keyIds = [
  "escape","esc","tab","space","enter","return","backspace","delete","insert","clear",
  "home","end","pageUp","pageDown","up","down","left","right",
  "f1","f12","a","z","0","9","+","?","_",
  "ctrl+c","ctrl+z","ctrl+[","ctrl+space","ctrl+-","shift+a","shift+tab","shift+enter",
  "alt+enter","alt+backspace","alt+c","alt+x","alt+up","alt+down","alt+left","alt+right",
  "super+k","ctrl+shift+p","ctrl+alt+x","shift+up","ctrl+left","ctrl+pageUp","shift+insert","ctrl+delete",
];

const capture = () => ({
  parseKey: inputs2.map((d) => keys.parseKey(d) ?? null),
  release: inputs2.map((d) => keys.isKeyRelease(d)),
  repeat: inputs2.map((d) => keys.isKeyRepeat(d)),
  matrix: keyIds.flatMap((id, i) =>
    inputs2.map((d, j) => keys.matchesKey(d, id) ? `${i}:${j}` : null).filter(Boolean)
  ),
});
const modes = {};
keys.setKittyProtocolActive(false);
modes.legacy = capture();
keys.setKittyProtocolActive(true);
modes.kitty = capture();
keys.setKittyProtocolActive(false);

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-core/tests");
writeFileSync(join(outDir, "fixtures/keys-golden.json"),
  JSON.stringify({ generator: "gen-golden-keys.mjs", inputs: inputs2, keyIds, modes }, null, 1) + "\n");
console.log(`keys-golden.json written (${inputs2.length} inputs x ${keyIds.length} keyIds x 2 modes)`);
