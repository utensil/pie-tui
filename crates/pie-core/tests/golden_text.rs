//! Golden vectors harvested from the pinned pi-tui reference build.
//! Regenerate with PI_TUI_DIST=... node tools/golden/gen-golden-text.mjs
use pie_core::text::{extract_ansi_code_len, strip_terminal_sequences, visible_width};

fn cases() -> Vec<serde_json::Value> {
    let raw = include_str!("fixtures/text-golden.json");
    serde_json::from_str::<serde_json::Value>(raw).expect("fixture json")["cases"]
        .as_array()
        .cloned()
        .unwrap()
}

#[test]
fn visible_width_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        let expected = c["visibleWidth"].as_u64().unwrap() as usize;
        assert_eq!(visible_width(input), expected, "visibleWidth({input:?})");
    }
}

#[test]
fn strip_terminal_sequences_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        let expected = c["stripped"].as_str().unwrap();
        assert_eq!(
            strip_terminal_sequences(input),
            expected,
            "strip({input:?})"
        );
    }
}

/// Map a JS UTF-16 code-unit index onto the byte offset of the scalar starting there.
fn unit_index_to_byte(input: &str, unit_index: usize) -> Option<usize> {
    let mut unit = 0usize;
    for (byte_off, ch) in input.char_indices() {
        if unit == unit_index {
            return Some(byte_off);
        }
        unit += ch.len_utf16();
    }
    if unit == unit_index {
        Some(input.len())
    } else {
        None
    }
}

#[test]
fn ansi_positions_match_reference() {
    for c in cases() {
        if let Some(ansi) = c.get("ansi") {
            let input = c["input"].as_str().unwrap();
            for pair in ansi.as_array().unwrap() {
                let js_at = pair["at"].as_u64().unwrap() as usize;
                let Some(byte_pos) = unit_index_to_byte(input, js_at) else {
                    continue;
                };
                if input.as_bytes()[byte_pos] != 0x1b {
                    continue; // reference scan list only meaningful at escape starts
                }
                let expected = pair["len"].as_u64().map(|v| v as usize);
                assert_eq!(
                    extract_ansi_code_len(input, byte_pos),
                    expected,
                    "extractAnsiCode({input:?},{js_at})"
                );
            }
        }
    }
}
