//! Terminal-visible text measurement, ANSI sequence handling.
//!
//! Ported to be behaviorally identical to the pinned pi-tui reference
//! (`@earendil-works/pi-tui@0.84.1`, see tools/surface-manifest.json); golden vectors
//! harvested from the reference live under tests/fixtures/.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

/// Parse an ANSI escape starting at byte position `pos`.
/// Mirrors the reference exactly: CSI terminated by one of `m G K H J`; OSC/APC
/// terminated by BEL or ST (`ESC \`).
pub fn extract_ansi_code_len(s: &str, pos: usize) -> Option<usize> {
    let b = s.as_bytes();
    if pos >= b.len() || b[pos] != 0x1b {
        return None;
    }
    let next = if pos + 1 < b.len() {
        Some(b[pos + 1])
    } else {
        None
    };
    match next {
        Some(b'[') => {
            // Byte-wise scan mirrors `str[j]` single-code-unit checks in the source;
            // terminator chars are ASCII so this cannot split a multi-byte char mid-
            // cluster differently than the reference's per-unit test.
            let mut j = pos + 2;
            while j < b.len() && !matches!(b[j], b'm' | b'G' | b'K' | b'H' | b'J') {
                j += 1;
            }
            if j < b.len() { Some(j + 1 - pos) } else { None }
        }
        Some(b']') | Some(b'_') => {
            let mut j = pos + 2;
            while j < b.len() {
                if b[j] == 0x07 {
                    return Some(j + 1 - pos);
                }
                if b[j] == 0x1b && j + 1 < b.len() && b[j + 1] == b'\\' {
                    return Some(j + 2 - pos);
                }
                j += 1;
            }
            None
        }
        _ => None,
    }
}

/// Remove ANSI, OSC, and APC control sequences while preserving visible text.
pub fn strip_terminal_sequences(s: &str) -> String {
    if !s.contains('\x1b') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if let Some(len) = extract_ansi_code_len(s, i) {
            i += len;
            continue;
        }
        // Advance by one full UTF-8 char (byte-equivalent to the reference's
        // unit-by-unit copy since stripped output keeps every kept unit).
        let ch_len = utf8_len_from_first_byte(b[i]);
        result.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    result
}

fn utf8_len_from_first_byte(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

static ZERO_WIDTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\p{Default_Ignorable_Code_Point}|\p{Control}|\p{Mark})+$").unwrap()
});
static LEADING_NON_PRINTING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Default_Ignorable_Code_Point}\p{Control}\p{Cf}\p{Mark}]+").unwrap()
});
static MARK_CHAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{M}$").unwrap());
// [\p{Spacing_Mark} -- [\u1734 \u302E \u302F]] | listed exceptions
static TERMINAL_SPACING_MARK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "^(?:[\\p{Mc}--[\u{1734}\u{302E}\u{302F}]]|[\u{065F}\u{0F7F}\u{102B}\u{102C}\u{1031}\
\u{1033}-\u{1035}\u{1038}\u{103A}-\u{103E}])+$",
    )
    .unwrap()
});

static RGI_EMOJI: LazyLock<BTreeSet<Vec<u32>>> = LazyLock::new(|| {
    include_str!("text/rgi_emoji.txt")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split_whitespace()
                .filter_map(|t| u32::from_str_radix(t.trim_start_matches("0x"), 16).ok())
                .collect()
        })
        .collect()
});

fn could_be_emoji(chars: &[char], utf16_units: usize) -> bool {
    let Some(first) = chars.first().copied() else {
        return false;
    };
    let cp = first as u32;
    (0x1f000..=0x1fbff).contains(&cp)
        || (0x2300..=0x23ff).contains(&cp)
        || (0x2600..=0x27bf).contains(&cp)
        || (0x2b50..=0x2b55).contains(&cp)
        || chars.contains(&'\u{fe0f}')
        || utf16_units > 2
}

fn is_rgi_emoji(chars: &[char]) -> bool {
    let seq: Vec<u32> = chars.iter().map(|c| *c as u32).collect();
    RGI_EMOJI.contains(&seq)
}

/// Width of a single grapheme cluster, mirroring graphemeWidth() of the reference.
pub fn grapheme_width(segment: &str) -> usize {
    if segment == "\t" {
        return 3;
    }
    let utf16_units = segment.chars().map(|c| c.len_utf16()).sum();
    let chars: Vec<char> = segment.chars().collect();

    // Marks that occupy cells even without a base character take priority.
    if TERMINAL_SPACING_MARK_RE.is_match(segment) {
        return chars.len();
    }
    if ZERO_WIDTH_RE.is_match(segment) {
        return 0;
    }
    if could_be_emoji(&chars, utf16_units) && is_rgi_emoji(&chars) {
        return 2;
    }
    // Strip leading non-printing chars, take the base codepoint.
    let base = LEADING_NON_PRINTING_RE.replace(segment, "").to_string();
    let Some(cp) = base.chars().next() else {
        return 0;
    };

    // Regional indicators render as full-width even when isolated while streaming.
    let cpc = cp as u32;
    if (0x1f1e6..=0x1f1ff).contains(&cpc) {
        return 2;
    }

    let mut width = east_asian_width_char(cp);

    // Intl.Segmenter can fold several terminal-spacing codepoints into one grapheme;
    // count trailing visible code points terminals may allocate cells for: Indic
    // consonants after marks, halfwidth/fullwidth forms, Thai/Lao AM vowels.
    let base_chars: Vec<char> = base.chars().collect();
    let mut follows_mark = false;
    for ch in base_chars.iter().skip(1) {
        let ch_str = ch.to_string();
        if TERMINAL_SPACING_MARK_RE.is_match(ch_str.as_str()) {
            width += 1;
            follows_mark = false;
        } else if MARK_CHAR_RE.is_match(ch_str.as_str()) {
            follows_mark = true;
        } else if !non_printing_char(*ch) {
            let c = *ch as u32;
            if follows_mark || (0xff00..=0xffef).contains(&c) {
                // halfwidth + fullwidth forms measure by EAW
                width += east_asian_width_char(*ch);
            } else if c == 0x0e33 || c == 0x0eb3 {
                width += 1; // SARA AM / Lao AM vowels
            }
            follows_mark = false;
        }
    }
    width
}

/// East Asian Width per the reference's get-east-asian-width default (ambiguous -> 1):
/// F/W -> 2, everything else -> 1.
fn east_asian_width_char(ch: char) -> usize {
    match unicode_width::UnicodeWidthChar::width(ch) {
        Some(2) => 2,
        _ => 1,
    }
}

fn non_printing_char(ch: char) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:\p{Default_Ignorable_Code_Point}|\p{Control}|\p{Cf}|\p{Mark})$").unwrap()
    });
    RE.is_match(&ch.to_string())
}

/// Fast-path printable-ASCII detection (mirrors reference).
fn is_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// Visible terminal-column width of `str`.
///
/// Tabs count as 3 columns; ANSI/OSC/APC sequences contribute nothing; CJK/emoji measure
/// by the reference's rules.
pub fn visible_width(s: &str) -> usize {
    if s.is_empty() {
        return 0;
    }
    if is_printable_ascii(s) {
        return s.len();
    }
    let mut clean = s.to_string();
    if clean.contains('\t') {
        clean = clean.replace('\t', "   ");
    }
    if clean.contains('\x1b') {
        clean = strip_terminal_sequences(&clean);
    }
    clean.graphemes(true).map(grapheme_width).sum()
}
