import assert from 'node:assert/strict'
import { pathToFileURL } from 'node:url'
import { join } from 'node:path'

const distribution = process.env.PI_TUI_DIST
assert.ok(distribution, 'PI_TUI_DIST must point to the pinned 0.84.2 dist')
assert.equal(process.version, 'v24.4.1', 'differential gate requires Node 24.4.1')

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')
const addedNames = [
  'CURSOR_MARKER',
  'Spacer',
  'TUI_KEYBINDINGS',
  'compositeTuiLine',
  'fuzzyFilter',
  'fuzzyMatch',
  'imageFallback',
  'renderImage',
]

const identity = (text) => text
const selectTheme = {
  selectedPrefix: identity,
  selectedText: identity,
  description: identity,
  scrollInfo: identity,
  noMatch: identity,
}
const markdownTheme = {
  heading: identity,
  link: identity,
  linkUrl: identity,
  code: identity,
  codeBlock: identity,
  codeBlockBorder: identity,
  quote: identity,
  quoteBorder: identity,
  hr: identity,
  listBullet: identity,
  bold: identity,
  italic: identity,
  strikethrough: identity,
  underline: identity,
}

function capture(operation) {
  try {
    return { value: operation() }
  } catch (error) {
    return { error: error?.name }
  }
}

for (const name of addedNames) {
  assert.equal(typeof adapter[name], typeof reference[name], `${name} type`)
  if (typeof reference[name] === 'function') {
    assert.equal(adapter[name].name, reference[name].name, `${name} name`)
    assert.equal(adapter[name].length, reference[name].length, `${name} length`)
  }
}

for (const name of Object.keys(reference)) {
  assert.equal(typeof adapter[name], typeof reference[name], `${name} M5 type`)
  if (typeof reference[name] === 'function') {
    assert.equal(adapter[name].name, reference[name].name, `${name} M5 name`)
    assert.equal(adapter[name].length, reference[name].length, `${name} M5 length`)
  }
}

for (const [name, createAdapter, createReference] of [
  ['Text', () => new adapter.Text('ab', 1, 1), () => new reference.Text('ab', 1, 1)],
  ['TruncatedText', () => new adapter.TruncatedText('abcdef', 1), () => new reference.TruncatedText('abcdef', 1)],
  ['Input', () => new adapter.Input(), () => new reference.Input()],
]) {
  const actual = createAdapter()
  const expected = createReference()
  if (name === 'Input') {
    actual.focused = expected.focused = true
    actual.setValue('ab')
    expected.setValue('ab')
    actual.handleInput('c')
    expected.handleInput('c')
    assert.equal(actual.getValue(), expected.getValue(), `${name} state`)
  }
  for (const width of [1, 5, 12]) {
    assert.deepEqual(actual.render(width), expected.render(width), `${name} ${width}`)
  }
}

for (const [name, makeAdapter, makeReference] of [
  [
    'Box',
    () => { const value = new adapter.Box(1, 1); value.addChild(new adapter.Text('a')); return value },
    () => { const value = new reference.Box(1, 1); value.addChild(new reference.Text('a')); return value },
  ],
  [
    'VStack',
    () => new adapter.VStack([new adapter.Text('a'), new adapter.Text('b')], { gap: 1 }),
    () => new reference.VStack([new reference.Text('a'), new reference.Text('b')], { gap: 1 }),
  ],
  [
    'HStack',
    () => new adapter.HStack([new adapter.Text('a'), new adapter.Text('b')], { gap: 1 }),
    () => new reference.HStack([new reference.Text('a'), new reference.Text('b')], { gap: 1 }),
  ],
]) {
  const actual = makeAdapter()
  const expected = makeReference()
  assert.deepEqual(actual.render(8), expected.render(8), `${name} render`)
}

const actualSelect = new adapter.SelectList(
  [{ value: 'alpha', label: 'Alpha' }, { value: 'beta', label: 'Beta' }],
  2,
  selectTheme,
)
const expectedSelect = new reference.SelectList(
  [{ value: 'alpha', label: 'Alpha' }, { value: 'beta', label: 'Beta' }],
  2,
  selectTheme,
)
actualSelect.setFilter('b')
expectedSelect.setFilter('b')
assert.deepEqual(actualSelect.render(20), expectedSelect.render(20), 'SelectList render')

const actualMarkdown = new adapter.Markdown('# M5', 0, 0, markdownTheme)
const expectedMarkdown = new reference.Markdown('# M5', 0, 0, markdownTheme)
assert.deepEqual(actualMarkdown.render(12), expectedMarkdown.render(12), 'Markdown render')

const tuiHost = { terminal: { rows: 24 }, requestRender() {} }
const actualEditor = new adapter.Editor(tuiHost, {
  borderColor: identity,
  selectList: selectTheme,
})
const expectedEditor = new reference.Editor(tuiHost, {
  borderColor: identity,
  selectList: selectTheme,
})
actualEditor.setText('hello')
expectedEditor.setText('hello')
actualEditor.insertTextAtCursor('!')
expectedEditor.insertTextAtCursor('!')
assert.deepEqual(actualEditor.getCursor(), expectedEditor.getCursor(), 'Editor cursor')
assert.deepEqual(actualEditor.render(20), expectedEditor.render(20), 'Editor render')

assert.equal(adapter.CURSOR_MARKER, reference.CURSOR_MARKER)
assert.deepEqual(adapter.TUI_KEYBINDINGS, reference.TUI_KEYBINDINGS)
for (const id of Object.keys(reference.TUI_KEYBINDINGS)) {
  assert.deepEqual(
    Object.getOwnPropertyDescriptors(adapter.TUI_KEYBINDINGS[id]),
    Object.getOwnPropertyDescriptors(reference.TUI_KEYBINDINGS[id]),
    `${id} descriptors`,
  )
}

const fuzzyCorpus = [
  '',
  'a',
  'A',
  'abc',
  'a-b',
  'a b',
  'abc123',
  '123abc',
  '😀',
  'x😀',
  '\ud800',
  'a\ud800',
  'İ',
  'i̇',
  'ß',
  'Σ',
  'ς',
  '中',
  'a/b',
  'fooBar',
  'FOOBAR',
  '\u00a0x',
  '\u200bx',
]
for (const query of fuzzyCorpus) {
  for (const text of fuzzyCorpus) {
    assert.deepEqual(
      adapter.fuzzyMatch(query, text),
      reference.fuzzyMatch(query, text),
      `fuzzyMatch ${JSON.stringify(query)} ${JSON.stringify(text)}`,
    )
  }
}

function runReentrantFilter(api) {
  const items = [
    { id: 'first', text: 'fooBar' },
    { id: 'second', text: 'fizzBuzz' },
  ]
  const appended = { id: 'appended', text: 'fab' }
  const callbacks = []
  const output = api.fuzzyFilter(items, 'fb', (item) => {
    callbacks.push(item.id)
    if (item.id === 'first') {
      items.push(appended)
      api.fuzzyFilter([{ text: 'nested' }], 'nes', (entry) => entry.text)
    }
    return item.text
  })
  return {
    callbacks,
    items: items.map((item) => item.id),
    output: output.map((item) => item.id),
  }
}
assert.deepEqual(runReentrantFilter(adapter), runReentrantFilter(reference))

assert.deepEqual(
  Object.getOwnPropertyNames(adapter.Spacer.prototype),
  Object.getOwnPropertyNames(reference.Spacer.prototype),
)
for (const lines of [undefined, 0, -1, 0.1, 1, 1.1, 2, NaN, null, '2']) {
  const expected =
    lines === undefined ? new reference.Spacer() : new reference.Spacer(lines)
  const actual = lines === undefined ? new adapter.Spacer() : new adapter.Spacer(lines)
  assert.deepEqual(actual.render(80), expected.render(80), `Spacer ${String(lines)}`)
  assert.deepEqual(
    Object.getOwnPropertyDescriptor(actual, 'lines'),
    Object.getOwnPropertyDescriptor(expected, 'lines'),
    `Spacer ${String(lines)} descriptor`,
  )
}

const renderDimensions = [
  { widthPx: 10, heightPx: 20 },
  { widthPx: '10', heightPx: '20' },
  { widthPx: 0, heightPx: 0 },
  { widthPx: NaN, heightPx: 5 },
]
const renderOptions = [
  {},
  { maxWidthCells: 2 },
  { maxWidthCells: '2.9' },
  { maxHeightCells: 2, imageId: 0, moveCursor: false },
  { imageId: -1 },
  { imageId: '7' },
  { preserveAspectRatio: false },
]
for (const protocol of [null, 'kitty', 'iterm2', 'bogus']) {
  for (const dimensions of renderDimensions) {
    for (const options of renderOptions) {
      reference.setCapabilities({
        images: protocol,
        trueColor: true,
        hyperlinks: true,
      })
      adapter.setCapabilities({
        images: protocol,
        trueColor: true,
        hyperlinks: true,
      })
      assert.deepEqual(
        capture(() => adapter.renderImage('QQ==', dimensions, options)),
        capture(() => reference.renderImage('QQ==', dimensions, options)),
        `renderImage ${protocol} ${JSON.stringify(dimensions)} ${JSON.stringify(options)}`,
      )
    }
  }
}

for (const hyperlinks of [false, true]) {
  for (const args of [
    ['image/png', null, undefined],
    ['image/png', { widthPx: 1, heightPx: 2 }, '/tmp/a b.png'],
    ['image/png', { widthPx: 'x', heightPx: null }, 'relative.png'],
    ['', {}, ''],
    [null, null, null],
  ]) {
    reference.setCapabilities({ images: null, trueColor: false, hyperlinks })
    adapter.setCapabilities({ images: null, trueColor: false, hyperlinks })
    assert.deepEqual(
      capture(() => Reflect.apply(adapter.imageFallback, null, args)),
      capture(() => Reflect.apply(reference.imageFallback, null, args)),
      `imageFallback ${hyperlinks} ${JSON.stringify(args)}`,
    )
  }
}

const compositeLines = [
  '',
  'abcdef',
  '中abcdef',
  'a😀bc',
  '\u001b[31mabcdef\u001b[0m',
  '\u001b]8;;https://x\u0007abcdef\u001b]8;;\u0007',
  '\ud800abc',
  'abc\udc00',
  '\u001b_Gx;y\u001b\\',
  '\u0001\u0002\u0003\u0004\ud800abc',
]
const compositeNumbers = [0, 1, 2, 2.5, -1, NaN, null, undefined]
let compositeCases = 0
for (const testCase of [
  ['a\ud800bc', '\udc00X', 0, 0.1, 1],
  ['abcdef', 'XY', 0, 1, 0.5],
  ['a\ud800中bc', '\udc00X', 0, 0, 1.9],
  ['a\ud800中bc', '\udc00X', 0, Number.MIN_VALUE, 0],
]) {
  assert.deepEqual(
    capture(() => Reflect.apply(adapter.compositeTuiLine, null, testCase)),
    capture(() => Reflect.apply(reference.compositeTuiLine, null, testCase)),
    `compositeTuiLine pinned fractional case ${compositeCases}`,
  )
  compositeCases += 1
}
for (const base of compositeLines) {
  for (const overlay of compositeLines.slice(0, 8)) {
    for (const start of compositeNumbers) {
      for (const width of compositeNumbers) {
        for (const total of [0, 1, 3, 5, 8, 8.5, null, undefined]) {
          const args = [base, overlay, start, width, total]
          assert.deepEqual(
            capture(() => Reflect.apply(adapter.compositeTuiLine, null, args)),
            capture(() => Reflect.apply(reference.compositeTuiLine, null, args)),
            `compositeTuiLine case ${compositeCases}`,
          )
          compositeCases += 1
        }
      }
    }
  }
}

const fractionalCompositePairs = [
  ['abcdef', 'XY'],
  ['a中bc', '😀'],
  ['a\u0301bc', '\u0301X'],
  ['a\ud800bc', '\udc00X'],
  ['\u001b[31mabcdef\u001b[0m', '\u001b[4mXY\u001b[0m'],
]
const fractionalCompositeNumbers = [
  0,
  -0,
  Number.MIN_VALUE,
  0.1,
  0.5,
  1.9,
  NaN,
  Infinity,
  -Infinity,
]
for (const [base, overlay] of fractionalCompositePairs) {
  for (const start of fractionalCompositeNumbers) {
    for (const width of fractionalCompositeNumbers) {
      for (const total of fractionalCompositeNumbers) {
        const args = [base, overlay, start, width, total]
        assert.deepEqual(
          capture(() => Reflect.apply(adapter.compositeTuiLine, null, args)),
          capture(() => Reflect.apply(reference.compositeTuiLine, null, args)),
          `compositeTuiLine fractional matrix case ${compositeCases}`,
        )
        compositeCases += 1
      }
    }
  }
}

const crossSegmentSurrogatePairs = [
  ['\ud800', '\udc00'],
  ['a\ud800', '\udc00b'],
  ['\udc00', '\ud800'],
  ['\u001b[31m\ud800\u001b[0m', '\u001b[4m\udc00\u001b[0m'],
  ['\ud800\u0301', '\udc00\u0301'],
  ['\ud800', '\ud800\udc00'],
]
const crossSegmentBoundaryNumbers = [
  -Number.MIN_VALUE,
  -0,
  0,
  Number.MIN_VALUE,
  0.1,
  0.5,
  1,
  1.9,
]
for (const [base, overlay] of crossSegmentSurrogatePairs) {
  for (const start of crossSegmentBoundaryNumbers) {
    for (const width of crossSegmentBoundaryNumbers) {
      for (const total of crossSegmentBoundaryNumbers) {
        const args = [base, overlay, start, width, total]
        assert.deepEqual(
          capture(() => Reflect.apply(adapter.compositeTuiLine, null, args)),
          capture(() => Reflect.apply(reference.compositeTuiLine, null, args)),
          `compositeTuiLine cross-segment surrogate case ${compositeCases}`,
        )
        compositeCases += 1
      }
    }
  }
}

const embeddedCompositeStyleCases = [
  ['\u001b[?25h\u001b[31m\udc00', '', 0, 0, 1],
  ['\u001b[?25l\u001b[1m\ud800', '', 0, 0, 1],
  ['\u001b[?1049h\u001b[38;5;240mA', '', 0, 0, 1],
  ['\u001b[31mA\u001b[?25h\u001b[0mB', '', 1, 0, 2],
  ['\u001b[31mA\u001b[?25l\u001b[39mB', '', 1, 0, 2],
]
for (const args of embeddedCompositeStyleCases) {
  assert.deepEqual(
    capture(() => Reflect.apply(adapter.compositeTuiLine, null, args)),
    capture(() => Reflect.apply(reference.compositeTuiLine, null, args)),
    `compositeTuiLine embedded ANSI style case ${compositeCases}`,
  )
  compositeCases += 1
}

const partialTokenPairs = [
  ['\ud800', ''],
  ['\udc00', 'X'],
  ['\ud800', 'a'],
  ['\ud800\u0301', ''],
  ['\udc00\ufe0f', 'X'],
  ['\ud800\u200d', 'a'],
  ['\ud800\u093e', ''],
  ['a\ud800', '\udc00'],
  ['\u0001\u0002\u0003\u0004\ud800', 'X'],
  ['\ud800', '\udc00'],
]
const partialTokenStarts = [
  -Number.MIN_VALUE,
  -0,
  Number.MIN_VALUE,
  0.1,
  1.9,
]
const partialTokenWidths = [
  -Infinity,
  -1.9,
  -1,
  -0.5,
  -Number.MIN_VALUE,
  -0,
  Number.MIN_VALUE,
  0.1,
  0.5,
  1,
  1.9,
  Infinity,
  NaN,
]
const partialTokenTotals = [
  -Number.MIN_VALUE,
  -0,
  0,
  Number.MIN_VALUE,
  0.1,
  0.5,
  1,
  1.9,
]
for (const [base, overlay] of partialTokenPairs) {
  for (const start of partialTokenStarts) {
    for (const width of partialTokenWidths) {
      for (const total of partialTokenTotals) {
        const args = [base, overlay, start, width, total]
        assert.deepEqual(
          capture(() => Reflect.apply(adapter.compositeTuiLine, null, args)),
          capture(() => Reflect.apply(reference.compositeTuiLine, null, args)),
          `compositeTuiLine atomic surrogate token case ${compositeCases}`,
        )
        compositeCases += 1
      }
    }
  }
}

adapter.setCellDimensions({ widthPx: 9, heightPx: 18 })
adapter.resetCapabilitiesCache()
reference.setCellDimensions({ widthPx: 9, heightPx: 18 })
reference.resetCapabilitiesCache()
console.log(
  `differential OK: ${addedNames.length} additions, ${fuzzyCorpus.length ** 2} fuzzy cases, ${compositeCases} composite cases`,
)
