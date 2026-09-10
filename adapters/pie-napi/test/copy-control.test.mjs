import assert from 'node:assert/strict'
import test from 'node:test'

import * as api from '../index.js'
import { exerciseNaturalCopyControls } from './natural-selection-scenario.mjs'
const { TuiAltScreen } = api

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

test('active selection copy reports natural drag, clear, callback and OSC 52 behavior', async () => {
  await exerciseNaturalCopyControls(api)
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
