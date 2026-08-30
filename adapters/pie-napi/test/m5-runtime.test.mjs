import assert from 'node:assert/strict'
import test from 'node:test'

import {
  Box,
  CombinedAutocompleteProvider,
  Container,
  Editor,
  HStack,
  Input,
  Markdown,
  ScrollView,
  SelectList,
  SettingsList,
  StdinBuffer,
  Text,
  TuiAltScreen,
  TuiMainScreen,
  VStack,
  isFocusable,
  isViewportTUI,
} from '../index.js'

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

class RecordingTerminal {
  writes = []
  starts = 0
  stops = 0
  columns = 20
  rows = 4
  kittyProtocolActive = false

  start(onInput, onResize) {
    this.starts += 1
    this.onInput = onInput
    this.onResize = onResize
  }
  stop() { this.stops += 1 }
  async drainInput() {}
  write(data) { this.writes.push(data) }
  moveBy(lines) { this.writes.push(`move:${lines}`) }
  hideCursor() { this.writes.push('hide') }
  showCursor() { this.writes.push('show') }
  clearLine() { this.writes.push('clear-line') }
  clearFromCursor() { this.writes.push('clear-from-cursor') }
  clearScreen() { this.writes.push('clear-screen') }
  setTitle(title) { this.writes.push(`title:${title}`) }
  setProgress(active) { this.writes.push(`progress:${active}`) }
}

test('M5 component facades preserve native state and mutable object graphs', () => {
  const input = new Input()
  const submits = []
  input.onSubmit = (value) => submits.push(value)
  input.focused = true
  input.setValue('ab')
  input.handleInput('c')
  input.handleInput('\r')
  assert.equal(input.getValue(), 'cab')
  assert.deepEqual(submits, ['cab'])
  assert.equal(isFocusable(input), true)

  const box = new Box(1, 1)
  const child = new Text('native')
  box.addChild(child)
  assert.equal(box.children[0], child)
  assert.deepEqual(box.render(12), [
    '            ',
    '            ',
    '  native    ',
    '            ',
    '            ',
  ])

  const vertical = new VStack([new Text('a'), new Text('b')], { gap: 1 })
  const horizontal = new HStack([new Text('a'), new Text('b')], { gap: 1 })
  assert.ok(vertical.render(8).some((line) => line.includes('a')))
  assert.ok(horizontal.render(8).some((line) => line.includes('b')))

  const scroll = new ScrollView(new Text('one\ntwo\nthree'), {
    follow: 'end',
    scrollbar: 'always',
  })
  scroll.updateLayout(3, 2, () => {})
  assert.equal(scroll.scrollTop, 1)
  assert.equal(scroll.getContentWidth(8), 7)
  assert.throws(() => scroll.addChild(new Text('extra')), /exactly one child/)
})

test('M5 interactive facades retain callbacks, themes, and completion identity', async () => {
  const selected = []
  const list = new SelectList(
    [{ value: 'alpha', label: 'Alpha' }, { value: 'beta', label: 'Beta' }],
    2,
    selectTheme,
  )
  list.onSelect = (item) => selected.push(item)
  list.handleInput('\r')
  assert.equal(selected[0], list.getSelectedItem())

  const changes = []
  const settings = new SettingsList(
    [{ id: 'mode', label: 'Mode', currentValue: 'a', values: ['a', 'b'] }],
    3,
    { label: identity, value: identity, description: identity, cursor: '> ', hint: identity },
    (id, value) => changes.push([id, value]),
    () => {},
  )
  settings.handleInput('\r')
  assert.deepEqual(changes, [['mode', 'b']])

  const provider = new CombinedAutocompleteProvider(
    [{ name: 'help', description: 'show help' }],
    process.cwd(),
  )
  const suggestions = await provider.getSuggestions(['/he'], 0, 3, {
    signal: new AbortController().signal,
  })
  assert.equal(suggestions.items[0].value, 'help')
  assert.deepEqual(
    provider.applyCompletion(['/he'], 0, 3, suggestions.items[0], '/he'),
    { lines: ['/help '], cursorLine: 0, cursorCol: 6 },
  )

  const requests = []
  const tui = { requestRender: (force) => requests.push(force) }
  const editor = new Editor(tui, { borderColor: identity, selectList: selectTheme })
  editor.setText('hello')
  editor.insertTextAtCursor('!')
  assert.equal(editor.getText(), 'hello!')
  assert.deepEqual(editor.getCursor(), { line: 0, col: 6 })
  assert.ok(requests.length >= 2)

  const markdown = new Markdown('# M5', 0, 0, markdownTheme)
  assert.deepEqual(markdown.render(8), ['M5      '])
})

test('StdinBuffer owns idle flush and paste delivery without leaked timers', async () => {
  const buffer = new StdinBuffer({ timeout: 5 })
  const data = []
  const pastes = []
  buffer.on('data', (value) => data.push(value))
  buffer.on('paste', (value) => pastes.push(value))
  buffer.process('\x1b')
  await new Promise((resolve) => setTimeout(resolve, 12))
  buffer.process('\x1b[200~pasted\x1b[201~')
  assert.deepEqual(data, ['\x1b'])
  assert.deepEqual(pastes, ['pasted'])
  buffer.destroy()
  assert.equal(buffer.getBuffer(), '')
})

test('main and alternate screen lifecycle renders and tears down deterministically', () => {
  const mainTerminal = new RecordingTerminal()
  const main = new TuiMainScreen(mainTerminal, false)
  const focused = new Input()
  main.addChild(new Text('main'))
  main.setFocus(focused)
  const overlay = new Text('overlay')
  const handle = main.showOverlay(overlay)
  assert.equal(main.hasOverlay(), true)
  handle.setHidden(true)
  assert.equal(main.hasOverlay(), false)
  handle.setHidden(false)
  main.hideOverlay()
  assert.equal(main.hasOverlay(), false)
  assert.equal(focused.focused, true)
  main.start()
  main.renderNow(true)
  main.stop()
  assert.equal(mainTerminal.starts, 1)
  assert.equal(mainTerminal.stops, 1)
  assert.ok(mainTerminal.writes.some((write) => write.includes?.('\x1b[?2026h')))
  assert.equal(mainTerminal.writes.includes('clear-screen'), false)

  const altTerminal = new RecordingTerminal()
  const alt = new TuiAltScreen(altTerminal, false, undefined, { mouse: true })
  alt.setLayoutRoot(new Text('alt'))
  assert.equal(isViewportTUI(alt), true)
  alt.start()
  alt.renderNow(true)
  alt.stop()
  assert.equal(altTerminal.starts, 1)
  assert.equal(altTerminal.stops, 1)
  assert.ok(altTerminal.writes.some((write) => write.includes?.('\x1b[?1049h')))
  assert.ok(altTerminal.writes.some((write) => write.includes?.('\x1b[?1049l')))
  assert.ok(altTerminal.writes.some((write) => write.includes?.('\x1b[?1000h')))
  assert.ok(altTerminal.writes.some((write) => write.includes?.('\x1b[?1000l')))
})
