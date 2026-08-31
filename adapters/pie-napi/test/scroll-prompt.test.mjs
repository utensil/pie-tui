import assert from 'node:assert/strict'
import test from 'node:test'

import { ScrollView, setCapabilities, Text, TuiAltScreen } from '../index.js'

class RecordingTerminal {
  columns = 20
  rows = 4
  kittyProtocolActive = false
  events = []
  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize }
  stop() {}
  async drainInput() {}
  write(data) { this.events.push(data) }
  moveBy() {}
  hideCursor() {}
  showCursor() {}
  clearLine() {}
  clearFromCursor() {}
  clearScreen() {}
  setTitle() {}
  setProgress() {}
}

function createPromptTui() {
  setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const lines = [
    'before',
    '\x1b]133;A\x07BEL prompt',
    'between one',
    'between two',
    '\x1b]133;A\x1b\\ST prompt',
    'between three',
    'between four',
    '\x1b]133;A\x07second BEL prompt',
    'tail one',
    'tail two',
    'tail three',
    'tail four',
  ].join('\n')
  const root = new ScrollView(new Text(lines, 0, 0), { primary: true, follow: 'none' })
  const tui = new TuiAltScreen(terminal, false, undefined)
  tui.setLayoutRoot(root)
  tui.start()
  tui.renderNow(true)
  return { root, terminal, tui }
}

test('AltScreen scrollToPrompt navigates BEL and ST OSC-133 prompt markers', () => {
  const { root, tui } = createPromptTui()
  const navigate = (start, direction) => {
    root.scrollTo(start)
    tui.renderNow(true)
    const before = root.scrollTop
    tui.scrollToPrompt(direction)
    return { before, after: root.scrollTop }
  }
  assert.deepEqual({
    nextFromTop: navigate(0, 1),
    nextFromFirst: navigate(1, 1),
    previousFromSecond: navigate(4, -1),
    previousFromTop: navigate(0, -1),
  }, {
    nextFromTop: { before: 0, after: 1 },
    nextFromFirst: { before: 1, after: 4 },
    previousFromSecond: { before: 4, after: 1 },
    previousFromTop: { before: 0, after: 0 },
  })
  tui.stop({ preserveScreen: true })
})

test('AltScreen scrollToPrompt is a no-op without markers or before layout', () => {
  const terminal = new RecordingTerminal()
  const tui = new TuiAltScreen(terminal, false, undefined)
  assert.doesNotThrow(() => tui.scrollToPrompt(1))
  const root = new ScrollView(new Text(
    Array.from({ length: 12 }, (_, index) => `plain-${index}`).join('\n'),
    0,
    0,
  ), { primary: true, follow: 'none' })
  tui.setLayoutRoot(root)
  tui.start()
  tui.renderNow(true)
  root.scrollTo(4)
  tui.renderNow(true)
  tui.scrollToPrompt(1)
  assert.equal(root.scrollTop, 4)
  tui.scrollToPrompt(-1)
  assert.equal(root.scrollTop, 4)
  tui.stop({ preserveScreen: true })
})
