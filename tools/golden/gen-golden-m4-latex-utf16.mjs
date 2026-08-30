// gen-golden-m4-latex-utf16.mjs — harvest renderLatex behavior as raw UTF-16
// units. Arrays keep lone surrogate outputs durable instead of asking JSON or a
// Rust String to represent values that only JavaScript strings can contain.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-m4-latex-utf16.mjs
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXPECTED = {
  package: "@earendil-works/pi-tui",
  version: "0.84.1",
  indexDtsSha256: "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3",
  latexDtsSha256: "76a3bda961e678e859bf8749d68b40a4ce20a08a701329e92758dedda79812f8",
  latexJsSha256: "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716",
  utilsJsSha256: "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
  node: "24.4.1",
  icu: "77.1",
  unicode: "16.0",
};

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const packageJson = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const actual = {
  package: packageJson.name,
  version: packageJson.version,
  indexDtsSha256: sha256(join(dist, "index.d.ts")),
  latexDtsSha256: sha256(join(dist, "latex.d.ts")),
  latexJsSha256: sha256(join(dist, "latex.js")),
  utilsJsSha256: sha256(join(dist, "utils.js")),
  node: process.versions.node,
  icu: process.versions.icu,
  unicode: process.versions.unicode,
};
for (const [key, expected] of Object.entries(EXPECTED)) {
  if (actual[key] !== expected) {
    throw new Error(`unexpected ${key}: expected ${expected}, got ${actual[key]}`);
  }
}

const { renderLatex } = await import(pathToFileURL(join(dist, "latex.js")));
const units = (value) =>
  Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
const fromUnits = (value) => String.fromCharCode(...value);
const harvest = ({ name, source, sourceUnits, options, family, body, form }) => {
  const exactSourceUnits = sourceUnits ?? units(source);
  const output = renderLatex(fromUnits(exactSourceUnits), options);
  return {
    name,
    ...(family ? { family } : {}),
    ...(body ? { body } : {}),
    ...(form ? { form } : {}),
    sourceUnits: exactSourceUnits,
    ...(options ? { options } : {}),
    outputUnits: output == null ? null : units(output),
  };
};

const bodyValues = [
  ["empty", ""],
  ["simple", "x"],
  ["multicodepoint", "ab"],
  ["cjk", "界"],
  ["astral", String.fromCodePoint(0x1f600)],
  ["combining", `x${String.fromCharCode(0x0301)}`],
];

// Six constructs x six bodies x three structural placements = 108 rows. The
// forms exercise the same body directly, through an otherwise-transparent
// group, and nested below another syntax-bearing node.
const grammarFactories = {
  bar: {
    direct: (body) => `\\bar{${body}}`,
    grouped: (body) => `{\\bar{${body}}}`,
    nested: (body) => `\\bar{{${body}}}`,
  },
  root: {
    direct: (body) => `\\sqrt{${body}}`,
    grouped: (body) => `{\\sqrt{${body}}}`,
    nested: (body) => `\\sqrt{{${body}}}`,
  },
  fraction: {
    direct: (body) => `\\frac{${body}}{q}`,
    grouped: (body) => `{\\frac{${body}}{q}}`,
    nested: (body) => `\\frac{{${body}}}{{q}}`,
  },
  subscript: {
    direct: (body) => `x_{${body}}`,
    grouped: (body) => `{x_{${body}}}`,
    nested: (body) => `x_{{${body}}}`,
  },
  superscript: {
    direct: (body) => `x^{${body}}`,
    grouped: (body) => `{x^{${body}}}`,
    nested: (body) => `x^{{${body}}}`,
  },
  unbracedRoot: {
    direct: (body) => `\\sqrt ${body}`,
    grouped: (body) => `{\\sqrt ${body}}`,
    nested: (body) => `\\frac{\\sqrt ${body}}{q}`,
  },
};

const grammarProduct = [];
for (const [family, forms] of Object.entries(grammarFactories)) {
  for (const [body, value] of bodyValues) {
    for (const [form, makeSource] of Object.entries(forms)) {
      grammarProduct.push(
        harvest({
          name: `grammar-${family}-${body}-${form}`,
          family,
          body,
          form,
          source: makeSource(value),
        }),
      );
    }
  }
}

const astral = [0xd83d, 0xde00];
const exactEdges = [
  { name: "edge-bar-simple", source: "\\bar{x}" },
  { name: "edge-bar-multicodepoint", source: "\\bar{ab}" },
  { name: "edge-nth-root-empty", source: "\\sqrt[5]{}" },
  { name: "edge-fraction-cjk-denominator", source: "\\frac{x}{界}" },
  { name: "edge-fraction-astral-denominator", sourceUnits: units("\\frac{x}{").concat(astral, units("}")) },
  { name: "edge-fraction-combining-denominator", source: `\\frac{x}{x${String.fromCharCode(0x0301)}}` },
  { name: "edge-fraction-display-cjk-width", source: "\\frac{a}{界}", options: { display: true } },
  { name: "edge-fraction-display-astral-width", sourceUnits: units("\\frac{a}{").concat(astral, units("}")), options: { display: true } },
  { name: "edge-fraction-display-combining-width", source: `\\frac{a}{x${String.fromCharCode(0x0301)}}`, options: { display: true } },
  { name: "edge-fraction-display-lone-high-width", sourceUnits: units("\\frac{a}{").concat([0xd83d], units("}")), options: { display: true } },
  { name: "edge-fraction-display-lone-low-width", sourceUnits: units("\\frac{a}{").concat([0xde00], units("}")), options: { display: true } },
  { name: "edge-nested-group-script", source: "x_{{a^b}}" },
  { name: "edge-unbraced-astral-root", sourceUnits: units("\\sqrt ").concat(astral) },
  { name: "edge-unbraced-astral-fraction", sourceUnits: units("\\frac").concat(astral, units("x")) },
  { name: "edge-lone-high-plain", sourceUnits: [0x41, 0xd83d, 0x42] },
  { name: "edge-lone-low-plain", sourceUnits: [0x41, 0xde00, 0x42] },
  { name: "edge-lone-high-bar", sourceUnits: units("\\bar{").concat([0xd83d], units("}")) },
  { name: "edge-lone-low-root", sourceUnits: units("\\sqrt{").concat([0xde00], units("}")) },
  { name: "edge-bmp-pua-collision", sourceUnits: [0x41, 0xe000, 0x42] },
  { name: "edge-supplementary-pua-collision", sourceUnits: [0x41, 0xdbc0, 0xdc00, 0x42] },
].map(harvest);

const roundtripSources = [
  [0x61],
  [0x754c],
  [0x78, 0x0301],
  astral,
  [0xd83d],
  [0xde00],
  [0xe000],
  [0xdbc0, 0xdc00],
  [0xdbff, 0xdfff],
  [0x41, 0xd83d, 0x42, 0xde00, 0x43, 0xe000, 0xdbc0, 0xdc00],
];
const roundtrip = roundtripSources.map((sourceUnits, index) =>
  harvest({ name: `plain-unit-roundtrip-${String(index).padStart(2, "0")}`, sourceUnits }),
);

// Detached-review corpus: a finite 861-case product spanning valid, malformed,
// BMP, astral, and lone-surrogate inputs. Keep this definition independent of
// the Rust implementation so the fixture remains a black-box npm oracle.
const reviewCases = [];
const addReview = (name, sourceUnits, display = false) =>
  reviewCases.push({ name, sourceUnits, options: { display } });
const concatUnits = (...parts) =>
  parts.flatMap((part) => (typeof part === "string" ? units(part) : part));

const reviewBodies = [
  ["empty", []],
  ["ascii", units("x")],
  ["multi", units("ab")],
  ["cjk", units("界")],
  ["combining", units("x\u0301")],
  ["astral", [0xd83d, 0xde00]],
  ["lone-high-start", [0xd800]],
  ["lone-high-end", [0xdbff]],
  ["lone-low-start", [0xdc00]],
  ["lone-low-end", [0xdfff]],
  ["reversed-pair", [0xdc00, 0xd800]],
  ["bmp-pua", [0xe000]],
  ["supp-pua", [0xdbc0, 0xdc00]],
  ["nul", [0]],
];

for (const display of [false, true]) {
  for (const [label, body] of reviewBodies) {
    addReview(`plain-${label}-d${Number(display)}`, body, display);
    addReview(`bar-braced-${label}-d${Number(display)}`, concatUnits("\\bar{", body, "}"), display);
    addReview(`bar-grouped-${label}-d${Number(display)}`, concatUnits("{\\bar{", body, "}}"), display);
    addReview(`bar-nested-${label}-d${Number(display)}`, concatUnits("\\bar{{", body, "}}"), display);
    addReview(`bar-unbraced-${label}-d${Number(display)}`, concatUnits("\\bar ", body), display);
    addReview(`root-braced-${label}-d${Number(display)}`, concatUnits("\\sqrt{", body, "}"), display);
    addReview(`root-grouped-${label}-d${Number(display)}`, concatUnits("{\\sqrt{", body, "}}"), display);
    addReview(`root-nested-${label}-d${Number(display)}`, concatUnits("\\sqrt{{", body, "}}"), display);
    addReview(`root-unbraced-${label}-d${Number(display)}`, concatUnits("\\sqrt ", body), display);
    addReview(`root-five-${label}-d${Number(display)}`, concatUnits("\\sqrt[5]{", body, "}"), display);
    addReview(`sub-braced-${label}-d${Number(display)}`, concatUnits("x_{", body, "}"), display);
    addReview(`sub-grouped-${label}-d${Number(display)}`, concatUnits("{x_{", body, "}}"), display);
    addReview(`sub-nested-${label}-d${Number(display)}`, concatUnits("x_{{", body, "}}"), display);
    addReview(`sub-unbraced-${label}-d${Number(display)}`, concatUnits("x_", body), display);
    addReview(`super-braced-${label}-d${Number(display)}`, concatUnits("x^{", body, "}"), display);
    addReview(`super-nested-${label}-d${Number(display)}`, concatUnits("x^{{", body, "}}"), display);
    addReview(`super-unbraced-${label}-d${Number(display)}`, concatUnits("x^", body), display);
  }
}

const reviewFractionParts = [
  ["empty", []],
  ["x", units("x")],
  ["ab", units("ab")],
  ["cjk", units("界")],
  ["combining", units("x\u0301")],
  ["astral", [0xd83d, 0xde00]],
  ["high", [0xd83d]],
  ["low", [0xde00]],
  ["plus", units("a+b")],
  ["space", units("a b")],
  ["nested-frac", units("\\frac{a}{b}")],
  ["nested-group", units("{{a}}")],
];
for (const display of [false, true]) {
  for (const [leftName, left] of reviewFractionParts) {
    for (const [rightName, right] of reviewFractionParts) {
      addReview(
        `fraction-${leftName}-over-${rightName}-d${Number(display)}`,
        concatUnits("\\frac{", left, "}{", right, "}"),
        display,
      );
    }
  }
}

for (const display of [false, true]) {
  for (const [name, source] of [
    ["fraction-dot-numerator", "\\frac{.}{x}"],
    ["fraction-decimal-numerator", "\\frac{1.2}{x}"],
    ["fraction-letter-dot-numerator", "\\frac{a.b}{x}"],
    ["fraction-digit-denominator", "\\frac{x}{12}"],
    ["fraction-decimal-denominator", "\\frac{x}{1.2}"],
    ["fraction-dot-denominator", "\\frac{x}{.}"],
    ["fraction-decimal-both", "\\frac{1.2}{3.4}"],
    ["root-dot", "\\sqrt{.}"],
    ["root-decimal", "\\sqrt{1.2}"],
    ["bar-command", "\\bar\\alpha"],
    ["script-dot-sub", "x_{.}"],
    ["script-decimal-sub", "x_{1.2}"],
    ["script-dot-super", "x^{.}"],
    ["script-decimal-super", "x^{1.2}"],
    ["operator-duplicate-sub", "\\sum_1_2"],
    ["operator-duplicate-super", "\\sum^1^2"],
  ]) {
    addReview(`${name}-d${Number(display)}`, units(source), display);
  }
}

for (const [label, pair] of [
  ["astral", [0xd83d, 0xde00]],
  ["high-low-boundary", [0xdbff, 0xdc00]],
  ["low-high", [0xde00, 0xd83d]],
  ["two-high", [0xd800, 0xdbff]],
  ["two-low", [0xdc00, 0xdfff]],
]) {
  addReview(`unbraced-fraction-${label}`, concatUnits("\\frac", pair, "q"));
  addReview(`unbraced-root-${label}`, concatUnits("\\sqrt ", pair));
  addReview(`unbraced-bar-${label}`, concatUnits("\\bar ", pair));
  addReview(`unbraced-sub-${label}`, concatUnits("x_", pair));
  addReview(`unbraced-super-${label}`, concatUnits("x^", pair));
}

for (const [name, source] of [
  ["empty-group", "{}"],
  ["nested-empty-group", "{{}}"],
  ["deep-empty-group", "{{{}}}"],
  ["empty-between-text", "a{}b"],
  ["empty-scripts", "x^{}_{}"],
  ["nested-script", "x_{{a^b}}"],
  ["nested-fraction", "\\frac{{\\frac{a}{b}}}{{\\frac{c}{d}}}"],
  ["root-empty", "\\sqrt{}"],
  ["root-nested-empty", "\\sqrt{{}}"],
  ["bar-empty", "\\bar{}"],
  ["bar-nested-empty", "\\bar{{}}"],
  ["stray-close", "}"],
  ["unclosed-group", "{x"],
  ["fraction-missing-denominator", "\\frac{x}"],
  ["fraction-unclosed", "\\frac{x}{y"],
  ["root-unclosed", "\\sqrt{x"],
  ["script-missing", "x^"],
  ["script-space-only", "x^ "],
  ["bare-command-slash", "\\"],
  ["unsupported", "\\definitelyUnsupported{x}"],
]) {
  addReview(name, units(source));
  addReview(`${name}-display`, units(source), true);
}

if (reviewCases.length !== 861) {
  throw new Error(`review corpus drifted: expected 861 cases, got ${reviewCases.length}`);
}
const reviewProduct = reviewCases.map(({ name, sourceUnits, options }) =>
  harvest({ name, sourceUnits, options }),
);

// Additional exact witnesses make the generalized accent rule and the
// operator/non-operator duplicate-script distinction explicit.
const repairEdges = [
  { name: "repair-hat-unbraced-text", source: "\\hat x" },
  { name: "repair-vec-unbraced-command", source: "\\vec\\alpha" },
  { name: "repair-hat-unbraced-astral", sourceUnits: concatUnits("\\hat ", astral) },
  { name: "repair-vec-unbraced-lone-high", sourceUnits: concatUnits("\\vec ", [0xd83d]) },
  { name: "repair-ordinary-duplicate-sub", source: "x_1_2" },
  { name: "repair-ordinary-duplicate-super", source: "x^1^2" },
  { name: "repair-ordinary-interleaved-duplicate", source: "x_1^2_3" },
  { name: "repair-operator-mixed-scripts", source: "\\sum_1^2" },
].map(harvest);

const accentArgumentMismatches = [];
for (const display of [false, true]) {
  for (const [label] of reviewBodies.slice(1)) {
    accentArgumentMismatches.push(`bar-unbraced-${label}-d${Number(display)}`);
  }
  accentArgumentMismatches.push(`bar-command-d${Number(display)}`);
}
accentArgumentMismatches.push(
  "unbraced-bar-astral",
  "unbraced-bar-high-low-boundary",
  "unbraced-bar-low-high",
  "unbraced-bar-two-high",
  "unbraced-bar-two-low",
);
const displayFractionMismatches = reviewFractionParts.map(
  ([leftName]) => `fraction-${leftName}-over-empty-d1`,
);
const rawUnitWidthMismatches = [
  ...["ab", "cjk", "astral"].flatMap((left) =>
    ["high", "low"].map((right) => `fraction-${left}-over-${right}-d1`),
  ),
  ...["high", "low"].flatMap((left) =>
    ["ab", "cjk", "astral"].map((right) => `fraction-${left}-over-${right}-d1`),
  ),
];
const inlineGroupingMismatches = [
  "fraction-dot-numerator-d0",
  "fraction-decimal-numerator-d0",
  "fraction-letter-dot-numerator-d0",
  "fraction-digit-denominator-d0",
  "fraction-decimal-denominator-d0",
  "fraction-decimal-both-d0",
];
const rootGroupingMismatches = [
  "root-dot-d0",
  "root-decimal-d0",
  "root-dot-d1",
  "root-decimal-d1",
];
const operatorDuplicateMismatches = [
  "operator-duplicate-sub-d0",
  "operator-duplicate-super-d0",
  "operator-duplicate-sub-d1",
  "operator-duplicate-super-d1",
];
const preRepairMismatchFamilies = {
  "accent-parse-argument": accentArgumentMismatches,
  "display-fraction-normalization": displayFractionMismatches,
  "raw-unit-visible-width": rawUnitWidthMismatches,
  "inline-grouping-predicates": inlineGroupingMismatches,
  "root-grouping-predicate": rootGroupingMismatches,
  "operator-duplicate-scripts": operatorDuplicateMismatches,
};
const preRepairMismatchNames = Object.values(preRepairMismatchFamilies).flat();
if (preRepairMismatchNames.length !== 71 || new Set(preRepairMismatchNames).size !== 71) {
  throw new Error("pre-repair mismatch receipt must contain 71 unique rows");
}

const fixture = {
  generator: "gen-golden-m4-latex-utf16.mjs",
  reference: actual,
  grammar: {
    description: "six constructs x six body classes x direct/grouped/nested structural forms",
    constructs: Object.keys(grammarFactories),
    bodies: bodyValues.map(([name]) => name),
    forms: ["direct", "grouped", "nested"],
    caseCount: grammarProduct.length,
  },
  // These rows were observed to disagree with the pre-UTF16 scalar candidate;
  // they make the oracle's pre-implementation red state durable without
  // checking a broken implementation into the repository.
  preImplementationRedWitnesses: [
    "edge-unbraced-astral-root",
    "edge-unbraced-astral-fraction",
    "edge-lone-high-plain",
    "edge-lone-low-plain",
  ],
  grammarProduct,
  exactEdges,
  roundtrip,
  detachedReview: {
    baselineHead: "d0291a6e9ca67cd08f3af26c68867ac05aa96078",
    caseCount: reviewProduct.length,
    preRepairMismatchCount: preRepairMismatchNames.length,
    preRepairMismatchFamilies,
  },
  reviewProduct,
  repairEdges,
};

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-core/tests/fixtures");
writeFileSync(join(outDir, "m4-latex-utf16.json"), `${JSON.stringify(fixture, null, 1)}\n`);
console.log(
  `harvested ${grammarProduct.length} grammar, ${exactEdges.length} edge, ${roundtrip.length} UTF-16 roundtrip, ${reviewProduct.length} detached-review, and ${repairEdges.length} repair-edge cases`,
);
