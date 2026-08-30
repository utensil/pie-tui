import assert from 'node:assert/strict'
import { Buffer, constants as bufferConstants } from 'node:buffer'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { homedir } from 'node:os'
import test from 'node:test'
import { pathToFileURL } from 'node:url'

import * as esm from '../index.js'
import fixture from './oracle-contract.json' with { type: 'json' }

const require = createRequire(import.meta.url)
const cjs = require('../index.cjs')
const marked = require('marked')
const expectedNames = Object.keys(fixture.runtimeTypes)

const fromUnits = (units) => String.fromCharCode(...units)
const toUnits = (value) =>
  Array.from({ length: value.length }, (_, index) => value.charCodeAt(index))

function applyEnvironment(values) {
  for (const [key, value] of Object.entries(values)) {
    if (value === null) {
      delete process.env[key]
    } else {
      process.env[key] = value
    }
  }
}

function checkCapabilitySnapshot(api, testCase) {
  const previous = new Map(
    fixture.capabilityEnvironmentKeys.map((key) => [key, process.env[key]]),
  )
  try {
    for (const key of fixture.capabilityEnvironmentKeys) delete process.env[key]
    applyEnvironment(testCase.initialEnvironment)
    let callbackCalls = 0
    const result = api.detectCapabilities(() => {
      callbackCalls += 1
      applyEnvironment(testCase.callbackMutations)
      return testCase.callbackResult
    })
    assert.deepEqual(result, testCase.result, testCase.label)
    assert.equal(callbackCalls, testCase.callbackCalls, testCase.label)
  } finally {
    for (const key of fixture.capabilityEnvironmentKeys) delete process.env[key]
    for (const [key, value] of previous) {
      if (value !== undefined) process.env[key] = value
    }
  }
}

function assertModuleSurface(module, label) {
  assert.deepEqual(Object.getOwnPropertyNames(module), expectedNames)
  assert.deepEqual(Object.getOwnPropertySymbols(module), [Symbol.toStringTag])
  assert.deepEqual(Object.getOwnPropertyDescriptor(module, Symbol.toStringTag), {
    value: 'Module',
    writable: false,
    enumerable: false,
    configurable: false,
  })
  assert.equal(Object.getPrototypeOf(module), null)
  assert.equal(Object.isExtensible(module), false)
  for (const [name, expectedType] of Object.entries(fixture.runtimeTypes)) {
    assert.equal(typeof module[name], expectedType, `${label} ${name} type`)
    const descriptor = Object.getOwnPropertyDescriptor(module, name)
    assert.ok(Object.hasOwn(descriptor, 'value'), `${label} ${name} data slot`)
    for (const [field, expected] of Object.entries(
      fixture.moduleExportDescriptor,
    )) {
      assert.equal(descriptor[field], expected, `${label} ${name} ${field}`)
    }
  }
  for (const [name, expected] of Object.entries(fixture.runtimeFunctions)) {
    assert.equal(module[name].name, expected.name, `${label} ${name} name`)
    assert.equal(module[name].length, expected.length, `${label} ${name} length`)
  }
  assert.equal(
    new Set(expectedNames.map((name) => module[name])).size,
    expectedNames.length,
    `${label} distinct export values`,
  )

  const firstName = expectedNames[0]
  const firstValue = module[firstName]
  assert.equal(Reflect.set(module, firstName, {}), false)
  assert.equal(
    Reflect.defineProperty(module, firstName, {
      value: {},
      writable: true,
      enumerable: true,
      configurable: false,
    }),
    false,
  )
  assert.equal(Reflect.deleteProperty(module, firstName), false)
  assert.equal(module[firstName], firstValue)
}

test('ESM and CJS expose the exact reviewed names, slots, and descriptors', () => {
  assertModuleSurface(esm, 'ESM')
  assertModuleSurface(cjs, 'CJS')
  for (const name of expectedNames) {
    assert.equal(esm[name], cjs[name], `shared loader slot ${name}`)
  }
  assert.equal(esm.Marked, marked.Marked)
  assert.deepEqual(Object.keys(esm.Key), fixture.keyKeys)
  for (const name of fixture.keyKeys) {
    const descriptor = Object.getOwnPropertyDescriptor(esm.Key, name)
    for (const [field, expected] of Object.entries(
      fixture.keyPropertyDescriptor,
    )) {
      assert.equal(descriptor[field], expected, `Key.${name} ${field}`)
    }
  }
  for (const [name, expected] of Object.entries(fixture.keyHelperFunctions)) {
    assert.equal(esm.Key[name].name, expected.name, `Key.${name} name`)
    assert.equal(esm.Key[name].length, expected.length, `Key.${name} length`)
  }
})

test('canonical constants retain exact content, descriptors, and shared identity', () => {
  assert.deepEqual(toUnits(esm.CURSOR_MARKER), [27, 95, 112, 105, 58, 99, 7])
  assert.equal(esm.CURSOR_MARKER, cjs.CURSOR_MARKER)
  assert.equal(esm.TUI_KEYBINDINGS, cjs.TUI_KEYBINDINGS)
  assert.equal(Object.keys(esm.TUI_KEYBINDINGS).length, 47)
  assert.equal(
    createHash('sha256')
      .update(JSON.stringify(esm.TUI_KEYBINDINGS))
      .digest('hex'),
    '4cb7c99127a022d9bdef391b871edc3a2d0f1325871695252410585ad27b1aea',
  )

  const cursorLeft = esm.TUI_KEYBINDINGS['tui.editor.cursorLeft']
  assert.equal(cursorLeft, cjs.TUI_KEYBINDINGS['tui.editor.cursorLeft'])
  assert.equal(
    cursorLeft.defaultKeys,
    cjs.TUI_KEYBINDINGS['tui.editor.cursorLeft'].defaultKeys,
  )
  assert.deepEqual(Object.getOwnPropertyDescriptor(cursorLeft, 'defaultKeys'), {
    value: cursorLeft.defaultKeys,
    writable: true,
    enumerable: true,
    configurable: true,
  })

  const originalDescription = cursorLeft.description
  cursorLeft.description = 'shared mutation receipt'
  assert.equal(
    cjs.TUI_KEYBINDINGS['tui.editor.cursorLeft'].description,
    'shared mutation receipt',
  )
  cursorLeft.description = originalDescription
})

test('fuzzy matching preserves UTF-16 scoring and callback reentrancy', () => {
  for (const [query, text, expected] of [
    ['fb', 'fooBar', { matches: true, score: -10.7 }],
    ['abc123', '123abc', { matches: true, score: -208.5 }],
    ['123abc', 'abc123', { matches: true, score: -208.5 }],
    ['😀', 'x😀', { matches: true, score: -4.7 }],
    ['\ud800', 'a\ud800', { matches: true, score: 0.1 }],
    ['', 'abc', { matches: true, score: 0 }],
    ['xz', 'abc', { matches: false, score: 0 }],
  ]) {
    assert.deepEqual(esm.fuzzyMatch(query, text), expected)
  }

  const first = { text: 'fooBar' }
  const second = { text: 'fizzBuzz' }
  const appended = { text: 'fab' }
  const items = [first, second]
  const callbacks = []
  const filtered = esm.fuzzyFilter(items, 'fb', (item) => {
    callbacks.push(item)
    if (item === first) {
      items.push(appended)
      assert.equal(
        esm.fuzzyFilter([{ text: 'nested' }], 'nes', (entry) => entry.text)[0]
          .text,
        'nested',
      )
    }
    return item.text
  })
  assert.deepEqual(callbacks, [first, second, appended])
  assert.deepEqual(filtered, [appended, first, second])
  assert.equal(esm.fuzzyFilter(items, '   ', () => assert.fail()), items)
})

test('Spacer keeps canonical prototypes while native state stays per instance', () => {
  assert.equal(esm.Spacer, cjs.Spacer)
  assert.deepEqual(Object.getOwnPropertyNames(esm.Spacer.prototype), [
    'constructor',
    'setLines',
    'invalidate',
    'render',
  ])
  const first = new esm.Spacer(2)
  const second = new cjs.Spacer(1)
  assert.equal(first instanceof esm.Spacer, true)
  assert.equal(first instanceof cjs.Spacer, true)
  assert.deepEqual(Object.getOwnPropertyDescriptor(first, 'lines'), {
    value: 2,
    writable: true,
    enumerable: true,
    configurable: true,
  })
  assert.deepEqual(first.render(80), ['', ''])
  assert.deepEqual(second.render(80), [''])
  first.setLines(1.25)
  second.setLines(3)
  assert.deepEqual(first.render(0), ['', ''])
  assert.deepEqual(second.render(0), ['', '', ''])
  assert.equal(first.invalidate(), undefined)
  assert.deepEqual(
    esm.Spacer.prototype.render.call({ lines: 2 }, 80),
    ['', ''],
  )
})

test('renderLatex crosses Node-API as exact raw UTF-16', () => {
  for (const testCase of fixture.latexCases) {
    const rendered = esm.renderLatex(fromUnits(testCase.input))
    assert.equal(typeof rendered, 'string', testCase.label)
    assert.deepEqual(toUnits(rendered), testCase.output, testCase.label)
  }
  assert.equal(esm.renderLatex('\\definitelyUnsupported{x}'), undefined)
  assert.throws(() => esm.renderLatex('x', null), TypeError)
})

test('undefined and null remain distinct at the public boundary', () => {
  assert.equal(esm.parseOsc11BackgroundColor('bad'), undefined)
  assert.equal(esm.parseTerminalColorSchemeReport('bad'), undefined)
  assert.equal(esm.parseKey(''), undefined)
  assert.equal(esm.decodeKittyPrintable(''), undefined)
  assert.equal(esm.getPngDimensions(''), null)
  assert.equal(esm.getImageDimensions('', 'image/png'), null)
})

test('cell and capability slots preserve caller identity across loaders', () => {
  const cell = { widthPx: 7, heightPx: 13 }
  esm.setCellDimensions(cell)
  assert.equal(esm.getCellDimensions(), cell)
  assert.equal(cjs.getCellDimensions(), cell)

  const capabilities = {
    images: 'kitty',
    trueColor: false,
    hyperlinks: true,
  }
  cjs.setCapabilities(capabilities)
  assert.equal(cjs.getCapabilities(), capabilities)
  assert.equal(esm.getCapabilities(), capabilities)

  esm.resetCapabilitiesCache()
  const detected = esm.getCapabilities()
  assert.ok(Object.hasOwn(detected, 'images'))
  assert.equal(detected.images, null)
  assert.equal(esm.getCapabilities(), detected)
  assert.equal(cjs.getCapabilities(), detected)

  esm.setCellDimensions({ widthPx: 9, heightPx: 18 })
  esm.resetCapabilitiesCache()
})

test('detectCapabilities snapshots all environment facts before the tmux callback', () => {
  for (const testCase of fixture.capabilitySnapshotCases) {
    checkCapabilitySnapshot(esm, testCase)
  }
})

test('image IDs and codec defaults match the reference receipts', () => {
  const originalRandom = Math.random
  try {
    Math.random = () => 0
    assert.equal(esm.allocateImageId(), 1)
    Math.random = () => 0.5
    assert.equal(esm.allocateImageId(), 0x80000000)
    Math.random = () => 1 - Number.EPSILON
    assert.equal(esm.allocateImageId(), 0xfffffffe)
  } finally {
    Math.random = originalRandom
  }

  assert.equal(esm.encodeKitty('QQ=='), '\u001b_Ga=T,f=100,q=2;QQ==\u001b\\')
  assert.equal(
    esm.encodeKitty('QQ==', { moveCursor: false }),
    '\u001b_Ga=T,f=100,q=2,C=1;QQ==\u001b\\',
  )
  assert.equal(
    esm.encodeKitty('QQ==', { imageId: 0 }),
    '\u001b_Ga=T,f=100,q=2;QQ==\u001b\\',
  )
  assert.equal(
    esm.encodeKitty('QQ==', { imageId: 0xffffffff }),
    '\u001b_Ga=T,f=100,q=2,i=4294967295;QQ==\u001b\\',
  )
  assert.equal(
    esm.encodeITerm2('QQ=='),
    '\u001b]1337;File=inline=1;size=1:QQ==\u0007',
  )
  assert.equal(
    esm.deleteKittyImage(0),
    '\u001b_Ga=d,d=I,i=0,q=2\u001b\\',
  )
  assert.equal(
    esm.deleteKittyImage(7),
    '\u001b_Ga=d,d=I,i=7,q=2\u001b\\',
  )
  assert.equal(
    esm.deleteKittyImage(0xffffffff),
    '\u001b_Ga=d,d=I,i=4294967295,q=2\u001b\\',
  )
  for (const imageId of [-1, 1.5, 0x100000000, NaN, Infinity, '7']) {
    assert.throws(() => esm.encodeKitty('QQ==', { imageId }), RangeError)
    assert.throws(() => esm.deleteKittyImage(imageId), RangeError)
  }
  for (const columns of [-1, 1.5, 0x100000000, NaN, Infinity, '7']) {
    assert.throws(() => esm.encodeKitty('QQ==', { columns }), RangeError)
  }
  assert.equal(esm.deleteAllKittyImages(), '\u001b_Ga=d,d=A,q=2\u001b\\')
  assert.equal(
    esm.calculateImageRows({ widthPx: 10, heightPx: 20 }, 2),
    2,
  )
})

test('renderImage reads live capability and cell facts with JavaScript Number semantics', () => {
  const capabilities = {
    images: null,
    trueColor: false,
    hyperlinks: false,
  }
  const dimensions = { widthPx: 9, heightPx: 18 }
  esm.setCapabilities(capabilities)
  esm.setCellDimensions(dimensions)
  assert.equal(esm.renderImage('QQ==', { widthPx: 18, heightPx: 36 }), null)

  capabilities.images = 'kitty'
  assert.deepEqual(
    esm.renderImage('QQ==', { widthPx: 18, heightPx: 36 }),
    {
      sequence: '\u001b_Ga=T,f=100,q=2,c=80,r=80;QQ==\u001b\\',
      columns: 80,
      rows: 80,
      imageId: undefined,
    },
  )
  assert.deepEqual(
    esm.renderImage('QQ==', { widthPx: 18, heightPx: 36 }, {
      maxWidthCells: null,
      maxHeightCells: null,
      imageId: 0,
      moveCursor: false,
    }),
    {
      sequence: '\u001b_Ga=T,f=100,q=2,C=1,c=1,r=1;QQ==\u001b\\',
      columns: 1,
      rows: 1,
      imageId: 0,
    },
  )
  const rawKitty = esm.renderImage(
    '\ud800',
    { widthPx: 18, heightPx: 36 },
    { maxWidthCells: 2, imageId: -7 },
  )
  assert.deepEqual(toUnits(rawKitty.sequence).slice(-3), [0xd800, 27, 92])
  assert.equal(rawKitty.imageId, -7)

  capabilities.images = 'iterm2'
  assert.deepEqual(
    esm.renderImage('QQ==', { widthPx: 18, heightPx: 36 }, {
      maxWidthCells: null,
      maxHeightCells: null,
      preserveAspectRatio: false,
    }),
    {
      sequence:
        '\u001b]1337;File=inline=1;size=1;width=1;height=auto;preserveAspectRatio=0:QQ==\u0007',
      columns: 1,
      rows: 1,
    },
  )

  esm.setCellDimensions({ widthPx: 9, heightPx: 18 })
  esm.resetCapabilitiesCache()
})

test('imageFallback snapshots live capability facts and Node path behavior', () => {
  const capabilities = {
    images: null,
    trueColor: false,
    hyperlinks: false,
  }
  esm.setCapabilities(capabilities)
  const absolute = `${homedir()}/folder/a b.png`
  assert.equal(
    esm.imageFallback('image/png', { widthPx: 12, heightPx: null }, absolute),
    '[Image: ~/folder/a b.png [image/png] 12xnull]',
  )
  capabilities.hyperlinks = true
  assert.equal(
    esm.imageFallback('image/png', undefined, absolute),
    `[Image: \u001b]8;;${pathToFileURL(absolute).href}\u001b\\~/folder/a b.png\u001b]8;;\u001b\\ [image/png]]`,
  )
  assert.equal(
    esm.imageFallback(null, { widthPx: 'x', heightPx: 0 }, ''),
    '[Image: [null] xx0]',
  )
  esm.resetCapabilitiesCache()
})

test('compositeTuiLine preserves ANSI, wide cells, raw UTF-16, and image identity', () => {
  for (const testCase of fixture.compositeTuiLineCases) {
    assert.deepEqual(
      toUnits(
        esm.compositeTuiLine(
          fromUnits(testCase.base),
          fromUnits(testCase.overlay),
          testCase.startCol,
          testCase.overlayWidth,
          testCase.totalWidth,
        ),
      ),
      testCase.output,
      testCase.label,
    )
  }
  const cases = [
    [
      ['abcdef', 'XY', 2, 2, 6],
      'ab\u001b[0m\u001b]8;;\u0007XY\u001b[0m\u001b]8;;\u0007ef',
    ],
    [
      ['a中bc', '😀', 1, 2, 6],
      'a\u001b[0m\u001b]8;;\u0007😀\u001b[0m\u001b]8;;\u0007bc ',
    ],
    [
      [
        '\u001b[31mabcdef\u001b[0m',
        '\u001b[4mXY\u001b[0m',
        2,
        2,
        7,
      ],
      '\u001b[31mab\u001b[0m\u001b]8;;\u0007\u001b[4mXY\u001b[0m\u001b]8;;\u0007\u001b[31mef\u001b[0m ',
    ],
    [
      ['abcdef', 'XY', 1.5, 2.5, 6.5],
      'a\u001b[0m\u001b]8;;\u0007XY\u001b[0m\u001b]8;;\u0007ef',
    ],
    [
      ['\ud800abc', '', 0, 0, 1],
      '\u001b[0m\u001b]8;;\u0007\u001b[0m\u001b]8;;\u0007\ud800a',
    ],
    [
      ['abcdef', '\udc00X', 1, 1, 5],
      'a\u001b[0m\u001b]8;;\u0007\udc00X\u001b[0m\u001b]8;;\u0007cde',
    ],
  ]
  for (const [args, expected] of cases) {
    assert.equal(Reflect.apply(esm.compositeTuiLine, null, args), expected)
  }
  const imageLine = 'prefix\u001b_Gx;y\u001b\\'
  assert.equal(esm.compositeTuiLine(imageLine, 'XY', 2, 2, 6), imageLine)
})

function canonicalEncodeKitty(base64Data, options = {}) {
  const chunkSize = 4096
  const params = ['a=T', 'f=100', 'q=2']
  if (options.moveCursor === false) params.push('C=1')
  if (options.columns) params.push(`c=${options.columns}`)
  if (options.rows) params.push(`r=${options.rows}`)
  if (options.imageId) params.push(`i=${options.imageId}`)
  if (base64Data.length <= chunkSize) {
    return `\u001b_G${params.join(',')};${base64Data}\u001b\\`
  }
  const chunks = []
  for (let offset = 0; offset < base64Data.length; offset += chunkSize) {
    const chunk = base64Data.slice(offset, offset + chunkSize)
    const isLast = offset + chunkSize >= base64Data.length
    if (offset === 0) {
      chunks.push(`\u001b_G${params.join(',')},m=1;${chunk}\u001b\\`)
    } else if (isLast) {
      chunks.push(`\u001b_Gm=0;${chunk}\u001b\\`)
    } else {
      chunks.push(`\u001b_Gm=1;${chunk}\u001b\\`)
    }
  }
  return chunks.join('')
}

test('Kitty chunks byte-exactly at 4096 JavaScript UTF-16 units', () => {
  for (const testCase of fixture.kittyUtf16Cases) {
    let input
    if (testCase.family === 'ascii') {
      input = 'A'.repeat(testCase.length)
    } else if (testCase.family === 'bmp') {
      input = 'é'.repeat(testCase.length)
    } else if (testCase.family === 'astral') {
      input =
        '😀'.repeat(Math.floor(testCase.length / 2)) +
        (testCase.length % 2 === 0 ? '' : 'x')
    } else {
      input = `${'x'.repeat(testCase.length - 2)}😀`
    }
    const expected = canonicalEncodeKitty(input)
    const actual = esm.encodeKitty(input)
    assert.equal(input.length, testCase.length, testCase.label)
    assert.ok(
      Buffer.from(actual, 'utf16le').equals(
        Buffer.from(expected, 'utf16le'),
      ),
      testCase.label,
    )
    assert.equal(
      createHash('sha256')
        .update(Buffer.from(actual, 'utf16le'))
        .digest('hex'),
      testCase.outputUtf16leSha256,
      `${testCase.label} pinned oracle digest`,
    )
  }

  const raw = fromUnits([0xd800, 0, 0xdc00])
  assert.deepEqual(
    toUnits(esm.encodeKitty(raw)),
    toUnits(canonicalEncodeKitty(raw)),
  )
})

test('image row calculation retains canonical JavaScript Number coercion', () => {
  const cases = [
    ['normal', [{ widthPx: 10, heightPx: 20 }, 2], 2],
    ['negative target', [{ widthPx: 10, heightPx: 20 }, -3], 1],
    ['fractional target', [{ widthPx: 10, heightPx: 20 }, 2.9], 2],
    [
      'above u32 target',
      [{ widthPx: 10, heightPx: 20 }, 4294967296],
      4294967296,
    ],
    ['string target', [{ widthPx: 10, heightPx: 20 }, '2.9'], 2],
    ['string fields', [{ widthPx: '10', heightPx: '20' }, 2], 2],
    ['null fields', [{ widthPx: null, heightPx: null }, 2], 1],
    ['negative fields', [{ widthPx: -10, heightPx: -20 }, 2], 1],
    ['fractional fields', [{ widthPx: 10.5, heightPx: 20.5 }, 2], 2],
    [
      'string cell fields',
      [
        { widthPx: 10, heightPx: 20 },
        2,
        { widthPx: '3', heightPx: '6' },
      ],
      2,
    ],
  ]
  for (const [label, args, expected] of cases) {
    assert.equal(Reflect.apply(esm.calculateImageRows, null, args), expected, label)
  }
  for (const [label, args] of [
    ['NaN target', [{ widthPx: 10, heightPx: 20 }, NaN]],
    ['missing target', [{ widthPx: 10, heightPx: 20 }]],
    ['missing width field', [{ heightPx: 20 }, 2]],
    ['missing height field', [{ widthPx: 10 }, 2]],
  ]) {
    assert.ok(
      Number.isNaN(Reflect.apply(esm.calculateImageRows, null, args)),
      label,
    )
  }
  assert.throws(
    () => esm.calculateImageRows({ widthPx: 10, heightPx: 20 }, 2, null),
    TypeError,
  )
  assert.throws(() => esm.calculateImageRows(null, 2), TypeError)

  esm.setCellDimensions({ widthPx: 1, heightPx: 999 })
  assert.equal(esm.calculateImageRows({ widthPx: 10, heightPx: 20 }, 2), 2)
  esm.setCellDimensions({ widthPx: 9, heightPx: 18 })
})

test('iTerm2 byte lengths are computed with Node Buffer base64 semantics', () => {
  const cases = [
    ['Q', 0],
    ['Q==', 0],
    ['\n', 0],
    ['é', 0],
    ['中', 0],
    ['😀', 1],
    ['💩QQ==', 3],
  ]
  for (const [value, size] of cases) {
    assert.equal(
      esm.encodeITerm2(value),
      `\u001b]1337;File=inline=1;size=${size}:${value}\u0007`,
      JSON.stringify(value),
    )
  }
})

test('padded truncation fails catchably before an impossible native allocation', () => {
  assert.equal(esm.truncateToWidth('x', 8, '...', true), 'x       ')
  assert.equal(
    esm.truncateToWidth('x', bufferConstants.MAX_STRING_LENGTH, '...', false),
    'x',
  )
  const child = spawnSync(process.execPath, ['test/boundary-child.mjs'], {
    cwd: new URL('..', import.meta.url),
    encoding: 'utf8',
    env: { ...process.env, NAPI_RS_NATIVE_LIBRARY_PATH: '' },
  })
  assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`)
  assert.match(child.stdout, /boundary child OK/)
})

test('color, key, hyperlink, and image parsers use the reviewed core', () => {
  assert.deepEqual(
    esm.parseOsc11BackgroundColor('\u001b]11;rgb:ffff/0000/7fff\u0007'),
    { r: 255, g: 0, b: 127 },
  )
  assert.equal(
    esm.parseTerminalColorSchemeReport('\u001b[?997;1n\u001b[?997;2n'),
    'light',
  )
  assert.deepEqual(Object.keys(esm.Key), fixture.keyKeys)
  assert.equal(esm.Key.ctrl('c'), 'ctrl+c')
  assert.equal(esm.Key.ctrlShift('p'), 'ctrl+shift+p')
  assert.equal(esm.parseKey('\u0003'), 'ctrl+c')
  assert.equal(esm.matchesKey('\u0003', 'ctrl+c'), true)
  assert.equal(esm.decodeKittyPrintable('\u001b[97u'), 'a')
  assert.equal(
    esm.hyperlink('docs', 'https://example.test'),
    '\u001b]8;;https://example.test\u001b\\docs\u001b]8;;\u001b\\',
  )
})

test('pure utilities are exact for well-formed strings and reject lossy input', () => {
  for (const testCase of fixture.wrapTextWithAnsiCases) {
    assert.deepEqual(
      esm
        .wrapTextWithAnsi(fromUnits(testCase.input), testCase.width)
        .map(toUnits),
      testCase.output,
      testCase.label,
    )
  }
  const styled = '\u001b[31mA\u001b[0m'
  assert.equal(esm.visibleWidth(styled), 1)
  assert.equal(esm.stripTerminalSequences(styled), 'A')
  assert.deepEqual(esm.wrapTextWithAnsi(styled, 5), [styled])
  assert.equal(esm.truncateToWidth(styled, 5), styled)
  assert.equal(esm.sliceByColumn(styled, 0, 5), styled)
  assert.equal(esm.visibleWidth('😀'), 2)
  assert.equal(esm.stripTerminalSequences('\u0000'), '\u0000')

  for (const operation of [
    () => esm.parseOsc11BackgroundColor('\ud800'),
    () => esm.parseTerminalColorSchemeReport('\ud800'),
    () => esm.encodeITerm2('QQ==', { width: '\ud800' }),
    () => esm.getPngDimensions('\ud800'),
    () => esm.hyperlink('\ud800', 'https://example.test'),
    () => esm.matchesKey('\ud800', 'ctrl+c'),
    () => esm.visibleWidth('\ud800'),
    () => esm.stripTerminalSequences('\ud800'),
    () => esm.wrapTextWithAnsi('\ud800', 5),
    () => esm.truncateToWidth('\ud800', 5),
    () => esm.sliceByColumn('\ud800', 0, 5),
  ]) {
    assert.throws(operation, RangeError)
  }
})
