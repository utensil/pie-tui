// gen-golden-render.mjs — drive the pinned pi-tui TuiMainScreen through scripted
// frame scenarios with a recording terminal; capture the exact write buffers so the
// Rust diff renderer can be asserted byte-for-byte.
//   PI_TUI_DIST=... node tools/golden/gen-golden-render.mjs
import { writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
// Deterministic environment for the reference.
delete process.env.PI_HARDWARE_CURSOR;
delete process.env.PI_CLEAR_ON_SHRINK;
delete process.env.PI_DEBUG_REDRAW;
delete process.env.TERMUX_VERSION;
process.env.TERM_PROGRAM = "";

const { TuiMainScreen } = await import("file://" + join(dist, "tui-main-screen.js"));

class RecordingTerminal {
  constructor(width, height) {
    this.columns = width;
    this.rows = height;
    this.writes = [];
  }
  write(data) {
    this.writes.push(data);
  }
  hideCursor() {
    this.write("\x1b[?25l");
  }
  showCursor() {
    this.write("\x1b[?25h");
  }
  moveBy(lines) {
    if (lines > 0) this.write(`\x1b[${lines}B`);
    else if (lines < 0) this.write(`\x1b[${-lines}A`);
  }
  clearLine() { this.write("\x1b[K"); }
  clearFromCursor() { this.write("\x1b[J"); }
  clearScreen() { this.write("\x1b[2J\x1b[H"); }
  setTitle(t) { this.write(`\x1b]0;${t}\x07`); }
  setProgress() {}
  stop() {}
  drainInput() {}
  get kittyProtocolActive() { return false; }
}

// Line-content palette (deterministic; includes ANSI + CJK + wide chars).
const L = {
  a: "alpha line",
  b: "beta line with more content",
  c: "\x1b[31mred line\x1b[0m tail",
  d: "delta 日本語 wide",
  e: "epsilon",
  x: "\x1b[1;32mchanged bold-green\x1b[0m",
  z: "ζ-last line with ANSI \x1b[4munderline\x1b[24m",
  cursor: "prompt> \x1b_pi:c\x07",
  // narrow variants for width-20 scenarios (visible width ≤ 20)
  n1: "narrow-α",
  n2: "\x1b[35mnarrow-magenta\x1b[0m",
  n3: "窄い narrow line",
  n4: "one more line",
  n5: "and another",
};

function scenario(name, width, height, frames) {
  return {
    name,
    width,
    height,
    frames: frames.map((value) => Array.isArray(value) ? { width, height, lines: value } : value),
  };
}

function frame(width, height, lines) {
  return { width, height, lines };
}

const scenarios = [
  scenario("first-render-then-append", 40, 8, [
    [L.a, L.b, L.c],
    [L.a, L.b, L.c, L.d],
    [L.a, L.b, L.c, L.d, L.e],
  ]),
  scenario("modify-middle", 40, 8, [
    [L.a, L.b, L.c],
    [L.a, L.x, L.c],
  ]),
  scenario("shrink-content", 40, 8, [
    [L.a, L.b, L.c, L.d],
    [L.a],
  ]),
  scenario("grow-after-shrink", 40, 8, [
    [L.a, L.b, L.c],
    [L.a],
    [L.a, L.b, L.c, L.z],
  ]),
  scenario("width-change-full", 40, 8, [
    frame(40, 8, [L.a, L.b, L.c]),
    frame(30, 8, [L.a, L.b, L.c]),
    frame(30, 8, [L.a, L.b, L.c]),
  ]),
  scenario("height-change-full", 30, 6, [
    frame(30, 6, [L.a, L.b]),
    frame(30, 4, [L.a, L.b]),
    frame(30, 4, [L.a, L.b]),
  ]),
  scenario("no-change-cursor-only", 40, 8, [
    [L.a, L.b],
    [L.a, L.b],
  ]),
  scenario("cursor-marker", 40, 8, [
    [L.a, L.cursor],
    [L.a, L.cursor],
  ]),
  scenario("content-exceeds-height", 20, 3, [
    [L.n1, L.n2, L.n3],
    [L.n1, L.n2, L.n3, L.n4],
    [L.n1, L.n2, L.n3, L.n4, L.n5],
    [L.n1, L.n2, L.n3, L.n4, L.n5, L.n2],
  ]),
  scenario("shrink-below-height", 20, 3, [
    [L.n1, L.n2, L.n3, L.n4, L.n5, L.n2],
    [L.n1],
  ]),
  scenario("empty-to-content", 40, 8, [
    [],
    [L.a],
  ]),
];

const out = [];
for (const sc of scenarios) {
  const terminal = new RecordingTerminal(sc.width, sc.height);
  const tui = new TuiMainScreen(terminal, false, "/tmp/pi-golden-unused");
  tui.stopped = false;
  const frameWrites = [];
  // First frame may need requestRender/force; call renderNow directly for determinism.
  for (let i = 0; i < sc.frames.length; i++) {
    const { width, height, lines } = sc.frames[i];
    terminal.columns = width;
    terminal.rows = height;
    tui.render = (w) => lines.map((l) => l);
    terminal.writes = [];
    const before = tui.previousWidth;
    if (i === 0) {
      tui.renderNow(true); // force
    } else {
      tui.renderNow();
    }
    frameWrites.push(terminal.writes.slice());
    void before;
  }
  out.push({
    name: sc.name,
    width: sc.width,
    height: sc.height,
    frames: sc.frames,
    frameWrites,
  });
}

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-term/tests");
writeFileSync(join(outDir, "fixtures/render-golden.json"), JSON.stringify({ generator: "gen-golden-render.mjs", reference: process.env.PI_TUI_REF_VERSION ?? "0.84.1", scenarios: out }, null, 1) + "\n");
console.log(`harvested ${scenarios.length} render scenarios`);
