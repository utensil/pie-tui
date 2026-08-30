import assert from 'node:assert/strict'
import test from 'node:test'

import {
  ScrollView,
  Text,
  TuiAltScreen,
  VStack,
  setCapabilities,
} from '../index.js'

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

function startAlt(root, {
  columns = 20,
  rows = 8,
  copySelection,
} = {}) {
  setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const tui = new TuiAltScreen(terminal, false, undefined, { copySelection })
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

// Coordinates are zero-based here; SGR mouse coordinates on the wire are 1-based.
function mouse(button, x, y, release = false) {
  return `\x1b[<${button};${x + 1};${y + 1}${release ? 'm' : 'M'}`
}

async function selectAndCopy(root, events, options = {}) {
  const copied = []
  const { beforeEvents, ...startOptions } = options
  const { terminal, tui } = startAlt(root, {
    ...startOptions,
    copySelection: async (text) => {
      copied.push(text)
      return true
    },
  })
  try {
    beforeEvents?.({ terminal, tui })
    for (const event of events) terminal.onInput(event)
    tui.renderNow()
    // Clipboard callbacks are intentionally asynchronous in the public API.
    await Promise.resolve()
    await Promise.resolve()
    return {
      copied,
      anchor: tui.selectionAnchor,
      focus: tui.selectionFocus,
      screen: tui.previousScreen,
    }
  } finally {
    stopAlt(tui)
  }
}

function nestedRoot() {
  const inner = new ScrollView(
    new Text('i0\ni1\ni2\ni3\ni4\ni5', 0, 0),
    { overscroll: 'chain', primary: false },
  )
  const content = new VStack(
    [
      { component: new Text('top', 0, 0), basis: 1, shrink: 0 },
      { component: inner, basis: 3, shrink: 0 },
      { component: new Text('t0\nt1\nt2', 0, 0), basis: 3, shrink: 0 },
    ],
    { gap: 0 },
  )
  const outer = new ScrollView(content, { primary: true })
  return { inner, outer }
}

test('selection remains owned by the hit ScrollView and clips drag to its pane', async () => {
  const { outer } = nestedRoot()
  const receipt = await selectAndCopy(outer, [
    mouse(0, 1, 2),
    mouse(32, 1, 4),
    mouse(0, 1, 4, true),
  ])

  assert.equal(receipt.anchor?.scrollView?.constructor?.name, 'ScrollView')
  assert.equal(receipt.focus?.scrollView, receipt.anchor?.scrollView)
  assert.deepEqual(
    { row: receipt.anchor?.row, col: receipt.anchor?.col },
    { row: 1, col: 1 },
  )
  assert.deepEqual(
    { row: receipt.focus?.row, col: receipt.focus?.col },
    { row: 2, col: 1 },
  )
  assert.deepEqual(receipt.copied, ['1\ni2'])
  assert.match(receipt.screen[2] ?? '', /\x1b\[7m/)
  assert.match(receipt.screen[3] ?? '', /\x1b\[7m/)
  assert.doesNotMatch(receipt.screen[4] ?? '', /\x1b\[7m/)
})

test('selection maps content rows to the pane after ScrollView scrolling', async () => {
  const fixture = nestedRoot()
  const receipt = await selectAndCopy(
    fixture.outer,
    [
      mouse(0, 1, 1),
      mouse(32, 1, 2),
      mouse(0, 1, 2, true),
    ],
    {
      beforeEvents: () => {
        fixture.inner.scrollTo(2)
      },
    },
  )
  assert.deepEqual(
    { row: receipt.anchor?.row, col: receipt.anchor?.col },
    { row: 2, col: 1 },
  )
  assert.deepEqual(
    { row: receipt.focus?.row, col: receipt.focus?.col },
    { row: 3, col: 1 },
  )
  assert.deepEqual(receipt.copied, ['2\ni3'])
  assert.match(receipt.screen[1] ?? '', /\x1b\[7m/)
  assert.match(receipt.screen[2] ?? '', /\x1b\[7m/)
  assert.doesNotMatch(receipt.screen[3] ?? '', /\x1b\[7m/)
})

test('selection endpoints snap to the complete wide grapheme', async () => {
  const receipt = await selectAndCopy(
    new Text('A界B', 0, 0),
    [mouse(0, 2, 0), mouse(32, 3, 0), mouse(0, 3, 0, true)],
    { columns: 8, rows: 2 },
  )
  assert.deepEqual(receipt.copied, ['界B'])
})

test('selection endpoints do not split a ZWJ emoji grapheme', async () => {
  const receipt = await selectAndCopy(
    new Text('👩‍👩‍👧‍👦X', 0, 0),
    [mouse(0, 1, 0), mouse(32, 2, 0), mouse(0, 2, 0, true)],
    { columns: 10, rows: 2 },
  )
  assert.deepEqual(receipt.copied, ['👩‍👩‍👧‍👦X'])
})

test('selection endpoints preserve combining-mark graphemes', async () => {
  const receipt = await selectAndCopy(
    new Text('éX', 0, 0),
    [mouse(0, 0, 0), mouse(32, 1, 0), mouse(0, 1, 0, true)],
    { columns: 8, rows: 2 },
  )
  assert.deepEqual(receipt.copied, ['éX'])
})

test('selection end columns snap to the complete wide grapheme', () => {
  const { tui } = startAlt(new Text('A界B', 0, 0), { columns: 8, rows: 2 })
  try {
    assert.deepEqual(
      tui.getSelectionColumns('A界B', 0, {
        start: { row: 0, col: 0 },
        end: { row: 0, col: 1 },
      }),
      { start: 0, end: 3 },
    )
  } finally {
    stopAlt(tui)
  }
})

test('selection bounds reject endpoints from different ScrollViews', () => {
  const { tui } = startAlt(new Text('x', 0, 0), { columns: 4, rows: 2 })
  try {
    tui.selectionAnchor = {
      row: 0,
      col: 0,
      scrollView: new ScrollView(new Text('left', 0, 0)),
    }
    tui.selectionFocus = {
      row: 0,
      col: 1,
      scrollView: new ScrollView(new Text('right', 0, 0)),
    }
    assert.equal(tui.getSelectionBounds(), undefined)
  } finally {
    stopAlt(tui)
  }
})
