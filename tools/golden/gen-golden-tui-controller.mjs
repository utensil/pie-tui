#!/usr/bin/env node
// Harvest shared TuiBase controller behavior from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-tui-controller.mjs
//
// All timers and next-tick work are driven by the fake clock below. The
// fixture records source hashes, never the local oracle path.
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
const eastAsianWidthEntry = requireFromUtils.resolve("get-east-asian-width");
const eastAsianWidthRoot = dirname(eastAsianWidthEntry);
const paths = {
  packageJson: join(dist, "..", "package.json"),
  tuiJs: join(dist, "tui.js"),
  tuiDts: join(dist, "tui.d.ts"),
  tuiAltScreenJs: join(dist, "tui-alt-screen.js"),
  tuiAltScreenDts: join(dist, "tui-alt-screen.d.ts"),
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
  eastAsianWidthPackageJson: join(eastAsianWidthRoot, "package.json"),
  eastAsianWidthIndexJs: eastAsianWidthEntry,
  eastAsianWidthIndexDts: join(eastAsianWidthRoot, "index.d.ts"),
  eastAsianWidthLookupJs: join(eastAsianWidthRoot, "lookup.js"),
  eastAsianWidthLookupDataJs: join(eastAsianWidthRoot, "lookup-data.js"),
  eastAsianWidthUtilitiesJs: join(eastAsianWidthRoot, "utilities.js"),
};
const expectedDigests = {
  packageJson: "f7f8f42f7cfa8c53c4f00bdc12c14cb035aa62d9fb73555661ce08b68da61290",
  tuiJs: "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a",
  tuiDts: "0b34c1688da2789a4d73d66a748287c8731f906bee480c5ef79633b66c9ab2f9",
  tuiAltScreenJs: "2886260eca46a0a66cdc9f407777bd17f05200326d495ebf903578c49f298e3a",
  tuiAltScreenDts: "849b7307e671465a5a1dffc77b9381e782db08a6a16a6e3dd8d79b5faac329a6",
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
  eastAsianWidthPackageJson: "d263e50dd1a43aee9acda4d7f066e66b0d0bde1f2852ea6e7153750a5e3a3e52",
  eastAsianWidthIndexJs: "d7b1ba05914c0fc311c20e5618bf8d0893c9c74078a07975e2df981445e64887",
  eastAsianWidthIndexDts: "228354b26813689534d5ecf1e2d1948795fe31bb7fa274b28fbc4056c1ea6ac3",
  eastAsianWidthLookupJs: "c80ecc22b120b27ef5ea9facb7000b8fd4ec037a84d9231d215f1c44bc9c21d0",
  eastAsianWidthLookupDataJs: "f6b40f86c9a2a6808ec808fa8ddcb8da261254cc6121d37ffaeb2bf35dad1d5b",
  eastAsianWidthUtilitiesJs: "4b08a7e9e3ffacbcf198a6abceb2338d52ac671899e52ccc2851c898bfccac42",
};
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
if (new Set(Object.values(paths)).size !== Object.keys(paths).length) {
  throw new Error("oracle provenance paths must be unique");
}
const sourceDigests = Object.fromEntries(Object.entries(paths).map(([key, path]) => [key, digest(path)]));
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
  "tui.js",
  "utils.js",
];
const runtimeClosure = new Set();
const runtimeQueue = ["tui-alt-screen.js"];
while (runtimeQueue.length > 0) {
  const relativePath = runtimeQueue.shift();
  if (runtimeClosure.has(relativePath)) continue;
  runtimeClosure.add(relativePath);
  const source = readFileSync(join(dist, relativePath), "utf8");
  const importPattern = /(?:from\s+|import\s*)["'](\.[^"']+)["']/g;
  for (const match of source.matchAll(importPattern)) {
    const imported = normalize(join(dirname(relativePath), match[1]));
    runtimeQueue.push(imported.endsWith(".js") ? imported : `${imported}.js`);
  }
}
const sortedRuntimeClosure = [...runtimeClosure].sort();
if (JSON.stringify(sortedRuntimeClosure) !== JSON.stringify(expectedRuntimeClosure)) {
  throw new Error(`runtime import closure mismatch: ${JSON.stringify(sortedRuntimeClosure)}`);
}
const pinnedPaths = new Set(Object.values(paths).map((path) => resolve(path)));
for (const relativePath of expectedRuntimeClosure) {
  const jsPath = resolve(join(dist, relativePath));
  const dtsPath = resolve(join(dist, relativePath.replace(/\.js$/, ".d.ts")));
  if (!pinnedPaths.has(jsPath) || !pinnedPaths.has(dtsPath)) {
    throw new Error(`runtime closure source/type pair is not pinned: ${relativePath}`);
  }
}
const referencePackage = JSON.parse(readFileSync(paths.packageJson, "utf8"));
const eastAsianWidthPackage = JSON.parse(readFileSync(paths.eastAsianWidthPackageJson, "utf8"));
if (referencePackage.name !== "@earendil-works/pi-tui" || referencePackage.version !== "0.84.1") {
  throw new Error(`unexpected reference package: ${referencePackage.name}@${referencePackage.version}`);
}
if (eastAsianWidthPackage.name !== "get-east-asian-width" || eastAsianWidthPackage.version !== "1.6.0") {
  throw new Error(`unexpected width package: ${eastAsianWidthPackage.name}@${eastAsianWidthPackage.version}`);
}
if (basename(eastAsianWidthEntry) !== "index.js") {
  throw new Error(`unexpected width entry: ${basename(eastAsianWidthEntry)}`);
}

delete process.env.PI_HARDWARE_CURSOR;
delete process.env.PI_CLEAR_ON_SHRINK;
delete process.env.PI_CODING_AGENT_DIR;

const [{ Container, TuiBase }, { TuiAltScreen }, terminalImage] = await Promise.all([
  import(pathToFileURL(paths.tuiJs).href),
  import(pathToFileURL(paths.tuiAltScreenJs).href),
  import(pathToFileURL(paths.terminalImageJs).href),
]);

const originals = {
  nextTick: process.nextTick,
  setTimeout: globalThis.setTimeout,
  clearTimeout: globalThis.clearTimeout,
  performanceNow: performance.now,
};

class FakeClock {
  constructor(now = 0) {
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
    const normalizedDelay = Math.max(0, Number(delay) || 0);
    this.timers.set(id, {
      id,
      due: this.now + normalizedDelay,
      callback: () => callback(...args),
    });
    this.facts.push(["schedule-timer", id, normalizedDelay, this.now + normalizedDelay]);
    return id;
  }

  clearTimeout(id) {
    if (this.timers.delete(id)) this.facts.push(["cancel-timer", id, this.now]);
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
        if (!this.timers.delete(task.id)) continue;
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
const resetClock = (now = 0) => {
  clock = new FakeClock(now);
  return clock;
};
const settlePromises = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

class FakeTerminal {
  constructor(columns = 80, rows = 24) {
    this.width = columns;
    this.height = rows;
    this.events = [];
    this.onInput = undefined;
    this.onResize = undefined;
  }

  get columns() { return this.width; }
  get rows() { return this.height; }
  get kittyProtocolActive() { return false; }
  start(onInput, onResize) {
    this.events.push(["start"]);
    this.onInput = onInput;
    this.onResize = onResize;
  }
  stop() {
    this.events.push(["stop"]);
    this.onInput = undefined;
    this.onResize = undefined;
  }
  write(data) { this.events.push(["write", data]); }
  hideCursor() { this.events.push(["hide-cursor"]); }
  showCursor() { this.events.push(["show-cursor"]); }
  feed(data) { this.onInput?.(data); }
  resize(columns, rows) {
    this.width = columns;
    this.height = rows;
    this.onResize?.();
  }
}

class ProbeTui extends TuiBase {
  mode = "regular";
  constructor(terminal, showHardwareCursor = false) {
    super(terminal, showHardwareCursor, "/oracle/logs");
    this.events = [];
  }
  doRender() { this.events.push(["render", clock.now]); }
  resetRenderState() { this.events.push(["reset", clock.now]); }
  beforeTerminalStart() { this.events.push(["before-start"]); }
  afterTerminalStart() { this.events.push(["after-start"]); }
  beforeTerminalStop(options) { this.events.push(["before-stop", options.preserveScreen ?? false]); }
  afterTerminalStop(options) { this.events.push(["after-stop", options.preserveScreen ?? false]); }
}

class ProbeComponent {
  constructor(name, lines = [name]) {
    this.name = name;
    this.lines = lines;
    this.focused = false;
    this.wantsKeyRelease = false;
    this.events = [];
  }
  render(width) {
    this.events.push(["render", width]);
    return this.lines;
  }
  invalidate() { this.events.push(["invalidate"]); }
  handleInput(data) { this.events.push(["input", data]); }
}

const cases = [];

{
  resetClock(100);
  terminalImage.setCapabilities({ images: "kitty", trueColor: true, hyperlinks: true });
  const terminal = new FakeTerminal(90, 30);
  const tui = new ProbeTui(terminal);
  tui.setTerminalColorSchemeNotifications(true);
  tui.start();
  const beforeFlush = {
    terminal: [...terminal.events],
    tui: [...tui.events],
    clock: [...clock.facts],
  };
  clock.flushCurrent();
  const afterInitial = { tui: [...tui.events], clock: [...clock.facts] };
  clock.advance(5);
  tui.requestRender();
  tui.requestRender();
  clock.runTicks();
  const coalesced = { tui: [...tui.events], clock: [...clock.facts] };
  tui.requestRender(true);
  clock.runTicks();
  const forced = { tui: [...tui.events], clock: [...clock.facts] };
  clock.advance(20);
  tui.requestRender();
  clock.runTicks();
  tui.stop({ preserveScreen: true });
  clock.advance(20);
  cases.push({
    name: "start-render-coalescing-stop",
    beforeFlush,
    afterInitial,
    coalesced,
    forced,
    final: { terminal: terminal.events, tui: tui.events, clock: clock.facts },
  });
}

{
  resetClock(100);
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const terminal = new FakeTerminal(80, 24);
  const tui = new ProbeTui(terminal);
  const focused = new ProbeComponent("focus");
  tui.setFocus(focused);
  tui.start();
  clock.flushCurrent();
  tui.events.length = 0;
  clock.facts.length = 0;

  terminal.feed("x");
  terminal.resize(81, 25);
  const beforeFlush = {
    componentEvents: [...focused.events],
    renders: [...tui.events],
    clock: [...clock.facts],
  };
  clock.flushCurrent();
  cases.push({
    name: "terminal-callbacks-drive-input-and-resize",
    beforeFlush,
    componentEvents: focused.events,
    renders: tui.events,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const terminal = new FakeTerminal();
  const tui = new ProbeTui(terminal);
  const focused = new ProbeComponent("focus");
  const listenerEvents = [];
  const first = (data) => {
    listenerEvents.push(["first", data]);
    return { data: `<${data}>` };
  };
  const second = (data) => {
    listenerEvents.push(["second", data]);
    if (data.includes("block")) return { consume: true, data: "ignored" };
    return { data: `${data}!` };
  };
  const third = (data) => {
    listenerEvents.push(["third", data]);
    return undefined;
  };
  const removeFirst = tui.addInputListener(first);
  tui.addInputListener(first);
  tui.addInputListener(second);
  tui.addInputListener(third);
  tui.setFocus(focused);
  tui.handleTerminalInput("x");
  clock.flushCurrent();
  tui.handleTerminalInput("block");
  clock.flushCurrent();
  removeFirst();
  tui.handleTerminalInput("y");
  clock.flushCurrent();
  const removeEmpty = tui.addInputListener((data) => {
    listenerEvents.push(["empty", data]);
    return { data: "" };
  });
  tui.handleTerminalInput("z");
  clock.flushCurrent();
  removeEmpty();
  const beforeRelease = [...focused.events];
  tui.handleTerminalInput("\x1b[97;1:3u");
  clock.flushCurrent();
  const filteredRelease = [...focused.events];
  focused.wantsKeyRelease = true;
  tui.handleTerminalInput("\x1b[97;1:3u");
  clock.flushCurrent();
  let debugCalls = 0;
  tui.onDebug = () => { debugCalls += 1; };
  tui.handleTerminalInput("\x1b[100;6u");
  clock.flushCurrent();
  const rawDebugTui = new ProbeTui(new FakeTerminal());
  const rawDebugFocus = new ProbeComponent("raw-debug");
  let rawDebugCalls = 0;
  rawDebugTui.setFocus(rawDebugFocus);
  rawDebugTui.onDebug = () => { rawDebugCalls += 1; };
  rawDebugTui.handleTerminalInput("\x1b[100;6u");
  clock.flushCurrent();
  cases.push({
    name: "listeners-transform-consume-release-debug",
    listenerEvents,
    componentEvents: focused.events,
    beforeRelease,
    filteredRelease,
    debugCalls,
    rawDebugCalls,
    rawDebugComponentEvents: rawDebugFocus.events,
    renders: tui.events,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const focused = new ProbeComponent("focus");
  const listenerEvents = [];
  let removeB = () => {};
  tui.addInputListener((data) => {
    listenerEvents.push(["a", data]);
    removeB();
  });
  removeB = tui.addInputListener((data) => listenerEvents.push(["b", data]));
  tui.addInputListener((data) => listenerEvents.push(["c", data]));
  tui.setFocus(focused);
  tui.handleTerminalInput("x");
  clock.flushCurrent();
  const firstDispatch = [...listenerEvents];
  tui.handleTerminalInput("y");
  clock.flushCurrent();
  cases.push({
    name: "input-listener-live-set-mutation",
    firstDispatch,
    listenerEvents,
    componentEvents: focused.events,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  tui.onDebug = () => {
    events.push("first");
    tui.onDebug = () => events.push("replacement");
  };
  tui.handleTerminalInput("\x1b[100;6u");
  tui.handleTerminalInput("\x1b[100;6u");
  cases.push({ name: "debug-callback-replacement", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const focused = new ProbeComponent("focus");
  const events = [];
  tui.addInputListener((data) => {
    events.push(["a", data]);
    if (data === "outer") tui.handleTerminalInput("inner");
  });
  tui.addInputListener((data) => events.push(["b", data]));
  tui.setFocus(focused);
  tui.handleTerminalInput("outer");
  clock.flushCurrent();
  cases.push({
    name: "recursive-input-listener-dispatch",
    events,
    componentEvents: focused.events,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  tui.onDebug = () => {
    events.push(events.length === 0 ? "outer" : "inner");
    if (events.length === 1) tui.handleTerminalInput("\x1b[100;6u");
  };
  tui.handleTerminalInput("\x1b[100;6u");
  cases.push({ name: "recursive-debug-dispatch", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const component = new ProbeComponent("focus");
  const events = [];
  let focused = false;
  Object.defineProperty(component, "focused", {
    configurable: true,
    get: () => focused,
    set: (value) => {
      focused = value;
      events.push(["focused", value]);
      tui.requestRender();
    },
  });
  tui.setFocus(component);
  clock.flushCurrent();
  clock.advance(16);
  cases.push({
    name: "reentrant-focus-setter-request-render",
    events,
    focused,
    renders: tui.events,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const terminal = new FakeTerminal(81, 25);
  const tui = new ProbeTui(terminal);
  const overlay = new ProbeComponent("overlay");
  const events = [];
  tui.showOverlay(overlay, {
    visible: (width, height) => {
      events.push(["visible", width, height]);
      tui.requestRender();
      return true;
    },
  });
  clock.flushCurrent();
  clock.advance(16);
  cases.push({
    name: "reentrant-visible-predicate-request-render",
    events,
    focused: overlay.focused,
    terminal: terminal.events,
    renders: tui.events,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const component = new ProbeComponent("root");
  const events = [];
  component.invalidate = () => {
    events.push("invalidate");
    tui.requestRender();
  };
  tui.addChild(component);
  tui.invalidate();
  clock.flushCurrent();
  clock.advance(16);
  cases.push({
    name: "reentrant-invalidate-request-render",
    events,
    renders: tui.events,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  const a = new ProbeComponent("a");
  const b = new ProbeComponent("b");
  a.invalidate = () => {
    events.push("a");
    tui.removeChild(b);
  };
  b.invalidate = () => events.push("b");
  tui.addChild(a);
  tui.addChild(b);
  tui.invalidate();
  cases.push({ name: "invalidate-live-root-deletion", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  const a = new ProbeComponent("a");
  const c = new ProbeComponent("c");
  let appended = false;
  a.invalidate = () => {
    events.push("a");
    if (!appended) {
      appended = true;
      tui.addChild(c);
    }
  };
  c.invalidate = () => events.push("c");
  tui.addChild(a);
  tui.invalidate();
  cases.push({ name: "invalidate-live-root-insertion", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  const a = new ProbeComponent("a");
  const b = new ProbeComponent("b");
  const d = new ProbeComponent("d");
  const e = new ProbeComponent("e");
  a.invalidate = () => {
    events.push("a");
    tui.clear();
    tui.addChild(d);
    tui.addChild(e);
  };
  b.invalidate = () => events.push("b");
  d.invalidate = () => events.push("d");
  e.invalidate = () => events.push("e");
  tui.addChild(a);
  tui.addChild(b);
  tui.invalidate();
  cases.push({ name: "invalidate-root-clear-rebinds-array", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  const a = new ProbeComponent("a");
  const b = new ProbeComponent("b");
  let bHandle;
  a.invalidate = () => {
    events.push("a");
    bHandle.hide();
  };
  b.invalidate = () => events.push("b");
  tui.showOverlay(a, { nonCapturing: true });
  bHandle = tui.showOverlay(b, { nonCapturing: true });
  tui.invalidate();
  cases.push({ name: "invalidate-live-overlay-deletion", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  const a = new ProbeComponent("a");
  const c = new ProbeComponent("c");
  let appended = false;
  a.invalidate = () => {
    events.push("a");
    if (!appended) {
      appended = true;
      tui.showOverlay(c, { nonCapturing: true });
    }
  };
  c.invalidate = () => events.push("c");
  tui.showOverlay(a, { nonCapturing: true });
  tui.invalidate();
  cases.push({ name: "invalidate-live-overlay-insertion", events });
}

{
  resetClock(0);
  terminalImage.setCellDimensions({ widthPx: 9, heightPx: 18 });
  const terminal = new FakeTerminal(80, 24);
  const tui = new ProbeTui(terminal);
  const base = new ProbeComponent("base");
  const overlay = new ProbeComponent("overlay");
  tui.addChild(base);
  const handle = tui.showOverlay(overlay, { nonCapturing: true });
  const listenerEvents = [];
  tui.addInputListener((data) => {
    listenerEvents.push(data);
    return undefined;
  });
  tui.handleTerminalInput("\x1b[6;0;7t");
  const zero = {
    dimensions: terminalImage.getCellDimensions(),
    base: [...base.events],
    overlay: [...overlay.events],
    tui: [...tui.events],
  };
  tui.handleTerminalInput("\x1b[6;20;10t");
  clock.flushCurrent();
  const valid = {
    dimensions: terminalImage.getCellDimensions(),
    base: [...base.events],
    overlay: [...overlay.events],
    tui: [...tui.events],
  };
  handle.hide();
  cases.push({ name: "cell-size-priority-and-invalidation", listenerEvents, zero, valid });
}

{
  resetClock(0);
  const terminal = new FakeTerminal();
  const tui = new ProbeTui(terminal);
  const results = [];
  const first = tui.queryTerminalBackgroundColor({ timeoutMs: 10 });
  const second = tui.queryTerminalBackgroundColor({ timeoutMs: 30 });
  first.then((value) => results.push(["first", value ?? null]));
  second.then((value) => results.push(["second", value ?? null]));
  clock.advance(10);
  await settlePromises();
  const afterTimeout = [...results];
  tui.handleTerminalInput("\x1b]11;#112233\x07");
  await settlePromises();
  const afterStaleReply = [...results];
  tui.handleTerminalInput("\x1b]11;rgb:ffff/0000/8080\x1b\\");
  await settlePromises();
  const afterSecondReply = [...results];
  const invalid = tui.queryTerminalBackgroundColor({ timeoutMs: 50 });
  invalid.then((value) => results.push(["invalid", value ?? null]));
  tui.handleTerminalInput("\x1b]11;bogus\x07");
  await settlePromises();
  cases.push({
    name: "osc11-fifo-timeout-and-parse",
    terminal: terminal.events,
    afterTimeout,
    afterStaleReply,
    afterSecondReply,
    final: results,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const terminal = new FakeTerminal();
  const tui = new ProbeTui(terminal);
  const notifications = [];
  const removeA = tui.onTerminalColorSchemeChange((scheme) => notifications.push(["a", scheme]));
  tui.onTerminalColorSchemeChange((scheme) => notifications.push(["b", scheme]));
  const queryResults = [];
  tui.queryTerminalColorScheme({ timeoutMs: 20 }).then((scheme) => queryResults.push(scheme ?? null));
  tui.handleTerminalInput("\x1b[?997;1n\x1b[?997;2n");
  await settlePromises();
  removeA();
  tui.handleTerminalInput("\x1b[?997;1n");
  tui.queryTerminalColorScheme({ timeoutMs: 5 }).then((scheme) => queryResults.push(scheme ?? null));
  clock.advance(5);
  await settlePromises();
  tui.setTerminalColorSchemeNotifications(true);
  tui.setTerminalColorSchemeNotifications(true);
  tui.setTerminalColorSchemeNotifications(false);
  cases.push({
    name: "color-scheme-listeners-query-notifications",
    notifications,
    queryResults,
    terminal: terminal.events,
    clock: clock.facts,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const persistentEvents = [];
  const orderedEvents = [];
  let removeB = () => {};
  tui.onTerminalColorSchemeChange((scheme) => {
    persistentEvents.push(["a", scheme]);
    orderedEvents.push(["a", scheme]);
    removeB();
  });

  const registerScheme = tui.onTerminalColorSchemeChange.bind(tui);
  tui.onTerminalColorSchemeChange = (listener) => registerScheme((scheme) => {
    orderedEvents.push(["query", scheme]);
    listener(scheme);
  });
  const queryResults = [];
  tui.queryTerminalColorScheme({ timeoutMs: 20 }).then((scheme) => queryResults.push(scheme ?? null));
  tui.onTerminalColorSchemeChange = registerScheme;

  removeB = tui.onTerminalColorSchemeChange((scheme) => {
    persistentEvents.push(["b", scheme]);
    orderedEvents.push(["b", scheme]);
  });
  tui.onTerminalColorSchemeChange((scheme) => {
    persistentEvents.push(["c", scheme]);
    orderedEvents.push(["c", scheme]);
  });

  tui.handleTerminalInput("\x1b[?997;2n");
  await settlePromises();
  const firstDispatch = [...orderedEvents];
  tui.handleTerminalInput("\x1b[?997;1n");
  await settlePromises();
  cases.push({
    name: "scheme-listener-live-set-and-query-order",
    firstDispatch,
    persistentEvents,
    orderedEvents,
    queryResults,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal());
  const events = [];
  tui.onTerminalColorSchemeChange((scheme) => {
    events.push(["a", scheme]);
    if (scheme === "light") tui.handleTerminalInput("\x1b[?997;1n");
  });
  tui.onTerminalColorSchemeChange((scheme) => events.push(["b", scheme]));
  tui.handleTerminalInput("\x1b[?997;2n");
  cases.push({ name: "recursive-scheme-listener-dispatch", events });
}

{
  resetClock(0);
  const terminal = new FakeTerminal(40, 12);
  const tui = new ProbeTui(terminal);
  const rootComponent = new ProbeComponent("root");
  const overlayA = new ProbeComponent("A", ["AAAA"]);
  const overlayB = new ProbeComponent("B", ["BBBB"]);
  const hidden = new ProbeComponent("hidden", ["NO"]);
  tui.addChild(rootComponent);
  tui.setFocus(rootComponent);
  const states = [];
  const snap = (label) => states.push({
    label,
    root: rootComponent.focused,
    a: overlayA.focused,
    b: overlayB.focused,
    hidden: hidden.focused,
    hasOverlay: tui.hasOverlay(),
    hasEntries: tui.hasOverlayEntries,
  });
  snap("root");
  const a = tui.showOverlay(overlayA, { anchor: "top-left", width: 4 });
  snap("show-a");
  const b = tui.showOverlay(overlayB, { anchor: "bottom-right", width: 4 });
  snap("show-b");
  const invisible = tui.showOverlay(hidden, { visible: () => false });
  snap("show-invisible");
  b.unfocus();
  snap("unfocus-b");
  b.focus();
  snap("refocus-b");
  b.setHidden(true);
  snap("hide-b-temporary");
  b.setHidden(false);
  snap("show-b-again");
  b.hide();
  snap("remove-b");
  a.hide();
  snap("remove-a");
  invisible.hide();
  snap("remove-invisible");
  cases.push({ name: "overlay-focus-stack-ownership", states, terminal: terminal.events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal(40, 12));
  const events = [];
  tui.showOverlay(new ProbeComponent("first"), {
    nonCapturing: true,
    visible: () => { events.push("first"); return true; },
  });
  tui.showOverlay(new ProbeComponent("second"), {
    nonCapturing: true,
    visible: () => { events.push("second"); return true; },
  });
  events.length = 0;
  const result = tui.hasOverlay();
  cases.push({ name: "has-overlay-short-circuits-later-visible", result, events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal(40, 12));
  const events = [];
  tui.showOverlay(new ProbeComponent("noncapturing"), {
    nonCapturing: true,
    visible: () => { events.push("noncapturing"); return true; },
  });
  const capturing = new ProbeComponent("capturing");
  tui.showOverlay(capturing, {
    visible: () => { events.push("capturing"); return true; },
  });
  events.length = 0;
  const topmost = tui.getTopmostVisibleOverlay();
  cases.push({
    name: "topmost-skips-noncapturing-before-visible",
    topmost: topmost?.component.name,
    events,
  });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal(40, 12));
  const events = [];
  tui.showOverlay(new ProbeComponent("noncapturing"), {
    nonCapturing: true,
    visible: () => { events.push("visible"); return true; },
  });
  cases.push({ name: "show-noncapturing-does-not-evaluate-visible", events });
}

{
  resetClock(0);
  const tui = new ProbeTui(new FakeTerminal(40, 12));
  const events = [];
  let second;
  tui.showOverlay(new ProbeComponent("first"), {
    nonCapturing: true,
    visible: () => {
      events.push("first");
      second.hide();
      return false;
    },
  });
  second = tui.showOverlay(new ProbeComponent("second"), {
    nonCapturing: true,
    visible: () => { events.push("second"); return false; },
  });
  tui.showOverlay(new ProbeComponent("third"), {
    nonCapturing: true,
    visible: () => { events.push("third"); return true; },
  });
  events.length = 0;
  const result = tui.hasOverlay();
  cases.push({ name: "live-reentrant-visibility-mutation-ordering", result, events });
}

{
  resetClock(0);
  const events = [];
  const child = new ProbeComponent("child");
  child.invalidate = () => events.push("child-invalidate");
  const layout = new ProbeComponent("layout");
  layout.invalidate = () => events.push("layout-invalidate");
  const tui = Object.create(TuiAltScreen.prototype);
  tui.children = [child];
  tui.overlayStack = [];
  tui.layoutRoot = undefined;
  tui.currentLayout = { marker: "before-change" };
  tui.requestRender = () => events.push("render-request");

  tui.setLayoutRoot(layout);
  const cacheClearedOnChange = tui.currentLayout === undefined;
  tui.currentLayout = { marker: "same-identity" };
  tui.setLayoutRoot(layout);
  const sameIdentityPreservedCache = tui.currentLayout?.marker === "same-identity";
  const mountedWithLayout = tui.getMountedRoots().map((component) => component.name);
  tui.invalidate();
  tui.setLayoutRoot(undefined);
  const mountedAfterClear = tui.getMountedRoots().map((component) => component.name);
  tui.invalidate();
  cases.push({
    name: "alt-layout-root-identity-cache-and-mounted-roots",
    events,
    cacheClearedOnChange,
    sameIdentityPreservedCache,
    mountedWithLayout,
    mountedAfterClear,
  });
}

{
  resetClock(0);
  const terminal = new FakeTerminal(100, 30);
  const tui = new ProbeTui(terminal);
  const layouts = [
    ["default", tui.resolveOverlayLayout(undefined, 10, 100, 30)],
    ["percent-margins", tui.resolveOverlayLayout({ width: "50%", maxHeight: "25%", row: "100%", col: "0%", margin: { top: 2, right: 3, bottom: 4, left: 5 } }, 20, 100, 30)],
    ["min-clamp-offset", tui.resolveOverlayLayout({ width: 2, minWidth: 200, anchor: "bottom-right", offsetX: 9, offsetY: 9, margin: -3 }, 5, 20, 8)],
    ["invalid-percent", tui.resolveOverlayLayout({ width: "bad", row: "bad", col: "bad" }, 3, 11, 7)],
    ["absolute-clamp", tui.resolveOverlayLayout({ width: 6, row: -20, col: 99 }, 4, 10, 6)],
  ];
  const low = new ProbeComponent("low", ["1111", "2222"]);
  const high = new ProbeComponent("high", ["XX"]);
  const lowHandle = tui.showOverlay(low, { width: 4, row: 1, col: 2, nonCapturing: true });
  const highHandle = tui.showOverlay(high, { width: 2, row: 1, col: 3, nonCapturing: true });
  const composited = tui.compositeOverlays(["abcdefghij"], 10, 4);
  highHandle.setHidden(true);
  const withoutHigh = tui.compositeOverlays(["abcdefghij"], 10, 4);
  lowHandle.hide();
  highHandle.hide();

  const invisibleTui = new ProbeTui(new FakeTerminal(10, 4));
  invisibleTui.showOverlay(new ProbeComponent("invisible"), { visible: () => false });
  const invisibleOnly = invisibleTui.compositeOverlays(["x"], 10, 4);
  const hiddenTui = new ProbeTui(new FakeTerminal(10, 4));
  const hiddenHandle = hiddenTui.showOverlay(new ProbeComponent("hidden"), { nonCapturing: true });
  hiddenHandle.setHidden(true);
  const hiddenOnly = hiddenTui.compositeOverlays(["x"], 10, 4);
  cases.push({
    name: "overlay-layout-and-composition",
    layouts,
    composited,
    withoutHigh,
    invisibleOnly,
    hiddenOnly,
  });
}

{
  resetClock(0);
  terminalImage.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
  const terminal = new FakeTerminal();
  const tui = new ProbeTui(terminal);
  tui.start();
  clock.runTicks();
  tui.stop();
  const afterFirstStop = [...terminal.events];
  tui.stop();
  clock.flushCurrent();
  cases.push({
    name: "no-image-cell-query-and-repeated-stop",
    afterFirstStop,
    final: terminal.events,
    tui: tui.events,
    clock: clock.facts,
  });
}

{
  resetClock(100);
  class ReentrantRenderTui extends ProbeTui {
    renderCount = 0;
    doRender() {
      this.events.push(["render", clock.now]);
      this.renderCount += 1;
      if (this.renderCount === 1) this.requestRender();
    }
  }
  const tui = new ReentrantRenderTui(new FakeTerminal());
  tui.requestRender();
  clock.flushCurrent();
  const afterFirstFrame = {
    renders: [...tui.events],
    pendingTimers: [...clock.timers.values()].map(({ due }) => due),
  };
  clock.advance(16);
  cases.push({
    name: "reentrant-render-schedules-follow-up-frame",
    afterFirstFrame,
    renders: tui.events,
    clock: clock.facts,
  });
}

process.nextTick = originals.nextTick;
globalThis.setTimeout = originals.setTimeout;
globalThis.clearTimeout = originals.clearTimeout;
performance.now = originals.performanceNow;

const fixture = {
  generator: "tools/golden/gen-golden-tui-controller.mjs",
  reference: {
    package: referencePackage.name,
    version: referencePackage.version,
    node: process.versions.node,
    icu: process.versions.icu,
    unicode: process.versions.unicode,
    platform: process.platform,
    arch: process.arch,
    sourceDigests,
    widthDependency: {
      package: eastAsianWidthPackage.name,
      version: eastAsianWidthPackage.version,
    },
  },
  cases,
};

const text = `${JSON.stringify(fixture, null, 2)}\n`;
const out = join(root, "crates", "pie-app", "tests", "fixtures", "tui-controller.json");
if (process.argv.includes("--check")) {
  if (readFileSync(out, "utf8") !== text) {
    console.error("tui-controller fixture is stale");
    process.exit(1);
  }
} else {
  writeFileSync(out, text);
  console.log(`harvested ${cases.length} TuiBase controller cases`);
}
