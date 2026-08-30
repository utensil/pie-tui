//! Deterministic terminal-image codecs, parsers, and layout math.
//!
//! Environment detection, capability caching, entropy, and terminal writes live
//! in `pie-term`. This module is deliberately an injected-input rank-0 layer.

use std::collections::{HashMap, VecDeque};

const KITTY_PREFIX: &str = "\x1b_G";
const ITERM2_PREFIX: &str = "\x1b]1337;File=";
const KITTY_CHUNK_SIZE: usize = 4096;
const MAX_KITTY_REGISTRY_ENTRIES: usize = 1000;

/// Supported inline-image protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    ITerm2,
}

/// Pixel dimensions of one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellDimensions {
    pub width_px: f64,
    pub height_px: f64,
}

impl Default for CellDimensions {
    fn default() -> Self {
        Self {
            width_px: 9.0,
            height_px: 18.0,
        }
    }
}

/// Intrinsic pixel dimensions of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width_px: u32,
    pub height_px: u32,
}

/// Rendered terminal-cell footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageCellSize {
    pub columns: usize,
    pub rows: usize,
}

/// Kitty transmission options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyEncodeOptions {
    pub columns: Option<usize>,
    pub rows: Option<usize>,
    pub image_id: Option<u32>,
    /// `None` and `Some(true)` use Kitty's cursor movement; `Some(false)` adds `C=1`.
    pub move_cursor: Option<bool>,
}

/// A numeric or textual iTerm2 dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ITerm2Dimension {
    Number(i64),
    Text(String),
}

impl std::fmt::Display for ITerm2Dimension {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(value) => value.fmt(formatter),
            Self::Text(value) => value.fmt(formatter),
        }
    }
}

/// iTerm2 inline-file options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ITerm2EncodeOptions {
    pub width: Option<ITerm2Dimension>,
    pub height: Option<ITerm2Dimension>,
    pub name: Option<String>,
    pub preserve_aspect_ratio: Option<bool>,
    pub inline: Option<bool>,
}

/// Generic inline-image render options.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageRenderOptions {
    pub max_width_cells: Option<f64>,
    pub max_height_cells: Option<f64>,
    pub preserve_aspect_ratio: Option<bool>,
    pub image_id: Option<u32>,
    pub move_cursor: Option<bool>,
}

/// Result of encoding one image for a selected protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRenderResult {
    pub sequence: String,
    pub columns: usize,
    pub rows: usize,
    pub image_id: Option<u32>,
}

/// Metadata retained for a Kitty image transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyImageMetadata {
    pub image_id: u32,
    pub columns: usize,
    pub rows: usize,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegisteredKittyImageMetadata {
    public: KittyImageMetadata,
    transmission_generation: u64,
}

/// Placement-only rewrite for a registered Kitty transmission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyImagePlacement {
    pub image_id: u32,
    pub transmission_generation: u64,
    pub transmission_bytes: usize,
    pub estimated_decoded_bytes: u128,
    pub sequence: String,
    pub replacement_line: String,
}

/// Bounded, explicit state for Kitty metadata. No global mutable state is used.
#[derive(Debug, Default)]
pub struct KittyImageRegistry {
    generation: u64,
    entries: HashMap<u32, RegisteredKittyImageMetadata>,
    order: VecDeque<u32>,
}

impl KittyImageRegistry {
    pub fn register(&mut self, metadata: KittyImageMetadata) {
        self.generation = self.generation.wrapping_add(1);
        if self.entries.remove(&metadata.image_id).is_some() {
            self.order.retain(|image_id| *image_id != metadata.image_id);
        }
        self.order.push_back(metadata.image_id);
        self.entries.insert(
            metadata.image_id,
            RegisteredKittyImageMetadata {
                public: metadata,
                transmission_generation: self.generation,
            },
        );
        if self.entries.len() > MAX_KITTY_REGISTRY_ENTRIES
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
    }

    pub fn metadata_for_line(&self, line: &str) -> Option<KittyImageMetadata> {
        Some(self.registered_for_line(line)?.public)
    }

    pub fn placement_for_line(&self, line: &str) -> Option<KittyImagePlacement> {
        let (match_index, first_controls, first_payload_start) = kitty_command(line)?;
        let registered = self.registered_for_controls(first_controls)?;
        let mut controls = first_controls;
        let mut payload_start = first_payload_start;
        let transmission_end = loop {
            let relative_end = line[payload_start..].find("\x1b\\")?;
            let end = payload_start + relative_end + 2;
            if !control_has(controls, "m", "1") {
                break end;
            }
            let command_start = end;
            if !line[command_start..].starts_with(KITTY_PREFIX) {
                return None;
            }
            let next = kitty_command_at(line, command_start)?;
            controls = next.0;
            payload_start = next.1;
        };

        const PLACEMENT_KEYS: &[&str] = &[
            "i", "p", "x", "y", "w", "h", "X", "Y", "c", "r", "C", "U", "z", "P", "Q", "H", "V",
        ];
        let retained: Vec<&str> = first_controls
            .split(',')
            .filter(|control| {
                control
                    .split_once('=')
                    .is_some_and(|(key, _)| PLACEMENT_KEYS.contains(&key))
            })
            .collect();
        let sequence = format!("\x1b_Ga=p,q=2,{}\x1b\\", retained.join(","));
        let replacement_line = format!(
            "{}{}{}",
            &line[..match_index],
            sequence,
            &line[transmission_end..]
        );
        Some(KittyImagePlacement {
            image_id: registered.public.image_id,
            transmission_generation: registered.transmission_generation,
            transmission_bytes: transmission_end - match_index,
            estimated_decoded_bytes: u128::from(registered.public.width_px)
                * u128::from(registered.public.height_px)
                * 4,
            sequence,
            replacement_line,
        })
    }

    fn registered_for_line(&self, line: &str) -> Option<&RegisteredKittyImageMetadata> {
        let (_, controls, _) = kitty_command(line)?;
        self.registered_for_controls(controls)
    }

    fn registered_for_controls(&self, controls: &str) -> Option<&RegisteredKittyImageMetadata> {
        let image_id = control_value(controls, "i")?.parse::<u32>().ok()?;
        self.entries.get(&image_id)
    }
}

/// Does a rendered line contain a Kitty or iTerm2 inline-image command?
pub fn is_image_line(line: &str) -> bool {
    line.contains(KITTY_PREFIX) || line.contains(ITERM2_PREFIX)
}

/// Deterministically map an injected `Math.random()`-style unit value to Kitty's ID range.
pub fn image_id_from_random(random_unit: f64) -> u32 {
    const RANGE: f64 = 4_294_967_294.0;
    let bounded = if random_unit.is_finite() {
        random_unit.clamp(0.0, 1.0 - f64::EPSILON)
    } else {
        0.0
    };
    (bounded * RANGE).floor() as u32 + 1
}

pub fn encode_kitty(base64_data: &str, options: &KittyEncodeOptions) -> String {
    let mut params = vec!["a=T".to_owned(), "f=100".to_owned(), "q=2".to_owned()];
    if options.move_cursor == Some(false) {
        params.push("C=1".to_owned());
    }
    if let Some(columns) = options.columns.filter(|value| *value != 0) {
        params.push(format!("c={columns}"));
    }
    if let Some(rows) = options.rows.filter(|value| *value != 0) {
        params.push(format!("r={rows}"));
    }
    if let Some(image_id) = options.image_id.filter(|value| *value != 0) {
        params.push(format!("i={image_id}"));
    }
    let params = params.join(",");
    if base64_data.len() <= KITTY_CHUNK_SIZE {
        return format!("\x1b_G{params};{base64_data}\x1b\\");
    }

    let mut result = String::with_capacity(base64_data.len() + 96);
    let mut offset = 0;
    let mut first = true;
    while offset < base64_data.len() {
        let end = (offset + KITTY_CHUNK_SIZE).min(base64_data.len());
        let chunk = &base64_data[offset..end];
        if first {
            result.push_str(&format!("\x1b_G{params},m=1;{chunk}\x1b\\"));
            first = false;
        } else if end == base64_data.len() {
            result.push_str(&format!("\x1b_Gm=0;{chunk}\x1b\\"));
        } else {
            result.push_str(&format!("\x1b_Gm=1;{chunk}\x1b\\"));
        }
        offset = end;
    }
    result
}

pub fn delete_kitty_image(image_id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={image_id},q=2\x1b\\")
}

pub fn delete_all_kitty_images() -> &'static str {
    "\x1b_Ga=d,d=A,q=2\x1b\\"
}

pub fn delete_all_kitty_placements() -> &'static str {
    "\x1b_Ga=d,d=a,q=2\x1b\\"
}

pub fn encode_iterm2(base64_data: &str, options: &ITerm2EncodeOptions) -> String {
    let mut params = vec![
        format!("inline={}", usize::from(options.inline != Some(false))),
        // `Buffer.byteLength(value, "base64")` assumes valid input and can
        // intentionally overestimate malformed/whitespace-bearing strings.
        format!("size={}", node_base64_byte_length(base64_data)),
    ];
    if let Some(width) = &options.width {
        params.push(format!("width={width}"));
    }
    if let Some(height) = &options.height {
        params.push(format!("height={height}"));
    }
    if let Some(name) = options.name.as_deref().filter(|name| !name.is_empty()) {
        params.push(format!("name={}", base64_encode(name.as_bytes())));
    }
    if options.preserve_aspect_ratio == Some(false) {
        params.push("preserveAspectRatio=0".to_owned());
    }
    format!("\x1b]1337;File={}:{}\x07", params.join(";"), base64_data)
}

pub fn crop_kitty_image_line(
    line: &str,
    hidden_rows: i64,
    visible_rows: i64,
    registry: &KittyImageRegistry,
) -> String {
    let Some(metadata) = registry.metadata_for_line(line) else {
        return line.to_owned();
    };
    let Some((match_index, controls, payload_start)) = kitty_command(line) else {
        return line.to_owned();
    };
    if hidden_rows < 0
        || hidden_rows >= i64::try_from(metadata.rows).unwrap_or(i64::MAX)
        || visible_rows <= 0
    {
        return line.to_owned();
    }
    let hidden = hidden_rows as usize;
    let visible = visible_rows as usize;
    let cropped_rows = visible.min(metadata.rows - hidden);
    if hidden == 0 && cropped_rows == metadata.rows {
        return line.to_owned();
    }
    let image_height = u128::from(metadata.height_px);
    let total_rows = metadata.rows as u128;
    let hidden = hidden as u128;
    let cropped_rows_u128 = cropped_rows as u128;
    let source_y = u32::try_from(image_height * hidden / total_rows).unwrap_or(metadata.height_px);
    let source_end = u32::try_from(
        (image_height * (hidden + cropped_rows_u128))
            .div_ceil(total_rows)
            .min(image_height),
    )
    .unwrap_or(metadata.height_px);
    let source_height = source_end
        .min(metadata.height_px)
        .saturating_sub(source_y)
        .max(1);
    let mut retained: Vec<&str> = controls
        .split(',')
        .filter(|control| !matches!(control.split_once('='), Some(("y" | "h" | "r", _))))
        .collect();
    let y = format!("y={source_y}");
    let h = format!("h={source_height}");
    let r = format!("r={cropped_rows}");
    retained.extend([y.as_str(), h.as_str(), r.as_str()]);
    format!(
        "{}\x1b_G{};{}",
        &line[..match_index],
        retained.join(","),
        &line[payload_start..]
    )
}

pub fn calculate_image_cell_size(
    image_dimensions: ImageDimensions,
    max_width_cells: f64,
    max_height_cells: Option<f64>,
    cell_dimensions: CellDimensions,
) -> ImageCellSize {
    let max_width = positive_floor(max_width_cells);
    let max_height = max_height_cells.map(positive_floor);
    let image_width = f64::from(image_dimensions.width_px.max(1));
    let image_height = f64::from(image_dimensions.height_px.max(1));
    let width_scale = (max_width as f64 * cell_dimensions.width_px) / image_width;
    let height_scale = max_height.map_or(width_scale, |height| {
        (height as f64 * cell_dimensions.height_px) / image_height
    });
    let scale = width_scale.min(height_scale);
    let columns = ((image_width * scale) / cell_dimensions.width_px).ceil() as usize;
    let rows = ((image_height * scale) / cell_dimensions.height_px).ceil() as usize;
    ImageCellSize {
        columns: columns.max(1).min(max_width),
        rows: rows.max(1).min(max_height.unwrap_or(usize::MAX)),
    }
}

pub fn calculate_image_rows(
    image_dimensions: ImageDimensions,
    target_width_cells: f64,
    cell_dimensions: CellDimensions,
) -> usize {
    calculate_image_cell_size(image_dimensions, target_width_cells, None, cell_dimensions).rows
}

pub fn get_png_dimensions(base64_data: &str) -> Option<ImageDimensions> {
    let buffer = permissive_base64_decode(base64_data);
    if buffer.len() < 24 || buffer.get(0..4)? != [0x89, 0x50, 0x4e, 0x47] {
        return None;
    }
    Some(ImageDimensions {
        width_px: u32::from_be_bytes(buffer.get(16..20)?.try_into().ok()?),
        height_px: u32::from_be_bytes(buffer.get(20..24)?.try_into().ok()?),
    })
}

pub fn get_jpeg_dimensions(base64_data: &str) -> Option<ImageDimensions> {
    let buffer = permissive_base64_decode(base64_data);
    if buffer.get(0..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2;
    while offset < buffer.len().saturating_sub(9) {
        if buffer[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = buffer[offset + 1];
        if (0xc0..=0xc2).contains(&marker) {
            return Some(ImageDimensions {
                height_px: u16::from_be_bytes(buffer.get(offset + 5..offset + 7)?.try_into().ok()?)
                    .into(),
                width_px: u16::from_be_bytes(buffer.get(offset + 7..offset + 9)?.try_into().ok()?)
                    .into(),
            });
        }
        if offset + 3 >= buffer.len() {
            return None;
        }
        let length = usize::from(u16::from_be_bytes(
            buffer.get(offset + 2..offset + 4)?.try_into().ok()?,
        ));
        if length < 2 {
            return None;
        }
        offset = offset.saturating_add(2 + length);
    }
    None
}

pub fn get_gif_dimensions(base64_data: &str) -> Option<ImageDimensions> {
    let buffer = permissive_base64_decode(base64_data);
    if buffer.len() < 10 || !matches!(buffer.get(0..6)?, b"GIF87a" | b"GIF89a") {
        return None;
    }
    Some(ImageDimensions {
        width_px: u16::from_le_bytes(buffer.get(6..8)?.try_into().ok()?).into(),
        height_px: u16::from_le_bytes(buffer.get(8..10)?.try_into().ok()?).into(),
    })
}

pub fn get_webp_dimensions(base64_data: &str) -> Option<ImageDimensions> {
    let buffer = permissive_base64_decode(base64_data);
    if buffer.len() < 30 || buffer.get(0..4)? != b"RIFF" || buffer.get(8..12)? != b"WEBP" {
        return None;
    }
    match buffer.get(12..16)? {
        b"VP8 " => Some(ImageDimensions {
            width_px: u16::from_le_bytes(buffer.get(26..28)?.try_into().ok()?).into(),
            height_px: u16::from_le_bytes(buffer.get(28..30)?.try_into().ok()?).into(),
        })
        .map(|dimensions| ImageDimensions {
            width_px: dimensions.width_px & 0x3fff,
            height_px: dimensions.height_px & 0x3fff,
        }),
        b"VP8L" => {
            let bits = u32::from_le_bytes(buffer.get(21..25)?.try_into().ok()?);
            Some(ImageDimensions {
                width_px: (bits & 0x3fff) + 1,
                height_px: ((bits >> 14) & 0x3fff) + 1,
            })
        }
        b"VP8X" => Some(ImageDimensions {
            width_px: (u32::from(buffer[24])
                | (u32::from(buffer[25]) << 8)
                | (u32::from(buffer[26]) << 16))
                + 1,
            height_px: (u32::from(buffer[27])
                | (u32::from(buffer[28]) << 8)
                | (u32::from(buffer[29]) << 16))
                + 1,
        }),
        _ => None,
    }
}

pub fn get_image_dimensions(base64_data: &str, mime_type: &str) -> Option<ImageDimensions> {
    match mime_type {
        "image/png" => get_png_dimensions(base64_data),
        "image/jpeg" => get_jpeg_dimensions(base64_data),
        "image/gif" => get_gif_dimensions(base64_data),
        "image/webp" => get_webp_dimensions(base64_data),
        _ => None,
    }
}

pub fn render_image(
    base64_data: &str,
    image_dimensions: ImageDimensions,
    protocol: Option<ImageProtocol>,
    options: &ImageRenderOptions,
    cell_dimensions: CellDimensions,
) -> Option<ImageRenderResult> {
    let protocol = protocol?;
    let size = calculate_image_cell_size(
        image_dimensions,
        options.max_width_cells.unwrap_or(80.0),
        options.max_height_cells,
        cell_dimensions,
    );
    match protocol {
        ImageProtocol::Kitty => Some(ImageRenderResult {
            sequence: encode_kitty(
                base64_data,
                &KittyEncodeOptions {
                    columns: Some(size.columns),
                    rows: Some(size.rows),
                    image_id: options.image_id,
                    move_cursor: options.move_cursor,
                },
            ),
            columns: size.columns,
            rows: size.rows,
            image_id: options.image_id,
        }),
        ImageProtocol::ITerm2 => Some(ImageRenderResult {
            sequence: encode_iterm2(
                base64_data,
                &ITerm2EncodeOptions {
                    width: Some(ITerm2Dimension::Number(size.columns as i64)),
                    height: Some(ITerm2Dimension::Text("auto".to_owned())),
                    preserve_aspect_ratio: Some(options.preserve_aspect_ratio.unwrap_or(true)),
                    ..ITerm2EncodeOptions::default()
                },
            ),
            columns: size.columns,
            rows: size.rows,
            image_id: None,
        }),
    }
}

/// Wrap visible text in an OSC 8 hyperlink.
pub fn hyperlink(text: &str, url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Pure image fallback formatter with environment-derived facts injected.
pub fn image_fallback(
    mime_type: &str,
    dimensions: Option<ImageDimensions>,
    filename: Option<&str>,
    home: &str,
    hyperlinks: bool,
) -> String {
    let mut parts = Vec::new();
    if let Some(filename) = filename {
        let display = shorten_image_path(filename, home);
        if hyperlinks && filename.starts_with('/') {
            parts.push(hyperlink(&display, &path_to_file_url(filename)));
        } else {
            parts.push(display);
        }
    }
    parts.push(format!("[{mime_type}]"));
    if let Some(dimensions) = dimensions {
        parts.push(format!("{}x{}", dimensions.width_px, dimensions.height_px));
    }
    format!("[Image: {}]", parts.join(" "))
}

fn positive_floor(value: f64) -> usize {
    if !value.is_finite() || value < 1.0 {
        1
    } else {
        value.floor() as usize
    }
}

fn kitty_command(line: &str) -> Option<(usize, &str, usize)> {
    let match_index = line.find(KITTY_PREFIX)?;
    let (controls, payload_start) = kitty_command_at(line, match_index)?;
    Some((match_index, controls, payload_start))
}

fn kitty_command_at(line: &str, command_start: usize) -> Option<(&str, usize)> {
    let controls_start = command_start + KITTY_PREFIX.len();
    let relative_end = line.get(controls_start..)?.find(';')?;
    let controls_end = controls_start + relative_end;
    Some((&line[controls_start..controls_end], controls_end + 1))
}

fn control_value<'a>(controls: &'a str, wanted: &str) -> Option<&'a str> {
    controls.split(',').find_map(|control| {
        let (key, value) = control.split_once('=')?;
        (key == wanted).then_some(value)
    })
}

fn control_has(controls: &str, wanted: &str, wanted_value: &str) -> bool {
    control_value(controls, wanted) == Some(wanted_value)
}

fn shorten_image_path(filename: &str, home: &str) -> String {
    if !home.is_empty()
        && (filename == home
            || filename
                .strip_prefix(home)
                .is_some_and(|tail| tail.starts_with(['/', '\\'])))
    {
        format!("~{}", &filename[home.len()..])
    } else {
        filename.to_owned()
    }
}

fn path_to_file_url(path: &str) -> String {
    let mut result = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn permissive_base64_decode(input: &str) -> Vec<u8> {
    let values: Vec<u8> = input
        .bytes()
        .take_while(|byte| *byte != b'=')
        .filter_map(base64_value)
        .collect();
    let mut output = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        output.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk.len() >= 3 {
            output.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        if chunk.len() >= 4 {
            output.push((chunk[2] << 6) | chunk[3]);
        }
    }
    output
}

fn node_base64_byte_length(input: &str) -> usize {
    let padding = if input.ends_with("==") {
        2
    } else if input.ends_with('=') {
        1
    } else {
        0
    };
    (input.len() * 3 / 4).saturating_sub(padding)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(char::from(TABLE[(a >> 2) as usize]));
        output.push(char::from(TABLE[((a & 0x03) << 4 | b >> 4) as usize]));
        output.push(if chunk.len() >= 2 {
            char::from(TABLE[((b & 0x0f) << 2 | c >> 6) as usize])
        } else {
            '='
        });
        output.push(if chunk.len() >= 3 {
            char::from(TABLE[(c & 0x3f) as usize])
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_bounded_and_refreshes_identity() {
        let mut registry = KittyImageRegistry::default();
        for image_id in 1..=1001 {
            registry.register(KittyImageMetadata {
                image_id,
                columns: 1,
                rows: 1,
                width_px: 1,
                height_px: 1,
            });
        }
        assert!(registry.metadata_for_line("\x1b_Gi=1;A\x1b\\").is_none());
        assert!(registry.metadata_for_line("\x1b_Gi=1001;A\x1b\\").is_some());
    }
}
