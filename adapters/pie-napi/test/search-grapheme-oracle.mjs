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
  'alt-screen-search.js': '1c8b437afa5c57d329001679e9ce8e38cc83cc63cdb7f18c0874c3e17075621c',
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

function probe(api, text, query) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const root = new api.ScrollView(new api.Text(text, 0, 0), { primary: true, follow: 'none' })
  const tui = new api.TuiAltScreen(terminal, false, undefined, {})
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  terminal.onInput('\x1b[102;6u')
  for (const character of query) terminal.onInput(character)
  tui.renderNow()
  const search = tui.activeSearch
  const result = {
    selectedKey: search?.selectedKey,
    selectedIndex: search?.selectedIndex,
    resultCount: search?.component?.resultCount,
    matches: search?.matches?.map((match) => match.segments),
  }
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
  return result
}

const cases = [
  ['ZWJ family search uses one display grapheme span', 'A 👨‍👩‍👧‍👦 X', '👨‍👩‍👧‍👦'],
]
for (const [label, text, query] of cases) {
  const actual = withStableTerminalEnvironment(() => probe(adapter, text, query))
  const expected = withStableTerminalEnvironment(() => probe(reference, text, query))
  assert.deepEqual(actual, expected, label)
  console.log(`Authenticated search-grapheme oracle OK: ${label}`)
}
