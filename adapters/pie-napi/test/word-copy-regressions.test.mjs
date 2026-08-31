import assert from 'node:assert/strict'
import test from 'node:test'

import { TuiAltScreen } from '../index.js'

class RecordingTerminal {
  columns = 20
  rows = 5

  start(onInput, onResize) {
    this.onInput = onInput
    this.onResize = onResize
  }

  stop() {}
  async drainInput() {}
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

test('AltScreen flash defaults to the authenticated 0.84.2 one-second lifetime', () => {
  const originalSetTimeout = globalThis.setTimeout
  const delays = []
  globalThis.setTimeout = (_callback, delay) => {
    delays.push(delay)
    return { unref() {} }
  }
  try {
    const tui = new TuiAltScreen(new RecordingTerminal(), false)
    tui.flash('Copied!')
    assert.deepEqual(delays, [1000])
  } finally {
    globalThis.setTimeout = originalSetTimeout
  }
})

test('AltScreen focus-out resets all selection state', () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiAltScreen(terminal, false)
  tui.start()
  tui.selectionAnchor = { row: 0, col: 0 }
  tui.selectionFocus = { row: 0, col: 1 }
  tui.selectionGranularity = 'word'
  tui.selectionInitialRange = {
    start: { row: 0, col: 0 },
    end: { row: 0, col: 5, boundary: true },
  }
  tui.selectionDragged = true

  terminal.onInput('\x1b[O')

  assert.equal(tui.selectionAnchor, undefined)
  assert.equal(tui.selectionFocus, undefined)
  assert.equal(tui.selectionGranularity, 'character')
  assert.equal(tui.selectionInitialRange, undefined)
  assert.equal(tui.selectionDragged, false)
  tui.stop({ preserveScreen: true })
})

test('AltScreen stop and restart dispose transient flash entries and timers', () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiAltScreen(terminal, false)
  tui.start()
  const originalSetTimeout = globalThis.setTimeout
  const originalClearTimeout = globalThis.clearTimeout
  const timers = []
  globalThis.setTimeout = (callback, delay) => {
    const timer = { callback, delay, cleared: false, unref() {} }
    timers.push(timer)
    return timer
  }
  globalThis.clearTimeout = (timer) => {
    timer.cleared = true
  }
  try {
    tui.flash('Copied!')
    assert.equal(tui.flashes.length, 1)
    tui.stop({ preserveScreen: true })
    assert.equal(tui.flashes.length, 0)
    assert.equal(timers.length, 1)
    assert.equal(timers[0].cleared, true)

    tui.start()
    assert.equal(tui.flashes.length, 0)
    tui.stop({ preserveScreen: true })
  } finally {
    globalThis.setTimeout = originalSetTimeout
    globalThis.clearTimeout = originalClearTimeout
  }
})
