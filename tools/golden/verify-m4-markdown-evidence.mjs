// Verify the independent M4 Markdown evidence packet against the exact
// pi-tui and marked runtime pair. This script never rewrites the packet.
import { createHash } from "node:crypto";
import { readFileSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const EXPECTED_PACKET_SHA256 =
  "46c35adecf350e79f8c37163c2d88b38a32672da03d30c21066e5149f23468d5";
const EXPECTED_PI_TUI_VERSION = "0.84.1";
const EXPECTED_MARKDOWN_JS_SHA256 =
  "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465";
const EXPECTED_LATEX_JS_SHA256 =
  "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716";
const EXPECTED_TERMINAL_IMAGE_JS_SHA256 =
  "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2";
const EXPECTED_UTILS_JS_SHA256 =
  "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052";
const EXPECTED_MARKED_VERSION = "18.0.5";
const EXPECTED_MARKED_ESM_SHA256 =
  "43e1fc0927b2d397bdc786c0a9efa8414ce18e7781d0b3490faceea35b7d0d15";
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

const dist = process.env.PI_TUI_DIST;
const markedRoot = process.env.MARKED_ROOT;
if (!dist || !markedRoot) {
  console.error("PI_TUI_DIST and MARKED_ROOT are required");
  process.exit(64);
}
const packetPath = resolve(
  process.argv[2] ??
    "crates/pie-components/tests/fixtures/m4-markdown-evidence-packet.json",
);
const sha256 = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
const packet = JSON.parse(readFileSync(packetPath, "utf8"));
const markdownPath = join(dist, "components", "markdown.js");
const latexPath = join(dist, "latex.js");
const terminalImagePath = join(dist, "terminal-image.js");
const utilsPath = join(dist, "utils.js");
const markedPath = join(markedRoot, "lib", "marked.esm.js");
const piPackage = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const markedPackage = JSON.parse(readFileSync(join(markedRoot, "package.json"), "utf8"));
const runtimeEastAsianWidth = createRequire(pathToFileURL(utilsPath)).resolve(
  "get-east-asian-width",
);
const eastAsianWidthRoot = dirname(runtimeEastAsianWidth);
const eastAsianWidthPackage = JSON.parse(
  readFileSync(join(eastAsianWidthRoot, "package.json"), "utf8"),
);

for (const [label, actual, expected] of [
  ["packet sha256", sha256(packetPath), EXPECTED_PACKET_SHA256],
  ["pi-tui version", piPackage.version, EXPECTED_PI_TUI_VERSION],
  ["markdown.js sha256", sha256(markdownPath), EXPECTED_MARKDOWN_JS_SHA256],
  ["latex.js sha256", sha256(latexPath), EXPECTED_LATEX_JS_SHA256],
  [
    "terminal-image.js sha256",
    sha256(terminalImagePath),
    EXPECTED_TERMINAL_IMAGE_JS_SHA256,
  ],
  ["utils.js sha256", sha256(utilsPath), EXPECTED_UTILS_JS_SHA256],
  ["marked version", markedPackage.version, EXPECTED_MARKED_VERSION],
  ["marked ESM sha256", sha256(markedPath), EXPECTED_MARKED_ESM_SHA256],
  [
    "get-east-asian-width version",
    eastAsianWidthPackage.version,
    EXPECTED_EAST_ASIAN_WIDTH_VERSION,
  ],
  [
    "get-east-asian-width package sha256",
    sha256(join(eastAsianWidthRoot, "package.json")),
    EXPECTED_EAST_ASIAN_WIDTH_PACKAGE_SHA256,
  ],
  [
    "get-east-asian-width index sha256",
    sha256(join(eastAsianWidthRoot, "index.js")),
    EXPECTED_EAST_ASIAN_WIDTH_INDEX_SHA256,
  ],
  [
    "get-east-asian-width lookup sha256",
    sha256(join(eastAsianWidthRoot, "lookup.js")),
    EXPECTED_EAST_ASIAN_WIDTH_LOOKUP_SHA256,
  ],
  [
    "get-east-asian-width data sha256",
    sha256(join(eastAsianWidthRoot, "lookup-data.js")),
    EXPECTED_EAST_ASIAN_WIDTH_DATA_SHA256,
  ],
  [
    "get-east-asian-width utilities sha256",
    sha256(join(eastAsianWidthRoot, "utilities.js")),
    EXPECTED_EAST_ASIAN_WIDTH_UTILITIES_SHA256,
  ],
  ["Node version", process.versions.node, "24.4.1"],
  ["ICU version", process.versions.icu, "77.1"],
  ["Unicode version", process.versions.unicode, "16.0"],
]) {
  if (actual !== expected) throw new Error(`unexpected ${label}: ${actual}`);
}
const runtimeMarked = createRequire(pathToFileURL(markdownPath)).resolve("marked");
if (realpathSync(runtimeMarked) !== realpathSync(markedPath)) {
  throw new Error("pi-tui Markdown did not resolve marked from MARKED_ROOT");
}
const { Markdown } = await import(pathToFileURL(markdownPath));

const identityTheme = () => ({
  heading: (value) => value,
  link: (value) => value,
  linkUrl: (value) => value,
  code: (value) => value,
  codeBlock: (value) => value,
  codeBlockBorder: (value) => value,
  quote: (value) => value,
  quoteBorder: (value) => value,
  hr: (value) => value,
  listBullet: (value) => value,
  bold: (value) => value,
  italic: (value) => value,
  strikethrough: (value) => value,
  underline: (value) => value,
});
const encode = (value) =>
  value.replaceAll("\0", "<NUL>").replaceAll("\n", "<LF>").replaceAll("\t", "<TAB>");
const logged = (events, name) => (value) => {
  events.push(`${name}:${encode(value)}`);
  return value;
};

const runOperations = (component, source, operations, events) => {
  const outputs = [];
  for (const operation of operations) {
    switch (operation.op) {
      case "construct":
        break;
      case "pushEvent":
        events.push(operation.event);
        break;
      case "render":
        outputs.push(component.render(operation.width));
        break;
      case "invalidate":
        component.invalidate();
        break;
      case "setText":
        component.setText(operation.value === "same source" ? source : operation.value);
        break;
      default:
        throw new Error(`unsupported operation: ${operation.op}`);
    }
  }
  return outputs;
};

const failures = [];
let checked = 0;
for (const row of [
  ...packet.priorThreeOfThirtyTwo,
  ...packet.additionalTenOfFifteen,
  { name: "default-options-marker", ...packet.defaultOptionsMutationGuard },
]) {
  const events = [];
  const component = new Markdown(row.source, row.paddingX, row.paddingY, identityTheme());
  const outputs = runOperations(component, row.source, row.operations, events);
  if (JSON.stringify(outputs) !== JSON.stringify(row.expectedOutputs)) failures.push(row.name);
  checked += 1;
}

{
  const row = packet.constructionTransformCallbackLifecycle;
  const events = [];
  const theme = {
    heading: logged(events, "heading"),
    link: logged(events, "link"),
    linkUrl: logged(events, "linkUrl"),
    code: logged(events, "code"),
    codeBlock: logged(events, "codeBlock"),
    codeBlockBorder: logged(events, "border"),
    quote: logged(events, "quote"),
    quoteBorder: logged(events, "quoteBorder"),
    hr: logged(events, "hr"),
    listBullet: logged(events, "bullet"),
    bold: logged(events, "bold"),
    italic: logged(events, "italic"),
    strikethrough: logged(events, "strike"),
    underline: logged(events, "underline"),
  };
  const defaultTextStyle = {
    color: logged(events, "color"),
    bgColor: logged(events, "bg"),
    bold: true,
    italic: true,
    strikethrough: true,
    underline: true,
  };
  const component = new Markdown(
    row.source,
    row.paddingX,
    row.paddingY,
    theme,
    defaultTextStyle,
    {
      transform: (source, width) => {
        events.push(`transform:${width}:${source}`);
        return source;
      },
    },
  );
  const outputs = runOperations(component, row.source, row.operations, events);
  if (JSON.stringify(outputs) !== JSON.stringify(row.expectedOutputs)) {
    failures.push("callback-cache-lifecycle outputs");
  }
  if (JSON.stringify(events) !== JSON.stringify(row.expectedEvents)) {
    failures.push("callback-cache-lifecycle events");
  }
  checked += 1;
}

if (checked !== 15 || failures.length !== 0) {
  throw new Error(`Markdown packet replay failed (${checked}/15): ${failures.join(", ")}`);
}
console.log(
  `verified 15 Markdown evidence cases, 5 lifecycle outputs, and 231 events against pi-tui ${piPackage.version} / marked ${markedPackage.version}`,
);
