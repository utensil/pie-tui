#!/usr/bin/env node
// Harvest terminal color and inline-image primitives from the pinned pi-tui build.
//
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-terminal-primitives.mjs
//
// The fixture records exact source digests, never the local oracle path.
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

const colorsPath = join(dist, "terminal-colors.js");
const imagePath = join(dist, "terminal-image.js");
const colors = await import(pathToFileURL(colorsPath).href);
const image = await import(pathToFileURL(imagePath).href);
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: {
      "terminal-colors.js": digest(colorsPath),
      "terminal-image.js": digest(imagePath),
    },
  },
  colors: [],
  imageLines: [],
  allocatedIds: [],
  kitty: [],
  kittyDeletes: {},
  iterm2: [],
  cellSizes: [],
  dimensions: [],
  metadata: {},
  maxDimensions: {},
  renders: [],
  hyperlinks: [],
  fallbacks: [],
};

for (const value of [
  "\x1b]11;#123456\x07",
  "\x1b]11;#123456789abc\x1b\\",
  "\x1b]11;rgb:1/22/333\x07",
  "\x1b]11;rgba:ffff/0000/8080/ffff\x07",
  "\x1b]11;RgB:a/b/c/ignored\x07",
  "\x1b]11; RGB:0/f/ff \x07",
  "\x1b]11;#fff\x07",
  "\x1b]11;rgb:zz/00/00\x07",
  "\x1b]10;#123456\x07",
  "\x1b]11;#123456",
  "\x1b[?997;1n",
  "\x1b[?997;2n",
  "\x1b[?997;1n\x1b[?997;2n",
  "\x1b[?997;3n",
]) {
  out.colors.push({
    value,
    isOsc11: colors.isOsc11BackgroundColorResponse(value),
    rgb: colors.parseOsc11BackgroundColor(value) ?? null,
    scheme: colors.parseTerminalColorSchemeReport(value) ?? null,
  });
}

for (const value of [
  "plain",
  "\x1b_Ga=T;AAAA\x1b\\",
  "prefix \x1b_Ga=T;AAAA\x1b\\ suffix",
  "\x1b]1337;File=inline=1:AAAA\x07",
  "\x1b[2A\x1b]1337;File=inline=1:AAAA\x07",
]) {
  out.imageLines.push({ value, result: image.isImageLine(value) });
}

const originalRandom = Math.random;
for (const random of [0, 0.5, 0.9999999999999999]) {
  Math.random = () => random;
  out.allocatedIds.push({ random, result: image.allocateImageId() });
}
Math.random = originalRandom;

for (const [label, data, options] of [
  ["empty", "", {}],
  ["options", "QUJD", { columns: 12, rows: 3, imageId: 42, moveCursor: false }],
  ["exact-chunk", "A".repeat(4096), { columns: 1 }],
  ["two-chunks", "B".repeat(4097), { rows: 2 }],
  ["three-chunks", "C".repeat(8193), { imageId: 7 }],
]) {
  out.kitty.push({ label, data, options, result: image.encodeKitty(data, options) });
}
out.kittyDeletes = {
  one: image.deleteKittyImage(42),
  allImages: image.deleteAllKittyImages(),
  allPlacements: image.deleteAllKittyPlacements(),
};

for (const [label, data, options] of [
  ["defaults", "aGVsbG8=", {}],
  ["metadata", "aGVsbG8=", { width: 12, height: "auto", name: "cat 日本.png", preserveAspectRatio: false, inline: false }],
  ["permissive-base64", "aGVs bG8!", { width: "50%", height: 3 }],
]) {
  out.iterm2.push({ label, data, options, result: image.encodeITerm2(data, options) });
}

for (const [imageDimensions, maxWidthCells, maxHeightCells, cellDimensions] of [
  [{ widthPx: 800, heightPx: 600 }, 60, undefined, undefined],
  [{ widthPx: 100, heightPx: 100 }, 10, undefined, { widthPx: 9, heightPx: 18 }],
  [{ widthPx: 1920, heightPx: 1080 }, 80, 12, { widthPx: 8, heightPx: 16 }],
  [{ widthPx: 0, heightPx: 0 }, 0, 0, { widthPx: 9, heightPx: 18 }],
  [{ widthPx: 33, heightPx: 77 }, 4.9, 3.9, { widthPx: 7, heightPx: 15 }],
]) {
  const args = cellDimensions === undefined
    ? [imageDimensions, maxWidthCells, maxHeightCells]
    : [imageDimensions, maxWidthCells, maxHeightCells, cellDimensions];
  out.cellSizes.push({
    imageDimensions,
    maxWidthCells,
    maxHeightCells: maxHeightCells ?? null,
    cellDimensions: cellDimensions ?? null,
    result: image.calculateImageCellSize(...args),
    rows: image.calculateImageRows(imageDimensions, maxWidthCells, cellDimensions ?? undefined),
  });
}

function png(width, height) {
  const b = Buffer.alloc(24);
  Buffer.from([0x89, 0x50, 0x4e, 0x47]).copy(b);
  b.writeUInt32BE(width, 16);
  b.writeUInt32BE(height, 20);
  return b;
}
function gif(width, height, version = "GIF89a") {
  const b = Buffer.alloc(10);
  b.write(version, 0, "ascii");
  b.writeUInt16LE(width, 6);
  b.writeUInt16LE(height, 8);
  return b;
}
function jpeg(width, height) {
  const b = Buffer.alloc(21);
  Buffer.from([0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08]).copy(b);
  b.writeUInt16BE(height, 7);
  b.writeUInt16BE(width, 9);
  return b;
}
function webp(kind, width, height) {
  const b = Buffer.alloc(30);
  b.write("RIFF", 0, "ascii");
  b.write("WEBP", 8, "ascii");
  b.write(kind, 12, "ascii");
  if (kind === "VP8 ") {
    b.writeUInt16LE(width, 26);
    b.writeUInt16LE(height, 28);
  } else if (kind === "VP8L") {
    b.writeUInt32LE(((height - 1) << 14) | (width - 1), 21);
  } else if (kind === "VP8X") {
    const w = width - 1;
    const h = height - 1;
    b[24] = w & 0xff; b[25] = (w >> 8) & 0xff; b[26] = (w >> 16) & 0xff;
    b[27] = h & 0xff; b[28] = (h >> 8) & 0xff; b[29] = (h >> 16) & 0xff;
  }
  return b;
}

for (const [label, buffer, mime] of [
  ["png", png(640, 480), "image/png"],
  ["gif87", gif(17, 19, "GIF87a"), "image/gif"],
  ["gif89", gif(320, 200), "image/gif"],
  ["jpeg", jpeg(1024, 768), "image/jpeg"],
  ["webp-vp8", webp("VP8 ", 333, 222), "image/webp"],
  ["webp-vp8l", webp("VP8L", 511, 257), "image/webp"],
  ["webp-vp8x", webp("VP8X", 70000, 50000), "image/webp"],
  ["invalid", Buffer.from("not an image"), "image/png"],
  ["unsupported-mime", png(3, 4), "image/bmp"],
]) {
  const data = buffer.toString("base64");
  out.dimensions.push({
    label,
    data,
    mime,
    png: image.getPngDimensions(data),
    jpeg: image.getJpegDimensions(data),
    gif: image.getGifDimensions(data),
    webp: image.getWebpDimensions(data),
    generic: image.getImageDimensions(data, mime),
  });
}
out.dimensions.push({
  label: "permissive-base64",
  data: png(11, 13).toString("base64").replace(/=+$/, "") + "!",
  mime: "image/png",
  png: image.getPngDimensions(png(11, 13).toString("base64").replace(/=+$/, "") + "!"),
  jpeg: null,
  gif: null,
  webp: null,
  generic: image.getImageDimensions(png(11, 13).toString("base64").replace(/=+$/, "") + "!", "image/png"),
});

image.registerKittyImageMetadata({ imageId: 23, columns: 8, rows: 5, widthPx: 80, heightPx: 90 });
const metadataLine = `pre${image.encodeKitty("A".repeat(4097), { columns: 8, rows: 5, imageId: 23, moveCursor: false })}post`;
out.metadata = {
  line: metadataLine,
  found: image.getKittyImageMetadata(metadataLine),
  placement: image.getKittyImagePlacement(metadataLine),
  crops: [
    [-1, 2], [0, 5], [0, 2], [2, 2], [4, 9], [5, 1], [1, 0],
  ].map(([hiddenRows, visibleRows]) => ({
    hiddenRows,
    visibleRows,
    result: image.cropKittyImageLine(metadataLine, hiddenRows, visibleRows),
  })),
};

const maxPngData = png(0xffffffff, 0xffffffff).toString("base64").replace(/=+$/, "") + "!";
const maxDimensions = image.getPngDimensions(maxPngData);
const maxLine = `pre${image.encodeKitty(maxPngData, {
  columns: 0xffffffff,
  rows: 0xffffffff,
  imageId: 0xfffffffe,
  moveCursor: false,
})}post`;
image.registerKittyImageMetadata({
  imageId: 0xfffffffe,
  columns: 0xffffffff,
  rows: 0xffffffff,
  widthPx: maxDimensions.widthPx,
  heightPx: maxDimensions.heightPx,
});
const maxPlacement = image.getKittyImagePlacement(maxLine);
out.maxDimensions = {
  data: maxPngData,
  dimensions: maxDimensions,
  line: maxLine,
  found: image.getKittyImageMetadata(maxLine),
  placement: maxPlacement,
  estimatedDecodedBytesText: String(maxPlacement.estimatedDecodedBytes),
  crop: image.cropKittyImageLine(maxLine, 0xfffffffe, 1),
};
const largeRows = Number.MAX_SAFE_INTEGER;
const largeRowImageId = 0xfffffffd;
const largeRowLine = `pre${image.encodeKitty(maxPngData, {
  columns: 1,
  rows: largeRows,
  imageId: largeRowImageId,
  moveCursor: false,
})}post`;
image.registerKittyImageMetadata({
  imageId: largeRowImageId,
  columns: 1,
  rows: largeRows,
  widthPx: maxDimensions.widthPx,
  heightPx: maxDimensions.heightPx,
});
out.maxDimensions.largeRowCrop = {
  imageId: largeRowImageId,
  rows: largeRows,
  hiddenRows: largeRows - 1,
  line: largeRowLine,
  result: image.cropKittyImageLine(largeRowLine, largeRows - 1, 1),
};

image.setCellDimensions({ widthPx: 9, heightPx: 18 });
for (const [protocol, options] of [
  [null, {}],
  ["kitty", { maxWidthCells: 10, maxHeightCells: 4, imageId: 99, moveCursor: false }],
  ["iterm2", { maxWidthCells: 10, maxHeightCells: 4, preserveAspectRatio: false }],
]) {
  image.setCapabilities({ images: protocol, trueColor: true, hyperlinks: true });
  out.renders.push({
    protocol,
    options,
    result: image.renderImage("aGVsbG8=", { widthPx: 100, heightPx: 50 }, options),
  });
}

for (const [text, url] of [
  ["docs", "https://example.test/a?b=c#d"],
  ["日本語", "file:///opt/data/a%20b.png"],
]) {
  out.hyperlinks.push({ text, url, result: image.hyperlink(text, url) });
}

for (const hyperlinks of [false, true]) {
  image.setCapabilities({ images: null, trueColor: true, hyperlinks });
  for (const [mime, dimensions, filename] of [
    ["image/png", undefined, undefined],
    ["image/jpeg", { widthPx: 640, heightPx: 480 }, undefined],
    ["image/webp", { widthPx: 10, heightPx: 20 }, "relative/a b.webp"],
    ["image/png", { widthPx: 3, heightPx: 4 }, "/opt/pi-home/pics/a b.png"],
    ["image/gif", { widthPx: 5, heightPx: 6 }, "/opt/data/x.gif"],
  ]) {
    out.fallbacks.push({
      hyperlinks,
      mime,
      dimensions: dimensions ?? null,
      filename: filename ?? null,
      result: image.imageFallback(mime, dimensions, filename),
    });
  }
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fixturePath = join(root, "crates/pie-core/tests/fixtures/terminal-primitives.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log(`wrote terminal primitive fixture: ${out.colors.length} colors, ${out.dimensions.length} images`);
