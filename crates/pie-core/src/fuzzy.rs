//! Fuzzy matching — port of reference `fuzzy.js`.
//!
//! Matches if all query characters appear in order (not necessarily
//! consecutive). Lower score = better match. Scores are f64 because the
//! reference adds `i * 0.1` penalties.

/// JS `/\s/` character test — the JavaScript whitespace set, which is NOT the
/// same as Rust `char::is_whitespace` (e.g. U+FEFF is JS-whitespace but has no
/// White_Space property). Kept exact for parity.
pub fn js_is_whitespace(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyResult {
    pub matches: bool,
    pub score: f64,
}

fn js_is_whitespace_unit(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(js_is_whitespace)
}

fn match_query(query_lower: &[u16], text_lower: &[u16]) -> FuzzyResult {
    if query_lower.is_empty() {
        return FuzzyResult {
            matches: true,
            score: 0.0,
        };
    }
    if query_lower.len() > text_lower.len() {
        return FuzzyResult {
            matches: false,
            score: 0.0,
        };
    }
    let mut query_index = 0usize;
    let mut score = 0.0f64;
    let mut last_match_index: isize = -1;
    let mut consecutive_matches = 0i64;
    let mut i = 0usize;
    while i < text_lower.len() && query_index < query_lower.len() {
        if text_lower[i] == query_lower[query_index] {
            let is_word_boundary = i == 0
                || js_is_whitespace_unit(text_lower[i - 1])
                || matches!(text_lower[i - 1], 0x2d | 0x5f | 0x2e | 0x2f | 0x3a);
            // Reward consecutive matches
            if last_match_index == i as isize - 1 {
                consecutive_matches += 1;
                score -= (consecutive_matches * 5) as f64;
            } else {
                consecutive_matches = 0;
                // Penalize gaps
                if last_match_index >= 0 {
                    score += ((i as isize) - last_match_index - 1) as f64 * 2.0;
                }
            }
            // Reward word boundary matches
            if is_word_boundary {
                score -= 10.0;
            }
            // Slight penalty for later matches
            score += i as f64 * 0.1;
            last_match_index = i as isize;
            query_index += 1;
        }
        i += 1;
    }
    if query_index < query_lower.len() {
        return FuzzyResult {
            matches: false,
            score: 0.0,
        };
    }
    if query_lower == text_lower {
        score -= 100.0;
    }
    FuzzyResult {
        matches: true,
        score,
    }
}

/// Fuzzy-match `query` against `text` (reference `fuzzyMatch`).
pub fn fuzzy_match(query: &str, text: &str) -> FuzzyResult {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();
    // JavaScript indexes strings as UTF-16 code units. That is observable in
    // both score penalties and exact-match rewards for astral characters.
    let query_units: Vec<u16> = query_lower.encode_utf16().collect();
    let text_units: Vec<u16> = text_lower.encode_utf16().collect();
    let primary = match_query(&query_units, &text_units);
    if primary.matches {
        return primary;
    }
    // Swapped alphanumeric query: "abc123" -> "123abc", "123abc" -> "abc123".
    let swapped = swapped_query(&query_lower);
    let Some(swapped_query) = swapped else {
        return primary;
    };
    let swapped_units: Vec<u16> = swapped_query.encode_utf16().collect();
    let swapped_match = match_query(&swapped_units, &text_units);
    if !swapped_match.matches {
        return primary;
    }
    FuzzyResult {
        matches: true,
        score: swapped_match.score + 5.0,
    }
}

/// Reference regexes `^(letters)(digits)$` / `^(digits)(letters)$` over the
/// lowercased query; returns the transposed form or None.
fn swapped_query(query_lower: &str) -> Option<String> {
    let letters: String = query_lower
        .chars()
        .filter(|c| c.is_ascii_lowercase())
        .collect();
    let digits: String = query_lower.chars().filter(|c| c.is_ascii_digit()).collect();
    let alpha_then_numeric = !letters.is_empty()
        && !digits.is_empty()
        && query_lower == format!("{}{}", letters, digits);
    let numeric_then_alpha = !letters.is_empty()
        && !digits.is_empty()
        && query_lower == format!("{}{}", digits, letters);
    if alpha_then_numeric {
        Some(format!("{}{}", digits, letters))
    } else if numeric_then_alpha {
        Some(format!("{}{}", letters, digits))
    } else {
        None
    }
}

/// Filter and sort items by fuzzy match quality, best matches first
/// (reference `fuzzyFilter`). Supports whitespace- and slash-separated
/// tokens; all tokens must match. Sort is stable (JS Array#sort stability).
pub fn fuzzy_filter<'a, T, F>(items: &'a [T], query: &str, get_text: F) -> Vec<&'a T>
where
    F: Fn(&T) -> String,
{
    let trimmed = query.trim_matches(js_is_whitespace);
    if trimmed.is_empty() {
        return items.iter().collect();
    }
    let tokens: Vec<&str> = trimmed
        .split(|c: char| js_is_whitespace(c) || c == '/')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return items.iter().collect();
    }
    let mut results: Vec<(&T, f64)> = Vec::new();
    for item in items {
        let text = get_text(item);
        let mut total_score = 0.0f64;
        let mut all_match = true;
        for token in &tokens {
            let m = fuzzy_match(token, &text);
            if m.matches {
                total_score += m.score;
            } else {
                all_match = false;
                break;
            }
        }
        if all_match {
            results.push((item, total_score));
        }
    }
    results.sort_by(|a, b| a.1.total_cmp(&b.1));
    results.into_iter().map(|(item, _)| item).collect()
}
