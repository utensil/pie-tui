// gen-golden-stdin.mjs — execute the stdin-buffer spec vectors (docs/specs/stdin-buffer.md §8)
// against the pinned pi-tui StdinBuffer and record emitted events as a fixture.
//   PI_TUI_DIST=... node tools/golden/gen-golden-stdin.mjs
// Timing is factored out per spec §4: the runtime's timeout only decides WHEN flush()
// runs, never WHAT it emits, so vectors process chunks then explicitly flush().
import { writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
const { StdinBuffer } = await import("file://" + join(dist, "stdin-buffer.js"));

const VECTORS = [
  { id: "V1", chunks: ["a", "b"] },
  { id: "V2", chunks: ["\x1b[<35", ";20;5m"] },
  { id: "V3", chunks: ["\x1b"] },
  { id: "V4", chunks: ["\x1b\x1b[27;1u"] },
  { id: "V5", chunks: ["\x1b[200~hello world\x1b[201~"] },
  { id: "V6", chunks: ["x\x1b[200~pasted\x1b[201~y"] },
  { id: "V7", chunks: ["\x1b[200~ab", "\x1b[201~"] },
  { id: "V8", chunks: ["\x1b[97u", "a"] },
  { id: "V9", chunks: ["\x1b[97u", "b"] },
  { id: "V10", chunks: ["\x1b[200~a\x1b[201~b"] },
  { id: "V11", chunks: ["\x1bOA"] },
  { id: "V12", chunks: ["\x1bOP\x1bOQ"] },
  { id: "V13", chunks: ["\x1b[M !!#"] },
  { id: "V14", chunks: ["\x1b]8;;url\x07"] },
  { id: "V15", chunks: ["\x1bP>|xterm\x1b\\"] },
  { id: "V16", chunks: ["\x1bGx"] },
  { id: "V17", chunks: ["\x1b[200~", "\x1b[201~"] },
  { id: "V18", chunks: ["\x1b[<1;2"] },
];

function runVector(chunks) {
  const buf = new StdinBuffer({ timeout: 10 });
  const events = [];
  buf.on("data", (s) => events.push(["data", s]));
  buf.on("paste", (c) => events.push(["paste", c]));
  for (const chunk of chunks) {
    buf.process(chunk);
  }
  for (const s of buf.flush()) {
    events.push(["data", s]);
  }
  return events;
}

const vectors = VECTORS.map(({ id, chunks }) => ({ id, chunks, events: runVector(chunks) }));

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-core/tests");
writeFileSync(
  join(outDir, "fixtures/stdin-golden.json"),
  JSON.stringify({ generator: "gen-golden-stdin.mjs", reference: process.env.PI_TUI_REF_VERSION ?? "0.84.1", vectors }, null, 1) + "\n"
);
console.log(`harvested ${vectors.length} stdin vectors`);
