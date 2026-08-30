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

function startAlt(root, { columns = 8, rows = 5, ...options } = {}) {
  setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  terminal.columns = columns
  terminal.rows = rows
  const tui = new TuiAltScreen(terminal, false, undefined, options)
  tui.setLayoutRoot(root)
  withStableTerminalEnvironment(() => tui.start())
  tui.renderNow(true)
  return { terminal, tui }
}

function stopAlt(tui) {
  withStableTerminalEnvironment(() => tui.stop({ preserveScreen: true }))
}

function writtenSince(terminal, start) {
  return terminal.events
    .slice(start)
    .filter(([kind]) => kind === 'write')
    .map(([, data]) => data)
    .join('')
}

function nestedRoot({
  overscroll = 'chain',
  innerPrimary = true,
  outerPrimary = false,
} = {}) {
  const inner = new ScrollView(
    new Text('i0\ni1\ni2\ni3\ni4\ni5', 0, 0),
    { overscroll, primary: innerPrimary },
  )
  const content = new VStack(
    [
      { component: new Text('top', 0, 0), basis: 1, shrink: 0 },
      { component: inner, basis: 3, shrink: 0 },
      { component: new Text('t0\nt1\nt2', 0, 0), basis: 3, shrink: 0 },
    ],
    { gap: 0 },
  )
  const outer = new ScrollView(content, { primary: outerPrimary })
  return { inner, outer }
}

test('Tier-1 always scrollbar paints the canonical column and drag maps the thumb', () => {
  const view = new ScrollView(
    new Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const { terminal, tui } = startAlt(view, { columns: 6, rows: 4 })
  try {
    assert.match(writtenSince(terminal, 0), /#/)
    terminal.onInput('\x1b[<0;6;1M')
    terminal.onInput('\x1b[<32;6;4M')
    terminal.onInput('\x1b[<0;6;4m')
    assert.equal(view.scrollTop, 6)
  } finally {
    stopAlt(tui)
  }
})

test('Tier-1 auto scrollbar paints after activity and hover keeps it active', () => {
  const view = new ScrollView(
    new Text('0\n1\n2\n3\n4\n5\n6\n7', 0, 0),
    {
      scrollbar: 'auto',
      scrollbarStyle: () => '#',
      scrollbarHideDelayMs: 60_000,
    },
  )
  const { terminal, tui } = startAlt(view, { columns: 6, rows: 4 })
  try {
    assert.equal(view.isScrollbarVisible, false)
    terminal.onInput('\x1b[<65;1;1M')
    tui.renderNow()
    assert.equal(view.scrollTop, 1)
    assert.equal(view.isScrollbarVisible, true)
    assert.match(writtenSince(terminal, 0), /#/)

    terminal.onInput('\x1b[<32;6;2M')
    view.setScrollbar('hidden')
    view.setScrollbar('auto')
    assert.equal(view.isScrollbarVisible, true)
  } finally {
    stopAlt(tui)
  }
})

test('Tier-1 wheel routing is deepest-first and chains the exact remainder', () => {
  const { inner, outer } = nestedRoot()
  const { terminal, tui } = startAlt(outer, { wheelScrollLines: 2 })
  try {
    terminal.onInput('\x1b[<65;2;3M')
    assert.deepEqual(
      [inner.scrollTop, outer.scrollTop],
      [2, 0],
      'the nested pane receives wheel input before the primary pane',
    )

    inner.scrollToEnd()
    outer.scrollToStart()
    terminal.onInput('\x1b[<65;2;3M')
    assert.deepEqual(
      [inner.scrollTop, outer.scrollTop],
      [3, 2],
      'the inner remainder chains to the outer pane without amplification',
    )
  } finally {
    stopAlt(tui)
  }
})

test('Tier-1 wheel hit-testing does not collapse to the primary pane', () => {
  const { inner, outer } = nestedRoot({
    innerPrimary: false,
    outerPrimary: true,
  })
  const { terminal, tui } = startAlt(outer, { wheelScrollLines: 2 })
  try {
    terminal.onInput('\x1b[<65;2;3M')
    assert.deepEqual([inner.scrollTop, outer.scrollTop], [2, 0])
  } finally {
    stopAlt(tui)
  }
})

test('Tier-1 contain overscroll consumes the nested remainder', () => {
  const { inner, outer } = nestedRoot({ overscroll: 'contain' })
  const { terminal, tui } = startAlt(outer, { wheelScrollLines: 2 })
  try {
    inner.scrollToEnd()
    outer.scrollToStart()
    terminal.onInput('\x1b[<65;2;3M')
    assert.deepEqual([inner.scrollTop, outer.scrollTop], [3, 0])
  } finally {
    stopAlt(tui)
  }
})

test('Tier-1 capturing overlays defer wheel input to their focused component', () => {
  const view = new ScrollView(new Text('0\n1\n2\n3\n4\n5', 0, 0))
  const { terminal, tui } = startAlt(view, { columns: 6, rows: 3 })
  const inputs = []
  const overlay = {
    focused: false,
    render: () => ['overlay'],
    invalidate() {},
    handleInput(data) { inputs.push(data) },
  }
  try {
    tui.showOverlay(overlay, { width: 6, anchor: 'top-left' })
    terminal.onInput('\x1b[<65;1;1M')
    assert.equal(view.scrollTop, 0)
    assert.deepEqual(inputs, ['\x1b[<65;1;1M'])
  } finally {
    stopAlt(tui)
  }
})

function assertScrollbarPointerStateCleared(view, tui) {
  view.setScrollbar('hidden')
  view.setScrollbar('auto')
  assert.equal(
    view.isScrollbarVisible,
    false,
    'hover activation does not survive the lifecycle boundary',
  )
  view.setScrollbar('always')
  const before = view.scrollTop
  tui.handleTerminalInput('\x1b[<32;6;4M')
  assert.equal(
    view.scrollTop,
    before,
    'an in-progress thumb drag does not survive the lifecycle boundary',
  )
}

test('Tier-1 stop clears scrollbar hover activation and in-progress drag', () => {
  const view = new ScrollView(
    new Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const { terminal, tui } = startAlt(view, { columns: 6, rows: 4 })
  terminal.onInput('\x1b[<32;6;1M')
  terminal.onInput('\x1b[<0;6;1M')
  stopAlt(tui)
  assertScrollbarPointerStateCleared(view, tui)
})

test('Tier-1 focus-out clears scrollbar hover activation and in-progress drag', () => {
  const view = new ScrollView(
    new Text('0\n1\n2\n3\n4\n5\n6\n7\n8\n9', 0, 0),
    { scrollbar: 'always', scrollbarStyle: () => '#' },
  )
  const { terminal, tui } = startAlt(view, { columns: 6, rows: 4 })
  try {
    terminal.onInput('\x1b[<32;6;1M')
    terminal.onInput('\x1b[<0;6;1M')
    terminal.onInput('\x1b[O')
    assertScrollbarPointerStateCleared(view, tui)
  } finally {
    stopAlt(tui)
  }
})
