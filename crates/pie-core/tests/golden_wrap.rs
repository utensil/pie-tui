//! M1c golden vectors: wrap/truncate/slice/column ops/segments/normalize,
//! harvested from the pinned pi-tui reference build.
//! Regenerate with PI_TUI_DIST=... node tools/golden/gen-golden-text.mjs
use pie_core::wrap::{
    apply_background_to_line, extract_segments, get_grapheme_cell_range, get_osc8_link_at_column,
    normalize_terminal_output, slice_by_column, slice_with_width, truncate_to_width,
    wrap_text_with_ansi,
};

fn cases() -> Vec<serde_json::Value> {
    let raw = include_str!("fixtures/text-golden.json");
    serde_json::from_str::<serde_json::Value>(raw).expect("fixture json")["cases"]
        .as_array()
        .cloned()
        .unwrap()
}

fn embedded_sgr_wrap_cases() -> Vec<serde_json::Value> {
    let raw = include_str!("fixtures/text-golden.json");
    serde_json::from_str::<serde_json::Value>(raw).expect("fixture json")["embeddedSgrWrapCases"]
        .as_array()
        .cloned()
        .unwrap()
}

#[test]
fn wrap_text_with_ansi_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for w in c["wrap"].as_array().unwrap() {
            let width = w["width"].as_u64().unwrap() as usize;
            let expected: Vec<&str> = w["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap())
                .collect();
            let got = wrap_text_with_ansi(input, width);
            assert_eq!(got, expected, "wrapTextWithAnsi({input:?}, {width})");
        }
    }
}

#[test]
fn wrap_text_with_ansi_tracks_embedded_sgr_like_reference() {
    for case in embedded_sgr_wrap_cases() {
        let input = case["input"].as_str().unwrap();
        let width = case["width"].as_u64().unwrap() as usize;
        let expected: Vec<&str> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap())
            .collect();
        assert_eq!(
            wrap_text_with_ansi(input, width),
            expected,
            "embedded SGR wrap case {input:?}"
        );
    }
}

#[test]
fn truncate_to_width_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for t in c["trunc"].as_array().unwrap() {
            let max_width = t["maxWidth"].as_u64().unwrap() as usize;
            let ellipsis = t["ellipsis"].as_str().unwrap();
            let pad = t["pad"].as_bool().unwrap();
            let expected = t["result"].as_str().unwrap();
            let got = truncate_to_width(input, max_width, ellipsis, pad);
            assert_eq!(
                got, expected,
                "truncateToWidth({input:?}, {max_width}, {ellipsis:?}, {pad})"
            );
        }
    }
}

#[test]
fn slice_by_column_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for s in c["slice"].as_array().unwrap() {
            let start_col = s["startCol"].as_u64().unwrap() as usize;
            let length = s["length"].as_u64().unwrap() as usize;
            let strict = s["strict"].as_bool().unwrap();
            assert_eq!(
                slice_by_column(input, start_col, length, strict),
                s["text"].as_str().unwrap(),
                "sliceByColumn({input:?}, {start_col}, {length}, {strict})"
            );
            assert_eq!(
                slice_with_width(input, start_col, length, strict).width,
                s["width"].as_u64().unwrap() as usize,
                "sliceWithWidth width({input:?}, {start_col}, {length}, {strict})"
            );
        }
    }
}

#[test]
fn column_helpers_match_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for col in c["columns"].as_array().unwrap() {
            let column = col["column"].as_u64().unwrap() as usize;
            let expected_range = col["cellRange"].as_object();
            let got_range = get_grapheme_cell_range(input, column);
            match (expected_range, got_range) {
                (None, None) => {}
                (Some(exp), Some(got)) => {
                    assert_eq!(got.start, exp["start"].as_u64().unwrap() as usize);
                    assert_eq!(got.end, exp["end"].as_u64().unwrap() as usize);
                }
                _ => panic!("getGraphemeCellRange({input:?}, {column}) presence mismatch"),
            }
            let expected_url = col["osc8"].as_str();
            let got_url = get_osc8_link_at_column(input, column);
            assert_eq!(
                got_url.as_deref(),
                expected_url,
                "getOsc8LinkAtColumn({input:?}, {column})"
            );
        }
    }
}

#[test]
fn extract_segments_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for s in c["segments"].as_array().unwrap() {
            let before_end = s["beforeEnd"].as_u64().unwrap() as usize;
            let after_start = s["afterStart"].as_u64().unwrap() as usize;
            let after_len = s["afterLen"].as_u64().unwrap() as usize;
            let strict_after = s["strictAfter"].as_bool().unwrap();
            let got = extract_segments(input, before_end, after_start, after_len, strict_after);
            assert_eq!(
                got.before,
                s["before"].as_str().unwrap(),
                "extractSegments before({input:?}, {before_end}, {after_start}, {after_len}, {strict_after})"
            );
            assert_eq!(
                got.after,
                s["after"].as_str().unwrap(),
                "extractSegments after({input:?}, {before_end}, {after_start}, {after_len}, {strict_after})"
            );
            assert_eq!(
                got.before_width,
                s["beforeWidth"].as_u64().unwrap() as usize,
                "extractSegments beforeWidth({input:?})"
            );
            assert_eq!(
                got.after_width,
                s["afterWidth"].as_u64().unwrap() as usize,
                "extractSegments afterWidth({input:?})"
            );
        }
    }
}

#[test]
fn apply_background_matches_reference() {
    let bg_fn = |s: &str| format!("\x1b[41m{s}\x1b[0m");
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        for b in c["bg"].as_array().unwrap() {
            let width = b["width"].as_u64().unwrap() as usize;
            assert_eq!(
                apply_background_to_line(input, width, bg_fn),
                b["result"].as_str().unwrap(),
                "applyBackgroundToLine({input:?}, {width})"
            );
        }
    }
}

#[test]
fn normalize_terminal_output_matches_reference() {
    for c in cases() {
        let input = c["input"].as_str().unwrap();
        assert_eq!(
            normalize_terminal_output(input),
            c["normalize"].as_str().unwrap(),
            "normalizeTerminalOutput({input:?})"
        );
    }
}
