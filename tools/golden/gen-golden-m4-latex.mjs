// gen-golden-m4-latex.mjs — harvest renderLatex black-box behavior from the
// exact pi-tui reference distribution. The generator checks the public
// declaration hashes before loading the runtime so fixtures cannot drift to a
// different package build with the same nominal version.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXPECTED_VERSION = "0.84.1";
const EXPECTED_INDEX_DTS_SHA256 = "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3";
const EXPECTED_LATEX_DTS_SHA256 = "76a3bda961e678e859bf8749d68b40a4ce20a08a701329e92758dedda79812f8";
const EXPECTED_LATEX_JS_SHA256 = "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const packageJson = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const indexDtsSha256 = sha256(join(dist, "index.d.ts"));
const latexDtsSha256 = sha256(join(dist, "latex.d.ts"));
const latexJsSha256 = sha256(join(dist, "latex.js"));

if (packageJson.version !== EXPECTED_VERSION) {
  throw new Error(`expected pi-tui ${EXPECTED_VERSION}, got ${packageJson.version}`);
}
if (indexDtsSha256 !== EXPECTED_INDEX_DTS_SHA256) {
  throw new Error(`unexpected index.d.ts sha256: ${indexDtsSha256}`);
}
if (latexDtsSha256 !== EXPECTED_LATEX_DTS_SHA256) {
  throw new Error(`unexpected latex.d.ts sha256: ${latexDtsSha256}`);
}
if (latexJsSha256 !== EXPECTED_LATEX_JS_SHA256) {
  throw new Error(`unexpected latex.js sha256: ${latexJsSha256}`);
}

const { renderLatex } = await import(pathToFileURL(join(dist, "latex.js")));

const cases = [
  { name: "empty", source: "" },
  { name: "whitespace", source: "  \t\n " },
  { name: "plain-inline", source: "x + y = z" },
  { name: "greek-symbols", source: "\\alpha + \\beta \\to \\Gamma" },
  { name: "relations-and-sets", source: "x \\in \\mathbb{R},\\quad x \\leq y \\neq z" },
  { name: "scripts", source: "x_{i+1}^2 + y^{n-1}" },
  { name: "fraction-inline", source: "\\frac{a+b}{c-d}" },
  { name: "fraction-display", source: "\\frac{a+b}{c-d}", options: { display: true } },
  { name: "nested-fraction-display", source: "\\frac{1}{1+\\frac{1}{x}}", options: { display: true } },
  { name: "sqrt-and-absolute", source: "\\sqrt{x^2 + y^2} + \\left|z\\right|" },
  {
    name: "matrix-inline",
    source: "\\begin{pmatrix}a & b \\\\ c & d\\end{pmatrix}",
  },
  {
    name: "matrix-display",
    source: "\\begin{bmatrix}1 & 22 \\\\ 333 & 4\\end{bmatrix}",
    options: { display: true },
  },
  {
    name: "operators-inline",
    source: "\\sum_{i=1}^{n} i + \\int_0^\\infty e^{-x} \\, dx",
  },
  {
    name: "operators-display",
    source: "\\sum_{i=1}^{n} i + \\prod_{k=1}^{m} k",
    options: { display: true },
  },
  { name: "named-functions", source: "\\sin(\\theta) + \\log_{2}(x)" },
  { name: "text-and-style", source: "\\text{speed} = \\mathbf{v} / \\mathrm{t}" },
  { name: "malformed-unclosed-group", source: "x + {y" },
  { name: "malformed-fraction", source: "\\frac{a}" },
  {
    name: "malformed-matrix",
    source: "\\begin{matrix}a & b \\\\ c\\end{matrix}",
  },
  { name: "unsupported-command", source: "\\definitelyUnsupported{x}" },
];

const adversarialCases = [
  {
    name: "variant-greek",
    source: "\\varepsilon \\vartheta \\varpi \\varrho \\varsigma \\varphi",
  },
  {
    name: "uppercase-greek",
    source: "\\Gamma \\Delta \\Theta \\Lambda \\Xi \\Pi \\Sigma \\Upsilon \\Phi \\Psi \\Omega",
  },
  {
    name: "arrow-family",
    source:
      "\\leftarrow \\rightarrow \\leftrightarrow \\Leftarrow \\Rightarrow \\Leftrightarrow \\mapsto \\uparrow \\downarrow",
  },
  {
    name: "set-logic-family",
    source:
      "\\notin \\ni \\subset \\supset \\subseteq \\supseteq \\cup \\cap \\wedge \\vee",
  },
  {
    name: "binary-symbol-family",
    source:
      "\\pm \\mp \\times \\div \\cdot \\ast \\star \\circ \\bullet \\oplus \\otimes",
  },
  {
    name: "relation-family",
    source: "\\equiv \\sim \\simeq \\cong \\approx \\propto \\ll \\gg",
  },
  {
    name: "left-angle-middle",
    source: "\\left\\langle x \\middle| y \\right\\rangle",
  },
  {
    name: "left-brace-middle",
    source: "\\left\\{ x \\middle\\| y \\right\\}",
  },
  { name: "nth-root-two", source: "\\sqrt[2]{x+1}" },
  { name: "nth-root-three", source: "\\sqrt[3]{x+1}" },
  { name: "nth-root-four", source: "\\sqrt[4]{x+1}" },
  { name: "nth-root-five", source: "\\sqrt[5]{x+1}" },
  { name: "root-unbraced", source: "\\sqrt x" },
  { name: "fraction-unbraced", source: "\\frac12" },
  { name: "fraction-empty-both", source: "\\frac{}{}" },
  { name: "fraction-empty-numerator", source: "\\frac{}{x}" },
  { name: "fraction-empty-denominator", source: "\\frac{x}{}" },
  { name: "fraction-nested-inline", source: "\\frac{\\frac{a}{b}}{\\frac{c}{d}}" },
  {
    name: "fraction-nested-display",
    source: "\\frac{\\frac{a}{b}}{\\frac{c}{d}}",
    options: { display: true },
  },
  { name: "scripts-sub-then-super", source: "x_1^2" },
  { name: "scripts-super-then-sub", source: "x^2_1" },
  { name: "scripts-repeated-super", source: "x^2^3" },
  { name: "scripts-repeated-sub", source: "x_1_2" },
  { name: "scripts-empty", source: "x^{}_{}" },
  { name: "operator-super-then-sub", source: "\\sum^n_{i=0}x", options: { display: true } },
  { name: "operator-sub-then-super", source: "\\sum_{i=0}^n x", options: { display: true } },
  { name: "pmatrix-one-row", source: "\\begin{pmatrix}a & bb\\end{pmatrix}" },
  {
    name: "cases-two-row",
    source: "\\begin{cases}x & x>0 \\\\ -x & x\\leq0\\end{cases}",
  },
  {
    name: "cases-three-row",
    source: "\\begin{cases}a & p \\\\ b & q \\\\ c & r\\end{cases}",
  },
  { name: "line-break-compact", source: "a\\\\b" },
  { name: "line-break-spaced", source: "a \\\\ b" },
  { name: "raw-combining-acute", source: "x\u0301 + y" },
  { name: "raw-combining-alone", source: "\u0301" },
  { name: "escaped-literals", source: "\\{x\\} \\_ \\% \\# \\&" },
  { name: "unknown-bare", source: "x+\\definitelyUnsupported" },
  { name: "malformed-left", source: "\\left( x" },
  {
    name: "matrix-nested-fraction",
    source: "\\begin{bmatrix}\\frac{a}{b} & x \\\\ y & \\sqrt{z}\\end{bmatrix}",
  },
  { name: "spacing-pm-div", source: "a\\pm b\\div c" },
  {
    name: "quantifiers-long-arrow",
    source: "\\forall x \\exists y x\\longrightarrow y",
  },
  { name: "bar-combining-overline", source: "\\bar{x}" },
  { name: "root-structural-empty", source: "\\sqrt{}" },
  { name: "fraction-multichar", source: "\\frac{ab}{cd}" },
  { name: "fraction-spaced-numerator", source: "\\frac{a b}{cd}" },
  { name: "fraction-spaced-denominator", source: "\\frac{ab}{c d}" },
  { name: "fraction-unbraced-after-space", source: "\\frac ab" },
  { name: "nested-script-expression", source: "y_{a^b}" },
];

const renderCases = (inputCases) => inputCases.map(({ name, source, options }) => {
  const output = renderLatex(source, options);
  return { name, source, ...(options ? { options } : {}), output: output ?? null };
});
const results = renderCases(cases);
const adversarialResults = renderCases(adversarialCases);

const reference = {
  package: "@earendil-works/pi-tui",
  version: packageJson.version,
  indexDtsSha256,
  latexDtsSha256,
  latexJsSha256,
};

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-core/tests");
writeFileSync(
  join(outDir, "fixtures/m4-latex.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-latex.mjs",
      reference,
      cases: results,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-latex-adversarial.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-latex.mjs",
      reference,
      cases: adversarialResults,
    },
    null,
    1,
  )}\n`,
);
console.log(
  `harvested ${results.length + adversarialResults.length} renderLatex cases (${adversarialResults.length} adversarial) from pi-tui ${packageJson.version}`,
);
