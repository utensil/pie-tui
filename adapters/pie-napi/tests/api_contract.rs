//! Compile-time and ledger-backed proof for the shipped Rust compatibility names.

use pie_napi::pi_tui::*;
use pie_term::capabilities::TerminalEnvironment;
use pie_term::process_terminal::ProcessTerminalBackend;

fn assert_type<T: ?Sized>() {}

#[derive(Default)]
struct ContractTerminalBackend;

impl ProcessTerminalBackend for ContractTerminalBackend {
    fn raw_mode(&self) -> bool {
        false
    }

    fn set_raw_mode(&mut self, _active: bool) {}
    fn set_utf8_encoding(&mut self) {}
    fn resume_input(&mut self) {}
    fn pause_input(&mut self) {}
    fn subscribe_input(&mut self) {}
    fn unsubscribe_input(&mut self) {}
    fn subscribe_drain_input(&mut self) {}
    fn unsubscribe_drain_input(&mut self) {}
    fn subscribe_resize(&mut self) {}
    fn unsubscribe_resize(&mut self) {}
    fn signal_winch(&mut self) {}
    fn enable_windows_vt_input(&mut self) {}
    fn write(&mut self, _data: &str) {}
    fn columns(&self) -> Option<usize> {
        Some(80)
    }
    fn rows(&self) -> Option<usize> {
        Some(24)
    }
    fn environment_columns(&self) -> Option<usize> {
        None
    }
    fn environment_rows(&self) -> Option<usize> {
        None
    }
}

#[test]
fn every_compiled_mapping_resolves_with_the_expected_top_level_kind() {
    assert_type::<AutocompleteItem>();
    assert_type::<dyn AutocompleteProvider>();
    assert_type::<AutocompleteSuggestions>();
    assert_type::<CombinedAutocompleteProvider>();
    assert_type::<SlashCommand>();
    assert_type::<Box>();
    assert_type::<CancellableLoader>();
    assert_type::<Editor>();
    assert_type::<EditorOptions>();
    assert_type::<EditorTheme>();
    assert_type::<HStack>();
    assert_type::<Image>();
    assert_type::<ImageOptions>();
    assert_type::<ImageTheme>();
    assert_type::<Input>();
    assert_type::<Loader>();
    assert_type::<LoaderIndicatorOptions>();
    assert_type::<DefaultTextStyle>();
    assert_type::<Markdown>();
    assert_type::<MarkdownOptions>();
    assert_type::<MarkdownTheme>();
    assert_type::<ScrollView>();
    assert_type::<ScrollViewOptions>();
    assert_type::<ScrollViewScrollToOptions>();
    assert_type::<ScrollViewScrollbar>();
    assert_type::<SelectItem>();
    assert_type::<SelectList>();
    assert_type::<SelectListLayoutOptions>();
    assert_type::<SelectListTheme>();
    assert_type::<SelectListTruncatePrimaryContext>();
    assert_type::<SettingItem>();
    assert_type::<SettingsList>();
    assert_type::<SettingsListTheme>();
    assert_type::<Spacer>();
    assert_type::<Text>();
    assert_type::<TruncatedText>();
    assert_type::<VStack>();
    assert_type::<dyn EditorComponent>();
    assert_type::<FuzzyMatch>();
    assert_type::<Keybinding>();
    assert_type::<KeybindingConflict>();
    assert_type::<KeybindingDefinition>();
    assert_type::<KeybindingDefinitions>();
    assert_type::<Keybindings>();
    assert_type::<KeybindingsConfig>();
    assert_type::<KeybindingsManager>();
    assert_type::<KeyEventType>();
    assert_type::<KeyId>();
    assert_type::<RenderLatexOptions>();
    assert_type::<StdinBuffer>();
    assert_type::<StdinBufferEventMap>();
    assert_type::<StdinBufferOptions>();
    assert_type::<ProcessTerminal<ContractTerminalBackend>>();
    assert_type::<dyn Terminal>();
    assert_type::<RgbColor>();
    assert_type::<TerminalColorScheme>();
    assert_type::<CellDimensions>();
    assert_type::<ImageDimensions>();
    assert_type::<ImageProtocol>();
    let no_image_protocol: ImageProtocol = None;
    assert!(no_image_protocol.is_none());
    assert_type::<ImageRenderOptions>();
    assert_type::<TerminalCapabilities>();
    assert_type::<dyn Component>();
    assert_type::<Container>();
    assert_type::<SizeValue>();
    assert_type::<TuiMainScreen>();
    assert_type::<TuiMainScreenRenderState>();

    let _fuzzy_match: fn(&str, &str) -> FuzzyMatch = fuzzyMatch;
    let values = vec![String::from("alpha")];
    assert_eq!(fuzzyFilter(&values, "alp", Clone::clone), vec![&values[0]]);

    let _get_keybindings: fn() -> pie_core::keybindings::global::SharedKeybindings = getKeybindings;
    let _set_keybindings: fn(KeybindingsManager) = setKeybindings;
    assert!(!TUI_KEYBINDINGS.is_empty());

    let _decode: fn(&str) -> Option<String> = decodeKittyPrintable;
    let _release: fn(&str) -> bool = isKeyRelease;
    let _repeat: fn(&str) -> bool = isKeyRepeat;
    let _kitty_active: fn() -> bool = isKittyProtocolActive;
    let _matches: fn(&str, &str) -> bool = matchesKey;
    let _parse: fn(&str) -> Option<String> = parseKey;
    let _set_kitty: fn(bool) = setKittyProtocolActive;

    let _render_latex: fn(&[u16], Option<RenderLatexOptions>) -> Option<Vec<u16>> = renderLatex;
    assert_eq!(
        renderLatex(
            &[
                b'\\' as u16,
                b'b' as u16,
                b'a' as u16,
                b'r' as u16,
                b' ' as u16,
                0xd83d,
                0xde00
            ],
            None,
        ),
        Some(vec![0xd83d, 0x0305, 0xde00]),
    );

    let _cursor_marker: &str = CURSOR_MARKER;
    let _composite: fn(&str, &str, usize, usize, usize) -> String = compositeTuiLine;
    let _link: fn(&str, usize) -> Option<String> = getOsc8LinkAtColumn;
    let _slice: fn(&str, usize, usize, Option<bool>) -> String = sliceByColumn;
    let _strip: fn(&str) -> String = stripTerminalSequences;
    let _truncate: fn(&str, usize, Option<&str>, Option<bool>) -> String = truncateToWidth;
    let _visible: fn(&str) -> usize = visibleWidth;
    let _wrap: fn(&str, usize) -> Vec<String> = wrapTextWithAnsi;

    let _parse_background: fn(&str) -> Option<RgbColor> = parseOsc11BackgroundColor;
    let _parse_scheme: fn(&str) -> Option<TerminalColorScheme> = parseTerminalColorSchemeReport;
    let _calculate_rows: fn(ImageDimensions, f64, Option<CellDimensions>) -> usize =
        calculateImageRows;
    let _delete_all: fn() -> &'static str = deleteAllKittyImages;
    let _delete_one: fn(u32) -> String = deleteKittyImage;
    let _encode_iterm: fn(&str, &pie_core::terminal_image::ITerm2EncodeOptions) -> String =
        encodeITerm2;
    let _encode_kitty: fn(&str, &pie_core::terminal_image::KittyEncodeOptions) -> String =
        encodeKitty;
    let _get_capabilities: fn() -> std::sync::Arc<TerminalCapabilities> = getCapabilities;
    let _set_capabilities: fn(std::sync::Arc<TerminalCapabilities>) = setCapabilities;
    let _reset_capabilities: fn() = resetCapabilitiesCache;
    let _gif: fn(&str) -> Option<ImageDimensions> = getGifDimensions;
    let _image_dimensions: fn(&str, &str) -> Option<ImageDimensions> = getImageDimensions;
    let _jpeg: fn(&str) -> Option<ImageDimensions> = getJpegDimensions;
    let _png: fn(&str) -> Option<ImageDimensions> = getPngDimensions;
    let _webp: fn(&str) -> Option<ImageDimensions> = getWebpDimensions;
    let _hyperlink: fn(&str, &str) -> String = hyperlink;
    let _fallback: fn(&str, Option<ImageDimensions>, Option<&str>, &str, bool) -> String =
        imageFallback;
    assert_eq!(
        imageFallback(
            "image/png",
            Some(ImageDimensions {
                width_px: 1,
                height_px: 2,
            }),
            Some(""),
            "",
            false,
        ),
        "[Image: [image/png] 1x2]",
    );
    let _render_image: fn(
        &str,
        ImageDimensions,
        ImageProtocol,
        &ImageRenderOptions,
        CellDimensions,
    ) -> Option<pie_core::terminal_image::ImageRenderResult> = renderImage;

    let detected = detectCapabilities(&TerminalEnvironment::default(), || false);
    assert_eq!(
        detected,
        TerminalCapabilities {
            images: None,
            true_color: false,
            hyperlinks: false,
        }
    );
}

#[test]
fn compiled_mapping_inventory_is_exactly_the_ledger_inventory() {
    let ledger: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/surface-coverage.json"))
            .expect("surface coverage ledger is valid JSON");
    let ledger_mappings: Vec<(String, String)> = ledger["symbols"]
        .as_array()
        .expect("symbols array")
        .iter()
        .filter(|symbol| !symbol["rustEvidence"].is_null())
        .map(|symbol| {
            (
                symbol["name"].as_str().expect("symbol name").to_owned(),
                symbol["kind"].as_str().expect("symbol kind").to_owned(),
            )
        })
        .collect();
    let compiled_mappings: Vec<(String, String)> = COMPILED_PUBLIC_SYMBOLS
        .iter()
        .map(|symbol| (symbol.name.to_owned(), symbol.export_kind.to_owned()))
        .collect();
    assert_eq!(compiled_mappings.len(), 106);
    assert_eq!(compiled_mappings, ledger_mappings);
}
