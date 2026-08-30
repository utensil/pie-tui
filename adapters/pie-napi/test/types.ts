import {
  CombinedAutocompleteProvider,
  CURSOR_MARKER,
  Editor,
  Key,
  Marked,
  Spacer,
  Text,
  TuiAltScreen,
  TuiMainScreen,
  TUI_KEYBINDINGS,
  allocateImageId,
  compositeTuiLine,
  detectCapabilities,
  encodeKitty,
  fuzzyFilter,
  fuzzyMatch,
  getCapabilities,
  imageFallback,
  renderImage,
  renderLatex,
  setCapabilities,
  setCapabilityOverrides,
  type CellDimensions,
  type Component,
  type ImageRenderOptions,
  type Keybinding,
  type KeyId,
  type TerminalCapabilities,
  type Terminal,
} from '../index.js'

const key: KeyId = Key.ctrlShift('p')
const cell: CellDimensions = { widthPx: 9, heightPx: 18 }
const capabilities: TerminalCapabilities = detectCapabilities(() => false)
const spacer: Component = new Spacer(2)
const binding: Keybinding = 'tui.editor.cursorLeft'
const imageOptions: ImageRenderOptions = {
  maxWidthCells: 80,
  imageId: allocateImageId(),
}
const cursorMarker: '\u001B_pi:c\u0007' = CURSOR_MARKER
const cursorLeftDefaults: ['left', 'ctrl+b'] =
  TUI_KEYBINDINGS['tui.editor.cursorLeft'].defaultKeys
const terminal: Terminal = {
  start() {},
  stop() {},
  async drainInput() {},
  write() {},
  columns: 80,
  rows: 24,
  kittyProtocolActive: false,
  moveBy() {},
  hideCursor() {},
  showCursor() {},
  clearLine() {},
  clearFromCursor() {},
  clearScreen() {},
  setTitle() {},
  setProgress() {},
}
const mainScreen = new TuiMainScreen(terminal)
const altScreen = new TuiAltScreen(terminal)
const autocomplete = new CombinedAutocompleteProvider([], process.cwd())
const editor = new Editor(mainScreen, {
  borderColor: (text) => text,
  selectList: {
    selectedPrefix: (text) => text,
    selectedText: (text) => text,
    description: (text) => text,
    scrollInfo: (text) => text,
    noMatch: (text) => text,
  },
})
editor.setAutocompleteProvider(autocomplete)
mainScreen.addChild(new Text('M5'))
altScreen.setLayoutRoot(mainScreen)
setCapabilities(capabilities)
setCapabilityOverrides({ images: 'kitty' })

void key
void cell
void getCapabilities()
void CURSOR_MARKER
void cursorMarker
void cursorLeftDefaults
void TUI_KEYBINDINGS[binding].defaultKeys
void spacer.render(80)
void fuzzyMatch('fb', 'fooBar').score
void fuzzyFilter([{ text: 'foo' }], 'fo', (item) => item.text)
void compositeTuiLine('base', 'overlay', 1, 2, 10)
void renderImage('QQ==', { widthPx: 1, heightPx: 1 }, imageOptions)
void imageFallback('image/png', { widthPx: 1, heightPx: 1 }, '/tmp/a.png')
void renderLatex('x', { display: true })
void encodeKitty('QQ==', { imageId: allocateImageId(), moveCursor: false })
void new Marked()
void editor.render(80)
void altScreen.isFollowingOutput
