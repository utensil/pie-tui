/**
 * `renderLatex`, `encodeKitty`, and the iTerm2 payload preserve raw UTF-16.
 * Other string inputs entering the native core must be well-formed UTF-16;
 * unpaired surrogates throw `RangeError`.
 * @packageDocumentation
 */
import { EventEmitter } from 'node:events'

export { Marked } from 'marked'
export type { Token, Tokens } from 'marked'

export interface RenderLatexOptions {
  display?: boolean
}

export interface RgbColor {
  r: number
  g: number
  b: number
}

export type TerminalColorScheme = 'dark' | 'light'
export type ImageProtocol = 'kitty' | 'iterm2' | null

export interface TerminalCapabilities {
  images: ImageProtocol
  trueColor: boolean
  hyperlinks: boolean
}

export interface CellDimensions {
  widthPx: number
  heightPx: number
}

export interface ImageDimensions {
  widthPx: number
  heightPx: number
}

export interface ImageRenderOptions {
  maxWidthCells?: number
  maxHeightCells?: number
  preserveAspectRatio?: boolean
  imageId?: number
  moveCursor?: boolean
}

export interface FuzzyMatch {
  matches: boolean
  score: number
}

export interface Component {
  render(width: number): string[]
  handleInput?(data: string): void
  wantsKeyRelease?: boolean
  invalidate(): void
}

export interface EditorComponent extends Component {
  getText(): string
  setText(text: string): void
  handleInput(data: string): void
  onSubmit?: (text: string) => void
  onChange?: (text: string) => void
  addToHistory?(text: string): void
  insertTextAtCursor?(text: string): void
  getExpandedText?(): string
  setAutocompleteProvider?(provider: AutocompleteProvider): void
  borderColor?: (text: string) => string
  setPaddingX?(padding: number): void
  setAutocompleteMaxVisible?(maxVisible: number): void
}

export interface Focusable {
  focused: boolean
}

export function isFocusable(
  component: Component | null,
): component is Component & Focusable

export interface ViewportTUI extends TUI {
  readonly mode: 'fullscreen'
  setLayoutRoot(component: Component | undefined): void
}

export function isViewportTUI(tui: TUI): tui is ViewportTUI

export class Text implements Component {
  constructor(
    text?: string,
    paddingX?: number,
    paddingY?: number,
    customBgFn?: (text: string) => string,
  )
  setText(text: string): void
  setCustomBgFn(customBgFn?: (text: string) => string): void
  invalidate(): void
  render(width: number): string[]
}

export class TruncatedText implements Component {
  constructor(text: string, paddingX?: number, paddingY?: number)
  invalidate(): void
  render(width: number): string[]
}

export class Input implements Component {
  onSubmit?: (value: string) => void
  onEscape?: () => void
  focused: boolean
  getValue(): string
  setValue(value: string): void
  handleInput(data: string): void
  invalidate(): void
  render(width: number): string[]
}

interface TUIRenderRequester {
  requestRender(force?: boolean): void
}

export interface LoaderIndicatorOptions {
  frames?: string[]
  intervalMs?: number
}

export class Loader extends Text {
  constructor(
    ui: TUIRenderRequester,
    spinnerColorFn: (str: string) => string,
    messageColorFn: (str: string) => string,
    message?: string,
    indicator?: LoaderIndicatorOptions,
  )
  render(width: number): string[]
  start(): void
  stop(): void
  setMessage(message: string): void
  setIndicator(indicator?: LoaderIndicatorOptions): void
}

export class CancellableLoader extends Loader {
  onAbort?: () => void
  readonly signal: AbortSignal
  readonly aborted: boolean
  handleInput(data: string): void
  dispose(): void
}

export class Container implements Component {
  children: Component[]
  addChild(component: Component): void
  removeChild(component: Component): void
  clear(): void
  invalidate(): void
  render(width: number): string[]
}

export class Box extends Container {
  constructor(
    paddingX?: number,
    paddingY?: number,
    bgFn?: (text: string) => string,
  )
  setBgFn(bgFn?: (text: string) => string): void
}

export interface StackEntryOptions {
  basis?: number | 'auto'
  grow?: number
  shrink?: number
  minSize?: number
  maxSize?: number
  visible?: (viewport: { width: number; height: number }) => boolean
}

export interface StackEntry extends StackEntryOptions {
  component: Component
}

export type StackChild = Component | StackEntry

export interface StackOptions {
  gap?: number
  align?: 'stretch' | 'start' | 'center' | 'end'
}

export class VStack extends Container {
  constructor(children?: StackChild[], options?: StackOptions)
  addChild(component: Component, options?: StackEntryOptions): void
  removeChild(component: Component): void
  clear(): void
  render(width: number): string[]
}

export class HStack extends Container {
  constructor(children?: StackChild[], options?: StackOptions)
  addChild(component: Component, options?: StackEntryOptions): void
  removeChild(component: Component): void
  clear(): void
  render(width: number): string[]
}

export interface StdinBufferOptions {
  timeout?: number
}

export interface StdinBufferEventMap {
  data: [string]
  paste: [string]
}

export class StdinBuffer extends EventEmitter<StdinBufferEventMap> {
  constructor(options?: StdinBufferOptions)
  process(data: string | Buffer): void
  flush(): string[]
  clear(): void
  getBuffer(): string
  destroy(): void
}

export interface AutocompleteItem {
  value: string
  label: string
  description?: string
}

export interface SlashCommand {
  name: string
  description?: string
  argumentHint?: string
  getArgumentCompletions?(
    argumentPrefix: string,
  ): AutocompleteItem[] | null | Promise<AutocompleteItem[] | null>
}

export interface AutocompleteSuggestions {
  items: AutocompleteItem[]
  prefix: string
}

export interface AutocompleteProvider {
  triggerCharacters?: string[]
  getSuggestions(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
    options: { signal: AbortSignal; force?: boolean },
  ): Promise<AutocompleteSuggestions | null>
  applyCompletion(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
    item: AutocompleteItem,
    prefix: string,
  ): { lines: string[]; cursorLine: number; cursorCol: number }
  shouldTriggerFileCompletion?(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
  ): boolean
}

export class CombinedAutocompleteProvider implements AutocompleteProvider {
  readonly triggerCharacters: string[]
  constructor(
    commands: (AutocompleteItem | SlashCommand)[] | undefined,
    basePath: string,
    fdPath?: string | null,
  )
  getSuggestions(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
    options: { signal: AbortSignal; force?: boolean },
  ): Promise<AutocompleteSuggestions | null>
  applyCompletion(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
    item: AutocompleteItem,
    prefix: string,
  ): { lines: string[]; cursorLine: number; cursorCol: number }
  shouldTriggerFileCompletion(
    lines: string[],
    cursorLine: number,
    cursorCol: number,
  ): boolean
}

export interface SelectItem extends AutocompleteItem {}
export interface SelectListTheme {
  selectedPrefix: (text: string) => string
  selectedText: (text: string) => string
  description: (text: string) => string
  scrollInfo: (text: string) => string
  noMatch: (text: string) => string
}
export interface SelectListTruncatePrimaryContext {
  text: string
  maxWidth: number
  columnWidth: number
  item: SelectItem
  isSelected: boolean
}
export interface SelectListLayoutOptions {
  minPrimaryColumnWidth?: number
  maxPrimaryColumnWidth?: number
  truncatePrimary?: (context: SelectListTruncatePrimaryContext) => string
}
export class SelectList implements Component {
  onSelect?: (item: SelectItem) => void
  onCancel?: () => void
  onSelectionChange?: (item: SelectItem) => void
  constructor(
    items: SelectItem[],
    maxVisible: number,
    theme: SelectListTheme,
    layout?: SelectListLayoutOptions,
  )
  setFilter(filter: string): void
  setSelectedIndex(index: number): void
  getSelectedItem(): SelectItem | null
  handleInput(data: string): void
  invalidate(): void
  render(width: number): string[]
}

export interface EditorTheme {
  borderColor: (text: string) => string
  selectList: SelectListTheme
}
export interface EditorOptions {
  paddingX?: number
  autocompleteMaxVisible?: number
}
export class Editor implements EditorComponent, Focusable {
  focused: boolean
  borderColor: (text: string) => string
  onSubmit?: (text: string) => void
  onChange?: (text: string) => void
  disableSubmit: boolean
  constructor(tui: TUI, theme: EditorTheme, options?: EditorOptions)
  getPaddingX(): number
  setPaddingX(padding: number): void
  getAutocompleteMaxVisible(): number
  setAutocompleteMaxVisible(maxVisible: number): void
  setAutocompleteProvider(provider: AutocompleteProvider): void
  addToHistory(text: string): void
  handleInput(data: string): void
  getText(): string
  getExpandedText(): string
  getLines(): string[]
  getCursor(): { line: number; col: number }
  setText(text: string): void
  insertTextAtCursor(text: string): void
  isShowingAutocomplete(): boolean
  invalidate(): void
  render(width: number): string[]
}

export interface ImageTheme {
  fallbackColor: (text: string) => string
}
export interface ImageOptions {
  maxWidthCells?: number
  maxHeightCells?: number
  filename?: string
  imageId?: number
}
export class Image implements Component {
  constructor(
    base64Data: string,
    mimeType: string,
    theme: ImageTheme,
    options?: ImageOptions,
    dimensions?: ImageDimensions,
  )
  getImageId(): number | undefined
  invalidate(): void
  render(width: number): string[]
}

export interface DefaultTextStyle {
  color?: (text: string) => string
  bgColor?: (text: string) => string
  bold?: boolean
  italic?: boolean
  strikethrough?: boolean
  underline?: boolean
}
export interface MarkdownTheme {
  heading: (text: string) => string
  link: (text: string) => string
  linkUrl: (text: string) => string
  code: (text: string) => string
  codeBlock: (text: string) => string
  codeBlockBorder: (text: string) => string
  quote: (text: string) => string
  quoteBorder: (text: string) => string
  hr: (text: string) => string
  listBullet: (text: string) => string
  bold: (text: string) => string
  italic: (text: string) => string
  strikethrough: (text: string) => string
  underline: (text: string) => string
  highlightCode?: (code: string, lang?: string) => string[]
  codeBlockIndent?: string
}
export interface MarkdownOptions {
  preserveOrderedListMarkers?: boolean
  preserveBackslashEscapes?: boolean
  transform?: (markdown: string, availableWidth: number) => string
  renderLatex?: boolean
}
export class Markdown implements Component {
  constructor(
    text: string,
    paddingX: number,
    paddingY: number,
    theme: MarkdownTheme,
    defaultTextStyle?: DefaultTextStyle,
    options?: MarkdownOptions,
  )
  setText(text: string): void
  invalidate(): void
  render(width: number): string[]
}

export type ScrollViewScrollbar = 'hidden' | 'auto' | 'always'
export interface ScrollViewOptions {
  axis?: 'vertical'
  follow?: 'none' | 'end'
  primary?: boolean
  overscroll?: 'chain' | 'contain'
  scrollbar?: ScrollViewScrollbar
  scrollbarStyle?: (text: string) => string
  scrollbarHideDelayMs?: number
}

export interface ScrollViewScrollToOptions {
  /** Keep follow-end disabled even when the target is the current content end. */
  disableFollow?: boolean
}
export class ScrollView extends Container {
  readonly primary: boolean
  readonly overscroll: 'chain' | 'contain'
  readonly scrollTop: number
  readonly isFollowingEnd: boolean
  readonly viewportHeight: number
  readonly scrollbar: ScrollViewScrollbar
  readonly isScrollbarVisible: boolean
  constructor(component: Component, options?: ScrollViewOptions)
  setScrollbar(scrollbar: ScrollViewScrollbar): void
  getContentWidth(width: number): number
  setScrollbarActive(active: boolean): void
  scrollTo(scrollTop: number, options?: ScrollViewScrollToOptions): void
  scrollBy(lines: number): number
  scrollToStart(): void
  scrollToEnd(): void
  updateLayout(
    contentHeight: number,
    viewportHeight: number,
    requestRender: () => void,
  ): void
}

export interface SettingItem {
  id: string
  label: string
  description?: string
  currentValue: string
  values?: string[]
  submenu?: (
    currentValue: string,
    done: (selectedValue?: string) => void,
  ) => Component
}
export interface SettingsListTheme {
  label: (text: string, selected: boolean) => string
  value: (text: string, selected: boolean) => string
  description: (text: string) => string
  cursor: string
  hint: (text: string) => string
}
interface SettingsListOptions {
  enableSearch?: boolean
}
export class SettingsList implements Component {
  constructor(
    items: SettingItem[],
    maxVisible: number,
    theme: SettingsListTheme,
    onChange: (id: string, newValue: string) => void,
    onCancel: () => void,
    options?: SettingsListOptions,
  )
  updateValue(id: string, newValue: string): void
  handleInput(data: string): void
  invalidate(): void
  render(width: number): string[]
}

export interface Terminal {
  start(onInput: (data: string) => void, onResize: () => void): void
  stop(): void
  drainInput(maxMs?: number, idleMs?: number): Promise<void>
  write(data: string): void
  readonly columns: number
  readonly rows: number
  readonly kittyProtocolActive: boolean
  moveBy(lines: number): void
  hideCursor(): void
  showCursor(): void
  clearLine(): void
  clearFromCursor(): void
  clearScreen(): void
  setTitle(title: string): void
  setProgress(active: boolean): void
}

export class ProcessTerminal implements Terminal {
  readonly kittyProtocolActive: boolean
  readonly modifyOtherKeysActive: boolean
  start(onInput: (data: string) => void, onResize: () => void): void
  stop(): void
  drainInput(maxMs?: number, idleMs?: number): Promise<void>
  write(data: string): void
  readonly columns: number
  readonly rows: number
  moveBy(lines: number): void
  hideCursor(): void
  showCursor(): void
  clearLine(): void
  clearFromCursor(): void
  clearScreen(): void
  setTitle(title: string): void
  setProgress(active: boolean): void
}

export type TuiMode = 'regular' | 'fullscreen'
export interface TuiStopOptions { preserveScreen?: boolean }
export type TuiInputListenerResult =
  | { consume?: boolean; data?: string }
  | undefined
export type TuiInputListener = (data: string) => TuiInputListenerResult
export type OverlayAnchor =
  | 'center'
  | 'top-left'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-right'
  | 'top-center'
  | 'bottom-center'
  | 'left-center'
  | 'right-center'
export interface OverlayMargin {
  top?: number
  right?: number
  bottom?: number
  left?: number
}
export type SizeValue = number | `${number}%`
export interface OverlayOptions {
  width?: SizeValue
  minWidth?: number
  maxHeight?: SizeValue
  anchor?: OverlayAnchor
  offsetX?: number
  offsetY?: number
  row?: SizeValue
  col?: SizeValue
  margin?: OverlayMargin | number
  visible?: (termWidth: number, termHeight: number) => boolean
  nonCapturing?: boolean
}
export interface OverlayUnfocusOptions {
  target: Component | null
}
export interface OverlayHandle {
  hide(): void
  setHidden(hidden: boolean): void
  isHidden(): boolean
  focus(): void
  unfocus(options?: OverlayUnfocusOptions): void
  isFocused(): boolean
}
export interface TUI extends Component, TUIRenderRequester {
  readonly mode: TuiMode
  children: Component[]
  terminal: Terminal
  onDebug?: () => void
  readonly fullRedraws: number
  addChild(component: Component): void
  removeChild(component: Component): void
  clear(): void
  getShowHardwareCursor(): boolean
  setShowHardwareCursor(enabled: boolean): void
  getClearOnShrink(): boolean
  setClearOnShrink(enabled: boolean): void
  setFocus(component: Component | null): void
  showOverlay(component: Component, options?: OverlayOptions): OverlayHandle
  hideOverlay(): void
  hasOverlay(): boolean
  start(): void
  stop(options?: TuiStopOptions): void
  renderNow(force?: boolean): void
  addInputListener(listener: TuiInputListener): () => void
  removeInputListener(listener: TuiInputListener): void
  onTerminalColorSchemeChange(
    listener: (scheme: TerminalColorScheme) => void,
  ): () => void
  setTerminalColorSchemeNotifications(enabled: boolean): void
  queryTerminalBackgroundColor(options: {
    timeoutMs: number
  }): Promise<RgbColor | undefined>
  queryTerminalColorScheme(options: {
    timeoutMs: number
  }): Promise<TerminalColorScheme | undefined>
}

export interface TuiMainScreenRenderState {
  previousLines: string[]
  previousWidth: number
  previousHeight: number
  cursorRow: number
  hardwareCursorRow: number
  maxLinesRendered: number
  previousViewportTop: number
}
export class TuiMainScreen extends Container implements TUI {
  readonly mode: 'regular'
  children: Component[]
  terminal: Terminal
  onDebug?: () => void
  readonly fullRedraws: number
  constructor(
    terminal: Terminal,
    showHardwareCursor?: boolean,
    logDirectory?: string,
  )
  captureRenderState(): TuiMainScreenRenderState
  restoreRenderState(state: TuiMainScreenRenderState): void
  getShowHardwareCursor(): boolean
  setShowHardwareCursor(enabled: boolean): void
  getClearOnShrink(): boolean
  setClearOnShrink(enabled: boolean): void
  setFocus(component: Component | null): void
  showOverlay(component: Component, options?: OverlayOptions): OverlayHandle
  hideOverlay(): void
  hasOverlay(): boolean
  start(): void
  stop(options?: TuiStopOptions): void
  renderNow(force?: boolean): void
  requestRender(force?: boolean): void
  addInputListener(listener: TuiInputListener): () => void
  removeInputListener(listener: TuiInputListener): void
  onTerminalColorSchemeChange(
    listener: (scheme: TerminalColorScheme) => void,
  ): () => void
  setTerminalColorSchemeNotifications(enabled: boolean): void
  queryTerminalBackgroundColor(options: { timeoutMs: number }): Promise<RgbColor | undefined>
  queryTerminalColorScheme(options: { timeoutMs: number }): Promise<TerminalColorScheme | undefined>
}

export interface TuiAltScreenOptions {
  wheelScrollLines?: number
  mouse?: boolean
  searchMatchStyle?: (text: string) => string
  searchCurrentMatchStyle?: (text: string) => string
  openUrl?: (url: string) => void
  onRightClickPaste?: () => void
  copySelection?: (text: string) => Promise<boolean>
}
export interface TuiAltScreen extends TUI, ViewportTUI {}
export class TuiAltScreen extends Container {
  readonly mode: 'fullscreen'
  readonly viewportTop: number
  readonly isFollowingOutput: boolean
  constructor(
    terminal: Terminal,
    showHardwareCursor?: boolean,
    logDirectory?: string,
    options?: TuiAltScreenOptions,
  )
  setLayoutRoot(component: Component | undefined): void
  scrollBy(lines: number): void
  scrollToTop(): void
  scrollToBottom(): void
  flash(message: string, durationMs?: number): void
}

export const CURSOR_MARKER: '\u001B_pi:c\u0007'

export class Spacer implements Component {
  private lines
  constructor(lines?: number)
  setLines(lines: number): void
  invalidate(): void
  render(width: number): string[]
}

export function fuzzyMatch(query: string, text: string): FuzzyMatch
export function fuzzyFilter<T>(
  items: T[],
  query: string,
  getText: (item: T) => string,
): T[]
export function compositeTuiLine(
  baseLine: string,
  overlayLine: string,
  startCol: number,
  overlayWidth: number,
  totalWidth: number,
): string

interface KittyEncodeOptions {
  /** Unsigned 32-bit integer (`0` through `0xffffffff`). */
  columns?: number
  /** Unsigned 32-bit integer (`0` through `0xffffffff`). */
  rows?: number
  /**
   * Unsigned 32-bit integer (`0` through `0xffffffff`). ID `0` is valid and
   * follows the reference truthiness behavior, so `encodeKitty` omits `i=`.
   */
  imageId?: number
  moveCursor?: boolean
}

interface ITerm2EncodeOptions {
  width?: number | string
  height?: number | string
  name?: string
  preserveAspectRatio?: boolean
  inline?: boolean
}

export function renderLatex(
  source: string,
  options?: RenderLatexOptions,
): string | undefined
export function parseOsc11BackgroundColor(data: string): RgbColor | undefined
export function parseTerminalColorSchemeReport(
  data: string,
): TerminalColorScheme | undefined

export function getCellDimensions(): CellDimensions
export function setCellDimensions(dimensions: CellDimensions): void
export function detectCapabilities(
  tmuxForwardsHyperlink?: () => boolean,
): TerminalCapabilities
export function getCapabilities(): TerminalCapabilities
export function resetCapabilitiesCache(): void
export function setCapabilities(capabilities: TerminalCapabilities): void
/** Override selected auto-detected capabilities. */
export function setCapabilityOverrides(
  overrides: Partial<TerminalCapabilities>,
): void

/** Returns the reference's actual random range: 1 through 0xfffffffe. */
export function allocateImageId(): number
export function calculateImageRows(
  imageDimensions: ImageDimensions,
  targetWidthCells: number,
  cellDimensions?: CellDimensions,
): number
/** Chunks at 4096 JavaScript UTF-16 code units, matching `String#slice`. */
export function encodeKitty(
  base64Data: string,
  options?: KittyEncodeOptions,
): string
export function encodeITerm2(
  base64Data: string,
  options?: ITerm2EncodeOptions,
): string
/** Accepts an unsigned 32-bit integer, including `0`. */
export function deleteKittyImage(imageId: number): string
export function deleteAllKittyImages(): string
export function getPngDimensions(base64Data: string): ImageDimensions | null
export function getJpegDimensions(base64Data: string): ImageDimensions | null
export function getGifDimensions(base64Data: string): ImageDimensions | null
export function getWebpDimensions(base64Data: string): ImageDimensions | null
export function getImageDimensions(
  base64Data: string,
  mimeType: string,
): ImageDimensions | null
export function renderImage(
  base64Data: string,
  imageDimensions: ImageDimensions,
  options?: ImageRenderOptions,
): {
  sequence: string
  columns: number
  rows: number
  imageId?: number
} | null
export function hyperlink(text: string, url: string): string
export function imageFallback(
  mimeType: string,
  dimensions?: ImageDimensions,
  filename?: string,
): string

type Letter =
  | 'a'
  | 'b'
  | 'c'
  | 'd'
  | 'e'
  | 'f'
  | 'g'
  | 'h'
  | 'i'
  | 'j'
  | 'k'
  | 'l'
  | 'm'
  | 'n'
  | 'o'
  | 'p'
  | 'q'
  | 'r'
  | 's'
  | 't'
  | 'u'
  | 'v'
  | 'w'
  | 'x'
  | 'y'
  | 'z'
type Digit = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'
type SymbolKey =
  | '`'
  | '-'
  | '='
  | '['
  | ']'
  | '\\'
  | ';'
  | "'"
  | ','
  | '.'
  | '/'
  | '!'
  | '@'
  | '#'
  | '$'
  | '%'
  | '^'
  | '&'
  | '*'
  | '('
  | ')'
  | '_'
  | '+'
  | '|'
  | '~'
  | '{'
  | '}'
  | ':'
  | '<'
  | '>'
  | '?'
type SpecialKey =
  | 'escape'
  | 'esc'
  | 'enter'
  | 'return'
  | 'tab'
  | 'space'
  | 'backspace'
  | 'delete'
  | 'insert'
  | 'clear'
  | 'home'
  | 'end'
  | 'pageUp'
  | 'pageDown'
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'f1'
  | 'f2'
  | 'f3'
  | 'f4'
  | 'f5'
  | 'f6'
  | 'f7'
  | 'f8'
  | 'f9'
  | 'f10'
  | 'f11'
  | 'f12'
type BaseKey = Letter | Digit | SymbolKey | SpecialKey
type ModifierName = 'ctrl' | 'shift' | 'alt' | 'super'
type ModifiedKeyId<
  KeyValue extends string,
  RemainingModifiers extends ModifierName = ModifierName,
> = {
  [Modifier in RemainingModifiers]:
    | `${Modifier}+${KeyValue}`
    | `${Modifier}+${ModifiedKeyId<
        KeyValue,
        Exclude<RemainingModifiers, Modifier>
      >}`
}[RemainingModifiers]

export type KeyId = BaseKey | ModifiedKeyId<BaseKey>
export type KeyEventType = 'press' | 'repeat' | 'release'

export interface Keybindings {
  'tui.editor.cursorUp': true
  'tui.editor.cursorDown': true
  'tui.editor.historyPrevious': true
  'tui.editor.historyNext': true
  'tui.editor.cursorLeft': true
  'tui.editor.cursorRight': true
  'tui.editor.cursorWordLeft': true
  'tui.editor.cursorWordRight': true
  'tui.editor.cursorLineStart': true
  'tui.editor.cursorLineEnd': true
  'tui.editor.jumpForward': true
  'tui.editor.jumpBackward': true
  'tui.editor.pageUp': true
  'tui.editor.pageDown': true
  'tui.editor.deleteCharBackward': true
  'tui.editor.deleteCharForward': true
  'tui.editor.deleteWordBackward': true
  'tui.editor.deleteWordForward': true
  'tui.editor.deleteToLineStart': true
  'tui.editor.deleteToLineEnd': true
  'tui.editor.yank': true
  'tui.editor.yankPop': true
  'tui.editor.undo': true
  'tui.input.newLine': true
  'tui.input.submit': true
  'tui.input.tab': true
  'tui.input.copy': true
  'tui.select.up': true
  'tui.select.down': true
  'tui.select.pageUp': true
  'tui.select.pageDown': true
  'tui.select.confirm': true
  'tui.select.cancel': true
  'tui.altScreen.pageUp': true
  'tui.altScreen.pageDown': true
  'tui.altScreen.halfPageUp': true
  'tui.altScreen.halfPageDown': true
  'tui.altScreen.previousPrompt': true
  'tui.altScreen.nextPrompt': true
  'tui.altScreen.top': true
  'tui.altScreen.bottom': true
}

export type Keybinding = keyof Keybindings
export interface KeybindingDefinition {
  defaultKeys: KeyId | KeyId[]
  description?: string
}
export type KeybindingDefinitions = Record<string, KeybindingDefinition>
export type KeybindingsConfig = Record<
  string,
  KeyId | KeyId[] | undefined
>
export const TUI_KEYBINDINGS: {
  readonly 'tui.editor.cursorUp': {
    readonly defaultKeys: 'up'
    readonly description: 'Move cursor up'
  }
  readonly 'tui.editor.cursorDown': {
    readonly defaultKeys: 'down'
    readonly description: 'Move cursor down'
  }
  readonly 'tui.editor.historyPrevious': {
    readonly defaultKeys: []
    readonly description: 'Select previous prompt history entry'
  }
  readonly 'tui.editor.historyNext': {
    readonly defaultKeys: []
    readonly description: 'Select next prompt history entry'
  }
  readonly 'tui.editor.cursorLeft': {
    readonly defaultKeys: ['left', 'ctrl+b']
    readonly description: 'Move cursor left'
  }
  readonly 'tui.editor.cursorRight': {
    readonly defaultKeys: ['right', 'ctrl+f']
    readonly description: 'Move cursor right'
  }
  readonly 'tui.editor.cursorWordLeft': {
    readonly defaultKeys: ['alt+left', 'ctrl+left', 'alt+b']
    readonly description: 'Move cursor word left'
  }
  readonly 'tui.editor.cursorWordRight': {
    readonly defaultKeys: ['alt+right', 'ctrl+right', 'alt+f']
    readonly description: 'Move cursor word right'
  }
  readonly 'tui.editor.cursorLineStart': {
    readonly defaultKeys: ['home', 'ctrl+home', 'ctrl+a']
    readonly description: 'Move to line start'
  }
  readonly 'tui.editor.cursorLineEnd': {
    readonly defaultKeys: ['end', 'ctrl+end', 'ctrl+e']
    readonly description: 'Move to line end'
  }
  readonly 'tui.editor.jumpForward': {
    readonly defaultKeys: 'ctrl+]'
    readonly description: 'Jump forward to character'
  }
  readonly 'tui.editor.jumpBackward': {
    readonly defaultKeys: 'ctrl+alt+]'
    readonly description: 'Jump backward to character'
  }
  readonly 'tui.editor.pageUp': {
    readonly defaultKeys: ['pageUp', 'ctrl+pageUp']
    readonly description: 'Page up'
  }
  readonly 'tui.editor.pageDown': {
    readonly defaultKeys: ['pageDown', 'ctrl+pageDown']
    readonly description: 'Page down'
  }
  readonly 'tui.editor.deleteCharBackward': {
    readonly defaultKeys: 'backspace'
    readonly description: 'Delete character backward'
  }
  readonly 'tui.editor.deleteCharForward': {
    readonly defaultKeys: ['delete', 'ctrl+d']
    readonly description: 'Delete character forward'
  }
  readonly 'tui.editor.deleteWordBackward': {
    readonly defaultKeys: ['ctrl+w', 'alt+backspace']
    readonly description: 'Delete word backward'
  }
  readonly 'tui.editor.deleteWordForward': {
    readonly defaultKeys: ['alt+d', 'alt+delete']
    readonly description: 'Delete word forward'
  }
  readonly 'tui.editor.deleteToLineStart': {
    readonly defaultKeys: 'ctrl+u'
    readonly description: 'Delete to line start'
  }
  readonly 'tui.editor.deleteToLineEnd': {
    readonly defaultKeys: 'ctrl+k'
    readonly description: 'Delete to line end'
  }
  readonly 'tui.editor.yank': {
    readonly defaultKeys: 'ctrl+y'
    readonly description: 'Yank'
  }
  readonly 'tui.editor.yankPop': {
    readonly defaultKeys: 'alt+y'
    readonly description: 'Yank pop'
  }
  readonly 'tui.editor.undo': {
    readonly defaultKeys: 'ctrl+-'
    readonly description: 'Undo'
  }
  readonly 'tui.input.newLine': {
    readonly defaultKeys: ['shift+enter', 'ctrl+j']
    readonly description: 'Insert newline'
  }
  readonly 'tui.input.submit': {
    readonly defaultKeys: 'enter'
    readonly description: 'Submit input'
  }
  readonly 'tui.input.tab': {
    readonly defaultKeys: 'tab'
    readonly description: 'Tab / autocomplete'
  }
  readonly 'tui.input.copy': {
    readonly defaultKeys: 'ctrl+c'
    readonly description: 'Copy selection'
  }
  readonly 'tui.select.up': {
    readonly defaultKeys: 'up'
    readonly description: 'Move selection up'
  }
  readonly 'tui.select.down': {
    readonly defaultKeys: 'down'
    readonly description: 'Move selection down'
  }
  readonly 'tui.select.pageUp': {
    readonly defaultKeys: 'pageUp'
    readonly description: 'Selection page up'
  }
  readonly 'tui.select.pageDown': {
    readonly defaultKeys: 'pageDown'
    readonly description: 'Selection page down'
  }
  readonly 'tui.select.confirm': {
    readonly defaultKeys: 'enter'
    readonly description: 'Confirm selection'
  }
  readonly 'tui.select.cancel': {
    readonly defaultKeys: ['escape', 'ctrl+c']
    readonly description: 'Cancel selection'
  }
  readonly 'tui.altScreen.pageUp': {
    readonly defaultKeys: 'pageUp'
    readonly description: 'Scroll viewport up one page'
  }
  readonly 'tui.altScreen.pageDown': {
    readonly defaultKeys: 'pageDown'
    readonly description: 'Scroll viewport down one page'
  }
  readonly 'tui.altScreen.halfPageUp': {
    readonly defaultKeys: []
    readonly description: 'Scroll viewport up half a page'
  }
  readonly 'tui.altScreen.halfPageDown': {
    readonly defaultKeys: []
    readonly description: 'Scroll viewport down half a page'
  }
  readonly 'tui.altScreen.previousPrompt': {
    readonly defaultKeys: 'ctrl+shift+up'
    readonly description: 'Jump to previous semantic prompt'
  }
  readonly 'tui.altScreen.nextPrompt': {
    readonly defaultKeys: 'ctrl+shift+down'
    readonly description: 'Jump to next semantic prompt'
  }
  readonly 'tui.altScreen.top': {
    readonly defaultKeys: 'home'
    readonly description: 'Scroll viewport to top'
  }
  readonly 'tui.altScreen.bottom': {
    readonly defaultKeys: 'end'
    readonly description: 'Scroll viewport to bottom'
  }
}

export interface KeybindingConflict {
  key: KeyId
  keybindings: string[]
}

export class KeybindingsManager {
  constructor(
    definitions: KeybindingDefinitions,
    userBindings?: KeybindingsConfig,
  )
  matches(data: string, keybinding: Keybinding): boolean
  getKeys(keybinding: Keybinding): KeyId[]
  getDefinition(keybinding: Keybinding): KeybindingDefinition
  getConflicts(): KeybindingConflict[]
  setUserBindings(userBindings: KeybindingsConfig): void
  getUserBindings(): KeybindingsConfig
  getResolvedBindings(): KeybindingsConfig
}

export function setKeybindings(keybindings: KeybindingsManager): void
export function getKeybindings(): KeybindingsManager

export const Key: {
  readonly escape: 'escape'
  readonly esc: 'esc'
  readonly enter: 'enter'
  readonly return: 'return'
  readonly tab: 'tab'
  readonly space: 'space'
  readonly backspace: 'backspace'
  readonly delete: 'delete'
  readonly insert: 'insert'
  readonly clear: 'clear'
  readonly home: 'home'
  readonly end: 'end'
  readonly pageUp: 'pageUp'
  readonly pageDown: 'pageDown'
  readonly up: 'up'
  readonly down: 'down'
  readonly left: 'left'
  readonly right: 'right'
  readonly f1: 'f1'
  readonly f2: 'f2'
  readonly f3: 'f3'
  readonly f4: 'f4'
  readonly f5: 'f5'
  readonly f6: 'f6'
  readonly f7: 'f7'
  readonly f8: 'f8'
  readonly f9: 'f9'
  readonly f10: 'f10'
  readonly f11: 'f11'
  readonly f12: 'f12'
  readonly backtick: '`'
  readonly hyphen: '-'
  readonly equals: '='
  readonly leftbracket: '['
  readonly rightbracket: ']'
  readonly backslash: '\\'
  readonly semicolon: ';'
  readonly quote: "'"
  readonly comma: ','
  readonly period: '.'
  readonly slash: '/'
  readonly exclamation: '!'
  readonly at: '@'
  readonly hash: '#'
  readonly dollar: '$'
  readonly percent: '%'
  readonly caret: '^'
  readonly ampersand: '&'
  readonly asterisk: '*'
  readonly leftparen: '('
  readonly rightparen: ')'
  readonly underscore: '_'
  readonly plus: '+'
  readonly pipe: '|'
  readonly tilde: '~'
  readonly leftbrace: '{'
  readonly rightbrace: '}'
  readonly colon: ':'
  readonly lessthan: '<'
  readonly greaterthan: '>'
  readonly question: '?'
  readonly ctrl: <KeyValue extends BaseKey>(key: KeyValue) => `ctrl+${KeyValue}`
  readonly shift: <KeyValue extends BaseKey>(key: KeyValue) => `shift+${KeyValue}`
  readonly alt: <KeyValue extends BaseKey>(key: KeyValue) => `alt+${KeyValue}`
  readonly super: <KeyValue extends BaseKey>(key: KeyValue) => `super+${KeyValue}`
  readonly ctrlShift: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `ctrl+shift+${KeyValue}`
  readonly shiftCtrl: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `shift+ctrl+${KeyValue}`
  readonly ctrlAlt: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `ctrl+alt+${KeyValue}`
  readonly altCtrl: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `alt+ctrl+${KeyValue}`
  readonly shiftAlt: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `shift+alt+${KeyValue}`
  readonly altShift: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `alt+shift+${KeyValue}`
  readonly ctrlSuper: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `ctrl+super+${KeyValue}`
  readonly superCtrl: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `super+ctrl+${KeyValue}`
  readonly shiftSuper: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `shift+super+${KeyValue}`
  readonly superShift: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `super+shift+${KeyValue}`
  readonly altSuper: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `alt+super+${KeyValue}`
  readonly superAlt: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `super+alt+${KeyValue}`
  readonly ctrlShiftAlt: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `ctrl+shift+alt+${KeyValue}`
  readonly ctrlShiftSuper: <KeyValue extends BaseKey>(
    key: KeyValue,
  ) => `ctrl+shift+super+${KeyValue}`
}

export function setKittyProtocolActive(active: boolean): void
export function isKittyProtocolActive(): boolean
export function isKeyRelease(data: string): boolean
export function isKeyRepeat(data: string): boolean
export function matchesKey(data: string, keyId: KeyId): boolean
export function parseKey(data: string): string | undefined
export function decodeKittyPrintable(data: string): string | undefined

/**
 * Utility inputs must be well-formed UTF-16. Ill-formed strings throw
 * `RangeError` instead of entering a lossy Rust `String` conversion. Column
 * and width arguments are unsigned 32-bit integers (`0` through
 * `0xffffffff`).
 */
export function visibleWidth(value: string): number
export function stripTerminalSequences(value: string): string
export function getOsc8LinkAtColumn(
  line: string,
  column: number,
): string | undefined
export function sliceByColumn(
  line: string,
  startCol: number,
  length: number,
  strict?: boolean,
): string
export function truncateToWidth(
  text: string,
  maxWidth: number,
  ellipsis?: string,
  pad?: boolean,
): string
export function wrapTextWithAnsi(text: string, width: number): string[]
