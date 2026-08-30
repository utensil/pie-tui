//! Compiled Rust compatibility namespace for the canonical `pi-tui` barrel.
//!
//! This is evidence for shipped Rust symbols, not a claim that NAPI/TypeScript
//! runtime bindings exist. `tools/surface-coverage.json` records every known
//! signature, default, lifecycle, and behavior gap separately.

#![allow(non_snake_case)]

pub use pie_components::{
    AutocompleteItem, AutocompleteProvider, AutocompleteSuggestions, BoxComponent as Box,
    CancellableLoader, CombinedAutocompleteProvider, DefaultTextStyle, Editor, EditorComponent,
    EditorOptions, EditorTheme, HStack, Image, ImageOptions, ImageTheme, Input, Loader,
    LoaderIndicatorOptions, Markdown, MarkdownOptions, MarkdownTheme, ScrollView,
    ScrollViewOptions, ScrollViewScrollToOptions, ScrollViewScrollbar, SelectItem, SelectList,
    SelectListLayoutOptions, SelectListTheme, SelectListTruncatePrimaryContext, SettingItem,
    SettingsList, SettingsListTheme, SizeValue, SlashCommand, Spacer, Text, TruncatedText, VStack,
};
pub use pie_components::{Component, Container};
pub use pie_core::fuzzy::FuzzyResult as FuzzyMatch;
pub use pie_core::keybindings::{
    KeyConflict as KeybindingConflict, KeybindingDef as KeybindingDefinition, KeybindingsManager,
    TUI_KEYBINDINGS,
};
pub use pie_core::keys::KeyEventType;
pub use pie_core::latex::RenderLatexOptions;
pub use pie_core::screen::{CURSOR_MARKER, composite_tui_line as compositeTuiLine};
pub use pie_core::stdin_buffer::StdinBuffer;
pub use pie_core::terminal_colors::{
    RgbColor, TerminalColorScheme, parse_osc11_background_color as parseOsc11BackgroundColor,
    parse_terminal_color_scheme_report as parseTerminalColorSchemeReport,
};
use pie_core::terminal_image::ImageProtocol as CoreImageProtocol;
pub use pie_core::terminal_image::{
    CellDimensions, ImageDimensions, ImageRenderOptions,
    delete_all_kitty_images as deleteAllKittyImages, delete_kitty_image as deleteKittyImage,
    encode_iterm2 as encodeITerm2, encode_kitty as encodeKitty,
    get_gif_dimensions as getGifDimensions, get_image_dimensions as getImageDimensions,
    get_jpeg_dimensions as getJpegDimensions, get_png_dimensions as getPngDimensions,
    get_webp_dimensions as getWebpDimensions, hyperlink, render_image as renderImage,
};
pub use pie_term::Terminal;
pub use pie_term::capabilities::{
    TerminalCapabilities, detect_capabilities as detectCapabilities,
    get_capabilities as getCapabilities, reset_capabilities_cache as resetCapabilitiesCache,
    set_capabilities as setCapabilities,
};
pub use pie_term::process_terminal::ProcessTerminal;
pub use pie_term::renderer::{
    MainScreenRenderer as TuiMainScreen, RenderState as TuiMainScreenRenderState,
};

/// Rust key identifiers are validated by `matchesKey`/`parseKey`; the binding
/// layer keeps their owned representation explicit.
pub type KeyId = String;
pub type Keybinding = String;
pub type Keybindings = std::collections::BTreeMap<Keybinding, bool>;
pub type KeybindingDefinitions = Vec<KeybindingDefinition>;
pub type KeybindingsConfig = Vec<(Keybinding, Vec<KeyId>)>;
/// Rust compatibility representation of `"kitty" | "iterm2" | null`.
pub type ImageProtocol = Option<CoreImageProtocol>;

/// Constructor options for the pure StdinBuffer model. Runtime timer wiring is
/// intentionally not claimed here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StdinBufferOptions {
    pub timeout: Option<u64>,
}

/// Rust event-union adaptation of the reference EventEmitter event map.
pub type StdinBufferEventMap = pie_core::stdin_buffer::StdinEvent;

pub fn fuzzyMatch(query: &str, text: &str) -> FuzzyMatch {
    pie_core::fuzzy::fuzzy_match(query, text)
}

pub fn fuzzyFilter<'a, T, F>(items: &'a [T], query: &str, get_text: F) -> Vec<&'a T>
where
    F: Fn(&T) -> String,
{
    pie_core::fuzzy::fuzzy_filter(items, query, get_text)
}

pub fn getKeybindings() -> pie_core::keybindings::global::SharedKeybindings {
    pie_core::keybindings::global::get_keybindings()
}

pub fn setKeybindings(manager: KeybindingsManager) {
    pie_core::keybindings::global::set_keybindings(manager);
}

pub fn decodeKittyPrintable(data: &str) -> Option<String> {
    pie_core::keys::decode_kitty_printable(data)
}

pub fn isKeyRelease(data: &str) -> bool {
    pie_core::keys::is_key_release(data)
}

pub fn isKeyRepeat(data: &str) -> bool {
    pie_core::keys::is_key_repeat(data)
}

pub fn isKittyProtocolActive() -> bool {
    pie_core::keys::is_kitty_protocol_active()
}

pub fn matchesKey(data: &str, key_id: &str) -> bool {
    pie_core::keys::matches_key(data, key_id)
}

pub fn parseKey(data: &str) -> Option<String> {
    pie_core::keys::parse_key(data)
}

pub fn setKittyProtocolActive(active: bool) {
    pie_core::keys::set_kitty_protocol_active(active);
}

pub fn getOsc8LinkAtColumn(line: &str, column: usize) -> Option<String> {
    pie_core::wrap::get_osc8_link_at_column(line, column)
}

pub fn sliceByColumn(line: &str, start_col: usize, length: usize, strict: Option<bool>) -> String {
    pie_core::wrap::slice_by_column(line, start_col, length, strict.unwrap_or(false))
}

pub fn stripTerminalSequences(value: &str) -> String {
    pie_core::text::strip_terminal_sequences(value)
}

pub fn truncateToWidth(
    text: &str,
    max_width: usize,
    ellipsis: Option<&str>,
    pad: Option<bool>,
) -> String {
    pie_core::wrap::truncate_to_width(
        text,
        max_width,
        ellipsis.unwrap_or("..."),
        pad.unwrap_or(false),
    )
}

pub fn visibleWidth(value: &str) -> usize {
    pie_core::text::visible_width(value)
}

pub fn wrapTextWithAnsi(text: &str, width: usize) -> Vec<String> {
    pie_core::wrap::wrap_text_with_ansi(text, width)
}

/// Rust compatibility adapter for the canonical optional cell-dimension input.
pub fn calculateImageRows(
    image_dimensions: ImageDimensions,
    target_width_cells: f64,
    cell_dimensions: Option<CellDimensions>,
) -> usize {
    pie_core::terminal_image::calculate_image_rows(
        image_dimensions,
        target_width_cells,
        cell_dimensions.unwrap_or_default(),
    )
}

/// Text fallback with host-derived home and hyperlink facts still explicit.
pub fn imageFallback(
    mime_type: &str,
    dimensions: Option<ImageDimensions>,
    filename: Option<&str>,
    home: &str,
    hyperlinks: bool,
) -> String {
    pie_core::terminal_image::image_fallback(
        mime_type,
        dimensions,
        filename.filter(|filename| !filename.is_empty()),
        home,
        hyperlinks,
    )
}

/// Exact JavaScript-string compatibility boundary, represented as raw UTF-16
/// units until the M5 NAPI facade converts to and from `JsString`.
pub fn renderLatex(source: &[u16], options: Option<RenderLatexOptions>) -> Option<Vec<u16>> {
    pie_core::latex::render_latex_utf16(source, options.unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledPublicSymbol {
    pub name: &'static str,
    pub export_kind: &'static str,
}

/// Exact top-level names with a compiled Rust mapping. A mapping may still be
/// partial; the exhaustive ledger is authoritative for conformance status.
pub const COMPILED_PUBLIC_SYMBOLS: &[CompiledPublicSymbol] = &[
    CompiledPublicSymbol {
        name: "AutocompleteItem",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "AutocompleteProvider",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "AutocompleteSuggestions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "CombinedAutocompleteProvider",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "SlashCommand",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Box",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "CancellableLoader",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Editor",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "EditorOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "EditorTheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "HStack",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Image",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "ImageOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ImageTheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Input",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Loader",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "LoaderIndicatorOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "DefaultTextStyle",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Markdown",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "MarkdownOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "MarkdownTheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ScrollView",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "ScrollViewOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ScrollViewScrollbar",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ScrollViewScrollToOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SelectItem",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SelectList",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "SelectListLayoutOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SelectListTheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SelectListTruncatePrimaryContext",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SettingItem",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "SettingsList",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "SettingsListTheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Spacer",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Text",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "TruncatedText",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "VStack",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "EditorComponent",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "FuzzyMatch",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "fuzzyFilter",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "fuzzyMatch",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getKeybindings",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Keybinding",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeybindingConflict",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeybindingDefinition",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeybindingDefinitions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Keybindings",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeybindingsConfig",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeybindingsManager",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "setKeybindings",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "TUI_KEYBINDINGS",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "decodeKittyPrintable",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "isKeyRelease",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "isKeyRepeat",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "isKittyProtocolActive",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "KeyEventType",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "KeyId",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "matchesKey",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "parseKey",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "setKittyProtocolActive",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "RenderLatexOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "renderLatex",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "StdinBuffer",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "StdinBufferEventMap",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "StdinBufferOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ProcessTerminal",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "Terminal",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "parseOsc11BackgroundColor",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "parseTerminalColorSchemeReport",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "RgbColor",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "TerminalColorScheme",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "CellDimensions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "calculateImageRows",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "deleteAllKittyImages",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "deleteKittyImage",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "detectCapabilities",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "encodeITerm2",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "encodeKitty",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getCapabilities",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getGifDimensions",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getImageDimensions",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getJpegDimensions",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getPngDimensions",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "getWebpDimensions",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "hyperlink",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "ImageDimensions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ImageProtocol",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "ImageRenderOptions",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "imageFallback",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "renderImage",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "resetCapabilitiesCache",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "setCapabilities",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "TerminalCapabilities",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Component",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "Container",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "CURSOR_MARKER",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "compositeTuiLine",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "SizeValue",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "TuiMainScreen",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "TuiMainScreenRenderState",
        export_kind: "type",
    },
    CompiledPublicSymbol {
        name: "getOsc8LinkAtColumn",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "sliceByColumn",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "stripTerminalSequences",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "truncateToWidth",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "visibleWidth",
        export_kind: "runtime",
    },
    CompiledPublicSymbol {
        name: "wrapTextWithAnsi",
        export_kind: "runtime",
    },
];
