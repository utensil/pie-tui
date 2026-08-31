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

function startAlt(api, root, { columns = 20, rows = 4 } = {}) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const tui = new api.TuiAltScreen(terminal, false, undefined)
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { tui, root }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

function scrollPromptReceipt(api) {
  const lines = [
    'before',
    '\x1b]133;A\x07BEL prompt',
    'between one',
    'between two',
    '\x1b]133;A\x1b\\ST prompt',
    'between three',
    'between four',
    '\x1b]133;A\x07second BEL prompt',
    'tail one',
    'tail two',
    'tail three',
    'tail four',
  ].join('\n')
  const root = new api.ScrollView(new api.Text(lines, 0, 0), {
    primary: true,
    follow: 'none',
  })
  const { tui } = startAlt(api, root)
  const navigate = (start, direction) => {
    root.scrollTo(start)
    tui.renderNow(true)
    const before = root.scrollTop
    tui.scrollToPrompt(direction)
    return { before, after: root.scrollTop }
  }
  const result = {
    nextFromTop: navigate(0, 1),
    nextFromFirst: navigate(1, 1),
    previousFromSecond: navigate(4, -1),
    previousFromTop: navigate(0, -1),
  }
  stopAlt(tui)
  return result
}

function noMarkerReceipt(api) {
  const root = new api.ScrollView(new api.Text(
    Array.from({ length: 12 }, (_, index) => `plain-${index}`).join('\n'),
    0,
    0,
  ), { primary: true, follow: 'none' })
  const { tui } = startAlt(api, root)
  root.scrollTo(4)
  tui.renderNow(true)
  const before = root.scrollTop
  tui.scrollToPrompt(1)
  const afterNext = root.scrollTop
  tui.scrollToPrompt(-1)
  const afterPrevious = root.scrollTop
  stopAlt(tui)
  return { before, afterNext, afterPrevious }
}

for (const [label, receipt] of [
  ['BEL/ST prompt marker navigation', scrollPromptReceipt],
  ['no-marker navigation no-op', noMarkerReceipt],
]) {
  const current = withStableTerminalEnvironment(() => receipt(adapter))
  const expected = withStableTerminalEnvironment(() => receipt(reference))
  assert.deepEqual(current, expected, label)
  console.log(`Scroll-to-prompt authenticated oracle OK: ${label}`)
}
