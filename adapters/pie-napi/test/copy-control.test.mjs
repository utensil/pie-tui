import assert from 'node:assert/strict'
import test from 'node:test'

import { TuiAltScreen } from '../index.js'

class Terminal {
  columns = 20
  rows = 5
  events = []
  write(value) { this.events.push(value) }
  start() {}
  stop() {}
  moveBy() {}
  hideCursor() {}
  showCursor() {}
  clearLine() {}
  clearFromCursor() {}
  clearScreen() {}
  setTitle() {}
  setProgress() {}
}

test('copyOnSelect defaults true and can be toggled', () => {
  const tui = new TuiAltScreen(new Terminal(), false)
  assert.equal(tui.getCopyOnSelect(), true)
  tui.setCopyOnSelect(false)
  assert.equal(tui.getCopyOnSelect(), false)
  tui.setCopyOnSelect(true)
  assert.equal(tui.getCopyOnSelect(), true)
})

test('active selection copy reports no selection and callback success/failure', async () => {
  const copied = []
  const tui = new TuiAltScreen(new Terminal(), false, undefined, {
    copySelection: async (text) => { copied.push(text); return true },
  })
  tui.getActiveSelectionText = () => undefined
  assert.equal(tui.hasActiveSelection(), false)
  assert.equal(await tui.copyActiveSelectionToClipboard(), false)
  tui.getActiveSelectionText = () => 'alpha'
  assert.equal(tui.hasActiveSelection(), true)
  assert.equal(await tui.copyActiveSelectionToClipboard(), true)
  assert.deepEqual(copied, ['alpha'])

  const failure = new TuiAltScreen(new Terminal(), false, undefined, {
    copySelection: async () => false,
  })
  failure.getActiveSelectionText = () => 'beta'
  assert.equal(await failure.copyActiveSelectionToClipboard(), false)
})

test('copyActiveSelectionToClipboard uses OSC 52 when no callback is supplied', async () => {
  const terminal = new Terminal()
  const tui = new TuiAltScreen(terminal, false)
  tui.getActiveSelectionText = () => 'gamma'
  assert.equal(await tui.copyActiveSelectionToClipboard(), true)
  assert.ok(terminal.events.some((value) => value.startsWith('\x1b]52;c;')))
})

test('copyOnSelect gates automatic release copying', () => {
  const terminal = new Terminal()
  const tui = new TuiAltScreen(terminal, false, undefined, { copyOnSelect: false })
  let copies = 0
  tui.copySelectionToClipboard = () => { copies += 1 }
  tui.selectionPressActive = true
  tui.selectionAnchor = { row: 0, col: 0 }
  tui.selectionFocus = { row: 0, col: 1 }
  tui.handleSelectionMouseEvent({ button: 0, x: 1, y: 0, release: true })
  assert.equal(copies, 0)

  const enabled = new TuiAltScreen(new Terminal(), false)
  enabled.copySelectionToClipboard = () => { copies += 1 }
  enabled.selectionPressActive = true
  enabled.selectionAnchor = { row: 0, col: 0 }
  enabled.selectionFocus = { row: 0, col: 1 }
  enabled.handleSelectionMouseEvent({ button: 0, x: 1, y: 0, release: true })
  assert.equal(copies, 1)
})
