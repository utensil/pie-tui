import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

const distribution = process.env.PI_TUI_DIST
assert.ok(
  distribution,
  'PI_TUI_DIST must point to authenticated pi-tui 0.84.2 dist',
)
const manifest = JSON.parse(
  await readFile(join(dirname(distribution), 'package.json'), 'utf8'),
)
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.2')
assert.equal(
  createHash('sha256')
    .update(await readFile(join(distribution, 'tui-alt-screen.js')))
    .digest('hex'),
  'ba91af10ad497538d730b6aaabb77d43cc0ad8b2e73154c42c17cbfdaadee11e',
  'tui-alt-screen.js SHA-256',
)

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  events = []
  columns = 6
  rows = 4
  kittyProtocolActive = false

  start(onInput, onResize) {
    this.onInput = onInput
    this.onResize = onResize
    this.events.push(['start'])
  }

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
  const saved = new Map(
    ['TERM', 'TMUX', 'ZELLIJ', 'STY'].map((name) => [name, process.env[name]]),
  )
  process.env.TERM = 'xterm-256color'
  delete process.env.TMUX
  delete process.env.ZELLIJ
  delete process.env.STY
  try {
    return callback()
  } finally {
    for (const [name, value] of saved) {
      if (value === undefined) delete process.env[name]
      else process.env[name] = value
    }
  }
}

function pointer(button, x, y, release = false) {
  return `\x1b[<${button};${x + 1};${y + 1}${release ? 'm' : 'M'}`
}

function startAlt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const root = new api.ScrollView(
    new api.Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { primary: true, scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const tui = new api.TuiAltScreen(terminal, false, undefined)
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function snapshot(api) {
  const { terminal, tui } = startAlt(api)

  // Begin a selection drag at the lower edge. The authenticated runtime starts
  // its 50ms selection auto-scroll loop while the pointer is held there.
  terminal.onInput(pointer(0, 1, 1))
  terminal.onInput(pointer(32, 1, 3))
  assert.ok(tui.selectionAutoScrollTimer, 'edge drag starts auto-scroll timer')
  assert.equal(tui.selectionAutoScrollDirection, 1)
  assert.deepEqual(tui.selectionDragPointer, { x: 1, y: 3 })

  // Press the scrollbar thumb while the selection drag is still active. This
  // must cancel every selection-auto-scroll transient before starting thumb
  // dragging; the current adapter intentionally remains red here.
  terminal.onInput(pointer(0, 5, 0))
  const result = {
    selectionPressActive: tui.selectionPressActive,
    selectionAnchor: tui.selectionAnchor,
    selectionFocus: tui.selectionFocus,
    selectionGranularity: tui.selectionGranularity,
    selectionInitialRange: tui.selectionInitialRange,
    selectionDragged: tui.selectionDragged,
    pressedUrl: tui.pressedUrl,
    autoScrollTimerActive: tui.selectionAutoScrollTimer !== undefined,
    autoScrollDirection: tui.selectionAutoScrollDirection,
    dragPointer: tui.selectionDragPointer,
    scrollbarDragActive: tui.scrollbarDrag !== undefined,
  }
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
  return result
}

const current = withStableTerminalEnvironment(() => snapshot(adapter))
const expected = withStableTerminalEnvironment(() => snapshot(reference))
assert.deepEqual(
  current,
  expected,
  'scrollbar thumb press cancels selection auto-scroll and transient selection state',
)
console.log('Scrollbar selection-cancel authenticated oracle OK')
