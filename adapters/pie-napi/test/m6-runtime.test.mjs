import assert from 'node:assert/strict'
import test from 'node:test'

import {
  getCellDimensions,
  ScrollView,
  setCapabilities,
  Text,
  TuiAltScreen,
  TuiMainScreen,
  VStack,
  stripTerminalSequences,
} from '../index.js'

class RecordingTerminal {
  events = []
  columns = 12
  rows = 4
  kittyProtocolActive = false
  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize; this.events.push(['start']) }
  stop() { this.events.push(['stop']) }
  async drainInput() {}
  write(data) { this.events.push(['write', data]) }
  moveBy(lines) { this.events.push(['moveBy', lines]) }
  hideCursor() { this.events.push(['hideCursor']) }
  showCursor() { this.events.push(['showCursor']) }
  clearLine() { this.events.push(['clearLine']) }
  clearFromCursor() { this.events.push(['clearFromCursor']) }
  clearScreen() { this.events.push(['clearScreen']) }
  setTitle(title) { this.events.push(['setTitle', title]) }
  setProgress(active) { this.events.push(['setProgress', active]) }
}

test('M6 OSC 11 queries and color-scheme notifications consume terminal reports', async () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiMainScreen(terminal, false)
  const schemes = []
  tui.onTerminalColorSchemeChange((scheme) => schemes.push(scheme))
  tui.setTerminalColorSchemeNotifications(true)
  tui.start()
  const query = tui.queryTerminalBackgroundColor({ timeoutMs: 100 })
  terminal.onInput('\x1b]11;rgb:ffff/0000/0000\x07')
  assert.deepEqual(await query, { r: 255, g: 0, b: 0 })
  terminal.onInput('\x1b[?997;1n')
  assert.deepEqual(schemes, ['dark'])
  tui.stop({ preserveScreen: true })
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value)
  assert.ok(writes.includes('\x1b[?2031h'))
  assert.ok(writes.includes('\x1b]11;?\x07'))
  assert.ok(writes.includes('\x1b[?2031l'))
})

test('M6 terminal queries preserve DSR, cell-size, and hex-color behavior', async () => {
  setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const tui = new TuiMainScreen(terminal, false)
  const scheme = tui.queryTerminalColorScheme({ timeoutMs: 100 })
  terminal.onInput?.('\x1b[?997;2n')
  tui.handleTerminalInput('\x1b[?997;2n')
  tui.start()
  terminal.onInput('\x1b[6;21;9t')
  const color = tui.queryTerminalBackgroundColor({ timeoutMs: 100 })
  terminal.onInput('\x1b]11;#ff0080\x07')
  assert.equal(await scheme, 'light')
  assert.deepEqual(await color, { r: 255, g: 0, b: 128 })
  assert.deepEqual(getCellDimensions(), { widthPx: 9, heightPx: 21 })
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value)
  assert.ok(writes.includes('\x1b[?996n'))
  assert.ok(writes.includes('\x1b[16t'))
  tui.stop({ preserveScreen: true })
  setCapabilities({ images: null, trueColor: true, hyperlinks: true })
})

test('M6 overlays honor width, anchor, margin, visibility, and focus', () => {
  const terminal = new RecordingTerminal()
  terminal.columns = 10
  terminal.rows = 3
  const tui = new TuiMainScreen(terminal, false)
  tui.addChild(new Text('base'))
  const overlay = new Text('O')
  const handle = tui.showOverlay(overlay, {
    width: 3,
    anchor: 'top-right',
    margin: { top: 1, right: 1 },
  })
  tui.start()
  tui.renderNow(true)
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => stripTerminalSequences(value))
  assert.ok(writes.some((value) => value.includes(' O ')))
  assert.equal(handle.isFocused(), true)
  handle.setHidden(true)
  assert.equal(tui.hasOverlay(), false)
})

test('M6 MainScreen is native-planned, differential, synchronized, and stopped-safe', () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiMainScreen(terminal, false)
  const text = new Text('one')
  tui.addChild(text)
  tui.start()
  tui.renderNow(true)
  text.setText('two')
  tui.renderNow()
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value)
  assert.equal(terminal.events.some(([kind]) => kind === 'clearScreen'), false)
  assert.ok(writes.filter((value) => value.includes('\x1b[?2026h')).length >= 2)
  assert.ok(writes.some((value) => value.includes('\x1b[2K two')))
  tui.stop({ preserveScreen: true })
  terminal.events = []
  tui.renderNow(true)
  assert.deepEqual(terminal.events, [])
})

test('M6 AltScreen defaults mouse on and emits native-planned synchronized diffs', () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiAltScreen(terminal, false)
  tui.addChild(new Text('alt'))
  tui.start()
  tui.renderNow(true)
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value)
  assert.ok(writes[0].includes('\x1b[?1049h'))
  assert.ok(writes[0].includes('\x1b[?1000h'))
  assert.ok(writes.some((value) => value.startsWith('\x1b[?2026h')))
  tui.stop({ preserveScreen: true })
  assert.ok(terminal.events.some(([kind, value]) => kind === 'write' && value.includes('\x1b[?1049l')))
})

test('M6 AltScreen allocates fullscreen VStack growth to the primary ScrollView', () => {
  const terminal = new RecordingTerminal()
  terminal.columns = 20
  terminal.rows = 6
  const transcript = new ScrollView(
    new Text('line-0\nline-1\nline-2\nline-3\nline-4\nline-5\nline-6'),
    { follow: 'end', primary: true },
  )
  const root = new VStack([
    { component: transcript, basis: 0, grow: 1, shrink: 1, minSize: 1 },
    { component: new Text('dock'), basis: 'auto', grow: 0, shrink: 0, minSize: 1 },
  ])
  const tui = new TuiAltScreen(terminal, false)
  tui.setLayoutRoot(root)
  tui.start()
  tui.renderNow(true)
  const output = stripTerminalSequences(
    terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value).join(''),
  )
  assert.match(output, /line-6/)
  assert.match(output, /dock/)
  assert.equal(transcript.viewportHeight, 3)
  assert.equal(transcript.scrollTop, 6)
  tui.stop({ preserveScreen: true })
})

test('M6 ScrollView disableFollow suppresses reattachment at the current end', () => {
  const view = new ScrollView(new Text('0\n1\n2\n3\n4'), { follow: 'end' })
  view.updateLayout(5, 2, () => {})
  assert.equal(view.scrollTop, 3)
  view.scrollTo(3, { disableFollow: true })
  assert.equal(view.isFollowingEnd, false)
  view.updateLayout(6, 2, () => {})
  assert.equal(view.scrollTop, 3)
  assert.equal(view.isFollowingEnd, false)
})
