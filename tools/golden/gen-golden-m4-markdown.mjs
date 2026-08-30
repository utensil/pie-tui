// gen-golden-m4-markdown.mjs — harvest Markdown black-box behavior from the
// exact pi-tui + marked distributions. Public declaration/runtime hashes are
// checked before execution so fixture provenance is deterministic.
import { createHash } from "node:crypto";
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXPECTED_PI_TUI_VERSION = "0.84.1";
const EXPECTED_INDEX_DTS_SHA256 = "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3";
const EXPECTED_MARKDOWN_DTS_SHA256 = "9c21b4bcb0b0b047438616cb2302b6b6df7630e73e0ee8da3f9f6bae7e565f66";
const EXPECTED_MARKDOWN_JS_SHA256 = "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465";
const EXPECTED_LATEX_DTS_SHA256 =
  "76a3bda961e678e859bf8749d68b40a4ce20a08a701329e92758dedda79812f8";
const EXPECTED_LATEX_JS_SHA256 =
  "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716";
const EXPECTED_TERMINAL_IMAGE_DTS_SHA256 =
  "ba498675c6f16339fe04c329dcd95757743f0f6d22a18879b2fda6e9e8b4d8ec";
const EXPECTED_TERMINAL_IMAGE_JS_SHA256 =
  "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2";
const EXPECTED_UTILS_DTS_SHA256 =
  "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2";
const EXPECTED_UTILS_JS_SHA256 =
  "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052";
const EXPECTED_MARKED_VERSION = "18.0.5";
const EXPECTED_MARKED_ESM_SHA256 = "43e1fc0927b2d397bdc786c0a9efa8414ce18e7781d0b3490faceea35b7d0d15";
const EXPECTED_EAST_ASIAN_WIDTH_VERSION = "1.6.0";
const EXPECTED_EAST_ASIAN_WIDTH_PACKAGE_SHA256 =
  "d263e50dd1a43aee9acda4d7f066e66b0d0bde1f2852ea6e7153750a5e3a3e52";
const EXPECTED_EAST_ASIAN_WIDTH_INDEX_SHA256 =
  "d7b1ba05914c0fc311c20e5618bf8d0893c9c74078a07975e2df981445e64887";
const EXPECTED_EAST_ASIAN_WIDTH_LOOKUP_SHA256 =
  "c80ecc22b120b27ef5ea9facb7000b8fd4ec037a84d9231d215f1c44bc9c21d0";
const EXPECTED_EAST_ASIAN_WIDTH_DATA_SHA256 =
  "f6b40f86c9a2a6808ec808fa8ddcb8da261254cc6121d37ffaeb2bf35dad1d5b";
const EXPECTED_EAST_ASIAN_WIDTH_UTILITIES_SHA256 =
  "4b08a7e9e3ffacbcf198a6abceb2338d52ac671899e52ccc2851c898bfccac42";
const EXPECTED_NODE_VERSION = "24.4.1";
const EXPECTED_ICU_VERSION = "77.1";
const EXPECTED_UNICODE_VERSION = "16.0";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
const markedRoot = process.env.MARKED_ROOT;
if (!markedRoot) {
  console.error("MARKED_ROOT not set (must name the independently pinned marked package root)");
  process.exit(64);
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const piPackage = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const markedPackage = JSON.parse(readFileSync(join(markedRoot, "package.json"), "utf8"));
const markdownModule = join(dist, "components", "markdown.js");
const runtimeMarked = createRequire(pathToFileURL(markdownModule)).resolve("marked");
const pinnedMarked = join(markedRoot, "lib", "marked.esm.js");
const runtimeEastAsianWidth = createRequire(pathToFileURL(join(dist, "utils.js"))).resolve(
  "get-east-asian-width",
);
const eastAsianWidthRoot = dirname(runtimeEastAsianWidth);
const eastAsianWidthPackage = JSON.parse(
  readFileSync(join(eastAsianWidthRoot, "package.json"), "utf8"),
);
const provenance = {
  package: "@earendil-works/pi-tui",
  version: piPackage.version,
  indexDtsSha256: sha256(join(dist, "index.d.ts")),
  markdownDtsSha256: sha256(join(dist, "components", "markdown.d.ts")),
  markdownJsSha256: sha256(join(dist, "components", "markdown.js")),
  latexDtsSha256: sha256(join(dist, "latex.d.ts")),
  latexJsSha256: sha256(join(dist, "latex.js")),
  terminalImageDtsSha256: sha256(join(dist, "terminal-image.d.ts")),
  terminalImageJsSha256: sha256(join(dist, "terminal-image.js")),
  utilsDtsSha256: sha256(join(dist, "utils.d.ts")),
  utilsJsSha256: sha256(join(dist, "utils.js")),
  markedVersion: markedPackage.version,
  markedEsmSha256: sha256(join(markedRoot, "lib", "marked.esm.js")),
  eastAsianWidthVersion: eastAsianWidthPackage.version,
  eastAsianWidthPackageSha256: sha256(join(eastAsianWidthRoot, "package.json")),
  eastAsianWidthIndexSha256: sha256(join(eastAsianWidthRoot, "index.js")),
  eastAsianWidthLookupSha256: sha256(join(eastAsianWidthRoot, "lookup.js")),
  eastAsianWidthDataSha256: sha256(join(eastAsianWidthRoot, "lookup-data.js")),
  eastAsianWidthUtilitiesSha256: sha256(join(eastAsianWidthRoot, "utilities.js")),
  node: process.versions.node,
  icu: process.versions.icu,
  unicode: process.versions.unicode,
};

for (const [field, actual, expected] of [
  ["pi-tui version", provenance.version, EXPECTED_PI_TUI_VERSION],
  ["index.d.ts sha256", provenance.indexDtsSha256, EXPECTED_INDEX_DTS_SHA256],
  ["markdown.d.ts sha256", provenance.markdownDtsSha256, EXPECTED_MARKDOWN_DTS_SHA256],
  ["markdown.js sha256", provenance.markdownJsSha256, EXPECTED_MARKDOWN_JS_SHA256],
  ["latex.d.ts sha256", provenance.latexDtsSha256, EXPECTED_LATEX_DTS_SHA256],
  ["latex.js sha256", provenance.latexJsSha256, EXPECTED_LATEX_JS_SHA256],
  [
    "terminal-image.d.ts sha256",
    provenance.terminalImageDtsSha256,
    EXPECTED_TERMINAL_IMAGE_DTS_SHA256,
  ],
  [
    "terminal-image.js sha256",
    provenance.terminalImageJsSha256,
    EXPECTED_TERMINAL_IMAGE_JS_SHA256,
  ],
  ["utils.d.ts sha256", provenance.utilsDtsSha256, EXPECTED_UTILS_DTS_SHA256],
  ["utils.js sha256", provenance.utilsJsSha256, EXPECTED_UTILS_JS_SHA256],
  ["marked version", provenance.markedVersion, EXPECTED_MARKED_VERSION],
  ["marked ESM sha256", provenance.markedEsmSha256, EXPECTED_MARKED_ESM_SHA256],
  [
    "get-east-asian-width version",
    provenance.eastAsianWidthVersion,
    EXPECTED_EAST_ASIAN_WIDTH_VERSION,
  ],
  [
    "get-east-asian-width package sha256",
    provenance.eastAsianWidthPackageSha256,
    EXPECTED_EAST_ASIAN_WIDTH_PACKAGE_SHA256,
  ],
  [
    "get-east-asian-width index sha256",
    provenance.eastAsianWidthIndexSha256,
    EXPECTED_EAST_ASIAN_WIDTH_INDEX_SHA256,
  ],
  [
    "get-east-asian-width lookup sha256",
    provenance.eastAsianWidthLookupSha256,
    EXPECTED_EAST_ASIAN_WIDTH_LOOKUP_SHA256,
  ],
  [
    "get-east-asian-width data sha256",
    provenance.eastAsianWidthDataSha256,
    EXPECTED_EAST_ASIAN_WIDTH_DATA_SHA256,
  ],
  [
    "get-east-asian-width utilities sha256",
    provenance.eastAsianWidthUtilitiesSha256,
    EXPECTED_EAST_ASIAN_WIDTH_UTILITIES_SHA256,
  ],
  ["Node version", provenance.node, EXPECTED_NODE_VERSION],
  ["ICU version", provenance.icu, EXPECTED_ICU_VERSION],
  ["Unicode version", provenance.unicode, EXPECTED_UNICODE_VERSION],
]) {
  if (actual !== expected) throw new Error(`unexpected ${field}: ${actual}`);
}

if (realpathSync(runtimeMarked) !== realpathSync(pinnedMarked)) {
  throw new Error("pi-tui Markdown runtime did not resolve marked from MARKED_ROOT");
}

const { Markdown } = await import(pathToFileURL(markdownModule));

const identityTheme = () => ({
  heading: (text) => text,
  link: (text) => text,
  linkUrl: (text) => text,
  code: (text) => text,
  codeBlock: (text) => text,
  codeBlockBorder: (text) => text,
  quote: (text) => text,
  quoteBorder: (text) => text,
  hr: (text) => text,
  listBullet: (text) => text,
  bold: (text) => text,
  italic: (text) => text,
  strikethrough: (text) => text,
  underline: (text) => text,
});

const ansi = (open, close) => (text) => `\x1b[${open}m${text}\x1b[${close}m`;
const styledTheme = (events = []) => ({
  heading: ansi(31, 39),
  link: ansi(36, 39),
  linkUrl: ansi(34, 39),
  code: ansi(33, 39),
  codeBlock: ansi(35, 39),
  codeBlockBorder: ansi(2, 22),
  quote: ansi(32, 39),
  quoteBorder: ansi(92, 39),
  hr: ansi(90, 39),
  listBullet: ansi(95, 39),
  bold: ansi(1, 22),
  italic: ansi(3, 23),
  strikethrough: ansi(9, 29),
  underline: ansi(4, 24),
  codeBlockIndent: "» ",
  highlightCode: (code, language) => {
    events.push(`highlight:${language ?? ""}:${code}`);
    return code.split("\n").map((line) => `\x1b[96m${line}\x1b[39m`);
  },
});

const encodeCallbackText = (text) =>
  text
    .replaceAll("\0", "<NUL>")
    .replaceAll("\n", "<LF>")
    .replaceAll("\t", "<TAB>")
    .replaceAll("\x1b", "<ESC>");

const loggedAnsi = (events, name, open, close) => (text) => {
  events.push(`${name}:${encodeCallbackText(text)}`);
  return `\x1b[${open}m${text}\x1b[${close}m`;
};

const mathStyleTheme = (events) => {
  const theme = identityTheme();
  theme.bold = loggedAnsi(events, "bold", 1, 22);
  return theme;
};

const cases = [
  {
    name: "empty-default",
    build: () => new Markdown("", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(20)] }),
  },
  {
    name: "paragraph-wrap-resize",
    build: () => new Markdown("Alpha beta gamma delta epsilon.\n\nSecond paragraph.", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(18), component.render(28)] }),
  },
  {
    name: "headings-inline-link",
    build: () =>
      new Markdown(
        "# Heading one\n\nText with **bold**, *italic*, ~~strike~~, `code`, and [link](https://example.test).",
        0,
        0,
        styledTheme(),
      ),
    script: (component) => ({ outputs: [component.render(72)] }),
  },
  {
    name: "quote-rule",
    build: () => new Markdown("> quoted text that wraps across rows\n> second line\n\n---", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "nested-unordered-list",
    build: () => new Markdown("- first item\n  - nested item\n- final item with wrapping words", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(22)] }),
  },
  {
    name: "ordered-normalized",
    build: () => new Markdown("3. third\n7. seventh", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "ordered-preserved",
    build: () =>
      new Markdown("3. third\n7. seventh", 0, 0, identityTheme(), undefined, {
        preserveOrderedListMarkers: true,
      }),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "backslash-escape-default",
    build: () => new Markdown("Escaped \\*star\\* and \\[bracket\\].", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(40)] }),
  },
  {
    name: "backslash-escape-preserved",
    build: () =>
      new Markdown("Escaped \\*star\\* and \\[bracket\\].", 0, 0, identityTheme(), undefined, {
        preserveBackslashEscapes: true,
      }),
    script: (component) => ({ outputs: [component.render(40)] }),
  },
  {
    name: "fenced-code-default-indent",
    build: () => new Markdown("```\nraw code\n```", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(20)] }),
  },
  {
    name: "fenced-code-highlight",
    build: (events) => new Markdown("```js\nconst x = 1;\nconsole.log(x);\n```", 0, 0, styledTheme(events)),
    script: (component, events) => ({ outputs: [component.render(36)], events }),
  },
  {
    name: "table-wide-narrow",
    build: () =>
      new Markdown(
        "| Name | Description |\n| --- | --- |\n| alpha | a description with wrapping words |\n| beta | short |",
        0,
        0,
        identityTheme(),
      ),
    script: (component) => ({ outputs: [component.render(46), component.render(25)] }),
  },
  {
    name: "latex-default",
    build: () => new Markdown("Inline $x_{i+1}^2$ and display:\n\n$$\\frac{a+b}{c-d}$$", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(38)] }),
  },
  {
    name: "latex-disabled",
    build: () =>
      new Markdown("Inline $x_{i+1}^2$ and $$\\frac{a}{b}$$.", 0, 0, identityTheme(), undefined, {
        renderLatex: false,
      }),
    script: (component) => ({ outputs: [component.render(50)] }),
  },
  {
    name: "padding-default-style-background",
    build: () =>
      new Markdown("Styled **body**.", 2, 1, styledTheme(), {
        color: ansi(37, 39),
        bgColor: ansi(44, 49),
        bold: true,
        italic: true,
        strikethrough: true,
        underline: true,
      }),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "transform-cache-invalidate-order",
    build: (events) => {
      const theme = styledTheme(events);
      return new Markdown("```txt\nseed\n```", 1, 0, theme, undefined, {
        transform: (markdown, availableWidth) => {
          events.push(`transform:${availableWidth}:${markdown}`);
          return markdown;
        },
      });
    },
    script: (component, events) => {
      const outputs = [component.render(20), component.render(20), component.render(16)];
      component.invalidate();
      outputs.push(component.render(16));
      component.setText("plain replacement");
      outputs.push(component.render(16));
      return { outputs, events };
    },
  },
];

const adversarialCases = [
  {
    name: "tabs-paragraph",
    build: () => new Markdown("alpha\tbeta\tomega", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(30)] }),
  },
  {
    name: "tabs-list",
    build: () => new Markdown("-\talpha\tbeta\n-\tomega", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(30)] }),
  },
  {
    name: "tabs-table",
    build: () =>
      new Markdown("| Key\t| Value |\n| --- | --- |\n| a\tb | c\td |", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(32)] }),
  },
  {
    name: "tabs-fence",
    build: () => new Markdown("```txt\n\talpha\tbeta\n```", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "tabs-after-transform",
    build: (events) =>
      new Markdown("seed", 0, 0, identityTheme(), undefined, {
        transform: (_markdown, width) => {
          events.push(`transform:${width}`);
          return "alpha\tbeta";
        },
      }),
    script: (component, events) => ({ outputs: [component.render(20)], events }),
  },
  {
    name: "setext-headings",
    build: () => new Markdown("Primary\n=======\n\nSecondary\n---------", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "underscore-emphasis",
    build: () => new Markdown("__bold__ _italic_ intraword_a_b", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(44)] }),
  },
  {
    name: "autolinks-url-email",
    build: () => new Markdown("<https://example.test> <user@example.test>", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(72)] }),
  },
  {
    name: "link-title",
    build: () =>
      new Markdown('[label](https://example.test "Example title")', 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(60)] }),
  },
  {
    name: "image-alt-title",
    build: () => new Markdown('![diagram](image.png "Caption")', 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(50)] }),
  },
  {
    name: "code-span-delimiters",
    build: () => new Markdown("` x ` and ``a`b`` and `a  b`", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(48)] }),
  },
  {
    name: "soft-break",
    build: () => new Markdown("soft line\nnext line", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "hard-break-spaces",
    build: () => new Markdown("hard line  \nnext line", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "hard-break-backslash",
    build: () => new Markdown("slash line\\\nnext line", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(24)] }),
  },
  {
    name: "indented-code",
    build: () => new Markdown("    alpha\n    beta", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "long-fence",
    build: () => new Markdown("````js\nconst ticks = ```;\n````", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(36)] }),
  },
  {
    name: "nested-mixed-lists",
    build: () =>
      new Markdown("1. outer\n   - inner\n     4. deep\n2. tail", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "nested-quotes",
    build: () => new Markdown("> outer\n>> inner\n> tail", 0, 0, styledTheme()),
    script: (component) => ({ outputs: [component.render(28)] }),
  },
  {
    name: "escaped-pipe-table",
    build: () =>
      new Markdown("| Key | Value |\n| --- | --- |\n| a\\|b | `c|d` |", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(30)] }),
  },
  {
    name: "multiline-display-math",
    build: () =>
      new Markdown("before\n\n$$\n\\frac{a+b}{c-d}\n+ \\sum_{i=1}^{n} i\n$$\n\nafter", 0, 0, identityTheme()),
    script: (component) => ({ outputs: [component.render(36)] }),
  },
  {
    name: "inline-code-theme-only",
    build: () => {
      const theme = identityTheme();
      theme.code = ansi(33, 39);
      return new Markdown("plain `code` tail", 0, 0, theme, { color: ansi(37, 39) });
    },
    script: (component) => ({ outputs: [component.render(32)] }),
  },
  {
    name: "code-callback-order",
    build: (events) => {
      const theme = identityTheme();
      theme.codeBlockBorder = loggedAnsi(events, "border", 2, 22);
      theme.highlightCode = (code, language) => {
        events.push(`highlight:${language ?? ""}:${encodeCallbackText(code)}`);
        return code.split("\n");
      };
      return new Markdown("```js\none\ntwo\n```", 0, 0, theme);
    },
    script: (component, events) => ({ outputs: [component.render(24)], events }),
  },
  {
    name: "background-callback-order",
    build: (events) =>
      new Markdown("one\n\ntwo", 1, 1, identityTheme(), {
        bgColor: loggedAnsi(events, "background", 44, 49),
      }),
    script: (component, events) => ({ outputs: [component.render(12)], events }),
  },
  {
    name: "default-prefix-nul-cache",
    build: (events) => {
      const theme = identityTheme();
      theme.bold = loggedAnsi(events, "bold", 1, 22);
      return new Markdown("a **b** c", 0, 0, theme, {
        color: loggedAnsi(events, "color", 37, 39),
        bold: true,
      });
    },
    script: (component, events) => ({
      outputs: [component.render(24), component.render(24), component.render(20)],
      events,
    }),
  },
];

// marked accepts non-1 ordered markers at a different continuation boundary
// from CommonMark. Keep the whole boundary and marker-width cross product as
// a black-box compatibility matrix instead of pinning a single source string.
const indentationCases = [3, 4, 5, 6, 7, 8, 9].flatMap((indent) =>
  ["4", "12", "321"].map((marker) => {
    const source = `1. outer\n   - inner\n${" ".repeat(indent)}${marker}. deep\n2. tail`;
    return {
      name: `indent-${indent}-marker-${marker}`,
      build: () => new Markdown(source, 0, 0, identityTheme()),
      script: (component) => ({
        indent,
        marker,
        source,
        outputs: [component.render(40)],
      }),
    };
  }),
);

const finalReviewCases = [
  {
    name: "task-markers",
    source: "- [x] done\n- [ ] todo",
    width: 30,
  },
  {
    name: "inline-math-malformed-fraction",
    source: "before $\\frac{a}$ after",
    width: 40,
  },
  {
    name: "inline-math-unclosed-group",
    source: "before $x + {y$ after",
    width: 40,
  },
  {
    name: "inline-math-unsupported",
    source: "before $\\definitelyUnsupported{x}$ after",
    width: 50,
  },
  {
    name: "entities-raw",
    source: "&amp; &lt; &#x1F600;",
    width: 40,
  },
  {
    name: "inline-html-entities-raw",
    source: "<em>a &amp; b</em>",
    width: 40,
  },
  {
    name: "unicode-before-sum",
    source: "$$\\frac{a}{b} + π \\sum_{i=1}^{n} i$$",
    width: 50,
  },
  {
    name: "unicode-before-sum-no-panic",
    source: "$$\\frac{a}{b} + 🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂 \\sum x$$",
    width: 80,
  },
  {
    name: "unicode-wide-sum-boundaries",
    source: "$$\\frac{α}{β} + 文 \\sum_{é=1}^{終} 🙂$$",
    width: 60,
  },
  {
    name: "unicode-inline-boundaries",
    source: "🙂$x_{é}^2$文",
    width: 40,
  },
].map(({ name, source, width }) => ({
  name,
  build: () => new Markdown(source, 0, 0, identityTheme()),
  script: (component) => ({ source, width, outputs: [component.render(width)] }),
}));

const mathStyleCases = [
  {
    name: "inline-math-supported-default-style",
    source: "$x_{i+1}^2$",
    width: 32,
  },
  {
    name: "inline-math-failed-default-style",
    source: "$\\frac{a}$",
    width: 32,
  },
  {
    name: "display-math-supported-default-style",
    source: "$$\\frac{a}{b}$$",
    width: 32,
  },
  {
    name: "display-math-failed-default-style",
    source: "$$\\frac{a}$$",
    width: 32,
  },
].map(({ name, source, width }) => ({
  name,
  build: (events) => {
    const transform = (markdown, availableWidth) => {
      events.push(`transform:${availableWidth}:${encodeCallbackText(markdown)}`);
      return markdown;
    };
    return new Markdown(
      source,
      0,
      0,
      mathStyleTheme(events),
      {
        color: loggedAnsi(events, "color", 37, 39),
        bold: true,
      },
      { transform },
    );
  },
  script: (component, events) => ({
    source,
    width,
    outputs: [component.render(width)],
    events,
  }),
}));

// These routes deliberately sit outside the former Markdown-local math
// subset. They prove that the component delegates to the canonical LaTeX
// model instead of maintaining a second parser with drifting semantics.
const coreLatexCases = [
  {
    name: "inline-unbraced-accent",
    source: "before $\\bar x$ after",
    width: 48,
  },
  {
    name: "inline-symbols-and-quantifiers",
    source: "before $a\\pm b\\cdot c\\div d \\longrightarrow \\forall x \\exists y$ after",
    width: 80,
  },
  {
    name: "inline-decimal-root",
    source: "before $\\sqrt{1.2}$ after",
    width: 48,
  },
  {
    name: "inline-duplicate-operator-script-fallback",
    source: "before $\\sum_1_2$ after",
    width: 48,
  },
  {
    name: "inline-astral-fraction-grouping",
    source: "before $\\frac{😀}{x}$ after",
    width: 48,
  },
  {
    name: "inline-combining-fraction-grouping",
    source: "before $\\frac{é}{12}$ after",
    width: 48,
  },
  {
    name: "display-empty-denominator",
    source: "$$\\frac{x}{}$$",
    width: 24,
  },
].map(({ name, source, width }) => ({
  name,
  build: () => new Markdown(source, 0, 0, identityTheme()),
  script: (component) => ({ source, width, outputs: [component.render(width)] }),
}));

const runCases = (inputCases) => inputCases.map(({ name, build, script }) => {
  const events = [];
  const component = build(events);
  return { name, ...script(component, events) };
});
const results = runCases(cases);
const adversarialResults = runCases(adversarialCases);
const indentationResults = runCases(indentationCases);
const finalReviewResults = runCases(finalReviewCases);
const mathStyleResults = runCases(mathStyleCases);
const coreLatexResults = runCases(coreLatexCases);

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-components/tests");
writeFileSync(
  join(outDir, "fixtures/m4-markdown.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: results,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-markdown-adversarial.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: adversarialResults,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-markdown-indent-matrix.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: indentationResults,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-markdown-final-review.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: finalReviewResults,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-markdown-math-style.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: mathStyleResults,
    },
    null,
    1,
  )}\n`,
);
writeFileSync(
  join(outDir, "fixtures/m4-markdown-core-latex.json"),
  `${JSON.stringify(
    {
      generator: "gen-golden-m4-markdown.mjs",
      reference: provenance,
      cases: coreLatexResults,
    },
    null,
    1,
  )}\n`,
);
console.log(
  `harvested ${results.length + adversarialResults.length} Markdown cases (${adversarialResults.length} adversarial), ${indentationResults.length} indentation-matrix cases, ${finalReviewResults.length} final-review cases, ${mathStyleResults.length} math-style cases, and ${coreLatexResults.length} core-LaTeX routes from pi-tui ${piPackage.version} / marked ${markedPackage.version}`,
);
