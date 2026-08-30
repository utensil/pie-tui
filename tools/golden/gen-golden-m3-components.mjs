#!/usr/bin/env node
// Harvest M3 SelectList, SettingsList, and autocomplete observations from the
// pinned pi-tui build. The fixture records digests, never the local oracle path.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-m3-components.mjs
import { createHash } from "node:crypto";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const sourceFiles = {
  "autocomplete.js": join(dist, "autocomplete.js"),
  "components/input.js": join(dist, "components/input.js"),
  "components/loader.js": join(dist, "components/loader.js"),
  "components/box.js": join(dist, "components/box.js"),
  "components/stack.js": join(dist, "components/stack.js"),
  "components/v-stack.js": join(dist, "components/v-stack.js"),
  "components/h-stack.js": join(dist, "components/h-stack.js"),
  "layout-node.js": join(dist, "layout-node.js"),
  "components/cancellable-loader.js": join(dist, "components/cancellable-loader.js"),
  "components/select-list.js": join(dist, "components/select-list.js"),
  "components/settings-list.js": join(dist, "components/settings-list.js"),
  "tui.js": join(dist, "tui.js"),
};
const [
  { CombinedAutocompleteProvider },
  { CancellableLoader },
  { SelectList },
  { SettingsList },
  { Container },
  keybindings,
  { Box: ReferenceBox },
  stackModule,
  { VStack },
  { HStack },
  { LAYOUT_NODE },
] =
  await Promise.all([
    import(pathToFileURL(sourceFiles["autocomplete.js"]).href),
    import(pathToFileURL(sourceFiles["components/cancellable-loader.js"]).href),
    import(pathToFileURL(sourceFiles["components/select-list.js"]).href),
    import(pathToFileURL(sourceFiles["components/settings-list.js"]).href),
    import(pathToFileURL(sourceFiles["tui.js"]).href),
    import(pathToFileURL(join(dist, "keybindings.js")).href),
    import(pathToFileURL(sourceFiles["components/box.js"]).href),
    import(pathToFileURL(sourceFiles["components/stack.js"]).href),
    import(pathToFileURL(sourceFiles["components/v-stack.js"]).href),
    import(pathToFileURL(sourceFiles["components/h-stack.js"]).href),
    import(pathToFileURL(sourceFiles["layout-node.js"]).href),
  ]);
const { KeybindingsManager, TUI_KEYBINDINGS, setKeybindings } = keybindings;
setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));

const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: Object.fromEntries(
      Object.entries(sourceFiles).map(([name, path]) => [name, digest(path)]),
    ),
  },
  selectList: [],
  settingsList: [],
  container: {},
  containerIdentity: {},
  boxLifecycle: {},
  stack: {},
  cancellableLoader: {},
  autocomplete: {},
};

{
  const events = [];
  const child = (name, rows) => ({
    render: (width) => {
      events.push(`render:${name}:${width}`);
      return rows.map((row) => `${name}:${row}:${width}`);
    },
    invalidate: () => events.push(`invalidate:${name}`),
  });
  const first = child("first", ["a", "b"]);
  const second = child("second", ["c"]);
  const container = new Container();
  container.addChild(first);
  container.addChild(second);
  const outputs = [container.render(17)];
  container.invalidate();
  container.removeChild(first);
  outputs.push(container.render(9));
  container.clear();
  outputs.push(container.render(4));
  out.container = { outputs, events };
}

{
  const events = [];
  const shared = {
    value: "one",
    render(width) {
      events.push(`render:${this.value}:${width}`);
      return [`${this.value}:${width}`];
    },
    invalidate() {},
  };
  const container = new Container();
  const middle = {
    render: (width) => {
      events.push(`render:middle:${width}`);
      return [`middle:${width}`];
    },
    invalidate() {},
  };
  container.addChild(shared);
  container.addChild(middle);
  container.addChild(shared);
  const outputs = [container.render(11)];
  container.removeChild(shared);
  shared.value = "two";
  outputs.push(container.render(7));
  container.removeChild(shared);
  outputs.push(container.render(5));
  container.clear();
  container.addChild(shared);
  outputs.push(container.render(3));
  out.containerIdentity = { outputs, events };
}

{
  const events = [];
  let tone = "red";
  const bg = (text) => {
    events.push(`bg:${tone}:${text}`);
    const code = tone === "red" ? 41 : 44;
    return `\x1b[${code}m${text}\x1b[49m`;
  };
  const child = (name) => ({
    render: (width) => {
      events.push(`render:${name}:${width}`);
      return [name];
    },
    invalidate: () => events.push(`invalidate:${name}`),
  });
  const first = child("one");
  const second = child("two");
  const box = new ReferenceBox(1, 1, bg);
  box.addChild(first);
  box.addChild(second);
  const outputs = [box.render(9), box.render(9)];
  tone = "blue";
  outputs.push(box.render(9));
  box.removeChild(first);
  outputs.push(box.render(7));
  box.invalidate();
  box.clear();
  outputs.push(box.render(7));
  out.boxLifecycle = { outputs, events };
}

{
  const events = [];
  const child = (name, rows) => ({
    render: (width) => {
      events.push(`render:${name}:${width}`);
      return rows.map((row) => `${name}:${row}`);
    },
    invalidate() {},
  });
  const first = child("first", ["a", "b"]);
  const second = child("second", ["x"]);
  const visible = (viewport) => {
    const result = viewport.width >= 8;
    events.push(`visible:${viewport.width}:${result}`);
    return result;
  };
  const stack = new VStack([], { gap: 1, align: "start" });
  stack.addChild(first);
  stack.addChild(second, {
    basis: 3,
    grow: 2,
    shrink: 4,
    minSize: 1,
    maxSize: 5,
    visible,
  });
  const node = stack[LAYOUT_NODE]();
  const entryShapes = node.entries.map((entry) => ({
    keys: Object.keys(entry).filter((key) => key !== "component"),
    basis: entry.basis ?? null,
    grow: entry.grow ?? null,
    shrink: entry.shrink ?? null,
    minSize: entry.minSize ?? null,
    maxSize: entry.maxSize ?? null,
    visibleAt6: entry.visible?.({ width: 6, height: 9 }) ?? true,
    visibleAt10: entry.visible?.({ width: 10, height: 9 }) ?? true,
  }));
  const outputs = [stack.render(10), stack.render(6)];
  stack.removeChild(first);
  outputs.push(stack.render(10));
  stack.clear();
  outputs.push(stack.render(10));

  const alignOutputs = {};
  for (const align of ["stretch", "start", "center", "end"]) {
    const horizontal = new HStack(
      [
        child(`left-${align}`, ["1", "2", "3"]),
        child(`right-${align}`, ["x"]),
      ],
      { gap: 1, align },
    );
    alignOutputs[align] = horizontal.render(25);
  }

  const allocationCases = [
    {
      name: "empty-options-intrinsic",
      result: stackModule.allocateStackSizes([{}, {}], [5, 2], undefined, 0),
    },
    {
      name: "grow-gap-max",
      result: stackModule.allocateStackSizes(
        [{ basis: 2, grow: 1, maxSize: 4 }, { basis: "auto", grow: 2 }],
        [9, 3],
        12,
        1,
      ),
    },
    {
      name: "shrink-min-weighted",
      result: stackModule.allocateStackSizes(
        [{ basis: 8, shrink: 1, minSize: 5 }, { basis: 6, shrink: 3, minSize: 1 }],
        [0, 0],
        9,
        0,
      ),
    },
  ];
  out.stack = {
    node: { type: node.type, gap: node.gap, align: node.align, entryShapes },
    outputs,
    alignOutputs,
    allocationCases,
    events,
  };
}

{
  const events = [];
  const loader = new CancellableLoader(
    { requestRender: () => events.push("request-render") },
    (text) => `spinner:${text}`,
    (text) => `message:${text}`,
    "Working",
    { frames: ["A", "B"], intervalMs: 10000 },
  );
  try {
    loader.onAbort = () => events.push("abort");
    const retainedSignal = loader.signal;
    retainedSignal.addEventListener("abort", () => events.push("signal-abort"));
    const outputs = [loader.render(24)];
    const states = [{
      aborted: loader.aborted,
      sameSignal: retainedSignal === loader.signal,
      frame: loader.currentFrame,
      scheduled: loader.intervalId !== null,
      frames: [...loader.frames],
    }];
    loader.currentFrame = 1;
    loader.updateDisplay();
    outputs.push(loader.render(24));
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null });
    loader.stop();
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null });
    loader.start();
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null });
    loader.setIndicator({ frames: ["!"], intervalMs: 5 });
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null, frames: [...loader.frames] });
    loader.setIndicator({ frames: [], intervalMs: 5 });
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null, frames: [...loader.frames] });
    loader.setIndicator({ intervalMs: 10000 });
    states.push({ frame: loader.currentFrame, scheduled: loader.intervalId !== null, frameCount: loader.frames.length });
    loader.setText("inherited");
    outputs.push(loader.render(12));
    loader.setMessage("Working again");
    outputs.push(loader.render(12));
    loader.handleInput("x");
    states.push({ aborted: loader.aborted });
    loader.handleInput("\x1b");
    states.push({ aborted: loader.aborted });
    loader.handleInput("\x1b");
    states.push({ aborted: loader.aborted });
    outputs.push(loader.render(12));
    out.cancellableLoader = { outputs, states, events };
  } finally {
    loader.dispose();
  }
}

const selectTheme = {
  selectedPrefix: (text) => `\x1b[35m${text}\x1b[39m`,
  selectedText: (text) => `\x1b[7m${text}\x1b[27m`,
  description: (text) => `\x1b[2m${text}\x1b[22m`,
  scrollInfo: (text) => `\x1b[36m${text}\x1b[39m`,
  noMatch: (text) => `\x1b[31m${text}\x1b[39m`,
};
const selectItems = [
  { value: "config", label: "Configure", description: "Change the active configuration" },
  { value: "connect", label: "Connect", description: "Connect\r\nto a remote" },
  { value: "copy", label: "Copy", description: "Copy the selected value" },
  { value: "close", label: "Close", description: "Close this view" },
  { value: "commit", label: "Commit", description: "Commit current changes" },
  { value: "continue", label: "Continue", description: "Continue execution" },
  { value: "value-only", label: "", description: "Falls back to value" },
];

{
  const events = [];
  const list = new SelectList(selectItems, 3, selectTheme);
  list.onSelectionChange = (item) => events.push(`change:${item.value}`);
  list.onSelect = (item) => events.push(`select:${item.value}`);
  list.onCancel = () => events.push("cancel");
  const outputs = [list.render(64)];
  list.handleInput("\x1b[B");
  outputs.push(list.render(64));
  list.setSelectedIndex(5);
  outputs.push(list.render(64));
  list.handleInput("\x1b[B");
  outputs.push(list.render(64));
  list.handleInput("\x1b[A");
  list.handleInput("\r");
  list.handleInput("\x1b");
  const selectedBeforeFilter = list.getSelectedItem()?.value ?? null;
  list.setFilter("co");
  outputs.push(list.render(64));
  const selectedAfterFilter = list.getSelectedItem()?.value ?? null;
  list.setFilter("zzz");
  outputs.push(list.render(18));
  out.selectList.push({
    name: "navigation-filter-callbacks",
    outputs,
    events,
    selectedBeforeFilter,
    selectedAfterFilter,
  });
}

{
  const contexts = [];
  const list = new SelectList(
    [
      { value: "alpha-long-value", label: "Alpha primary column", description: "alpha description" },
      { value: "beta", label: "Beta", description: "beta description" },
    ],
    5,
    selectTheme,
    {
      minPrimaryColumnWidth: 12,
      maxPrimaryColumnWidth: 20,
      truncatePrimary: (context) => {
        contexts.push({
          text: context.text,
          maxWidth: context.maxWidth,
          columnWidth: context.columnWidth,
          value: context.item.value,
          isSelected: context.isSelected,
        });
        return context.text.toUpperCase();
      },
    },
  );
  const outputs = [list.render(58), list.render(22)];
  list.setSelectedIndex(-20);
  outputs.push(list.render(58));
  list.setSelectedIndex(99);
  outputs.push(list.render(58));
  out.selectList.push({ name: "layout-truncation-bounds", outputs, contexts });
}

{
  const list = new SelectList(
    [
      { value: "empty", label: "Empty description", description: "" },
      { value: "newlines", label: "Newline description", description: "\r\n\n" },
      { value: "spaces", label: "Whitespace description", description: " \r\n " },
    ],
    3,
    selectTheme,
  );
  const outputs = [list.render(64)];
  list.setSelectedIndex(1);
  outputs.push(list.render(64));
  list.setSelectedIndex(2);
  outputs.push(list.render(64));
  out.selectList.push({ name: "description-truthiness", outputs });
}

const settingsTheme = {
  label: (text, selected) => selected ? `\x1b[1m${text}\x1b[22m` : text,
  value: (text, selected) => selected ? `\x1b[36m${text}\x1b[39m` : `\x1b[2m${text}\x1b[22m`,
  description: (text) => `\x1b[3m${text}\x1b[23m`,
  cursor: "» ",
  hint: (text) => `\x1b[2m${text}\x1b[22m`,
};
const makeSettings = () => [
  { id: "theme", label: "Theme", currentValue: "dark", values: ["dark", "light"], description: "Color theme used for the interface." },
  { id: "language", label: "Language", currentValue: "English", values: ["English", "中文", "日本語"], description: "Language for generated text and labels." },
  { id: "format", label: "Output format", currentValue: "compact", values: ["compact", "expanded"] },
  { id: "telemetry", label: "Telemetry", currentValue: "off", values: ["off", "on"] },
  { id: "wrap", label: "Line wrapping", currentValue: "auto", values: ["auto", "never"] },
];

const runSearchSteps = (steps) => {
  const list = new SettingsList(
    makeSettings(),
    4,
    settingsTheme,
    () => {},
    () => {},
    { enableSearch: true },
  );
  return steps.map(({ input, width }) => {
    if (input !== null) list.handleInput(input);
    return list.render(width)[0];
  });
};

{
  const events = [];
  const list = new SettingsList(
    makeSettings(),
    3,
    settingsTheme,
    (id, value) => events.push(`change:${id}:${value}`),
    () => events.push("cancel"),
  );
  const outputs = [list.render(52)];
  list.handleInput("\x1b[B");
  outputs.push(list.render(52));
  list.handleInput("\r");
  outputs.push(list.render(52));
  list.updateValue("theme", "solarized");
  list.handleInput("\x1b[A");
  outputs.push(list.render(34));
  list.handleInput(" ");
  outputs.push(list.render(34));
  list.handleInput("\x1b");
  out.settingsList.push({ name: "navigation-cycle-update", outputs, events });
}

{
  const events = [];
  const list = new SettingsList(
    makeSettings(),
    4,
    settingsTheme,
    (id, value) => events.push(`change:${id}:${value}`),
    () => events.push("cancel"),
    { enableSearch: true },
  );
  const outputs = [list.render(46)];
  list.handleInput("l");
  list.handleInput("a");
  outputs.push(list.render(46));
  list.handleInput("\x7f");
  outputs.push(list.render(30));
  list.handleInput(" ");
  outputs.push(list.render(30));
  list.handleInput("\r");
  outputs.push(list.render(46));
  list.handleInput("\x1b");
  out.settingsList.push({ name: "search-input-filter", outputs, events });
}

{
  out.settingsList.push({
    name: "search-editor-key-subset",
    grapheme: runSearchSteps([
      { input: "A👩‍💻e\u0301Z", width: 20 },
      { input: "\x1b[D", width: 20 },
      { input: "\x1b[D", width: 20 },
      { input: "\x04", width: 20 },
      { input: "\x7f", width: 20 },
    ]),
    wordKillYankUndo: runSearchSteps([
      { input: "alpha-beta gamma", width: 24 },
      { input: "\x1bb", width: 24 },
      { input: "\x1bb", width: 24 },
      { input: "\x1bd", width: 24 },
      { input: "\x17", width: 24 },
      { input: "\x19", width: 24 },
      { input: "\x01", width: 24 },
      { input: "\x1bf", width: 24 },
      { input: "\x0b", width: 24 },
      { input: "\x19", width: 24 },
      { input: "\x15", width: 24 },
      { input: "\x19", width: 24 },
      { input: "\x1by", width: 24 },
      { input: "\x1f", width: 24 },
      { input: "\x1f", width: 24 },
    ]),
    pasteViewport: runSearchSteps([
      { input: "Q", width: 12 },
      { input: "\x1b[200~one\r\n", width: 12 },
      { input: "two\t三\n\x1b[201~Z", width: 12 },
      { input: "\x01", width: 12 },
      { input: "\x1bf", width: 12 },
      { input: "\x05", width: 12 },
    ]),
    undoCoalescing: runSearchSteps([
      { input: "a", width: 16 },
      { input: "b", width: 16 },
      { input: " ", width: 16 },
      { input: "c", width: 16 },
      { input: "\x1f", width: 16 },
      { input: "\x1f", width: 16 },
      { input: "\x1f", width: 16 },
    ]),
    wordBoundaries: runSearchSteps([
      { input: "don't 3.14 word🙂!!", width: 32 },
      { input: "\x01", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x1bf", width: 32 },
      { input: "\x05", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
      { input: "\x1bb", width: 32 },
    ]),
  });
}

{
  const noSearch = new SettingsList([], 3, settingsTheme, () => {}, () => {});
  const search = new SettingsList([], 3, settingsTheme, () => {}, () => {}, { enableSearch: true });
  out.settingsList.push({
    name: "empty-lists",
    outputs: [noSearch.render(24), search.render(24)],
    events: [],
  });
}

{
  const events = [];
  const item = {
    id: "backend",
    label: "Backend",
    currentValue: "old",
    submenu: (currentValue, done) => ({
      invalidate: () => events.push("submenu:invalidate"),
      render: (width) => [`submenu:${currentValue}:${width}`],
      handleInput: (data) => {
        events.push(`submenu:input:${JSON.stringify(data)}`);
        if (data === "x") done("new");
      },
    }),
  };
  const list = new SettingsList(
    [item],
    3,
    settingsTheme,
    (id, value) => events.push(`change:${id}:${value}`),
    () => events.push("cancel"),
  );
  const outputs = [list.render(30)];
  list.handleInput("\r");
  outputs.push(list.render(30));
  list.invalidate();
  list.handleInput("x");
  outputs.push(list.render(30));
  out.settingsList.push({ name: "submenu-delegation", outputs, events });
}

{
  const externalEvents = [];
  let completeExternal;
  const externalList = new SettingsList(
    [{
      id: "external",
      label: "External",
      currentValue: "old",
      submenu: (_current, done) => {
        completeExternal = done;
        return {
          invalidate: () => externalEvents.push("submenu:invalidate"),
          render: (width) => [`external-submenu:${width}`],
        };
      },
    }],
    3,
    settingsTheme,
    (id, value) => externalEvents.push(`change:${id}:${value}`),
    () => {},
  );
  const externalOutputs = [externalList.render(28)];
  externalList.handleInput("\r");
  externalOutputs.push(externalList.render(28));
  completeExternal("async");
  externalList.invalidate();
  externalOutputs.push(externalList.render(28));

  const renderEvents = [];
  const renderList = new SettingsList(
    [{
      id: "render",
      label: "Render",
      currentValue: "old",
      submenu: (_current, done) => ({
        invalidate: () => renderEvents.push("submenu:invalidate-after-render"),
        render: (width) => {
          renderEvents.push(`submenu:render:${width}`);
          done("rendered");
          return [`render-submenu:${width}`];
        },
      }),
    }],
    3,
    settingsTheme,
    (id, value) => renderEvents.push(`change:${id}:${value}`),
    () => renderEvents.push("cancel"),
  );
  renderList.handleInput("\r");
  const renderOutputs = [renderList.render(26)];
  renderList.handleInput("\x1b");
  renderList.invalidate();
  renderOutputs.push(renderList.render(26));

  const invalidateEvents = [];
  const invalidateList = new SettingsList(
    [{
      id: "invalidate",
      label: "Invalidate",
      currentValue: "old",
      submenu: (_current, done) => ({
        invalidate: () => {
          invalidateEvents.push("submenu:invalidate");
          done("invalidated");
        },
        render: (width) => [`invalidate-submenu:${width}`],
      }),
    }],
    3,
    settingsTheme,
    (id, value) => invalidateEvents.push(`change:${id}:${value}`),
    () => invalidateEvents.push("cancel"),
  );
  invalidateList.handleInput("\r");
  invalidateList.invalidate();
  invalidateList.handleInput("\x1b");
  const invalidateOutputs = [invalidateList.render(30)];

  out.settingsList.push({
    name: "submenu-completion-drainage",
    external: { outputs: externalOutputs, events: externalEvents },
    render: { outputs: renderOutputs, events: renderEvents },
    invalidate: { outputs: invalidateOutputs, events: invalidateEvents },
  });
}

const localeOrderNames = [
  "Zoo",
  "alpha",
  "Álpha",
  "äther",
  "Beta",
  "file10",
  "file2",
  "_under",
  "a-b",
  "a b",
];

function assertCaseFoldUnique(names) {
  const seen = new Map();
  for (const name of names) {
    const key = name.toLowerCase();
    const previous = seen.get(key);
    if (previous !== undefined) {
      throw new Error(
        `locale-order fixture names must remain case-fold unique: ${JSON.stringify(previous)} conflicts with ${JSON.stringify(name)}`,
      );
    }
    seen.set(key, name);
  }
}

// The reference reads real directory entries before applying localeCompare.
// Reject aliases before creating the temp tree so every host receives the same
// oracle input, including case-insensitive filesystems.
assertCaseFoldUnique(localeOrderNames);

const tempRoot = mkdtempSync(join(tmpdir(), "pie-tui-m3-oracle-"));
try {
  mkdirSync(join(tempRoot, "alpha-dir"));
  mkdirSync(join(tempRoot, "space dir"));
  writeFileSync(join(tempRoot, "alpha.txt"), "alpha");
  writeFileSync(join(tempRoot, "beta.md"), "beta");
  writeFileSync(join(tempRoot, "space file.txt"), "space");
  writeFileSync(join(tempRoot, "🎉.txt"), "astral");
  mkdirSync(join(tempRoot, "locale-order"));
  for (const name of localeOrderNames) {
    writeFileSync(join(tempRoot, "locale-order", name), name);
  }
  const fakeFd = join(tempRoot, "fake-fd.mjs");
  const commonFdArgs = [
    "--base-directory",
    tempRoot,
    "--max-results",
    "100",
    "--type",
    "f",
    "--type",
    "d",
    "--follow",
    "--hidden",
    "--exclude",
    ".git",
    "--exclude",
    ".git/*",
    "--exclude",
    ".git/**",
  ];
  const allowedFdArgs = [
    [...commonFdArgs, "li"],
    [...commonFdArgs, "sp"],
    [...commonFdArgs, "--full-path", String.raw`src[\\/]li`],
    [...commonFdArgs, "--full-path", String.raw`src\.\[x\][\\/]li\+`],
  ];
  writeFileSync(
    fakeFd,
    `#!/usr/bin/env node
const actual = process.argv.slice(2);
const allowed = ${JSON.stringify(allowedFdArgs)};
if (!allowed.some((expected) => JSON.stringify(expected) === JSON.stringify(actual))) {
  process.stderr.write(\`unexpected argv: \${JSON.stringify(actual)}\\n\`);
  process.exit(64);
}
const escapedQuery = ${JSON.stringify(String.raw`src\.\[x\][\\/]li\+`)};
process.stdout.write(actual.at(-1) === escapedQuery
  ? "src.[x]/li+.rs\\n"
  : "src/lib.rs\\nsrc/tools/\\ndocs/readme.md\\nlib-top.txt\\nspace dir/\\nspace file.txt\\n");
`,
  );
  chmodSync(fakeFd, 0o755);

  const commands = [
    { name: "config", description: "Configure the application", argumentHint: "[scope]" },
    { value: "commit", label: "Commit", description: "Commit current changes" },
    {
      name: "model",
      description: "Select model",
      getArgumentCompletions: async (prefix) => [
        { value: "fast", label: "fast", description: "Fast model" },
        { value: "full", label: "full", description: "Full model" },
        { value: "reasoning", label: "reasoning" },
      ].filter((item) => item.value.startsWith(prefix)),
    },
    {
      name: "mode",
      getArgumentCompletions: (prefix) => [
        { value: "safe", label: "safe" },
        { value: "speed", label: "speed" },
      ].filter((item) => item.value.startsWith(prefix)),
    },
  ];
  const provider = new CombinedAutocompleteProvider(commands, tempRoot, fakeFd);
  const signal = new AbortController().signal;
  const suggestions = [];
  for (const scenario of [
    { name: "slash-empty", lines: ["/"], line: 0, col: 1, force: false },
    { name: "slash-fuzzy", lines: ["/cf"], line: 0, col: 3, force: false },
    { name: "slash-arguments", lines: ["/model f"], line: 0, col: 8, force: false },
    { name: "slash-arguments-ready", lines: ["/mode s"], line: 0, col: 7, force: false },
    { name: "local-prefix", lines: ["open ./a"], line: 0, col: 8, force: false },
    { name: "forced-empty-token", lines: ["open "], line: 0, col: 5, force: true },
    { name: "quoted-space", lines: ["open \"sp"], line: 0, col: 8, force: false },
    { name: "at-fuzzy", lines: ["attach @li"], line: 0, col: 10, force: false },
    { name: "at-fuzzy-quoted", lines: ["attach @\"sp"], line: 0, col: 11, force: false },
    { name: "at-fuzzy-multi-segment", lines: ["attach @src/li"], line: 0, col: "attach @src/li".length, force: false },
    { name: "at-fuzzy-escaped-multi", lines: ["attach @src.[x]/li+"], line: 0, col: "attach @src.[x]/li+".length, force: false },
    { name: "astral-cursor-input", lines: ["🎉 attach @li"], line: 0, col: "🎉 attach @li".length, force: false },
    { name: "astral-local-prefix", lines: ["🎉 open ./🎉"], line: 0, col: "🎉 open ./🎉".length, force: false },
    { name: "locale-order", lines: ["open ./locale-order/"], line: 0, col: "open ./locale-order/".length, force: false },
  ]) {
    suggestions.push({
      ...scenario,
      result: await provider.getSuggestions(
        scenario.lines,
        scenario.line,
        scenario.col,
        { signal, force: scenario.force },
      ),
    });
  }
  const aborted = new AbortController();
  aborted.abort();
  suggestions.push({
    name: "aborted-at-fuzzy",
    lines: ["@li"],
    line: 0,
    col: 3,
    force: false,
    result: await provider.getSuggestions(["@li"], 0, 3, { signal: aborted.signal }),
  });

  const completionCases = [
    { name: "slash", lines: ["/cf tail"], line: 0, col: 3, item: { value: "config", label: "config" }, prefix: "/cf" },
    { name: "attachment-file", lines: ["see @al now"], line: 0, col: 7, item: { value: "@alpha.txt", label: "alpha.txt" }, prefix: "@al" },
    { name: "attachment-dir", lines: ["see @al"], line: 0, col: 7, item: { value: "@alpha-dir/", label: "alpha-dir/" }, prefix: "@al" },
    { name: "quoted-existing-close", lines: ["open \"sp\" tail"], line: 0, col: 8, item: { value: "\"space file.txt\"", label: "space file.txt" }, prefix: "\"sp" },
    { name: "argument", lines: ["/model f tail"], line: 0, col: 8, item: { value: "fast", label: "fast" }, prefix: "f" },
    { name: "plain-path", lines: ["open ./a tail"], line: 0, col: 8, item: { value: "./alpha.txt", label: "alpha.txt" }, prefix: "./a" },
    { name: "astral-input-output", lines: ["🎉 see @x tail"], line: 0, col: "🎉 see @x".length, item: { value: "@🎉.txt", label: "🎉.txt" }, prefix: "@x" },
  ].map((scenario) => ({
    ...scenario,
    result: provider.applyCompletion(
      scenario.lines,
      scenario.line,
      scenario.col,
      scenario.item,
      scenario.prefix,
    ),
  }));

  const triggerCases = [
    ["slash-command", ["/conf"], 0, 5],
    ["slash-argument", ["/conf path"], 0, 10],
    ["plain", ["hello"], 0, 5],
  ].map(([name, lines, line, col]) => ({
    name,
    lines,
    line,
    col,
    result: provider.shouldTriggerFileCompletion(lines, line, col),
  }));

  out.autocomplete = {
    triggerCharacters: provider.triggerCharacters ?? null,
    suggestions,
    completionCases,
    triggerCases,
  };
} finally {
  rmSync(tempRoot, { recursive: true, force: true });
}

const fixturePath = join(root, "crates/pie-components/tests/fixtures/m3-components.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log(
  `wrote M3 component fixture: ${out.selectList.length} SelectList, ${out.settingsList.length} SettingsList, ${out.autocomplete.suggestions.length} autocomplete suggestion scenarios`,
);
