//! M5 terminal primitive differentials against the pinned pi-tui 0.84.1 build.

use std::sync::OnceLock;

use pie_core::terminal_colors::{
    RgbColor, TerminalColorScheme, is_osc11_background_color_response,
    parse_osc11_background_color, parse_terminal_color_scheme_report,
};
use pie_core::terminal_image::{
    CellDimensions, ITerm2Dimension, ITerm2EncodeOptions, ImageCellSize, ImageDimensions,
    ImageProtocol, ImageRenderOptions, ImageRenderResult, KittyEncodeOptions, KittyImageMetadata,
    KittyImagePlacement, KittyImageRegistry, calculate_image_cell_size, calculate_image_rows,
    crop_kitty_image_line, delete_all_kitty_images, delete_all_kitty_placements,
    delete_kitty_image, encode_iterm2, encode_kitty, get_gif_dimensions, get_image_dimensions,
    get_jpeg_dimensions, get_png_dimensions, get_webp_dimensions, hyperlink, image_fallback,
    image_id_from_random, is_image_line, render_image,
};

static FIXTURE: OnceLock<serde_json::Value> = OnceLock::new();

fn fixture() -> &'static serde_json::Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/terminal-primitives.json"))
            .expect("terminal-primitives.json is valid JSON")
    })
}

#[test]
fn oracle_is_exactly_pinned() {
    let oracle = &fixture()["oracle"];
    assert_eq!(oracle["package"], "@earendil-works/pi-tui");
    assert_eq!(oracle["version"], "0.84.1");
    assert_eq!(
        oracle["files"]["terminal-colors.js"],
        "e26c8c31d161d175817b3335baab4476737719c389a2a39312aa2ece67ccb119"
    );
    assert_eq!(
        oracle["files"]["terminal-image.js"],
        "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2"
    );
}

#[test]
fn terminal_color_parsers_match() {
    for case in fixture()["colors"].as_array().expect("color cases") {
        let value = case["value"].as_str().expect("value");
        assert_eq!(
            is_osc11_background_color_response(value),
            case["isOsc11"].as_bool().expect("boolean"),
            "OSC 11 recognition for {value:?}"
        );
        assert_eq!(
            rgb_json(parse_osc11_background_color(value)),
            case["rgb"],
            "OSC 11 parse for {value:?}"
        );
        assert_eq!(
            scheme_json(parse_terminal_color_scheme_report(value)),
            case["scheme"],
            "color-scheme parse for {value:?}"
        );
    }
}

#[test]
fn image_protocol_encoders_and_ids_match_byte_for_byte() {
    for case in fixture()["imageLines"].as_array().expect("image lines") {
        assert_eq!(
            is_image_line(case["value"].as_str().unwrap()),
            case["result"].as_bool().unwrap()
        );
    }
    for case in fixture()["allocatedIds"].as_array().expect("ids") {
        assert_eq!(
            image_id_from_random(case["random"].as_f64().unwrap()),
            case["result"].as_u64().unwrap() as u32
        );
    }
    for case in fixture()["kitty"].as_array().expect("kitty cases") {
        let options = kitty_options(&case["options"]);
        assert_eq!(
            encode_kitty(case["data"].as_str().unwrap(), &options),
            case["result"].as_str().unwrap(),
            "{}",
            case["label"].as_str().unwrap()
        );
    }
    assert_eq!(
        delete_kitty_image(42),
        fixture()["kittyDeletes"]["one"].as_str().unwrap()
    );
    assert_eq!(
        delete_all_kitty_images(),
        fixture()["kittyDeletes"]["allImages"].as_str().unwrap()
    );
    assert_eq!(
        delete_all_kitty_placements(),
        fixture()["kittyDeletes"]["allPlacements"].as_str().unwrap()
    );
    for case in fixture()["iterm2"].as_array().expect("iTerm2 cases") {
        let options = iterm2_options(&case["options"]);
        assert_eq!(
            encode_iterm2(case["data"].as_str().unwrap(), &options),
            case["result"].as_str().unwrap(),
            "{}",
            case["label"].as_str().unwrap()
        );
    }
}

#[test]
fn cell_math_and_dimension_parsers_match() {
    for case in fixture()["cellSizes"].as_array().expect("cell cases") {
        let image = dimensions(&case["imageDimensions"]);
        let cells = case["cellDimensions"]
            .is_object()
            .then(|| cell_dimensions(&case["cellDimensions"]));
        let got = calculate_image_cell_size(
            image,
            case["maxWidthCells"].as_f64().unwrap(),
            case["maxHeightCells"].as_f64(),
            cells.unwrap_or_default(),
        );
        assert_eq!(cell_size_json(got), case["result"]);
        assert_eq!(
            calculate_image_rows(
                image,
                case["maxWidthCells"].as_f64().unwrap(),
                cells.unwrap_or_default(),
            ),
            case["rows"].as_u64().unwrap() as usize
        );
    }

    for case in fixture()["dimensions"].as_array().expect("dimension cases") {
        let data = case["data"].as_str().unwrap();
        let mime = case["mime"].as_str().unwrap();
        assert_eq!(dimensions_json(get_png_dimensions(data)), case["png"]);
        assert_eq!(dimensions_json(get_jpeg_dimensions(data)), case["jpeg"]);
        assert_eq!(dimensions_json(get_gif_dimensions(data)), case["gif"]);
        assert_eq!(dimensions_json(get_webp_dimensions(data)), case["webp"]);
        assert_eq!(
            dimensions_json(get_image_dimensions(data, mime)),
            case["generic"]
        );
    }
}

#[test]
fn kitty_metadata_placement_and_crop_match() {
    let oracle = &fixture()["metadata"];
    let line = oracle["line"].as_str().unwrap();
    let mut registry = KittyImageRegistry::default();
    registry.register(KittyImageMetadata {
        image_id: 23,
        columns: 8,
        rows: 5,
        width_px: 80,
        height_px: 90,
    });
    assert_eq!(
        metadata_json(registry.metadata_for_line(line)),
        oracle["found"]
    );
    assert_eq!(
        placement_json(registry.placement_for_line(line)),
        oracle["placement"]
    );
    for case in oracle["crops"].as_array().unwrap() {
        assert_eq!(
            crop_kitty_image_line(
                line,
                case["hiddenRows"].as_i64().unwrap(),
                case["visibleRows"].as_i64().unwrap(),
                &registry,
            ),
            case["result"].as_str().unwrap()
        );
    }
}

#[test]
fn max_dimension_header_placement_and_crop_match() {
    let oracle = &fixture()["maxDimensions"];
    let dimensions = dimensions(&oracle["dimensions"]);
    assert_eq!(
        get_png_dimensions(oracle["data"].as_str().unwrap()),
        Some(dimensions)
    );

    let mut registry = KittyImageRegistry::default();
    registry.register(KittyImageMetadata {
        image_id: 23,
        columns: 8,
        rows: 5,
        width_px: 80,
        height_px: 90,
    });
    registry.register(KittyImageMetadata {
        image_id: oracle["found"]["imageId"].as_u64().unwrap() as u32,
        columns: oracle["found"]["columns"].as_u64().unwrap() as usize,
        rows: oracle["found"]["rows"].as_u64().unwrap() as usize,
        width_px: dimensions.width_px,
        height_px: dimensions.height_px,
    });

    let line = oracle["line"].as_str().unwrap();
    assert_eq!(
        metadata_json(registry.metadata_for_line(line)),
        oracle["found"]
    );
    let placement = registry.placement_for_line(line).unwrap();
    assert_eq!(placement.image_id, u32::MAX - 1);
    assert_eq!(placement.transmission_generation, 2);
    assert_eq!(placement.transmission_bytes, 95);
    assert_eq!(
        placement.estimated_decoded_bytes,
        73_786_976_260_478_468_100_u128
    );
    assert_eq!(
        placement.estimated_decoded_bytes as f64,
        oracle["estimatedDecodedBytesText"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
    );
    assert_eq!(
        placement.sequence,
        oracle["placement"]["sequence"].as_str().unwrap()
    );
    assert_eq!(
        placement.replacement_line,
        oracle["placement"]["replacementLine"].as_str().unwrap()
    );
    assert_eq!(
        crop_kitty_image_line(line, i64::from(u32::MAX - 1), 1, &registry),
        oracle["crop"].as_str().unwrap()
    );

    let large_row_crop = &oracle["largeRowCrop"];
    registry.register(KittyImageMetadata {
        image_id: large_row_crop["imageId"].as_u64().unwrap() as u32,
        columns: 1,
        rows: large_row_crop["rows"].as_u64().unwrap() as usize,
        width_px: dimensions.width_px,
        height_px: dimensions.height_px,
    });
    assert_eq!(
        crop_kitty_image_line(
            large_row_crop["line"].as_str().unwrap(),
            large_row_crop["hiddenRows"].as_i64().unwrap(),
            1,
            &registry,
        ),
        large_row_crop["result"].as_str().unwrap()
    );
}

#[test]
fn render_hyperlink_and_fallback_helpers_match() {
    for case in fixture()["renders"].as_array().expect("render cases") {
        let protocol = match case["protocol"].as_str() {
            Some("kitty") => Some(ImageProtocol::Kitty),
            Some("iterm2") => Some(ImageProtocol::ITerm2),
            None => None,
            other => panic!("unexpected protocol {other:?}"),
        };
        let got = render_image(
            "aGVsbG8=",
            ImageDimensions {
                width_px: 100,
                height_px: 50,
            },
            protocol,
            &render_options(&case["options"]),
            CellDimensions::default(),
        );
        assert_eq!(render_json(got), case["result"]);
    }
    for case in fixture()["hyperlinks"].as_array().unwrap() {
        assert_eq!(
            hyperlink(
                case["text"].as_str().unwrap(),
                case["url"].as_str().unwrap()
            ),
            case["result"].as_str().unwrap()
        );
    }
    for case in fixture()["fallbacks"].as_array().unwrap() {
        let image_dimensions = case["dimensions"]
            .is_object()
            .then(|| dimensions(&case["dimensions"]));
        assert_eq!(
            image_fallback(
                case["mime"].as_str().unwrap(),
                image_dimensions,
                case["filename"].as_str(),
                "/opt/pi-home",
                case["hyperlinks"].as_bool().unwrap(),
            ),
            case["result"].as_str().unwrap()
        );
    }
}

fn rgb_json(value: Option<RgbColor>) -> serde_json::Value {
    value.map_or(
        serde_json::Value::Null,
        |rgb| serde_json::json!({ "r": rgb.r, "g": rgb.g, "b": rgb.b }),
    )
}

fn scheme_json(value: Option<TerminalColorScheme>) -> serde_json::Value {
    match value {
        Some(TerminalColorScheme::Dark) => serde_json::Value::String("dark".to_owned()),
        Some(TerminalColorScheme::Light) => serde_json::Value::String("light".to_owned()),
        None => serde_json::Value::Null,
    }
}

fn kitty_options(value: &serde_json::Value) -> KittyEncodeOptions {
    KittyEncodeOptions {
        columns: value["columns"].as_u64().map(|value| value as usize),
        rows: value["rows"].as_u64().map(|value| value as usize),
        image_id: value["imageId"].as_u64().map(|value| value as u32),
        move_cursor: value["moveCursor"].as_bool(),
    }
}

fn iterm2_dimension(value: &serde_json::Value) -> Option<ITerm2Dimension> {
    value.as_i64().map(ITerm2Dimension::Number).or_else(|| {
        value
            .as_str()
            .map(|text| ITerm2Dimension::Text(text.to_owned()))
    })
}

fn iterm2_options(value: &serde_json::Value) -> ITerm2EncodeOptions {
    ITerm2EncodeOptions {
        width: iterm2_dimension(&value["width"]),
        height: iterm2_dimension(&value["height"]),
        name: value["name"].as_str().map(str::to_owned),
        preserve_aspect_ratio: value["preserveAspectRatio"].as_bool(),
        inline: value["inline"].as_bool(),
    }
}

fn dimensions(value: &serde_json::Value) -> ImageDimensions {
    ImageDimensions {
        width_px: value["widthPx"].as_u64().unwrap() as u32,
        height_px: value["heightPx"].as_u64().unwrap() as u32,
    }
}

fn cell_dimensions(value: &serde_json::Value) -> CellDimensions {
    CellDimensions {
        width_px: value["widthPx"].as_f64().unwrap(),
        height_px: value["heightPx"].as_f64().unwrap(),
    }
}

fn cell_size_json(value: ImageCellSize) -> serde_json::Value {
    serde_json::json!({ "columns": value.columns, "rows": value.rows })
}

fn dimensions_json(value: Option<ImageDimensions>) -> serde_json::Value {
    value.map_or(
        serde_json::Value::Null,
        |value| serde_json::json!({ "widthPx": value.width_px, "heightPx": value.height_px }),
    )
}

fn metadata_json(value: Option<KittyImageMetadata>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |value| {
        serde_json::json!({
            "imageId": value.image_id,
            "columns": value.columns,
            "rows": value.rows,
            "widthPx": value.width_px,
            "heightPx": value.height_px,
        })
    })
}

fn placement_json(value: Option<KittyImagePlacement>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |value| {
        serde_json::json!({
            "imageId": value.image_id,
            "transmissionGeneration": value.transmission_generation,
            "transmissionBytes": value.transmission_bytes,
            "estimatedDecodedBytes": value.estimated_decoded_bytes,
            "sequence": value.sequence,
            "replacementLine": value.replacement_line,
        })
    })
}

fn render_options(value: &serde_json::Value) -> ImageRenderOptions {
    ImageRenderOptions {
        max_width_cells: value["maxWidthCells"].as_f64(),
        max_height_cells: value["maxHeightCells"].as_f64(),
        preserve_aspect_ratio: value["preserveAspectRatio"].as_bool(),
        image_id: value["imageId"].as_u64().map(|value| value as u32),
        move_cursor: value["moveCursor"].as_bool(),
    }
}

fn render_json(value: Option<ImageRenderResult>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |value| {
        let mut object = serde_json::Map::from_iter([
            ("sequence".to_owned(), value.sequence.into()),
            ("columns".to_owned(), value.columns.into()),
            ("rows".to_owned(), value.rows.into()),
        ]);
        if let Some(image_id) = value.image_id {
            object.insert("imageId".to_owned(), image_id.into());
        }
        serde_json::Value::Object(object)
    })
}
