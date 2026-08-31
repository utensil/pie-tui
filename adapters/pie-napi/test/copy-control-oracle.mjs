import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

const distribution = process.env.PI_TUI_0844_DIST
assert.ok(distribution, 'PI_TUI_0844_DIST must point to authenticated pi-tui 0.84.4 dist')
const manifest = JSON.parse(await readFile(join(dirname(distribution), 'package.json'), 'utf8'))
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.4')
const sourceHashes = {
  'tui-alt-screen.js': '4e82f4d560558e4d704ddbf2414c3def2a14c28aaaa2985ff571dedec074946d',
  'tui-alt-screen.d.ts': '87165a2ced929067e379ccca8d226a6e07859012a8fb65c49b738594aa4e0729',
}
for (const [name, expected] of Object.entries(sourceHashes)) {
  const actual = createHash('sha256').update(await readFile(join(distribution, name))).digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

class RecordingTerminal {
  columns = 30
  rows = 8
  events = []
  kittyProtocolActive = false
  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize }
  stop() {}
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

function make(api, options = {}) {
  api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
  const terminal = new RecordingTerminal()
  const copied = []
  const copySelection = options.useCallback === false
    ? undefined
    : options.copySelection ?? (async (text) => { copied.push(text); return true })
  const tui = new api.TuiAltScreen(terminal, false, undefined, { ...options, copySelection })
  tui.getActiveSelectionText = () => undefined
  return { tui, terminal, copied }
}

async function snapshot(api) {
  const defaultTui = make(api).tui
  const disabled = make(api, { copyOnSelect: false }).tui
  assert.equal(defaultTui.getCopyOnSelect(), true)
  assert.equal(disabled.getCopyOnSelect(), false)
  disabled.setCopyOnSelect(true)
  assert.equal(disabled.getCopyOnSelect(), true)
  disabled.setCopyOnSelect(false)
  assert.equal(disabled.getCopyOnSelect(), false)

  const noSelection = make(api)
  assert.equal(noSelection.tui.hasActiveSelection(), false)
  assert.equal(await noSelection.tui.copyActiveSelectionToClipboard(), false)
  assert.deepEqual(noSelection.copied, [])

  const success = make(api)
  success.tui.getActiveSelectionText = () => 'alpha'
  assert.equal(success.tui.hasActiveSelection(), true)
  assert.equal(await success.tui.copyActiveSelectionToClipboard(), true)
  assert.deepEqual(success.copied, ['alpha'])

  const copiedFailure = []
  const failure = make(api, { copySelection: async (text) => { copiedFailure.push(text); return false } })
  failure.tui.getActiveSelectionText = () => 'beta'
  assert.equal(await failure.tui.copyActiveSelectionToClipboard(), false)
  assert.deepEqual(copiedFailure, ['beta'])

  const osc52 = make(api, { useCallback: false })
  osc52.tui.getActiveSelectionText = () => 'gamma'
  assert.equal(await osc52.tui.copyActiveSelectionToClipboard(), true)
  assert.ok(osc52.terminal.events.some((value) => value.startsWith('\x1b]52;c;')))
}

await snapshot(reference)
await snapshot(adapter)
console.log('copy-control oracle passed')
