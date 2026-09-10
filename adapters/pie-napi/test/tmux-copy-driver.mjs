import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const entry = process.argv[2]
const receiptPath = process.env.RECEIPT_PATH
assert.ok(entry && receiptPath, 'package entry and RECEIPT_PATH are required')
assert.equal(process.stdin.isTTY, true, 'driver must run in a real PTY')
assert.equal(process.stdout.isTTY, true, 'driver must run in a real PTY')

const api = await import(pathToFileURL(resolve(entry)))
api.setCapabilities({ images: null, trueColor: true, hyperlinks: true })
const baselineInputListeners = process.stdin.listenerCount('data')
const baselineResizeListeners = process.stdout.listenerCount('resize')
const stty = () => execFileSync('stty', ['-g'], { encoding: 'utf8', stdio: [0, 'pipe', 'pipe'] }).trim()
const baselineStty = stty()
const terminal = new api.ProcessTerminal()
const tui = new api.TuiAltScreen(terminal, false, undefined, { copyOnSelect: false })
const receipt = {
  ready: false,
  realTty: true,
  copyOnSelect: tui.getCopyOnSelect(),
  selected: false,
  explicitCopy: false,
  selectionCleared: false,
  terminalRestored: false,
  listenersRestored: false,
  exitCode: null,
}

function save() {
  writeFileSync(receiptPath, `${JSON.stringify(receipt)}\n`)
}

process.on('exit', (code) => {
  receipt.exitCode = code
  receipt.terminalRestored = process.stdin.isRaw === false && stty() === baselineStty
  receipt.listenersRestored = process.stdin.listenerCount('data') === baselineInputListeners &&
    process.stdout.listenerCount('resize') === baselineResizeListeners
  save()
})

let closing = false
let removeInputListener
let selectionPoll
async function close() {
  if (closing) return
  closing = true
  clearInterval(selectionPoll)
  removeInputListener?.()
  tui.stop({ preserveScreen: true })
  api.resetCapabilitiesCache()
  process.stdin.pause()
}

removeInputListener = tui.addInputListener((data) => {
  // This test-only binding calls the component API; the current consumer has no active-copy binding.
  if (data === 'c') {
    void tui.copyActiveSelectionToClipboard().then((copied) => {
      receipt.explicitCopy = copied
      save()
    }).catch(fail)
    return { consume: true }
  }
  if (data === 'q') {
    void close()
    return { consume: true }
  }
  return undefined
})

tui.setLayoutRoot(new api.Text('alpha beta\ngamma delta', 0, 0))
tui.start()
tui.renderNow(true)
let everSelected = false
selectionPoll = setInterval(() => {
  const selected = tui.hasActiveSelection()
  if (selected === receipt.selected) return
  receipt.selected = selected
  if (selected) everSelected = true
  else if (everSelected) receipt.selectionCleared = true
  save()
}, 20)
receipt.ready = true
save()

process.once('SIGINT', () => { void close() })
function fail(error) {
  process.exitCode = 1
  process.stderr.write(`${error.stack ?? error}\n`)
  void close()
}
process.once('uncaughtException', fail)
process.once('unhandledRejection', fail)
