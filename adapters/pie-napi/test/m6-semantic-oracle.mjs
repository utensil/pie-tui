import assert from 'node:assert/strict'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const distribution = process.env.PI_TUI_DIST
assert.ok(distribution, 'PI_TUI_DIST must point to authenticated pi-tui 0.84.2 dist')
const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

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

function withStableTerminalEnvironment(callback) {
  const saved = new Map(['TERM', 'TMUX', 'ZELLIJ', 'STY'].map((name) => [name, process.env[name]]))
  process.env.TERM = 'xterm-256color'
  delete process.env.TMUX
  delete process.env.ZELLIJ
  delete process.env.STY
  try { return callback() } finally {
    for (const [name, value] of saved) value === undefined ? delete process.env[name] : process.env[name] = value
  }
}

function mainReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const tui = new api.TuiMainScreen(terminal, false)
  const text = new api.Text('one')
  tui.addChild(text)
  tui.start()
  tui.renderNow(true)
  text.setText('two')
  tui.renderNow()
  tui.stop({ preserveScreen: true })
  return terminal.events
}

function mainStateReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const tui = new api.TuiMainScreen(terminal, false)
  tui.addChild(new api.Text('one\ntwo\nthree\nfour\nfive'))
  tui.start()
  tui.renderNow(true)
  const captured = tui.captureRenderState()
  tui.stop()
  return { captured, events: terminal.events }
}

function altReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const tui = new api.TuiAltScreen(terminal, false)
  tui.addChild(new api.Text('alt'))
  tui.start()
  tui.renderNow(true)
  tui.stop({ preserveScreen: true })
  return terminal.events
}

async function colorReceipt(api) {
  const terminal = new RecordingTerminal()
  const tui = new api.TuiMainScreen(terminal, false)
  const query = tui.queryTerminalBackgroundColor({ timeoutMs: 100 })
  tui.handleTerminalInput('\x1b]11;rgb:ffff/0000/0000\x07')
  return { color: await query, events: terminal.events }
}

async function terminalReceipt(api) {
  api.setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const tui = new api.TuiMainScreen(terminal, false)
  const scheme = tui.queryTerminalColorScheme({ timeoutMs: 100 })
  tui.handleTerminalInput('\x1b[?997;2n')
  tui.start()
  tui.handleTerminalInput('\x1b[6;21;9t')
  const hex = tui.queryTerminalBackgroundColor({ timeoutMs: 100 })
  tui.handleTerminalInput('\x1b]11;#ff0080\x07')
  const receipt = {
    scheme: await scheme,
    color: await hex,
    dimensions: api.getCellDimensions(),
    events: terminal.events,
  }
  tui.stop({ preserveScreen: true })
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  return receipt
}

function overlayReceipt(api) {
  const terminal = new RecordingTerminal()
  terminal.columns = 10
  terminal.rows = 3
  const tui = new api.TuiMainScreen(terminal, false)
  tui.addChild(new api.Text('base'))
  const overlay = new api.Text('O')
  const handle = tui.showOverlay(overlay, {
    width: 3,
    anchor: 'top-right',
    margin: { top: 1, right: 1 },
  })
  tui.start()
  tui.renderNow(true)
  const visible = [...terminal.events]
  const focused = handle.isFocused()
  handle.setHidden(true)
  tui.stop({ preserveScreen: true })
  return { visible, focused, hidden: handle.isHidden(), hasOverlay: tui.hasOverlay() }
}

function altScrollReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = 8
  terminal.rows = 3
  const tui = new api.TuiAltScreen(terminal, false)
  tui.addChild(new api.Text('0\n1\n2\n3\n4'))
  tui.start()
  tui.renderNow(true)
  const states = [[tui.viewportTop, tui.isFollowingOutput]]
  terminal.onInput('\x1b[<64;1;1M')
  tui.renderNow()
  states.push([tui.viewportTop, tui.isFollowingOutput])
  tui.scrollToTop()
  tui.renderNow()
  states.push([tui.viewportTop, tui.isFollowingOutput])
  tui.scrollToBottom()
  tui.renderNow()
  states.push([tui.viewportTop, tui.isFollowingOutput])
  tui.stop({ preserveScreen: true })
  return { states, events: terminal.events }
}

function altKeyboardSearchReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = 12
  terminal.rows = 3
  const tui = new api.TuiAltScreen(terminal, false)
  tui.addChild(new api.Text('zero\nneedle one\ntwo\nneedle two\nlast', 0, 0))
  tui.start()
  tui.renderNow(true)
  terminal.onInput('\x1b[5~')
  tui.renderNow()
  const afterPageUp = [tui.viewportTop, tui.isFollowingOutput]
  terminal.onInput('\x1b[102;6u')
  for (const character of 'needle') terminal.onInput(character)
  tui.renderNow()
  const afterQuery = [tui.viewportTop, tui.isFollowingOutput]
  const searchVisible = terminal.events.some(
    ([kind, value]) => kind === 'write' && value.includes('Find transcript'),
  )
  const currentMatchVisible = terminal.events.some(
    ([kind, value]) => kind === 'write' && value.includes('\x1b[1;7mneedle'),
  )
  terminal.onInput('\r')
  tui.renderNow()
  const afterNext = [tui.viewportTop, tui.isFollowingOutput]
  terminal.onInput('\x1b')
  tui.stop({ preserveScreen: true })
  return { afterPageUp, afterQuery, afterNext, searchVisible, currentMatchVisible }
}

async function altSelectionReceipt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = 8
  terminal.rows = 2
  const copied = []
  const tui = new api.TuiAltScreen(terminal, false, undefined, {
    copySelection: async (text) => { copied.push(text); return true },
  })
  tui.addChild(new api.Text('abcdef', 0, 0))
  tui.start()
  tui.renderNow(true)
  terminal.onInput('\x1b[<0;1;1M')
  terminal.onInput('\x1b[<32;3;1M')
  tui.renderNow()
  const highlighted = terminal.events.some(
    ([kind, value]) => kind === 'write' && value.includes('\x1b[7mabc\x1b[27m'),
  )
  terminal.onInput('\x1b[<0;3;1m')
  await Promise.resolve()
  await Promise.resolve()
  tui.stop({ preserveScreen: true })
  return { copied, highlighted }
}

function altImageReceipt(api) {
  api.setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = 8
  terminal.rows = 2
  const tui = new api.TuiAltScreen(terminal, false)
  tui.addChild(new api.Image(
    'QQ==',
    'image/png',
    { fallbackColor: (text) => text },
    { imageId: 7, maxWidthCells: 1, maxHeightCells: 1 },
    { widthPx: 1, heightPx: 1 },
  ))
  tui.addChild(new api.Text('one\ntwo\nthree', 0, 0))
  tui.start()
  tui.renderNow(true)
  tui.scrollToTop()
  tui.renderNow()
  tui.scrollToBottom()
  tui.renderNow()
  tui.scrollToTop()
  tui.renderNow()
  const writes = terminal.events.filter(([kind]) => kind === 'write').map(([, value]) => value)
  const receipt = {
    transmissions: writes.filter((value) => value.includes(';QQ==')).length,
    placements: writes.filter((value) => value.includes('a=p,q=2')).length,
    placementDeletes: writes.filter((value) => value.includes('a=d,d=a,q=2')).length,
  }
  tui.stop({ preserveScreen: true })
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  return receipt
}

assert.deepEqual(mainReceipt(adapter), mainReceipt(reference), 'MainScreen transaction receipt')
assert.deepEqual(mainStateReceipt(adapter), mainStateReceipt(reference), 'MainScreen state/stop receipt')
assert.deepEqual(
  withStableTerminalEnvironment(() => altReceipt(adapter)),
  withStableTerminalEnvironment(() => altReceipt(reference)),
  'AltScreen lifecycle/transaction receipt',
)
assert.deepEqual(await colorReceipt(adapter), await colorReceipt(reference), 'OSC 11 query receipt')
assert.deepEqual(await terminalReceipt(adapter), await terminalReceipt(reference), 'terminal query/cell-size receipt')
assert.deepEqual(overlayReceipt(adapter), overlayReceipt(reference), 'overlay layout/focus receipt')
assert.deepEqual(
  withStableTerminalEnvironment(() => altScrollReceipt(adapter)),
  withStableTerminalEnvironment(() => altScrollReceipt(reference)),
  'AltScreen scrolling/mouse receipt',
)
assert.deepEqual(
  withStableTerminalEnvironment(() => altKeyboardSearchReceipt(adapter)),
  withStableTerminalEnvironment(() => altKeyboardSearchReceipt(reference)),
  'AltScreen keyboard/search receipt',
)
assert.deepEqual(
  await withStableTerminalEnvironment(() => altSelectionReceipt(adapter)),
  await withStableTerminalEnvironment(() => altSelectionReceipt(reference)),
  'AltScreen selection/copy receipt',
)
assert.deepEqual(
  withStableTerminalEnvironment(() => altImageReceipt(adapter)),
  withStableTerminalEnvironment(() => altImageReceipt(reference)),
  'AltScreen Kitty cache/placement receipt',
)
console.log('M6 semantic oracle OK: authenticated 0.84.2 TuiBase/Main/Alt search/selection/image receipts')
