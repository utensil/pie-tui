import assert from 'node:assert/strict'


assert.equal(typeof global.gc, 'function', 'run with --expose-gc')
const { Editor } = await import('../index.js')
const plain = (text) => text
const theme = {
  borderColor: plain,
  selectList: {
    selectedPrefix: plain, selectedText: plain, description: plain,
    scrollInfo: plain, noMatch: plain,
  },
}
function abandonedEditor() {
  const editor = new Editor({ terminal: { rows: 24 }, requestRender() {} }, theme)
  const provider = {
    // A legal public provider can reference its editor, e.g. to replace a
    // provider during getSuggestions. No request is started by this fixture.
    getSuggestions() { return editor.getText() ? null : null },
    applyCompletion() { return { lines: [''], cursorLine: 0, cursorCol: 0 } },
  }
  editor.setAutocompleteProvider(provider)
  return { editor: new WeakRef(editor), provider: new WeakRef(provider) }
}
const references = abandonedEditor()
// WeakRef keeps its target alive for the job in which it is created. Yield
// before collection and do not dereference until after the final GC round.
await new Promise((resolve) => setImmediate(resolve))
for (let round = 0; round < 20; round += 1) {
  global.gc()
  await new Promise((resolve) => setImmediate(resolve))
}
const receipt = {
  editorRetained: references.editor.deref() !== undefined,
  providerRetained: references.provider.deref() !== undefined,
}
console.log(JSON.stringify(receipt))
{
  assert.equal(receipt.editorRetained, false, 'abandoned editor is rooted by native callback references')
  assert.equal(receipt.providerRetained, false, 'abandoned provider is rooted by native callback references')
}
