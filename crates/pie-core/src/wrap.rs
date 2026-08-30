//! ANSI-aware wrapping, truncation, and column slicing.
//!
//! Faithful port of the pinned pi-tui reference (`@earendil-works/pi-tui@0.84.1`,
//! `dist/utils.js`): wrapTextWithAnsi, truncateToWidth, sliceByColumn,
//! getGraphemeCellRange, OSC8 column helpers, and the AnsiCodeTracker that
//! preserves styling across line breaks.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::{extract_ansi_code_len, grapheme_width, visible_width};

/// A parsed OSC 8 hyperlink sequence: `\x1b]8;params;url terminator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc8Hyperlink {
    pub params: String,
    pub url: String,
    pub terminator: String,
}

/// Parse an OSC 8 hyperlink escape code; `None` when the code is not OSC 8.
/// `Some(None)`-shaped cases in the reference (OSC 8 with empty URL) collapse to
/// `Some(hyperlink)` only when a URL is present; an empty URL yields `None` here
/// but still terminates the active link in tracker state, so callers that need
/// the distinction use [`parse_osc8_hyperlink_full`].
pub fn parse_osc8_hyperlink(ansi_code: &str) -> Option<Osc8Hyperlink> {
    match parse_osc8_hyperlink_full(ansi_code) {
        FullParse::NotOsc8 => None,
        FullParse::Parsed(h) => Some(h),
        FullParse::EmptyUrl => None,
    }
}

/// Result of reference `parseOsc8Hyperlink`, preserving its three outcomes:
/// `undefined` (not OSC 8), `null` (OSC 8 with empty URL), or a hyperlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullParse {
    NotOsc8,
    EmptyUrl,
    Parsed(Osc8Hyperlink),
}

/// Exact port of reference `parseOsc8Hyperlink` tri-state return.
pub fn parse_osc8_hyperlink_full(ansi_code: &str) -> FullParse {
    if !ansi_code.starts_with("\x1b]8;") {
        return FullParse::NotOsc8;
    }
    let terminator = if ansi_code.ends_with('\x07') {
        "\x07"
    } else {
        "\x1b\\"
    };
    let body = &ansi_code[4..ansi_code.len() - terminator.len()];
    let Some(sep) = body.find(';') else {
        return FullParse::NotOsc8;
    };
    let params = &body[..sep];
    let url = &body[sep + 1..];
    if url.is_empty() {
        return FullParse::EmptyUrl;
    }
    FullParse::Parsed(Osc8Hyperlink {
        params: params.to_string(),
        url: url.to_string(),
        terminator: terminator.to_string(),
    })
}

/// Reference `formatOsc8Hyperlink`.
pub fn format_osc8_hyperlink(h: &Osc8Hyperlink) -> String {
    format!("\x1b]8;{};{}{}", h.params, h.url, h.terminator)
}

/// Reference `formatOsc8Close`.
pub fn format_osc8_close(terminator: &str) -> String {
    format!("\x1b]8;;{terminator}")
}

/// Close sequence for whatever OSC 8 hyperlink is active at the end of
/// `prefix` ("" when none) — reference `getActiveOsc8Close`.
pub fn get_active_osc8_close(prefix: &str) -> String {
    if !prefix.contains("\x1b]8;") {
        return String::new();
    }
    let mut active: Option<Osc8Hyperlink> = None;
    let mut i = 0;
    while i < prefix.len() {
        if let Some(len) = extract_ansi_code_len(prefix, i) {
            match parse_osc8_hyperlink_full(&prefix[i..i + len]) {
                FullParse::Parsed(h) => active = Some(h),
                FullParse::EmptyUrl => active = None,
                FullParse::NotOsc8 => {}
            }
            i += len;
        } else {
            i += utf8_char_len(prefix.as_bytes()[i]);
        }
    }
    active
        .map(|h| format_osc8_close(&h.terminator))
        .unwrap_or_default()
}

fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Track active ANSI SGR codes to preserve styling across line breaks.
///
/// Port of reference `AnsiCodeTracker`: attributes are tracked individually so
/// they can be re-emitted selectively; OSC 8 state survives SGR resets.
#[derive(Debug, Default, Clone)]
pub struct AnsiCodeTracker {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    blink: bool,
    inverse: bool,
    hidden: bool,
    strikethrough: bool,
    fg_color: Option<String>,
    bg_color: Option<String>,
    active_hyperlink: Option<Osc8Hyperlink>,
}

impl AnsiCodeTracker {
    /// Feed one complete escape sequence (SGR or OSC 8 hyperlink).
    pub fn process(&mut self, ansi_code: &str) {
        // OSC 8 open/close; the original terminator is preserved because some
        // terminals only make BEL-terminated links clickable.
        match parse_osc8_hyperlink_full(ansi_code) {
            FullParse::Parsed(h) => {
                self.active_hyperlink = Some(h);
                return;
            }
            FullParse::EmptyUrl => {
                self.active_hyperlink = None;
                return;
            }
            FullParse::NotOsc8 => {}
        }
        let Some(params) = first_sgr_params(ansi_code) else {
            return;
        };
        if params.is_empty() || params == "0" {
            self.reset();
            return;
        }
        let parts: Vec<&str> = params.split(';').collect();
        let mut i = 0;
        while i < parts.len() {
            // Reference `Number.parseInt(part, 10)` accepts leading digits
            // ("2a" -> 2) and yields NaN when there are none.
            let digits: String = parts[i].chars().take_while(char::is_ascii_digit).collect();
            let Ok(code) = digits.parse::<u32>() else {
                i += 1;
                continue;
            };
            // 256-color and RGB forms consume multiple parameters.
            if code == 38 || code == 48 {
                if parts.get(i + 1) == Some(&"5") && parts.get(i + 2).is_some() {
                    let color = format!("{};{};{}", parts[i], parts[i + 1], parts[i + 2]);
                    self.set_color(code == 38, color);
                    i += 3;
                    continue;
                } else if parts.get(i + 1) == Some(&"2") && parts.get(i + 4).is_some() {
                    let color = format!(
                        "{};{};{};{};{}",
                        parts[i],
                        parts[i + 1],
                        parts[i + 2],
                        parts[i + 3],
                        parts[i + 4]
                    );
                    self.set_color(code == 38, color);
                    i += 5;
                    continue;
                }
            }
            match code {
                0 => self.reset(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                5 => self.blink = true,
                7 => self.inverse = true,
                8 => self.hidden = true,
                9 => self.strikethrough = true,
                21 => self.bold = false,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                25 => self.blink = false,
                27 => self.inverse = false,
                28 => self.hidden = false,
                29 => self.strikethrough = false,
                39 => self.fg_color = None,
                49 => self.bg_color = None,
                30..=37 | 90..=97 => self.fg_color = Some(code.to_string()),
                40..=47 | 100..=107 => self.bg_color = Some(code.to_string()),
                _ => {}
            }
            i += 1;
        }
    }

    fn set_color(&mut self, is_fg: bool, color: String) {
        if is_fg {
            self.fg_color = Some(color);
        } else {
            self.bg_color = Some(color);
        }
    }

    /// SGR reset; does not affect OSC 8 hyperlink state.
    pub fn reset(&mut self) {
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.blink = false;
        self.inverse = false;
        self.hidden = false;
        self.strikethrough = false;
        self.fg_color = None;
        self.bg_color = None;
    }

    /// Clear all state for reuse.
    pub fn clear(&mut self) {
        self.reset();
        self.active_hyperlink = None;
    }

    /// Re-open sequence for all active attributes (and hyperlink).
    pub fn get_active_codes(&self) -> String {
        let mut codes: Vec<String> = Vec::new();
        for (active, code) in [
            (self.bold, "1"),
            (self.dim, "2"),
            (self.italic, "3"),
            (self.underline, "4"),
            (self.blink, "5"),
            (self.inverse, "7"),
            (self.hidden, "8"),
            (self.strikethrough, "9"),
        ] {
            if active {
                codes.push(code.to_string());
            }
        }
        if let Some(fg) = &self.fg_color {
            codes.push(fg.clone());
        }
        if let Some(bg) = &self.bg_color {
            codes.push(bg.clone());
        }
        let mut result = if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        };
        if let Some(h) = &self.active_hyperlink {
            result.push_str(&format_osc8_hyperlink(h));
        }
        result
    }

    /// Any live state (SGR attributes, colors, or hyperlink)?
    pub fn has_active_codes(&self) -> bool {
        self.bold
            || self.dim
            || self.italic
            || self.underline
            || self.blink
            || self.inverse
            || self.hidden
            || self.strikethrough
            || self.fg_color.is_some()
            || self.bg_color.is_some()
            || self.active_hyperlink.is_some()
    }

    /// Reset codes to emit at line end: underline off (prevents bleed into
    /// padding) and OSC 8 close (re-opened on the next line).
    pub fn get_line_end_reset(&self) -> String {
        let mut result = String::new();
        if self.underline {
            result.push_str("\x1b[24m");
        }
        if let Some(h) = &self.active_hyperlink {
            result.push_str(&format_osc8_close(&h.terminator));
        }
        result
    }
}

fn first_sgr_params(ansi_code: &str) -> Option<&str> {
    if !ansi_code.ends_with('m') {
        return None;
    }
    let bytes = ansi_code.as_bytes();
    let mut index = 0;
    while index + 2 <= bytes.len() {
        if bytes[index] == b'\x1b' && bytes.get(index + 1) == Some(&b'[') {
            let params_start = index + 2;
            let mut end = params_start;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b';') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'm') {
                return Some(&ansi_code[params_start..end]);
            }
        }
        index += 1;
    }
    None
}

/// Feed every escape sequence in `text` to `tracker` (reference
/// `updateTrackerFromText`).
pub fn update_tracker_from_text(text: &str, tracker: &mut AnsiCodeTracker) {
    let mut i = 0;
    while i < text.len() {
        if let Some(len) = extract_ansi_code_len(text, i) {
            tracker.process(&text[i..i + len]);
            i += len;
        } else {
            i += utf8_char_len(text.as_bytes()[i]);
        }
    }
}

/// A clipped fragment plus its visible width (reference shape
/// `{ text, width }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub text: String,
    pub width: usize,
}

/// Clip `text` to `maxWidth` visible columns, grapheme-exact, ANSI/tab-aware
/// (reference `truncateFragmentToWidth`). Stops cleanly at the limit.
pub fn truncate_fragment_to_width(text: &str, max_width: usize) -> Fragment {
    if max_width == 0 || text.is_empty() {
        return Fragment {
            text: String::new(),
            width: 0,
        };
    }
    if is_printable_ascii(text) {
        // ASCII: byte offsets equal code-unit counts, so JS `slice(0, maxWidth)`
        // is a plain byte cut.
        let clipped = &text[..max_width.min(text.len())];
        return Fragment {
            text: clipped.to_string(),
            width: clipped.len(),
        };
    }
    let has_ansi = text.contains('\x1b');
    let has_tabs = text.contains('\t');
    let mut result = String::new();
    let mut width = 0usize;
    if !has_ansi && !has_tabs {
        for segment in graphemes(text) {
            let w = grapheme_width(segment);
            if width + w > max_width {
                break;
            }
            result.push_str(segment);
            width += w;
        }
        return Fragment {
            text: result,
            width,
        };
    }
    let mut pending_ansi = String::new();
    let mut i = 0usize;
    while i < text.len() {
        if let Some(len) = extract_ansi_code_len(text, i) {
            pending_ansi.push_str(&text[i..i + len]);
            i += len;
            continue;
        }
        if text.as_bytes()[i] == b'\t' {
            if width + 3 > max_width {
                break;
            }
            if !pending_ansi.is_empty() {
                result.push_str(&pending_ansi);
                pending_ansi.clear();
            }
            result.push('\t');
            width += 3;
            i += 1;
            continue;
        }
        let mut end = i;
        while end < text.len() && text.as_bytes()[end] != b'\t' {
            if extract_ansi_code_len(text, end).is_some() {
                break;
            }
            end += 1;
        }
        for segment in graphemes(&text[i..end]) {
            let w = grapheme_width(segment);
            if width + w > max_width {
                return Fragment {
                    text: result,
                    width,
                };
            }
            if !pending_ansi.is_empty() {
                result.push_str(&pending_ansi);
                pending_ansi.clear();
            }
            result.push_str(segment);
            width += w;
        }
        i = end;
    }
    Fragment {
        text: result,
        width,
    }
}

/// Reference `finalizeTruncatedResult`: prefix + hyperlink close + reset +
/// ellipsis + reset, optionally padded to `max_width` visible columns.
fn finalize_truncated_result(
    prefix: &str,
    prefix_width: usize,
    ellipsis: &str,
    ellipsis_width: usize,
    max_width: usize,
    pad: bool,
) -> String {
    const RESET: &str = "\x1b[0m";
    let hyperlink_close = get_active_osc8_close(prefix);
    let visible = prefix_width + ellipsis_width;
    let mut result = format!("{prefix}{hyperlink_close}{RESET}");
    if !ellipsis.is_empty() {
        result.push_str(ellipsis);
        result.push_str(RESET);
    }
    if pad {
        result.push_str(&" ".repeat(max_width.saturating_sub(visible)));
    }
    result
}

/// Truncate to fit `max_width` visible columns, appending `ellipsis` when
/// truncation occurred; optionally pad to exactly `max_width` (reference
/// `truncateToWidth`). ANSI sequences count for zero width.
pub fn truncate_to_width(text: &str, max_width: usize, ellipsis: &str, pad: bool) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.is_empty() {
        return if pad {
            " ".repeat(max_width)
        } else {
            String::new()
        };
    }
    let ellipsis_width = visible_width(ellipsis);
    if ellipsis_width >= max_width {
        let text_width = visible_width(text);
        if text_width <= max_width {
            return if pad {
                format!("{text}{}", " ".repeat(max_width - text_width))
            } else {
                text.to_string()
            };
        }
        let clipped = truncate_fragment_to_width(ellipsis, max_width);
        if clipped.width == 0 {
            return if pad {
                " ".repeat(max_width)
            } else {
                String::new()
            };
        }
        return finalize_truncated_result("", 0, &clipped.text, clipped.width, max_width, pad);
    }
    if is_printable_ascii(text) {
        if text.len() <= max_width {
            return if pad {
                format!("{text}{}", " ".repeat(max_width - text.len()))
            } else {
                text.to_string()
            };
        }
        let target_width = max_width - ellipsis_width;
        return finalize_truncated_result(
            &text[..target_width],
            target_width,
            ellipsis,
            ellipsis_width,
            max_width,
            pad,
        );
    }
    let target_width = max_width - ellipsis_width;
    let mut result = String::new();
    let mut pending_ansi = String::new();
    let mut visible_so_far = 0usize;
    let mut kept_width = 0usize;
    let mut keep_contiguous_prefix = true;
    let mut overflowed = false;
    #[allow(unused_assignments)] // assigned in both branches before any read
    let mut exhausted_input = false;
    let has_ansi = text.contains('\x1b');
    let has_tabs = text.contains('\t');
    if !has_ansi && !has_tabs {
        for segment in graphemes(text) {
            let width = grapheme_width(segment);
            if keep_contiguous_prefix && kept_width + width <= target_width {
                result.push_str(segment);
                kept_width += width;
            } else {
                keep_contiguous_prefix = false;
            }
            visible_so_far += width;
            if visible_so_far > max_width {
                overflowed = true;
                break;
            }
        }
        exhausted_input = !overflowed;
    } else {
        let mut i = 0usize;
        while i < text.len() {
            if let Some(len) = extract_ansi_code_len(text, i) {
                pending_ansi.push_str(&text[i..i + len]);
                i += len;
                continue;
            }
            if text.as_bytes()[i] == b'\t' {
                if keep_contiguous_prefix && kept_width + 3 <= target_width {
                    if !pending_ansi.is_empty() {
                        result.push_str(&pending_ansi);
                        pending_ansi.clear();
                    }
                    result.push('\t');
                    kept_width += 3;
                } else {
                    keep_contiguous_prefix = false;
                    pending_ansi.clear();
                }
                visible_so_far += 3;
                if visible_so_far > max_width {
                    overflowed = true;
                    break;
                }
                i += 1;
                continue;
            }
            let mut end = i;
            while end < text.len() && text.as_bytes()[end] != b'\t' {
                if extract_ansi_code_len(text, end).is_some() {
                    break;
                }
                end += 1;
            }
            for segment in graphemes(&text[i..end]) {
                let width = grapheme_width(segment);
                if keep_contiguous_prefix && kept_width + width <= target_width {
                    if !pending_ansi.is_empty() {
                        result.push_str(&pending_ansi);
                        pending_ansi.clear();
                    }
                    result.push_str(segment);
                    kept_width += width;
                } else {
                    keep_contiguous_prefix = false;
                    pending_ansi.clear();
                }
                visible_so_far += width;
                if visible_so_far > max_width {
                    overflowed = true;
                    break;
                }
            }
            if overflowed {
                break;
            }
            i = end;
        }
        exhausted_input = i >= text.len();
    }
    if !overflowed && exhausted_input {
        return if pad {
            format!(
                "{text}{}",
                " ".repeat(max_width.saturating_sub(visible_so_far))
            )
        } else {
            text.to_string()
        };
    }
    finalize_truncated_result(
        &result,
        kept_width,
        ellipsis,
        ellipsis_width,
        max_width,
        pad,
    )
}

fn graphemes(s: &str) -> impl Iterator<Item = &str> {
    use unicode_segmentation::UnicodeSegmentation;
    s.graphemes(true)
}

fn is_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

// ---------------------------------------------------------------- wrap ----

/// CJK scripts that allow line breaks between any two of their graphemes
/// (reference `cjkBreakRegex`).
static CJK_BREAK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "[\\p{Script_Extensions=Han}\\p{Script_Extensions=Hiragana}\
\u{0020}\\p{Script_Extensions=Katakana}\\p{Script_Extensions=Hangul}\
\u{0020}\\p{Script_Extensions=Bopomofo}]",
    )
    .unwrap()
});

pub(crate) fn is_cjk_break_segment(segment: &str) -> bool {
    CJK_BREAK_RE.is_match(segment)
}

/// JS-exact whitespace (`String::trim` set) for trimEnd/trim parity.
fn is_js_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_whitespace)
}

fn js_trim_all_whitespace(s: &str) -> bool {
    s.chars().all(is_js_whitespace)
}

/// JS `s.trim() === ""` (reference uses this as the empty-text test).
pub fn js_trim_is_empty(s: &str) -> bool {
    js_trim_all_whitespace(s)
}

/// Reference `PUNCTUATION_REGEX` character set.
/// Reference `isPunctuationChar` (`PUNCTUATION_REGEX` test; used by later
/// component waves).
pub fn is_punctuation_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')'
            | '{'
            | '}'
            | '['
            | ']'
            | '<'
            | '>'
            | '.'
            | ','
            | ';'
            | ':'
            | '\''
            | '"'
            | '!'
            | '?'
            | '+'
            | '-'
            | '='
            | '*'
            | '/'
            | '\\'
            | '|'
            | '&'
            | '%'
            | '^'
            | '$'
            | '#'
            | '@'
            | '~'
            | '`'
    )
}

/// Reference `isWhitespaceChar` (`/\s/` test).
pub fn is_whitespace_char(ch: char) -> bool {
    is_js_whitespace(ch)
}

/// One token from [`split_into_tokens_with_ansi`]: a run of word graphemes, a
/// single space, or one standalone CJK grapheme — each carrying its preceding
/// ANSI codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Space,
    Word,
}

/// Split text into words while keeping ANSI codes attached (reference
/// `splitIntoTokensWithAnsi`). CJK graphemes become standalone tokens.
pub fn split_into_tokens_with_ansi(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut pending_ansi = String::new();
    let mut current_kind: Option<TokenKind> = None;

    let flush_current =
        |tokens: &mut Vec<String>, current: &mut String, kind: &mut Option<TokenKind>| {
            if !current.is_empty() {
                tokens.push(std::mem::take(current));
                *kind = None;
            }
        };

    let mut i = 0usize;
    while i < text.len() {
        if let Some(len) = extract_ansi_code_len(text, i) {
            // Hold ANSI codes separately; they attach to the next visible char.
            pending_ansi.push_str(&text[i..i + len]);
            i += len;
            continue;
        }
        let mut end = i;
        while end < text.len() && extract_ansi_code_len(text, end).is_none() {
            end += 1;
        }
        for segment in graphemes(&text[i..end]) {
            let segment_is_space = segment == " ";
            if !segment_is_space && is_cjk_break_segment(segment) {
                flush_current(&mut tokens, &mut current, &mut current_kind);
                tokens.push(std::mem::take(&mut pending_ansi) + segment);
                continue;
            }
            let segment_kind = if segment_is_space {
                TokenKind::Space
            } else {
                TokenKind::Word
            };
            if !current.is_empty() && current_kind.as_ref() != Some(&segment_kind) {
                flush_current(&mut tokens, &mut current, &mut current_kind);
            }
            if !pending_ansi.is_empty() {
                current.push_str(&pending_ansi);
                pending_ansi.clear();
            }
            current_kind = Some(segment_kind);
            current.push_str(segment);
        }
        i = end;
    }
    // Remaining pending ANSI attach to the last token.
    if !pending_ansi.is_empty() {
        if !current.is_empty() {
            current.push_str(&pending_ansi);
        } else if let Some(last) = tokens.last_mut() {
            last.push_str(&pending_ansi);
        } else {
            current = pending_ansi;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Break an over-long word across lines grapheme by grapheme, carrying ANSI
/// state (reference `breakLongWord`). Returns at least `[""]`.
fn break_long_word(word: &str, width: usize, tracker: &mut AnsiCodeTracker) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = tracker.get_active_codes();
    let mut current_width = 0usize;

    // Separate ANSI codes from visible content; grapheme-segment the rest.
    let mut segments: Vec<(bool, String)> = Vec::new(); // (is_ansi, value)
    let mut i = 0usize;
    while i < word.len() {
        if let Some(len) = extract_ansi_code_len(word, i) {
            segments.push((true, word[i..i + len].to_string()));
            i += len;
        } else {
            let mut end = i;
            while end < word.len() && extract_ansi_code_len(word, end).is_none() {
                end += 1;
            }
            for seg in graphemes(&word[i..end]) {
                segments.push((false, seg.to_string()));
            }
            i = end;
        }
    }

    for (is_ansi, value) in segments {
        if is_ansi {
            current_line.push_str(&value);
            tracker.process(&value);
            continue;
        }
        if value.is_empty() {
            continue;
        }
        let gw = visible_width(&value);
        if current_width + gw > width {
            let line_end_reset = tracker.get_line_end_reset();
            current_line.push_str(&line_end_reset);
            lines.push(std::mem::take(&mut current_line));
            current_line = tracker.get_active_codes();
            current_width = 0;
        }
        current_line.push_str(&value);
        current_width += gw;
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Word-wrap one physical line (reference `wrapSingleLine`). Lines are NOT
/// padded; trailing whitespace is trimmed off every result line.
fn wrap_single_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let visible_length = visible_width(line);
    if visible_length <= width {
        return vec![line.to_string()];
    }
    let mut wrapped: Vec<String> = Vec::new();
    let mut tracker = AnsiCodeTracker::default();
    let tokens = split_into_tokens_with_ansi(line);
    let mut current_line = String::new();
    let mut current_visible_length = 0usize;

    for token in &tokens {
        let token_visible_length = visible_width(token);
        let is_whitespace = js_trim_all_whitespace(token);

        if token_visible_length > width && !is_whitespace {
            if !current_line.is_empty() {
                let line_end_reset = tracker.get_line_end_reset();
                current_line.push_str(&line_end_reset);
                wrapped.push(std::mem::take(&mut current_line));
                // (reference resets currentVisibleLength here too, but it is
                // unconditionally overwritten below — a dead store we omit)
            }
            let broken = break_long_word(token, width, &mut tracker);
            for broken_line in &broken[..broken.len() - 1] {
                wrapped.push(broken_line.clone());
            }
            current_line = broken[broken.len() - 1].clone();
            current_visible_length = visible_width(&current_line);
            continue; // tracker already updated inside break_long_word
        }

        let total_needed = current_visible_length + token_visible_length;
        if total_needed > width && current_visible_length > 0 {
            let mut line_to_wrap = js_trim_end(&current_line).to_string();
            let line_end_reset = tracker.get_line_end_reset();
            line_to_wrap.push_str(&line_end_reset);
            wrapped.push(line_to_wrap);
            if is_whitespace {
                current_line = tracker.get_active_codes();
                current_visible_length = 0;
            } else {
                current_line = tracker.get_active_codes() + token;
                current_visible_length = token_visible_length;
            }
        } else {
            current_line.push_str(token);
            current_visible_length += token_visible_length;
        }
        update_tracker_from_text(token, &mut tracker);
    }
    if !current_line.is_empty() {
        wrapped.push(current_line);
    }
    if wrapped.is_empty() {
        return vec![String::new()];
    }
    // Trailing whitespace can push lines past the requested width.
    wrapped
        .into_iter()
        .map(|l| js_trim_end(&l).to_string())
        .collect()
}

/// Split into lines exactly like JS `text.split(/\r\n|\r|\n/)` (a trailing
/// break yields a trailing empty element).
fn split_lines_js(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                i += if i + 1 < b.len() && b[i + 1] == b'\n' {
                    2
                } else {
                    1
                };
                start = i;
            }
            _ => i += 1,
        }
    }
    lines.push(&text[start..]);
    lines
}

/// Word-wrap text with ANSI codes preserved across line breaks (reference
/// `wrapTextWithAnsi`). Only word-wraps — no padding, no backgrounds. Styles
/// active at a line end are re-opened at the start of the next.
pub fn wrap_text_with_ansi(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let input_lines = split_lines_js(text);
    let mut result: Vec<String> = Vec::new();
    let mut tracker = AnsiCodeTracker::default();
    for input_line in input_lines {
        let prefix = if result.is_empty() {
            String::new()
        } else {
            tracker.get_active_codes()
        };
        for wrapped_line in wrap_single_line(&(prefix + input_line), width) {
            result.push(wrapped_line);
        }
        update_tracker_from_text(input_line, &mut tracker);
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

// ------------------------------------------------------- column ops ----

/// Terminal-cell range occupied by the grapheme at a visible column
/// (reference `getGraphemeCellRange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub start: usize,
    pub end: usize,
}

pub fn get_grapheme_cell_range(line: &str, column: usize) -> Option<CellRange> {
    let mut current_col = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        if let Some(len) = extract_ansi_code_len(line, i) {
            i += len;
            continue;
        }
        let mut text_end = i;
        while text_end < line.len() && extract_ansi_code_len(line, text_end).is_none() {
            text_end += 1;
        }
        for segment in graphemes(&line[i..text_end]) {
            let width = grapheme_width(segment);
            if width > 0 && column >= current_col && column < current_col + width {
                return Some(CellRange {
                    start: current_col,
                    end: current_col + width,
                });
            }
            current_col += width;
        }
        i = text_end;
    }
    None
}

/// Split an OSC 8 open/close code into its URL: `Some(url)` when the code is
/// an OSC 8 hyperlink shape (URL may be empty for a close sequence), `None`
/// when it is not OSC 8. Mirrors the reference regex
/// `/^\x1b\]8;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)$/`.
fn osc8_open_split(ansi_code: &str) -> Option<String> {
    // Reference regex: /^\x1b\]8;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)$/
    let rest = ansi_code.strip_prefix("\x1b]8;")?;
    let params_end = rest.find(';')?;
    let after = &rest[params_end + 1..];
    let url = if let Some(u) = after.strip_suffix('\x07') {
        u
    } else {
        after.strip_suffix("\x1b\\")?
    };
    if url.chars().any(|c| c == '\x07' || c == '\x1b') {
        return None;
    }
    Some(url.to_string())
}

/// OSC 8 hyperlink URL covering a visible terminal column, if any (reference
/// `getOsc8LinkAtColumn`). A close sequence clears the active URL
/// (`activeUrl = hyperlink[1] || undefined`).
pub fn get_osc8_link_at_column(line: &str, column: usize) -> Option<String> {
    let mut active_url: Option<String> = None;
    let mut current_col = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        if let Some(len) = extract_ansi_code_len(line, i) {
            if let Some(url) = osc8_open_split(&line[i..i + len]) {
                active_url = if url.is_empty() { None } else { Some(url) };
            }
            i += len;
            continue;
        }
        let mut text_end = i;
        while text_end < line.len() && extract_ansi_code_len(line, text_end).is_none() {
            text_end += 1;
        }
        for segment in graphemes(&line[i..text_end]) {
            let width = if segment == "\t" {
                3
            } else {
                grapheme_width(segment)
            };
            if column >= current_col && column < current_col + width {
                return active_url;
            }
            current_col += width;
        }
        i = text_end;
    }
    None
}

/// A slice of a line plus the visible width actually captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceWithWidth {
    pub text: String,
    pub width: usize,
}

/// Extract a range of visible columns from a line, handling ANSI codes and
/// wide chars (reference `sliceWithWidth`). `strict` excludes a wide grapheme
/// at the right boundary that would extend past the range.
pub fn slice_with_width(
    line: &str,
    start_col: usize,
    length: usize,
    strict: bool,
) -> SliceWithWidth {
    if length == 0 {
        return SliceWithWidth {
            text: String::new(),
            width: 0,
        };
    }
    let end_col = start_col + length;
    let mut result = String::new();
    let mut result_width = 0usize;
    let mut current_col = 0usize;
    let mut pending_ansi = String::new();
    let mut i = 0usize;
    'outer: while i < line.len() {
        if let Some(len) = extract_ansi_code_len(line, i) {
            if current_col >= start_col && current_col < end_col {
                result.push_str(&line[i..i + len]);
            } else if current_col < start_col {
                pending_ansi.push_str(&line[i..i + len]);
            }
            i += len;
            continue;
        }
        let mut text_end = i;
        while text_end < line.len() && extract_ansi_code_len(line, text_end).is_none() {
            text_end += 1;
        }
        for segment in graphemes(&line[i..text_end]) {
            let w = grapheme_width(segment);
            let in_range = current_col >= start_col && current_col < end_col;
            let fits = !strict || current_col + w <= end_col;
            if in_range && fits {
                if !pending_ansi.is_empty() {
                    result.push_str(&pending_ansi);
                    pending_ansi.clear();
                }
                result.push_str(segment);
                result_width += w;
            }
            current_col += w;
            if current_col >= end_col {
                break 'outer;
            }
        }
        i = text_end;
        if current_col >= end_col {
            break;
        }
    }
    SliceWithWidth {
        text: result,
        width: result_width,
    }
}

/// [`slice_with_width`] keeping only the text.
pub fn slice_by_column(line: &str, start_col: usize, length: usize, strict: bool) -> String {
    slice_with_width(line, start_col, length, strict).text
}

/// "Before"/"after" segments around an overlay region in one pass, with the
/// styling active before the overlay re-applied to the after-segment
/// (reference `extractSegments`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtractedSegments {
    pub before: String,
    pub before_width: usize,
    pub after: String,
    pub after_width: usize,
}

pub fn extract_segments(
    line: &str,
    before_end: usize,
    after_start: usize,
    after_len: usize,
    strict_after: bool,
) -> ExtractedSegments {
    let mut out = ExtractedSegments::default();
    let after_end = after_start + after_len;
    let mut current_col = 0usize;
    let mut i = 0usize;
    let mut pending_ansi_before = String::new();
    let mut after_started = false;
    let mut style_tracker = AnsiCodeTracker::default();

    let done = |after_len: usize, current_col: usize, before_end: usize, after_end: usize| {
        if after_len == 0 {
            current_col >= before_end
        } else {
            current_col >= after_end
        }
    };

    'outer: while i < line.len() {
        if let Some(len) = extract_ansi_code_len(line, i) {
            let code = &line[i..i + len];
            style_tracker.process(code);
            if current_col < before_end {
                pending_ansi_before.push_str(code);
            } else if current_col >= after_start && current_col < after_end && after_started {
                out.after.push_str(code);
            }
            i += len;
            continue;
        }
        let mut text_end = i;
        while text_end < line.len() && extract_ansi_code_len(line, text_end).is_none() {
            text_end += 1;
        }
        for segment in graphemes(&line[i..text_end]) {
            let w = grapheme_width(segment);
            if current_col < before_end && current_col + w <= before_end {
                if !pending_ansi_before.is_empty() {
                    out.before.push_str(&pending_ansi_before);
                    pending_ansi_before.clear();
                }
                out.before.push_str(segment);
                out.before_width += w;
            } else if current_col >= after_start && current_col < after_end {
                let fits = !strict_after || current_col + w <= after_end;
                if fits {
                    if !after_started {
                        // First after-grapheme inherits styling from before the
                        // overlay.
                        out.after.push_str(&style_tracker.get_active_codes());
                        after_started = true;
                    }
                    out.after.push_str(segment);
                    out.after_width += w;
                }
            }
            current_col += w;
            if done(after_len, current_col, before_end, after_end) {
                break 'outer;
            }
        }
        i = text_end;
        if done(after_len, current_col, before_end, after_end) {
            break;
        }
    }
    out
}

/// Apply a background color to a line, padding to full width first (reference
/// `applyBackgroundToLine`).
pub fn apply_background_to_line(
    line: &str,
    width: usize,
    bg_fn: impl Fn(&str) -> String,
) -> String {
    let visible_len = visible_width(line);
    let padding = " ".repeat(width.saturating_sub(visible_len));
    let with_padding = format!("{line}{padding}");
    bg_fn(&with_padding)
}

/// Normalize text for terminal output without changing logical editor content:
/// Thai/Lao AM vowels decompose to their compatibility forms (width-equal,
/// avoids stale-cell repaint artifacts), tabs outside escape sequences expand
/// to the fixed 3-column layout width (reference `normalizeTerminalOutput`).
pub fn normalize_terminal_output(s: &str) -> String {
    let mut normalized = String::with_capacity(s.len());
    if s.contains('\u{0e33}') || s.contains('\u{0eb3}') {
        for ch in s.chars() {
            match ch {
                '\u{0e33}' => normalized.push_str("\u{0e4d}\u{0e32}"),
                '\u{0eb3}' => normalized.push_str("\u{0ecd}\u{0eb2}"),
                other => normalized.push(other),
            }
        }
    } else {
        normalized.push_str(s);
    }
    if !normalized.contains('\t') {
        return normalized;
    }
    let mut result = String::with_capacity(normalized.len());
    let mut i = 0usize;
    while i < normalized.len() {
        if let Some(len) = extract_ansi_code_len(&normalized, i) {
            result.push_str(&normalized[i..i + len]);
            i += len;
            continue;
        }
        match normalized.as_bytes()[i] {
            b'\t' => {
                result.push_str("   ");
                i += 1;
            }
            first => {
                let ch_len = utf8_char_len(first);
                result.push_str(&normalized[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    result
}
