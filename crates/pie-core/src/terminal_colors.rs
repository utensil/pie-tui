//! Pure parsers for terminal color reports.
//!
//! These functions intentionally accept the same bounded OSC 11 and color-scheme
//! response language as the pinned JavaScript reference. They perform no terminal
//! I/O; the rank-1 adapter decides when to query and how to buffer responses.

/// One 8-bit RGB terminal color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Terminal background color scheme reported by CSI `?997`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorScheme {
    Dark,
    Light,
}

/// Whether `data` is one complete OSC 11 background-color response.
pub fn is_osc11_background_color_response(data: &str) -> bool {
    osc11_payload(data).is_some()
}

/// Parse one complete OSC 11 response into normalized 8-bit RGB channels.
pub fn parse_osc11_background_color(data: &str) -> Option<RgbColor> {
    let value = osc11_payload(data)?.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            6 if hex.bytes().all(|byte| byte.is_ascii_hexdigit()) => Some(RgbColor {
                r: u8::from_str_radix(&hex[0..2], 16).ok()?,
                g: u8::from_str_radix(&hex[2..4], 16).ok()?,
                b: u8::from_str_radix(&hex[4..6], 16).ok()?,
            }),
            12 if hex.bytes().all(|byte| byte.is_ascii_hexdigit()) => Some(RgbColor {
                r: parse_osc_hex_channel(&hex[0..4])?,
                g: parse_osc_hex_channel(&hex[4..8])?,
                b: parse_osc_hex_channel(&hex[8..12])?,
            }),
            _ => None,
        };
    }

    let channel_text = if value
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgb:"))
    {
        &value[4..]
    } else if value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgba:"))
    {
        &value[5..]
    } else {
        value
    };
    let mut channels = channel_text.split('/');
    Some(RgbColor {
        r: parse_osc_hex_channel(channels.next()?)?,
        g: parse_osc_hex_channel(channels.next()?)?,
        b: parse_osc_hex_channel(channels.next()?)?,
    })
}

/// Parse one or more concatenated CSI `?997;1n`/`?997;2n` reports.
///
/// Like JavaScript regular-expression capture semantics, the final report wins.
pub fn parse_terminal_color_scheme_report(data: &str) -> Option<TerminalColorScheme> {
    let mut rest = data;
    let mut scheme = None;
    while !rest.is_empty() {
        if let Some(next) = rest.strip_prefix("\x1b[?997;1n") {
            scheme = Some(TerminalColorScheme::Dark);
            rest = next;
        } else {
            let next = rest.strip_prefix("\x1b[?997;2n")?;
            scheme = Some(TerminalColorScheme::Light);
            rest = next;
        }
    }
    scheme
}

fn osc11_payload(data: &str) -> Option<&str> {
    let rest = data.strip_prefix("\x1b]11;")?;
    let payload = rest
        .strip_suffix('\x07')
        .or_else(|| rest.strip_suffix("\x1b\\"))?;
    (!payload.contains(['\x07', '\x1b'])).then_some(payload)
}

fn parse_osc_hex_channel(channel: &str) -> Option<u8> {
    if channel.is_empty() || !channel.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    // Accumulating as f64 mirrors parseInt/division for the small channel widths
    // terminals emit and remains well-defined for permissive longer inputs.
    let mut value = 0.0_f64;
    for byte in channel.bytes() {
        value = value * 16.0 + f64::from(hex_value(byte)?);
    }
    let max = 16.0_f64.powi(i32::try_from(channel.len()).ok()?) - 1.0;
    (max.is_finite() && max > 0.0).then(|| ((value / max) * 255.0).round() as u8)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_embedded_terminators() {
        assert!(!is_osc11_background_color_response(
            "\x1b]11;#000000\x07ignored\x07"
        ));
        assert_eq!(parse_terminal_color_scheme_report(""), None);
    }
}
