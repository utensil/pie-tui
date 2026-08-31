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
for (const [name, expected] of Object.entries({
  'tui-alt-screen.js': 'ba91af10ad497538d730b6aaabb77d43cc0ad8b2e73154c42c17cbfdaadee11e',
  'alt-screen-search.js': '1c8b437afa5c57d329001679e9ce8e38cc83cc63cdb7f18c0874c3e17075621c',
})) {
  const actual = createHash('sha256')
    .update(await readFile(join(distribution, name)))
    .digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  events = []
  columns = 12
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

function startAlt(api, root, { columns = 12, rows = 4, ...options } = {}) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const tui = new api.TuiAltScreen(terminal, false, undefined, options)
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

function pointer(button, x, y, release = false) {
  return `\x1b[<${button};${x + 1};${y + 1}${release ? 'm' : 'M'}`
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds))
}

/**
 * 0.84.2 starts a 50ms interval while a selection drag is held on a clipped
 * edge. Each tick scrolls the owning ScrollView by one row and recomputes the
 * focus point from the held pointer. The adapter currently has no timer path;
 * this receipt is intentionally standalone until the implementation tranche.
 */
async function selectionAutoScrollReceipt(api) {
  const lines = Array.from({ length: 18 }, (_, index) => `line-${index}`).join('\n')
  const root = new api.ScrollView(new api.Text(lines, 0, 0), {
    primary: true,
    follow: 'none',
  })
  const { terminal, tui } = startAlt(api, root, { columns: 12, rows: 4 })
  const before = root.scrollTop
  terminal.onInput(pointer(0, 1, 1))
  terminal.onInput(pointer(32, 1, 3))
  const timerStarted = tui.selectionAutoScrollTimer !== undefined
  await sleep(135)
  const during = {
    scrollTop: root.scrollTop,
    focusRow: tui.selectionFocus?.row,
    direction: tui.selectionAutoScrollDirection,
    timerStarted,
  }
  terminal.onInput(pointer(0, 1, 3, true))
  await Promise.resolve()
  stopAlt(tui)
  return { before, during }
}

function searchMutationReceipt(api) {
  const text = new api.Text('needle zero\nneedle one\nneedle two\nneedle three', 0, 0)
  const root = new api.ScrollView(text, { primary: true, follow: 'none' })
  const { terminal, tui } = startAlt(api, root, { columns: 20, rows: 3 })
  // tui.altScreen.search (CSI-u F6) opens the authenticated search overlay.
  terminal.onInput('\x1b[102;6u')
  for (const character of 'needle') terminal.onInput(character)
  tui.renderNow()
  const initial = {
    selectedIndex: tui.activeSearch?.selectedIndex,
    selectedKey: tui.activeSearch?.selectedKey,
    viewportTop: tui.viewportTop,
    resultIndex: tui.activeSearch?.component.resultIndex,
    resultCount: tui.activeSearch?.component.resultCount,
  }
  terminal.onInput('\r')
  tui.renderNow()
  const afterNext = {
    selectedIndex: tui.activeSearch?.selectedIndex,
    selectedKey: tui.activeSearch?.selectedKey,
    viewportTop: tui.viewportTop,
    resultIndex: tui.activeSearch?.component.resultIndex,
  }
  // Insert a row before the current match. Reference retains the selected
  // match by its segment key; an index-only implementation selects a neighbor.
  text.setText('prefix\nneedle zero\nneedle one\nneedle two\nneedle three')
  tui.renderNow()
  const afterMutation = {
    selectedIndex: tui.activeSearch?.selectedIndex,
    selectedKey: tui.activeSearch?.selectedKey,
    viewportTop: tui.viewportTop,
    resultIndex: tui.activeSearch?.component.resultIndex,
    resultCount: tui.activeSearch?.component.resultCount,
  }
  terminal.onInput('\x1b')
  stopAlt(tui)
  return { initial, afterNext, afterMutation }
}

for (const [label, receipt] of [
  ['selection auto-scroll timer and focus recomputation', selectionAutoScrollReceipt],
  ['search selected-match mutation and reveal state', searchMutationReceipt],
]) {
  const current = await withStableTerminalEnvironment(() => receipt(adapter))
  const expected = await withStableTerminalEnvironment(() => receipt(reference))
  assert.deepEqual(current, expected, label)
  console.log(`Auto-scroll/search authenticated oracle OK: ${label}`)
}
