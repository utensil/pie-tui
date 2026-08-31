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
  columns = 20
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

function startAlt(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const root = new api.ScrollView(
    new api.Text('first\nsecond\nthird\nfourth', 0, 0),
    { primary: true, follow: 'none' },
  )
  const tui = new api.TuiAltScreen(terminal, false, undefined)
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function x10PrimaryPress(x = 1, y = 1) {
  // Legacy X10 encodes button, x, and y as one byte each after ESC [ M.
  return `\x1b[M${String.fromCharCode(32)}${String.fromCharCode(x + 33)}${String.fromCharCode(y + 33)}`
}

function snapshot(api) {
  const { terminal, tui } = startAlt(api)
  terminal.onInput(x10PrimaryPress())
  const result = {
    consumed: tui.selectionPressActive === false,
    selectionPressActive: tui.selectionPressActive,
    hasAnchor: tui.selectionAnchor !== undefined,
    hasFocus: tui.selectionFocus !== undefined,
    granularity: tui.selectionGranularity,
    bounds: tui.getSelectionBounds(),
  }
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
  return result
}

const current = withStableTerminalEnvironment(() => snapshot(adapter))
const expected = withStableTerminalEnvironment(() => snapshot(reference))
assert.deepEqual(
  current,
  expected,
  'legacy X10 primary press is consumed without entering selection',
)
console.log('Legacy X10 authenticated oracle OK')
