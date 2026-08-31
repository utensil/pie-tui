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
  'layout.js': 'c117ab224541475a1dc4ac08b30e38a0e3c73277ba72167f38c524800bdcf9ac',
})) {
  const actual = createHash('sha256').update(await readFile(join(distribution, name))).digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  events = []
  columns = 32
  rows = 4
  kittyProtocolActive = false
  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize; this.events.push(['start']) }
  stop() { this.events.push(['stop']) }
  write(data) { this.events.push(['write', data]) }
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
  delete process.env.TMUX
  delete process.env.ZELLIJ
  delete process.env.STY
  try { return callback() } finally {
    for (const [name, value] of saved) {
      if (value === undefined) delete process.env[name]
      else process.env[name] = value
    }
  }
}

function snapshot(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const primary = new api.ScrollView(
    new api.Text('prefix needle suffix\nsecond needle row', 0, 0),
    { primary: true, scrollbar: 'always', follow: 'none' },
  )
  const root = new api.HStack([
    { component: new api.Text('L'), basis: 4 },
    { component: primary, grow: 1 },
  ])
  const tui = new api.TuiAltScreen(terminal, false, undefined, {
    searchMatchStyle: (text) => `<m>${text}</m>`,
    searchCurrentMatchStyle: (text) => `<c>${text}</c>`,
  })
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  tui.openSearch()
  for (const character of 'needle') terminal.onInput(character)
  tui.renderNow()
  const layout = tui.currentLayout
  const highlighted = tui.applySearchHighlights(layout.lines, layout)
  const result = {
    matches: tui.activeSearch?.matches,
    selectedIndex: tui.activeSearch?.selectedIndex,
    highlighted,
  }
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
  return result
}

const current = withStableTerminalEnvironment(() => snapshot(adapter))
const expected = withStableTerminalEnvironment(() => snapshot(reference))
assert.deepEqual(current, expected, 'search highlight ranges honor primary pane x/clip/scrollbar geometry')
console.log('Search pane authenticated oracle OK')
