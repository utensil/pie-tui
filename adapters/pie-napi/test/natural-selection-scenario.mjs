import assert from 'node:assert/strict'

const press = '\x1b[<0;1;1M'
const drag = '\x1b[<32;5;1M'
const release = '\x1b[<0;5;1m'
const clickRelease = '\x1b[<0;1;1m'

export class RecordingTerminal {
  columns = 30
  rows = 8
  events = []
  kittyProtocolActive = false
  started = false
  stopped = false

  start(onInput, onResize) {
    this.onInput = onInput
    this.onResize = onResize
    this.started = true
  }

  stop() { this.stopped = true }
  write(value) { this.events.push(value) }
  moveBy() {}
  hideCursor() {}
  showCursor() {}
  clearLine() {}
  clearFromCursor() {}
  clearScreen() {}
  setTitle() {}
  setProgress() {}
}

export const naturalSelectionInput = { press, drag, release, clickRelease }

export async function exerciseNaturalSelection(api) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const copied = []
  const tui = new api.TuiAltScreen(terminal, false, undefined, {
    copyOnSelect: false,
    copySelection: async (text) => {
      copied.push(text)
      return true
    },
  })
  tui.setLayoutRoot(new api.Text('alpha beta\ngamma delta', 0, 0))

  try {
    tui.start()
    tui.renderNow(true)

    assert.equal(tui.getCopyOnSelect(), false)
    assert.equal(tui.hasActiveSelection(), false, 'a fresh TUI has no selection')
    assert.equal(await tui.copyActiveSelectionToClipboard(), false, 'copy with no selection is canonical false')

    terminal.onInput(press)
    terminal.onInput(drag)
    terminal.onInput(release)
    await new Promise((resolve) => setImmediate(resolve))

    assert.deepEqual(copied, [], 'copyOnSelect:false must not copy on mouse release')
    assert.equal(tui.hasActiveSelection(), true, 'the real SGR drag creates an active selection')
    assert.equal(await tui.copyActiveSelectionToClipboard(), true, 'explicit copy succeeds')
    assert.deepEqual(copied, ['alpha'])

    terminal.onInput(press)
    terminal.onInput(clickRelease)
    await new Promise((resolve) => setImmediate(resolve))
    assert.equal(tui.hasActiveSelection(), false, 'a plain click clears the non-empty selection')
    assert.equal(await tui.copyActiveSelectionToClipboard(), false, 'copy after clearing is false')
    assert.deepEqual(copied, ['alpha'])

    return {
      naturalMouseSelection: true,
      selectedText: copied[0],
      copyOnSelect: tui.getCopyOnSelect(),
      initialCopy: false,
      implicitCopies: 0,
      explicitCopy: true,
      clearedCopy: false,
    }
  } finally {
    tui.stop({ preserveScreen: true })
    assert.equal(terminal.stopped, true, 'stop disposes the terminal session')
    api.resetCapabilitiesCache()
  }
}

export async function exerciseNaturalCopyControls(api) {
  const natural = await exerciseNaturalSelection(api)
  const cases = [
    { name: 'default automatic release', auto: true, callback: true, succeeds: true },
    { name: 'callback failure', auto: false, callback: true, succeeds: false },
    { name: 'OSC 52 transport', auto: false, callback: false, succeeds: true },
    { name: 'ScrollView source', auto: false, callback: true, succeeds: true, scroll: true },
    { name: 'empty selection text', auto: false, callback: true, succeeds: false, empty: true },
  ]
  const receipts = []
  for (const scenario of cases) {
    api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
    const terminal = new RecordingTerminal()
    const copied = []
    const options = scenario.auto ? {} : { copyOnSelect: false }
    if (scenario.callback) options.copySelection = async (text) => {
      copied.push(text)
      return scenario.succeeds
    }
    const tui = new api.TuiAltScreen(terminal, false, undefined, options)
    const text = new api.Text(scenario.empty ? '     ' : 'alpha beta\ngamma delta', 0, 0)
    tui.setLayoutRoot(scenario.scroll ? new api.ScrollView(text, { primary: true }) : text)
    try {
      tui.start()
      tui.renderNow(true)
      for (const input of [press, drag, release]) terminal.onInput(input)
      await new Promise((resolve) => setImmediate(resolve))
      assert.equal(tui.hasActiveSelection(), !scenario.empty, scenario.name)
      if (!scenario.auto) {
        assert.deepEqual(copied, [], `${scenario.name}: no implicit callback`)
        assert.equal(terminal.events.some((value) => value.startsWith('\x1b]52;c;')), false)
      }
      const result = scenario.auto ? true : await tui.copyActiveSelectionToClipboard()
      assert.equal(result, scenario.succeeds, scenario.name)
      assert.deepEqual(copied, scenario.callback && !scenario.empty ? ['alpha'] : [], scenario.name)
      const osc52 = terminal.events.filter((value) => value.startsWith('\x1b]52;c;'))
      assert.deepEqual(osc52, scenario.callback ? [] : ['\x1b]52;c;YWxwaGE=\x07'], scenario.name)
      receipts.push({ name: scenario.name, result, copied, osc52 })
    } finally {
      tui.stop({ preserveScreen: true })
      assert.equal(terminal.stopped, true)
      api.resetCapabilitiesCache()
    }
  }
  return { natural, cases: receipts }
}
