#!/usr/bin/env node
// Build the exhaustive checked-in disposition ledger from the canonical barrel.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const api = JSON.parse(readFileSync(join(root, "tools/api-surface.json"), "utf8"));
const napiPackage = JSON.parse(readFileSync(join(root, "adapters/pie-napi/package.json"), "utf8"));
const napiOracle = JSON.parse(
  readFileSync(join(root, "adapters/pie-napi/test/oracle-contract.json"), "utf8"),
);

const compiled = new Set([
  "AutocompleteItem", "AutocompleteProvider", "AutocompleteSuggestions",
  "CombinedAutocompleteProvider", "SlashCommand", "Box", "CancellableLoader", "HStack",
  "Editor", "EditorOptions", "EditorTheme", "Image", "ImageOptions", "ImageTheme", "Input",
  "Loader", "LoaderIndicatorOptions",
  "DefaultTextStyle", "Markdown", "MarkdownOptions", "MarkdownTheme", "SelectItem", "SelectList",
  "ScrollView", "ScrollViewOptions", "ScrollViewScrollbar", "ScrollViewScrollToOptions",
  "SelectListLayoutOptions", "SelectListTheme", "SelectListTruncatePrimaryContext", "SettingItem",
  "SettingsList", "SettingsListTheme", "Spacer", "Text", "TruncatedText", "VStack",
  "EditorComponent", "FuzzyMatch",
  "fuzzyFilter", "fuzzyMatch", "getKeybindings", "Keybinding", "KeybindingConflict",
  "KeybindingDefinition", "KeybindingDefinitions", "Keybindings", "KeybindingsConfig",
  "KeybindingsManager", "setKeybindings", "TUI_KEYBINDINGS", "decodeKittyPrintable",
  "isKeyRelease", "isKeyRepeat", "isKittyProtocolActive", "KeyEventType", "KeyId",
  "matchesKey", "parseKey", "setKittyProtocolActive", "RenderLatexOptions", "renderLatex",
  "StdinBuffer", "StdinBufferEventMap", "StdinBufferOptions", "ProcessTerminal", "Terminal",
  "parseOsc11BackgroundColor", "parseTerminalColorSchemeReport", "RgbColor", "TerminalColorScheme",
  "CellDimensions", "calculateImageRows", "deleteAllKittyImages", "deleteKittyImage",
  "detectCapabilities", "encodeITerm2", "encodeKitty", "getCapabilities", "getGifDimensions",
  "getImageDimensions", "getJpegDimensions", "getPngDimensions", "getWebpDimensions", "hyperlink",
  "ImageDimensions", "ImageProtocol", "ImageRenderOptions", "imageFallback", "renderImage",
  "resetCapabilitiesCache", "setCapabilities", "TerminalCapabilities", "Component", "Container",
  "CURSOR_MARKER", "compositeTuiLine", "SizeValue", "TuiMainScreen",
  "TuiMainScreenRenderState", "getOsc8LinkAtColumn",
  "sliceByColumn", "stripTerminalSequences", "truncateToWidth", "visibleWidth",
  "wrapTextWithAnsi",
]);

const partial = new Set([
  "CombinedAutocompleteProvider", "CancellableLoader", "getKeybindings", "KeybindingsManager",
  "Editor", "Image", "Input", "Loader", "Markdown", "ProcessTerminal", "ScrollView",
  "SettingsList", "StackChild", "StackEntry",
  "StackEntryOptions", "StackOptions", "EditorComponent", "setKeybindings", "Key", "Container",
  "allocateImageId", "calculateImageRows", "getCellDimensions", "imageFallback", "renderImage",
  "setCellDimensions", "OverlayAnchor", "OverlayHandle", "OverlayMargin", "OverlayOptions",
  "OverlayUnfocusOptions", "TUI", "TuiInputListener", "TuiInputListenerResult", "TuiMode",
  "TuiStopOptions", "ViewportTUI", "TuiAltScreen", "TuiAltScreenOptions",
]);

const verified = new Set([
  "SlashCommand", "SelectList", "TruncatedText", "FuzzyMatch", "fuzzyFilter", "fuzzyMatch",
  "isKeyRelease", "isKeyRepeat", "matchesKey", "parseKey", "CURSOR_MARKER", "getOsc8LinkAtColumn",
  "sliceByColumn",
  "stripTerminalSequences", "truncateToWidth", "visibleWidth", "wrapTextWithAnsi",
  "RenderLatexOptions", "renderLatex",
  "ScrollViewScrollbar", "parseOsc11BackgroundColor", "parseTerminalColorSchemeReport",
  "RgbColor", "TerminalColorScheme", "CellDimensions", "deleteAllKittyImages",
  "deleteKittyImage", "detectCapabilities", "encodeITerm2", "encodeKitty", "getGifDimensions",
  "getImageDimensions", "getJpegDimensions", "getPngDimensions", "getWebpDimensions", "hyperlink",
  "ImageDimensions", "ImageProtocol", "ImageRenderOptions", "TerminalCapabilities",
]);

const evidenceByStatement = {
  "S02-autocomplete": ["crates/pie-components/src/autocomplete.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S03-box": ["crates/pie-components/src/box_component.rs", "crates/pie-components/tests/components_golden.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S04-cancellable-loader": ["crates/pie-components/src/cancellable_loader.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S05-editor": ["crates/pie-components/src/editor.rs", "crates/pie-components/tests/editor_input_golden.rs", "crates/pie-components/tests/editor_autocomplete.rs"],
  "S06-h-stack": ["crates/pie-components/src/vstack_hstack.rs", "crates/pie-components/tests/components_golden.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S07-image": ["crates/pie-components/src/image.rs", "crates/pie-components/tests/golden_image_component.rs"],
  "S08-input": ["crates/pie-components/src/input.rs", "crates/pie-components/tests/input_golden.rs"],
  "S09-loader": ["crates/pie-components/src/loader.rs", "crates/pie-components/tests/components_golden.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S10-markdown": ["crates/pie-components/src/markdown.rs", "crates/pie-components/tests/golden_m4_markdown.rs", "crates/pie-components/tests/golden_m4_markdown_packet.rs"],
  "S11-scroll-view": ["crates/pie-components/src/layout.rs", "crates/pie-components/src/scroll_view.rs", "crates/pie-components/tests/golden_layout_scroll.rs"],
  "S12-select-list": ["crates/pie-components/src/select_list.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S13-settings-list": ["crates/pie-components/src/settings_list.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S14-spacer": ["crates/pie-components/src/spacer.rs", "crates/pie-components/tests/components_golden.rs"],
  "S15-text": ["crates/pie-components/src/text.rs", "crates/pie-components/tests/components_golden.rs"],
  "S16-truncated-text": ["crates/pie-components/src/truncated_text.rs", "crates/pie-components/tests/components_golden.rs"],
  "S17-v-stack": ["crates/pie-components/src/vstack_hstack.rs", "crates/pie-components/src/stack.rs", "crates/pie-components/tests/golden_m3_components.rs"],
  "S18-editor-component": ["crates/pie-components/src/editor_component.rs", "crates/pie-components/tests/editor_model_surface.rs"],
  "S19-fuzzy": ["crates/pie-core/src/fuzzy.rs", "crates/pie-core/tests/golden_m3_core.rs"],
  "S20-keybindings": ["crates/pie-core/src/keybindings.rs", "crates/pie-core/tests/golden_m3_core.rs"],
  "S21-keys": ["crates/pie-core/src/keys.rs", "crates/pie-core/tests/golden_keys.rs"],
  "S22-latex": ["crates/pie-core/src/latex.rs", "crates/pie-core/tests/golden_m4_latex_utf16.rs"],
  "S23-stdin-buffer": ["crates/pie-core/src/stdin_buffer.rs", "crates/pie-core/tests/golden_stdin.rs"],
  "S24-terminal": ["crates/pie-term/src/lib.rs", "crates/pie-term/src/process_terminal.rs", "crates/pie-term/tests/golden_process_terminal.rs"],
  "S25-terminal-colors": ["crates/pie-core/src/terminal_colors.rs", "crates/pie-core/tests/golden_terminal_primitives.rs"],
  "S26-terminal-image": ["crates/pie-core/src/terminal_image.rs", "crates/pie-term/src/capabilities.rs", "crates/pie-core/tests/golden_terminal_primitives.rs", "crates/pie-term/tests/golden_terminal_capabilities.rs"],
  "S27-tui": [
    "crates/pie-components/src/lib.rs",
    "crates/pie-components/src/container.rs",
    "crates/pie-components/src/size_value.rs",
    "crates/pie-components/src/tui.rs",
    "crates/pie-components/tests/golden_m3_components.rs",
    "crates/pie-components/tests/golden_layout_scroll.rs",
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/src/lib.rs",
    "crates/pie-app/src/tui_controller.rs",
    "crates/pie-app/tests/tui_controller.rs",
    "crates/pie-core/src/screen.rs",
  ],
  "S28-tui-alt-screen": [
    "crates/pie-app/src/tui_alt_screen.rs",
    "crates/pie-app/tests/fixtures/main-alt-controller.json",
    "crates/pie-app/tests/main_alt_controller.rs",
  ],
  "S29-tui-main-screen": [
    "crates/pie-term/src/renderer.rs",
    "crates/pie-term/tests/golden_render.rs",
    "crates/pie-app/src/tui_main_screen.rs",
    "crates/pie-app/tests/fixtures/main-alt-controller.json",
    "crates/pie-app/tests/main_alt_controller.rs",
  ],
  "S30-utils": ["crates/pie-core/src/text.rs", "crates/pie-core/src/wrap.rs", "crates/pie-core/tests/golden_text.rs", "crates/pie-core/tests/golden_wrap.rs"],
};

const milestoneByStatement = {
  "S01-marked": "external", "S02-autocomplete": "M3", "S03-box": "M3",
  "S04-cancellable-loader": "M3", "S05-editor": "M4", "S06-h-stack": "M3",
  "S07-image": "M5", "S08-input": "M4", "S09-loader": "M3", "S10-markdown": "M4",
  "S11-scroll-view": "M5", "S12-select-list": "M3", "S13-settings-list": "M3",
  "S14-spacer": "M3", "S15-text": "M3", "S16-truncated-text": "M3",
  "S17-v-stack": "M3", "S18-editor-component": "M4", "S19-fuzzy": "M3",
  "S20-keybindings": "M3", "S21-keys": "M1", "S22-latex": "M4",
  "S23-stdin-buffer": "M1", "S24-terminal": "M2", "S25-terminal-colors": "M5",
  "S26-terminal-image": "M5", "S27-tui": "M5", "S28-tui-alt-screen": "M5",
  "S29-tui-main-screen": "M2", "S30-utils": "M1",
};

const gaps = {
  CombinedAutocompleteProvider: [
    "UTF-16 cursors, async providers, live cancellation, fd argv/path escaping, and the pinned Latin/ASCII ordering corpus are covered, but the hand-pinned Rust collation key falls back to scalar order outside that corpus; general ICU/host-locale localeCompare parity remains open.",
  ],
  Box: ["Removal, cache, background sampling, and lifecycle behavior are covered, but the Rust public seam owns Box<dyn Component> and removes by BoxChildId rather than retained Component object identity."],
  CancellableLoader: ["Live signal/callback behavior, Loader/Text forwarding, and bounded cleanup are covered, but inherited Loader still lacks a real runtime-owned timer and exact JS class/NAPI surface."],
  Editor: ["The rank-2 state, rendering, paste, history, kill/yank, live-keybinding, and autocomplete lifecycle corpus is covered, but the default word fallback retains 1,439 measured Thai-adjacent ICU77/ICU4X divergences. The M5 adapter supplies the JavaScript class/property and event-loop seams; this direct Rust row retains the host-Intl and ABI-shape residuals."],
  HStack: ["Removal, visibility, sizing, and alignment behavior are covered, but the exact children/StackOptions constructor and retained Component identity mapping remain open."],
  Image: ["Pinned render, dimension, cache, ID, and ownership behavior is covered, but the canonical concrete constructor maps to multiple Rust constructors and exact inline rendering requires with_environment; Rust returns a cloned Vec instead of the identical cached JavaScript Array, while the host must consume kitty_image_ownership because Drop performs no terminal deletion. The M5 adapter supplies the JavaScript class and identity seams."],
  ImageOptions: ["Fields and defaults are exercised, but Rust uses bounded numeric types and owned Strings rather than the exact JavaScript object/value boundary."],
  ImageTheme: ["Fallback styling is exercised, but the Rust callback ownership/ABI is not JavaScript function identity."],
  Input: ["The rank-2 input/edit/paste/kill/yank/undo/live-keybinding corpus is covered, but the default word fallback retains the documented Thai-adjacent residual. The M5 adapter supplies the Focusable, callback-property, coercion, and runtime facade; this row measures the narrower direct Rust shape."],
  Loader: [
    "Text forwarding, indicator semantics, empty/one-frame scheduling state, and lifecycle behavior are covered, but Rust models scheduling without the real runtime-owned interval/TUI requestRender wiring or exact JS class/NAPI surface.",
  ],
  Markdown: ["The pinned marked 18 token, rendering, callback-order, cache, source-preservation, indentation, and LaTeX corpus is covered, but canonical JavaScript can return strings with unpaired UTF-16 units and retains JavaScript Array identity; Rust Component returns owned valid-UTF-8 Vec<String>, while the M5 adapter owns the JavaScript class boundary."],
  ScrollView: ["Vertical layout, follow, remainder, nested-hit, scrollbar, fake-clock, stale-timer, and Drop behavior is covered, but the default PassiveTimerHost queues and never fires timeouts; exact direct-Rust timing needs an injected ScrollViewTimerHost. Rust child ownership is not canonical JavaScript Component/children identity; the M5 adapter supplies the runtime timer and class seams."],
  ScrollViewOptions: ["Fields and defaults are exercised, but Rust carries a Horizontal invalid-witness variant and Rust callback/timer ABIs rather than the exact JavaScript options object."],
  ScrollViewScrollToOptions: ["The M6 adapter and Rust state model preserve disableFollow-at-end behavior; the direct Rust option remains a typed bool rather than the exact optional JavaScript object."],
  SettingsList: ["Search editing matches the pinned grapheme, Latin punctuation, viewport, paste, kill/yank, and undo corpus, and submenu completion drains during render/invalidate; dictionary segmentation parity for CJK/Thai remains open, and the embedded search model is not the canonical public Input class."],
  Spacer: ["Rust Default yields zero lines; the reference omitted constructor argument defaults to one."],
  StackChild: ["No exact public Component-or-StackEntry union mapping."],
  StackEntry: ["Rust StackEntry models only StackEntryOptions and omits the canonical required component: Component field."],
  StackEntryOptions: ["No exact public options type and visible(viewport) callback mapping."],
  StackOptions: ["No exact public gap/alignment options type."],
  VStack: ["Removal, visibility, sizing, and alignment behavior are covered, but the exact children/StackOptions constructor and retained Component identity mapping remain open."],
  EditorComponent: ["The object-safe Rust trait covers the rank-2 editor methods, but it substitutes set_on_submit, set_on_change, and set_border_color methods for canonical optional callback and borderColor properties and inherits the partial Rust Component shape. The M5 declaration facade supplies the exact public interface separately."],
  getKeybindings: ["The public adapter proves a retained SharedKeybindings handle with live identity and rebinding, but returns that Rust-specific handle rather than the canonical KeybindingsManager and does not expose getKeys, getDefinition, getConflicts, or getResolvedBindings."],
  KeybindingsManager: ["The canonical constructor accepts an optional KeybindingsConfig whose values may be scalar, array, or undefined; Rust requires Vec<(String, Vec<String>)> and get_user_bindings always returns vectors, so scalar input shape cannot round-trip."],
  setKeybindings: ["Shared handles preserve live identity after retrieval, but setKeybindings still takes ownership and cannot prove the JS caller retains and mutates the exact passed object."],
  Key: ["The M5 package exposes the canonical key-helper object; this row remains partial only because the direct public Rust compatibility namespace uses a different shape."],
  StdinBuffer: ["Pure process/flush semantics and a real lone-ESC reader timeout are covered. Direct Rust keeps timer ownership in pie-app, while the M5 JavaScript facade supplies the canonical runtime timer/class seam."],
  ProcessTerminal: ["Rust exposes ProcessTerminal<B: ProcessTerminalBackend>, requires new(backend), host-delivered stdin/resize, and explicit tick/poll calls. The M5 JavaScript facade supplies the concrete zero-argument process stdin/stdout implementation with real listeners/timers and Promise-returning drainInput; this row records the narrower direct Rust shape."],
  Terminal: ["The Rust Terminal trait remains synchronous and omits canonical drainInput(maxMs?, idleMs?): Promise<void>. ProcessTerminal implements draining through begin_drain_input/poll_drain_input over explicit host clock ticks, with no concrete process backend or JavaScript/NAPI interface."],
  calculateImageRows: ["Five oracle rows match, including the canonical omitted third-argument literal { widthPx: 9, heightPx: 18 } default; the remaining gap is that Rust's typed numeric boundary does not yet cover every JavaScript number coercion."],
  allocateImageId: ["The M5 package exposes bounded process-global allocation semantics; the direct public Rust compatibility namespace does not expose this function."],
  getCellDimensions: ["The M5 package exposes caller-visible cell-dimension state and object identity; the direct public Rust compatibility namespace retains a different shape."],
  imageFallback: ["Formatting matches the pinned rows with home and hyperlink capability facts injected; canonical imageFallback reads os.homedir() and cached capabilities internally, while Rust requires both facts explicitly."],
  renderImage: ["Null, Kitty, and iTerm2 encoding/layout rows match, but canonical renderImage reads cached capabilities and mutable global cell dimensions; Rust requires both facts explicitly and adapts null to Option."],
  getCapabilities: ["The process-global Rust cache and M5 package preserve the reviewed JavaScript cache/object behavior; the direct Rust namespace retains its typed shape."],
  resetCapabilitiesCache: ["The process-global Rust reset and M5 package module-instance behavior are reviewed; the direct Rust namespace retains its typed shape."],
  setCapabilities: ["The Rust cache retains supplied Arc identity and the M5 package preserves caller JavaScript object identity; this row records the direct Rust ABI difference."],
  setCellDimensions: ["The M5 package preserves caller cell-dimension object identity; the direct public Rust compatibility namespace retains a typed shape."],
  SizeValue: ["Absolute/percentage representation and floor behavior are present, but Rust FromStr accepts only unsigned decimal percentages rather than the entire template-literal lexical space, and the JavaScript union/NAPI shape remains open."],
  Component: ["Rust Component now provides default-false wants_key_release() behavior and structural focus hooks, but the canonical optional mutable wantsKeyRelease property, the separate Focusable.focused property shape, and JavaScript/NAPI property identity remain absent."],
  Container: ["ComponentHandle and ComponentRef cover retained and nested identity, duplicate-first removal, cross-owner/stale no-op, and re-add behavior, but the canonical mutable public children array and exact addChild/removeChild Component-object signature are not exposed."],
  OverlayAnchor: ["Rust exposes all nine semantic anchors as public OverlayAnchor variants; the oracle exercises center, top-left, and bottom-right layout, but the canonical JavaScript string union and NAPI boundary remain absent."],
  OverlayHandle: ["The public rank-3 OverlayHandle covers hide, temporary visibility, focus, unfocus, and focus-state behavior, but Rust's OverlayUnfocus argument and retained handle identity are not the canonical optional JavaScript options/object boundary, and no NAPI export exists."],
  OverlayMargin: ["Rust models four required i64 fields defaulting to zero plus OverlayMargins::All for the numeric union; canonical fields are optional JavaScript numbers with different coercion and object semantics, and no NAPI mapping exists."],
  OverlayOptions: ["Layout, default, visibility, and non-capturing behavior is reviewed, but Rust uses Option, usize, i64, SizeValue, and Rc callback fields rather than the exact optional JavaScript object, number coercions, callback identity, and NAPI shape."],
  OverlayUnfocusOptions: ["Rust OverlayUnfocus::Restore and OverlayUnfocus::Target(Option<ComponentRef>) model omitted options versus an explicit target, but they are not the canonical OverlayUnfocusOptions { target: Component | null } JavaScript/NAPI object."],
  TUI: ["The object-safe rank-2 Tui trait and rank-3 TuiBaseController cover shared lifecycle, focus, overlays, listener dispatch, queries, and scheduling, and reviewed injected Rust Main/Alt controllers build on that foundation. Tui is not a Component supertrait, and direct Rust lacks canonical mutable children, terminal, onDebug, and Promise-query property shapes; the M5 adapter supplies the production host/event loop and JavaScript facade."],
  TuiInputListener: ["Rust retains callback identity and live Set-style ordering and reentrancy, but uses a valid-UTF-8 borrowed Rust callback rather than exact JavaScript function/string identity or NAPI."],
  TuiInputListenerResult: ["Outer Option plus TuiInputListenerResult models undefined/pass, consume, and transform, but consume is a required bool and data is Option<String>, not the canonical optional JavaScript object/string/coercion identity, and no NAPI mapping exists."],
  TuiMode: ["Rust exposes Regular and Fullscreen variants and both injected Main/Alt controller modes are oracle-exercised; the exact JavaScript string union and NAPI boundary remain absent."],
  TuiStopOptions: ["Rust uses a required preserve_screen bool with a false default rather than the canonical optional preserveScreen JavaScript property object, and no NAPI mapping exists."],
  ViewportTUI: ["The ViewportTui trait, TuiBaseController layout-root behavior, and injected Rust Alt controller are reviewed. Direct Rust lacks the canonical VIEWPORT_TUI symbol brand and JavaScript interface; the M5 adapter supplies isViewportTUI, the public type facade, and production host event loop."],
  TuiAltScreen: ["The injected Rust TuiAltScreen covers the pinned 0.84.1 lifecycle, alternate-buffer, layout-root, focus/overlay, scroll/mouse/selection, image ownership, and teardown corpus. It still uses injected runtime and terminal seams rather than the canonical JavaScript class; the M5 adapter supplies ProcessTerminal/task pumping, JavaScript identity, package exposure, and current-consumer execution."],
  TuiAltScreenOptions: ["Wheel, mouse, URL-open, and right-click callback behavior is exercised through typed Rust options, but Rust callback ownership and numeric/value coercion are not the canonical optional JavaScript object or NAPI boundary."],
  TuiMainScreen: ["The injected Rust TuiMainScreen covers the pinned 0.84.1 lifecycle, differential render, resize, cursor, Kitty ownership, render-state, and teardown corpus plus real tmux execution. It still uses injected runtime and terminal seams rather than the canonical JavaScript class; the M5 adapter supplies ProcessTerminal/task pumping, JavaScript identity, package exposure, and current-consumer execution."],
};

const behaviorByName = {
  AutocompleteProvider: ["crates/pie-components/tests/golden_m3_components.rs"],
  CombinedAutocompleteProvider: ["crates/pie-components/tests/golden_m3_components.rs"],
  SlashCommand: ["crates/pie-components/tests/golden_m3_components.rs"],
  Box: ["crates/pie-components/tests/golden_m3_components.rs"],
  CancellableLoader: ["crates/pie-components/tests/golden_m3_components.rs"],
  Editor: ["crates/pie-components/tests/editor_input_golden.rs", "crates/pie-components/tests/editor_autocomplete.rs"],
  EditorOptions: ["crates/pie-components/tests/editor_input_golden.rs"],
  EditorTheme: ["crates/pie-components/tests/editor_input_golden.rs"],
  HStack: ["crates/pie-components/tests/golden_m3_components.rs"],
  Image: ["crates/pie-components/tests/golden_image_component.rs"],
  ImageOptions: ["crates/pie-components/tests/golden_image_component.rs"],
  ImageTheme: ["crates/pie-components/tests/golden_image_component.rs"],
  Input: ["crates/pie-components/tests/input_golden.rs"],
  Loader: ["crates/pie-components/tests/golden_m3_components.rs"],
  DefaultTextStyle: ["crates/pie-components/tests/golden_m4_markdown.rs"],
  Markdown: ["crates/pie-components/tests/golden_m4_markdown.rs", "crates/pie-components/tests/golden_m4_markdown_packet.rs"],
  MarkdownOptions: ["crates/pie-components/tests/golden_m4_markdown.rs"],
  MarkdownTheme: ["crates/pie-components/tests/golden_m4_markdown.rs"],
  ScrollView: ["crates/pie-components/tests/golden_layout_scroll.rs"],
  ScrollViewOptions: ["crates/pie-components/tests/golden_layout_scroll.rs"],
  ScrollViewScrollbar: ["crates/pie-components/tests/golden_layout_scroll.rs"],
  ScrollViewScrollToOptions: ["crates/pie-components/tests/golden_layout_scroll.rs", "adapters/pie-napi/test/m6-runtime.test.mjs"],
  SelectList: ["crates/pie-components/tests/golden_m3_components.rs"],
  SettingsList: ["crates/pie-components/tests/golden_m3_components.rs"],
  TruncatedText: ["crates/pie-components/tests/components_golden.rs"],
  VStack: ["crates/pie-components/tests/golden_m3_components.rs"],
  EditorComponent: ["crates/pie-components/tests/editor_model_surface.rs"],
  FuzzyMatch: ["crates/pie-core/tests/golden_m3_core.rs"],
  fuzzyFilter: ["crates/pie-core/tests/golden_m3_core.rs"],
  fuzzyMatch: ["crates/pie-core/tests/golden_m3_core.rs"],
  getKeybindings: ["crates/pie-core/tests/golden_m3_core.rs"],
  KeybindingsManager: ["crates/pie-core/tests/golden_m3_core.rs"],
  setKeybindings: ["crates/pie-core/tests/golden_m3_core.rs"],
  isKeyRelease: ["crates/pie-core/tests/golden_keys.rs"],
  isKeyRepeat: ["crates/pie-core/tests/golden_keys.rs"],
  matchesKey: ["crates/pie-core/tests/golden_keys.rs"],
  parseKey: ["crates/pie-core/tests/golden_keys.rs"],
  RenderLatexOptions: ["crates/pie-core/tests/golden_m4_latex_utf16.rs"],
  renderLatex: ["crates/pie-core/tests/golden_m4_latex_utf16.rs"],
  StdinBuffer: ["crates/pie-core/tests/golden_stdin.rs", "tools/tmux-smoke.sh"],
  ProcessTerminal: ["crates/pie-term/tests/golden_process_terminal.rs"],
  Terminal: ["crates/pie-term/tests/golden_process_terminal.rs"],
  parseOsc11BackgroundColor: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  parseTerminalColorSchemeReport: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  RgbColor: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  TerminalColorScheme: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  CellDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  allocateImageId: ["adapters/pie-napi/test/runtime.test.mjs"],
  calculateImageRows: [
    "crates/pie-core/tests/golden_terminal_primitives.rs",
    "adapters/pie-napi/test/runtime.test.mjs",
  ],
  deleteAllKittyImages: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  deleteKittyImage: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  detectCapabilities: ["crates/pie-term/tests/golden_terminal_capabilities.rs"],
  encodeITerm2: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  encodeKitty: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  getCapabilities: [
    "crates/pie-term/tests/golden_terminal_capabilities.rs",
    "adapters/pie-napi/test/runtime.test.mjs",
  ],
  getCellDimensions: ["adapters/pie-napi/test/runtime.test.mjs"],
  getGifDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  getImageDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  getJpegDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  getPngDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  getWebpDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  hyperlink: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  ImageDimensions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  ImageProtocol: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  ImageRenderOptions: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  imageFallback: ["crates/pie-core/tests/golden_terminal_primitives.rs", "crates/pie-components/tests/golden_image_component.rs"],
  renderImage: ["crates/pie-core/tests/golden_terminal_primitives.rs"],
  resetCapabilitiesCache: [
    "crates/pie-term/tests/golden_terminal_capabilities.rs",
    "adapters/pie-napi/test/runtime.test.mjs",
  ],
  setCapabilities: [
    "crates/pie-term/tests/golden_terminal_capabilities.rs",
    "adapters/pie-napi/test/runtime.test.mjs",
  ],
  setCellDimensions: ["adapters/pie-napi/test/runtime.test.mjs"],
  TerminalCapabilities: ["crates/pie-term/tests/golden_terminal_capabilities.rs"],
  CURSOR_MARKER: ["crates/pie-term/tests/golden_render.rs"],
  Component: [
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
  ],
  Container: [
    "crates/pie-components/tests/golden_m3_components.rs",
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
  ],
  OverlayAnchor: [
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
  ],
  OverlayHandle: ["crates/pie-app/tests/tui_controller.rs"],
  OverlayMargin: ["crates/pie-app/tests/tui_controller.rs"],
  OverlayOptions: [
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
  ],
  OverlayUnfocusOptions: ["crates/pie-app/tests/tui_controller.rs"],
  TUI: [
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
    "crates/pie-app/tests/main_alt_controller.rs",
  ],
  TuiInputListener: ["crates/pie-app/tests/tui_controller.rs"],
  TuiInputListenerResult: ["crates/pie-app/tests/tui_controller.rs"],
  TuiMode: ["crates/pie-app/tests/tui_controller.rs", "crates/pie-app/tests/main_alt_controller.rs"],
  TuiStopOptions: ["crates/pie-app/tests/tui_controller.rs", "crates/pie-app/tests/main_alt_controller.rs"],
  ViewportTUI: [
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-app/tests/tui_controller.rs",
    "crates/pie-app/tests/main_alt_controller.rs",
  ],
  Key: ["adapters/pie-napi/test/runtime.test.mjs"],
  SizeValue: ["crates/pie-components/tests/golden_layout_scroll.rs"],
  TuiAltScreen: [
    "crates/pie-app/tests/fixtures/main-alt-controller.json",
    "crates/pie-app/tests/main_alt_controller.rs",
    "tools/tmux-main-alt-smoke.sh",
  ],
  TuiAltScreenOptions: [
    "crates/pie-app/tests/fixtures/main-alt-controller.json",
    "crates/pie-app/tests/main_alt_controller.rs",
  ],
  TuiMainScreen: [
    "crates/pie-term/tests/golden_render.rs",
    "crates/pie-app/tests/fixtures/main-alt-controller.json",
    "crates/pie-app/tests/main_alt_controller.rs",
    "tools/tmux-smoke.sh",
    "tools/tmux-main-alt-smoke.sh",
  ],
  getOsc8LinkAtColumn: ["crates/pie-core/tests/golden_wrap.rs"],
  sliceByColumn: ["crates/pie-core/tests/golden_wrap.rs"],
  stripTerminalSequences: ["crates/pie-core/tests/golden_text.rs"],
  truncateToWidth: ["crates/pie-core/tests/golden_wrap.rs"],
  visibleWidth: ["crates/pie-core/tests/golden_text.rs"],
  wrapTextWithAnsi: ["crates/pie-core/tests/golden_wrap.rs"],
};

const symbols = [];
for (const statement of api.statements) {
  for (const symbol of statement.symbols) {
    const isCompiled = compiled.has(symbol.name);
    const status = partial.has(symbol.name)
      ? "partial"
      : verified.has(symbol.name)
        ? "verified"
        : isCompiled
          ? "ported"
          : statement.id === "S01-marked"
            ? "external"
            : "deferred";
    symbols.push({
      id: `${statement.id}:${symbol.name}`,
      statementId: statement.id,
      name: symbol.name,
      kind: symbol.exportKind,
      signatureSha256: symbol.signatureSha256,
      defaultMetadata: symbol.defaultMetadata,
      milestone: milestoneByStatement[statement.id],
      status,
      rustEvidence: isCompiled
        ? {
            export: `pie_napi::pi_tui::${symbol.name}`,
            contractTest: "adapters/pie-napi/tests/api_contract.rs",
            productPaths: evidenceByStatement[statement.id] ?? [],
          }
        : null,
      behaviorEvidence: behaviorByName[symbol.name] ?? [],
      gaps: gaps[symbol.name] ?? [],
    });
  }
}

const statements = api.statements.map((statement) => {
  const rows = symbols.filter((symbol) => symbol.statementId === statement.id);
  const ported = rows.filter((symbol) => ["ported", "verified"].includes(symbol.status)).length;
  const verifiedCount = rows.filter((symbol) => symbol.status === "verified").length;
  let status = "partial";
  if (rows.every((symbol) => symbol.status === "external")) status = "external";
  else if (rows.every((symbol) => symbol.status === "deferred")) status = "deferred";
  else if (ported === rows.length) status = verifiedCount === rows.length ? "verified" : "ported";
  return {
    id: statement.id,
    status,
    portedSymbols: ported,
    verifiedSymbols: verifiedCount,
    totalSymbols: rows.length,
  };
});

const metrics = {
  symbols: {
    total: symbols.length,
    compiledMappings: symbols.filter((symbol) => symbol.rustEvidence !== null).length,
    portedOrVerified: symbols.filter((symbol) => ["ported", "verified"].includes(symbol.status)).length,
    verified: symbols.filter((symbol) => symbol.status === "verified").length,
    m3Target: 80,
  },
  statements: {
    total: statements.length,
    complete: statements.filter((statement) => ["ported", "verified"].includes(statement.status)).length,
    verified: statements.filter((statement) => statement.status === "verified").length,
  },
};

if (metrics.symbols.compiledMappings !== 106 || metrics.symbols.portedOrVerified !== 88
  || metrics.symbols.verified !== 40 || metrics.statements.complete !== 12
  || metrics.statements.verified !== 5) {
  throw new Error(`unexpected coverage metrics: ${JSON.stringify(metrics)}`);
}

const ledger = {
  generator: "tools/build-coverage-ledger.mjs",
  reference: api.reference,
  bindings: {
    napi: {
      package: napiPackage.name,
      private: napiPackage.private,
      dropIn: true,
      selectedRuntimeExports: Object.keys(napiOracle.runtimeTypes ?? {}),
      selectedRuntimeExportCount: Object.keys(napiOracle.runtimeTypes ?? {}).length,
      canonicalRuntimeExportCount: api.statements
        .flatMap((statement) => statement.symbols)
        .filter((symbol) => symbol.exportKind === "runtime").length,
      evidence: [
        "adapters/pie-napi/test/check-reference.mjs",
        "adapters/pie-napi/test/check-surface.mjs",
        "adapters/pie-napi/test/check-type-surface.mjs",
        "adapters/pie-napi/test/check-upstream-drift.mjs",
        "adapters/pie-napi/test/runtime.test.mjs",
        "adapters/pie-napi/test/m5-runtime.test.mjs",
        "adapters/pie-napi/test/m6-runtime.test.mjs",
        "adapters/pie-napi/test/m6-semantic-oracle.mjs",
        "adapters/pie-napi/test/differential.mjs",
        "adapters/pie-napi/test/pack-consumer.mjs",
        "adapters/pie-napi/test/artifact-repro.mjs",
        "adapters/pie-napi/test/check-mutations.mjs",
        "tools/tmux-napi-smoke.sh",
        "tools/check-current-dsh-consumer.sh",
      ],
    },
  },
  policy: {
    ordinaryFloor: 88,
    compiledFloor: 106,
    verifiedFloor: 40,
    m3Target: 80,
    note: "The ordinary integrity gate enforces the proved ported, compiled, and verified floors. --milestone M3 independently enforces the historical 80-symbol target, now satisfied by reviewed M5 foundation work.",
  },
  metrics,
  statements,
  symbols,
};
const outputPath = join(root, "tools/surface-coverage.json");
const output = `${JSON.stringify(ledger, null, 2)}\n`;
const args = process.argv.slice(2);
if (args.length > 1 || (args.length === 1 && args[0] !== "--check")) {
  console.error("usage: node tools/build-coverage-ledger.mjs [--check]");
  process.exit(64);
}
if (args[0] === "--check") {
  if (readFileSync(outputPath, "utf8") !== output) {
    console.error("coverage ledger is stale; run node tools/build-coverage-ledger.mjs");
    process.exit(1);
  }
  console.log(`coverage ledger current: ${metrics.symbols.portedOrVerified}/${metrics.symbols.total} ported, ${metrics.symbols.verified} verified`);
} else {
  writeFileSync(outputPath, output);
  console.log(`coverage ledger written: ${metrics.symbols.portedOrVerified}/${metrics.symbols.total} ported, ${metrics.symbols.verified} verified`);
}
