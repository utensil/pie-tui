//! ICU word navigation using JavaScript-compatible UTF-16 cursor offsets.

use std::sync::OnceLock;

use crate::fuzzy::js_is_whitespace;
use crate::wrap::{is_cjk_break_segment, is_punctuation_char};
use unicode_segmentation::UnicodeSegmentation;

/// A segment returned by a word-boundary implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSegment {
    pub text: String,
    pub is_word_like: bool,
}

pub type WordSegmenterFn<'a> = dyn Fn(&str) -> Vec<WordSegment> + 'a;
pub type AtomicSegmentFn<'a> = dyn Fn(&str) -> bool + 'a;

/// Unicode data version used for grapheme-atomic editor coordinates.
///
/// This is public so a downstream consumer can prove that its independently
/// resolved dependency graph retained the pinned Unicode 16 tables.
pub const SEGMENTATION_UNICODE_VERSION: (u64, u64, u64) = unicode_segmentation::UNICODE_VERSION;

/// Optional reference-compatible overrides used for atomic editor markers.
#[derive(Default)]
pub struct WordNavOptions<'a> {
    pub segment: Option<&'a WordSegmenterFn<'a>>,
    pub is_atomic_segment: Option<&'a AtomicSegmentFn<'a>>,
}

fn word_segmenter() -> icu_segmenter::WordSegmenterBorrowed<'static> {
    static SEGMENTER: OnceLock<icu_segmenter::WordSegmenterBorrowed<'static>> = OnceLock::new();
    *SEGMENTER.get_or_init(|| {
        icu_segmenter::WordSegmenter::new_dictionary(
            icu_segmenter::options::WordBreakInvariantOptions::default(),
        )
    })
}

/// Segment text with the pure fallback used when no host segmenter is injected.
///
/// Unicode 16 UAX 29 handles ordinary spans. Pinned ICU4X dictionary data is
/// limited to contiguous Thai and CJK spans. ICU dictionary lexicons differ,
/// so callers that require complete host parity must inject a segmenter through
/// [`WordNavOptions::segment`].
pub fn default_word_segments(text: &str) -> Vec<WordSegment> {
    let ordinary_segments = text.split_word_bounds().collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut index = 0;
    while index < ordinary_segments.len() {
        let ordinary_segment = ordinary_segments[index];
        if is_complex_script_segment(ordinary_segment) {
            let first = index;
            index += 1;
            while index < ordinary_segments.len()
                && is_complex_script_segment(ordinary_segments[index])
            {
                index += 1;
            }
            let complex_run = ordinary_segments[first..index].concat();
            result.extend(dictionary_word_segments(&complex_run));
        } else {
            result.push(WordSegment {
                text: ordinary_segment.to_owned(),
                is_word_like: node_ordinary_word_like(ordinary_segment),
            });
            index += 1;
        }
    }
    reconcile_node_thai_boundaries(result)
}

fn dictionary_word_segments(text: &str) -> Vec<WordSegment> {
    let mut boundaries = word_segmenter().segment_str(text);
    let mut previous = boundaries.next().unwrap_or(0);
    let mut result = Vec::new();
    while let Some(boundary) = boundaries.next() {
        result.push(WordSegment {
            text: text[previous..boundary].to_owned(),
            is_word_like: boundaries.is_word_like(),
        });
        previous = boundary;
    }
    result
}

fn is_thai(character: char) -> bool {
    ('\u{0e00}'..='\u{0e7f}').contains(&character)
}

fn is_complex_script_segment(segment: &str) -> bool {
    segment.chars().any(is_thai)
        || (!segment.is_empty()
            && segment.chars().all(|character| {
                let mut buffer = [0; 4];
                let scalar = character.encode_utf8(&mut buffer);
                !js_is_whitespace(character) && is_cjk_break_segment(scalar)
            }))
}

fn is_thai_word_base(character: char) -> bool {
    matches!(
        character,
        '\u{0e01}'..='\u{0e2e}'
            | '\u{0e30}'
            | '\u{0e32}'..='\u{0e33}'
            | '\u{0e40}'..='\u{0e46}'
            | '\u{0e50}'..='\u{0e59}'
    )
}

fn node_ordinary_word_like(segment: &str) -> bool {
    // ICU 77 gives a UAX 29 word ending in ExtendNumLet + Extend the
    // non-word status even when an earlier scalar is a letter. This is the
    // only status-level delta exercised by the pinned ordinary-shell product.
    // Keep the boundary shell in unicode-segmentation; this branch only
    // mirrors the status consumed by pi-tui's word-navigation helper.
    let mut reversed = segment.chars().rev().peekable();
    while reversed.next_if_eq(&'\u{0301}').is_some() {}
    if reversed.next_if_eq(&'_').is_some() && segment.ends_with('\u{0301}') {
        return false;
    }

    segment.chars().any(|character| {
        character.is_alphanumeric() || matches!(character, '\u{0600}' | '\u{11a3a}')
    }) || segment
        .chars()
        .filter(|&character| character == '_')
        .count()
        >= 2
}

#[derive(Clone, Copy)]
struct WordSegmentClass {
    word_like: bool,
    has_thai: bool,
    has_cjk: bool,
    is_connector: bool,
}

fn classify_word_segment(segment: &WordSegment) -> WordSegmentClass {
    let has_thai = segment.text.chars().any(is_thai);
    let has_thai_letter = segment.text.chars().any(is_thai_word_base);
    WordSegmentClass {
        // ICU4X reports a bare Thai combining mark as word-like even though
        // ICU 77's Intl.Segmenter leaves it interword. A Thai base restores
        // the word classification; the mark alone must stay non-word.
        word_like: if has_thai && !has_thai_letter {
            false
        } else {
            segment.is_word_like || has_thai_letter
        },
        has_thai,
        has_cjk: is_cjk_break_segment(&segment.text),
        is_connector: segment.text == "_",
    }
}

fn node_joins_thai_boundary(left: WordSegmentClass, right: WordSegmentClass) -> bool {
    if !(left.word_like || left.is_connector) || !(right.word_like || right.is_connector) {
        return false;
    }
    (left.has_thai && !right.has_thai && !right.has_cjk)
        || (right.has_thai && !left.has_thai && !left.has_cjk)
}

fn reconcile_node_thai_mid_punctuation(segments: Vec<WordSegment>) -> Vec<WordSegment> {
    let mut reconciled = Vec::with_capacity(segments.len());
    let mut index = 0;
    while index < segments.len() {
        if index + 2 < segments.len() && segments[index + 1].text == "." {
            let left = classify_word_segment(&segments[index]);
            let right = classify_word_segment(&segments[index + 2]);
            if left.word_like
                && right.word_like
                && !left.has_cjk
                && !right.has_cjk
                && (left.has_thai || right.has_thai)
            {
                reconciled.push(WordSegment {
                    text: [
                        segments[index].text.as_str(),
                        ".",
                        segments[index + 2].text.as_str(),
                    ]
                    .concat(),
                    is_word_like: true,
                });
                index += 3;
                continue;
            }
        }
        reconciled.push(segments[index].clone());
        index += 1;
    }
    reconciled
}

/// ICU 77's `Intl.Segmenter` marks Thai segments as word-like and attaches an
/// adjacent non-CJK word segment, while preserving Thai/Thai and CJK/Thai
/// boundaries. ICU4X 2.0 exposes those pieces separately, so reconcile only
/// the original mixed-script boundaries after segmentation.
fn reconcile_node_thai_boundaries(segments: Vec<WordSegment>) -> Vec<WordSegment> {
    let mut reconciled: Vec<WordSegment> = Vec::with_capacity(segments.len());
    let mut previous = None;
    for mut segment in segments {
        let class = classify_word_segment(&segment);
        segment.is_word_like = class.word_like;
        if previous.is_some_and(|left| node_joins_thai_boundary(left, class)) {
            let previous_segment = reconciled.last_mut().expect("a previous segment exists");
            previous_segment.text.push_str(&segment.text);
            previous_segment.is_word_like = true;
        } else {
            reconciled.push(segment);
        }
        previous = reconciled.last().map(classify_word_segment);
    }
    reconcile_node_thai_mid_punctuation(reconciled)
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Convert a valid UTF-16 cursor boundary to a byte offset. An offset inside
/// a surrogate pair is conservatively clamped to the scalar's start; the
/// editor itself only emits scalar/grapheme boundaries.
fn utf16_to_byte(text: &str, cursor: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units >= cursor || units + character.len_utf16() > cursor {
            return byte;
        }
        units += character.len_utf16();
    }
    text.len()
}

fn segment_text(text: &str, options: &WordNavOptions<'_>) -> Vec<WordSegment> {
    options
        .segment
        .map_or_else(|| default_word_segments(text), |segment| segment(text))
}

fn is_atomic(segment: &str, options: &WordNavOptions<'_>) -> bool {
    options
        .is_atomic_segment
        .is_some_and(|predicate| predicate(segment))
}

fn contains_js_whitespace(segment: &str) -> bool {
    segment.chars().any(js_is_whitespace)
}

/// Move one reference word backward. `cursor` and the return value are UTF-16
/// code-unit offsets, matching JavaScript string indices.
pub fn find_word_backward(text: &str, cursor: usize, options: &WordNavOptions<'_>) -> usize {
    if cursor == 0 {
        return 0;
    }
    let cursor = cursor.min(utf16_len(text));
    let prefix = &text[..utf16_to_byte(text, cursor)];
    let mut segments = segment_text(prefix, options);
    let mut result = cursor;

    while segments.last().is_some_and(|segment| {
        !is_atomic(&segment.text, options) && contains_js_whitespace(&segment.text)
    }) {
        result -= utf16_len(&segments.pop().expect("last segment exists").text);
    }

    let Some(last) = segments.last() else {
        return result;
    };
    if is_atomic(&last.text, options) {
        return result - utf16_len(&last.text);
    }
    if last.is_word_like {
        let segment_len = utf16_len(&last.text);
        let last_punctuation_end = last
            .text
            .char_indices()
            .filter(|(_, character)| is_punctuation_char(*character))
            .map(|(byte, character)| utf16_len(&last.text[..byte]) + character.len_utf16())
            .next_back();
        return last_punctuation_end
            .map_or(result - segment_len, |end| result - (segment_len - end));
    }

    while segments.last().is_some_and(|segment| {
        !is_atomic(&segment.text, options)
            && !segment.is_word_like
            && !contains_js_whitespace(&segment.text)
    }) {
        result -= utf16_len(&segments.pop().expect("last segment exists").text);
    }
    result
}

/// Move one reference word forward. `cursor` and the return value are UTF-16
/// code-unit offsets, matching JavaScript string indices.
pub fn find_word_forward(text: &str, cursor: usize, options: &WordNavOptions<'_>) -> usize {
    let text_units = utf16_len(text);
    if cursor >= text_units {
        return text_units;
    }
    let cursor = cursor.min(text_units);
    let suffix = &text[utf16_to_byte(text, cursor)..];
    let segments = segment_text(suffix, options);
    let mut iter = segments.into_iter().peekable();
    let mut result = cursor;

    while iter.peek().is_some_and(|segment| {
        !is_atomic(&segment.text, options) && contains_js_whitespace(&segment.text)
    }) {
        result += utf16_len(&iter.next().expect("peeked segment exists").text);
    }

    let Some(first) = iter.next() else {
        return result;
    };
    if is_atomic(&first.text, options) {
        return result + utf16_len(&first.text);
    }
    if first.is_word_like {
        let stop = first
            .text
            .char_indices()
            .find(|(_, character)| is_punctuation_char(*character))
            .map_or_else(
                || utf16_len(&first.text),
                |(byte, _)| utf16_len(&first.text[..byte]),
            );
        return result + stop;
    }

    let mut current = first;
    loop {
        if is_atomic(&current.text, options)
            || current.is_word_like
            || contains_js_whitespace(&current.text)
        {
            break;
        }
        result += utf16_len(&current.text);
        let Some(next) = iter.next() else {
            break;
        };
        current = next;
    }
    result
}
