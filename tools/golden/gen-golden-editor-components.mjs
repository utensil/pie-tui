#!/usr/bin/env node
// Harvest Editor/Input component behavior from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-editor-components.mjs
//
// The fixture records source digests, never the local oracle path. Timers and
// provider futures are fully controlled so the async trace contains no sleeps.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join, resolve } from "node:path";
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
  editorJs: join(dist, "components", "editor.js"),
  editorDts: join(dist, "components", "editor.d.ts"),
  inputJs: join(dist, "components", "input.js"),
  inputDts: join(dist, "components", "input.d.ts"),
  editorComponentDts: join(dist, "editor-component.d.ts"),
  selectListJs: join(dist, "components", "select-list.js"),
  keybindingsJs: join(dist, "keybindings.js"),
  keysJs: join(dist, "keys.js"),
  killRingJs: join(dist, "kill-ring.js"),
  tuiJs: join(dist, "tui.js"),
  undoStackJs: join(dist, "undo-stack.js"),
  utilsJs: join(dist, "utils.js"),
  wordNavigationJs: join(dist, "word-navigation.js"),
  keybindingsDts: join(dist, "keybindings.d.ts"),
  keysDts: join(dist, "keys.d.ts"),
  utilsDts: join(dist, "utils.d.ts"),
  eastAsianWidthPackageJson: join(eastAsianWidthRoot, "package.json"),
  eastAsianWidthIndexJs: eastAsianWidthEntry,
  eastAsianWidthLookupJs: join(eastAsianWidthRoot, "lookup.js"),
  eastAsianWidthLookupDataJs: join(eastAsianWidthRoot, "lookup-data.js"),
  eastAsianWidthUtilitiesJs: join(eastAsianWidthRoot, "utilities.js"),
};
const expectedDigests = {
  packageJson: "f7f8f42f7cfa8c53c4f00bdc12c14cb035aa62d9fb73555661ce08b68da61290",
  editorJs: "a384c140d84e5352605250fab0e1284add133dbdda1e986419c4a0778ffa0853",
  editorDts: "fc3b400c5c965e4906971df836c1fae0a62c89524cda44d85b3cca86782332be",
  inputJs: "4762edfaa75de102aabc00f8660c591f2d7de1ba6e7e212900dd81c48f231e63",
  inputDts: "f2a2bab62c83d6e4b2b69a243c96df26f81ac98963031a67c92a083c57309871",
  editorComponentDts: "5de081a8879f096e0bb0f7efb1f4751bab259b1a77663be041938758d22e50c6",
  selectListJs: "ea14ebd2f64ed045563360b598eeccc816f7f9f252df6b7bc492309cfe49c545",
  keybindingsJs: "d27090a36394fc4f59350e7f3234c601082d950e179ba6742d9557aae2a72168",
  keysJs: "14b18205fd5e56ed3b183392c82bd72e41ba3dab1d345e47b2b17af6988493cc",
  killRingJs: "52212d532f2c5b85ed8977b0f4431f43998c6dc7746d26efc81eb7975b119122",
  tuiJs: "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a",
  undoStackJs: "7fbb318db3521aa1fa6804ffe50245c18d9e9f210a85a48e175fae6a629259cb",
  utilsJs: "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
  wordNavigationJs: "72618be2d05d6c20d9987d0d74de487335056fa0a00a145687f6106a6ae6b9d0",
  keybindingsDts: "93450b5ff2259c52767d4bc3dffb17d7c9341f866507cf00aba67cddf42b51b0",
  keysDts: "58d05b6227c8657e2109931eb2875de3a675e7bccc7f5eafde5467d539636344",
  utilsDts: "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
  eastAsianWidthPackageJson: "d263e50dd1a43aee9acda4d7f066e66b0d0bde1f2852ea6e7153750a5e3a3e52",
  eastAsianWidthIndexJs: "d7b1ba05914c0fc311c20e5618bf8d0893c9c74078a07975e2df981445e64887",
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
const eastAsianWidthPackage = JSON.parse(readFileSync(paths.eastAsianWidthPackageJson, "utf8"));
const referencePackage = JSON.parse(readFileSync(paths.packageJson, "utf8"));
if (referencePackage.name !== "@earendil-works/pi-tui" || referencePackage.version !== "0.84.1") {
  throw new Error(`unexpected reference package: ${referencePackage.name}@${referencePackage.version}`);
}
if (eastAsianWidthPackage.name !== "get-east-asian-width" || eastAsianWidthPackage.version !== "1.6.0") {
  throw new Error(`unexpected resolved get-east-asian-width package: ${eastAsianWidthPackage.name}@${eastAsianWidthPackage.version}`);
}
if (basename(eastAsianWidthEntry) !== "index.js") {
  throw new Error(`unexpected get-east-asian-width entry: ${basename(eastAsianWidthEntry)}`);
}

const [{ Editor }, { Input }, keybindings] = await Promise.all([
  import(pathToFileURL(paths.editorJs).href),
  import(pathToFileURL(paths.inputJs).href),
  import(pathToFileURL(join(dist, "keybindings.js")).href),
]);
const { KeybindingsManager, TUI_KEYBINDINGS, setKeybindings } = keybindings;

const plain = (text) => text;
const theme = {
  borderColor: plain,
  selectList: {
    selectedPrefix: plain,
    selectedText: plain,
    description: plain,
    scrollInfo: plain,
    noMatch: plain,
  },
};

function fakeTui(rows = 24) {
  const events = [];
  return {
    terminal: { rows },
    events,
    requestRender(force = false) {
      events.push({ type: "render", force });
    },
  };
}

const snapshot = (editor) => ({
  text: editor.getText(),
  expandedText: editor.getExpandedText(),
  lines: editor.getLines(),
  cursor: editor.getCursor(),
  paddingX: editor.getPaddingX(),
  autocompleteMaxVisible: editor.getAutocompleteMaxVisible(),
  focused: editor.focused,
  disableSubmit: editor.disableSubmit,
  showingAutocomplete: editor.isShowingAutocomplete(),
});

setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));

const editorCases = [];
{
  const tui = fakeTui();
  const editor = new Editor(tui, theme);
  editorCases.push({ name: "defaults-empty", state: snapshot(editor), render: editor.render(8) });
  editor.focused = true;
  editorCases.push({ name: "focused-empty", state: snapshot(editor), render: editor.render(8) });
}
{
  const tui = fakeTui();
  const editor = new Editor(tui, theme, { paddingX: 2, autocompleteMaxVisible: 99 });
  editor.focused = true;
  editor.setText("A\r\nB\t👩🏽‍💻é");
  editorCases.push({ name: "options-normalize-unicode", state: snapshot(editor), render: editor.render(14) });
  editor.insertTextAtCursor("\rX\tY");
  editorCases.push({ name: "insert-normalize-unicode", state: snapshot(editor), render: editor.render(14) });
}
{
  const tui = fakeTui();
  const editor = new Editor(tui, theme);
  const effects = [];
  editor.onChange = (text) => effects.push(["change", text]);
  editor.onSubmit = (text) => effects.push(["submit", text]);
  editor.setText("  old  ");
  effects.length = 0;
  editor.handleInput("\r");
  editorCases.push({ name: "submit-effect-order", effects, state: snapshot(editor) });
}
{
  const tui = fakeTui();
  const editor = new Editor(tui, theme);
  editor.handleInput("👩🏽‍💻é");
  const before = snapshot(editor);
  editor.handleInput("\x1b[D");
  const afterLeft = snapshot(editor);
  editor.handleInput("\x7f");
  editorCases.push({ name: "grapheme-atomic-input", before, afterLeft, afterBackspace: snapshot(editor) });
}
{
  const editor = new Editor(fakeTui(), theme);
  editor.setText("aeXé");
  editor.handleInput("\x01");
  editor.handleInput("\x1d");
  editor.handleInput("é");
  editorCases.push({ name: "multi-codepoint-jump-target", state: snapshot(editor) });
}
{
  const editor = new Editor(fakeTui(), theme);
  const effects = [];
  editor.onChange = (text) => effects.push(["change", text]);
  editor.onSubmit = (text) => effects.push(["submit", text]);
  editor.setText("\\");
  effects.length = 0;
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS, {
    "tui.input.newLine": "enter",
    "tui.input.submit": "shift+enter",
  }));
  editor.handleInput("\r");
  editorCases.push({ name: "live-newline-submit-backslash-enter", effects, state: snapshot(editor) });
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));
}
for (const lineCount of [10, 11]) {
  const tui = fakeTui();
  const editor = new Editor(tui, theme);
  const text = Array.from({ length: lineCount }, (_, index) => `line-${index + 1}`).join("\n");
  editor.handleInput(`\x1b[200~${text}\x1b[201~`);
  const afterPaste = snapshot(editor);
  editor.handleInput("\x1f");
  editorCases.push({ name: `paste-${lineCount}-lines`, afterPaste, afterUndo: snapshot(editor) });
}
{
  const paste = Array.from({ length: 11 }, (_, index) => `line-${index + 1}`).join("\n");
  const owned = new Editor(fakeTui(), theme);
  owned.focused = true;
  owned.handleInput(`\x1b[200~${paste}\x1b[201~`);
  const wrapped = owned.render(6);
  owned.handleInput("\x01");
  const ownedAtStart = owned.render(24);

  const literal = new Editor(fakeTui(), theme);
  literal.focused = true;
  literal.setText("[paste #1 +11 lines]");
  literal.handleInput("\x01");
  editorCases.push({
    name: "owned-paste-marker-render",
    wrapped,
    ownedAtStart,
    literalAtStart: literal.render(24),
  });
}
{
  const tui = fakeTui(10);
  const editor = new Editor(tui, theme);
  editor.focused = true;
  editor.setText(Array.from({ length: 9 }, (_, index) => `row-${index + 1}`).join("\n"));
  editorCases.push({ name: "long-document-scroll-top-marker", state: snapshot(editor), render: editor.render(12) });
  editorCases.push({ name: "resize-rewrap", renderWide: editor.render(20), renderNarrow: editor.render(8), state: snapshot(editor) });
}

const inputCases = [];
{
  const input = new Input();
  inputCases.push({ name: "defaults", value: input.getValue(), focused: input.focused, render: input.render(8) });
  input.setValue("abcdef");
  input.focused = true;
  inputCases.push({ name: "set-value-cursor-zero", value: input.getValue(), render: input.render(8) });
}
{
  const input = new Input();
  const events = [];
  input.onSubmit = (value) => events.push(["submit", value]);
  input.onEscape = () => events.push(["escape"]);
  input.setValue("abcdef");
  input.handleInput("\r");
  input.handleInput("\x1b");
  inputCases.push({ name: "submit-no-clear-escape", events, value: input.getValue() });
}
{
  const input = new Input();
  input.handleInput("A");
  input.handleInput("\x1b[200~b\r\nc\rd\ne\tf\x1b[201~");
  inputCases.push({ name: "paste-flatten-tabs", value: input.getValue(), render: input.render(20) });
}
{
  const input = new Input();
  input.focused = true;
  input.handleInput("a👩🏽‍💻éz");
  input.handleInput("\x1b[D");
  input.handleInput("\x1b[D");
  inputCases.push({ name: "unicode-cursor-render", value: input.getValue(), render: input.render(10) });
  inputCases.push({ name: "horizontal-viewport", value: input.getValue(), render: input.render(6) });
}
{
  const input = new Input();
  const events = [];
  input.onSubmit = (value) => events.push(value);
  input.setValue("held");
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS, { "tui.input.submit": "x" }));
  input.handleInput("\r");
  const afterOld = input.getValue();
  input.handleInput("x");
  inputCases.push({ name: "live-global-keybindings", afterOld, afterNew: input.getValue(), events });
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));
}

// A deterministic clock replacing the one timer used by Editor autocomplete.
let now = 0;
let nextTimer = 1;
const timers = new Map();
const realSetTimeout = globalThis.setTimeout;
const realClearTimeout = globalThis.clearTimeout;
globalThis.setTimeout = (callback, delay = 0) => {
  const id = nextTimer++;
  timers.set(id, { at: now + Number(delay), callback });
  return id;
};
globalThis.clearTimeout = (id) => timers.delete(id);
const advance = async (milliseconds) => {
  const target = now + milliseconds;
  for (;;) {
    const due = [...timers.entries()]
      .filter(([, timer]) => timer.at <= target)
      .sort((left, right) => left[1].at - right[1].at || left[0] - right[0])[0];
    if (!due) break;
    timers.delete(due[0]);
    now = due[1].at;
    due[1].callback();
    await flush();
  }
  now = target;
  await flush();
};
const flush = async () => {
  for (let index = 0; index < 8; index++) await Promise.resolve();
};
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
const item = (value) => ({ value, label: value });

const autocompleteCases = [];
try {
  {
    const tui = fakeTui();
    const editor = new Editor(tui, theme);
    const calls = [];
    const pending = [];
    editor.setAutocompleteProvider({
      triggerCharacters: ["#"],
      getSuggestions(lines, line, col, options) {
        const gate = deferred();
        const call = { id: calls.length + 1, text: lines.join("\n"), line, col, force: options.force ?? false, aborted: false };
        options.signal.addEventListener("abort", () => { call.aborted = true; });
        calls.push(call);
        pending.push(gate);
        return gate.promise;
      },
      applyCompletion(lines, line, col, selected, prefix) {
        const text = lines[line];
        const start = col - prefix.length;
        const next = [...lines];
        next[line] = text.slice(0, start) + selected.value + text.slice(col);
        return { lines: next, cursorLine: line, cursorCol: start + selected.value.length };
      },
      shouldTriggerFileCompletion() { return true; },
    });
    editor.handleInput("/");
    await flush();
    editor.handleInput("a");
    await flush();
    const beforeASettles = structuredClone(calls);
    pending[0].resolve({ items: [item("/alpha")], prefix: "/" });
    await flush();
    const afterASettles = structuredClone(calls);
    pending[1].resolve({ items: [item("/about"), item("/alpha")], prefix: "/a" });
    await flush();
    autocompleteCases.push({
      name: "serialized-supersession-stale-cannot-win",
      beforeASettles,
      afterASettles,
      finalCalls: calls,
      state: snapshot(editor),
      render: editor.render(24),
    });
  }
  {
    const tui = fakeTui();
    const editor = new Editor(tui, theme);
    const calls = [];
    const pending = [];
    editor.setAutocompleteProvider({
      triggerCharacters: ["#"],
      getSuggestions(lines, line, col, options) {
        const gate = deferred();
        calls.push({ text: lines.join("\n"), line, col, force: options.force ?? false });
        pending.push(gate);
        return gate.promise;
      },
      applyCompletion() { throw new Error("not applied"); },
    });
    editor.handleInput("#");
    await advance(19);
    const beforeBoundary = calls.length;
    editor.handleInput("a");
    await advance(19);
    const afterReset19 = calls.length;
    await advance(1);
    const atReset20 = calls.length;
    autocompleteCases.push({ name: "custom-trigger-debounce-reset", beforeBoundary, afterReset19, atReset20, calls });
    pending[0].resolve(null);
    await flush();
  }
  {
    const tui = fakeTui();
    const editor = new Editor(tui, theme);
    const gate = deferred();
    let aborted = false;
    editor.setAutocompleteProvider({
      getSuggestions(_lines, _line, _col, options) {
        options.signal.addEventListener("abort", () => { aborted = true; });
        return gate.promise;
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    });
    editor.handleInput("/");
    await flush();
    editor.handleInput("\x1b");
    const afterEscape = { aborted, showing: editor.isShowingAutocomplete() };
    gate.resolve({ items: [item("/help")], prefix: "/" });
    await flush();
    autocompleteCases.push({ name: "escape-before-visible-does-not-abort", afterEscape, afterResolve: { aborted, showing: editor.isShowingAutocomplete() } });
  }
  {
    const tui = fakeTui();
    const editor = new Editor(tui, theme);
    const events = [];
    editor.onChange = (text) => events.push(["change", text]);
    editor.setAutocompleteProvider({
      getSuggestions(lines, line, col, options) {
        events.push(["request", options.force ?? false, lines.join("\n"), line, col]);
        return Promise.resolve({ items: [item("file.txt")], prefix: "" });
      },
      applyCompletion(lines, line) {
        events.push(["apply", "file.txt"]);
        const next = [...lines];
        next[line] = "file.txt";
        return { lines: next, cursorLine: line, cursorCol: 8 };
      },
      shouldTriggerFileCompletion(lines, line, col) {
        events.push(["predicate", lines.join("\n"), line, col]);
        return true;
      },
    });
    editor.handleInput("\t");
    await flush();
    autocompleteCases.push({ name: "forced-tab-single-auto-apply", events, state: snapshot(editor), renders: tui.events });
  }
  {
    const editor = new Editor(fakeTui(), theme);
    const calls = [];
    const pending = [];
    editor.setAutocompleteProvider({
      getSuggestions(lines, line, col, options) {
        const gate = deferred();
        const call = { text: lines.join("\n"), line, col, force: options.force ?? false, aborted: false };
        options.signal.addEventListener("abort", () => { call.aborted = true; });
        calls.push(call);
        pending.push(gate);
        return gate.promise;
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    });
    editor.handleInput("/");
    await flush();
    const afterSlash = structuredClone(calls);
    editor.handleInput("a");
    await flush();
    const afterA = structuredClone(calls);
    editor.handleInput("\x7f");
    await flush();
    const afterBackspace = { calls: structuredClone(calls), state: snapshot(editor) };
    pending[0].resolve(null);
    await flush();
    const afterFirstSettles = structuredClone(calls);
    pending[1].resolve({ items: [item("/help"), item("/history")], prefix: "/" });
    await flush();
    autocompleteCases.push({
      name: "pending-slash-backspace-retrigger",
      afterSlash,
      afterA,
      afterBackspace,
      afterFirstSettles,
      finalCalls: calls,
      state: snapshot(editor),
    });
  }
  {
    setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS, {
      "tui.editor.historyPrevious": "ctrl+p",
      "tui.editor.historyNext": "ctrl+n",
    }));
    const calls = [];
    const pending = [];
    const provider = {
      getSuggestions(lines, line, col, options) {
        const gate = deferred();
        const call = { text: lines.join("\n"), line, col, force: options.force ?? false, aborted: false };
        options.signal.addEventListener("abort", () => { call.aborted = true; });
        calls.push(call);
        pending.push(gate);
        return gate.promise;
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    };

    const previous = new Editor(fakeTui(), theme);
    previous.addToHistory("older");
    previous.setAutocompleteProvider(provider);
    previous.handleInput("/");
    await flush();
    previous.handleInput("\x10");
    const afterPrevious = { call: structuredClone(calls[0]), state: snapshot(previous) };
    pending[0].resolve(null);
    await flush();

    const next = new Editor(fakeTui(), theme);
    next.addToHistory("older");
    next.setText("draft");
    next.handleInput("\x10");
    next.setAutocompleteProvider(provider);
    next.handleInput("\t");
    await flush();
    next.handleInput("\x0e");
    const afterNext = { call: structuredClone(calls[1]), state: snapshot(next) };
    pending[1].resolve(null);
    await flush();
    autocompleteCases.push({ name: "history-actions-abort-active", afterPrevious, afterNext });
    setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));
  }
  {
    const calls = [];
    const provider = {
      getSuggestions(lines, line, col, options) {
        calls.push({ text: lines.join("\n"), line, col, force: options.force ?? false });
        return Promise.resolve({ items: [item("/alpha"), item("/about")], prefix: lines[line].slice(0, col) });
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    };
    const editor = new Editor(fakeTui(), theme);
    editor.setAutocompleteProvider(provider);
    editor.handleInput("/");
    await flush();
    const beforeLeft = calls.length;
    editor.handleInput("\x1b[D");
    await flush();
    autocompleteCases.push({
      name: "open-menu-horizontal-requery",
      beforeLeft,
      calls,
      state: snapshot(editor),
    });
  }
  {
    const calls = [];
    const provider = {
      getSuggestions(lines, line, col, options) {
        calls.push({ text: lines.join("\n"), line, col, force: options.force ?? false });
        return Promise.resolve({ items: [item("/alpha"), item("/about")], prefix: lines[line].slice(0, col) });
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    };
    const editor = new Editor(fakeTui(), theme);
    editor.setAutocompleteProvider(provider);
    for (const character of "/abc") {
      editor.handleInput(character);
      await flush();
    }
    const actions = [];
    for (const [name, data] of [
      ["line-start", "\x01"],
      ["word-right", "\x1bf"],
      ["word-left", "\x1bb"],
      ["line-end", "\x05"],
      ["kill-line-start", "\x15"],
      ["yank", "\x19"],
      ["undo", "\x1f"],
    ]) {
      const before = calls.length;
      editor.handleInput(data);
      await flush();
      actions.push({ name, requestDelta: calls.length - before, state: snapshot(editor) });
    }
    autocompleteCases.push({ name: "non-refresh-editor-actions", baselineCalls: 4, calls, actions });
  }
  {
    const calls = [];
    const provider = {
      getSuggestions(lines, line, col, options) {
        calls.push({ text: lines.join("\n"), line, col, force: options.force ?? false });
        return Promise.resolve(null);
      },
      applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
    };
    const editor = new Editor(fakeTui(), theme);
    editor.setAutocompleteProvider(provider);
    editor.handleInput("/");
    await flush();
    editor.handleInput("a");
    await flush();
    const beforeBackspace = calls.length;
    editor.handleInput("\x7f");
    await flush();
    const afterBackspace = { requestDelta: calls.length - beforeBackspace, calls: structuredClone(calls), state: snapshot(editor) };

    editor.setText("");
    for (const character of "/ab") {
      editor.handleInput(character);
      await flush();
    }
    editor.handleInput("\x1b[D");
    const beforeDelete = calls.length;
    editor.handleInput("\x1b[3~");
    await flush();
    const afterDelete = { requestDelta: calls.length - beforeDelete, calls, state: snapshot(editor) };
    autocompleteCases.push({ name: "backspace-forward-delete-retrigger", afterBackspace, afterDelete });
  }
  {
    const whitespaceCases = [];
    for (const [name, whitespace] of [["nbsp", "\u00a0"], ["feff", "\ufeff"]]) {
      const calls = [];
      const editor = new Editor(fakeTui(), theme);
      editor.setAutocompleteProvider({
        triggerCharacters: ["#"],
        getSuggestions(lines, line, col, options) {
          calls.push({ text: lines.join("\n"), line, col, force: options.force ?? false });
          return Promise.resolve(null);
        },
        applyCompletion(lines, line, col) { return { lines, cursorLine: line, cursorCol: col }; },
      });
      editor.handleInput(whitespace);
      editor.handleInput("#");
      await flush();
      const afterTrigger = { calls: structuredClone(calls), pendingTimers: timers.size };
      editor.handleInput("a");
      await flush();
      whitespaceCases.push({ name, afterTrigger, afterContinuation: { calls, pendingTimers: timers.size }, state: snapshot(editor) });
    }
    autocompleteCases.push({ name: "continued-trigger-js-whitespace", cases: whitespaceCases });
  }
  {
    const events = [];
    const editor = new Editor(fakeTui(), theme);
    editor.onChange = (text) => events.push(["change", text]);
    editor.onSubmit = (text) => events.push(["submit", text]);
    editor.setAutocompleteProvider({
      getSuggestions() { return Promise.resolve({ items: [item("/help"), item("/history")], prefix: "/" }); },
      applyCompletion(lines, line, col, selected, prefix) {
        const next = [...lines];
        next[line] = next[line].slice(0, col - prefix.length) + selected.value + next[line].slice(col);
        return { lines: next, cursorLine: line, cursorCol: col - prefix.length + selected.value.length };
      },
    });
    editor.handleInput("/");
    await flush();
    events.length = 0;
    editor.handleInput("\r");
    await flush();
    autocompleteCases.push({ name: "slash-confirm-silent-apply", events, state: snapshot(editor) });
  }
  {
    const events = [];
    const editor = new Editor(fakeTui(), theme);
    editor.onChange = (text) => events.push(["change", text]);
    editor.setAutocompleteProvider({
      getSuggestions() { return Promise.resolve({ items: [item("#alice"), item("#alex")], prefix: "#" }); },
      applyCompletion(lines, line, col, selected, prefix) {
        const next = [...lines];
        next[line] = next[line].slice(0, col - prefix.length) + selected.value + next[line].slice(col);
        return { lines: next, cursorLine: line, cursorCol: col - prefix.length + selected.value.length };
      },
    });
    editor.handleInput("#");
    await advance(20);
    events.length = 0;
    editor.handleInput("\r");
    await flush();
    autocompleteCases.push({ name: "ordinary-confirm-emits-change", events, state: snapshot(editor) });
  }
} finally {
  globalThis.setTimeout = realSetTimeout;
  globalThis.clearTimeout = realClearTimeout;
  setKeybindings(new KeybindingsManager(TUI_KEYBINDINGS));
}

const fixture = {
  generator: "tools/golden/gen-golden-editor-components.mjs",
  reference: referencePackage.version,
  referencePackage: { name: referencePackage.name, version: referencePackage.version },
  sourceDigests,
  dependencies: {
    getEastAsianWidth: { name: eastAsianWidthPackage.name, version: eastAsianWidthPackage.version, entry: basename(eastAsianWidthEntry) },
  },
  runtime: { node: process.versions.node, icu: process.versions.icu, unicode: process.versions.unicode },
  editorCases,
  inputCases,
  autocompleteCases,
};
const text = JSON.stringify(fixture, null, 2) + "\n";
const output = join(root, "crates", "pie-components", "tests", "fixtures", "editor-components.json");
if (process.argv.includes("--check")) {
  if (readFileSync(output, "utf8") !== text) {
    console.error("editor-components fixture is stale");
    process.exit(1);
  }
} else {
  writeFileSync(output, text);
  console.log(`harvested ${editorCases.length} editor, ${inputCases.length} input, and ${autocompleteCases.length} autocomplete cases`);
}
