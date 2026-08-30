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

const authenticatedSources = {
  'layout.js': 'c117ab224541475a1dc4ac08b30e38a0e3c73277ba72167f38c524800bdcf9ac',
  'tui-alt-screen.js': 'ba91af10ad497538d730b6aaabb77d43cc0ad8b2e73154c42c17cbfdaadee11e',
  'components/scroll-view.js': '47b1626e12b097cb1fe94fb22007416f3912c0903f814fefc95b967732a87f36',
}
const manifest = JSON.parse(
  await readFile(join(dirname(distribution), 'package.json'), 'utf8'),
)
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.2')
for (const [name, expected] of Object.entries(authenticatedSources)) {
  const actual = createHash('sha256')
    .update(await readFile(join(distribution, name)))
    .digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  events = []
  columns = 20
  rows = 8
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

function startAlt(api, root, {
  columns = 20,
  rows = 8,
} = {}) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const copied = []
  const tui = new api.TuiAltScreen(terminal, false, undefined, {
    copySelection: async (text) => {
      copied.push(text)
      return true
    },
  })
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui, copied }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

function mouse(button, x, y, release = false) {
  return `\x1b[<${button};${x + 1};${y + 1}${release ? 'm' : 'M'}`
}

function nestedRoot(api) {
  const inner = new api.ScrollView(
    new api.Text('i0\ni1\ni2\ni3\ni4\ni5', 0, 0),
    { overscroll: 'chain', primary: false },
  )
  const content = new api.VStack(
    [
      { component: new api.Text('top', 0, 0), basis: 1, shrink: 0 },
      { component: inner, basis: 3, shrink: 0 },
      { component: new api.Text('t0\nt1\nt2', 0, 0), basis: 3, shrink: 0 },
    ],
    { gap: 0 },
  )
  const outer = new api.ScrollView(content, { primary: true })
  return { outer }
}

async function receipt(api, rootFactory, events, dimensions) {
  const { terminal, tui, copied } = startAlt(api, rootFactory(api), dimensions)
  try {
    for (const event of events) terminal.onInput(event)
    tui.renderNow()
    await Promise.resolve()
    await Promise.resolve()
    const point = (value) => value && {
      row: value.row,
      col: value.col,
      hasScrollView: Boolean(value.scrollView),
    }
    return {
      copied: [...copied],
      anchor: point(tui.selectionAnchor),
      focus: point(tui.selectionFocus),
      highlightRows: tui.previousScreen.map((line) => line.includes('\x1b[7m')),
    }
  } finally {
    stopAlt(tui)
  }
}

const nestedEvents = [
  mouse(0, 1, 2),
  mouse(32, 1, 4),
  mouse(0, 1, 4, true),
]
const wideEvents = [
  mouse(0, 2, 0),
  mouse(32, 3, 0),
  mouse(0, 3, 0, true),
]
const zwjEvents = [
  mouse(0, 1, 0),
  mouse(32, 2, 0),
  mouse(0, 2, 0, true),
]
const combiningEvents = [
  mouse(0, 0, 0),
  mouse(32, 1, 0),
  mouse(0, 1, 0, true),
]

const cases = [
  {
    label: 'nested ScrollView identity and clip',
    root: (api) => nestedRoot(api).outer,
    events: nestedEvents,
    dimensions: { columns: 20, rows: 8 },
  },
  {
    label: 'wide grapheme endpoint snapping',
    root: () => new adapter.Text('A界B', 0, 0),
    referenceRoot: (api) => new api.Text('A界B', 0, 0),
    events: wideEvents,
    dimensions: { columns: 8, rows: 2 },
  },
  {
    label: 'ZWJ grapheme endpoint snapping',
    root: () => new adapter.Text('👩‍👩‍👧‍👦X', 0, 0),
    referenceRoot: (api) => new api.Text('👩‍👩‍👧‍👦X', 0, 0),
    events: zwjEvents,
    dimensions: { columns: 10, rows: 2 },
  },
  {
    label: 'combining grapheme endpoint safety',
    root: () => new adapter.Text('éX', 0, 0),
    referenceRoot: (api) => new api.Text('éX', 0, 0),
    events: combiningEvents,
    dimensions: { columns: 8, rows: 2 },
  },
]

for (const item of cases) {
  const referenceReceipt = await receipt(
    reference,
    item.referenceRoot ?? item.root,
    item.events,
    item.dimensions,
  )
  const adapterReceipt = await receipt(
    adapter,
    item.root,
    item.events,
    item.dimensions,
  )
  assert.deepEqual(adapterReceipt, referenceReceipt, item.label)
  console.log(`Selection geometry authenticated oracle OK: ${item.label}`)
}
