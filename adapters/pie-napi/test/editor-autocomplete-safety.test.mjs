import assert from 'node:assert/strict'
import test from 'node:test'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { Worker } from 'node:worker_threads'
import { Editor } from '../index.js'

const plain = (text) => text
const theme = {
  borderColor: plain,
  selectList: {
    selectedPrefix: plain, selectedText: plain, description: plain,
    scrollInfo: plain, noMatch: plain,
  },
}
const makeEditor = () => new Editor({ terminal: { rows: 24 }, requestRender() {} }, theme)
const delay = () => new Promise((resolve) => setTimeout(resolve, 5))
async function waitFor(predicate) {
  const deadline = Date.now() + 1000
  while (!predicate() && Date.now() < deadline) await delay()
  assert.ok(predicate(), 'autocomplete did not reach the expected state')
}
const provider = {
  shouldTriggerFileCompletion() { return true },
  getSuggestions(lines, line, col) {
    return { prefix: lines[line].slice(0, col), items: [{ value: 'done', label: 'done' }] }
  },
  applyCompletion() { return { lines: ['done'], cursorLine: 0, cursorCol: 4 } },
}
async function recover(editor) {
  editor.setAutocompleteProvider(provider)
  editor.setText('file')
  editor.handleInput('\t')
  await waitFor(() => editor.getText() === 'done')
}

for (const key of ['\t', '\r']) {
  for (const reenter of [false, true]) {
    test(`failed apply preserves text and suppresses submit (${JSON.stringify(key)}, reentry=${reenter})`, async () => {
      const editor = makeEditor()
      const failure = new Error('completion failed')
      editor.setAutocompleteProvider({
        ...provider,
        applyCompletion() {
          if (reenter) editor.setText('reentrant edit')
          throw failure
        },
      })
      editor.handleInput('/')
      editor.handleInput('m')
      await waitFor(() => editor.isShowingAutocomplete())
      const cursor = editor.getCursor()
      const changes = []
      const submissions = []
      editor.onChange = (text) => changes.push(text)
      editor.onSubmit = (text) => submissions.push(text)
      assert.throws(() => editor.handleInput(key), reenter ? /borrow/i : (error) => error === failure)
      assert.equal(editor.getText(), '/m')
      assert.deepEqual(editor.getCursor(), cursor)
      assert.deepEqual(changes, [])
      assert.deepEqual(submissions, [])
      await recover(editor)
    })
  }
}

test('sync predicate reentry is rejected and the editor remains usable', async () => {
  const editor = makeEditor()
  editor.setText('file')
  editor.setAutocompleteProvider({ ...provider, shouldTriggerFileCompletion() { return editor.getText().length > 0 } })
  assert.throws(() => editor.handleInput('\t'), /borrow/i)
  assert.equal(editor.getText(), 'file')
  await recover(editor)
})

test('async provider and forced-apply failures release subsequent requests', async () => {
  for (const failure of [
    { getSuggestions() { throw new Error('sync request failure') } },
    { getSuggestions() { return Promise.reject(new Error('async request failure')) } },
    { getSuggestions() { return { prefix: 'file', items: [{}] } } },
    { applyCompletion() { throw new Error('async forced apply failure') } },
  ]) {
    const editor = makeEditor()
    let requested = false
    const broken = { ...provider, ...failure }
    editor.setAutocompleteProvider({
      ...broken,
      getSuggestions(...args) { requested = true; return broken.getSuggestions(...args) },
    })
    editor.setText('file')
    editor.handleInput('\t')
    await waitFor(() => requested)
    await delay()
    assert.equal(editor.getText(), 'file')
    await recover(editor)
  }
})

test('text mutations and submission abort the live signal and discard stale results', async () => {
  for (const mutate of [
    (editor) => editor.setText('new text'),
    (editor) => editor.insertTextAtCursor('new text'),
    (editor) => editor.handleInput('\r'),
  ]) {
    const editor = makeEditor()
    let signal
    let resolve
    editor.setAutocompleteProvider({
      ...provider,
      getSuggestions(_lines, _line, _col, options) {
        signal = options.signal
        return new Promise((settle) => { resolve = settle })
      },
    })
    editor.handleInput('/')
    await waitFor(() => signal !== undefined)
    mutate(editor)
    assert.equal(signal.aborted, true)
    const text = editor.getText()
    resolve({ prefix: '/', items: [{ value: 'stale', label: 'stale' }] })
    await delay()
    assert.equal(editor.getText(), text)
    assert.equal(editor.isShowingAutocomplete(), false)
  }
})

test('provider hooks run in independent worker-thread environments', async () => {
  await Promise.all(Array.from({ length: 3 }, () => new Promise((resolve, reject) => {
    const worker = new Worker(`
      const { parentPort, workerData } = require('node:worker_threads')
      import(workerData.adapter).then(async ({ Editor }) => {
        const plain = text => text
        const editor = new Editor({ requestRender() {}, terminal: { rows: 24 } }, {
          borderColor: plain,
          selectList: { selectedPrefix: plain, selectedText: plain, description: plain, scrollInfo: plain, noMatch: plain },
        })
        const provider = {
          shouldTriggerFileCompletion() { return this === provider },
          getSuggestions() { return { prefix: 'file', items: [{ value: 'done', label: 'done' }] } },
          applyCompletion() {
            if (this !== provider) throw Error('lost receiver')
            return { lines: ['done'], cursorLine: 0, cursorCol: 4 }
          },
        }
        editor.setAutocompleteProvider(provider)
        editor.setText('file')
        editor.handleInput('\\t')
        const deadline = Date.now() + 1000
        while (editor.getText() !== 'done' && Date.now() < deadline) {
          await new Promise(resolve => setTimeout(resolve, 5))
        }
        parentPort.postMessage(editor.getText())
      }).catch(error => { throw error })
    `, { eval: true, workerData: { adapter: new URL('../index.js', import.meta.url).href } })
    let result
    worker.on('message', (message) => { result = message })
    worker.on('error', reject)
    worker.on('exit', (code) => {
      try {
        assert.equal(code, 0)
        assert.equal(result, 'done')
        resolve()
      } catch (error) { reject(error) }
    })
  })))
  await recover(makeEditor())
})

test('native hooks allow an abandoned provider and editor to be collected', () => {
  const result = spawnSync(process.execPath, ['--expose-gc', fileURLToPath(new URL('./editor-autocomplete-gc.mjs', import.meta.url))], { encoding: 'utf8', timeout: 15000 })
  assert.equal(result.status, 0, result.stderr || result.stdout)
})

test('a predicate failure still delivers the committed text change exactly once', async () => {
  const editor = makeEditor()
  const failure = new Error('predicate exploded')
  let throwPredicate = false
  editor.setAutocompleteProvider({
    ...provider,
    shouldTriggerFileCompletion() {
      if (throwPredicate) throw failure
      return true
    },
    getSuggestions() {
      return { prefix: 'file', items: [{ value: 'first', label: 'first' }, { value: 'second', label: 'second' }] }
    },
  })
  editor.setText('file')
  editor.handleInput('\t')
  await waitFor(() => editor.isShowingAutocomplete())
  const changes = []
  editor.onChange = (text) => changes.push(text)
  throwPredicate = true
  assert.throws(() => editor.handleInput('x'), (error) => error === failure)
  assert.equal(editor.getText(), 'filex')
  assert.deepEqual(changes, ['filex'])
  await recover(editor)
  assert.equal(changes.filter((text) => text === 'filex').length, 1)
})
