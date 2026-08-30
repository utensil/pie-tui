#!/usr/bin/env node
// gen-golden-text.mjs — harvest behavior vectors from the pinned pi-tui build.
//   PI_TUI_DIST=... node tools/golden/gen-golden-text.mjs
// Deterministic corpus only (no randomness) so fixtures are stable review artifacts.
import { writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
const utils = await import("file://" + join(dist, "utils.js"));
const {
  visibleWidth,
  stripTerminalSequences,
  extractAnsiCode,
  wrapTextWithAnsi,
  truncateToWidth,
  sliceByColumn,
  sliceWithWidth,
  getGraphemeCellRange,
  getOsc8LinkAtColumn,
  extractSegments,
  applyBackgroundToLine,
  normalizeTerminalOutput,
} = utils;

const corpus = [
  "", "hello", "  spaces  ", "ascii~!@#",
  "\t tabbed \t", "a\tb",
  "こんにちは世界", "カタカナひらがな", "한국어 텍스트", "漢字CJK·混排mixed",
  "e\u0301clair", "é", "naïve",
  "กัดฟันไทย", "สระอำ", "ໍາລາວ",
  "\u0915\u094D\u0937 devanagari-ks\u0323\u0301",
  "\u{1F600} grin", "\u{1F1EF}\u{1F1F5} flag-jp", "\u{1F468}\u200D\u{1F469}\u200D\u{1F466}\u200D\u{1F466} family",
  "\u{1F44D}\u{1F3FD} thumbsup-tone", "\u2714\uFE0F check-vs16", "1\uFE0F\u20E3 keycap",
  "\u{1F441}\uFE0F\u200D\u{1F5E8}\uFE0F eye-speech",
  "\u00B1 ambiguous", "\u2190 arrows", "\u2014 em—dash …ellipsis",
  "\uFF80\uFF9E halfwidth-pa\u0301",
  "\x1b[31mred\x1b[0m plain-after", "\x1b[1;32mbold-green\x1b[39m\x1b[22m", "\x1b]8;;https://x.example/link\x07link text\x1b]8;;\x07",
  "mix \x1b[4munder\u6f22\u5b57\x1b[24m tail", "\x1b_e-marker\x07 apc?",
  "trailing-csi-no-term\x1b[31", "lone-\x1b escape",
  "\u{10FFFF} max-plane", "\u0007 bel-in-view", "\u{200B} zwsp",
  // M1c corpus: ANSI-across-linebreaks, CJK breaks, tabs, truncation shapes.
  "\x1b[31mred first\ncontinues red\n\x1b[0mplain tail",
  "line1 \x1b[1;4mbold-underline\nstill bold tail",
  "\x1b]8;;https://x.example/abc\x07click \x1b]8;;\x07here\n\x1b]8;;https://y.example/xyz\x07again\x1b]8;;\x07",
  "line\r\nwith crlf\rand lone-cr",
  "trailing newline\n",
  "\x1b[44m \x1b[0m\n\x1b[45m \x1b[0m",
  "漢字テキストは折り返しされる",
  "한국어 가나다른 english mix",
  "中文 english 中文 mix with 記号!",
  "short 漢字 lines\nsecond 漢字 line",
  "supercalifragilisticexpialidocious",
  "\x1b[32mgreen-longword-with-dashes\x1b[0m tail",
  "\x1b[31mre\x1b[33myellow-re\x1b[0mally-long-styled-token",
  "\ttab start", "a\tb\tc", "\x1b[44m\tbg-tab\x1b[0m",
  "\x1b[38;5;240mgray text here\x1b[0m",
  "\x1b[38;2;12;34;56mtruecolor words\x1b[0m end",
  "ambiguous±width 日本語",
  "กัด ฟัน ไทย wrap",
];

const WRAP_WIDTHS = [3, 5, 8, 10, 12, 20];
const EMBEDDED_SGR_WRAP_INPUTS = [
  "\x1b[?25h\x1b[31mAB",
  "\x1b[?25l\x1b[1mAB",
  "\x1b[?1049h\x1b[38;5;240mAB",
  "\x1b[31mA\x1b[?25h\x1b[0mB",
  "\x1b[31mA\x1b[?25l\x1b[39mB",
];
const TRUNC_CASES = [
  ...[0, 1, 2, 3, 5, 8, 12, 20, 40].flatMap((maxWidth) => [
    { maxWidth, ellipsis: "...", pad: false },
    { maxWidth, ellipsis: "...", pad: true },
  ]),
  { maxWidth: 2, ellipsis: "…", pad: false },
  { maxWidth: 1, ellipsis: "...", pad: false },
  { maxWidth: 4, ellipsis: "", pad: true },
];
const SLICE_CASES = [
  [0, 4], [0, 5], [2, 4], [3, 10], [0, 1], [1, 1], [6, 8], [0, 0], [4, 100],
];
const SEG_CASES = [
  { beforeEnd: 2, afterStart: 2, afterLen: 4 },
  { beforeEnd: 0, afterStart: 3, afterLen: 3 },
  { beforeEnd: 5, afterStart: 5, afterLen: 5 },
  { beforeEnd: 3, afterStart: 3, afterLen: 0 },
];
const BG_FN = (s) => `\x1b[41m${s}\x1b[0m`;

const cases = [];
for (const input of corpus) {
  // Record escape scans as JS-code-unit index + length pairs; the Rust loader maps
  // each unit index to its byte offset before comparing.
  let ansi;
  if (input.includes("\x1b")) {
    ansi = [];
    let unit = 0;
    for (const ch of input) {
      if (ch === "\x1b") {
        const r = extractAnsiCode(input, unit);
        ansi.push({ at: unit, len: r ? r.length : null });
      }
      unit += ch.codePointAt(0) > 0xffff ? 2 : 1;
    }
  }
  const wrap = WRAP_WIDTHS.map((width) => ({ width, lines: wrapTextWithAnsi(input, width) }));
  const trunc = TRUNC_CASES.map(({ maxWidth, ellipsis, pad }) => ({
    maxWidth, ellipsis, pad,
    result: truncateToWidth(input, maxWidth, ellipsis, pad),
  }));
  const slice = SLICE_CASES.flatMap(([startCol, length]) =>
    [false, true].map((strict) => {
      const r = sliceWithWidth(input, startCol, length, strict);
      return { startCol, length, strict, text: r.text, width: r.width };
    })
  );
  const vw = visibleWidth(input);
  const columns = [];
  for (let col = 0; col <= Math.min(vw, 12) + 1; col++) {
    columns.push({
      column: col,
      cellRange: getGraphemeCellRange(input, col) ?? null,
      osc8: getOsc8LinkAtColumn(input, col) ?? null,
    });
  }
  const segments = SEG_CASES.flatMap(({ beforeEnd, afterStart, afterLen }) =>
    [false, true].map((strictAfter) => {
      const r = extractSegments(input, beforeEnd, afterStart, afterLen, strictAfter);
      return { beforeEnd, afterStart, afterLen, strictAfter, ...r };
    })
  );
  const bg = [8, 12].map((width) => ({
    width,
    result: applyBackgroundToLine(input, width, BG_FN),
  }));
  cases.push({
    input,
    visibleWidth: visibleWidth(input),
    stripped: stripTerminalSequences(input),
    ansi,
    wrap,
    trunc,
    slice,
    columns,
    segments,
    bg,
    normalize: normalizeTerminalOutput(input),
  });
}

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-core/tests");
const embeddedSgrWrapCases = EMBEDDED_SGR_WRAP_INPUTS.map((input) => ({
  input,
  width: 1,
  lines: wrapTextWithAnsi(input, 1),
}));
writeFileSync(join(outDir, "fixtures/text-golden.json"), JSON.stringify({ generator: "gen-golden-text.mjs", reference: process.env.PI_TUI_REF_VERSION ?? "0.84.1", cases, embeddedSgrWrapCases }, null, 1) + "\n");
console.log(`harvested ${cases.length} cases and ${embeddedSgrWrapCases.length} embedded-SGR wrap cases`);
