import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

const distribution = process.env.PI_TUI_DIST
assert.ok(distribution, 'PI_TUI_DIST must point to authenticated pi-tui 0.84.2 dist')
const manifest = JSON.parse(await readFile(join(dirname(distribution), 'package.json'), 'utf8'))
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.2')
for (const [name, expected] of Object.entries({
  'tui-alt-screen.js': 'ba91af10ad497538d730b6aaabb77d43cc0ad8b2e73154c42c17cbfdaadee11e',
  'utils.js': '70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052',
})) {
  const actual = createHash('sha256').update(await readFile(join(distribution, name))).digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  columns = 20
  rows = 6
  kittyProtocolActive = false
  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize }
  stop() {}
  write() {}
  moveBy() {}
  hideCursor() {}
  showCursor() {}
  clearLine() {}
  clearFromCursor() {}
  clearScreen() {}
  setTitle() {}
  setProgress() {}
}

function withStableTerminalEnvironment(callback) {
  const names = ['TERM', 'TMUX', 'ZELLIJ', 'STY']
  const saved = new Map(names.map((name) => [name, process.env[name]]))
  process.env.TERM = 'xterm-256color'
  for (const name of names.slice(1)) delete process.env[name]
  try { return callback() } finally {
    for (const [name, value] of saved) {
      if (value === undefined) delete process.env[name]
      else process.env[name] = value
    }
  }
}

function probe(api, line) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const root = new api.ScrollView(new api.Text(line, 0, 0), { primary: true, follow: 'none' })
  const tui = new api.TuiAltScreen(terminal, false, undefined, {
    searchMatchStyle: (text) => `\x1b[4m${text}\x1b[24m`,
    searchCurrentMatchStyle: (text) => `\x1b[1;7m${text}\x1b[22;27m`,
  })
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  tui.activeSearch = {
    matches: [{ segments: [{ row: 0, startCol: 0, endCol: 3 }] }],
    selectedIndex: 0,
  }
  const result = tui.applySearchHighlights([line], tui.currentLayout)
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
  return result[0]
}

const line = '\x1b[31mfoo\x1b[0m bar'
const actual = withStableTerminalEnvironment(() => probe(adapter, line))
const expected = withStableTerminalEnvironment(() => probe(reference, line))
assert.equal(actual, expected, 'search highlighting preserves ANSI boundaries')
console.log('Authenticated search-highlight oracle OK: ANSI-safe match styling')
