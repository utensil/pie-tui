#!/usr/bin/env node
// Harvest pure editor-state behavior from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-editor-state.mjs
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
const killRingPath = join(dist, "kill-ring.js");
const undoStackPath = join(dist, "undo-stack.js");
const wordNavigationPath = join(dist, "word-navigation.js");
const utilsPath = join(dist, "utils.js");
const editorPath = join(dist, "components", "editor.js");
const [{ KillRing }, { UndoStack }, wordNavigation, { Editor }] = await Promise.all([
  import(pathToFileURL(killRingPath).href),
  import(pathToFileURL(undoStackPath).href),
  import(pathToFileURL(wordNavigationPath).href),
  import(pathToFileURL(editorPath).href),
]);
const { findWordBackward, findWordForward } = wordNavigation;
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const expectedRuntime = { node: "24.4.1", icu: "77.1", unicode: "16.0" };
for (const [name, version] of Object.entries(expectedRuntime)) {
  if (process.versions[name] !== version) {
    throw new Error(
      `editor-state oracle requires ${name} ${version}, got ${process.versions[name] ?? "missing"}`,
    );
  }
}
const expectedUtilsDigest = "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052";
if (digest(utilsPath) !== expectedUtilsDigest) {
  throw new Error(`editor-state oracle requires pinned utils.js ${expectedUtilsDigest}`);
}

function killRingTrace(label, actions) {
  const ring = new KillRing();
  const trace = [];
  for (const action of actions) {
    if (action.type === "push") {
      ring.push(action.text, {
        prepend: action.prepend,
        accumulate: action.accumulate,
      });
    } else if (action.type === "rotate") {
      ring.rotate();
    }
    trace.push({ length: ring.length, peek: ring.peek() ?? null });
  }
  return { label, actions, trace };
}

const undo = new UndoStack();
const first = { lines: ["alpha"], cursor: { line: 0, col: 5 } };
undo.push(first);
first.lines[0] = "mutated-after-push";
first.cursor.col = 0;
const second = { lines: ["beta", "gamma"], cursor: { line: 1, col: 5 } };
undo.push(second);
second.lines.push("mutated-after-push");
const undoTrace = [
  { length: undo.length },
  { length: undo.length - 1, popped: undo.pop() ?? null },
  { length: undo.length - 1, popped: undo.pop() ?? null },
  { length: undo.length, popped: undo.pop() ?? null },
];
undo.push({ lines: ["clear-me"], cursor: { line: 0, col: 8 } });
undo.clear();
undoTrace.push({ length: undo.length, popped: undo.pop() ?? null });

const wordCases = [
  ["empty-backward", "", 0, "backward"],
  ["empty-forward", "", 0, "forward"],
  ["ascii-backward", "hello world", 11, "backward"],
  ["ascii-forward", "hello world", 0, "forward"],
  ["skip-space-backward", "hello   world", 8, "backward"],
  ["skip-space-forward", "hello   world", 5, "forward"],
  ["punctuation-backward", "foo.bar+baz", 11, "backward"],
  ["punctuation-forward", "foo.bar+baz", 0, "forward"],
  ["punctuation-run", "foo...bar", 3, "forward"],
  ["apostrophe-backward", "can't stop", 5, "backward"],
  ["apostrophe-forward", "can't stop", 0, "forward"],
  ["bom-whitespace-backward", "alpha\ufeffbeta", 6, "backward"],
  ["bom-whitespace-forward", "alpha\ufeffbeta", 5, "forward"],
  ["emoji-backward-utf16", "go 🎉 now", 5, "backward"],
  ["emoji-forward-utf16", "go 🎉 now", 3, "forward"],
  ["zwj-backward-utf16", "x👩🏽‍💻y", 8, "backward"],
  ["zwj-forward-utf16", "x👩🏽‍💻y", 1, "forward"],
  ["combining-backward", "élan fini", 5, "backward"],
  ["combining-forward", "élan fini", 0, "forward"],
  ["cjk-backward", "完成作业", 4, "backward"],
  ["cjk-forward", "完成作业", 0, "forward"],
  ["latin-cjk-thai-forward", "A中กB", 0, "forward"],
  ["latin-cjk-thai-backward", "A中กB", 4, "backward"],
  ["cjk-thai-dictionary-forward", "中ภาษาไทยB", 0, "forward"],
  ["cjk-thai-dictionary-backward", "中ภาษาไทยB", 9, "backward"],
  ["thai-forward", "ภาษาไทย ทดสอบ", 0, "forward"],
  ["thai-backward", "ภาษาไทย ทดสอบ", 7, "backward"],
  ["thai-pair-forward", "กข", 0, "forward"],
  ["thai-pair-backward", "กข", 2, "backward"],
  ["latin-thai-pair-forward", "AกขB", 0, "forward"],
  ["latin-thai-pair-backward", "AกขB", 4, "backward"],
  ["latin-thai-bridge-forward", "AกB", 0, "forward"],
  ["latin-thai-bridge-backward", "AกB", 3, "backward"],
  ["latin-thai-dictionary-forward", "AภาษาไทยB", 0, "forward"],
  ["latin-thai-dictionary-backward", "AภาษาไทยB", 9, "backward"],
  ["cjk-thai-boundary-forward", "中กB", 0, "forward"],
  ["cjk-thai-boundary-backward", "中กB", 3, "backward"],
  ["cjk-thai-pair-boundary-forward", "中กขB", 0, "forward"],
  ["cjk-thai-pair-boundary-backward", "中กขB", 4, "backward"],
  ["thai-punctuation-forward", "ก.ข", 0, "forward"],
  ["thai-punctuation-backward", "ก.ข", 3, "backward"],
  ["thai-space-forward", "ก ข", 0, "forward"],
  ["thai-space-backward", "ก ข", 3, "backward"],
  ["latin-cjk-forward", "A中B", 0, "forward"],
  ["latin-cjk-backward", "A中B", 3, "backward"],
  ["combining-bridge-forward", "A\u0301B", 0, "forward"],
  ["combining-bridge-backward", "A\u0301B", 3, "backward"],
  ["prepend-bridge-forward", "\u0600AB", 0, "forward"],
  ["prepend-bridge-backward", "\u0600AB", 3, "backward"],
  ["punctuation-adjacency-forward", "A.B", 0, "forward"],
  ["punctuation-adjacency-backward", "A.B", 3, "backward"],
  ["unicode-11a3a-forward", "A\u{11A3A}B", 0, "forward"],
  ["unicode-11a3a-backward", "A\u{11A3A}B", 4, "backward"],
  ["unicode-1acf-forward", "A\u{1ACF}B", 0, "forward"],
  ["unicode-1acf-backward", "A\u{1ACF}B", 3, "backward"],
];

const wordNavigationCases = wordCases.map(([label, text, cursor, direction]) => ({
  label,
  text,
  cursor,
  direction,
  result:
    direction === "backward"
      ? findWordBackward(text, cursor)
      : findWordForward(text, cursor),
}));

const wordSegmentationTexts = [
  ["thai-product-dictionary", "พัชรี"],
  ["thai-red-panda-whole", "แพนด้าแดง"],
  ["cjk-dictionary", "完成作业"],
  ["thai-pair", "กข"],
  ["latin-thai-pair", "AกขB"],
  ["latin-thai-bridge", "AกB"],
  ["cjk-thai-boundary", "中กB"],
  ["cjk-thai-pair-boundary", "中กขB"],
  ["latin-cjk-thai-boundary", "A中กB"],
  ["cjk-thai-dictionary-boundary", "中ภาษาไทยB"],
  ["latin-thai-dictionary", "AภาษาไทยB"],
  ["thai-phrase", "ภาษาไทย ทดสอบ"],
  ["thai-punctuation", "ก.ข"],
  ["thai-space", "ก ข"],
  ["latin-cjk", "A中B"],
  ["combining-bridge", "A\u0301B"],
  ["prepend-bridge", "\u0600AB"],
  ["punctuation-adjacency", "A.B"],
  ["unicode-11a3a", "A\u{11A3A}B"],
  ["unicode-1acf", "A\u{1ACF}B"],
];
const wordOracleSegmenter = new Intl.Segmenter(undefined, { granularity: "word" });
const wordSegmentationCases = wordSegmentationTexts.map(([label, text]) => ({
  label,
  text,
  segments: [...wordOracleSegmenter.segment(text)].map(({ segment, index, isWordLike }) => ({
    segment,
    index,
    isWordLike,
  })),
}));

// Exhaustive mixed-script receipt. The 14 atoms cover Latin, Thai,
// CJK, combining, prepend, ASCII punctuation, whitespace, underscore, and the
// Unicode 16 boundary sentinels already used by the focused vectors. Every
// Cartesian string of length 1 through 4 is evaluated at every scalar-valid
// UTF-16 boundary: 14 + 14^2 + 14^3 + 14^4 = 41,370 strings.
const wordProductAtoms = [
  ["latin-a", "A"],
  ["latin-b", "B"],
  ["ascii-digit", "1"],
  ["thai-product", "พัชรี"],
  ["cjk-zhong", "中"],
  ["punctuation-question", "?"],
  ["combining-acute", "\u0301"],
  ["arabic-prepend", "\u0600"],
  ["punctuation-dot", "."],
  ["punctuation-plus", "+"],
  ["whitespace-space", " "],
  ["underscore", "_"],
  ["unicode-11a3a", "\u{11A3A}"],
  ["unicode-1acf", "\u{1ACF}"],
];

const FNV_OFFSET = 14695981039346656037n;
const FNV_PRIME = 1099511628211n;
const FNV_MASK = (1n << 64n) - 1n;

function fnvByte(hash, byte) {
  return ((hash ^ BigInt(byte)) * FNV_PRIME) & FNV_MASK;
}

function fnvU32(hash, value) {
  let current = hash;
  for (let shift = 0; shift < 32; shift += 8) {
    current = fnvByte(current, (value >>> shift) & 0xff);
  }
  return current;
}

function fnvUtf8(hash, text) {
  const bytes = Buffer.from(text, "utf8");
  let current = fnvU32(hash, bytes.length);
  for (const byte of bytes) current = fnvByte(current, byte);
  return current;
}

function scalarUtf16Boundaries(text) {
  const boundaries = [0];
  let offset = 0;
  for (const scalar of text) {
    offset += scalar.length;
    boundaries.push(offset);
  }
  return boundaries;
}

function hashWordProductCase(initial, text) {
  const boundaries = scalarUtf16Boundaries(text);
  let hash = fnvUtf8(initial, text);
  hash = fnvU32(hash, boundaries.length);
  for (const cursor of boundaries) {
    hash = fnvU32(hash, cursor);
    hash = fnvU32(hash, findWordForward(text, cursor));
    hash = fnvU32(hash, findWordBackward(text, cursor));
  }
  return { hash, boundaryCount: boundaries.length };
}

function cartesianWord(index, length) {
  const scalars = Array(length);
  let remainder = index;
  for (let position = length - 1; position >= 0; position--) {
    scalars[position] = wordProductAtoms[remainder % wordProductAtoms.length][1];
    remainder = Math.floor(remainder / wordProductAtoms.length);
  }
  return scalars.join("");
}

function hasThai(text) {
  return [...text].some((scalar) => scalar >= "\u0e00" && scalar <= "\u0e7f");
}

function wordOracleWitness(text) {
  return {
    text,
    segments: [...wordOracleSegmenter.segment(text)].map(({ segment, index, isWordLike }) => ({
      segment,
      index,
      isWordLike,
    })),
    observations: scalarUtf16Boundaries(text).map((cursor) => [
      cursor,
      findWordForward(text, cursor),
      findWordBackward(text, cursor),
    ]),
  };
}

function wordProductReceipt() {
  let digest = FNV_OFFSET;
  let nonThaiDigest = FNV_OFFSET;
  let caseCount = 0;
  let boundaryCount = 0;
  let nonThaiCaseCount = 0;
  let nonThaiBoundaryCount = 0;
  const buckets = [];
  const nonThaiBuckets = [];
  const caseDigests = [];
  for (let length = 1; length <= 4; length++) {
    const casesAtLength = wordProductAtoms.length ** length;
    const casesPerPrefix = wordProductAtoms.length ** (length - 1);
    for (let prefixIndex = 0; prefixIndex < wordProductAtoms.length; prefixIndex++) {
      let bucketDigest = FNV_OFFSET;
      let bucketBoundaryCount = 0;
      let nonThaiBucketDigest = FNV_OFFSET;
      let nonThaiBucketCaseCount = 0;
      let nonThaiBucketBoundaryCount = 0;
      const first = prefixIndex * casesPerPrefix;
      const end = first + casesPerPrefix;
      for (let index = first; index < end; index++) {
        const text = cartesianWord(index, length);
        const caseReceipt = hashWordProductCase(FNV_OFFSET, text);
        caseDigests.push(caseReceipt.hash.toString(16).padStart(16, "0"));
        const overall = hashWordProductCase(digest, text);
        digest = overall.hash;
        const bucket = hashWordProductCase(bucketDigest, text);
        bucketDigest = bucket.hash;
        caseCount++;
        boundaryCount += overall.boundaryCount;
        bucketBoundaryCount += bucket.boundaryCount;
        if (!hasThai(text)) {
          const nonThaiOverall = hashWordProductCase(nonThaiDigest, text);
          const nonThaiBucket = hashWordProductCase(nonThaiBucketDigest, text);
          nonThaiDigest = nonThaiOverall.hash;
          nonThaiBucketDigest = nonThaiBucket.hash;
          nonThaiCaseCount++;
          nonThaiBoundaryCount += nonThaiOverall.boundaryCount;
          nonThaiBucketCaseCount++;
          nonThaiBucketBoundaryCount += nonThaiBucket.boundaryCount;
        }
      }
      buckets.push({
        length,
        prefix: wordProductAtoms[prefixIndex][0],
        caseCount: casesPerPrefix,
        boundaryCount: bucketBoundaryCount,
        fnv1a64: bucketDigest.toString(16).padStart(16, "0"),
      });
      if (nonThaiBucketCaseCount > 0) {
        nonThaiBuckets.push({
          length,
          prefix: wordProductAtoms[prefixIndex][0],
          caseCount: nonThaiBucketCaseCount,
          boundaryCount: nonThaiBucketBoundaryCount,
          fnv1a64: nonThaiBucketDigest.toString(16).padStart(16, "0"),
        });
      }
    }
    if (casesAtLength !== casesPerPrefix * wordProductAtoms.length) {
      throw new Error("word product cardinality drifted");
    }
  }
  if (caseCount !== 41370 || boundaryCount !== 250044) {
    throw new Error(`word product shape drifted: ${caseCount} cases / ${boundaryCount} boundaries`);
  }
  if (nonThaiCaseCount !== 30940 || nonThaiBoundaryCount !== 152126) {
    throw new Error(
      `non-Thai product shape drifted: ${nonThaiCaseCount} cases / ${nonThaiBoundaryCount} boundaries`,
    );
  }
  return {
    algorithm: "FNV-1a-64 over UTF-8 text and LE u32 scalar-boundary/forward/back triples",
    lengths: [1, 2, 3, 4],
    atoms: wordProductAtoms.map(([label, text]) => ({ label, text })),
    caseCount,
    boundaryCount,
    fnv1a64: digest.toString(16).padStart(16, "0"),
    buckets,
    caseFnv1a64: caseDigests.join(""),
    nonThai: {
      verificationStatus: "verified-against-node-24.4.1-icu-77.1",
      caseCount: nonThaiCaseCount,
      boundaryCount: nonThaiBoundaryCount,
      fnv1a64: nonThaiDigest.toString(16).padStart(16, "0"),
      buckets: nonThaiBuckets,
    },
    defaultFallbackResidual: {
      verificationStatus: "partial-requires-m5-host-intl-segmenter",
      scope: "Thai-bearing cases where the pure ICU4X fallback differs from the Node oracle",
      caseCount: 1439,
      fnv1a64: "bf789ac4bcd5b222",
      witnesses: ["พัชรีพัชรี", "พัชรี\u0301"],
    },
    dictionaryWitness: wordOracleWitness("พัชรี"),
    redPandaWitness: wordOracleWitness("แพนด้าแดง"),
  };
}

const graphemeCases = [
  ["combining", "A\u0301B"],
  ["prepend", "\u0600AB"],
  ["unicode-11a3a", "A\u{11A3A}B"],
  ["unicode-1acf", "A\u{1ACF}B"],
];
const graphemeSegmentationCases = graphemeCases.map(([label, text]) => ({
  label,
  text,
  segments: [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text)].map(
    ({ segment, index }) => ({ segment, index }),
  ),
}));

const atomicText = "[paste #1 1200 chars]";
const atomicOptions = {
  segment: (text) => [{ segment: text, index: 0, input: text }],
  isAtomicSegment: (segment) => segment === atomicText,
};
wordNavigationCases.push(
  {
    label: "atomic-forward",
    text: atomicText,
    cursor: 0,
    direction: "forward",
    result: findWordForward(atomicText, 0, atomicOptions),
    atomic: true,
  },
  {
    label: "atomic-backward",
    text: atomicText,
    cursor: atomicText.length,
    direction: "backward",
    result: findWordBackward(atomicText, atomicText.length, atomicOptions),
    atomic: true,
  },
);

const identity = (text) => text;
const editorTheme = {
  borderColor: identity,
  selectList: {
    selectedPrefix: identity,
    selectedText: identity,
    description: identity,
    scrollIndicator: identity,
    noMatch: identity,
  },
};

function createEditor() {
  const tui = {
    terminal: { rows: 24 },
    requestRender() {},
  };
  const editor = new Editor(tui, editorTheme);
  const effects = [];
  editor.onChange = (text) => effects.push({ type: "change", text });
  editor.onSubmit = (text) => effects.push({ type: "submit", text });
  return { editor, effects, tui };
}

function editorSnapshot(editor, effects) {
  return {
    text: editor.getText(),
    expandedText: editor.getExpandedText(),
    lines: editor.getLines(),
    cursor: editor.getCursor(),
    effects: [...effects],
    pastes: [...editor.pastes.entries()],
    pasteCounter: editor.pasteCounter,
    history: [...editor.history],
    historyIndex: editor.historyIndex,
    killLength: editor.killRing.length,
    killPeek: editor.killRing.peek() ?? null,
    undoLength: editor.undoStack.length,
    lastAction: editor.lastAction,
    preferredVisualCol: editor.preferredVisualCol,
    snappedFromCursorCol: editor.snappedFromCursorCol,
    visualLines: editor.buildVisualLineMap(editor.lastWidth),
  };
}

function applyEditorAction(editor, tui, action) {
  switch (action.type) {
    case "set_text":
      editor.setText(action.text);
      break;
    case "insert_text":
      editor.insertTextAtCursor(action.text);
      break;
    case "type":
      editor.insertCharacter(action.text);
      break;
    case "paste":
      editor.handlePaste(action.text);
      break;
    case "new_line":
      editor.addNewLine();
      break;
    case "submit":
      editor.submitValue();
      break;
    case "backspace":
      editor.handleBackspace();
      break;
    case "delete_forward":
      editor.handleForwardDelete();
      break;
    case "line_start":
      editor.moveToLineStart();
      break;
    case "line_end":
      editor.moveToLineEnd();
      break;
    case "delete_line_start":
      editor.deleteToStartOfLine();
      break;
    case "delete_line_end":
      editor.deleteToEndOfLine();
      break;
    case "delete_word_backward":
      editor.deleteWordBackwards();
      break;
    case "delete_word_forward":
      editor.deleteWordForward();
      break;
    case "move_left":
      editor.moveCursor(0, -1);
      break;
    case "move_right":
      editor.moveCursor(0, 1);
      break;
    case "move_up":
      editor.moveCursor(-1, 0);
      break;
    case "move_down":
      editor.moveCursor(1, 0);
      break;
    case "move_word_backward":
      editor.moveWordBackwards();
      break;
    case "move_word_forward":
      editor.moveWordForwards();
      break;
    case "page_up":
      editor.pageScroll(-1);
      break;
    case "page_down":
      editor.pageScroll(1);
      break;
    case "jump_backward":
      editor.jumpToChar(action.text, "backward");
      break;
    case "jump_forward":
      editor.jumpToChar(action.text, "forward");
      break;
    case "add_history":
      editor.addToHistory(action.text);
      break;
    case "history_previous":
      editor.navigateHistory(-1);
      break;
    case "history_next":
      editor.navigateHistory(1);
      break;
    case "yank":
      editor.yank();
      break;
    case "yank_pop":
      editor.yankPop();
      break;
    case "undo":
      editor.undo();
      break;
    case "set_view":
      editor.lastWidth = action.width;
      tui.terminal.rows = action.rows;
      break;
    default:
      throw new Error(`unknown editor action: ${action.type}`);
  }
}

function editorTrace(label, actions) {
  const { editor, effects, tui } = createEditor();
  const initial = editorSnapshot(editor, effects);
  const steps = [];
  for (const action of actions) {
    effects.length = 0;
    applyEditorAction(editor, tui, action);
    steps.push({ action, state: editorSnapshot(editor, effects) });
  }
  return { label, initial, steps };
}

const tenLines = Array.from({ length: 10 }, (_, index) => `line-${index + 1}`).join("\n");
const elevenLines = Array.from({ length: 11 }, (_, index) => `line-${index + 1}`).join("\n");
const pageLines = Array.from({ length: 12 }, (_, index) => `row-${index}`).join("\n");
const nestedPasteOne = `${"x".repeat(1001)}[paste #2 1001 chars]`;
const nestedPasteTwo = "y".repeat(1001);

const editorTraces = [
  editorTrace("normalization-and-set-undo", [
    { type: "set_text", text: "a\r\nb\rc\td" },
    { type: "undo" },
  ]),
  editorTrace("typed-word-coalescence", [
    { type: "type", text: "a" },
    { type: "type", text: "b" },
    { type: "type", text: "c" },
    { type: "type", text: " " },
    { type: "type", text: "d" },
    { type: "type", text: "e" },
    { type: "undo" },
    { type: "undo" },
  ]),
  editorTrace("atomic-programmatic-insert", [
    { type: "set_text", text: "tail" },
    { type: "line_start" },
    { type: "insert_text", text: "A\r\nB\t" },
    { type: "undo" },
  ]),
  editorTrace("submit-effect-order", [
    { type: "set_text", text: "  hello\nworld  " },
    { type: "submit" },
    { type: "undo" },
  ]),
  editorTrace("grapheme-atomic-editing", [
    { type: "set_text", text: "A👩🏽‍💻éZ" },
    { type: "move_left" },
    { type: "backspace" },
    { type: "move_left" },
    { type: "delete_forward" },
    { type: "undo" },
    { type: "undo" },
  ]),
  editorTrace("paste-ten-lines-inline", [{ type: "paste", text: tenLines }]),
  editorTrace("paste-eleven-lines-marker", [
    { type: "paste", text: elevenLines },
    { type: "backspace" },
    { type: "undo" },
  ]),
  editorTrace("paste-marker-word-navigation", [
    { type: "paste", text: elevenLines },
    { type: "move_word_backward" },
    { type: "move_word_forward" },
  ]),
  editorTrace("nested-paste-map-order", [
    { type: "paste", text: nestedPasteOne },
    { type: "paste", text: nestedPasteTwo },
  ]),
  editorTrace("wide-paste-marker-navigation", [
    { type: "set_view", width: 5, rows: 24 },
    { type: "paste", text: elevenLines },
    { type: "move_up" },
    { type: "move_down" },
    { type: "page_up" },
    { type: "page_down" },
  ]),
  editorTrace("wide-paste-marker-resize-continuation", [
    { type: "set_view", width: 5, rows: 24 },
    { type: "paste", text: elevenLines },
    { type: "new_line" },
    { type: "type", text: "X" },
    { type: "move_up" },
    { type: "move_up" },
    { type: "set_view", width: 10, rows: 24 },
    { type: "move_down" },
  ]),
  editorTrace("wide-marker-width-two-prefix", [
    { type: "set_view", width: 2, rows: 24 },
    { type: "set_text", text: "a" },
    { type: "paste", text: elevenLines },
    { type: "move_up" },
    { type: "move_up" },
  ]),
  editorTrace("wide-marker-ascii-context", [
    { type: "set_view", width: 2, rows: 24 },
    { type: "set_text", text: "a" },
    { type: "paste", text: elevenLines },
    { type: "type", text: "b" },
    { type: "move_up" },
    { type: "move_down" },
  ]),
  editorTrace("wide-marker-whitespace-context", [
    { type: "set_view", width: 2, rows: 24 },
    { type: "set_text", text: "a " },
    { type: "paste", text: elevenLines },
    { type: "type", text: " " },
    { type: "type", text: "b" },
  ]),
  editorTrace("wide-marker-cjk-context", [
    { type: "set_view", width: 2, rows: 24 },
    { type: "set_text", text: "中" },
    { type: "paste", text: elevenLines },
    { type: "type", text: "界" },
  ]),
  editorTrace("adjacent-wide-paste-markers", [
    { type: "set_view", width: 5, rows: 20 },
    { type: "paste", text: elevenLines },
    { type: "paste", text: elevenLines },
    { type: "move_up" },
    { type: "move_down" },
    { type: "page_up" },
    { type: "set_view", width: 3, rows: 20 },
    { type: "page_down" },
  ]),
  editorTrace("wide-marker-width-one-page-snap-zero", [
    { type: "set_view", width: 1, rows: 20 },
    { type: "paste", text: elevenLines },
    { type: "page_up" },
    { type: "page_up" },
    { type: "page_down" },
  ]),
  editorTrace("literal-marker-is-not-owned", [
    { type: "set_text", text: "[paste #1 1200 chars]" },
    { type: "backspace" },
  ]),
  editorTrace("history-draft-and-placement", [
    { type: "add_history", text: " one " },
    { type: "add_history", text: "one" },
    { type: "add_history", text: "two" },
    { type: "set_text", text: "draft" },
    { type: "history_previous" },
    { type: "history_previous" },
    { type: "history_next" },
    { type: "history_next" },
  ]),
  editorTrace("kill-accumulate-yank-undo", [
    { type: "set_text", text: "alpha beta\ngamma" },
    { type: "delete_word_backward" },
    { type: "delete_word_backward" },
    { type: "delete_line_start" },
    { type: "yank" },
    { type: "undo" },
    { type: "undo" },
  ]),
  editorTrace("yank-pop-cycle", [
    { type: "set_text", text: "one two three" },
    { type: "delete_word_backward" },
    { type: "move_word_backward" },
    { type: "delete_word_backward" },
    { type: "yank" },
    { type: "yank_pop" },
    { type: "undo" },
  ]),
  editorTrace("multi-line-character-jump", [
    { type: "set_text", text: "aba\ncab" },
    { type: "jump_backward", text: "a" },
    { type: "jump_backward", text: "a" },
    { type: "jump_forward", text: "b" },
    { type: "jump_forward", text: "x" },
  ]),
  editorTrace("preferred-logical-column", [
    { type: "set_text", text: "123456\nx\nabcdef" },
    { type: "move_up" },
    { type: "move_up" },
    { type: "move_down" },
    { type: "move_down" },
  ]),
  editorTrace("preferred-wrapped-column", [
    { type: "set_view", width: 4, rows: 24 },
    { type: "set_text", text: "abcdefgh\nx\nabcdefgh" },
    { type: "move_up" },
    { type: "move_up" },
    { type: "move_up" },
  ]),
  editorTrace("page-actions", [
    { type: "set_view", width: 80, rows: 20 },
    { type: "set_text", text: pageLines },
    { type: "page_up" },
    { type: "page_up" },
    { type: "page_down" },
  ]),
];

const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    runtime: expectedRuntime,
    files: {
      "kill-ring.js": digest(killRingPath),
      "undo-stack.js": digest(undoStackPath),
      "word-navigation.js": digest(wordNavigationPath),
      "utils.js": digest(utilsPath),
      "components/editor.js": digest(editorPath),
    },
  },
  killRing: [
    killRingTrace("empty-and-single", [
      { type: "push", text: "", prepend: false, accumulate: false },
      { type: "push", text: "alpha", prepend: false, accumulate: false },
      { type: "rotate" },
    ]),
    killRingTrace("accumulate-directions", [
      { type: "push", text: "middle", prepend: false, accumulate: false },
      { type: "push", text: "-tail", prepend: false, accumulate: true },
      { type: "push", text: "head-", prepend: true, accumulate: true },
    ]),
    killRingTrace("rotate-cycle", [
      { type: "push", text: "one", prepend: false, accumulate: false },
      { type: "push", text: "two", prepend: false, accumulate: false },
      { type: "push", text: "three", prepend: false, accumulate: false },
      { type: "rotate" },
      { type: "rotate" },
      { type: "rotate" },
    ]),
  ],
  undoStack: undoTrace,
  graphemeSegmentation: graphemeSegmentationCases,
  wordSegmentation: wordSegmentationCases,
  wordNavigation: wordNavigationCases,
  wordProduct: wordProductReceipt(),
  editor: editorTraces,
};

const fixturePath = join(root, "crates/pie-core/tests/fixtures/editor-state.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log(
  `wrote editor-state fixture: ${out.killRing.length} kill-ring traces, ${out.undoStack.length} undo steps, ${out.wordNavigation.length} word-navigation vectors, ${out.wordProduct.caseCount} product strings, ${out.editor.length} editor traces`,
);
