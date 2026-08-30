#!/usr/bin/env node
import {
  ProcessTerminal,
  TuiAltScreen,
  TuiMainScreen,
} from '../../index.js'

const mode = process.argv[2] === 'alt' ? 'alt' : 'main'
const terminal = new ProcessTerminal()
const screen = mode === 'alt'
  ? new TuiAltScreen(terminal, false)
  : new TuiMainScreen(terminal, false)

let count = 0
const component = {
  focused: false,
  invalidate() {},
  render() {
    return [
      `pie-tui-native ${mode} screen`,
      'reference contract: pi-tui 0.84.2',
      `count: ${count}`,
      `viewport: ${terminal.columns}x${terminal.rows}`,
      'j: increment   q: quit',
    ]
  },
  handleInput(data) {
    if (data === 'j') {
      count += 1
      screen.requestRender(true)
    } else if (data === 'q') {
      screen.stop()
    }
  },
}

if (mode === 'alt') screen.setLayoutRoot(component)
else screen.addChild(component)
screen.setFocus(component)
screen.start()
