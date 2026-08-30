#!/usr/bin/env node
// Harvest the M5 layout/ScrollView contract from the pinned pi-tui build.
// The checked-in fixture records package metadata, source digests, and vectors.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-layout-scroll.mjs
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
const sourceFiles = {
  "index.d.ts": join(dist, "index.d.ts"),
  "index.js": join(dist, "index.js"),
  "tui.d.ts": join(dist, "tui.d.ts"),
  "tui.js": join(dist, "tui.js"),
  "layout.d.ts": join(dist, "layout.d.ts"),
  "layout.js": join(dist, "layout.js"),
  "layout-node.d.ts": join(dist, "layout-node.d.ts"),
  "layout-node.js": join(dist, "layout-node.js"),
  "utils.d.ts": join(dist, "utils.d.ts"),
  "utils.js": join(dist, "utils.js"),
  "terminal-image.d.ts": join(dist, "terminal-image.d.ts"),
  "terminal-image.js": join(dist, "terminal-image.js"),
  "components/stack.d.ts": join(dist, "components/stack.d.ts"),
  "components/stack.js": join(dist, "components/stack.js"),
  "components/v-stack.d.ts": join(dist, "components/v-stack.d.ts"),
  "components/v-stack.js": join(dist, "components/v-stack.js"),
  "components/h-stack.d.ts": join(dist, "components/h-stack.d.ts"),
  "components/h-stack.js": join(dist, "components/h-stack.js"),
  "components/scroll-view.d.ts": join(dist, "components/scroll-view.d.ts"),
  "components/scroll-view.js": join(dist, "components/scroll-view.js"),
};
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const [
  layoutModule,
  { VStack },
  { HStack },
  { ScrollView },
  { CURSOR_MARKER },
  { allocateStackSizes },
] =
  await Promise.all([
    import(pathToFileURL(sourceFiles["layout.js"]).href),
    import(pathToFileURL(sourceFiles["components/v-stack.js"]).href),
    import(pathToFileURL(sourceFiles["components/h-stack.js"]).href),
    import(pathToFileURL(sourceFiles["components/scroll-view.js"]).href),
    import(pathToFileURL(sourceFiles["tui.js"]).href),
    import(pathToFileURL(sourceFiles["components/stack.js"]).href),
  ]);
const {
  getScrollbarGeometry,
  getScrollViewBox,
  getScrollViewsAt,
  renderLayoutFrame,
} = layoutModule;

const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: Object.fromEntries(
      Object.entries(sourceFiles).map(([name, path]) => [name, digest(path)]),
    ),
  },
  sizeValue: {
    declaration: "number | `${number}%`",
    percentRule: "floor(reference * percentage / 100)",
  },
  layout: {},
  scroll: {},
};

const probe = (name, rows, calls) => ({
  render(width) {
    calls.push(`${name}:${width}`);
    return rows.map((row) => row.replaceAll("$w", String(width)));
  },
  invalidate() {},
});

const rect = ({ x, y, width, height }) => ({ x, y, width, height });
const boxSnapshot = (box, names = new Map()) => ({
  rect: rect(box.rect),
  clip: rect(box.clip),
  lineOffset: box.lineOffset ?? null,
  scrollView: box.scrollView === undefined ? null : (names.get(box.scrollView) ?? "scroll"),
  layer: box.layer,
  children: box.children.map((child) => boxSnapshot(child, names)),
});
const frameSnapshot = (frame, names = new Map()) => ({
  width: frame.width,
  height: frame.height,
  lines: frame.lines,
  primaryScrollView: frame.primaryScrollView === undefined
    ? null
    : (names.get(frame.primaryScrollView) ?? "scroll"),
  root: boxSnapshot(frame.root, names),
});

{
  const calls = [];
  const top = probe("top", ["top"], calls);
  const cursor = probe("cursor", ["b0", "b1", `b2${CURSOR_MARKER}`, "b3"], calls);
  const stack = new VStack(
    [
      { component: top, basis: 1, shrink: 0 },
      { component: cursor, basis: 4, shrink: 1, minSize: 1 },
    ],
    { gap: 1, align: "start" },
  );
  const frame = renderLayoutFrame(stack, 5, 3, () => calls.push("request"));
  out.layout.vertical5x3 = { frame: frameSnapshot(frame), calls };
}

{
  const calls = [];
  const left = probe("left", ["LL", "L2", "L3"], calls);
  const right = probe("right", ["R"], calls);
  const stack = new HStack(
    [
      { component: left, basis: "auto", grow: 1, maxSize: 4 },
      { component: right, basis: "auto", grow: 2 },
    ],
    { gap: 1, align: "end" },
  );
  const frame = renderLayoutFrame(stack, 8, 3, () => calls.push("request"));
  out.layout.horizontal8x3 = { frame: frameSnapshot(frame), calls };
}

{
  const calls = [];
  const leaf = probe("safe", ["x"], calls);
  const frame = renderLayoutFrame(leaf, 0, 0, () => calls.push("request"));
  out.layout.minimumViewport = { frame: frameSnapshot(frame), calls };
}

{
  const maxSafe = Number.MAX_SAFE_INTEGER;
  out.layout.stackEdgeCases = {
    maxSafeShrink: allocateStackSizes(
      [{ basis: maxSafe, shrink: maxSafe }],
      [0],
      0,
      0,
    ),
    contradictoryGrow: allocateStackSizes(
      [{ basis: 0, grow: 1, minSize: 5, maxSize: 2 }],
      [0],
      10,
      0,
    ),
    contradictoryShrink: allocateStackSizes(
      [{ basis: 10, shrink: 1, minSize: 5, maxSize: 2 }],
      [0],
      0,
      0,
    ),
    contradictoryAuto: allocateStackSizes(
      [{ basis: "auto", grow: 1, shrink: 1, minSize: 7, maxSize: 3 }],
      [2],
      20,
      0,
    ),
  };
}

{
  const calls = [];
  const zones = probe(
    "zones",
    [
      "\x1b]133;A\x07alpha",
      "\x1b]133;B\x1b\\beta",
      "\x1b]133;C\x07\x1b]133;A\x1b\\\x1b]133;B\x07gamma",
      "prefix\x1b]133;A\x07inside",
      "\x1b]133;D\x07unsupported",
      "\x1b]133;Aunterminated",
    ],
    calls,
  );
  const frame = renderLayoutFrame(zones, 24, 6, () => calls.push("request"));
  out.layout.shellZones = { frame: frameSnapshot(frame), calls };
}

{
  const calls = [];
  const child = probe("always-child", ["0000", "1111", "2222", "3333", "4444", "5555"], calls);
  const view = new ScrollView(child, { scrollbar: "always", scrollbarStyle: () => "#" });
  const names = new Map([[view, "always"]]);
  const requests = [];
  const first = renderLayoutFrame(view, 5, 3, () => requests.push("request"));
  const firstGeometry = getScrollbarGeometry(getScrollViewBox(first, view));
  const remainder = view.scrollBy(2);
  const second = renderLayoutFrame(view, 5, 3, () => requests.push("request"));
  const secondGeometry = getScrollbarGeometry(getScrollViewBox(second, view));
  out.scroll.always5x3 = {
    first: frameSnapshot(first, names),
    firstGeometry,
    remainder,
    second: frameSnapshot(second, names),
    secondGeometry,
    state: { scrollTop: view.scrollTop, following: view.isFollowingEnd, viewportHeight: view.viewportHeight },
    calls,
    requests,
  };
}

{
  const calls = [];
  const child = probe(
    "auto-child",
    ["row-0000", "row-1111", "row-2222", "row-3333", "row-4444", "row-5555"],
    calls,
  );
  const view = new ScrollView(child, { scrollbar: "auto", scrollbarStyle: () => "#" });
  const names = new Map([[view, "auto"]]);
  const requests = [];
  const first = renderLayoutFrame(view, 8, 3, () => requests.push("request"));
  const firstVisible = view.isScrollbarVisible;
  const remainder = view.scrollBy(1);
  const second = renderLayoutFrame(view, 8, 3, () => requests.push("request"));
  out.scroll.auto8x3 = {
    first: frameSnapshot(first, names),
    firstVisible,
    remainder,
    second: frameSnapshot(second, names),
    secondVisible: view.isScrollbarVisible,
    geometry: getScrollbarGeometry(getScrollViewBox(second, view)),
    calls,
    requests,
  };
}

{
  const calls = [];
  const rows = ["0", "1", "2", "3", "4", "5"];
  const child = {
    render(width) {
      calls.push(`follow:${width}:${rows.length}`);
      return [...rows];
    },
    invalidate() {},
  };
  const view = new ScrollView(child, { follow: "end" });
  const requests = [];
  const states = [];
  const capture = (name) => states.push({
    name,
    scrollTop: view.scrollTop,
    following: view.isFollowingEnd,
    viewportHeight: view.viewportHeight,
  });
  renderLayoutFrame(view, 5, 3, () => requests.push("request"));
  capture("attached");
  states.push({ name: "detach-remainder", remainder: view.scrollBy(-1) });
  capture("detached");
  rows.push("6", "7");
  renderLayoutFrame(view, 5, 3, () => requests.push("request"));
  capture("growth-detached");
  view.scrollToEnd();
  capture("reattached");
  rows.push("8", "9");
  renderLayoutFrame(view, 5, 3, () => requests.push("request"));
  capture("growth-attached");
  out.scroll.followEnd = { states, calls, requests };
}

{
  const capture = (options) => {
    const child = { render: () => ["0", "1", "2", "3", "4", "5"], invalidate() {} };
    const view = new ScrollView(child, options);
    renderLayoutFrame(view, 5, 3, () => {});
    return {
      overscroll: view.overscroll,
      sequence: [-2, 5, -5].map((delta) => ({
        delta,
        remainder: view.scrollBy(delta),
        scrollTop: view.scrollTop,
      })),
    };
  };
  out.scroll.remainders = [capture(undefined), capture({ overscroll: "contain" })];
}

{
  const calls = [];
  const innerChild = probe("inner-child", ["i0", "i1", "i2", "i3", "i4"], calls);
  const inner = new ScrollView(innerChild, { primary: true });
  const top = probe("nest-top", ["top"], calls);
  const content = new VStack(
    [top, { component: inner, basis: 3 }],
    { gap: 0 },
  );
  const outer = new ScrollView(content);
  const names = new Map([[outer, "outer"], [inner, "inner"]]);
  const frame = renderLayoutFrame(outer, 8, 4, () => calls.push("request"));
  out.scroll.nestedHit = {
    frame: frameSnapshot(frame, names),
    at2x2: getScrollViewsAt(frame, 2, 2).map((view) => names.get(view)),
    at2x0: getScrollViewsAt(frame, 2, 0).map((view) => names.get(view)),
  };
}

{
  const calls = [];
  const natural = new ScrollView(probe("natural", ["n0", "n1"], calls));
  const explicit = new ScrollView(
    probe("explicit", ["e0", "e1"], calls),
    { primary: true },
  );
  const names = new Map([[natural, "natural"], [explicit, "explicit"]]);
  const stack = new VStack(
    [
      { component: natural, basis: 2 },
      { component: explicit, basis: 2 },
    ],
    { gap: 0 },
  );
  const frame = renderLayoutFrame(stack, 8, 4, () => calls.push("request"));
  out.scroll.explicitPrimary = { frame: frameSnapshot(frame, names), calls };
}

{
  const realSetTimeout = globalThis.setTimeout;
  const realClearTimeout = globalThis.clearTimeout;
  let now = 0;
  let nextId = 1;
  const timers = new Map();
  const events = [];
  globalThis.setTimeout = (callback, delay) => {
    const handle = {
      id: nextId++,
      unref() { events.push(`unref:${this.id}`); },
    };
    timers.set(handle, { callback, due: now + delay });
    events.push(`set:${handle.id}:${delay}`);
    return handle;
  };
  globalThis.clearTimeout = (handle) => {
    const existed = timers.delete(handle);
    events.push(`clear:${handle?.id ?? "?"}:${existed}`);
  };
  const advance = (milliseconds) => {
    now += milliseconds;
    const due = [...timers.entries()]
      .filter(([, timer]) => timer.due <= now)
      .sort((left, right) => left[1].due - right[1].due);
    for (const [handle, timer] of due) {
      if (!timers.delete(handle)) continue;
      events.push(`fire:${handle.id}`);
      timer.callback();
    }
  };
  try {
    const requests = [];
    const child = { render: () => ["0", "1", "2", "3", "4"], invalidate() {} };
    const view = new ScrollView(child, { scrollbar: "auto", scrollbarHideDelayMs: 10 });
    renderLayoutFrame(view, 5, 2, () => requests.push("request"));
    view.setScrollbarActive(true);
    const activeVisible = view.isScrollbarVisible;
    view.setScrollbarActive(false);
    const waitingVisible = view.isScrollbarVisible;
    advance(9);
    const beforeDeadlineVisible = view.isScrollbarVisible;
    advance(1);
    const hiddenVisible = view.isScrollbarVisible;
    view.scrollBy(1);
    const scrolledVisible = view.isScrollbarVisible;
    view.setScrollbar("hidden");
    out.scroll.fakeClock = {
      activeVisible,
      waitingVisible,
      beforeDeadlineVisible,
      hiddenVisible,
      scrolledVisible,
      requests,
      events,
      pendingTimers: timers.size,
    };
  } finally {
    globalThis.setTimeout = realSetTimeout;
    globalThis.clearTimeout = realClearTimeout;
  }
}

{
  const component = { render: () => [], invalidate() {} };
  const view = new ScrollView(component);
  const errors = [];
  for (const [name, action] of [
    ["addChild", () => view.addChild(component)],
    ["removeChild", () => view.removeChild(component)],
    ["clear", () => view.clear()],
  ]) {
    try {
      action();
    } catch (error) {
      errors.push({ name, message: error.message });
    }
  }
  try {
    new ScrollView(component, { axis: "horizontal" });
  } catch (error) {
    errors.push({ name: "axis", message: error.message });
  }
  out.scroll.errors = errors;
}

writeFileSync(
  join(root, "crates/pie-components/tests/fixtures/layout-scroll-golden.json"),
  `${JSON.stringify(out, null, 2)}\n`,
);
console.log("wrote crates/pie-components/tests/fixtures/layout-scroll-golden.json");
