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
  'components/alt-screen-flash.js': '6ca2016101ca570a94fdaa18bfe8edbc6734243cb5363d21110e809fcd47db12',
  'utils.js': '70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052',
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
  columns = 30
  rows = 5
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

function mouse(button, x, y, release = false) {
  return `\x1b[<${button};${x + 1};${y + 1}${release ? 'm' : 'M'}`
}

function nestedRoot(api) {
  const inner = new api.ScrollView(
    new api.Text('alpha\nbeta\ngamma\ndelta\nepsilon\nzeta', 0, 0),
    { overscroll: 'chain', primary: false },
  )
  const content = new api.VStack(
    [
      { component: new api.Text('top', 0, 0), basis: 1, shrink: 0 },
      { component: inner, basis: 3, shrink: 0 },
      { component: new api.Text('tail', 0, 0), basis: 3, shrink: 0 },
    ],
    { gap: 0 },
  )
  return { inner, outer: new api.ScrollView(content, { primary: true }) }
}

function startAlt(api, root, {
  columns = 30,
  rows = 5,
  copySelection,
} = {}) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const tui = new api.TuiAltScreen(terminal, false, undefined, { copySelection })
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

async function ticks() {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
}

async function withClock(times, callback) {
  const original = Date.now
  let index = 0
  Date.now = () => times[Math.min(index++, times.length - 1)]
  try {
    return await callback()
  } finally {
    Date.now = original
  }
}

async function clickSequence(api, root, events, options = {}) {
  const copied = []
  const { clock = [1000], ...startOptions } = options
  const { terminal, tui } = startAlt(api, root, {
    ...startOptions,
    copySelection: options.useCallback === false
      ? undefined
      : async (text) => {
        copied.push(text)
        return options.copyResult ?? true
      },
  })
  try {
    await withClock(clock, async () => {
      for (const event of events) terminal.onInput(event)
    })
    tui.renderNow()
    await ticks()
    tui.renderNow()
    return {
      copied,
      anchor: tui.selectionAnchor && {
        row: tui.selectionAnchor.row,
        col: tui.selectionAnchor.col,
        boundary: tui.selectionAnchor.boundary,
        hasScrollView: Boolean(tui.selectionAnchor.scrollView),
      },
      focus: tui.selectionFocus && {
        row: tui.selectionFocus.row,
        col: tui.selectionFocus.col,
        boundary: tui.selectionFocus.boundary,
        hasScrollView: Boolean(tui.selectionFocus.scrollView),
      },
      granularity: tui.selectionGranularity,
      flashes: (Array.isArray(tui.flashes) ? tui.flashes : (tui.flashes.entries ?? []))
        .map((entry) => typeof entry === 'string' ? entry : entry.message),
      writes: terminal.events
        .filter(([kind]) => kind === 'write')
        .map(([, value]) => value),
    }
  } finally {
    stopAlt(tui)
  }
}

function tripleClickEvents(x, y = 0) {
  return [
    mouse(0, x, y), mouse(0, x, y, true),
    mouse(0, x, y), mouse(0, x, y, true),
    mouse(0, x, y), mouse(0, x, y, true),
  ]
}

function doubleClickEvents(x, y = 0) {
  return [
    mouse(0, x, y), mouse(0, x, y, true),
    mouse(0, x, y), mouse(0, x, y, true),
  ]
}

async function doubleClickWords(api) {
  return clickSequence(
    api,
    new api.Text('alpha beta, 42', 0, 0),
    doubleClickEvents(2),
  )
}

async function doubleClickPunctuationWhitespace(api) {
  const punctuation = await clickSequence(
    api,
    new api.Text('alpha beta, 42', 0, 0),
    doubleClickEvents(10),
  )
  const whitespace = await clickSequence(
    api,
    new api.Text('alpha beta, 42', 0, 0),
    doubleClickEvents(5),
  )
  return { punctuation, whitespace }
}

async function tripleClickLine(api) {
  return clickSequence(
    api,
    new api.Text('alpha beta, 42\nnext line', 0, 0),
    tripleClickEvents(2),
  )
}

async function timeoutDoesNotBecomeDoubleClick(api) {
  return clickSequence(
    api,
    new api.Text('alpha beta', 0, 0),
    doubleClickEvents(2),
    { clock: [0, 600] },
  )
}

async function unicodeWords(api) {
  const emoji = await clickSequence(
    api,
    new api.Text('go 👩‍👩‍👧‍👦 café', 0, 0),
    doubleClickEvents(3),
  )
  const combining = await clickSequence(
    api,
    new api.Text('go 👩‍👩‍👧‍👦 café', 0, 0),
    doubleClickEvents(7),
  )
  return { emoji, combining }
}

async function nestedScrolledWord(api) {
  const fixture = nestedRoot(api)
  const { terminal, tui } = startAlt(api, fixture.outer)
  fixture.inner.scrollTo(2)
  tui.renderNow()
  try {
    const copied = []
    tui.copySelection = async (text) => { copied.push(text); return true }
    await withClock([1000], async () => {
      for (const event of doubleClickEvents(1, 1)) terminal.onInput(event)
    })
    tui.renderNow()
    await ticks()
    tui.renderNow()
    return {
      copied,
      anchor: tui.selectionAnchor && {
        row: tui.selectionAnchor.row,
        col: tui.selectionAnchor.col,
        hasScrollView: Boolean(tui.selectionAnchor.scrollView),
      },
      focus: tui.selectionFocus && {
        row: tui.selectionFocus.row,
        col: tui.selectionFocus.col,
        boundary: tui.selectionFocus.boundary,
        hasScrollView: Boolean(tui.selectionFocus.scrollView),
      },
      granularity: tui.selectionGranularity,
    }
  } finally {
    stopAlt(tui)
  }
}

async function noSelection(api) {
  return clickSequence(
    api,
    new api.Text('alpha beta', 0, 0),
    [mouse(0, 2), mouse(0, 2, 0, true)],
  )
}

async function osc52(api) {
  const receipt = await clickSequence(
    api,
    new api.Text('alpha beta', 0, 0),
    doubleClickEvents(2),
    { useCallback: false },
  )
  const payloads = receipt.writes
    .flatMap((value) => [...value.matchAll(/\x1b\]52;c;([^\x07]*)\x07/g)].map((match) => match[1]))
  return { ...receipt, payloads }
}

async function focusOutAndStop(api) {
  const { terminal, tui } = startAlt(api, new api.Text('alpha beta', 0, 0))
  await withClock([1000], async () => {
    for (const event of doubleClickEvents(2).slice(0, 3)) terminal.onInput(event)
  })
  tui.renderNow()
  terminal.onInput('\x1b[O')
  const focusOut = {
    anchor: tui.selectionAnchor,
    focus: tui.selectionFocus,
    pressActive: tui.selectionPressActive,
    lastClick: tui.lastClick,
  }
  stopAlt(tui)
  return {
    focusOut,
    stopped: !tui.altScreenActive,
    stopEvents: terminal.events.filter(([kind]) => kind === 'write').slice(-2),
  }
}

const cases = [
  ['double-click word selection', doubleClickWords],
  ['punctuation and whitespace word boundaries', doubleClickPunctuationWhitespace],
  ['triple-click line selection', tripleClickLine],
  ['double-click timeout', timeoutDoesNotBecomeDoubleClick],
  ['Unicode word graphemes', unicodeWords],
  ['nested ScrollView word ownership after scroll', nestedScrolledWord],
  ['single click has no selection/copy', noSelection],
  ['callback copy success and flash', (api) => clickSequence(api, new api.Text('alpha beta', 0, 0), doubleClickEvents(2), { copyResult: true })],
  ['callback copy failure and flash', (api) => clickSequence(api, new api.Text('alpha beta', 0, 0), doubleClickEvents(2), { copyResult: false })],
  ['OSC52 clipboard payload', osc52],
  ['focus-out and stop cleanup', focusOutAndStop],
]

for (const [label, receipt] of cases) {
  const expected = await receipt(reference)
  const actual = await receipt(adapter)
  assert.deepEqual(actual, expected, label)
  console.log(`Word/copy authenticated oracle OK: ${label}`)
}
