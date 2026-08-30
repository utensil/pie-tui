'use strict'

const { execSync } = require('node:child_process')
const { Buffer, constants: { MAX_STRING_LENGTH } } = require('node:buffer')
const { EventEmitter } = require('node:events')
const { readdirSync, statSync } = require('node:fs')
const { homedir } = require('node:os')
const { dirname, join, isAbsolute } = require('node:path')
const { pathToFileURL } = require('node:url')
const { Marked } = require('marked')
const native = require('./native-loader.cjs')

const MAX_PAD_WIDTH = MAX_STRING_LENGTH
const SEGMENT_RESET = '\x1b[0m\x1b]8;;\x07'

const fromNullable = (value) => (value === null ? undefined : value)

function assertWellFormed(value, parameter) {
  if (typeof value !== 'string') {
    throw new TypeError(`${parameter} must be a string`)
  }
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index)
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1)
      if (!(next >= 0xdc00 && next <= 0xdfff)) {
        throw new RangeError(`${parameter} contains an unpaired UTF-16 surrogate`)
      }
      index += 1
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw new RangeError(`${parameter} contains an unpaired UTF-16 surrogate`)
    }
  }
  return value
}

function assertColumn(value, parameter) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new RangeError(`${parameter} must be an unsigned 32-bit integer`)
  }
  return value
}

function assertImageId(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new RangeError('imageId must be an unsigned 32-bit integer')
  }
  return value
}

function encodeRawUtf16Strings(...values) {
  for (const value of values) {
    if (typeof value !== 'string') {
      throw new TypeError('composite lines must be strings')
    }
  }
  // C0 controls form standalone zero-width graphemes, so they preserve the
  // reference's raw UTF-16 substring behavior across napi's UTF-8 String seam.
  let prefix = '\u0001\u0002\u0003\u0004'
  while (values.some((value) => value.includes(prefix))) {
    prefix = `\u0001${prefix}`
  }

  const replacements = []
  const encoded = values.map((value) => {
    let result = ''
    for (let index = 0; index < value.length; index += 1) {
      const unit = value.charCodeAt(index)
      if (
        unit >= 0xd800 &&
        unit <= 0xdbff &&
        value.charCodeAt(index + 1) >= 0xdc00 &&
        value.charCodeAt(index + 1) <= 0xdfff
      ) {
        result += value[index] + value[index + 1]
        index += 1
      } else if (unit >= 0xd800 && unit <= 0xdfff) {
        const bits = replacements.length
          .toString(2)
          .padStart(32, '0')
          .replaceAll('0', '\u0002')
          .replaceAll('1', '\u0003')
        const token = `${prefix}${bits}\u0004`
        replacements.push([token, unit])
        result += token
      } else {
        result += value[index]
      }
    }
    return result
  })
  const decode = (value) => {
    let result = value
    for (const [token, unit] of replacements) {
      result = result.split(token).join(String.fromCharCode(unit))
    }
    return result
  }
  return { encoded, decode, tokenPrefix: prefix }
}

function visibleWidthRawUtf16(value) {
  // The reference strips ANSI before segmenting final output, which can join
  // surrogate units that originated on opposite sides of a segment reset.
  const firstPass = encodeRawUtf16Strings(value)
  const strippedRaw = firstPass.decode(
    native.nativeStripTerminalSequences(firstPass.encoded[0]),
  )
  const secondPass = encodeRawUtf16Strings(strippedRaw)
  return native.nativeCompositeVisibleWidth(
    secondPass.encoded[0],
    secondPass.tokenPrefix,
  )
}

function probeTmuxHyperlinks() {
  try {
    const termfeatures = execSync(
      "tmux display-message -p '#{client_termfeatures}'",
      {
        encoding: 'utf8',
        timeout: 250,
        stdio: ['ignore', 'pipe', 'ignore'],
      },
    )
    return termfeatures
      .split(',')
      .map((feature) => feature.trim())
      .includes('hyperlinks')
  } catch {
    return false
  }
}

let cellDimensions = { widthPx: 9, heightPx: 18 }
let cachedCapabilities = null

const CURSOR_MARKER = native.nativeCursorMarker()
const TUI_KEYBINDINGS = {}
for (const definition of native.nativeGetTuiKeybindingDefinitions()) {
  TUI_KEYBINDINGS[definition.id] = {
    defaultKeys: definition.defaultKeys,
    description: definition.description,
  }
}

function normalizeBindingKeys(keys) {
  if (keys === undefined) return []
  const seen = new Set()
  const result = []
  for (const key of Array.isArray(keys) ? keys : [keys]) {
    if (!seen.has(key)) {
      seen.add(key)
      result.push(key)
    }
  }
  return result
}

class KeybindingsManager {
  constructor(definitions, userBindings = {}) {
    this.definitions = definitions
    this.userBindings = userBindings
    this.keysById = new Map()
    this.conflicts = []
    this.rebuild()
  }

  rebuild() {
    this.keysById.clear()
    this.conflicts = []
    const userClaims = new Map()
    for (const [keybinding, keys] of Object.entries(this.userBindings)) {
      if (!(keybinding in this.definitions)) continue
      for (const key of normalizeBindingKeys(keys)) {
        const claimants = userClaims.get(key) ?? new Set()
        claimants.add(keybinding)
        userClaims.set(key, claimants)
      }
    }
    for (const [key, keybindings] of userClaims) {
      if (keybindings.size > 1) {
        this.conflicts.push({ key, keybindings: [...keybindings] })
      }
    }
    for (const [id, definition] of Object.entries(this.definitions)) {
      const userKeys = this.userBindings[id]
      this.keysById.set(
        id,
        userKeys === undefined
          ? normalizeBindingKeys(definition.defaultKeys)
          : normalizeBindingKeys(userKeys),
      )
    }
  }

  matches(data, keybinding) {
    return (this.keysById.get(keybinding) ?? []).some((key) =>
      matchesKey(data, key),
    )
  }

  getKeys(keybinding) {
    return [...(this.keysById.get(keybinding) ?? [])]
  }

  getDefinition(keybinding) {
    return this.definitions[keybinding]
  }

  getConflicts() {
    return this.conflicts.map((conflict) => ({
      ...conflict,
      keybindings: [...conflict.keybindings],
    }))
  }

  setUserBindings(userBindings) {
    this.userBindings = userBindings
    this.rebuild()
  }

  getUserBindings() {
    return { ...this.userBindings }
  }

  getResolvedBindings() {
    const resolved = {}
    for (const id of Object.keys(this.definitions)) {
      const keys = this.keysById.get(id) ?? []
      resolved[id] = keys.length === 1 ? keys[0] : [...keys]
    }
    return resolved
  }
}

let globalKeybindings = null
function setKeybindings(keybindings) {
  globalKeybindings = keybindings
}

function getKeybindings() {
  if (!globalKeybindings) {
    globalKeybindings = new KeybindingsManager(TUI_KEYBINDINGS)
  }
  return globalKeybindings
}

const spacerRegistry = new WeakMap()
class Spacer {
  constructor(lines = 1) {
    this.lines = lines
    spacerRegistry.set(this, new native.NativeSpacerState())
  }

  setLines(lines) {
    this.lines = lines
  }

  invalidate() {}

  render(_width) {
    const state = spacerRegistry.get(this)
    if (!state) {
      const result = []
      for (let index = 0; index < this.lines; index += 1) result.push('')
      return result
    }
    state.setLines(Number(this.lines))
    return state.render()
  }
}

const textRegistry = new WeakMap()
class Text {
  constructor(text = '', paddingX = 1, paddingY = 1, customBgFn) {
    this.customBgFn = customBgFn
    textRegistry.set(
      this,
      new native.NativeTextState(
        assertWellFormed(text, 'text'),
        assertColumn(paddingX, 'paddingX'),
        assertColumn(paddingY, 'paddingY'),
      ),
    )
  }

  setText(text) {
    textRegistry.get(this).setText(assertWellFormed(text, 'text'))
  }

  setCustomBgFn(customBgFn) {
    this.customBgFn = customBgFn
    this.invalidate()
  }

  invalidate() {
    textRegistry.get(this).invalidate()
  }

  render(width) {
    const lines = textRegistry.get(this).render(assertColumn(width, 'width'))
    if (!this.customBgFn) return lines
    return lines.map((line) => this.customBgFn(line))
  }
}

const truncatedTextRegistry = new WeakMap()
class TruncatedText {
  constructor(text, paddingX = 0, paddingY = 0) {
    truncatedTextRegistry.set(
      this,
      new native.NativeTruncatedTextState(
        assertWellFormed(text, 'text'),
        assertColumn(paddingX, 'paddingX'),
        assertColumn(paddingY, 'paddingY'),
      ),
    )
  }

  invalidate() {
    truncatedTextRegistry.get(this).invalidate()
  }

  render(width) {
    return truncatedTextRegistry
      .get(this)
      .render(assertColumn(width, 'width'))
  }
}

const inputRegistry = new WeakMap()
class Input {
  constructor() {
    this.onSubmit = undefined
    this.onEscape = undefined
    inputRegistry.set(this, new native.NativeInputState())
  }

  get focused() {
    return inputRegistry.get(this).focused
  }

  set focused(value) {
    inputRegistry.get(this).focused = Boolean(value)
  }

  getValue() {
    return inputRegistry.get(this).getValue()
  }

  setValue(value) {
    inputRegistry.get(this).setValue(assertWellFormed(value, 'value'))
  }

  handleInput(data) {
    const events = inputRegistry
      .get(this)
      .handleInput(assertWellFormed(data, 'data'))
    if (events.submit !== undefined && events.submit !== null) {
      this.onSubmit?.(events.submit)
    }
    if (events.escape) this.onEscape?.()
  }

  invalidate() {
    inputRegistry.get(this).invalidate()
  }

  render(width) {
    return inputRegistry.get(this).render(assertColumn(width, 'width'))
  }
}

const editorRegistry = new WeakMap()
class Editor {
  constructor(tui, theme, options = {}) {
    this.tui = tui
    this.theme = theme
    this.borderColor = theme.borderColor
    this.onSubmit = undefined
    this.onChange = undefined
    this.autocompleteProvider = undefined
    editorRegistry.set(
      this,
      new native.NativeEditorState(
        assertColumn(options.paddingX ?? 0, 'options.paddingX'),
        options.autocompleteMaxVisible === undefined
          ? undefined
          : assertColumn(
              options.autocompleteMaxVisible,
              'options.autocompleteMaxVisible',
            ),
      ),
    )
  }

  emitEvents(events) {
    if (events.change !== undefined && events.change !== null) {
      this.onChange?.(events.change)
    }
    if (events.submit !== undefined && events.submit !== null) {
      this.onSubmit?.(events.submit)
    }
  }

  get focused() {
    return editorRegistry.get(this).focused
  }

  set focused(value) {
    editorRegistry.get(this).focused = Boolean(value)
  }

  get disableSubmit() {
    return editorRegistry.get(this).disableSubmit
  }

  set disableSubmit(value) {
    editorRegistry.get(this).disableSubmit = Boolean(value)
  }

  getPaddingX() { return editorRegistry.get(this).getPaddingX() }
  setPaddingX(value) {
    editorRegistry.get(this).setPaddingX(assertColumn(value, 'padding'))
    this.tui.requestRender()
  }
  getAutocompleteMaxVisible() {
    return editorRegistry.get(this).getAutocompleteMaxVisible()
  }
  setAutocompleteMaxVisible(value) {
    editorRegistry
      .get(this)
      .setAutocompleteMaxVisible(assertColumn(value, 'maxVisible'))
    this.tui.requestRender()
  }
  setAutocompleteProvider(provider) { this.autocompleteProvider = provider }
  addToHistory(text) {
    editorRegistry.get(this).addToHistory(assertWellFormed(text, 'text'))
  }
  invalidate() { editorRegistry.get(this).invalidate() }
  render(width) {
    return editorRegistry.get(this).render(assertColumn(width, 'width'))
  }
  handleInput(data) {
    this.emitEvents(
      editorRegistry.get(this).handleInput(assertWellFormed(data, 'data')),
    )
    this.tui.requestRender()
  }
  getText() { return editorRegistry.get(this).getText() }
  getExpandedText() { return editorRegistry.get(this).getExpandedText() }
  getLines() { return editorRegistry.get(this).getLines() }
  getCursor() { return editorRegistry.get(this).getCursor() }
  setText(text) {
    this.emitEvents(
      editorRegistry.get(this).setText(assertWellFormed(text, 'text')),
    )
    this.tui.requestRender()
  }
  insertTextAtCursor(text) {
    this.emitEvents(
      editorRegistry
        .get(this)
        .insertTextAtCursor(assertWellFormed(text, 'text')),
    )
    this.tui.requestRender()
  }
  isShowingAutocomplete() {
    return editorRegistry.get(this).isShowingAutocomplete()
  }
}

const markdownRegistry = new WeakMap()
class Markdown {
  constructor(text, paddingX, paddingY, theme, defaultTextStyle, options) {
    this.text = text
    this.paddingX = paddingX
    this.paddingY = paddingY
    this.theme = theme
    this.defaultTextStyle = defaultTextStyle
    this.options = options ? { ...options } : {}
    markdownRegistry.set(
      this,
      new native.NativeMarkdownState(
        assertWellFormed(text, 'text'),
        assertColumn(paddingX, 'paddingX'),
        assertColumn(paddingY, 'paddingY'),
        this.options.preserveOrderedListMarkers,
        this.options.preserveBackslashEscapes,
        this.options.renderLatex,
      ),
    )
  }

  setText(text) {
    this.text = assertWellFormed(text, 'text')
    markdownRegistry.get(this).setText(this.text)
  }

  invalidate() { markdownRegistry.get(this).invalidate() }

  render(width) {
    const contentWidth = Math.max(1, width - this.paddingX * 2)
    const transformed = this.options.transform?.(this.text, contentWidth)
    const state = markdownRegistry.get(this)
    if (transformed !== undefined) state.setText(assertWellFormed(transformed, 'text'))
    const lines = state.render(assertColumn(width, 'width'))
    if (transformed !== undefined) state.setText(this.text)
    const bg = this.defaultTextStyle?.bgColor
    return bg ? lines.map((line) => bg(line)) : lines
  }
}

class Image {
  constructor(base64Data, mimeType, theme, options = {}, dimensions) {
    this.base64Data = base64Data
    this.mimeType = mimeType
    this.theme = theme
    this.options = options
    this.dimensions =
      dimensions ??
      getImageDimensions(base64Data, mimeType) ??
      { widthPx: 800, heightPx: 600 }
    this.imageId = options.imageId
    this.cachedLines = undefined
    this.cachedWidth = undefined
  }

  getImageId() { return this.imageId }
  invalidate() {
    this.cachedLines = undefined
    this.cachedWidth = undefined
  }

  render(width) {
    if (this.cachedLines && this.cachedWidth === width) return this.cachedLines
    const maxWidth = Math.max(
      1,
      Math.min(width - 2, this.options.maxWidthCells ?? 60),
    )
    const dimensions = getCellDimensions()
    const defaultMaxHeight = Math.max(
      1,
      Math.ceil((maxWidth * dimensions.widthPx) / dimensions.heightPx),
    )
    const maxHeight = this.options.maxHeightCells ?? defaultMaxHeight
    const capabilities = getCapabilities()
    let lines
    if (capabilities.images) {
      if (capabilities.images === 'kitty' && this.imageId === undefined) {
        this.imageId = allocateImageId()
      }
      const result = renderImage(this.base64Data, this.dimensions, {
        maxWidthCells: maxWidth,
        maxHeightCells: maxHeight,
        imageId: this.imageId,
        moveCursor: false,
      })
      if (result) {
        if (result.imageId) this.imageId = result.imageId
        if (capabilities.images === 'kitty') {
          lines = [result.sequence]
          for (let index = 0; index < result.rows - 1; index += 1) lines.push('')
        } else {
          lines = []
          for (let index = 0; index < result.rows - 1; index += 1) lines.push('')
          const offset = result.rows - 1
          lines.push(`${offset > 0 ? `\x1b[${offset}A` : ''}${result.sequence}`)
        }
      }
    }
    if (!lines) {
      const fallback = imageFallback(
        this.mimeType,
        this.dimensions,
        this.options.filename,
      )
      lines = [truncateToWidth(this.theme.fallbackColor(fallback), width)]
    }
    this.cachedLines = lines
    this.cachedWidth = width
    return lines
  }
}

const VIEWPORT_TUI = Symbol.for('@earendil-works/pi-tui/viewport')
function isFocusable(component) {
  return component !== null && 'focused' in component
}

function isViewportTUI(tui) {
  return tui[VIEWPORT_TUI] === true
}

class Container {
  constructor() {
    this.children = []
  }

  addChild(component) {
    this.children.push(component)
  }

  removeChild(component) {
    const index = this.children.indexOf(component)
    if (index !== -1) this.children.splice(index, 1)
  }

  clear() {
    this.children = []
  }

  invalidate() {
    for (const child of this.children) child.invalidate?.()
  }

  render(width) {
    const lines = []
    for (const child of this.children) lines.push(...child.render(width))
    return lines
  }
}

class SelectList {
  constructor(items, maxVisible, theme, layout = {}) {
    this.items = items
    this.filteredItems = items
    this.selectedIndex = 0
    this.maxVisible = maxVisible
    this.theme = theme
    this.layout = layout
    this.onSelect = undefined
    this.onCancel = undefined
    this.onSelectionChange = undefined
  }

  setFilter(filter) {
    this.filteredItems = this.items.filter((item) =>
      item.value.toLowerCase().startsWith(filter.toLowerCase()),
    )
    this.selectedIndex = 0
  }
  setSelectedIndex(index) {
    this.selectedIndex = Math.max(
      0,
      Math.min(index, this.filteredItems.length - 1),
    )
  }
  invalidate() {}
  render(width) {
    if (this.filteredItems.length === 0) {
      return [this.theme.noMatch('  No matching commands')]
    }
    const primaryWidth = this.getPrimaryColumnWidth()
    const start = Math.max(
      0,
      Math.min(
        this.selectedIndex - Math.floor(this.maxVisible / 2),
        this.filteredItems.length - this.maxVisible,
      ),
    )
    const end = Math.min(start + this.maxVisible, this.filteredItems.length)
    const lines = []
    for (let index = start; index < end; index += 1) {
      const item = this.filteredItems[index]
      if (!item) continue
      const description = item.description
        ? item.description.replace(/[\r\n]+/g, ' ').trim()
        : undefined
      lines.push(
        this.renderItem(
          item,
          index === this.selectedIndex,
          width,
          description,
          primaryWidth,
        ),
      )
    }
    if (start > 0 || end < this.filteredItems.length) {
      lines.push(
        this.theme.scrollInfo(
          truncateToWidth(
            `  (${this.selectedIndex + 1}/${this.filteredItems.length})`,
            width - 2,
            '',
          ),
        ),
      )
    }
    return lines
  }
  handleInput(data) {
    const bindings = getKeybindings()
    if (bindings.matches(data, 'tui.select.up')) {
      this.selectedIndex =
        this.selectedIndex === 0
          ? this.filteredItems.length - 1
          : this.selectedIndex - 1
      this.notifySelectionChange()
    } else if (bindings.matches(data, 'tui.select.down')) {
      this.selectedIndex =
        this.selectedIndex === this.filteredItems.length - 1
          ? 0
          : this.selectedIndex + 1
      this.notifySelectionChange()
    } else if (bindings.matches(data, 'tui.select.confirm')) {
      const item = this.filteredItems[this.selectedIndex]
      if (item) this.onSelect?.(item)
    } else if (bindings.matches(data, 'tui.select.cancel')) {
      this.onCancel?.()
    }
  }
  renderItem(item, selected, width, description, primaryWidth) {
    const prefix = selected ? '→ ' : '  '
    const prefixWidth = visibleWidth(prefix)
    if (description && width > 40) {
      const effective = Math.max(
        1,
        Math.min(primaryWidth, width - prefixWidth - 4),
      )
      const maximum = Math.max(1, effective - 2)
      const value = this.truncatePrimary(item, selected, maximum, effective)
      const spacing = ' '.repeat(
        Math.max(1, effective - visibleWidth(value)),
      )
      const remaining = width - prefixWidth - visibleWidth(value) - spacing.length - 2
      if (remaining > 10) {
        const desc = truncateToWidth(description, remaining, '')
        return selected
          ? this.theme.selectedText(`${prefix}${value}${spacing}${desc}`)
          : prefix + value + this.theme.description(spacing + desc)
      }
    }
    const maximum = width - prefixWidth - 2
    const value = this.truncatePrimary(item, selected, maximum, maximum)
    return selected ? this.theme.selectedText(`${prefix}${value}`) : prefix + value
  }
  getPrimaryColumnWidth() {
    const rawMin =
      this.layout.minPrimaryColumnWidth ??
      this.layout.maxPrimaryColumnWidth ??
      32
    const rawMax =
      this.layout.maxPrimaryColumnWidth ??
      this.layout.minPrimaryColumnWidth ??
      32
    const minimum = Math.max(1, Math.min(rawMin, rawMax))
    const maximum = Math.max(1, Math.max(rawMin, rawMax))
    const widest = this.filteredItems.reduce(
      (value, item) =>
        Math.max(value, visibleWidth(item.label || item.value) + 2),
      0,
    )
    return Math.max(minimum, Math.min(widest, maximum))
  }
  truncatePrimary(item, selected, maxWidth, columnWidth) {
    const text = item.label || item.value
    const value = this.layout.truncatePrimary
      ? this.layout.truncatePrimary({
          text,
          maxWidth,
          columnWidth,
          item,
          isSelected: selected,
        })
      : truncateToWidth(text, maxWidth, '')
    return truncateToWidth(value, maxWidth, '')
  }
  notifySelectionChange() {
    const item = this.filteredItems[this.selectedIndex]
    if (item) this.onSelectionChange?.(item)
  }
  getSelectedItem() { return this.filteredItems[this.selectedIndex] || null }
}

class ScrollView extends Container {
  constructor(component, options = {}) {
    super()
    if (options.axis !== undefined && options.axis !== 'vertical') {
      throw new Error(`Unsupported ScrollView axis: ${options.axis}`)
    }
    this.child = component
    this.children.push(component)
    this.followEnd = (options.follow ?? 'none') === 'end'
    this.followingEnd = this.followEnd
    this.followSuppressedAtEnd = false
    this.primary = options.primary ?? false
    this.overscroll = options.overscroll ?? 'chain'
    this.currentScrollbar = options.scrollbar ?? 'hidden'
    this.scrollbarStyle =
      options.scrollbarStyle ?? ((text) => `\x1b[100m${text}\x1b[49m`)
    this.scrollbarHideDelayMs = Math.max(
      0,
      Math.floor(options.scrollbarHideDelayMs ?? 1000),
    )
    this.currentScrollTop = 0
    this.contentHeight = 0
    this.currentViewportHeight = 0
    this.requestRenderCallback = undefined
    this.transientScrollbarVisible = false
    this.scrollbarActive = false
    this.scrollbarHideTimer = undefined
  }
  get scrollTop() { return this.currentScrollTop }
  get isFollowingEnd() { return this.followingEnd }
  get viewportHeight() { return this.currentViewportHeight }
  get scrollbar() { return this.currentScrollbar }
  get isScrollbarVisible() {
    if (this.scrollbar === 'always') return this.currentViewportHeight > 0
    return (
      this.scrollbar === 'auto' &&
      this.contentHeight > this.currentViewportHeight &&
      this.transientScrollbarVisible
    )
  }
  setScrollbar(scrollbar) {
    if (scrollbar === this.currentScrollbar) return
    this.currentScrollbar = scrollbar
    if (scrollbar !== 'auto') this.hideTransientScrollbar()
    else if (this.scrollbarActive) this.markScrollbarActivity()
    this.requestRenderCallback?.()
  }
  getContentWidth(width) {
    return this.scrollbar === 'always' && width > 1 ? width - 1 : width
  }
  markScrollbarActivity() {
    if (this.scrollbar !== 'auto' || this.contentHeight <= this.currentViewportHeight) return
    this.transientScrollbarVisible = true
    if (this.scrollbarHideTimer) clearTimeout(this.scrollbarHideTimer)
    this.scrollbarHideTimer = undefined
    if (this.scrollbarActive) return
    this.scrollbarHideTimer = setTimeout(() => {
      this.scrollbarHideTimer = undefined
      this.transientScrollbarVisible = false
      this.requestRenderCallback?.()
    }, this.scrollbarHideDelayMs)
    this.scrollbarHideTimer.unref?.()
  }
  hideTransientScrollbar() {
    this.transientScrollbarVisible = false
    if (this.scrollbarHideTimer) clearTimeout(this.scrollbarHideTimer)
    this.scrollbarHideTimer = undefined
  }
  setScrollbarActive(active) {
    if (active === this.scrollbarActive) return
    this.scrollbarActive = active
    this.markScrollbarActivity()
  }
  scrollTo(scrollTop, options = {}) {
    const requested = Number.isFinite(scrollTop)
      ? Math.trunc(scrollTop)
      : this.currentScrollTop
    const maximum = Math.max(0, this.contentHeight - this.currentViewportHeight)
    const next = Math.max(0, Math.min(maximum, requested))
    const nextFollowSuppressedAtEnd = options.disableFollow === true && next === maximum
    const nextFollowingEnd =
      !nextFollowSuppressedAtEnd && this.followEnd && next === maximum
    if (
      next === this.currentScrollTop &&
      nextFollowingEnd === this.followingEnd &&
      nextFollowSuppressedAtEnd === this.followSuppressedAtEnd
    ) return
    const moved = next !== this.currentScrollTop
    this.currentScrollTop = next
    this.followingEnd = nextFollowingEnd
    this.followSuppressedAtEnd = nextFollowSuppressedAtEnd
    if (moved) this.markScrollbarActivity()
    this.requestRenderCallback?.()
  }
  scrollBy(lines) {
    const requested = Number.isFinite(lines) ? Math.trunc(lines) : 0
    if (requested === 0) return 0
    const maximum = Math.max(0, this.contentHeight - this.currentViewportHeight)
    const start = this.followingEnd ? maximum : this.currentScrollTop
    const next = Math.max(0, Math.min(maximum, start + requested))
    const moved = next - start
    this.currentScrollTop = next
    const wasFollowingEnd = this.followingEnd
    this.followingEnd = this.followEnd && next === maximum
    this.followSuppressedAtEnd = false
    if (moved !== 0) {
      this.markScrollbarActivity()
    }
    if (moved !== 0 || this.followingEnd !== wasFollowingEnd) this.requestRenderCallback?.()
    return requested - moved
  }
  scrollToStart() {
    const changed = this.currentScrollTop !== 0 ||
      this.followingEnd !== (this.followEnd && this.contentHeight <= this.currentViewportHeight)
    this.currentScrollTop = 0
    this.followingEnd = this.followEnd && this.contentHeight <= this.currentViewportHeight
    this.followSuppressedAtEnd = false
    if (changed) {
      this.markScrollbarActivity()
      this.requestRenderCallback?.()
    }
  }
  scrollToEnd() {
    const next = Math.max(0, this.contentHeight - this.currentViewportHeight)
    const changed = this.currentScrollTop !== next || this.followingEnd !== this.followEnd
    this.currentScrollTop = next
    this.followingEnd = this.followEnd
    this.followSuppressedAtEnd = false
    if (changed) {
      this.markScrollbarActivity()
      this.requestRenderCallback?.()
    }
  }
  updateLayout(contentHeight, viewportHeight, requestRender) {
    this.contentHeight = Math.max(0, Math.floor(contentHeight))
    this.currentViewportHeight = Math.max(0, Math.floor(viewportHeight))
    this.requestRenderCallback = requestRender
    const maximum = Math.max(0, this.contentHeight - this.currentViewportHeight)
    if (this.followingEnd) this.currentScrollTop = maximum
    else this.currentScrollTop = Math.max(0, Math.min(this.currentScrollTop, maximum))
    if (this.currentScrollTop < maximum) this.followSuppressedAtEnd = false
    if (this.followEnd && this.currentScrollTop === maximum && !this.followSuppressedAtEnd) {
      this.followingEnd = true
    }
    if (this.contentHeight <= this.currentViewportHeight) this.hideTransientScrollbar()
  }
  addChild() { throw new Error('ScrollView has exactly one child') }
  removeChild() { throw new Error('ScrollView child cannot be removed') }
  clear() { throw new Error('ScrollView child cannot be cleared') }
  render(width) {
    const contentWidth = this.getContentWidth(width)
    const lines = this.child.render(contentWidth)
    return contentWidth === width ? lines : lines.map((line) => `${line} `)
  }
}

class SettingsList {
  constructor(items, maxVisible, theme, onChange, onCancel, options = {}) {
    this.items = items
    this.filteredItems = items
    this.maxVisible = maxVisible
    this.theme = theme
    this.onChange = onChange
    this.onCancel = onCancel
    this.selectedIndex = 0
    this.searchEnabled = options.enableSearch ?? false
    this.searchInput = this.searchEnabled ? new Input() : undefined
    this.submenuComponent = null
    this.submenuItemIndex = null
  }
  updateValue(id, newValue) {
    const item = this.items.find((candidate) => candidate.id === id)
    if (item) item.currentValue = newValue
  }
  invalidate() { this.submenuComponent?.invalidate?.() }
  render(width) {
    if (this.submenuComponent) return this.submenuComponent.render(width)
    const lines = []
    if (this.searchInput) lines.push(...this.searchInput.render(width), '')
    if (this.items.length === 0) {
      lines.push(this.theme.hint('  No settings available'))
      if (this.searchEnabled) this.addHintLine(lines, width)
      return lines
    }
    const display = this.searchEnabled ? this.filteredItems : this.items
    if (display.length === 0) {
      lines.push(truncateToWidth(this.theme.hint('  No matching settings'), width))
      this.addHintLine(lines, width)
      return lines
    }
    const start = Math.max(
      0,
      Math.min(
        this.selectedIndex - Math.floor(this.maxVisible / 2),
        display.length - this.maxVisible,
      ),
    )
    const end = Math.min(start + this.maxVisible, display.length)
    const labelWidth = Math.min(
      30,
      Math.max(...this.items.map((item) => visibleWidth(item.label))),
    )
    for (let index = start; index < end; index += 1) {
      const item = display[index]
      if (!item) continue
      const selected = index === this.selectedIndex
      const prefix = selected ? this.theme.cursor : '  '
      const label = item.label + ' '.repeat(Math.max(0, labelWidth - visibleWidth(item.label)))
      const used = visibleWidth(prefix) + labelWidth + 2
      const value = this.theme.value(
        truncateToWidth(item.currentValue, width - used - 2, ''),
        selected,
      )
      lines.push(
        truncateToWidth(
          `${prefix}${this.theme.label(label, selected)}  ${value}`,
          width,
        ),
      )
    }
    if (start > 0 || end < display.length) {
      lines.push(
        this.theme.hint(
          truncateToWidth(`  (${this.selectedIndex + 1}/${display.length})`, width - 2, ''),
        ),
      )
    }
    const selected = display[this.selectedIndex]
    if (selected?.description) {
      lines.push('')
      for (const line of wrapTextWithAnsi(selected.description, width - 4)) {
        lines.push(this.theme.description(`  ${line}`))
      }
    }
    this.addHintLine(lines, width)
    return lines
  }
  handleInput(data) {
    if (this.submenuComponent) {
      this.submenuComponent.handleInput?.(data)
      return
    }
    const bindings = getKeybindings()
    const display = this.searchEnabled ? this.filteredItems : this.items
    if (bindings.matches(data, 'tui.select.up')) {
      if (display.length) this.selectedIndex = this.selectedIndex === 0 ? display.length - 1 : this.selectedIndex - 1
    } else if (bindings.matches(data, 'tui.select.down')) {
      if (display.length) this.selectedIndex = this.selectedIndex === display.length - 1 ? 0 : this.selectedIndex + 1
    } else if (
      bindings.matches(data, 'tui.select.confirm') ||
      (data === ' ' && (!this.searchEnabled || this.searchInput?.getValue().length === 0))
    ) {
      this.activateItem()
    } else if (bindings.matches(data, 'tui.select.cancel')) {
      this.onCancel()
    } else if (this.searchInput) {
      this.searchInput.handleInput(data)
      this.filteredItems = fuzzyFilter(this.items, this.searchInput.getValue(), (item) => item.label)
      this.selectedIndex = 0
    }
  }
  activateItem() {
    const item = (this.searchEnabled ? this.filteredItems : this.items)[this.selectedIndex]
    if (!item) return
    if (item.submenu) {
      this.submenuItemIndex = this.selectedIndex
      this.submenuComponent = item.submenu(item.currentValue, (selectedValue) => {
        if (selectedValue !== undefined) {
          item.currentValue = selectedValue
          this.onChange(item.id, selectedValue)
        }
        this.closeSubmenu()
      })
    } else if (item.values?.length) {
      item.currentValue = item.values[(item.values.indexOf(item.currentValue) + 1) % item.values.length]
      this.onChange(item.id, item.currentValue)
    }
  }
  closeSubmenu() {
    this.submenuComponent = null
    if (this.submenuItemIndex !== null) {
      this.selectedIndex = this.submenuItemIndex
      this.submenuItemIndex = null
    }
  }
  addHintLine(lines, width) {
    lines.push('')
    lines.push(
      truncateToWidth(
        this.theme.hint(
          this.searchEnabled
            ? '  Type to search · Enter/Space to change · Esc to cancel'
            : '  Enter/Space to change · Esc to cancel',
        ),
        width,
      ),
    )
  }
}

class TuiBase extends Container {
  constructor(terminal, showHardwareCursor, _logDirectory) {
    super()
    this.terminal = terminal
    this.focusedComponent = null
    this.inputListeners = new Set()
    this.onDebug = undefined
    this.renderRequested = false
    this.immediateRenderScheduled = false
    this.renderTimer = undefined
    this.lastRenderAt = 0
    this.showHardwareCursor = showHardwareCursor ?? process.env.PI_HARDWARE_CURSOR === '1'
    this.clearOnShrink = process.env.PI_CLEAR_ON_SHRINK === '1'
    this.fullRedrawCount = 0
    this.stopped = false
    this.overlayStack = []
    this.focusOrderCounter = 0
    this.colorSchemeListeners = new Set()
    this.terminalColorSchemeNotificationsEnabled = false
    this.pendingOsc11BackgroundReplies = 0
    this.pendingOsc11BackgroundQueries = []
  }
  get fullRedraws() { return this.fullRedrawCount }
  getShowHardwareCursor() { return this.showHardwareCursor }
  setShowHardwareCursor(value) {
    this.showHardwareCursor = Boolean(value)
    if (!this.showHardwareCursor) this.terminal.hideCursor()
    this.requestRender()
  }
  getClearOnShrink() { return this.clearOnShrink }
  setClearOnShrink(value) { this.clearOnShrink = Boolean(value) }
  getFocusedComponent() { return this.focusedComponent }
  setFocus(component) {
    if (isFocusable(this.focusedComponent)) this.focusedComponent.focused = false
    this.focusedComponent = component
    if (isFocusable(component)) component.focused = true
  }
  showOverlay(component, options = {}) {
    const entry = {
      component,
      options,
      hidden: false,
      previous: this.focusedComponent,
      focusOrder: ++this.focusOrderCounter,
    }
    this.overlayStack.push(entry)
    if (!options.nonCapturing && this.isOverlayVisible(entry)) this.setFocus(component)
    this.terminal.hideCursor()
    this.requestRender()
    const handle = {
      hide: () => {
        const index = this.overlayStack.indexOf(entry)
        if (index !== -1) this.overlayStack.splice(index, 1)
        if (this.focusedComponent === component) {
          const visible = this.getTopmostVisibleOverlay()
          this.setFocus(visible?.component ?? entry.previous)
        }
        this.requestRender()
      },
      setHidden: (hidden) => {
        const next = Boolean(hidden)
        if (entry.hidden === next) return
        entry.hidden = next
        if (next && this.focusedComponent === component) {
          const visible = [...this.overlayStack]
            .reverse()
            .find((candidate) => this.isOverlayVisible(candidate) && candidate !== entry)
          this.setFocus(visible?.component ?? entry.previous)
        } else if (!next && !options.nonCapturing) {
          this.setFocus(component)
        }
        this.requestRender()
      },
      isHidden: () => entry.hidden,
      focus: () => {
        if (!this.overlayStack.includes(entry) || !this.isOverlayVisible(entry)) return
        entry.focusOrder = ++this.focusOrderCounter
        this.setFocus(component)
        this.requestRender()
      },
      unfocus: (settings) => this.setFocus(settings?.target ?? entry.previous),
      isFocused: () => this.focusedComponent === component,
    }
    entry.handle = handle
    return handle
  }
  hideOverlay() { this.overlayStack.at(-1)?.handle?.hide?.() }
  hasOverlay() { return this.overlayStack.some((entry) => this.isOverlayVisible(entry)) }
  isOverlayVisible(entry) {
    if (entry.hidden) return false
    return entry.options.visible
      ? Boolean(entry.options.visible(this.terminal.columns, this.terminal.rows))
      : true
  }
  getTopmostVisibleOverlay() {
    return this.overlayStack
      .filter((entry) => !entry.options.nonCapturing && this.isOverlayVisible(entry))
      .sort((left, right) => right.focusOrder - left.focusOrder)[0]
  }
  invalidate() { super.invalidate(); this.resetRenderState() }
  resetRenderState() {}
  start() {
    this.stopped = false
    this.beforeTerminalStart?.()
    this.terminal.start(
      (data) => this.handleTerminalInput(data),
      () => { this.resetRenderState(); this.requestRender(true) },
    )
    this.afterTerminalStart?.()
    this.terminal.hideCursor()
    if (this.terminalColorSchemeNotificationsEnabled) this.terminal.write('\x1b[?2031h')
    this.queryCellSize()
    this.requestRender()
  }
  stop(options = {}) {
    if (this.stopped) return
    this.stopped = true
    if (this.terminalColorSchemeNotificationsEnabled) this.terminal.write('\x1b[?2031l')
    this.beforeTerminalStop?.(options)
    if (this.renderTimer) clearTimeout(this.renderTimer)
    this.renderTimer = undefined
    this.renderRequested = false
    this.terminal.showCursor()
    this.terminal.stop()
    this.afterTerminalStop?.(options)
  }
  handleTerminalInput(data) {
    if (this.consumeOsc11BackgroundResponse(data)) return
    const scheme = parseTerminalColorSchemeReport(data)
    if (scheme) {
      for (const listener of this.colorSchemeListeners) listener(scheme)
      return
    }
    let current = data
    for (const listener of this.inputListeners) {
      const result = listener(current)
      if (result?.data !== undefined) current = result.data
      if (result?.consume) return
    }
    if (current.length === 0) return
    if (this.consumeCellSizeResponse(current)) return
    if (matchesKey(current, 'shift+ctrl+d') && this.onDebug) {
      this.onDebug()
      return
    }
    if (isKeyRelease(current) && !this.focusedComponent?.wantsKeyRelease) return
    this.focusedComponent?.handleInput?.(current)
    if (this.focusedComponent?.handleInput) this.requestImmediateRender()
  }
  renderNow(force = false) {
    if (force) this.resetRenderState()
    this.renderRequested = false
    if (this.renderTimer) clearTimeout(this.renderTimer)
    this.renderTimer = undefined
    this.doRender()
    this.lastRenderAt = Date.now()
  }
  requestRender(force = false) {
    if (force) {
      this.resetRenderState()
      this.requestImmediateRender()
      return
    }
    if (this.renderRequested) return
    this.renderRequested = true
    process.nextTick(() => this.scheduleRender())
  }
  requestImmediateRender() {
    if (this.renderTimer) clearTimeout(this.renderTimer)
    this.renderTimer = undefined
    this.renderRequested = true
    if (this.immediateRenderScheduled) return
    this.immediateRenderScheduled = true
    process.nextTick(() => {
      this.immediateRenderScheduled = false
      if (this.stopped || !this.renderRequested) return
      if (this.renderTimer) clearTimeout(this.renderTimer)
      this.renderTimer = undefined
      this.renderRequested = false
      this.lastRenderAt = Date.now()
      this.doRender()
    })
  }
  scheduleRender() {
    if (this.stopped || this.renderTimer || !this.renderRequested) return
    const delay = Math.max(0, 16 - (Date.now() - this.lastRenderAt))
    this.renderTimer = setTimeout(() => {
      this.renderTimer = undefined
      if (this.stopped || !this.renderRequested) return
      this.renderRequested = false
      this.lastRenderAt = Date.now()
      this.doRender()
      if (this.renderRequested) this.scheduleRender()
    }, delay)
  }
  addInputListener(listener) {
    this.inputListeners.add(listener)
    return () => this.inputListeners.delete(listener)
  }
  removeInputListener(listener) { this.inputListeners.delete(listener) }
  onTerminalColorSchemeChange(listener) {
    this.colorSchemeListeners.add(listener)
    return () => this.colorSchemeListeners.delete(listener)
  }
  setTerminalColorSchemeNotifications(enabled) {
    const next = Boolean(enabled)
    if (this.terminalColorSchemeNotificationsEnabled === next) return
    this.terminalColorSchemeNotificationsEnabled = next
    if (!this.stopped) this.terminal.write(next ? '\x1b[?2031h' : '\x1b[?2031l')
  }
  queryCellSize() {
    if (getCapabilities().images) this.terminal.write('\x1b[16t')
  }
  consumeCellSizeResponse(data) {
    const match = /^\x1b\[6;(\d+);(\d+)t$/.exec(data)
    if (!match) return false
    const heightPx = Number.parseInt(match[1], 10)
    const widthPx = Number.parseInt(match[2], 10)
    if (heightPx <= 0 || widthPx <= 0) return true
    setCellDimensions({ widthPx, heightPx })
    this.invalidate()
    this.requestRender()
    return true
  }
  queryTerminalBackgroundColor({ timeoutMs }) {
    return new Promise((resolve) => {
      const query = { resolve, settled: false, timer: undefined }
      query.timer = setTimeout(() => {
        if (query.settled) return
        query.settled = true
        query.resolve(undefined)
        query.resolve = undefined
      }, timeoutMs)
      query.timer.unref?.()
      this.pendingOsc11BackgroundReplies += 1
      this.pendingOsc11BackgroundQueries.push(query)
      this.terminal.write('\x1b]11;?\x07')
    })
  }
  consumeOsc11BackgroundResponse(data) {
    if (this.pendingOsc11BackgroundReplies <= 0 || !/^\x1b\]11;[^\x07\x1b]*(?:\x07|\x1b\\)$/i.test(data)) return false
    const rgb = parseOsc11BackgroundColor(data)
    this.pendingOsc11BackgroundReplies -= 1
    const query = this.pendingOsc11BackgroundQueries.shift()
    if (query && !query.settled) {
      query.settled = true
      if (query.timer) clearTimeout(query.timer)
      query.resolve?.(rgb)
      query.resolve = undefined
    }
    return true
  }
  queryTerminalColorScheme({ timeoutMs }) {
    return new Promise((resolve) => {
      let settled = false
      let timer
      let unsubscribe = () => {}
      const settle = (scheme) => {
        if (settled) return
        settled = true
        if (timer) clearTimeout(timer)
        unsubscribe()
        resolve(scheme)
      }
      unsubscribe = this.onTerminalColorSchemeChange(settle)
      timer = setTimeout(() => settle(undefined), timeoutMs)
      timer.unref?.()
      this.terminal.write('\x1b[?996n')
    })
  }
  render(width) { return super.render(width) }
  compositeOverlays(lines, width, height) {
    if (this.overlayStack.length === 0) return lines
    const base = [...lines]
    const ordered = this.overlayStack
      .filter((entry) => this.isOverlayVisible(entry))
      .sort((left, right) => left.focusOrder - right.focusOrder)
    for (const entry of ordered) {
      const margin = typeof entry.options.margin === 'number'
        ? { top: entry.options.margin, right: entry.options.margin, bottom: entry.options.margin, left: entry.options.margin }
        : (entry.options.margin ?? {})
      const left = Math.max(0, margin.left ?? 0)
      const right = Math.max(0, margin.right ?? 0)
      const top = Math.max(0, margin.top ?? 0)
      const bottom = Math.max(0, margin.bottom ?? 0)
      const availableWidth = Math.max(1, width - left - right)
      const requestedWidth = typeof entry.options.width === 'string' && /%$/.test(entry.options.width)
        ? Math.floor(width * Number.parseFloat(entry.options.width) / 100)
        : (entry.options.width ?? Math.min(80, availableWidth))
      const overlayWidth = Math.max(1, Math.min(availableWidth, Math.max(entry.options.minWidth ?? 1, requestedWidth)))
      let overlay = entry.component.render(overlayWidth)
      const maxHeight = typeof entry.options.maxHeight === 'string' && /%$/.test(entry.options.maxHeight)
        ? Math.floor(height * Number.parseFloat(entry.options.maxHeight) / 100)
        : entry.options.maxHeight
      if (maxHeight !== undefined) overlay = overlay.slice(0, Math.max(1, maxHeight))
      const availableHeight = Math.max(1, height - top - bottom)
      const anchor = entry.options.anchor ?? 'center'
      let overlayRow = entry.options.row ?? (anchor.startsWith('top') ? top : anchor.startsWith('bottom') ? top + availableHeight - overlay.length : top + Math.floor((availableHeight - overlay.length) / 2))
      let overlayCol = entry.options.col ?? (anchor.endsWith('left') || anchor === 'left-center' ? left : anchor.endsWith('right') || anchor === 'right-center' ? left + availableWidth - overlayWidth : left + Math.floor((availableWidth - overlayWidth) / 2))
      if (typeof overlayRow === 'string') overlayRow = top + Math.floor(Math.max(0, availableHeight - overlay.length) * Number.parseFloat(overlayRow) / 100)
      if (typeof overlayCol === 'string') overlayCol = left + Math.floor(Math.max(0, availableWidth - overlayWidth) * Number.parseFloat(overlayCol) / 100)
      overlayRow = Math.max(top, Math.min(overlayRow + (entry.options.offsetY ?? 0), height - bottom - overlay.length))
      overlayCol = Math.max(left, Math.min(overlayCol + (entry.options.offsetX ?? 0), width - right - overlayWidth))
      while (base.length < Math.max(height, overlayRow + overlay.length)) base.push('')
      const viewportStart = Math.max(0, base.length - height)
      for (let overlayLine = 0; overlayLine < overlay.length; overlayLine += 1) {
        const lineIndex = viewportStart + overlayRow + overlayLine
        if (lineIndex < 0 || lineIndex >= base.length) continue
        base[lineIndex] = compositeTuiLine(
          base[lineIndex] ?? '',
          overlay[overlayLine],
          overlayCol,
          overlayWidth,
          width,
        )
      }
    }
    return base
  }
}

const mainScreenPlannerRegistry = new WeakMap()
class TuiMainScreen extends TuiBase {
  constructor(...args) {
    super(...args)
    this.mode = 'regular'
    mainScreenPlannerRegistry.set(this, new native.NativeMainScreenPlanner())
  }
  captureRenderState() {
    const state = mainScreenPlannerRegistry.get(this).capture()
    return { ...state, previousLines: [...state.previousLines] }
  }
  restoreRenderState(state) {
    mainScreenPlannerRegistry.get(this).restore({
      ...state,
      previousLines: [...state.previousLines],
    })
  }
  resetRenderState() { mainScreenPlannerRegistry.get(this).reset() }
  beforeTerminalStart() { mainScreenPlannerRegistry.get(this).setStopped(false) }
  beforeTerminalStop(options = {}) {
    const state = this.captureRenderState()
    if (!options.preserveScreen && state.previousLines.length > 0) {
      this.terminal.write(' ')
      const difference = state.previousLines.length - state.hardwareCursorRow
      if (difference > 0) this.terminal.write(`\x1b[${difference}B`)
      else if (difference < 0) this.terminal.write(`\x1b[${-difference}A`)
      this.terminal.write('\r\n')
    }
    mainScreenPlannerRegistry.get(this).setStopped(true)
  }
  doRender() {
    if (this.stopped) return
    const width = Math.max(1, this.terminal.columns)
    const height = Math.max(1, this.terminal.rows)
    const lines = this.compositeOverlays(this.render(width), width, height)
    const planner = mainScreenPlannerRegistry.get(this)
    const actions = planner.render(
      lines,
      width,
      height,
      false,
      this.clearOnShrink && this.overlayStack.length === 0,
      this.showHardwareCursor,
    )
    for (const action of actions) {
      if (action.kind === 'write') this.terminal.write(action.data)
      else if (action.kind === 'hideCursor') this.terminal.hideCursor()
      else if (action.kind === 'showCursor') this.terminal.showCursor()
    }
    this.fullRedrawCount = planner.fullRedraws
  }
}

const ALT_FOCUS_IN = '\x1b[I'
const ALT_FOCUS_OUT = '\x1b[O'
const ALT_PAGE_OVERLAP = 4
const ALT_OSC133_ZONE_PREFIX = /^(?:\x1b\]133;[ABC](?:\x07|\x1b\\))+/
const ALT_KITTY_PLACEMENT_KEYS = new Set([
  'i', 'p', 'x', 'y', 'w', 'h', 'X', 'Y', 'c', 'r', 'C', 'U', 'z', 'P', 'Q', 'H', 'V',
])

function getKittyImagePlacement(line) {
  const match = /\x1b_G([^;]*);/.exec(line)
  if (!match) return undefined
  const imageIdText = /(?:^|,)i=(\d+)(?:,|$)/.exec(match[1])?.[1]
  const metadata = imageIdText === undefined
    ? undefined
    : kittyImageMetadata.get(Number.parseInt(imageIdText, 10))
  if (!metadata) return undefined
  let commandStart = match.index
  let commandControls = match[1]
  let transmissionEnd
  while (true) {
    const terminator = line.indexOf('\x1b\\', commandStart + 3)
    if (terminator === -1) return undefined
    transmissionEnd = terminator + 2
    if (!/(?:^|,)m=1(?:,|$)/.test(commandControls)) break
    commandStart = transmissionEnd
    if (!line.startsWith('\x1b_G', commandStart)) return undefined
    const controlsEnd = line.indexOf(';', commandStart + 3)
    if (controlsEnd === -1) return undefined
    commandControls = line.slice(commandStart + 3, controlsEnd)
  }
  const controls = match[1]
    .split(',')
    .filter((control) => ALT_KITTY_PLACEMENT_KEYS.has(control.split('=', 1)[0] ?? ''))
  const sequence = `\x1b_Ga=p,q=2,${controls.join(',')}\x1b\\`
  return {
    imageId: metadata.imageId,
    transmissionGeneration: metadata.transmissionGeneration,
    transmissionBytes: transmissionEnd - match.index,
    estimatedDecodedBytes: metadata.widthPx * metadata.heightPx * 4,
    replacementLine: `${line.slice(0, match.index)}${sequence}${line.slice(transmissionEnd)}`,
  }
}

function findAltScreenSearchMatches(lines, query) {
  const normalized = String(query).replace(/\s+/gu, ' ').trim()
  if (!normalized) return []
  let text = ''
  const source = []
  let pendingSeparator = false
  for (let row = 0; row < lines.length; row += 1) {
    const plain = stripTerminalSequences(lines[row] ?? '')
    let column = 0
    for (const character of plain) {
      const width = visibleWidth(character)
      if (/^\s+$/u.test(character)) {
        if (text.length > 0) pendingSeparator = true
        column += width
        continue
      }
      if (pendingSeparator) {
        text += ' '
        source.push(undefined)
        pendingSeparator = false
      }
      text += character
      for (let index = 0; index < character.length; index += 1) {
        source.push({ row, startCol: column, endCol: column + width })
      }
      column += width
    }
    if (text.length > 0) pendingSeparator = true
  }
  const escaped = normalized.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const expression = new RegExp(escaped, 'giu')
  const matches = []
  for (const match of text.matchAll(expression)) {
    const segments = []
    for (let index = match.index; index < match.index + match[0].length; index += 1) {
      const span = source[index]
      if (!span) continue
      const previous = segments.at(-1)
      if (previous && previous.row === span.row && span.startCol <= previous.endCol) {
        previous.endCol = Math.max(previous.endCol, span.endCol)
      } else {
        segments.push({ ...span })
      }
    }
    if (segments.length > 0) matches.push({ segments })
  }
  return matches
}

class AltScreenSearchComponent {
  constructor(onQueryChange) {
    this.input = new Input()
    this.onQueryChange = onQueryChange
    this.resultCount = 0
    this.resultIndex = -1
    this._focused = false
  }
  get focused() { return this._focused }
  set focused(value) { this._focused = Boolean(value); this.input.focused = Boolean(value) }
  setResult(index, count) { this.resultIndex = index; this.resultCount = count }
  handleInput(data) {
    const previous = this.input.getValue()
    this.input.handleInput(data)
    const query = this.input.getValue()
    if (query !== previous) this.onQueryChange(query)
  }
  invalidate() { this.input.invalidate() }
  render(width) {
    const safeWidth = Math.max(1, width)
    const label = ' Find transcript'
    const query = this.input.getValue()
    const status = !query ? '' : this.resultCount === 0 ? 'No matches ' : `${this.resultIndex + 1}/${this.resultCount} `
    const gap = ' '.repeat(Math.max(1, safeWidth - visibleWidth(label) - visibleWidth(status)))
    const title = truncateToWidth(`${label}${gap}${status}`, safeWidth, '', true)
    return [`\x1b[7m${title}\x1b[27m`, ...this.input.render(safeWidth)]
  }
}

const altScreenPlannerRegistry = new WeakMap()

function intersectLayoutRect(a, b) {
  const x = Math.max(a.x, b.x)
  const y = Math.max(a.y, b.y)
  const right = Math.min(a.x + a.width, b.x + b.width)
  const bottom = Math.min(a.y + a.height, b.y + b.height)
  return { x, y, width: Math.max(0, right - x), height: Math.max(0, bottom - y) }
}

function renderLayoutFrame(root, width, height, requestRender) {
  const safeWidth = Math.max(1, Math.floor(width))
  const safeHeight = Math.max(1, Math.floor(height))
  const context = {
    viewport: { width: safeWidth, height: safeHeight },
    cache: new Map(),
    requestRender,
    primaryScrollView: undefined,
  }
  const renderCached = (component, componentWidth) => {
    const normalizedWidth = Math.max(1, Math.floor(componentWidth))
    let widths = context.cache.get(component)
    if (!widths) {
      widths = new Map()
      context.cache.set(component, widths)
    }
    if (!widths.has(normalizedWidth)) widths.set(normalizedWidth, component.render(normalizedWidth))
    return widths.get(normalizedWidth)
  }
  const translate = (box, deltaY) => {
    box.rect.y += deltaY
    for (const child of box.children) translate(child, deltaY)
  }
  const updateClips = (box, parentClip) => {
    box.clip = intersectLayoutRect(parentClip, box.rect)
    for (const child of box.children) updateClips(child, box.clip)
  }
  const layout = (component, x, y, componentWidth, componentHeight, clip) => {
    const normalizedWidth = Math.max(1, Math.floor(componentWidth))
    if (component instanceof ScrollView) {
      const previousTop = component.scrollTop
      const contentWidth = component.getContentWidth(normalizedWidth)
      const child = layout(component.child, x, y - previousTop, contentWidth, undefined, clip)
      const contentHeight = child.rect.height
      const viewportHeight = componentHeight === undefined
        ? contentHeight
        : Math.max(0, Math.floor(componentHeight))
      component.updateLayout(contentHeight, viewportHeight, requestRender)
      translate(child, previousTop - component.scrollTop)
      const rect = { x, y, width: normalizedWidth, height: viewportHeight }
      if (component.primary || !context.primaryScrollView) {
        context.primaryScrollView = component
        context.primaryContentLines = renderCached(component.child, contentWidth)
        context.primaryRect = rect
      }
      const box = {
        component,
        rect,
        clip: intersectLayoutRect(clip, rect),
        children: [child],
        scrollView: component,
      }
      updateClips(child, box.clip)
      return box
    }
    if (component instanceof VStack) {
      const entries = component.visibleEntries(context.viewport)
      const intrinsic = entries.map((entry) =>
        typeof entry.basis === 'number'
          ? entry.basis
          : renderCached(entry.component, normalizedWidth).length,
      )
      const sizes = component.allocate(entries, intrinsic, componentHeight)
      const naturalHeight = sizes.reduce((sum, size) => sum + size, 0) +
        Math.max(0, entries.length - 1) * component.gap
      const allocatedHeight = componentHeight === undefined
        ? naturalHeight
        : Math.max(0, Math.floor(componentHeight))
      const rect = { x, y, width: normalizedWidth, height: allocatedHeight }
      const box = { component, rect, clip: intersectLayoutRect(clip, rect), children: [] }
      let childY = y
      for (let index = 0; index < entries.length; index += 1) {
        const child = layout(
          entries[index].component,
          x,
          childY,
          normalizedWidth,
          sizes[index],
          box.clip,
        )
        box.children.push(child)
        childY += sizes[index] + component.gap
      }
      return box
    }
    if (component instanceof HStack) {
      const entries = component.visibleEntries(context.viewport)
      const intrinsicWidths = entries.map((entry) =>
        typeof entry.basis === 'number'
          ? entry.basis
          : renderCached(entry.component, normalizedWidth)
              .reduce((maximum, line) => Math.max(maximum, visibleWidth(line)), 0),
      )
      const widths = component.allocate(entries, intrinsicWidths, normalizedWidth)
      const intrinsicHeights = entries.map((entry, index) =>
        widths[index] === 0 ? 0 : renderCached(entry.component, widths[index]).length,
      )
      const allocatedHeight = componentHeight === undefined
        ? intrinsicHeights.reduce((maximum, value) => Math.max(maximum, value), 0)
        : Math.max(0, Math.floor(componentHeight))
      const rect = { x, y, width: normalizedWidth, height: allocatedHeight }
      const box = { component, rect, clip: intersectLayoutRect(clip, rect), children: [] }
      let childX = x
      for (let index = 0; index < entries.length; index += 1) {
        const childWidth = widths[index]
        if (childWidth > 0) {
          const naturalHeight = intrinsicHeights[index]
          const childHeight = component.align === 'stretch'
            ? allocatedHeight
            : Math.min(allocatedHeight, naturalHeight)
          let childY = y
          if (component.align === 'center') childY += Math.floor((allocatedHeight - childHeight) / 2)
          else if (component.align === 'end') childY += allocatedHeight - childHeight
          box.children.push(layout(
            entries[index].component,
            childX,
            childY,
            childWidth,
            childHeight,
            box.clip,
          ))
        }
        childX += childWidth + component.gap
      }
      return box
    }
    const lines = renderCached(component, normalizedWidth)
    const allocatedHeight = componentHeight === undefined
      ? lines.length
      : Math.max(0, Math.floor(componentHeight))
    let lineOffset = 0
    if (lines.length > allocatedHeight && allocatedHeight > 0) {
      const cursorLine = lines.findIndex((line) => line.includes(CURSOR_MARKER))
      if (cursorLine >= allocatedHeight) lineOffset = cursorLine - allocatedHeight + 1
    }
    const rect = { x, y, width: normalizedWidth, height: allocatedHeight }
    return {
      component,
      rect,
      clip: intersectLayoutRect(clip, rect),
      children: [],
      lines,
      lineOffset,
    }
  }
  const rootBox = layout(root, 0, 0, safeWidth, safeHeight, {
    x: 0,
    y: 0,
    width: safeWidth,
    height: safeHeight,
  })
  const lines = Array.from({ length: safeHeight }, () => '')
  const paint = (box) => {
    if (box.lines) {
      const first = Math.max(0, box.rect.y, box.clip.y)
      const last = Math.min(
        lines.length,
        box.rect.y + box.rect.height,
        box.clip.y + box.clip.height,
      )
      for (let row = first; row < last; row += 1) {
        const line = box.lines[(box.lineOffset ?? 0) + row - box.rect.y]
        if (line === undefined) continue
        lines[row] = box.rect.x === 0 && box.rect.width >= safeWidth && !lines[row]
          ? line
          : compositeTuiLine(lines[row], line, box.rect.x, box.rect.width, safeWidth)
      }
    }
    for (const child of box.children) paint(child)
  }
  paint(rootBox)
  return {
    root: rootBox,
    width: safeWidth,
    height: safeHeight,
    lines,
    primaryScrollView: context.primaryScrollView,
    primaryContentLines: context.primaryContentLines,
    primaryRect: context.primaryRect,
  }
}

class TuiAltScreen extends TuiBase {
  constructor(terminal, showHardwareCursor, logDirectory, options = {}) {
    super(terminal, showHardwareCursor, logDirectory)
    this.mode = 'fullscreen'
    this.layoutRoot = undefined
    this.currentLayout = undefined
    this.wheelScrollLines = Math.max(1, Math.floor(options.wheelScrollLines ?? 1))
    this.mouseEnabled = options.mouse ?? true
    this.searchMatchStyle = options.searchMatchStyle ?? ((text) => `\x1b[4m${text}\x1b[24m`)
    this.searchCurrentMatchStyle = options.searchCurrentMatchStyle ?? ((text) => `\x1b[1;7m${text}\x1b[22;27m`)
    this.openUrl = options.openUrl
    this.onRightClickPaste = options.onRightClickPaste
    this.copySelection = options.copySelection
    this.implicitScrollView = new ScrollView({
      render: (width) => Container.prototype.render.call(this, width),
      invalidate: () => { for (const child of this.children) child.invalidate() },
    }, { follow: 'end', primary: true })
    this.previousScreen = []
    this.lastDocument = []
    this.altScreenActive = false
    this.flashes = []
    this.imageProtocol = null
    this.savedCapabilities = undefined
    this.uploadedKittyImages = new Map()
    this.selectionAnchor = undefined
    this.selectionFocus = undefined
    this.selectionGranularity = 'character'
    this.selectionInitialRange = undefined
    this.lastClick = undefined
    this.selectionPressActive = false
    this.selectionDragged = false
    this.pressedUrl = undefined
    this.activeSearch = undefined
    altScreenPlannerRegistry.set(this, new native.NativeAltScreenPlanner())
    this.addInputListener((data) => this.handleViewportInput(data))
  }
  getPrimaryScrollView() {
    return this.currentLayout?.primaryScrollView ?? this.implicitScrollView
  }
  get viewportTop() { return this.getPrimaryScrollView().scrollTop }
  get isFollowingOutput() { return this.getPrimaryScrollView().isFollowingEnd }
  setLayoutRoot(component) {
    if (this.layoutRoot === component) return
    this.layoutRoot = component
    this.currentLayout = undefined
    this.requestRender()
  }
  beforeTerminalStart() {
    this.altScreenActive = true
    this.previousScreen = []
    this.lastDocument = []
    this.selectionAnchor = undefined
    this.selectionFocus = undefined
    this.selectionGranularity = 'character'
    this.selectionInitialRange = undefined
    this.lastClick = undefined
    this.selectionPressActive = false
    this.selectionDragged = false
    this.pressedUrl = undefined
    const capabilities = getCapabilities()
    this.imageProtocol = capabilities.images
    this.uploadedKittyImages.clear()
    if (capabilities.images === 'iterm2') {
      this.savedCapabilities = capabilities
      setCapabilities({ ...capabilities, images: null })
      this.invalidate()
    }
    altScreenPlannerRegistry.get(this).reset()
    const multiplexer = process.env.TMUX !== undefined || process.env.ZELLIJ !== undefined ||
      process.env.STY !== undefined || /^(tmux|screen)/i.test(process.env.TERM ?? '')
    const mouse = multiplexer
      ? '\x1b[?1000h\x1b[?1002h\x1b[?1004h\x1b[?1006h'
      : '\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1004h\x1b[?1006h'
    this.terminal.write(`\x1b[?1049h\x1b[?7l${this.mouseEnabled ? mouse : ''}\x1b[2J\x1b[H\x1b[?25l`)
  }
  beforeTerminalStop() {
    this.closeSearch()
    this.selectionPressActive = false
    if (!this.altScreenActive) return
    const disableMouse = '\x1b[?1006l\x1b[?1004l\x1b[?1003l\x1b[?1002l\x1b[?1000l'
    const images = this.imageProtocol === 'kitty' ? deleteAllKittyImages() : ''
    this.terminal.write(`\x1b[?2026h${images}${this.mouseEnabled ? disableMouse : ''}\x1b[?7h\x1b[?2026l`)
    this.uploadedKittyImages.clear()
  }
  afterTerminalStop(options = {}) {
    if (!this.altScreenActive) return
    this.altScreenActive = false
    if (options.preserveScreen) {
      this.terminal.write('\x1b[?2026h\x1b[?1049l\x1b[?25h\x1b[?2026l')
    } else {
      const width = Math.max(1, this.terminal.columns)
      const lines = this.render(width).map((line) =>
        line.replace(ALT_OSC133_ZONE_PREFIX, '').replaceAll(CURSOR_MARKER, ''),
      )
      this.lastDocument = lines.map((line) =>
        isImageLine(line) || visibleWidth(line) <= width
          ? line
          : sliceByColumn(line, 0, width, true),
      )
      let buffer = '\x1b[?2026h\x1b[?1049l\x1b[?7l'
      for (let row = 0; row < this.lastDocument.length; row += 1) {
        if (row > 0) buffer += '\r\n'
        buffer += `\r\x1b[2K${this.lastDocument[row]}`
      }
      buffer += '\x1b[0m\x1b[?7h\r\n\x1b[?25h\x1b[?2026l'
      this.terminal.write(buffer)
    }
    if (this.savedCapabilities) {
      setCapabilities(this.savedCapabilities)
      this.savedCapabilities = undefined
    }
  }
  render(width) {
    return this.layoutRoot
      ? this.layoutRoot.render(width)
      : Container.prototype.render.call(this, width)
  }
  scrollBy(lines) {
    this.getPrimaryScrollView().scrollBy(lines)
    this.requestRender()
  }
  scrollToTop() { this.getPrimaryScrollView().scrollToStart(); this.requestRender() }
  scrollToBottom() { this.getPrimaryScrollView().scrollToEnd(); this.requestRender() }
  scrollToPrompt(direction) {
    for (let row = this.viewportTop + direction; row >= 0 && row < this.lastDocument.length; row += direction) {
      if (!/^\x1b\]133;A(?:\x07|\x1b\\)/.test(this.lastDocument[row] ?? '')) continue
      this.getPrimaryScrollView().scrollTo(row)
      this.requestRender()
      return
    }
  }
  openSearch() {
    if (this.activeSearch) {
      this.activeSearch.overlay?.focus()
      return
    }
    const component = new AltScreenSearchComponent((query) => this.updateSearchQuery(query))
    const search = {
      component,
      query: '',
      matches: [],
      selectedIndex: -1,
      anchorRow: this.viewportTop,
      selectionMode: 'query',
    }
    this.activeSearch = search
    search.overlay = this.showOverlay(component, {
      anchor: 'top-right', width: '40%', minWidth: 24, margin: 1,
    })
  }
  closeSearch() {
    const search = this.activeSearch
    if (!search) return
    this.activeSearch = undefined
    search.overlay?.hide()
    this.requestRender()
  }
  updateSearchQuery(query) {
    const search = this.activeSearch
    if (!search || query === search.query) return
    search.anchorRow = search.matches[search.selectedIndex]?.segments[0]?.row ?? this.viewportTop
    search.query = query
    search.selectionMode = 'query'
    search.component.setResult(-1, 0)
    this.requestRender()
  }
  navigateSearch(direction) {
    if (!this.activeSearch?.query) return
    this.activeSearch.selectionMode = direction < 0 ? 'previous' : 'next'
    this.requestRender()
  }
  refreshSearch(document, height) {
    const search = this.activeSearch
    if (!search) return
    const matches = findAltScreenSearchMatches(document, search.query)
    let selectedIndex = search.selectedIndex
    if (matches.length === 0) selectedIndex = -1
    else if (search.selectionMode === 'query') {
      selectedIndex = matches.findIndex((match) => (match.segments[0]?.row ?? 0) >= search.anchorRow)
      if (selectedIndex < 0) selectedIndex = 0
    } else if (search.selectionMode === 'next') {
      selectedIndex = selectedIndex < 0 ? 0 : (selectedIndex + 1) % matches.length
    } else if (search.selectionMode === 'previous') {
      selectedIndex = selectedIndex < 0 ? matches.length - 1 : (selectedIndex - 1 + matches.length) % matches.length
    } else {
      selectedIndex = Math.min(Math.max(0, selectedIndex), matches.length - 1)
    }
    const reveal = search.selectionMode !== 'retain'
    search.matches = matches
    search.selectedIndex = selectedIndex
    search.selectionMode = 'retain'
    search.component.setResult(selectedIndex, matches.length)
    if (!reveal || selectedIndex < 0) return
    const first = matches[selectedIndex]?.segments[0]
    const last = matches[selectedIndex]?.segments.at(-1)
    if (!first || !last) return
    if (first.row < this.viewportTop || last.row >= this.viewportTop + height) {
      this.getPrimaryScrollView().scrollTo(first.row - Math.floor(height / 3), { disableFollow: true })
    }
  }
  flash(message, durationMs = 2000) {
    const entry = { message: String(message) }
    this.flashes.push(entry)
    this.requestRender()
    const timer = setTimeout(() => {
      const index = this.flashes.indexOf(entry)
      if (index !== -1) this.flashes.splice(index, 1)
      this.requestRender()
    }, durationMs)
    timer.unref?.()
  }
  handleViewportInput(data) {
    if (data === ALT_FOCUS_OUT) {
      const hadSelection = this.getSelectionBounds() !== undefined
      this.selectionPressActive = false
      this.selectionAnchor = undefined
      this.selectionFocus = undefined
      this.lastClick = undefined
      this.pressedUrl = undefined
      if (hadSelection) this.requestRender()
      return { consume: true }
    }
    if (data === ALT_FOCUS_IN) return { consume: true }
    const mouse = this.parseMouseEvent(data)
    if (mouse && (mouse.button & 64) !== 0) {
      const direction = (mouse.button & 3) === 0 ? -1 : (mouse.button & 3) === 1 ? 1 : 0
      if (direction !== 0) this.scrollBy(direction * this.wheelScrollLines)
      return { consume: true }
    }
    if (mouse) {
      if (this.handleRightClickPaste(mouse)) return { consume: true }
      this.handleSelectionMouseEvent(mouse)
      return { consume: true }
    }
    if (this.isMouseSequence(data)) return { consume: true }
    const bindings = getKeybindings()
    const release = isKeyRelease(data)
    if (bindings.matches(data, 'tui.altScreen.search')) {
      if (!release) this.openSearch()
      return { consume: true }
    }
    if (this.activeSearch?.overlay?.isFocused()) {
      if (bindings.matches(data, 'tui.altScreen.searchNext')) {
        if (!release) this.navigateSearch(1)
        return { consume: true }
      }
      if (bindings.matches(data, 'tui.altScreen.searchPrevious')) {
        if (!release) this.navigateSearch(-1)
        return { consume: true }
      }
      if (bindings.matches(data, 'tui.altScreen.searchClose')) {
        if (!release) this.closeSearch()
        return { consume: true }
      }
    }
    const focusedOverlay = this.getTopmostVisibleOverlay()
    if (focusedOverlay && this.activeSearch?.overlay?.isFocused() !== true) return undefined
    const height = this.getPrimaryScrollView().viewportHeight
    for (const [id, action] of [
      ['tui.altScreen.pageUp', () => this.scrollBy(-Math.max(1, height - ALT_PAGE_OVERLAP))],
      ['tui.altScreen.pageDown', () => this.scrollBy(Math.max(1, height - ALT_PAGE_OVERLAP))],
      ['tui.altScreen.halfPageUp', () => this.scrollBy(-Math.max(1, Math.floor(height / 2)))],
      ['tui.altScreen.halfPageDown', () => this.scrollBy(Math.max(1, Math.floor(height / 2)))],
      ['tui.altScreen.lineUp', () => this.scrollBy(-1)],
      ['tui.altScreen.lineDown', () => this.scrollBy(1)],
      ['tui.altScreen.previousPrompt', () => this.scrollToPrompt(-1)],
      ['tui.altScreen.nextPrompt', () => this.scrollToPrompt(1)],
      ['tui.altScreen.top', () => this.scrollToTop()],
      ['tui.altScreen.bottom', () => this.scrollToBottom()],
    ]) {
      if (!bindings.matches(data, id)) continue
      if (!release) action()
      return { consume: true }
    }
    return undefined
  }
  parseMouseEvent(data) {
    const match = /^\x1b\[<(\d+);(\d+);(\d+)([Mm])$/.exec(data)
    if (match) return {
      button: Number.parseInt(match[1], 10),
      x: Number.parseInt(match[2], 10) - 1,
      y: Number.parseInt(match[3], 10) - 1,
      release: match[4] === 'm',
    }
    if (data.length === 6 && data.startsWith('\x1b[M')) return {
      button: data.charCodeAt(3) - 32,
      x: data.charCodeAt(4) - 33,
      y: data.charCodeAt(5) - 33,
      release: false,
    }
    return undefined
  }
  isMouseSequence(data) {
    return /^\x1b\[<\d+;\d+;\d+[Mm]$/.test(data) || (data.length === 6 && data.startsWith('\x1b[M'))
  }
  handleRightClickPaste(event) {
    if (!this.onRightClickPaste || process.platform !== 'win32' || event.release || event.button !== 2) return false
    try { this.onRightClickPaste() } catch {}
    return true
  }
  selectionPoint(event) {
    const primaryTop = this.currentLayout?.primaryRect?.y ?? 0
    return {
      row: Math.max(0, Math.min(this.lastDocument.length - 1, this.viewportTop + event.y - primaryTop)),
      col: Math.max(0, Math.min(this.terminal.columns - 1, event.x)),
    }
  }
  handleSelectionMouseEvent(event) {
    const button = event.button & 3
    if (button !== 0 && !(event.release && button === 3)) return
    const point = this.selectionPoint(event)
    if (event.release) {
      if (!this.selectionPressActive) return
      this.selectionPressActive = false
      this.selectionFocus = point
      const clickedUrl = !this.selectionDragged &&
        this.selectionAnchor?.row === point.row && this.selectionAnchor?.col === point.col
          ? this.pressedUrl : undefined
      this.pressedUrl = undefined
      if (clickedUrl && this.openUrl) {
        this.selectionAnchor = undefined
        this.selectionFocus = undefined
        try { this.openUrl(clickedUrl) } catch {}
      } else {
        void this.copySelectionToClipboard()
      }
      this.requestRender()
      return
    }
    if ((event.button & 32) !== 0) {
      if (!this.selectionPressActive || !this.selectionAnchor) return
      this.selectionDragged = true
      this.selectionFocus = point
      this.requestRender()
      return
    }
    this.selectionPressActive = true
    this.selectionAnchor = point
    this.selectionFocus = point
    this.selectionDragged = false
    this.pressedUrl = getOsc8LinkAtColumn(this.previousScreen[event.y] ?? '', point.col)
    this.requestRender()
  }
  getSelectionBounds() {
    const anchor = this.selectionAnchor
    const focus = this.selectionFocus
    if (!anchor || !focus || (anchor.row === focus.row && anchor.col === focus.col)) return undefined
    return anchor.row < focus.row || (anchor.row === focus.row && anchor.col < focus.col)
      ? { start: anchor, end: focus }
      : { start: focus, end: anchor }
  }
  getSelectionColumns(line, row, selection) {
    const width = visibleWidth(line)
    const start = row === selection.start.row ? Math.min(selection.start.col, width) : 0
    const end = row === selection.end.row ? Math.min(selection.end.col + 1, width) : width
    return { start, end }
  }
  async copySelectionToClipboard() {
    const selection = this.getSelectionBounds()
    if (!selection) return
    const lines = []
    for (let row = selection.start.row; row <= selection.end.row; row += 1) {
      const line = this.lastDocument[row] ?? ''
      const columns = this.getSelectionColumns(line, row, selection)
      lines.push(stripTerminalSequences(sliceByColumn(line, columns.start, Math.max(0, columns.end - columns.start), true)).trimEnd())
    }
    const text = lines.join('\n')
    if (!text) return
    if (this.copySelection) {
      const copied = await this.copySelection(text)
      this.flash(copied ? 'Copied!' : 'Copy failed')
    } else {
      this.terminal.write(`\x1b]52;c;${Buffer.from(text).toString('base64')}\x07`)
      this.flash('Copied!')
    }
  }
  applySearchHighlights(screen) {
    const search = this.activeSearch
    if (!search || search.selectedIndex < 0) return screen
    const result = [...screen]
    for (let index = 0; index < search.matches.length; index += 1) {
      for (const segment of search.matches[index].segments) {
        const row = (this.currentLayout?.primaryRect?.y ?? 0) + segment.row - this.viewportTop
        if (row < 0 || row >= result.length || isImageLine(result[row] ?? '')) continue
        const line = result[row] ?? ''
        const width = visibleWidth(line)
        const start = Math.min(segment.startCol, width)
        const end = Math.min(segment.endCol, width)
        if (end <= start) continue
        const before = sliceByColumn(line, 0, start, true)
        const match = sliceByColumn(line, start, end - start, true)
        const after = sliceByColumn(line, end, Math.max(0, width - end), true)
        const style = index === search.selectedIndex ? this.searchCurrentMatchStyle : this.searchMatchStyle
        result[row] = `${before}${style(match)}${after}`
      }
    }
    return result
  }
  applySelection(screen) {
    const selection = this.getSelectionBounds()
    if (!selection) return screen
    return screen.map((line, row) => {
      const documentRow = this.viewportTop + row - (this.currentLayout?.primaryRect?.y ?? 0)
      if (documentRow < selection.start.row || documentRow > selection.end.row || isImageLine(line)) return line
      const columns = this.getSelectionColumns(line, documentRow, selection)
      if (columns.end <= columns.start) return line
      const width = visibleWidth(line)
      return `${sliceByColumn(line, 0, columns.start, true)}\x1b[7m${sliceByColumn(line, columns.start, columns.end - columns.start, true)}\x1b[27m${sliceByColumn(line, columns.end, Math.max(0, width - columns.end), true)}`
    })
  }
  prepareKittyScreen(screen) {
    const visibleIds = new Set()
    const lines = screen.map((line) => {
      const placement = getKittyImagePlacement(line)
      if (!placement) return line
      visibleIds.add(placement.imageId)
      const cached = this.uploadedKittyImages.get(placement.imageId)
      this.uploadedKittyImages.delete(placement.imageId)
      this.uploadedKittyImages.set(placement.imageId, placement)
      return cached?.transmissionGeneration === placement.transmissionGeneration
        ? placement.replacementLine : line
    })
    let offscreen = [...this.uploadedKittyImages.entries()].filter(([id]) => !visibleIds.has(id))
    let transmissionBytes = offscreen.reduce((sum, [, item]) => sum + item.transmissionBytes, 0)
    let decodedBytes = offscreen.reduce((sum, [, item]) => sum + item.estimatedDecodedBytes, 0)
    let evicted = ''
    for (const [id, item] of offscreen) {
      if (offscreen.length <= 16 && transmissionBytes <= 32 * 1024 * 1024 && decodedBytes <= 64 * 1024 * 1024) break
      evicted += deleteKittyImage(id)
      this.uploadedKittyImages.delete(id)
      offscreen = offscreen.slice(1)
      transmissionBytes -= item.transmissionBytes
      decodedBytes -= item.estimatedDecodedBytes
    }
    return { lines, evicted }
  }
  doRender() {
    if (this.stopped || !this.altScreenActive) return
    const width = Math.max(1, this.terminal.columns)
    const height = Math.max(1, this.terminal.rows)
    const root = this.layoutRoot ?? this.implicitScrollView
    this.currentLayout = renderLayoutFrame(root, width, height, () => this.requestRender())
    let document = this.currentLayout.lines
      .map((line) => line.replace(ALT_OSC133_ZONE_PREFIX, ''))
    this.lastDocument = (this.currentLayout.primaryContentLines ?? document)
      .map((line) => line.replace(ALT_OSC133_ZONE_PREFIX, ''))
    const beforeSearchTop = this.viewportTop
    this.refreshSearch(this.lastDocument, this.getPrimaryScrollView().viewportHeight)
    if (this.viewportTop !== beforeSearchTop) {
      this.currentLayout = renderLayoutFrame(root, width, height, () => this.requestRender())
      document = this.currentLayout.lines
        .map((line) => line.replace(ALT_OSC133_ZONE_PREFIX, ''))
    }
    let screen = document.slice(0, height)
    while (screen.length < height) screen.push('')
    screen = this.applySearchHighlights(screen)
    if (this.flashes.length > 0) {
      const message = this.flashes.at(-1).message
      const flashWidth = Math.min(width, visibleWidth(message))
      screen[0] = compositeTuiLine(screen[0] ?? '', message, width - flashWidth, flashWidth, width)
    }
    let composited = this.compositeOverlays(screen, width, height)
    composited = this.applySelection(composited)
    const planner = altScreenPlannerRegistry.get(this)
    const fullRedraw = this.previousScreen.length === 0
    const imagesNeedRedraw = composited.some((line, row) =>
      line !== this.previousScreen[row] && (isImageLine(line) || isImageLine(this.previousScreen[row] ?? '')),
    )
    const hadUploadedKittyImages = this.uploadedKittyImages.size > 0
    const prepared = (fullRedraw || imagesNeedRedraw) && this.imageProtocol === 'kitty'
      ? this.prepareKittyScreen(composited)
      : { lines: composited, evicted: '' }
    let buffer = planner.render(prepared.lines, width, height)
    let imagePrefix = prepared.evicted
    if (fullRedraw && this.imageProtocol === 'kitty') {
      imagePrefix = `${hadUploadedKittyImages ? '\x1b_Ga=d,d=a,q=2\x1b\\' : deleteAllKittyImages()}${imagePrefix}`
    } else if (imagesNeedRedraw && this.imageProtocol === 'kitty') {
      imagePrefix = `\x1b_Ga=d,d=a,q=2\x1b\\${imagePrefix}`
    } else if (imagesNeedRedraw && this.imageProtocol === 'iterm2') {
      imagePrefix = `\x1b[2J${imagePrefix}`
    }
    if (imagePrefix) buffer = buffer.replace('\x1b[?2026h', `\x1b[?2026h${imagePrefix}`)
    this.terminal.write(buffer)
    this.previousScreen = composited
    this.fullRedrawCount = planner.fullRedraws
  }
}
Object.defineProperty(TuiAltScreen.prototype, VIEWPORT_TUI, { value: true })

const DEFAULT_LOADER_FRAMES = [
  '⠋', '⠙', '⠹', '⠸', '⠼',
  '⠴', '⠦', '⠧', '⠇', '⠏',
]
const loaderRegistry = new WeakMap()
class Loader extends Text {
  constructor(
    ui,
    spinnerColorFn,
    messageColorFn,
    message = 'Loading...',
    indicator,
  ) {
    super('', 1, 0)
    this.ui = ui
    this.spinnerColorFn = spinnerColorFn
    this.messageColorFn = messageColorFn
    this.message = message
    this.intervalId = undefined
    this.configure(indicator)
  }

  configure(indicator) {
    const custom = indicator !== undefined
    const sourceFrames = custom
      ? (indicator.frames ?? DEFAULT_LOADER_FRAMES)
      : DEFAULT_LOADER_FRAMES
    const frames = custom
      ? sourceFrames.slice()
      : sourceFrames.map((frame) => this.spinnerColorFn(frame))
    const intervalMs =
      Number.isFinite(indicator?.intervalMs) && indicator.intervalMs > 0
        ? Math.trunc(indicator.intervalMs)
        : 80
    const styledMessage = this.messageColorFn(this.message)
    const state = loaderRegistry.get(this)
    if (state) {
      state.setMessage(styledMessage)
      state.setIndicator(frames, intervalMs)
    } else {
      loaderRegistry.set(
        this,
        new native.NativeLoaderState(styledMessage, frames, intervalMs),
      )
    }
    this.restartAnimation()
  }

  restartAnimation() {
    if (this.intervalId !== undefined) clearInterval(this.intervalId)
    this.intervalId = undefined
    const state = loaderRegistry.get(this)
    if (!state?.running) return
    this.intervalId = setInterval(() => {
      state.advanceFrame()
      this.ui.requestRender()
    }, state.intervalMs)
  }

  render(width) {
    return loaderRegistry.get(this).render(assertColumn(width, 'width'))
  }

  invalidate() {
    loaderRegistry.get(this).invalidate()
  }

  start() {
    loaderRegistry.get(this).start()
    this.restartAnimation()
  }

  stop() {
    loaderRegistry.get(this).stop()
    if (this.intervalId !== undefined) clearInterval(this.intervalId)
    this.intervalId = undefined
  }

  setMessage(message) {
    this.message = assertWellFormed(message, 'message')
    loaderRegistry.get(this).setMessage(this.messageColorFn(this.message))
    this.ui.requestRender()
  }

  setIndicator(indicator) {
    this.configure(indicator)
    this.ui.requestRender()
  }
}

class CancellableLoader extends Loader {
  constructor(...args) {
    super(...args)
    this.abortController = new AbortController()
    this.onAbort = undefined
  }

  get signal() {
    return this.abortController.signal
  }

  get aborted() {
    return this.signal.aborted
  }

  handleInput(data) {
    if (matchesKey(data, 'escape') && !this.aborted) {
      this.abortController.abort()
      this.onAbort?.()
    }
  }

  dispose() {
    this.stop()
  }
}

const stdinBufferRegistry = new WeakMap()
class StdinBuffer extends EventEmitter {
  constructor(options = {}) {
    super()
    this.timeoutMs = options.timeout ?? 10
    this.timeout = null
    stdinBufferRegistry.set(
      this,
      new native.NativeStdinBufferState(
        assertColumn(this.timeoutMs, 'options.timeout'),
      ),
    )
  }

  emitEvents(events) {
    for (const event of events) this.emit(event.kind, event.value)
  }

  process(data) {
    if (this.timeout) clearTimeout(this.timeout)
    this.timeout = null
    let value
    if (Buffer.isBuffer(data)) {
      value =
        data.length === 1 && data[0] > 127
          ? `\x1b${String.fromCharCode(data[0] - 128)}`
          : data.toString()
    } else {
      value = assertWellFormed(data, 'data')
    }
    const state = stdinBufferRegistry.get(this)
    this.emitEvents(state.process(value))
    if (state.getBuffer().length > 0) {
      this.timeout = setTimeout(() => {
        for (const sequence of this.flush()) this.emit('data', sequence)
      }, this.timeoutMs)
    }
  }

  flush() {
    if (this.timeout) clearTimeout(this.timeout)
    this.timeout = null
    return stdinBufferRegistry.get(this).flush()
  }

  clear() {
    if (this.timeout) clearTimeout(this.timeout)
    this.timeout = null
    stdinBufferRegistry.get(this).clear()
  }

  getBuffer() {
    return stdinBufferRegistry.get(this).getBuffer()
  }

  destroy() {
    this.clear()
  }
}

class ProcessTerminal {
  constructor() {
    this.wasRaw = false
    this.inputHandler = undefined
    this.resizeHandler = undefined
    this.stdinBuffer = undefined
    this.stdinDataHandler = undefined
    this.progressInterval = undefined
    this._kittyProtocolActive = false
    this._modifyOtherKeysActive = false
  }
  get kittyProtocolActive() { return this._kittyProtocolActive }
  get modifyOtherKeysActive() { return this._modifyOtherKeysActive }
  start(onInput, onResize) {
    if (this.inputHandler) return
    this.inputHandler = onInput
    this.resizeHandler = onResize
    this.wasRaw = Boolean(process.stdin.isRaw)
    process.stdin.setEncoding('utf8')
    process.stdin.setRawMode?.(true)
    process.stdin.resume()
    this.write('\x1b[?2004h')
    this.stdinBuffer = new StdinBuffer({ timeout: 10 })
    this.stdinBuffer.on('data', onInput)
    this.stdinBuffer.on('paste', (value) => onInput(`\x1b[200~${value}\x1b[201~`))
    this.stdinDataHandler = (data) => this.stdinBuffer?.process(data)
    process.stdin.on('data', this.stdinDataHandler)
    process.stdout.on('resize', onResize)
    if (process.platform !== 'win32') {
      try { process.kill(process.pid, 'SIGWINCH') } catch {}
    }
    this.write('\x1b[>7u\x1b[?u\x1b[c')
  }
  async drainInput(maxMs = 1000, idleMs = 50) {
    this.write('\x1b[<u')
    this._kittyProtocolActive = false
    setKittyProtocolActive(false)
    await new Promise((resolve) => {
      const start = Date.now()
      let idle
      let maximum
      const finish = () => {
        clearTimeout(idle)
        clearTimeout(maximum)
        process.stdin.off('data', resetIdle)
        resolve()
      }
      const resetIdle = () => {
        clearTimeout(idle)
        idle = setTimeout(finish, idleMs)
      }
      process.stdin.on('data', resetIdle)
      resetIdle()
      maximum = setTimeout(finish, Math.max(0, maxMs - (Date.now() - start)))
    })
  }
  stop() {
    if (!this.inputHandler) return
    this.setProgress(false)
    this.write('\x1b[?2004l')
    if (this._kittyProtocolActive) this.write('\x1b[<u')
    if (this._modifyOtherKeysActive) this.write('\x1b[>4;0m')
    this.stdinBuffer?.destroy()
    if (this.stdinDataHandler) process.stdin.off('data', this.stdinDataHandler)
    if (this.resizeHandler) process.stdout.off('resize', this.resizeHandler)
    process.stdin.pause()
    process.stdin.setRawMode?.(this.wasRaw)
    this.inputHandler = undefined
    this.resizeHandler = undefined
    this.stdinBuffer = undefined
    this.stdinDataHandler = undefined
  }
  write(data) { process.stdout.write(data) }
  get columns() { return process.stdout.columns || Number(process.env.COLUMNS) || 80 }
  get rows() { return process.stdout.rows || Number(process.env.LINES) || 24 }
  moveBy(lines) {
    if (lines > 0) this.write(`\x1b[${lines}B`)
    else if (lines < 0) this.write(`\x1b[${-lines}A`)
  }
  hideCursor() { this.write('\x1b[?25l') }
  showCursor() { this.write('\x1b[?25h') }
  clearLine() { this.write('\x1b[2K\r') }
  clearFromCursor() { this.write('\x1b[0J') }
  clearScreen() { this.write('\x1b[2J\x1b[H') }
  setTitle(title) { this.write(`\x1b]0;${title}\x07`) }
  setProgress(active) {
    if (active) {
      this.write('\x1b]9;4;3\x07')
      if (!this.progressInterval) {
        this.progressInterval = setInterval(() => this.write('\x1b]9;4;3\x07'), 1000)
        this.progressInterval.unref?.()
      }
    } else {
      if (this.progressInterval) clearInterval(this.progressInterval)
      this.progressInterval = undefined
      this.write('\x1b]9;4;0\x07')
    }
  }
}

class Box extends Container {
  constructor(paddingX = 1, paddingY = 1, bgFn) {
    super()
    this.paddingX = paddingX
    this.paddingY = paddingY
    this.bgFn = bgFn
    this.cache = undefined
  }

  addChild(component) {
    super.addChild(component)
    this.cache = undefined
  }

  removeChild(component) {
    super.removeChild(component)
    this.cache = undefined
  }

  clear() {
    super.clear()
    this.cache = undefined
  }

  setBgFn(bgFn) {
    this.bgFn = bgFn
  }

  invalidate() {
    this.cache = undefined
    super.invalidate()
  }

  applyBg(line, width) {
    const padded = line + ' '.repeat(Math.max(0, width - visibleWidth(line)))
    return this.bgFn ? this.bgFn(padded) : padded
  }

  render(width) {
    if (this.children.length === 0) return []
    const contentWidth = Math.max(1, width - this.paddingX * 2)
    const leftPad = ' '.repeat(this.paddingX)
    const childLines = []
    for (const child of this.children) {
      for (const line of child.render(contentWidth)) childLines.push(leftPad + line)
    }
    if (childLines.length === 0) return []
    const bgSample = this.bgFn ? this.bgFn('test') : undefined
    if (
      this.cache?.width === width &&
      this.cache.bgSample === bgSample &&
      this.cache.childLines.length === childLines.length &&
      this.cache.childLines.every((line, index) => line === childLines[index])
    ) {
      return this.cache.lines
    }
    const result = []
    for (let index = 0; index < this.paddingY; index += 1) {
      result.push(this.applyBg('', width))
    }
    for (const line of childLines) result.push(this.applyBg(line, width))
    for (let index = 0; index < this.paddingY; index += 1) {
      result.push(this.applyBg('', width))
    }
    this.cache = { childLines, width, bgSample, lines: result }
    return result
  }
}

function normalizeStackSize(value, fallback) {
  return value === undefined || !Number.isFinite(value)
    ? fallback
    : Math.max(0, Math.floor(value))
}

class Stack extends Container {
  constructor(children = [], options = {}) {
    super()
    this.entries = []
    this.gap = normalizeStackSize(options.gap, 0)
    this.align = options.align ?? 'stretch'
    for (const child of children) {
      if (!('render' in child)) this.addChild(child.component, child)
      else this.addChild(child)
    }
  }

  addChild(component, options = {}) {
    super.addChild(component)
    this.entries.push({
      component,
      ...(options.basis === undefined ? {} : { basis: options.basis }),
      ...(options.grow === undefined
        ? {}
        : { grow: normalizeStackSize(options.grow, 0) }),
      ...(options.shrink === undefined
        ? {}
        : { shrink: normalizeStackSize(options.shrink, 1) }),
      ...(options.minSize === undefined
        ? {}
        : { minSize: normalizeStackSize(options.minSize, 0) }),
      ...(options.maxSize === undefined
        ? {}
        : { maxSize: normalizeStackSize(options.maxSize, 0xffffffff) }),
      ...(options.visible === undefined ? {} : { visible: options.visible }),
    })
  }

  removeChild(component) {
    super.removeChild(component)
    const index = this.entries.findIndex((entry) => entry.component === component)
    if (index !== -1) this.entries.splice(index, 1)
  }

  clear() {
    super.clear()
    this.entries.length = 0
  }

  visibleEntries(viewport) {
    return this.entries.filter((entry) => entry.visible?.(viewport) ?? true)
  }

  allocate(entries, intrinsicSizes, availableSize) {
    return native.nativeAllocateStackSizes(
      entries.map((entry) => ({
        ...(entry.basis === undefined || entry.basis === 'auto'
          ? {}
          : { basis: assertColumn(normalizeStackSize(entry.basis, 0), 'basis') }),
        ...(entry.grow === undefined ? {} : { grow: entry.grow }),
        ...(entry.shrink === undefined ? {} : { shrink: entry.shrink }),
        ...(entry.minSize === undefined ? {} : { minSize: entry.minSize }),
        ...(entry.maxSize === undefined ? {} : { maxSize: entry.maxSize }),
      })),
      intrinsicSizes.map((size) => assertColumn(size, 'intrinsicSize')),
      availableSize,
      assertColumn(this.gap, 'gap'),
    )
  }
}

class VStack extends Stack {
  render(width) {
    const viewport = { width: Math.max(1, width), height: Number.MAX_SAFE_INTEGER }
    const entries = this.visibleEntries(viewport)
    const rendered = entries.map((entry) => entry.component.render(viewport.width))
    const sizes = this.allocate(
      entries,
      rendered.map((lines) => lines.length),
      undefined,
    )
    const lines = []
    for (let index = 0; index < entries.length; index += 1) {
      if (index > 0) {
        for (let gap = 0; gap < this.gap; gap += 1) lines.push('')
      }
      const childLines = rendered[index].slice(0, sizes[index])
      lines.push(...childLines)
      for (let padding = childLines.length; padding < sizes[index]; padding += 1) {
        lines.push('')
      }
    }
    return lines
  }
}

class HStack extends Stack {
  render(width) {
    const safeWidth = Math.max(1, width)
    const viewport = { width: safeWidth, height: Number.MAX_SAFE_INTEGER }
    const entries = this.visibleEntries(viewport)
    if (entries.length === 0) return []
    const intrinsicWidths = entries.map((entry) =>
      entry.component
        .render(safeWidth)
        .reduce((maximum, line) => Math.max(maximum, visibleWidth(line)), 0),
    )
    const widths = this.allocate(entries, intrinsicWidths, safeWidth)
    const rendered = entries.map((entry, index) =>
      widths[index] === 0 ? [] : entry.component.render(widths[index]),
    )
    const height = rendered.reduce(
      (maximum, lines) => Math.max(maximum, lines.length),
      0,
    )
    const result = Array.from({ length: height }, () => '')
    let x = 0
    for (let index = 0; index < rendered.length; index += 1) {
      const lines = rendered[index]
      const childWidth = widths[index]
      let offset = 0
      if (this.align === 'center') offset = Math.floor((height - lines.length) / 2)
      else if (this.align === 'end') offset = height - lines.length
      for (let row = 0; row < lines.length; row += 1) {
        const target = row + offset
        if (target < 0 || target >= result.length) continue
        result[target] = compositeTuiLine(
          result[target],
          lines[row],
          x,
          childWidth,
          safeWidth,
        )
      }
      x += childWidth + this.gap
    }
    return result
  }
}

function getCellDimensions() {
  return cellDimensions
}

function setCellDimensions(dimensions) {
  cellDimensions = dimensions
}

function snapshotTerminalEnvironment() {
  return {
    termProgram: process.env.TERM_PROGRAM,
    terminalEmulator: process.env.TERMINAL_EMULATOR,
    term: process.env.TERM,
    colorTerm: process.env.COLORTERM,
    tmux: process.env.TMUX,
    kittyWindowId: process.env.KITTY_WINDOW_ID,
    ghosttyResourcesDir: process.env.GHOSTTY_RESOURCES_DIR,
    weztermPane: process.env.WEZTERM_PANE,
    warpSessionId: process.env.WARP_SESSION_ID,
    warpTerminalSessionUuid: process.env.WARP_TERMINAL_SESSION_UUID,
    itermSessionId: process.env.ITERM_SESSION_ID,
    wtSession: process.env.WT_SESSION,
  }
}

function detectCapabilities(tmuxForwardsHyperlink = probeTmuxHyperlinks) {
  const environment = snapshotTerminalEnvironment()
  const term = (environment.term || '').toLowerCase()
  const inTmux = Boolean(environment.tmux) || term.startsWith('tmux')
  const tmuxForwards = inTmux ? tmuxForwardsHyperlink() : false
  return native.nativeDetectCapabilities(
    environment,
    Boolean(tmuxForwards),
  )
}

function getCapabilities() {
  if (!cachedCapabilities) {
    cachedCapabilities = detectCapabilities()
  }
  return cachedCapabilities
}

function resetCapabilitiesCache() {
  cachedCapabilities = null
}

function setCapabilities(capabilities) {
  cachedCapabilities = capabilities
}

function renderLatex(source, options = {}) {
  const result = native.nativeRenderLatex(source, {
    display: options.display,
  })
  return fromNullable(result)
}

function fuzzyMatch(query, text) {
  return native.nativeFuzzyMatchLowerUtf16(
    query.toLowerCase(),
    text.toLowerCase(),
  )
}

function fuzzyFilter(items, query, getText) {
  if (!query.trim()) return items
  const tokens = query
    .trim()
    .split(/[\s/]+/)
    .filter((token) => token.length > 0)
  if (tokens.length === 0) return items

  const results = []
  for (const item of items) {
    const text = getText(item)
    let totalScore = 0
    let allMatch = true
    for (const token of tokens) {
      const match = fuzzyMatch(token, text)
      if (match.matches) {
        totalScore += match.score
      } else {
        allMatch = false
        break
      }
    }
    if (allMatch) results.push({ item, totalScore })
  }
  results.sort((left, right) => left.totalScore - right.totalScore)
  return results.map((result) => result.item)
}

class CombinedAutocompleteProvider {
  constructor(commands = [], basePath, fdPath = null) {
    this.commands = commands
    this.basePath = basePath
    this.fdPath = fdPath
    this.triggerCharacters = ['@', '/', '.', '~']
  }

  async getSuggestions(lines, cursorLine, cursorCol, options) {
    const current = lines[cursorLine] || ''
    const before = current.slice(0, cursorCol)
    if (!options.force && before.startsWith('/')) {
      const space = before.indexOf(' ')
      if (space === -1) {
        const prefix = before.slice(1)
        const items = fuzzyFilter(
          this.commands.map((command) => {
            const name = 'name' in command ? command.name : command.value
            const hint = command.argumentHint
            const description = command.description ?? ''
            return {
              value: name,
              label: name,
              ...(hint || description
                ? { description: hint && description ? `${hint} — ${description}` : hint || description }
                : {}),
            }
          }),
          prefix,
          (item) => item.value,
        )
        return items.length ? { items, prefix: before } : null
      }
      const name = before.slice(1, space)
      const argument = before.slice(space + 1)
      const command = this.commands.find(
        (candidate) => ('name' in candidate ? candidate.name : candidate.value) === name,
      )
      if (command?.getArgumentCompletions) {
        const items = await command.getArgumentCompletions(argument)
        return items?.length ? { items, prefix: argument } : null
      }
      return null
    }
    const match = /(?:^|[\s='"])(@?[^\s='"\n]*)$/.exec(before)
    if (!match || (!options.force && !match[1].startsWith('@'))) return null
    const prefix = match[1]
    const attachment = prefix.startsWith('@')
    const raw = attachment ? prefix.slice(1) : prefix
    const expanded = raw.startsWith('~/') ? join(homedir(), raw.slice(2)) : raw
    const directoryPart = expanded.includes('/') ? dirname(expanded) : '.'
    const query = expanded.includes('/') ? expanded.slice(expanded.lastIndexOf('/') + 1) : expanded
    const base = isAbsolute(directoryPart)
      ? directoryPart
      : join(this.basePath, directoryPart)
    let entries
    try { entries = readdirSync(base) } catch { return null }
    const items = fuzzyFilter(entries, query, (entry) => entry).slice(0, 100).map((entry) => {
      const full = join(base, entry)
      let directory = false
      try { directory = statSync(full).isDirectory() } catch {}
      const relativePrefix = raw.includes('/') ? raw.slice(0, raw.lastIndexOf('/') + 1) : ''
      const value = `${attachment ? '@' : ''}${relativePrefix}${entry}${directory ? '/' : ''}`
      return { value, label: `${entry}${directory ? '/' : ''}` }
    })
    return items.length ? { items, prefix } : null
  }

  applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
    const current = lines[cursorLine] || ''
    const before = current.slice(0, cursorCol - prefix.length)
    const after = current.slice(cursorCol)
    const slash = prefix.startsWith('/') && before.trim() === '' && !prefix.slice(1).includes('/')
    const suffix = slash || (prefix.startsWith('@') && !item.label.endsWith('/')) ? ' ' : ''
    const value = slash ? `/${item.value}` : item.value
    const updated = [...lines]
    updated[cursorLine] = `${before}${value}${suffix}${after}`
    return {
      lines: updated,
      cursorLine,
      cursorCol: before.length + value.length + suffix.length,
    }
  }

  shouldTriggerFileCompletion(lines, cursorLine, cursorCol) {
    const before = (lines[cursorLine] || '').slice(0, cursorCol)
    return /(?:^|\s)@[^\s]*$/.test(before)
  }
}

function parseOsc11BackgroundColor(data) {
  return fromNullable(
    native.nativeParseOsc11BackgroundColor(assertWellFormed(data, 'data')),
  )
}

function parseTerminalColorSchemeReport(data) {
  return fromNullable(
    native.nativeParseTerminalColorSchemeReport(
      assertWellFormed(data, 'data'),
    ),
  )
}

function allocateImageId() {
  return native.nativeImageIdFromRandom(Math.random())
}

function calculateImageCellSize(
  imageDimensions,
  maxWidthCells,
  maxHeightCells,
  dimensions,
) {
  const maxWidth = Math.max(1, Math.floor(maxWidthCells))
  const maxHeight =
    maxHeightCells === undefined
      ? undefined
      : Math.max(1, Math.floor(maxHeightCells))
  const imageWidth = Math.max(1, imageDimensions.widthPx)
  const imageHeight = Math.max(1, imageDimensions.heightPx)
  const widthScale = (maxWidth * dimensions.widthPx) / imageWidth
  const heightScale =
    maxHeight === undefined
      ? widthScale
      : (maxHeight * dimensions.heightPx) / imageHeight
  const scale = Math.min(widthScale, heightScale)
  const scaledWidthPx = imageWidth * scale
  const scaledHeightPx = imageHeight * scale
  const columns = Math.ceil(scaledWidthPx / dimensions.widthPx)
  const rows = Math.ceil(scaledHeightPx / dimensions.heightPx)
  return {
    columns: Math.max(1, Math.min(maxWidth, columns)),
    rows: Math.max(
      1,
      maxHeight === undefined ? rows : Math.min(maxHeight, rows),
    ),
  }
}

function calculateImageRows(
  imageDimensions,
  targetWidthCells,
  dimensions = { widthPx: 9, heightPx: 18 },
) {
  return calculateImageCellSize(
    imageDimensions,
    targetWidthCells,
    undefined,
    dimensions,
  ).rows
}

function encodeKitty(base64Data, options = {}) {
  if (typeof base64Data !== 'string') {
    throw new TypeError('base64Data must be a string')
  }
  const nativeOptions = {
    moveCursor: options.moveCursor,
  }
  if (options.columns !== undefined) {
    nativeOptions.columns = assertColumn(options.columns, 'options.columns')
  }
  if (options.rows !== undefined) {
    nativeOptions.rows = assertColumn(options.rows, 'options.rows')
  }
  if (options.imageId !== undefined) {
    nativeOptions.imageId = assertImageId(options.imageId)
  }
  return native.nativeEncodeKittyUtf16(base64Data, nativeOptions)
}

function encodeITerm2(base64Data, options = {}) {
  if (typeof base64Data !== 'string') {
    throw new TypeError('base64Data must be a string')
  }
  const nativeOptions = {
    preserveAspectRatio: options.preserveAspectRatio,
    inline: options.inline,
  }
  if (options.width !== undefined) {
    nativeOptions.width = assertWellFormed(
      String(options.width),
      'options.width',
    )
  }
  if (options.height !== undefined) {
    nativeOptions.height = assertWellFormed(
      String(options.height),
      'options.height',
    )
  }
  if (options.name !== undefined) {
    nativeOptions.name = assertWellFormed(options.name, 'options.name')
  }
  return native.nativeEncodeITerm2Utf16(
    base64Data,
    Buffer.byteLength(base64Data, 'base64'),
    nativeOptions,
  )
}

function encodeKittyUnchecked(base64Data, options = {}) {
  const chunkSize = 4096
  const params = ['a=T', 'f=100', 'q=2']
  if (options.moveCursor === false) params.push('C=1')
  if (options.columns) params.push(`c=${options.columns}`)
  if (options.rows) params.push(`r=${options.rows}`)
  if (options.imageId) params.push(`i=${options.imageId}`)
  if (base64Data.length <= chunkSize) {
    return `\x1b_G${params.join(',')};${base64Data}\x1b\\`
  }
  const chunks = []
  for (let offset = 0; offset < base64Data.length; offset += chunkSize) {
    const chunk = base64Data.slice(offset, offset + chunkSize)
    const isLast = offset + chunkSize >= base64Data.length
    if (offset === 0) {
      chunks.push(`\x1b_G${params.join(',')},m=1;${chunk}\x1b\\`)
    } else if (isLast) {
      chunks.push(`\x1b_Gm=0;${chunk}\x1b\\`)
    } else {
      chunks.push(`\x1b_Gm=1;${chunk}\x1b\\`)
    }
  }
  return chunks.join('')
}

function encodeITerm2Unchecked(base64Data, options = {}) {
  const params = [
    `inline=${options.inline !== false ? 1 : 0}`,
    `size=${Buffer.byteLength(base64Data, 'base64')}`,
  ]
  if (options.width !== undefined) params.push(`width=${options.width}`)
  if (options.height !== undefined) params.push(`height=${options.height}`)
  if (options.name) {
    params.push(`name=${Buffer.from(options.name).toString('base64')}`)
  }
  if (options.preserveAspectRatio === false) {
    params.push('preserveAspectRatio=0')
  }
  return `\x1b]1337;File=${params.join(';')}:${base64Data}\x07`
}

const kittyImageMetadata = new Map()
let kittyTransmissionGeneration = 0

function registerKittyImageMetadata(metadata) {
  kittyTransmissionGeneration += 1
  kittyImageMetadata.delete(metadata.imageId)
  kittyImageMetadata.set(metadata.imageId, {
    ...metadata,
    transmissionGeneration: kittyTransmissionGeneration,
  })
  if (kittyImageMetadata.size > 1000) {
    const oldestImageId = kittyImageMetadata.keys().next().value
    if (oldestImageId !== undefined) kittyImageMetadata.delete(oldestImageId)
  }
}

function renderImage(base64Data, imageDimensions, options = {}) {
  const capabilities = getCapabilities()
  if (!capabilities.images) return null
  const maxWidth = options.maxWidthCells ?? 80
  const size = calculateImageCellSize(
    imageDimensions,
    maxWidth,
    options.maxHeightCells,
    getCellDimensions(),
  )
  if (capabilities.images === 'kitty') {
    if (options.imageId !== undefined) {
      registerKittyImageMetadata({
        imageId: options.imageId,
        columns: size.columns,
        rows: size.rows,
        widthPx: imageDimensions.widthPx,
        heightPx: imageDimensions.heightPx,
      })
    }
    const sequence = encodeKittyUnchecked(base64Data, {
      columns: size.columns,
      rows: size.rows,
      imageId: options.imageId,
      moveCursor: options.moveCursor,
    })
    return {
      sequence,
      columns: size.columns,
      rows: size.rows,
      imageId: options.imageId,
    }
  }
  if (capabilities.images === 'iterm2') {
    const sequence = encodeITerm2Unchecked(base64Data, {
      width: size.columns,
      height: 'auto',
      preserveAspectRatio: options.preserveAspectRatio ?? true,
    })
    return { sequence, columns: size.columns, rows: size.rows }
  }
  return null
}

function deleteKittyImage(imageId) {
  return native.nativeDeleteKittyImage(assertImageId(imageId))
}

function deleteAllKittyImages() {
  return native.nativeDeleteAllKittyImages()
}

function getPngDimensions(base64Data) {
  return native.nativeGetPngDimensions(assertWellFormed(base64Data, 'base64Data'))
}

function getJpegDimensions(base64Data) {
  return native.nativeGetJpegDimensions(assertWellFormed(base64Data, 'base64Data'))
}

function getGifDimensions(base64Data) {
  return native.nativeGetGifDimensions(assertWellFormed(base64Data, 'base64Data'))
}

function getWebpDimensions(base64Data) {
  return native.nativeGetWebpDimensions(assertWellFormed(base64Data, 'base64Data'))
}

function getImageDimensions(base64Data, mimeType) {
  return native.nativeGetImageDimensions(
    assertWellFormed(base64Data, 'base64Data'),
    assertWellFormed(mimeType, 'mimeType'),
  )
}

function hyperlink(text, url) {
  return native.nativeHyperlink(
    assertWellFormed(text, 'text'),
    assertWellFormed(url, 'url'),
  )
}

function hyperlinkUnchecked(text, url) {
  return `\x1b]8;;${url}\x1b\\${text}\x1b]8;;\x1b\\`
}

function shortenImagePath(filename) {
  const home = homedir()
  if (
    home &&
    (filename === home ||
      filename.startsWith(`${home}/`) ||
      filename.startsWith(`${home}\\`))
  ) {
    return `~${filename.slice(home.length)}`
  }
  return filename
}

function imageFallback(mimeType, dimensions, filename) {
  const parts = []
  if (filename) {
    const display = shortenImagePath(filename)
    if (getCapabilities().hyperlinks && isAbsolute(filename)) {
      parts.push(hyperlinkUnchecked(display, pathToFileURL(filename).href))
    } else {
      parts.push(display)
    }
  }
  parts.push(`[${mimeType}]`)
  if (dimensions) parts.push(`${dimensions.widthPx}x${dimensions.heightPx}`)
  return `[Image: ${parts.join(' ')}]`
}

function setKittyProtocolActive(active) {
  native.nativeSetKittyProtocolActive(active)
}

function isKittyProtocolActive() {
  return native.nativeIsKittyProtocolActive()
}

function isKeyRelease(data) {
  return native.nativeIsKeyRelease(assertWellFormed(data, 'data'))
}

function isKeyRepeat(data) {
  return native.nativeIsKeyRepeat(assertWellFormed(data, 'data'))
}

function matchesKey(data, keyId) {
  return native.nativeMatchesKey(
    assertWellFormed(data, 'data'),
    assertWellFormed(keyId, 'keyId'),
  )
}

function parseKey(data) {
  return fromNullable(native.nativeParseKey(assertWellFormed(data, 'data')))
}

function decodeKittyPrintable(data) {
  return fromNullable(
    native.nativeDecodeKittyPrintable(assertWellFormed(data, 'data')),
  )
}

function visibleWidth(value) {
  return native.nativeVisibleWidth(assertWellFormed(value, 'value'))
}

function stripTerminalSequences(value) {
  return native.nativeStripTerminalSequences(assertWellFormed(value, 'value'))
}

function getOsc8LinkAtColumn(line, column) {
  return fromNullable(
    native.nativeGetOsc8LinkAtColumn(
      assertWellFormed(line, 'line'),
      assertColumn(column, 'column'),
    ),
  )
}

function sliceByColumn(line, startCol, length, strict = false) {
  return native.nativeSliceByColumn(
    assertWellFormed(line, 'line'),
    assertColumn(startCol, 'startCol'),
    assertColumn(length, 'length'),
    strict,
  )
}

function truncateToWidth(text, maxWidth, ellipsis = '...', pad = false) {
  const checkedMaxWidth = assertColumn(maxWidth, 'maxWidth')
  if (pad && checkedMaxWidth > MAX_PAD_WIDTH) {
    throw new RangeError('padded output exceeds the JavaScript string limit')
  }
  return native.nativeTruncateToWidth(
    assertWellFormed(text, 'text'),
    checkedMaxWidth,
    assertWellFormed(ellipsis, 'ellipsis'),
    pad,
  )
}

function wrapTextWithAnsi(text, width) {
  return native.nativeWrapTextWithAnsi(
    assertWellFormed(text, 'text'),
    assertColumn(width, 'width'),
  )
}

function isImageLine(line) {
  return (
    line.startsWith('\x1b_G') ||
    line.startsWith('\x1b]1337;File=') ||
    line.includes('\x1b_G') ||
    line.includes('\x1b]1337;File=')
  )
}

function compositeTuiLine(
  baseLine,
  overlayLine,
  startCol,
  overlayWidth,
  totalWidth,
) {
  if (isImageLine(baseLine)) return baseLine
  const { encoded, decode, tokenPrefix } = encodeRawUtf16Strings(
    baseLine,
    overlayLine,
  )
  const [encodedBase, encodedOverlay] = encoded
  const afterStart = startCol + overlayWidth
  const afterLength = totalWidth - afterStart
  const base = native.nativeExtractCompositeSegments(
    encodedBase,
    Number(startCol),
    Number(afterStart),
    Number(afterLength),
    tokenPrefix,
  )
  const overlay = native.nativeSliceComposite(
    encodedOverlay,
    Number(overlayWidth),
    tokenPrefix,
  )
  const overlayVisibleWidth = native.nativeCompositeVisibleWidth(
    overlay,
    tokenPrefix,
  )
  const beforePad = Math.max(0, startCol - base.beforeWidth)
  const overlayPad = Math.max(0, overlayWidth - overlayVisibleWidth)
  const actualBeforeWidth = Math.max(startCol, base.beforeWidth)
  const actualOverlayWidth = Math.max(overlayWidth, overlayVisibleWidth)
  const afterTarget = Math.max(
    0,
    totalWidth - actualBeforeWidth - actualOverlayWidth,
  )
  const afterPad = Math.max(0, afterTarget - base.afterWidth)
  const result =
    base.before +
    ' '.repeat(beforePad) +
    SEGMENT_RESET +
    overlay +
    ' '.repeat(overlayPad) +
    SEGMENT_RESET +
    base.after +
    ' '.repeat(afterPad)
  const decodedResult = decode(result)
  const finalResult =
    visibleWidthRawUtf16(decodedResult) <= totalWidth
      ? result
      : native.nativeSliceComposite(result, Number(totalWidth), tokenPrefix)
  return finalResult === result ? decodedResult : decode(finalResult)
}

const Key = {
  escape: 'escape',
  esc: 'esc',
  enter: 'enter',
  return: 'return',
  tab: 'tab',
  space: 'space',
  backspace: 'backspace',
  delete: 'delete',
  insert: 'insert',
  clear: 'clear',
  home: 'home',
  end: 'end',
  pageUp: 'pageUp',
  pageDown: 'pageDown',
  up: 'up',
  down: 'down',
  left: 'left',
  right: 'right',
  f1: 'f1',
  f2: 'f2',
  f3: 'f3',
  f4: 'f4',
  f5: 'f5',
  f6: 'f6',
  f7: 'f7',
  f8: 'f8',
  f9: 'f9',
  f10: 'f10',
  f11: 'f11',
  f12: 'f12',
  backtick: '`',
  hyphen: '-',
  equals: '=',
  leftbracket: '[',
  rightbracket: ']',
  backslash: '\\',
  semicolon: ';',
  quote: "'",
  comma: ',',
  period: '.',
  slash: '/',
  exclamation: '!',
  at: '@',
  hash: '#',
  dollar: '$',
  percent: '%',
  caret: '^',
  ampersand: '&',
  asterisk: '*',
  leftparen: '(',
  rightparen: ')',
  underscore: '_',
  plus: '+',
  pipe: '|',
  tilde: '~',
  leftbrace: '{',
  rightbrace: '}',
  colon: ':',
  lessthan: '<',
  greaterthan: '>',
  question: '?',
  ctrl: (key) => `ctrl+${key}`,
  shift: (key) => `shift+${key}`,
  alt: (key) => `alt+${key}`,
  super: (key) => `super+${key}`,
  ctrlShift: (key) => `ctrl+shift+${key}`,
  shiftCtrl: (key) => `shift+ctrl+${key}`,
  ctrlAlt: (key) => `ctrl+alt+${key}`,
  altCtrl: (key) => `alt+ctrl+${key}`,
  shiftAlt: (key) => `shift+alt+${key}`,
  altShift: (key) => `alt+shift+${key}`,
  ctrlSuper: (key) => `ctrl+super+${key}`,
  superCtrl: (key) => `super+ctrl+${key}`,
  shiftSuper: (key) => `shift+super+${key}`,
  superShift: (key) => `super+shift+${key}`,
  altSuper: (key) => `alt+super+${key}`,
  superAlt: (key) => `super+alt+${key}`,
  ctrlShiftAlt: (key) => `ctrl+shift+alt+${key}`,
  ctrlShiftSuper: (key) => `ctrl+shift+super+${key}`,
}

const selectedExports = {
  Box,
  CURSOR_MARKER,
  CancellableLoader,
  CombinedAutocompleteProvider,
  Container,
  Editor,
  HStack,
  Image,
  Input,
  Key,
  KeybindingsManager,
  Loader,
  Markdown,
  Marked,
  ProcessTerminal,
  ScrollView,
  SelectList,
  SettingsList,
  Spacer,
  StdinBuffer,
  TUI_KEYBINDINGS,
  Text,
  TruncatedText,
  TuiAltScreen,
  TuiMainScreen,
  VStack,
  allocateImageId,
  calculateImageRows,
  compositeTuiLine,
  decodeKittyPrintable,
  deleteAllKittyImages,
  deleteKittyImage,
  detectCapabilities,
  encodeITerm2,
  encodeKitty,
  fuzzyFilter,
  fuzzyMatch,
  getCapabilities,
  getCellDimensions,
  getGifDimensions,
  getImageDimensions,
  getJpegDimensions,
  getKeybindings,
  getOsc8LinkAtColumn,
  getPngDimensions,
  getWebpDimensions,
  hyperlink,
  imageFallback,
  isFocusable,
  isKeyRelease,
  isKeyRepeat,
  isKittyProtocolActive,
  isViewportTUI,
  matchesKey,
  parseKey,
  parseOsc11BackgroundColor,
  parseTerminalColorSchemeReport,
  renderImage,
  renderLatex,
  resetCapabilitiesCache,
  setCapabilities,
  setCellDimensions,
  setKeybindings,
  setKittyProtocolActive,
  sliceByColumn,
  stripTerminalSequences,
  truncateToWidth,
  visibleWidth,
  wrapTextWithAnsi,
}

const runtimeTarget = Object.create(null)
for (const [name, value] of Object.entries(selectedExports)) {
  Object.defineProperty(runtimeTarget, name, {
    value,
    writable: true,
    enumerable: true,
    configurable: false,
  })
}
Object.defineProperty(runtimeTarget, Symbol.toStringTag, {
  value: 'Module',
  writable: false,
  enumerable: false,
  configurable: false,
})
Object.preventExtensions(runtimeTarget)
const runtime = new Proxy(runtimeTarget, {
  set: () => false,
  defineProperty: () => false,
  deleteProperty: () => false,
})

module.exports = runtime
