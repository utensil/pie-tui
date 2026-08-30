#!/usr/bin/env node
// Harvest TuiMainScreen/TuiAltScreen behavior from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-main-alt-controller.mjs
//
// Oracle provenance is checked before any reference module is imported. Timers,
// next-tick work, and click time are deterministic; no live terminal is used.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join, normalize, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const requireFromUtils = createRequire(pathToFileURL(join(dist, "utils.js")));
const widthEntry = requireFromUtils.resolve("get-east-asian-width");
const widthRoot = dirname(widthEntry);

const sourceFiles = {
  packageJson: join(dist, "..", "package.json"),
  tuiMainScreenJs: join(dist, "tui-main-screen.js"),
  tuiMainScreenDts: join(dist, "tui-main-screen.d.ts"),
  tuiAltScreenJs: join(dist, "tui-alt-screen.js"),
  tuiAltScreenDts: join(dist, "tui-alt-screen.d.ts"),
  tuiJs: join(dist, "tui.js"),
  tuiDts: join(dist, "tui.d.ts"),
  terminalDts: join(dist, "terminal.d.ts"),
  altScreenFlashJs: join(dist, "components", "alt-screen-flash.js"),
  altScreenFlashDts: join(dist, "components", "alt-screen-flash.d.ts"),
  scrollViewJs: join(dist, "components", "scroll-view.js"),
  scrollViewDts: join(dist, "components", "scroll-view.d.ts"),
  stackJs: join(dist, "components", "stack.js"),
  stackDts: join(dist, "components", "stack.d.ts"),
  keybindingsJs: join(dist, "keybindings.js"),
  keybindingsDts: join(dist, "keybindings.d.ts"),
  keysJs: join(dist, "keys.js"),
  keysDts: join(dist, "keys.d.ts"),
  layoutJs: join(dist, "layout.js"),
  layoutDts: join(dist, "layout.d.ts"),
  layoutNodeJs: join(dist, "layout-node.js"),
  layoutNodeDts: join(dist, "layout-node.d.ts"),
  terminalColorsJs: join(dist, "terminal-colors.js"),
  terminalColorsDts: join(dist, "terminal-colors.d.ts"),
  terminalImageJs: join(dist, "terminal-image.js"),
  terminalImageDts: join(dist, "terminal-image.d.ts"),
  utilsJs: join(dist, "utils.js"),
  utilsDts: join(dist, "utils.d.ts"),
  widthPackageJson: join(widthRoot, "package.json"),
  widthIndexJs: widthEntry,
  widthIndexDts: join(widthRoot, "index.d.ts"),
  widthLookupJs: join(widthRoot, "lookup.js"),
  widthLookupDataJs: join(widthRoot, "lookup-data.js"),
  widthUtilitiesJs: join(widthRoot, "utilities.js"),
};

const expectedDigests = {
  packageJson: "f7f8f42f7cfa8c53c4f00bdc12c14cb035aa62d9fb73555661ce08b68da61290",
  tuiMainScreenJs: "c039d81fb26597669b13fd249dd4bf685c9a7608e3eb0c9bb3460df89271cd49",
  tuiMainScreenDts: "f2441514370d3f5726347aa5c58a2215f3da70f708a33e4784545ef96224b73f",
  tuiAltScreenJs: "2886260eca46a0a66cdc9f407777bd17f05200326d495ebf903578c49f298e3a",
  tuiAltScreenDts: "849b7307e671465a5a1dffc77b9381e782db08a6a16a6e3dd8d79b5faac329a6",
  tuiJs: "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a",
  tuiDts: "0b34c1688da2789a4d73d66a748287c8731f906bee480c5ef79633b66c9ab2f9",
  terminalDts: "ddaeac54d2c3db8e04a71626dcc7c4721655e4ee8452f94da7290c09a60f4d34",
  altScreenFlashJs: "6ca2016101ca570a94fdaa18bfe8edbc6734243cb5363d21110e809fcd47db12",
  altScreenFlashDts: "b8e5800da49d8d88ed59d6fee341e64362d2b43e0b806b21d8c00562ad146a86",
  scrollViewJs: "796fdaa30bfb850df9d3e9647cd7b08c1bf3f3775335ce90a158c177c282f53f",
  scrollViewDts: "7ee8cf2eced7d5a3cc68f0f054c9813f93b43019bdd72745b56f0c603b13f097",
  stackJs: "02b4dafebc728f1c0e8d01b5cc330f82eb760c58bc71c5cc9bff6d98bf34dbf3",
  stackDts: "4a263287262dd550b75213d4234e84e9060ad4536d78d42bcb052e8012a7c212",
  keybindingsJs: "d27090a36394fc4f59350e7f3234c601082d950e179ba6742d9557aae2a72168",
  keybindingsDts: "93450b5ff2259c52767d4bc3dffb17d7c9341f866507cf00aba67cddf42b51b0",
  keysJs: "14b18205fd5e56ed3b183392c82bd72e41ba3dab1d345e47b2b17af6988493cc",
  keysDts: "58d05b6227c8657e2109931eb2875de3a675e7bccc7f5eafde5467d539636344",
  layoutJs: "fdc6c58b4245e735a0daabdc93201017e77cbbb01d7d440eda6427270556b2af",
  layoutDts: "cfa0950012579f3912d7f6887a2b24f8618fa5e7eb0df15447ec992cf806a40a",
  layoutNodeJs: "73c3942b68d52ed29072f1f78184c99d405f9259bb4e24a1b6b0e3688381f7f5",
  layoutNodeDts: "8a19dd70f320755c3793cbec86902ad037afedd4151f1d174c0175cc281cf77d",
  terminalColorsJs: "e26c8c31d161d175817b3335baab4476737719c389a2a39312aa2ece67ccb119",
  terminalColorsDts: "8f24828eefa8b3a6f0290bc2c8f3d6d03c9cf69c70558f008576991a77168e45",
  terminalImageJs: "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2",
  terminalImageDts: "ba498675c6f16339fe04c329dcd95757743f0f6d22a18879b2fda6e9e8b4d8ec",
  utilsJs: "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
  utilsDts: "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
  widthPackageJson: "d263e50dd1a43aee9acda4d7f066e66b0d0bde1f2852ea6e7153750a5e3a3e52",
  widthIndexJs: "d7b1ba05914c0fc311c20e5618bf8d0893c9c74078a07975e2df981445e64887",
  widthIndexDts: "228354b26813689534d5ecf1e2d1948795fe31bb7fa274b28fbc4056c1ea6ac3",
  widthLookupJs: "c80ecc22b120b27ef5ea9facb7000b8fd4ec037a84d9231d215f1c44bc9c21d0",
  widthLookupDataJs: "f6b40f86c9a2a6808ec808fa8ddcb8da261254cc6121d37ffaeb2bf35dad1d5b",
  widthUtilitiesJs: "4b08a7e9e3ffacbcf198a6abceb2338d52ac671899e52ccc2851c898bfccac42",
};

const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
if (new Set(Object.values(sourceFiles)).size !== Object.keys(sourceFiles).length) {
  throw new Error("oracle provenance paths must be unique");
}
const sourceDigests = Object.fromEntries(
  Object.entries(sourceFiles).map(([key, path]) => [key, digest(path)]),
);
for (const [key, expected] of Object.entries(expectedDigests)) {
  if (sourceDigests[key] !== expected) {
    throw new Error(`${key} digest mismatch: ${sourceDigests[key]} != ${expected}`);
  }
}

const expectedRuntimeClosure = [
  "components/alt-screen-flash.js",
  "components/scroll-view.js",
  "components/stack.js",
  "keybindings.js",
  "keys.js",
  "layout-node.js",
  "layout.js",
  "terminal-colors.js",
  "terminal-image.js",
  "tui-alt-screen.js",
  "tui-main-screen.js",
  "tui.js",
  "utils.js",
];
const runtimeClosure = new Set();
const runtimeQueue = ["tui-main-screen.js", "tui-alt-screen.js"];
while (runtimeQueue.length > 0) {
  const relativePath = runtimeQueue.shift();
  if (runtimeClosure.has(relativePath)) continue;
  runtimeClosure.add(relativePath);
  const source = readFileSync(join(dist, relativePath), "utf8");
  const importPattern = /(?:from\s+|import\s*)["'](\.[^"']+)["']/g;
  for (const match of source.matchAll(importPattern)) {
    const imported = normalize(join(dirname(relativePath), match[1]));
    const importedJs = imported.endsWith(".js") ? imported : `${imported}.js`;
    runtimeQueue.push(importedJs);
  }
}
const sortedRuntimeClosure = [...runtimeClosure].sort();
if (JSON.stringify(sortedRuntimeClosure) !== JSON.stringify(expectedRuntimeClosure)) {
  throw new Error(`runtime import closure mismatch: ${JSON.stringify(sortedRuntimeClosure)}`);
}
const pinnedPaths = new Set(Object.values(sourceFiles).map((path) => resolve(path)));
for (const relativePath of expectedRuntimeClosure) {
  const jsPath = resolve(join(dist, relativePath));
  const dtsPath = resolve(join(dist, relativePath.replace(/\.js$/, ".d.ts")));
  if (!pinnedPaths.has(jsPath) || !pinnedPaths.has(dtsPath)) {
    throw new Error(`runtime closure source/type pair is not pinned: ${relativePath}`);
  }
}

const referencePackage = JSON.parse(readFileSync(sourceFiles.packageJson, "utf8"));
const widthPackage = JSON.parse(readFileSync(sourceFiles.widthPackageJson, "utf8"));
if (referencePackage.name !== "@earendil-works/pi-tui" || referencePackage.version !== "0.84.1") {
  throw new Error(`unexpected reference package: ${referencePackage.name}@${referencePackage.version}`);
}
if (widthPackage.name !== "get-east-asian-width" || widthPackage.version !== "1.6.0") {
  throw new Error(`unexpected width package: ${widthPackage.name}@${widthPackage.version}`);
}
if (basename(widthEntry) !== "index.js") {
  throw new Error(`unexpected width entry: ${basename(widthEntry)}`);
}

for (const name of [
  "PI_CLEAR_ON_SHRINK",
  "PI_CODING_AGENT_DIR",
  "PI_DEBUG_REDRAW",
  "PI_HARDWARE_CURSOR",
  "PI_TUI_DEBUG",
  "STY",
  "TERMUX_VERSION",
  "TMUX",
  "ZELLIJ",
]) {
  delete process.env[name];
}
process.env.TERM = "xterm-256color";

const [
  { TuiMainScreen },
  { TuiAltScreen },
  tuiModule,
  terminalImage,
  keybindingsModule,
] = await Promise.all([
  import(pathToFileURL(sourceFiles.tuiMainScreenJs).href),
  import(pathToFileURL(sourceFiles.tuiAltScreenJs).href),
  import(pathToFileURL(sourceFiles.tuiJs).href),
  import(pathToFileURL(sourceFiles.terminalImageJs).href),
  import(pathToFileURL(sourceFiles.keybindingsJs).href),
]);
const { CURSOR_MARKER } = tuiModule;
const { KeybindingsManager, TUI_KEYBINDINGS, setKeybindings } = keybindingsModule;

const originals = {
  nextTick: process.nextTick,
  setTimeout: globalThis.setTimeout,
  clearTimeout: globalThis.clearTimeout,
  performanceNow: performance.now,
  dateNow: Date.now,
};

class FakeClock {
  constructor(now = 100) {
    this.now = now;
    this.nextId = 1;
    this.ticks = [];
    this.timers = new Map();
    this.facts = [];
  }

  nextTick(callback, ...args) {
    const id = this.nextId++;
    this.ticks.push({ id, callback: () => callback(...args) });
    this.facts.push(["schedule-tick", id, this.now]);
  }

  setTimeout(callback, delay = 0, ...args) {
    const id = this.nextId++;
    const handle = { id, unref() {} };
    const normalizedDelay = Math.max(0, Number(delay) || 0);
    this.timers.set(handle, {
      id,
      handle,
      due: this.now + normalizedDelay,
      callback: () => callback(...args),
    });
    this.facts.push(["schedule-timer", id, normalizedDelay, this.now + normalizedDelay]);
    return handle;
  }

  clearTimeout(handle) {
    if (this.timers.delete(handle)) {
      this.facts.push(["cancel-timer", handle.id, this.now]);
    }
  }

  runTicks() {
    while (this.ticks.length > 0) {
      const task = this.ticks.shift();
      this.facts.push(["run-tick", task.id, this.now]);
      task.callback();
    }
  }

  runDue() {
    let ran = true;
    while (ran) {
      ran = false;
      const due = [...this.timers.values()]
        .filter((task) => task.due <= this.now)
        .sort((a, b) => a.due - b.due || a.id - b.id);
      for (const task of due) {
        if (!this.timers.delete(task.handle)) continue;
        this.facts.push(["run-timer", task.id, task.due]);
        task.callback();
        this.runTicks();
        ran = true;
      }
    }
  }

  flushCurrent() {
    this.runTicks();
    this.runDue();
  }

  advance(ms) {
    this.now += ms;
    this.runDue();
  }
}

let clock = new FakeClock();
process.nextTick = (callback, ...args) => clock.nextTick(callback, ...args);
globalThis.setTimeout = (callback, delay, ...args) => clock.setTimeout(callback, delay, ...args);
globalThis.clearTimeout = (id) => clock.clearTimeout(id);
performance.now = () => clock.now;
Date.now = () => clock.now;
const resetClock = (now = 100) => {
  clock = new FakeClock(now);
  return clock;
};

class FakeTerminal {
  constructor(columns, rows, trace) {
    this.width = columns;
    this.height = rows;
    this.trace = trace;
    this.events = [];
    this.onInput = undefined;
    this.onResize = undefined;
  }

  get columns() { return this.width; }
  get rows() { return this.height; }
  get kittyProtocolActive() { return false; }
  start(onInput, onResize) {
    this.record("start");
    this.onInput = onInput;
    this.onResize = onResize;
  }
  stop() {
    this.record("stop");
    this.onInput = undefined;
    this.onResize = undefined;
  }
  async drainInput() { this.record("drain-input"); }
  write(data) { this.record("write", data); }
  moveBy(lines) { this.record("move-by", lines); }
  hideCursor() { this.record("hide-cursor"); }
  showCursor() { this.record("show-cursor"); }
  clearLine() { this.record("clear-line"); }
  clearFromCursor() { this.record("clear-from-cursor"); }
  clearScreen() { this.record("clear-screen"); }
  setTitle(title) { this.record("set-title", title); }
  setProgress(active) { this.record("set-progress", active); }
  feed(data) {
    this.trace.push(["terminal", "input", data]);
    this.onInput?.(data);
  }
  resize(columns, rows) {
    this.width = columns;
    this.height = rows;
    this.trace.push(["terminal", "resize", columns, rows]);
    this.onResize?.();
  }
  record(kind, value) {
    const event = value === undefined ? [kind] : [kind, value];
    this.events.push(event);
    this.trace.push(["terminal", ...event]);
  }
}

class ProbeComponent {
  constructor(name, lines, trace) {
    this.name = name;
    this.lines = [...lines];
    this.trace = trace;
    this._focused = false;
    this.wantsKeyRelease = false;
  }

  get focused() { return this._focused; }
  set focused(value) {
    this._focused = value;
    this.trace.push([this.name, "focus", value]);
  }
  render(width) {
    this.trace.push([this.name, "render", width]);
    return [...this.lines];
  }
  invalidate() { this.trace.push([this.name, "invalidate"]); }
  handleInput(data) { this.trace.push([this.name, "input", data]); }
}

const phase = (terminal, trace) => {
  const snapshot = {
    terminal: terminal.events.map((event) => [...event]),
    trace: trace.map((event) => [...event]),
    clock: clock.facts.map((event) => [...event]),
  };
  terminal.events.length = 0;
  trace.length = 0;
  clock.facts.length = 0;
  return snapshot;
};

const cases = [];

{
  resetClock();
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(8, 3, trace);
  const main = new TuiMainScreen(terminal, true, "/oracle/logs");
  const rootComponent = new ProbeComponent("root", ["one", `tw${CURSOR_MARKER}o`], trace);
  main.addChild(rootComponent);
  main.setFocus(rootComponent);
  main.start();
  clock.flushCurrent();
  const initial = phase(terminal, trace);

  rootComponent.lines = ["one", `T${CURSOR_MARKER}WO`];
  main.renderNow();
  const differential = phase(terminal, trace);

  terminal.resize(10, 4);
  clock.flushCurrent();
  clock.advance(16);
  const resize = phase(terminal, trace);

  main.stop();
  cases.push({
    name: "main-lifecycle-diff-resize-cursor-stop",
    initial,
    differential,
    resize,
    stop: phase(terminal, trace),
    mode: main.mode,
    fullRedraws: main.fullRedraws,
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(5, 3, trace);
  const main = new TuiMainScreen(terminal, false, "/oracle/logs");
  const rootComponent = new ProbeComponent("long", ["0", "1", "2", "3", "4"], trace);
  main.addChild(rootComponent);
  main.start();
  clock.flushCurrent();
  const initial = { ...phase(terminal, trace), state: main.captureRenderState() };

  rootComponent.lines[4] = "X";
  main.renderNow();
  const visibleChange = { ...phase(terminal, trace), state: main.captureRenderState() };

  rootComponent.lines.push("5", "6");
  main.renderNow();
  const append = { ...phase(terminal, trace), state: main.captureRenderState() };

  rootComponent.lines[0] = "Z";
  main.renderNow();
  const aboveViewport = { ...phase(terminal, trace), state: main.captureRenderState() };
  main.stop({ preserveScreen: true });
  cases.push({
    name: "main-long-document-viewport-and-preserved-stop",
    initial,
    visibleChange,
    append,
    aboveViewport,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: "kitty", trueColor: true, hyperlinks: true });
  const trace = [];
  const terminal = new FakeTerminal(8, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: false });
  const rawImage = (imageId) => `\x1b_Ga=T,f=100,q=2,c=1,r=1,i=${imageId};QUFBQQ==\x1b\\`;
  const rootComponent = new ProbeComponent("raw-kitty", [rawImage(1)], trace);
  alt.addChild(rootComponent);
  alt.start();
  clock.flushCurrent();
  phase(terminal, trace);

  const renderWrites = [];
  for (let imageId = 2; imageId <= 18; imageId++) {
    rootComponent.lines[0] = rawImage(imageId);
    alt.renderNow();
    renderWrites.push(...terminal.events
      .filter(([kind]) => kind === "write")
      .map(([, write]) => write));
    phase(terminal, trace);
  }
  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-raw-unregistered-kitty-lines-are-not-owned",
    renderWrites,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: "kitty", trueColor: true, hyperlinks: true });
  const trace = [];
  const terminal = new FakeTerminal(8, 4, trace);
  const main = new TuiMainScreen(terminal, false, "/oracle/logs");
  const image = terminalImage.renderImage(
    "QUJDRA==",
    { widthPx: 18, heightPx: 36 },
    { imageId: 7, maxWidthCells: 2 },
  );
  const rootComponent = new ProbeComponent("image", [image.sequence, "", "tail"], trace);
  main.addChild(rootComponent);
  main.start();
  clock.flushCurrent();
  const captured = main.captureRenderState();
  const initial = phase(terminal, trace);

  rootComponent.lines = ["gone", "tail"];
  main.renderNow();
  const removal = phase(terminal, trace);

  const restored = new TuiMainScreen(new FakeTerminal(8, 4, []), false, "/oracle/logs");
  restored.restoreRenderState(captured);
  main.stop({ preserveScreen: true });
  cases.push({
    name: "main-kitty-ownership-and-render-state-restore",
    image: { columns: image.columns, rows: image.rows, imageId: image.imageId },
    initial,
    removal,
    captured,
    restored: restored.captureRenderState(),
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: "kitty", trueColor: true, hyperlinks: true });
  const trace = [];
  const terminal = new FakeTerminal(8, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: false });
  const images = Array.from({ length: 18 }, (_, index) => terminalImage.renderImage(
    "QUFBQQ==",
    { widthPx: 9, heightPx: 18 },
    { imageId: index + 1, maxWidthCells: 1 },
  ));
  const rootComponent = new ProbeComponent("kitty-cache", [images[0].sequence], trace);
  alt.addChild(rootComponent);
  alt.start();
  clock.flushCurrent();
  phase(terminal, trace);

  let atBound;
  let eviction;
  for (let imageId = 2; imageId <= 18; imageId++) {
    rootComponent.lines[0] = images[imageId - 1].sequence;
    alt.renderNow();
    const rendered = phase(terminal, trace);
    if (imageId === 17) atBound = rendered;
    if (imageId === 18) eviction = rendered;
  }
  rootComponent.lines[0] = images[0].sequence;
  alt.renderNow();
  const revisit = phase(terminal, trace);
  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-kitty-offscreen-cache-eviction-and-revisit",
    atBound,
    eviction,
    revisit,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(6, 3, trace);
  const alt = new TuiAltScreen(terminal, true, "/oracle/logs", { mouse: true });
  const rootComponent = new ProbeComponent("doc", ["a", "b", "c", "d", `e${CURSOR_MARKER}`], trace);
  alt.addChild(rootComponent);
  alt.setFocus(rootComponent);
  alt.start();
  clock.flushCurrent();
  const initial = {
    ...phase(terminal, trace),
    viewportTop: alt.viewportTop,
    following: alt.isFollowingOutput,
  };

  rootComponent.lines[4] = `E${CURSOR_MARKER}`;
  alt.renderNow();
  const differential = phase(terminal, trace);

  terminal.resize(7, 4);
  clock.flushCurrent();
  clock.advance(16);
  const resize = {
    ...phase(terminal, trace),
    viewportTop: alt.viewportTop,
    following: alt.isFollowingOutput,
  };

  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-lifecycle-diff-resize-preserved-stop",
    initial,
    differential,
    resize,
    stop: phase(terminal, trace),
    mode: alt.mode,
    fullRedraws: alt.fullRedraws,
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(8, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: false });
  const child = new ProbeComponent("child", ["child"], trace);
  const layoutRoot = new ProbeComponent("layout", ["layout-0", "layout-1"], trace);
  const overlay = new ProbeComponent("overlay", ["OV"], trace);
  alt.addChild(child);
  alt.setFocus(child);
  alt.setLayoutRoot(layoutRoot);
  const handle = alt.showOverlay(overlay, {
    anchor: "top-left",
    width: 3,
    visible: (width, height) => {
      trace.push(["overlay-visible", width, height]);
      return true;
    },
  });
  alt.start();
  clock.flushCurrent();
  const withLayoutAndOverlay = {
    ...phase(terminal, trace),
    childFocused: child.focused,
    overlayFocused: overlay.focused,
    handleFocused: handle.isFocused(),
  };

  alt.setLayoutRoot(layoutRoot);
  clock.flushCurrent();
  const identityNoop = phase(terminal, trace);

  alt.invalidate();
  const invalidate = phase(terminal, trace);

  handle.hide();
  alt.setLayoutRoot(undefined);
  alt.renderNow();
  const restoredChildren = {
    ...phase(terminal, trace),
    childFocused: child.focused,
    overlayFocused: overlay.focused,
  };

  alt.stop();
  cases.push({
    name: "alt-layout-root-focus-overlay-and-main-screen-restore",
    withLayoutAndOverlay,
    identityNoop,
    invalidate,
    restoredChildren,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(10, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", {
    mouse: true,
    wheelScrollLines: 2,
  });
  const rootComponent = new ProbeComponent(
    "scroll",
    ["zero", "one", "two", "three", "four", "five", "six"],
    trace,
  );
  alt.addChild(rootComponent);
  alt.setFocus(rootComponent);
  alt.start();
  clock.flushCurrent();
  phase(terminal, trace);
  const states = [];
  const capture = (name) => states.push({
    name,
    viewportTop: alt.viewportTop,
    following: alt.isFollowingOutput,
    phase: phase(terminal, trace),
  });

  terminal.feed("\x1b[<64;1;1M");
  alt.renderNow();
  capture("wheel-up");

  terminal.feed("\x1b[5;1:3~");
  alt.renderNow();
  capture("page-up-release");

  terminal.feed("\x1b[5~");
  alt.renderNow();
  capture("page-up-press");

  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS, {
    "tui.altScreen.pageUp": "f1",
  }));
  terminal.feed("\x1b[5~");
  alt.renderNow();
  capture("old-binding-after-replacement");
  terminal.feed("\x1bOP");
  alt.renderNow();
  capture("new-binding-after-replacement");
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));

  terminal.feed("\x1b[F");
  alt.renderNow();
  capture("bottom");
  terminal.feed("\x1b[H");
  alt.renderNow();
  capture("top");
  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-scroll-mouse-release-filter-and-live-keybindings",
    states,
    stop: phase(terminal, trace),
  });
}

{
  resetClock(1000);
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(12, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: true });
  const rootComponent = new ProbeComponent("select", ["alpha beta", "second", "third"], trace);
  alt.addChild(rootComponent);
  alt.start();
  clock.flushCurrent();
  phase(terminal, trace);
  const clicks = [];
  const click = (name) => {
    terminal.feed("\x1b[<0;2;1M");
    terminal.feed("\x1b[<0;2;1m");
    alt.renderNow();
    clicks.push({ name, phase: phase(terminal, trace) });
    clock.advance(10);
  };
  click("single");
  click("double-word");
  click("triple-line");

  terminal.feed("\x1b[<0;1;2M");
  terminal.feed("\x1b[<32;4;2M");
  terminal.feed("\x1b[O");
  terminal.feed("\x1b[<0;4;2m");
  alt.renderNow();
  const focusOutCancellation = phase(terminal, trace);
  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-selection-clipboard-granularity-and-focus-out",
    clicks,
    focusOutCancellation,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: "kitty", trueColor: true, hyperlinks: true });
  const trace = [];
  const terminal = new FakeTerminal(8, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: false });
  const firstImage = terminalImage.renderImage(
    "QUFBQQ==",
    { widthPx: 9, heightPx: 18 },
    { imageId: 9, maxWidthCells: 1 },
  );
  const rootComponent = new ProbeComponent("kitty", [firstImage.sequence, "tail"], trace);
  alt.addChild(rootComponent);
  alt.start();
  clock.flushCurrent();
  const first = phase(terminal, trace);

  rootComponent.lines[1] = "TAIL";
  alt.renderNow();
  const textOnlyChange = phase(terminal, trace);

  const secondImage = terminalImage.renderImage(
    "QkJCQg==",
    { widthPx: 9, heightPx: 18 },
    { imageId: 9, maxWidthCells: 1 },
  );
  rootComponent.lines[0] = secondImage.sequence;
  alt.renderNow();
  const retransmit = phase(terminal, trace);

  rootComponent.lines[0] = "plain";
  alt.renderNow();
  const removal = phase(terminal, trace);
  alt.stop({ preserveScreen: true });
  cases.push({
    name: "alt-kitty-transmission-placement-and-teardown-ownership",
    first,
    textOnlyChange,
    retransmit,
    removal,
    stop: phase(terminal, trace),
  });
}

{
  resetClock();
  terminalImage.setCapabilities({ images: "iterm2", trueColor: true, hyperlinks: true });
  const trace = [];
  const terminal = new FakeTerminal(5, 3, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: false });
  const image = terminalImage.renderImage(
    "QUJDRA==",
    { widthPx: 9, heightPx: 18 },
    { maxWidthCells: 1 },
  );
  const rootComponent = new ProbeComponent(
    "iterm",
    [image.sequence, "\x1b]133;A\x07abcdef", `x${CURSOR_MARKER}`],
    trace,
  );
  alt.addChild(rootComponent);
  alt.start();
  clock.flushCurrent();
  const during = {
    ...phase(terminal, trace),
    capabilities: terminalImage.getCapabilities(),
  };
  alt.stop();
  cases.push({
    name: "alt-iterm-capability-suspension-and-unpreserved-stop",
    image: { columns: image.columns, rows: image.rows },
    during,
    stop: phase(terminal, trace),
    restoredCapabilities: terminalImage.getCapabilities(),
  });
}

{
  resetClock();
  process.env.TMUX = "oracle";
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const trace = [];
  const terminal = new FakeTerminal(4, 2, trace);
  const alt = new TuiAltScreen(terminal, false, "/oracle/logs", { mouse: true });
  alt.addChild(new ProbeComponent("mux", ["x"], trace));
  alt.start();
  clock.flushCurrent();
  alt.stop({ preserveScreen: true });
  delete process.env.TMUX;
  cases.push({
    name: "alt-multiplexer-button-motion-lifecycle",
    lifecycle: phase(terminal, trace),
  });
}

process.nextTick = originals.nextTick;
globalThis.setTimeout = originals.setTimeout;
globalThis.clearTimeout = originals.clearTimeout;
performance.now = originals.performanceNow;
Date.now = originals.dateNow;

const fixture = {
  generator: "tools/golden/gen-golden-main-alt-controller.mjs",
  reference: {
    package: referencePackage.name,
    version: referencePackage.version,
    node: process.versions.node,
    icu: process.versions.icu,
    unicode: process.versions.unicode,
    platform: process.platform,
    arch: process.arch,
    sourceDigests,
    runtimeClosure: sortedRuntimeClosure,
    widthDependency: {
      package: widthPackage.name,
      version: widthPackage.version,
    },
  },
  cases,
};

const text = `${JSON.stringify(fixture, null, 2)}\n`;
const out = join(root, "crates", "pie-app", "tests", "fixtures", "main-alt-controller.json");
if (process.argv.includes("--check")) {
  if (readFileSync(out, "utf8") !== text) {
    console.error("main-alt-controller fixture is stale");
    process.exit(1);
  }
} else {
  writeFileSync(out, text);
  console.log(`harvested ${cases.length} Main/Alt controller cases`);
}
