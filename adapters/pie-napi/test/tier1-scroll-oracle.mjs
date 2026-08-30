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
  columns = 8
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

function startAlt(api, root, { columns = 8, rows = 5, ...options } = {}) {
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

function written(terminal) {
  return terminal.events
    .filter(([kind]) => kind === 'write')
    .map(([, data]) => data)
    .join('')
}

function nestedRoot(api, {
  overscroll = 'chain',
  innerPrimary = true,
  outerPrimary = false,
} = {}) {
  const inner = new api.ScrollView(
    new api.Text('i0\ni1\ni2\ni3\ni4\ni5', 0, 0),
    { overscroll, primary: innerPrimary },
  )
  const outer = new api.ScrollView(
    new api.VStack(
      [
        { component: new api.Text('top', 0, 0), basis: 1, shrink: 0 },
        { component: inner, basis: 3, shrink: 0 },
        { component: new api.Text('t0\nt1\nt2', 0, 0), basis: 3, shrink: 0 },
      ],
      { gap: 0 },
    ),
    { primary: outerPrimary },
  )
  return { inner, outer }
}

function paintAndPointerReceipt(api) {
  const always = new api.ScrollView(
    new api.Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const first = startAlt(api, always, { columns: 6, rows: 4 })
  const alwaysScreen = written(first.terminal)
  first.terminal.onInput('\x1b[<0;6;1M')
  first.terminal.onInput('\x1b[<32;6;4M')
  first.terminal.onInput('\x1b[<0;6;4m')
  const dragTop = always.scrollTop
  stopAlt(first.tui)

  const auto = new api.ScrollView(
    new api.Text('0\n1\n2\n3\n4\n5\n6\n7', 0, 0),
    {
      scrollbar: 'auto',
      scrollbarStyle: () => '#',
      scrollbarHideDelayMs: 60_000,
    },
  )
  const second = startAlt(api, auto, { columns: 6, rows: 4 })
  second.terminal.onInput('\x1b[<65;1;1M')
  second.tui.renderNow()
  const autoScreen = written(second.terminal)
  second.terminal.onInput('\x1b[<32;6;2M')
  auto.setScrollbar('hidden')
  auto.setScrollbar('auto')
  const receipt = {
    alwaysPainted: alwaysScreen.includes('#'),
    autoPainted: autoScreen.includes('#'),
    autoTop: auto.scrollTop,
    hoverReactivated: auto.isScrollbarVisible,
    dragTop,
  }
  stopAlt(second.tui)
  return receipt
}

function routingReceipt(api) {
  const chain = nestedRoot(api)
  const first = startAlt(api, chain.outer, { wheelScrollLines: 2 })
  first.terminal.onInput('\x1b[<65;2;3M')
  const deepest = [chain.inner.scrollTop, chain.outer.scrollTop]
  chain.inner.scrollToEnd()
  chain.outer.scrollToStart()
  first.terminal.onInput('\x1b[<65;2;3M')
  const remainder = [chain.inner.scrollTop, chain.outer.scrollTop]
  stopAlt(first.tui)

  const nonPrimary = nestedRoot(api, {
    innerPrimary: false,
    outerPrimary: true,
  })
  const second = startAlt(api, nonPrimary.outer, { wheelScrollLines: 2 })
  second.terminal.onInput('\x1b[<65;2;3M')
  const hitBeforePrimary = [
    nonPrimary.inner.scrollTop,
    nonPrimary.outer.scrollTop,
  ]
  stopAlt(second.tui)

  const contain = nestedRoot(api, { overscroll: 'contain' })
  const third = startAlt(api, contain.outer, { wheelScrollLines: 2 })
  contain.inner.scrollToEnd()
  contain.outer.scrollToStart()
  third.terminal.onInput('\x1b[<65;2;3M')
  const contained = [contain.inner.scrollTop, contain.outer.scrollTop]
  stopAlt(third.tui)
  return { deepest, remainder, hitBeforePrimary, contained }
}

function overlayReceipt(api) {
  const view = new api.ScrollView(new api.Text('0\n1\n2\n3\n4\n5', 0, 0))
  const { terminal, tui } = startAlt(api, view, { columns: 6, rows: 3 })
  const inputs = []
  const overlay = {
    focused: false,
    render: () => ['overlay'],
    invalidate() {},
    handleInput(data) { inputs.push(data) },
  }
  tui.showOverlay(overlay, { width: 6, anchor: 'top-left' })
  terminal.onInput('\x1b[<65;1;1M')
  const receipt = { scrollTop: view.scrollTop, inputs }
  stopAlt(tui)
  return receipt
}

function cleanupState(api, boundary) {
  const view = new api.ScrollView(
    new api.Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const { terminal, tui } = startAlt(api, view, { columns: 6, rows: 4 })
  terminal.onInput('\x1b[<32;6;1M')
  terminal.onInput('\x1b[<0;6;1M')
  if (boundary === 'stop') stopAlt(tui)
  else terminal.onInput('\x1b[O')
  view.setScrollbar('hidden')
  view.setScrollbar('auto')
  const hoverActive = view.isScrollbarVisible
  view.setScrollbar('always')
  const before = view.scrollTop
  tui.handleTerminalInput('\x1b[<32;6;4M')
  const after = view.scrollTop
  if (boundary !== 'stop') stopAlt(tui)
  return { hoverActive, before, after }
}

function lifecycleCleanupReceipt(api) {
  return {
    stop: cleanupState(api, 'stop'),
    focusOut: cleanupState(api, 'focusOut'),
  }
}

for (const [label, receipt] of [
  ['scrollbar paint/hover/drag', paintAndPointerReceipt],
  ['nested wheel routing', routingReceipt],
  ['overlay wheel deferral', overlayReceipt],
  ['scrollbar lifecycle cleanup', lifecycleCleanupReceipt],
]) {
  assert.deepEqual(receipt(adapter), receipt(reference), label)
  console.log(`Tier-1 authenticated oracle OK: ${label}`)
}
