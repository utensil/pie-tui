#!/usr/bin/env node
// Harvest the Image component from the exact pinned pi-tui distribution.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-image-component.mjs
//
// Capability, cell-size, home-directory, and random-ID inputs are fixed before
// every scenario. The fixture records source digests, never the oracle path.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

process.env.HOME = "/opt/pi-home";
delete process.env.TMUX;
delete process.env.KITTY_WINDOW_ID;
delete process.env.GHOSTTY_RESOURCES_DIR;
delete process.env.WEZTERM_PANE;
delete process.env.WARP_SESSION_ID;
delete process.env.WARP_TERMINAL_SESSION_UUID;
delete process.env.ITERM_SESSION_ID;
delete process.env.WT_SESSION;
process.env.TERM = "dumb";
process.env.TERM_PROGRAM = "";
process.env.TERMINAL_EMULATOR = "";
process.env.COLORTERM = "";

const sourcePaths = {
  "components/image.d.ts": join(dist, "components", "image.d.ts"),
  "components/image.js": join(dist, "components", "image.js"),
  "terminal-image.d.ts": join(dist, "terminal-image.d.ts"),
  "terminal-image.js": join(dist, "terminal-image.js"),
  "utils.d.ts": join(dist, "utils.d.ts"),
  "utils.js": join(dist, "utils.js"),
};
const expectedDigests = {
  "components/image.d.ts": "45cfb14d766704c70017d7ec3a2d382f148fbf56b7f76c4c3155cc80bb5ff6cb",
  "components/image.js": "dd6791e17fbeb0a48c2b73d521d31356edf11795e44e0fae05b5f8c322c470e1",
  "terminal-image.d.ts": "ba498675c6f16339fe04c329dcd95757743f0f6d22a18879b2fda6e9e8b4d8ec",
  "terminal-image.js": "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2",
  "utils.d.ts": "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
  "utils.js": "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
};
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const files = Object.fromEntries(Object.entries(sourcePaths).map(([name, path]) => [name, digest(path)]));
for (const [name, expected] of Object.entries(expectedDigests)) {
  if (files[name] !== expected) {
    throw new Error(`${name} digest mismatch: expected ${expected}, got ${files[name]}`);
  }
}

const terminalImage = await import(pathToFileURL(sourcePaths["terminal-image.js"]).href);
const { Image } = await import(pathToFileURL(sourcePaths["components/image.js"]).href);
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));

const theme = { fallbackColor: (value) => `\x1b[35m${value}\x1b[0m` };
const cells = (widthPx, heightPx) => ({ widthPx, heightPx });
const dimensions = (widthPx, heightPx) => ({ widthPx, heightPx });

function setFacts(images, cellDimensions = cells(9, 18), hyperlinks = false) {
  terminalImage.setCapabilities({ images, trueColor: true, hyperlinks });
  terminalImage.setCellDimensions(cellDimensions);
}

function png(width, height) {
  const buffer = Buffer.alloc(24);
  Buffer.from([0x89, 0x50, 0x4e, 0x47]).copy(buffer);
  buffer.writeUInt32BE(width, 16);
  buffer.writeUInt32BE(height, 20);
  return buffer.toString("base64");
}

function gif(width, height) {
  const buffer = Buffer.alloc(10);
  buffer.write("GIF89a", 0, "ascii");
  buffer.writeUInt16LE(width, 6);
  buffer.writeUInt16LE(height, 8);
  return buffer.toString("base64");
}

function jpeg(width, height) {
  const buffer = Buffer.alloc(21);
  Buffer.from([0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08]).copy(buffer);
  buffer.writeUInt16BE(height, 7);
  buffer.writeUInt16BE(width, 9);
  return buffer.toString("base64");
}

function webp(width, height) {
  const buffer = Buffer.alloc(30);
  buffer.write("RIFF", 0, "ascii");
  buffer.write("WEBP", 8, "ascii");
  buffer.write("VP8X", 12, "ascii");
  const encodedWidth = width - 1;
  const encodedHeight = height - 1;
  buffer[24] = encodedWidth & 0xff;
  buffer[25] = (encodedWidth >> 8) & 0xff;
  buffer[26] = (encodedWidth >> 16) & 0xff;
  buffer[27] = encodedHeight & 0xff;
  buffer[28] = (encodedHeight >> 8) & 0xff;
  buffer[29] = (encodedHeight >> 16) & 0xff;
  return buffer.toString("base64");
}

function construct({ data, mime, options = {}, explicitDimensions }) {
  return new Image(data, mime, theme, options, explicitDimensions);
}

const originalRandom = Math.random;
let randomValues = [];
let randomCalls = 0;
Math.random = () => {
  randomCalls += 1;
  if (randomValues.length === 0) throw new Error("unexpected Math.random call");
  return randomValues.shift();
};
function setRandom(...values) {
  randomValues = [...values];
  randomCalls = 0;
}

const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files,
    fixedHome: process.env.HOME,
  },
  dimensionPriority: [],
  formats: [],
  kitty: {},
  iterm2: {},
  boundaries: [],
  fallback: {},
  cache: {},
  fallbackCache: {},
  providedId: {},
  providedZeroId: {},
  defaultLimits: {},
  hugeDimensions: {},
};

try {
  setFacts(null);
  const parsedPng = png(11, 13);
  for (const scenario of [
    {
      label: "explicit-beats-parsed",
      data: parsedPng,
      mime: "image/png",
      explicitDimensions: dimensions(100, 50),
    },
    { label: "parsed", data: parsedPng, mime: "image/png" },
    { label: "malformed-default", data: "not-base64!!", mime: "image/png" },
    { label: "unsupported-default", data: parsedPng, mime: "image/bmp" },
  ]) {
    const component = construct(scenario);
    out.dimensionPriority.push({
      label: scenario.label,
      data: scenario.data,
      mime: scenario.mime,
      explicitDimensions: scenario.explicitDimensions ?? null,
      dimensions: component.dimensions,
      lines: component.render(80),
    });
  }

  for (const [label, data, mime] of [
    ["png", png(17, 19), "image/png"],
    ["jpeg", jpeg(1024, 768), "image/jpeg"],
    ["gif", gif(320, 200), "image/gif"],
    ["webp", webp(70000, 50000), "image/webp"],
    ["malformed", "!!!!", "image/png"],
    ["unsupported", png(3, 4), "image/avif"],
  ]) {
    const component = construct({ data, mime });
    out.formats.push({ label, data, mime, dimensions: component.dimensions, lines: component.render(80) });
  }

  setFacts("kitty", cells(10, 20));
  setRandom(0.25);
  const kitty = construct({
    data: "QUJD",
    mime: "image/png",
    explicitDimensions: dimensions(100, 50),
  });
  const kittyWidth20 = kitty.render(20);
  const kittyWidth20Again = kitty.render(20);
  const kittyWidth8 = kitty.render(8);
  out.kitty = {
    width20: kittyWidth20,
    width20Again: kittyWidth20Again,
    width8: kittyWidth8,
    sameWidthSameReference: kittyWidth20 === kittyWidth20Again,
    widthMissNewReference: kittyWidth20 !== kittyWidth8,
    imageId: kitty.getImageId(),
    randomCalls,
  };

  setFacts("iterm2", cells(10, 20));
  const iterm = construct({
    data: "QUJD",
    mime: "image/png",
    options: { maxWidthCells: 5 },
    explicitDimensions: dimensions(100, 50),
  });
  out.iterm2 = { maxWidth5: iterm.render(20), imageId: iterm.getImageId() ?? null };

  for (const scenario of [
    { label: "width-zero", width: 0, options: { imageId: 77 } },
    { label: "width-one", width: 1, options: { imageId: 77 } },
    { label: "width-two", width: 2, options: { imageId: 77 } },
    { label: "width-three", width: 3, options: { imageId: 77 } },
    { label: "max-width-zero", width: 20, options: { maxWidthCells: 0, imageId: 77 } },
    { label: "max-width-negative", width: 20, options: { maxWidthCells: -5, imageId: 77 } },
    { label: "explicit-max-height-one", width: 20, options: { maxHeightCells: 1, imageId: 77 } },
    { label: "explicit-max-height-two", width: 20, options: { maxHeightCells: 2, imageId: 77 } },
  ]) {
    setFacts("kitty", cells(10, 20));
    const component = construct({
      data: "QUJD",
      mime: "image/png",
      options: scenario.options,
      explicitDimensions: dimensions(100, 50),
    });
    out.boundaries.push({ label: scenario.label, width: scenario.width, lines: component.render(scenario.width) });
  }

  setFacts(null, cells(10, 20), false);
  const fallbackPlain = construct({
    data: "!!!!",
    mime: "image/png",
    options: { filename: "/opt/pi-home/pictures/very-long-image-name.png" },
  });
  const plain = fallbackPlain.render(16);
  setFacts(null, cells(10, 20), true);
  const fallbackLink = construct({
    data: png(3, 4),
    mime: "image/png",
    options: { filename: "/opt/pi-home/pictures/a b.png" },
  });
  const emptyFilename = construct({
    data: "!!!!",
    mime: "image/png",
    options: { filename: "" },
    explicitDimensions: dimensions(3, 4),
  });
  out.fallback = {
    styledTruncated: plain,
    styledHyperlinkTruncated: fallbackLink.render(32),
    emptyFilename: emptyFilename.render(80),
  };

  setFacts("kitty", cells(10, 20));
  setRandom(0.5);
  const cached = construct({
    data: "QUJD",
    mime: "image/png",
    explicitDimensions: dimensions(100, 50),
  });
  const first = cached.render(20);
  const second = cached.render(20);
  setFacts("iterm2", cells(5, 10));
  const staleAfterFactChange = cached.render(20);
  const widthMiss = cached.render(19);
  setFacts(null, cells(99, 101));
  const staleWidthMiss = cached.render(19);
  cached.invalidate();
  const refreshedAfterInvalidate = cached.render(19);
  out.cache = {
    first,
    second,
    staleAfterFactChange,
    widthMiss,
    staleWidthMiss,
    refreshedAfterInvalidate,
    sameWidthSameReference: first === second,
    factChangeSameReference: first === staleAfterFactChange,
    widthMissNewReference: first !== widthMiss,
    changedFactsSameWidthReference: widthMiss === staleWidthMiss,
    invalidateNewReference: staleWidthMiss !== refreshedAfterInvalidate,
    imageId: cached.getImageId(),
    randomCalls,
  };

  setFacts(null, cells(10, 20), false);
  setRandom(0.75);
  const fallbackCached = construct({
    data: "QUJD",
    mime: "image/png",
    explicitDimensions: dimensions(100, 50),
  });
  const fallbackFirst = fallbackCached.render(20);
  setFacts("kitty", cells(10, 20));
  const fallbackStale = fallbackCached.render(20);
  fallbackCached.invalidate();
  const fallbackRefreshed = fallbackCached.render(20);
  out.fallbackCache = {
    first: fallbackFirst,
    stale: fallbackStale,
    refreshed: fallbackRefreshed,
    factChangeSameReference: fallbackFirst === fallbackStale,
    invalidateNewReference: fallbackStale !== fallbackRefreshed,
    imageId: fallbackCached.getImageId(),
    randomCalls,
  };

  setFacts("kitty", cells(10, 20));
  setRandom();
  const providedId = construct({
    data: "QUJD",
    mime: "image/png",
    options: { imageId: 42 },
    explicitDimensions: dimensions(100, 50),
  });
  out.providedId = { lines: providedId.render(20), imageId: providedId.getImageId(), randomCalls };

  setFacts("kitty", cells(10, 20));
  setRandom();
  const providedZeroId = construct({
    data: "QUJD",
    mime: "image/png",
    options: { imageId: 0 },
    explicitDimensions: dimensions(100, 50),
  });
  out.providedZeroId = {
    lines: providedZeroId.render(20),
    imageId: providedZeroId.getImageId(),
    randomCalls,
  };

  setFacts("kitty", cells(10, 20));
  setRandom();
  const defaultWidthCap = construct({
    data: "QUJD",
    mime: "image/png",
    options: { imageId: 77 },
    explicitDimensions: dimensions(100, 50),
  });
  const tallCellAspect = construct({
    data: "QUJD",
    mime: "image/png",
    options: { imageId: 77 },
    explicitDimensions: dimensions(100, 1000),
  });
  out.defaultLimits = {
    width100: defaultWidthCap.render(100),
    tallWidth22: tallCellAspect.render(22),
    randomCalls,
  };

  setFacts("kitty", cells(10, 20));
  setRandom();
  const huge = construct({
    data: png(0xffffffff, 0xffffffff),
    mime: "image/png",
    options: { maxWidthCells: Number.MAX_SAFE_INTEGER, maxHeightCells: 1, imageId: 0xfffffffe },
    explicitDimensions: dimensions(0xffffffff, 0xffffffff),
  });
  out.hugeDimensions = {
    width: Number.MAX_SAFE_INTEGER,
    dimensions: huge.dimensions,
    lines: huge.render(Number.MAX_SAFE_INTEGER),
  };
} finally {
  Math.random = originalRandom;
  terminalImage.resetCapabilitiesCache();
  terminalImage.setCellDimensions(cells(9, 18));
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fixturePath = join(root, "crates/pie-components/tests/fixtures/image-component.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log(`wrote Image fixture: ${out.dimensionPriority.length + out.formats.length + out.boundaries.length + 10} scenario groups`);
