import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  cp,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const mutationRoot = await mkdtemp(join(tmpdir(), 'pie-tui-napi-mutations-'))
const directFiles = [
  'LICENSE',
  'README.md',
  'index.cjs',
  'index.d.ts',
  'index.js',
  'native-loader.cjs',
  'package.json',
  'runtime.cjs',
]
const testFiles = [
  'artifact-helpers.mjs',
  'boundary-child.mjs',
  'check-reference.mjs',
  'check-capability-overrides-reference.mjs',
  'capability-overrides.test.mjs',
  'oracle-contract.json',
  'm6-runtime.test.mjs',
  'm6-semantic-oracle.mjs',
  'pack-consumer.mjs',
  'postm6-randomized.test.mjs',
  'runtime.test.mjs',
  'tier1-scroll.test.mjs',
  'tier1-scroll-oracle.mjs',
  'selection-geometry.test.mjs',
  'selection-geometry-oracle.mjs',
  'word-copy-oracle.mjs',
  'word-copy-regressions.test.mjs',
  'upstream-drift.json',
]

for (const entry of await readdir(packageRoot)) {
  if (entry.endsWith('.node')) {
    directFiles.push(entry)
  }
}

async function prepareCase(name) {
  const directory = join(mutationRoot, name)
  await mkdir(join(directory, 'test'), { recursive: true })
  for (const file of directFiles) {
    await copyFile(join(packageRoot, file), join(directory, file))
  }
  for (const file of testFiles) {
    await copyFile(join(packageRoot, 'test', file), join(directory, 'test', file))
  }
  await symlink(join(packageRoot, 'node_modules'), join(directory, 'node_modules'))
  return directory
}

async function replaceOnce(directory, file, from, to) {
  const path = join(directory, file)
  const content = await readFile(path, 'utf8')
  const occurrences = content.split(from).length - 1
  assert.equal(occurrences, 1, `${file}: mutation marker count`)
  await writeFile(path, content.replace(from, to))
}

const runtimeMutations = [
  {
    name: 'capability-overlay-clone-loss',
    from: '  capabilityOverrides = { ...overrides }',
    to: '  capabilityOverrides = overrides',
    expected:
      'capability overrides clone the input and take partial precedence',
    testFile: 'capability-overrides.test.mjs',
  },
  {
    name: 'capability-overlay-precedence-inversion',
    from:
      '      ...detectCapabilities(\n        hyperlinks === undefined ? undefined : () => hyperlinks,\n      ),\n      ...capabilityOverrides,',
    to:
      '      ...capabilityOverrides,\n      ...detectCapabilities(\n        hyperlinks === undefined ? undefined : () => hyperlinks,\n      ),',
    expected:
      'capability overrides clone the input and take partial precedence',
    testFile: 'capability-overrides.test.mjs',
  },
  {
    name: 'capability-overlay-equality-cache-loss',
    from: '    capabilityOverrides.images === overrides.images &&',
    to: '    false && capabilityOverrides.images === overrides.images &&',
    expected:
      'equal overrides preserve cache identity while changed values invalidate it',
    testFile: 'capability-overrides.test.mjs',
  },
  {
    name: 'capability-overlay-change-keeps-stale-cache',
    from: '  capabilityOverrides = { ...overrides }\n  cachedCapabilities = null',
    to: '  capabilityOverrides = { ...overrides }\n  // mutation: keep stale cache',
    expected:
      'equal overrides preserve cache identity while changed values invalidate it',
    testFile: 'capability-overrides.test.mjs',
  },
  {
    name: 'capability-overlay-reset-clears-persistence',
    from: 'function resetCapabilitiesCache() {\n  cachedCapabilities = null\n}',
    to:
      'function resetCapabilitiesCache() {\n  cachedCapabilities = null\n  capabilityOverrides = {}\n}',
    expected:
      'persistent overrides survive reset and resume after setCapabilities',
    testFile: 'capability-overrides.test.mjs',
  },
  {
    name: 'cursor-marker-byte-drift',
    from: 'const CURSOR_MARKER = native.nativeCursorMarker()',
    to: "const CURSOR_MARKER = `${native.nativeCursorMarker()}x`",
    expected:
      'canonical constants retain exact content, descriptors, and shared identity',
  },
  {
    name: 'keybinding-table-omission',
    from: 'for (const definition of native.nativeGetTuiKeybindingDefinitions()) {',
    to:
      'for (const definition of native.nativeGetTuiKeybindingDefinitions().slice(1)) {',
    expected:
      'canonical constants retain exact content, descriptors, and shared identity',
  },
  {
    name: 'spacer-native-state-bypass',
    from: '    state.setLines(Number(this.lines))',
    to: '    state.setLines(0)',
    expected:
      'Spacer keeps canonical prototypes while native state stays per instance',
  },
  {
    name: 'fuzzy-match-case-drift',
    from: '    text.toLowerCase(),',
    to: '    text.toUpperCase(),',
    expected:
      'fuzzy matching preserves UTF-16 scoring and callback reentrancy',
  },
  {
    name: 'fuzzy-filter-callback-snapshot',
    from: '  for (const item of items) {',
    to: '  for (const item of [...items]) {',
    expected:
      'fuzzy matching preserves UTF-16 scoring and callback reentrancy',
  },
  {
    name: 'render-image-capability-inversion',
    from: '  if (!capabilities.images) return null',
    to: '  if (capabilities.images) return null',
    expected:
      'renderImage reads live capability and cell facts with JavaScript Number semantics',
  },
  {
    name: 'image-fallback-link-omission',
    from: '    if (getCapabilities().hyperlinks && isAbsolute(filename)) {',
    to: '    if (false && getCapabilities().hyperlinks && isAbsolute(filename)) {',
    expected:
      'imageFallback snapshots live capability facts and Node path behavior',
  },
  {
    name: 'composite-raw-utf16-loss',
    from:
      '  const { encoded, decode, tokenPrefix } = encodeRawUtf16Strings(\n    baseLine,\n    overlayLine,\n  )',
    to:
      "  const encoded = [baseLine.toWellFormed(), overlayLine.toWellFormed()]\n  const decode = (value) => value\n  const tokenPrefix = ''",
    expected:
      'compositeTuiLine preserves ANSI, wide cells, raw UTF-16, and image identity',
  },
  {
    name: 'composite-fractional-boundary-floor',
    from:
      '  const overlay = native.nativeSliceComposite(\n    encodedOverlay,\n    Number(overlayWidth),\n    tokenPrefix,\n  )',
    to:
      '  const overlay = native.nativeSliceComposite(\n    encodedOverlay,\n    Math.floor(Number(overlayWidth)),\n    tokenPrefix,\n  )',
    expected:
      'compositeTuiLine preserves ANSI, wide cells, raw UTF-16, and image identity',
  },
  {
    name: 'composite-cross-segment-utf16-width',
    from: '    visibleWidthRawUtf16(decodedResult) <= totalWidth',
    to: '    native.nativeVisibleWidth(result) <= totalWidth',
    expected:
      'compositeTuiLine preserves ANSI, wide cells, raw UTF-16, and image identity',
  },
  {
    name: 'composite-partial-token-leak',
    from:
      '    Number(afterLength),\n    tokenPrefix,\n  )',
    to:
      "    Number(afterLength),\n    '\\u0000',\n  )",
    expected:
      'compositeTuiLine preserves ANSI, wide cells, raw UTF-16, and image identity',
  },
  {
    name: 'raw-utf16-loss',
    from: 'native.nativeRenderLatex(source, {',
    to: 'native.nativeRenderLatex(source.toWellFormed(), {',
    expected: 'renderLatex crosses Node-API as exact raw UTF-16',
  },
  {
    name: 'nullable-collapse',
    from: 'const fromNullable = (value) => (value === null ? undefined : value)',
    to: 'const fromNullable = (value) => value',
    expected: 'undefined and null remain distinct at the public boundary',
  },
  {
    name: 'cell-identity-clone',
    from: '  cellDimensions = dimensions\n',
    to: '  cellDimensions = { ...dimensions }\n',
    expected: 'cell and capability slots preserve caller identity across loaders',
  },
  {
    name: 'capability-post-callback-env-reread',
    from:
      '  return native.nativeDetectCapabilities(\n    environment,\n    Boolean(tmuxForwards),\n  )',
    to:
      '  return native.nativeDetectCapabilities(\n    snapshotTerminalEnvironment(),\n    Boolean(tmuxForwards),\n  )',
    expected:
      'detectCapabilities snapshots all environment facts before the tmux callback',
  },
  {
    name: 'reject-image-id-zero',
    from:
      "  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {\n    throw new RangeError('imageId must be an unsigned 32-bit integer')",
    to:
      "  if (!Number.isSafeInteger(value) || value < 1 || value > 0xffffffff) {\n    throw new RangeError('imageId must be an unsigned 32-bit integer')",
    expected: 'image IDs and codec defaults match the reference receipts',
  },
  {
    name: 'kitty-utf8-byte-slicing',
    from: 'native.nativeEncodeKittyUtf16(base64Data, nativeOptions)',
    to: 'native.nativeEncodeKitty(base64Data, nativeOptions)',
    expected: 'Kitty chunks byte-exactly at 4096 JavaScript UTF-16 units',
  },
  {
    name: 'calculate-rows-u32-coercion',
    from:
      '  return calculateImageCellSize(\n    imageDimensions,\n    targetWidthCells,\n    undefined,\n    dimensions,\n  ).rows',
    to:
      '  return native.nativeCalculateImageRows(\n    imageDimensions,\n    targetWidthCells,\n    dimensions,\n  )',
    expected: 'image row calculation retains canonical JavaScript Number coercion',
  },
  {
    name: 'iterm2-rust-size-approximation',
    from:
      "  return native.nativeEncodeITerm2Utf16(\n    base64Data,\n    Buffer.byteLength(base64Data, 'base64'),\n    nativeOptions,\n  )",
    to: '  return native.nativeEncodeITerm2(base64Data, nativeOptions)',
    expected: 'iTerm2 byte lengths are computed with Node Buffer base64 semantics',
  },
  {
    name: 'default-arity-drift',
    from: '  dimensions = { widthPx: 9, heightPx: 18 },\n',
    to: '  dimensions,\n',
    expected: 'ESM and CJS expose the exact reviewed names, slots, and descriptors',
  },
  {
    name: 'namespace-export-order',
    from: 'const selectedExports = {\n  Box,\n  CURSOR_MARKER,\n',
    to: 'const selectedExports = {\n  CURSOR_MARKER,\n  Box,\n',
    expected: 'ESM and CJS expose the exact reviewed names, slots, and descriptors',
  },
  {
    name: 'namespace-mutation-traps',
    from:
      'const runtime = new Proxy(runtimeTarget, {\n  set: () => false,\n  defineProperty: () => false,\n  deleteProperty: () => false,\n})',
    to: 'const runtime = runtimeTarget',
    expected: 'ESM and CJS expose the exact reviewed names, slots, and descriptors',
  },
  {
    name: 'duplicate-export-value',
    from: '  decodeKittyPrintable,\n',
    to: '  decodeKittyPrintable: parseKey,\n',
    expected: 'ESM and CJS expose the exact reviewed names, slots, and descriptors',
  },
  {
    name: 'unintended-export',
    from: 'const selectedExports = {\n',
    to: 'const selectedExports = {\n  unintendedM5Export: () => undefined,\n',
    expected: 'ESM and CJS expose the exact reviewed names, slots, and descriptors',
  },
  {
    name: 'lossy-utility-input',
    from: 'function assertWellFormed(value, parameter) {\n',
    to:
      'function assertWellFormed(value, parameter) {\n  return value\n',
    expected: 'pure utilities are exact for well-formed strings and reject lossy input',
  },
]

const m6RuntimeMutations = [
  {
    name: 'm6-main-native-planner-omission',
    from: '    mainScreenPlannerRegistry.set(this, new native.NativeMainScreenPlanner())',
    to: '    mainScreenPlannerRegistry.set(this, new native.NativeAltScreenPlanner())',
    expected: 'M6 MainScreen is native-planned, differential, synchronized, and stopped-safe',
  },
  {
    name: 'm6-color-scheme-query-byte-drift',
    from: "      this.terminal.write('\\x1b[?996n')",
    to: "      this.terminal.write('\\x1b[?995n')",
    expected: 'M6 OSC 11 queries and color-scheme notifications consume terminal reports',
  },
  {
    name: 'm6-cell-size-query-byte-drift',
    from: "    if (getCapabilities().images) this.terminal.write('\\x1b[16t')",
    to: "    if (getCapabilities().images) this.terminal.write('\\x1b[15t')",
    expected: 'M6 terminal queries preserve DSR, cell-size, and hex-color behavior',
  },
  {
    name: 'm6-scroll-disable-follow-loss',
    from: '    const nextFollowSuppressedAtEnd = options.disableFollow === true && next === maximum',
    to: '    const nextFollowSuppressedAtEnd = false',
    expected: 'M6 ScrollView disableFollow suppresses reattachment at the current end',
  },
  {
    name: 'm6-alt-mouse-default-inversion',
    from: '    this.mouseEnabled = options.mouse ?? true',
    to: '    this.mouseEnabled = options.mouse ?? false',
    expected: 'M6 AltScreen defaults mouse on and emits native-planned synchronized diffs',
  },
  {
    name: 'm6-alt-layout-height-collapse',
    from: '    this.currentLayout = renderLayoutFrame(root, width, height, () => this.requestRender())\n    let document = this.currentLayout.lines',
    to: '    this.currentLayout = renderLayoutFrame(root, width, 1, () => this.requestRender())\n    let document = this.currentLayout.lines',
    expected: 'M6 AltScreen allocates fullscreen VStack growth to the primary ScrollView',
  },
]

const postM6RandomizedMutations = [
  {
    name: 'postm6-scroll-upper-clamp-loss',
    from: '    const next = Math.max(0, Math.min(maximum, requested))',
    to: '    const next = Math.max(0, requested)',
  },
  {
    name: 'postm6-vstack-available-height-loss',
    from: '      const sizes = component.allocate(entries, intrinsic, componentHeight)',
    to: '      const sizes = component.allocate(entries, intrinsic, undefined)',
  },
]

const tier1ScrollMutations = [
  {
    name: 'tier1-scroll-hit-order-reversed',
    from: '  matches.sort((left, right) => right.depth - left.depth)',
    to: '  matches.sort((left, right) => left.depth - right.depth)',
    pattern: 'wheel hit-testing does not collapse',
  },
  {
    name: 'tier1-scroll-primary-only-routing',
    from:
      '    for (const scrollView of this.currentLayout\n' +
      '      ? getScrollViewsAt(this.currentLayout, event.x, event.y)\n' +
      '      : []) {',
    to: '    for (const scrollView of [this.getPrimaryScrollView()]) {',
    pattern: 'wheel hit-testing does not collapse',
  },
  {
    name: 'tier1-scroll-contain-ignored',
    from:
      "      if (remaining === 0 || scrollView.overscroll === 'contain') break",
    to: '      if (remaining === 0) break',
    pattern: 'contain overscroll consumes',
  },
  {
    name: 'tier1-scroll-remainder-discarded',
    from: '      remaining = scrollView.scrollBy(remaining)',
    to: '      scrollView.scrollBy(remaining)\n      remaining = 0',
    pattern: 'chains the exact remainder',
  },
  {
    name: 'tier1-scroll-thumb-height-drift',
    from: '      Math.round((trackHeight * trackHeight) / contentHeight),',
    to: '      trackHeight,',
    pattern: 'always scrollbar paints',
  },
  {
    name: 'tier1-scrollbar-column-drift',
    from:
      '  const column = box.rect.x + box.rect.width - 1\n' +
      '  if (column < box.clip.x || column >= box.clip.x + box.clip.width) {',
    to:
      '  const column = box.rect.x + box.rect.width - 2\n' +
      '  if (column < box.clip.x || column >= box.clip.x + box.clip.width) {',
    pattern: 'always scrollbar paints',
  },
  {
    name: 'tier1-scrollbar-drag-mapping-loss',
    from:
      '          : Math.round(\n' +
      '              (thumbOffset / maxThumbOffset) * geometry.maxScrollTop,\n' +
      '            )',
    to: '          : 0',
    pattern: 'always scrollbar paints',
  },
  {
    name: 'tier1-scrollbar-paint-omission',
    from: '    paintScrollbar(box, lines, safeWidth)',
    to: '    // mutation: omit scrollbar painting',
    pattern: 'always scrollbar paints',
  },
  {
    name: 'tier1-scrollbar-hover-activation-loss',
    from: '    this.scrollbarHover?.setScrollbarActive(true)',
    to: '    this.scrollbarHover?.setScrollbarActive(false)',
    pattern: 'auto scrollbar paints',
  },
  {
    name: 'tier1-scroll-overlay-deferral-loss',
    from:
      '      if (this.shouldDeferViewportInputToOverlay()) return undefined\n' +
      '      if (direction !== 0) this.routeWheel({ direction, x: mouse.x, y: mouse.y })',
    to:
      '      if (false && this.shouldDeferViewportInputToOverlay()) return undefined\n' +
      '      if (direction !== 0) this.routeWheel({ direction, x: mouse.x, y: mouse.y })',
    pattern: 'capturing overlays defer wheel',
  },
  {
    name: 'tier1-scrollbar-stop-cleanup-loss',
    from:
      '  beforeTerminalStop() {\n' +
      '    this.closeSearch()\n' +
      '    this.selectionPressActive = false\n' +
      '    this.clearFlashes()\n' +
      '    this.stopScrollbarHover()\n' +
      '    this.stopScrollbarDrag()',
    to:
      '  beforeTerminalStop() {\n' +
      '    this.closeSearch()\n' +
      '    this.selectionPressActive = false',
    pattern: 'stop clears scrollbar hover activation',
  },
  {
    name: 'tier1-scrollbar-focus-out-cleanup-loss',
    from:
      "      this.selectionGranularity = 'character'\n" +
      '      this.selectionInitialRange = undefined\n' +
      '      this.lastClick = undefined\n' +
      '      this.selectionDragged = false\n' +
      '      this.pressedUrl = undefined\n' +
      '      this.stopScrollbarHover()\n' +
      '      this.stopScrollbarDrag()\n' +
      '      if (hadSelection) this.requestRender()',
    to:
      '      this.lastClick = undefined\n' +
      '      this.pressedUrl = undefined\n' +
      '      if (hadSelection) this.requestRender()',
    pattern: 'focus-out clears scrollbar hover activation',
  },
]

const selectionGeometryMutations = [
  {
    name: 'selection-scrollview-identity-loss',
    from:
      '    const scrollView = !this.hasOverlay() && this.currentLayout\n' +
      '      ? getScrollViewsAt(this.currentLayout, event.x, event.y)[0]\n' +
      '      : undefined',
    to: '    const scrollView = undefined',
    pattern: 'selection remains owned by the hit ScrollView',
  },
  {
    name: 'selection-scrollview-clip-loss',
    from:
      '    const pointerRow = Math.max(visibleTop, Math.min(visibleBottom, y))',
    to: '    const pointerRow = y',
    pattern: 'selection remains owned by the hit ScrollView',
  },
  {
    name: 'selection-scrollview-source-loss',
    from: '      sourceLines = box.scrollContentLines',
    to: '      sourceLines = this.lastDocument',
    pattern: 'selection remains owned by the hit ScrollView',
  },
  {
    name: 'selection-cross-pane-identity-allow',
    from:
      '    if (!anchor || !focus || anchor.scrollView !== focus.scrollView ||\n' +
      '      (anchor.row === focus.row && anchor.col === focus.col)) return undefined',
    to:
      '    if (!anchor || !focus ||\n' +
      '      (anchor.row === focus.row && anchor.col === focus.col)) return undefined',
    pattern: 'selection bounds reject endpoints from different ScrollViews',
  },
  {
    name: 'selection-grapheme-start-snap-loss',
    from:
      '      start = getLayoutGraphemeCellRange(line, selection.start.col)?.start ??\n' +
      '        Math.min(selection.start.col, lineWidth)',
    to: '      start = Math.min(selection.start.col, lineWidth)',
    pattern: 'selection endpoints snap to the complete wide grapheme',
  },
  {
    name: 'selection-grapheme-end-snap-loss',
    from:
      '        : getLayoutGraphemeCellRange(line, selection.end.col)?.end ??\n' +
      '          Math.min(selection.end.col + 1, lineWidth)',
    to: '        : Math.min(selection.end.col + 1, lineWidth)',
    pattern: 'selection end columns snap to the complete wide grapheme',
  },
]

const wordCopyMutations = [
  {
    name: 'word-selection-helper-loss',
    from: '    const word = this.getWordSelection(anchor)',
    to: '    const word = undefined',
    expected: 'double-click word selection',
  },
  {
    name: 'word-boundary-width-loss',
    from: '      const end = start + visibleWidth(segment.segment)',
    to: '      const end = start + 1',
    expected: 'double-click word selection',
  },
  {
    name: 'double-click-timeout-guard-loss',
    from: '      now - previous.timestamp <= DOUBLE_CLICK_INTERVAL_MS &&',
    to: '      true &&',
    expected: 'double-click timeout',
  },
  {
    name: 'triple-click-line-range-loss',
    from: "        ? this.getLineSelection(anchor)\n        : undefined",
    to: "        ? undefined\n        : undefined",
    expected: 'triple-click line selection',
  },
  {
    name: 'copy-callback-result-loss',
    from: "      this.flash(copied ? 'Copied!' : 'Copy failed')",
    to: "      this.flash('Copied!')",
    expected: 'callback copy failure and flash',
  },
  {
    name: 'osc52-base64-encoding-loss',
    from: "      this.terminal.write(`\\x1b]52;c;${Buffer.from(text).toString('base64')}\\x07`)",
    to: "      this.terminal.write(`\\x1b]52;c;${text}\\x07`)",
    expected: 'OSC52 clipboard payload',
  },
]

const wordCopyRegressionMutations = [
  {
    name: 'word-copy-flash-default-drift',
    from: '  flash(message, durationMs = 1000) {',
    to: '  flash(message, durationMs = 2000) {',
    expected: 'AltScreen flash defaults to the authenticated 0.84.2 one-second lifetime',
  },
  {
    name: 'word-copy-focus-granularity-reset-loss',
    from:
      "      this.selectionGranularity = 'character'\n" +
      '      this.selectionInitialRange = undefined',
    to:
      "      this.selectionGranularity = 'word'\n" +
      '      this.selectionInitialRange = undefined',
    expected: 'AltScreen focus-out resets all selection state',
  },
  {
    name: 'word-copy-focus-drag-reset-loss',
    from:
      '      this.selectionDragged = false\n' +
      '      this.pressedUrl = undefined',
    to:
      '      this.selectionDragged = true\n' +
      '      this.pressedUrl = undefined',
    expected: 'AltScreen focus-out resets all selection state',
  },
  {
    name: 'word-copy-flash-disposal-loss',
    from:
      '  beforeTerminalStop() {\n' +
      '    this.closeSearch()\n' +
      '    this.selectionPressActive = false\n' +
      '    this.clearFlashes()',
    to:
      '  beforeTerminalStop() {\n' +
      '    this.closeSearch()\n' +
      '    this.selectionPressActive = false',
    expected: 'AltScreen stop and restart dispose transient flash entries and timers',
  },
]

const m6SemanticMutations = [
  {
    name: 'm6-alt-search-open-omission',
    from: '      if (!release) this.openSearch()',
    to: '      if (false && !release) this.openSearch()',
    expected: 'AltScreen keyboard/search receipt',
  },
  {
    name: 'm6-alt-selection-copy-range-loss',
    from: "      lines.push(stripTerminalSequences(sliceByColumn(line, columns.start, Math.max(0, columns.end - columns.start), true)).trimEnd())",
    to: "      lines.push(stripTerminalSequences(sliceByColumn(line, columns.start, Math.max(0, columns.end - columns.start - 1), true)).trimEnd())",
    expected: 'AltScreen selection/copy receipt',
  },
  {
    name: 'm6-alt-kitty-metadata-ownership-loss',
    from: '      registerKittyImageMetadata({',
    to: '      false && registerKittyImageMetadata({',
    expected: 'AltScreen Kitty cache/placement receipt',
  },
]

async function expectKilled(name, directory, args, expected) {
  return expectKilledWithEnv(name, directory, args, expected)
}

async function expectKilledWithEnv(
  name,
  directory,
  args,
  expected,
  extraEnv = {},
) {
  const result = spawnSync(process.execPath, args, {
    cwd: directory,
    encoding: 'utf8',
    env: {
      ...process.env,
      NAPI_RS_NATIVE_LIBRARY_PATH: '',
      ...extraEnv,
    },
  })
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`
  assert.notEqual(result.status, 0, `mutation survived: ${name}`)
  assert.ok(output.includes(expected), `mutation did not reach its gate: ${name}`)
  console.log(`mutation killed: ${name}`)
}

async function prepareReferenceCopy(directory) {
  const sourceDist = process.env.PI_TUI_DIST
  assert.ok(sourceDist, 'PI_TUI_DIST is required for provenance mutations')
  assert.ok(
    process.env.PI_TUI_TARBALL,
    'PI_TUI_TARBALL is required for provenance mutations',
  )
  const sourcePackage = dirname(sourceDist)
  const referenceRoot = join(directory, 'reference')
  const copiedPackage = join(referenceRoot, 'package')
  const copiedDist = join(copiedPackage, 'dist')
  await mkdir(copiedPackage, { recursive: true })
  await cp(sourceDist, copiedDist, { recursive: true })
  await copyFile(
    join(sourcePackage, 'package.json'),
    join(copiedPackage, 'package.json'),
  )
  await symlink(
    dirname(dirname(sourcePackage)),
    join(referenceRoot, 'node_modules'),
  )
  return copiedDist
}

try {
  for (const mutation of runtimeMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(
      directory,
      'runtime.cjs',
      mutation.from,
      mutation.to,
    )
    await expectKilled(
      mutation.name,
      directory,
      ['--test', `test/${mutation.testFile ?? 'runtime.test.mjs'}`],
      mutation.expected,
    )
  }

  const overlaySourceDigestDirectory = await prepareCase(
    'capability-overlay-reference-source-digest',
  )
  await replaceOnce(
    overlaySourceDigestDirectory,
    'test/upstream-drift.json',
    '4d3f9ac5c61c42e3d198ea90db05fe9159ea8942191dafed5c6fafc9be870bd6',
    '5d3f9ac5c61c42e3d198ea90db05fe9159ea8942191dafed5c6fafc9be870bd6',
  )
  await expectKilled(
    'capability-overlay-reference-source-digest',
    overlaySourceDigestDirectory,
    ['test/check-capability-overrides-reference.mjs'],
    'terminal-image.js SHA-256',
  )

  for (const mutation of m6RuntimeMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      ['--test', 'test/m6-runtime.test.mjs'],
      mutation.expected,
    )
  }

  for (const mutation of postM6RandomizedMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      ['--test', 'test/postm6-randomized.test.mjs'],
      'post-M6 deterministic fullscreen ScrollView state machine stays bounded',
    )
  }

  for (const mutation of tier1ScrollMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      [
        '--test',
        `--test-name-pattern=${mutation.pattern}`,
        'test/tier1-scroll.test.mjs',
      ],
      mutation.pattern,
    )
  }

  for (const mutation of selectionGeometryMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      [
        '--test',
        `--test-name-pattern=${mutation.pattern}`,
        'test/selection-geometry.test.mjs',
      ],
      mutation.pattern,
    )
  }

  for (const mutation of wordCopyMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      ['test/word-copy-oracle.mjs'],
      mutation.expected,
    )
  }

  for (const mutation of wordCopyRegressionMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      ['--test', `--test-name-pattern=${mutation.expected}`, 'test/word-copy-regressions.test.mjs'],
      mutation.expected,
    )
  }

  for (const mutation of m6SemanticMutations) {
    const directory = await prepareCase(mutation.name)
    await replaceOnce(directory, 'runtime.cjs', mutation.from, mutation.to)
    await expectKilled(
      mutation.name,
      directory,
      ['test/m6-semantic-oracle.mjs'],
      mutation.expected,
    )
  }

  const paddingDirectory = await prepareCase('padded-allocation-guard')
  await replaceOnce(
    paddingDirectory,
    'runtime.cjs',
    'const MAX_PAD_WIDTH = MAX_STRING_LENGTH',
    'const MAX_PAD_WIDTH = 64',
  )
  await replaceOnce(
    paddingDirectory,
    'runtime.cjs',
    "  if (pad && checkedMaxWidth > MAX_PAD_WIDTH) {\n    throw new RangeError('padded output exceeds the JavaScript string limit')\n  }\n",
    '',
  )
  const paddingResult = spawnSync(
    process.execPath,
    ['test/boundary-child.mjs'],
    {
      cwd: paddingDirectory,
      encoding: 'utf8',
      env: {
        ...process.env,
        NAPI_RS_NATIVE_LIBRARY_PATH: '',
        PIE_NAPI_PAD_WIDTH: '65',
      },
    },
  )
  const paddingOutput = `${paddingResult.stdout ?? ''}\n${paddingResult.stderr ?? ''}`
  assert.notEqual(paddingResult.status, 0, 'mutation survived: padded-allocation-guard')
  assert.match(paddingOutput, /Missing expected exception/)
  console.log('mutation killed: padded-allocation-guard')

  const artifactDigestDirectory = await prepareCase(
    'reference-artifact-digest',
  )
  await replaceOnce(
    artifactDigestDirectory,
    'test/oracle-contract.json',
    '3abec26d852a9574fd341b8b4984277fc76dabb57a0360df4c19cc1fc0df993e',
    '0abec26d852a9574fd341b8b4984277fc76dabb57a0360df4c19cc1fc0df993e',
  )
  await expectKilled(
    'reference-artifact-digest',
    artifactDigestDirectory,
    ['test/check-reference.mjs'],
    'reference tarball sha256',
  )

  const dependencyTreeDirectory = await prepareCase(
    'reference-dependency-tree-digest',
  )
  await replaceOnce(
    dependencyTreeDirectory,
    'test/oracle-contract.json',
    '0cefc607630cc42e61985c544c55bde6955c23ec49d0b3d0f39fe51ec4296767',
    '1cefc607630cc42e61985c544c55bde6955c23ec49d0b3d0f39fe51ec4296767',
  )
  await expectKilled(
    'reference-dependency-tree-digest',
    dependencyTreeDirectory,
    ['test/check-reference.mjs'],
    'get-east-asian-width tree digest',
  )

  const selectedClosureDirectory = await prepareCase(
    'reference-selected-closure-digest',
  )
  await replaceOnce(
    selectedClosureDirectory,
    'test/oracle-contract.json',
    '4e8de99d7a73e192b1215d5e37c1cbd687be1c8917edfd0ceb7636f44352cbc8',
    '5e8de99d7a73e192b1215d5e37c1cbd687be1c8917edfd0ceb7636f44352cbc8',
  )
  await expectKilled(
    'reference-selected-closure-digest',
    selectedClosureDirectory,
    ['test/check-reference.mjs'],
    'selected surface closure fuzzy.js',
  )

  const sourceOrderDirectory = await prepareCase(
    'reference-source-export-order',
  )
  await replaceOnce(
    sourceOrderDirectory,
    'test/oracle-contract.json',
    '    "Marked",\n    "CombinedAutocompleteProvider",',
    '    "CombinedAutocompleteProvider",\n    "Marked",',
  )
  await expectKilled(
    'reference-source-export-order',
    sourceOrderDirectory,
    ['test/check-reference.mjs'],
    'canonical index.js runtime export source order',
  )

  const canonicalTypeDirectory = await prepareCase(
    'reference-canonical-runtime-type',
  )
  await replaceOnce(
    canonicalTypeDirectory,
    'test/oracle-contract.json',
    '  "canonicalRuntimeTypes": {\n    "Box": "function",',
    '  "canonicalRuntimeTypes": {\n    "Box": "object",',
  )
  await expectKilled(
    'reference-canonical-runtime-type',
    canonicalTypeDirectory,
    ['test/check-reference.mjs'],
    'Box',
  )

  for (const [name, relativeFile] of [
    ['reference-index-mutation', 'index.js'],
    ['reference-transitive-mutation', 'terminal-image.js'],
  ]) {
    const directory = await prepareCase(name)
    const copiedDist = await prepareReferenceCopy(directory)
    const path = join(copiedDist, relativeFile)
    await writeFile(path, `${await readFile(path, 'utf8')}\n// mutation\n`)
    await expectKilledWithEnv(
      name,
      directory,
      ['test/check-reference.mjs'],
      'colocated dist tree digest',
      { PI_TUI_DIST: copiedDist },
    )
  }

  const dependencyDirectory = await prepareCase(
    'reference-dependency-version',
  )
  const dependencyDist = await prepareReferenceCopy(dependencyDirectory)
  await replaceOnce(
    dirname(dependencyDist),
    'package.json',
    '"marked": "18.0.5"',
    '"marked": "18.0.4"',
  )
  await expectKilledWithEnv(
    'reference-dependency-version',
    dependencyDirectory,
    ['test/check-reference.mjs'],
    'colocated install dependencies',
    { PI_TUI_DIST: dependencyDist },
  )

  const readmeCountDirectory = await prepareCase('readme-selected-count')
  await replaceOnce(
    readmeCountDirectory,
    'README.md',
    'complete 69-export runtime namespace and exact 133-name baseline\nof the authenticated pi-tui 0.84.2 package',
    'incomplete 68-export runtime namespace and exact 133-name baseline\nof the authenticated pi-tui 0.84.2 package',
  )
  await expectKilled(
    'readme-selected-count',
    readmeCountDirectory,
    ['test/pack-consumer.mjs'],
    'README pins the complete 69/133 baseline',
  )

  const readmeBoundaryDirectory = await prepareCase(
    'readme-drop-in-boundary',
  )
  await replaceOnce(
    readmeBoundaryDirectory,
    'README.md',
    'it is not the upstream package and does not claim compatibility outside the\ndocumented parity ledger',
    'it is the upstream package and claims compatibility outside the\ndocumented parity ledger',
  )
  await expectKilled(
    'readme-drop-in-boundary',
    readmeBoundaryDirectory,
    ['test/pack-consumer.mjs'],
    'README preserves the recorded compatibility boundary',
  )

  const differentialWiringDirectory = await prepareCase(
    'verify-m6-oracle-wiring',
  )
  await replaceOnce(
    differentialWiringDirectory,
    'package.json',
    ' && npm run test:m6oracle',
    '',
  )
  await expectKilled(
    'verify-m6-oracle-wiring',
    differentialWiringDirectory,
    ['test/pack-consumer.mjs'],
    'verify command includes the authenticated 0.84.2 M6 semantic gate',
  )

  const overlayWiringDirectory = await prepareCase(
    'verify-capability-overlay-wiring',
  )
  await replaceOnce(
    overlayWiringDirectory,
    'package.json',
    ' && npm run oracle:overlay',
    '',
  )
  await expectKilled(
    'verify-capability-overlay-wiring',
    overlayWiringDirectory,
    ['test/pack-consumer.mjs'],
    'verify command includes the authenticated 0.84.4 overlay gate',
  )

  const tier1WiringDirectory = await prepareCase(
    'verify-tier1-scroll-oracle-wiring',
  )
  await replaceOnce(
    tier1WiringDirectory,
    'package.json',
    ' && npm run test:tier1oracle',
    '',
  )
  await expectKilled(
    'verify-tier1-scroll-oracle-wiring',
    tier1WiringDirectory,
    ['test/pack-consumer.mjs'],
    'verify command includes the authenticated 0.84.2 Tier-1 scroll gate',
  )

  const selectionWiringDirectory = await prepareCase(
    'verify-selection-geometry-oracle-wiring',
  )
  await replaceOnce(
    selectionWiringDirectory,
    'package.json',
    ' && npm run test:selectionoracle',
    '',
  )
  await expectKilled(
    'verify-selection-geometry-oracle-wiring',
    selectionWiringDirectory,
    ['test/pack-consumer.mjs'],
    'verify command includes the authenticated selection geometry gate',
  )

  const wordCopyWiringDirectory = await prepareCase(
    'word-copy-oracle-standalone-wiring',
  )
  await replaceOnce(
    wordCopyWiringDirectory,
    'package.json',
    '  "test:wordcopyoracle": "node test/word-copy-oracle.mjs",',
    '  "test:wordcopyoracle": "node test/m6-semantic-oracle.mjs",',
  )
  await expectKilled(
    'word-copy-oracle-standalone-wiring',
    wordCopyWiringDirectory,
    ['test/pack-consumer.mjs'],
    'package preserves the standalone authenticated word/copy gate',
  )

  const licenseOmissionDirectory = await prepareCase('license-omission')
  await rm(join(licenseOmissionDirectory, 'LICENSE'))
  await expectKilled(
    'license-omission',
    licenseOmissionDirectory,
    ['test/pack-consumer.mjs'],
    'ENOENT',
  )

  const licenseAlterationDirectory = await prepareCase('license-alteration')
  await writeFile(
    join(licenseAlterationDirectory, 'LICENSE'),
    'MIT License\n\naltered mutation\n',
  )
  await expectKilled(
    'license-alteration',
    licenseAlterationDirectory,
    ['test/pack-consumer.mjs'],
    'package LICENSE matches the repo receipt',
  )

  const packDirectory = await prepareCase('native-artifact-omission')
  await replaceOnce(
    packDirectory,
    'package.json',
    '    "pie-tui-native.*.node",\n',
    '',
  )
  await expectKilled(
    'native-artifact-omission',
    packDirectory,
    ['test/pack-consumer.mjs'],
    'AssertionError',
  )
} finally {
  await rm(mutationRoot, { recursive: true, force: true })
}
