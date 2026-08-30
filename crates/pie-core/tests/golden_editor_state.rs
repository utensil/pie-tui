//! Pure editor-state differentials against the pinned pi-tui 0.84.1 build.

use std::sync::OnceLock;

use pie_core::editor_model::{
    EditorAction, EditorEffect, EditorModel, EditorModelSnapshot, EditorVisualLine,
};
use pie_core::kill_ring::{KillRing, PushOptions};
use pie_core::undo_stack::UndoStack;
use pie_core::word_navigation::{
    WordNavOptions, WordSegment, default_word_segments, find_word_backward, find_word_forward,
};
use unicode_segmentation::{UNICODE_VERSION, UnicodeSegmentation};

static FIXTURE: OnceLock<serde_json::Value> = OnceLock::new();

fn fixture() -> &'static serde_json::Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/editor-state.json"))
            .expect("editor-state.json is valid JSON")
    })
}

#[test]
fn oracle_is_exactly_pinned() {
    let oracle = &fixture()["oracle"];
    assert_eq!(oracle["package"], "@earendil-works/pi-tui");
    assert_eq!(oracle["version"], "0.84.1");
    assert_eq!(oracle["runtime"]["node"], "24.4.1");
    assert_eq!(oracle["runtime"]["icu"], "77.1");
    assert_eq!(oracle["runtime"]["unicode"], "16.0");
    assert_eq!(
        oracle["files"]["kill-ring.js"],
        "52212d532f2c5b85ed8977b0f4431f43998c6dc7746d26efc81eb7975b119122"
    );
    assert_eq!(
        oracle["files"]["undo-stack.js"],
        "7fbb318db3521aa1fa6804ffe50245c18d9e9f210a85a48e175fae6a629259cb"
    );
    assert_eq!(
        oracle["files"]["word-navigation.js"],
        "72618be2d05d6c20d9987d0d74de487335056fa0a00a145687f6106a6ae6b9d0"
    );
    assert_eq!(
        oracle["files"]["utils.js"],
        "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052"
    );
    assert_eq!(
        oracle["files"]["components/editor.js"],
        "a384c140d84e5352605250fab0e1284add133dbdda1e986419c4a0778ffa0853"
    );
}

#[test]
fn grapheme_segmentation_matches_node_unicode_16() {
    assert_eq!(UNICODE_VERSION, (16, 0, 0));
    for case in fixture()["graphemeSegmentation"]
        .as_array()
        .expect("grapheme vectors")
    {
        let label = case["label"].as_str().expect("label");
        let text = case["text"].as_str().expect("text");
        let actual = text
            .grapheme_indices(true)
            .map(|(byte, segment)| (text[..byte].encode_utf16().count(), segment.to_owned()))
            .collect::<Vec<_>>();
        let expected = case["segments"]
            .as_array()
            .expect("segments")
            .iter()
            .map(|segment| {
                (
                    segment["index"].as_u64().expect("segment index") as usize,
                    segment["segment"]
                        .as_str()
                        .expect("segment text")
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{label}: {text:?}");
    }
}

#[test]
fn kill_ring_matches_accumulation_and_rotation_traces() {
    for case in fixture()["killRing"].as_array().expect("kill-ring traces") {
        let label = case["label"].as_str().expect("label");
        let actions = case["actions"].as_array().expect("actions");
        let expected = case["trace"].as_array().expect("trace");
        let mut ring = KillRing::new();
        for (index, action) in actions.iter().enumerate() {
            match action["type"].as_str().expect("action type") {
                "push" => ring.push(
                    action["text"].as_str().expect("text"),
                    PushOptions {
                        prepend: action["prepend"].as_bool().expect("prepend"),
                        accumulate: action["accumulate"].as_bool().expect("accumulate"),
                    },
                ),
                "rotate" => ring.rotate(),
                other => panic!("unknown kill-ring action: {other}"),
            }
            assert_eq!(
                ring.len(),
                expected[index]["length"].as_u64().expect("length") as usize,
                "{label} step {index} length"
            );
            assert_eq!(
                ring.peek(),
                expected[index]["peek"].as_str(),
                "{label} step {index} peek"
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TestState {
    lines: Vec<String>,
    cursor: (usize, usize),
}

#[test]
fn undo_stack_clones_on_push_and_clears() {
    let mut stack = UndoStack::new();
    let mut first = TestState {
        lines: vec!["alpha".to_owned()],
        cursor: (0, 5),
    };
    stack.push(&first);
    first.lines[0] = "mutated-after-push".to_owned();
    first.cursor.1 = 0;
    let mut second = TestState {
        lines: vec!["beta".to_owned(), "gamma".to_owned()],
        cursor: (1, 5),
    };
    stack.push(&second);
    second.lines.push("mutated-after-push".to_owned());

    let trace = fixture()["undoStack"].as_array().expect("undo trace");
    assert_eq!(stack.len(), trace[0]["length"].as_u64().unwrap() as usize);
    assert_eq!(
        stack.pop(),
        Some(TestState {
            lines: vec!["beta".to_owned(), "gamma".to_owned()],
            cursor: (1, 5),
        })
    );
    assert_eq!(
        stack.pop(),
        Some(TestState {
            lines: vec!["alpha".to_owned()],
            cursor: (0, 5),
        })
    );
    assert_eq!(stack.pop(), None);
    stack.push(&first);
    stack.clear();
    assert!(stack.is_empty());
    assert_eq!(stack.pop(), None);
}

#[test]
fn word_segmentation_matches_node_icu_77_vectors() {
    for case in fixture()["wordSegmentation"]
        .as_array()
        .expect("word-segmentation vectors")
    {
        let label = case["label"].as_str().expect("label");
        let text = case["text"].as_str().expect("text");
        let mut index = 0;
        let actual = default_word_segments(text)
            .into_iter()
            .map(|segment| {
                let current = (segment.text.clone(), index, segment.is_word_like);
                index += segment.text.encode_utf16().count();
                current
            })
            .collect::<Vec<_>>();
        let expected = case["segments"]
            .as_array()
            .expect("segments")
            .iter()
            .map(|segment| {
                (
                    segment["segment"].as_str().expect("segment").to_owned(),
                    segment["index"].as_u64().expect("segment index") as usize,
                    segment["isWordLike"].as_bool().expect("word-like status"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{label}: {text:?}");
    }
}

const WORD_PRODUCT_ATOMS: [(&str, &str); 14] = [
    ("latin-a", "A"),
    ("latin-b", "B"),
    ("ascii-digit", "1"),
    ("thai-product", "พัชรี"),
    ("cjk-zhong", "中"),
    ("punctuation-question", "?"),
    ("combining-acute", "\u{0301}"),
    ("arabic-prepend", "\u{0600}"),
    ("punctuation-dot", "."),
    ("punctuation-plus", "+"),
    ("whitespace-space", " "),
    ("underscore", "_"),
    ("unicode-11a3a", "\u{11a3a}"),
    ("unicode-1acf", "\u{1acf}"),
];

#[derive(Clone, Copy)]
struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn byte(&mut self, value: u8) {
        self.0 = (self.0 ^ u64::from(value)).wrapping_mul(Self::PRIME);
    }

    fn u32(&mut self, value: usize) {
        let value = u32::try_from(value).expect("word product coordinate fits u32");
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn utf8(&mut self, value: &str) {
        self.u32(value.len());
        for byte in value.bytes() {
            self.byte(byte);
        }
    }

    fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

fn scalar_utf16_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut offset = 0;
    for scalar in text.chars() {
        offset += scalar.len_utf16();
        boundaries.push(offset);
    }
    boundaries
}

fn observe_word_product_case(text: &str) -> Vec<(usize, usize, usize)> {
    scalar_utf16_boundaries(text)
        .into_iter()
        .map(|cursor| {
            (
                cursor,
                find_word_forward(text, cursor, &WordNavOptions::default()),
                find_word_backward(text, cursor, &WordNavOptions::default()),
            )
        })
        .collect()
}

fn hash_word_product_case(hash: &mut Fnv1a64, text: &str, observations: &[(usize, usize, usize)]) {
    hash.utf8(text);
    hash.u32(observations.len());
    for &(cursor, forward, backward) in observations {
        hash.u32(cursor);
        hash.u32(forward);
        hash.u32(backward);
    }
}

fn cartesian_word(mut index: usize, length: usize) -> String {
    let mut atoms = vec![""; length];
    for position in (0..length).rev() {
        atoms[position] = WORD_PRODUCT_ATOMS[index % WORD_PRODUCT_ATOMS.len()].1;
        index /= WORD_PRODUCT_ATOMS.len();
    }
    atoms.concat()
}

fn contains_thai(text: &str) -> bool {
    text.chars()
        .any(|character| ('\u{0e00}'..='\u{0e7f}').contains(&character))
}

fn fixture_observations(value: &serde_json::Value) -> Vec<(usize, usize, usize)> {
    value
        .as_array()
        .expect("word observations")
        .iter()
        .map(|triple| {
            (
                triple[0].as_u64().expect("cursor") as usize,
                triple[1].as_u64().expect("forward") as usize,
                triple[2].as_u64().expect("backward") as usize,
            )
        })
        .collect()
}

fn fixture_segments(value: &serde_json::Value) -> Vec<WordSegment> {
    value
        .as_array()
        .expect("word segments")
        .iter()
        .map(|segment| WordSegment {
            text: segment["segment"]
                .as_str()
                .expect("segment text")
                .to_owned(),
            is_word_like: segment["isWordLike"].as_bool().expect("word-like status"),
        })
        .collect()
}

fn constructor_boundaries(
    segmenter: icu_segmenter::WordSegmenterBorrowed<'static>,
    text: &str,
) -> Vec<usize> {
    segmenter.segment_str(text).collect()
}

#[test]
fn dictionary_witness_is_exact_and_red_panda_debt_stays_explicit() {
    let receipt = &fixture()["wordProduct"];
    let dictionary = &receipt["dictionaryWitness"];
    let dictionary_text = dictionary["text"].as_str().expect("dictionary text");
    assert_eq!(dictionary_text, "พัชรี");
    assert_eq!(
        default_word_segments(dictionary_text),
        fixture_segments(&dictionary["segments"])
    );
    assert_eq!(
        observe_word_product_case(dictionary_text),
        fixture_observations(&dictionary["observations"])
    );
    assert_eq!(
        constructor_boundaries(
            icu_segmenter::WordSegmenter::new_dictionary(Default::default()),
            dictionary_text,
        ),
        vec![0, 9, 15]
    );
    assert_eq!(
        constructor_boundaries(
            icu_segmenter::WordSegmenter::new_auto(Default::default()),
            dictionary_text,
        ),
        vec![0, 15],
        "the pinned auto constructor must remain a decisive mutation"
    );

    let red_panda = &receipt["redPandaWitness"];
    let red_panda_text = red_panda["text"].as_str().expect("red-panda text");
    assert_eq!(red_panda_text, "แพนด้าแดง");
    assert_eq!(
        default_word_segments(red_panda_text),
        fixture_segments(&red_panda["segments"]),
        "whole-string segmentation is reference-exact"
    );
    assert_ne!(
        observe_word_product_case(red_panda_text),
        fixture_observations(&red_panda["observations"]),
        "interior prefix/suffix navigation remains host-segmenter debt"
    );
    assert_eq!(
        constructor_boundaries(
            icu_segmenter::WordSegmenter::new_dictionary(Default::default()),
            red_panda_text,
        ),
        vec![0, 18, 27]
    );
    assert_eq!(
        constructor_boundaries(
            icu_segmenter::WordSegmenter::new_auto(Default::default()),
            red_panda_text,
        ),
        vec![0, 9, 18, 27]
    );
}

#[test]
fn word_navigation_product_partitions_verified_shell_and_host_residual() {
    let receipt = &fixture()["wordProduct"];
    assert_eq!(
        receipt["algorithm"],
        "FNV-1a-64 over UTF-8 text and LE u32 scalar-boundary/forward/back triples"
    );
    assert_eq!(receipt["lengths"], serde_json::json!([1, 2, 3, 4]));
    assert_eq!(receipt["caseCount"], 41_370);
    assert_eq!(receipt["boundaryCount"], 250_044);
    let expected_atoms = receipt["atoms"].as_array().expect("word product atoms");
    assert_eq!(expected_atoms.len(), WORD_PRODUCT_ATOMS.len());
    for (expected, &(label, text)) in expected_atoms.iter().zip(WORD_PRODUCT_ATOMS.iter()) {
        assert_eq!(expected["label"], label);
        assert_eq!(expected["text"], text);
        assert!(!text.is_empty(), "{label} must remain a non-empty atom");
    }

    let expected_full_buckets = receipt["buckets"].as_array().expect("full Node buckets");
    assert_eq!(expected_full_buckets.len(), 4 * WORD_PRODUCT_ATOMS.len());
    let expected_case_digests = receipt["caseFnv1a64"]
        .as_str()
        .expect("per-case Node digests");
    assert_eq!(expected_case_digests.len(), 41_370 * 16);

    let non_thai = &receipt["nonThai"];
    assert_eq!(
        non_thai["verificationStatus"],
        "verified-against-node-24.4.1-icu-77.1"
    );
    let expected_non_thai_buckets = non_thai["buckets"]
        .as_array()
        .expect("non-Thai Node buckets");
    assert_eq!(
        expected_non_thai_buckets.len(),
        4 * (WORD_PRODUCT_ATOMS.len() - 1)
    );

    let mut non_thai_overall = Fnv1a64::new();
    let mut non_thai_case_count = 0;
    let mut non_thai_boundary_count = 0;
    let mut non_thai_bucket_index = 0;
    let mut residual = Fnv1a64::new();
    let mut residual_case_count = 0;
    let mut saw_repeated_thai = false;
    let mut saw_combining_attachment = false;
    let mut ordinal = 0;

    for length in 1_usize..=4 {
        let cases_per_prefix = WORD_PRODUCT_ATOMS.len().pow((length - 1) as u32);
        for (prefix_index, &(prefix, _)) in WORD_PRODUCT_ATOMS.iter().enumerate() {
            let expected_full = &expected_full_buckets[(length - 1) * 14 + prefix_index];
            assert_eq!(expected_full["length"], length);
            assert_eq!(expected_full["prefix"], prefix);
            assert_eq!(expected_full["caseCount"], cases_per_prefix);

            let mut non_thai_bucket = Fnv1a64::new();
            let mut non_thai_bucket_cases = 0;
            let mut non_thai_bucket_boundaries = 0;
            let first = prefix_index * cases_per_prefix;
            for index in first..first + cases_per_prefix {
                let text = cartesian_word(index, length);
                let observations = observe_word_product_case(&text);
                let mut actual_case = Fnv1a64::new();
                hash_word_product_case(&mut actual_case, &text, &observations);
                let start = ordinal * 16;
                let expected_case = &expected_case_digests[start..start + 16];

                if contains_thai(&text) {
                    if actual_case.hex() != expected_case {
                        residual.utf8(&text);
                        residual.utf8(expected_case);
                        residual_case_count += 1;
                        saw_repeated_thai |= text == "พัชรีพัชรี";
                        saw_combining_attachment |= text == "พัชรี\u{0301}";
                    }
                } else {
                    assert_eq!(
                        actual_case.hex(),
                        expected_case,
                        "ordinary-shell product drift at {text:?}"
                    );
                    hash_word_product_case(&mut non_thai_overall, &text, &observations);
                    hash_word_product_case(&mut non_thai_bucket, &text, &observations);
                    non_thai_case_count += 1;
                    non_thai_boundary_count += observations.len();
                    non_thai_bucket_cases += 1;
                    non_thai_bucket_boundaries += observations.len();
                }
                ordinal += 1;
            }
            if non_thai_bucket_cases > 0 {
                let expected = &expected_non_thai_buckets[non_thai_bucket_index];
                assert_eq!(expected["length"], length);
                assert_eq!(expected["prefix"], prefix);
                assert_eq!(expected["caseCount"], non_thai_bucket_cases);
                assert_eq!(expected["boundaryCount"], non_thai_bucket_boundaries);
                assert_eq!(
                    non_thai_bucket.hex(),
                    expected["fnv1a64"]
                        .as_str()
                        .expect("non-Thai bucket digest"),
                    "non-Thai length {length}, prefix {prefix}"
                );
                non_thai_bucket_index += 1;
            }
        }
    }

    assert_eq!(ordinal, 41_370);
    assert_eq!(non_thai_case_count, 30_940);
    assert_eq!(non_thai_boundary_count, 152_126);
    assert_eq!(non_thai["caseCount"], non_thai_case_count);
    assert_eq!(non_thai["boundaryCount"], non_thai_boundary_count);
    assert_eq!(
        non_thai_overall.hex(),
        non_thai["fnv1a64"].as_str().expect("non-Thai digest")
    );

    let expected_residual = &receipt["defaultFallbackResidual"];
    assert_eq!(
        expected_residual["verificationStatus"],
        "partial-requires-m5-host-intl-segmenter"
    );
    assert!(
        residual_case_count > 0,
        "the partial row must not be mislabeled verified"
    );
    assert!(
        saw_repeated_thai,
        "residual must retain repeated-Thai witness"
    );
    assert!(
        saw_combining_attachment,
        "residual must retain Thai-plus-combining witness"
    );
    assert_eq!(expected_residual["caseCount"], residual_case_count);
    assert_eq!(
        residual.hex(),
        expected_residual["fnv1a64"]
            .as_str()
            .expect("residual digest")
    );
}

#[test]
fn word_navigation_matches_icu_and_utf16_vectors() {
    let mut mismatches = Vec::new();
    for case in fixture()["wordNavigation"]
        .as_array()
        .expect("word-navigation vectors")
    {
        let label = case["label"].as_str().expect("label");
        let text = case["text"].as_str().expect("text");
        let cursor = case["cursor"].as_u64().expect("cursor") as usize;
        let expected = case["result"].as_u64().expect("result") as usize;
        let atomic = case["atomic"].as_bool().unwrap_or(false);
        let segmenter = |value: &str| {
            vec![WordSegment {
                text: value.to_owned(),
                is_word_like: false,
            }]
        };
        let is_atomic = |value: &str| value == text;
        let options = if atomic {
            WordNavOptions {
                segment: Some(&segmenter),
                is_atomic_segment: Some(&is_atomic),
            }
        } else {
            WordNavOptions::default()
        };
        let actual = match case["direction"].as_str().expect("direction") {
            "backward" => find_word_backward(text, cursor, &options),
            "forward" => find_word_forward(text, cursor, &options),
            other => panic!("unknown direction: {other}"),
        };
        if actual != expected {
            mismatches.push(format!(
                "{label}: {text:?} at UTF-16 {cursor}: got {actual}, expected {expected}"
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn editor_model_matches_reference_state_and_effect_traces() {
    for trace in fixture()["editor"].as_array().expect("editor traces") {
        let label = trace["label"].as_str().expect("trace label");
        let mut model = EditorModel::new();
        assert_editor_state(label, "initial", &model.snapshot(), &trace["initial"]);
        for (index, step) in trace["steps"]
            .as_array()
            .expect("editor steps")
            .iter()
            .enumerate()
        {
            let action = editor_action(&step["action"]);
            let effects = model.apply(action);
            let expected_effects = editor_effects(&step["state"]["effects"]);
            assert_eq!(effects, expected_effects, "{label} step {index} effects");
            assert_editor_state(
                label,
                &format!("step {index}"),
                &model.snapshot(),
                &step["state"],
            );
        }
    }
}

#[test]
fn nested_paste_markers_expand_in_registry_order() {
    let mut model = EditorModel::new();
    let first = format!("{}[paste #2 1001 chars]", "x".repeat(1001));
    let second = "y".repeat(1001);
    model.apply(EditorAction::Paste(first));
    model.apply(EditorAction::Paste(second));

    let expanded = model.expanded_text();
    assert!(!expanded.contains("[paste #"));
    assert_eq!(expanded.matches('y').count(), 2002);
    assert_eq!(expanded.encode_utf16().count(), 3003);
}

fn eleven_line_paste() -> String {
    (1..=11)
        .map(|index| format!("line-{index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn visual_ranges(model: &EditorModel) -> Vec<(usize, usize)> {
    model
        .snapshot()
        .visual_lines
        .into_iter()
        .map(|line| (line.start_col, line.start_col + line.length))
        .collect()
}

#[test]
fn owned_wide_marker_wraps_visually_but_moves_atomically() {
    let mut model = EditorModel::new();
    model.apply(EditorAction::SetView { width: 5, rows: 24 });
    model.apply(EditorAction::Paste(eleven_line_paste()));

    assert_eq!(
        model.snapshot().visual_lines,
        vec![
            EditorVisualLine {
                logical_line: 0,
                start_col: 0,
                length: 5,
            },
            EditorVisualLine {
                logical_line: 0,
                start_col: 5,
                length: 5,
            },
            EditorVisualLine {
                logical_line: 0,
                start_col: 10,
                length: 4,
            },
            EditorVisualLine {
                logical_line: 0,
                start_col: 14,
                length: 5,
            },
            EditorVisualLine {
                logical_line: 0,
                start_col: 19,
                length: 1,
            },
        ]
    );
    assert_eq!(model.cursor().col, 20);

    model.apply(EditorAction::MoveUp);
    assert_eq!(model.cursor().col, 0);
    assert_eq!(model.snapshot().snapped_from_cursor_col, Some(15));
}

#[test]
fn wide_marker_retains_outer_boundary_with_prefix_at_width_two() {
    let mut model = EditorModel::new();
    model.apply(EditorAction::SetView { width: 2, rows: 24 });
    model.apply(EditorAction::SetText("a".to_owned()));
    model.apply(EditorAction::Paste(eleven_line_paste()));

    assert_eq!(model.text(), "a[paste #1 +11 lines]");
    assert_eq!(
        visual_ranges(&model),
        vec![
            (0, 1),
            (1, 3),
            (3, 5),
            (5, 7),
            (7, 8),
            (8, 10),
            (10, 11),
            (11, 13),
            (13, 15),
            (15, 17),
            (17, 19),
            (19, 21),
        ]
    );

    model.apply(EditorAction::MoveUp);
    assert_eq!(model.cursor().col, 1);
    assert_eq!(model.snapshot().snapped_from_cursor_col, Some(18));
    model.apply(EditorAction::MoveUp);
    assert_eq!(model.cursor().col, 0);
}

#[test]
fn adjacent_owned_markers_do_not_share_visual_subchunks() {
    let mut model = EditorModel::new();
    model.apply(EditorAction::SetView { width: 5, rows: 20 });
    model.apply(EditorAction::Paste(eleven_line_paste()));
    model.apply(EditorAction::Paste(eleven_line_paste()));

    assert_eq!(
        visual_ranges(&model),
        vec![
            (0, 5),
            (5, 10),
            (10, 14),
            (14, 19),
            (19, 20),
            (20, 25),
            (25, 30),
            (30, 34),
            (34, 39),
            (39, 40),
        ]
    );
}

#[test]
fn width_one_page_up_preserves_zero_pre_snap_coordinate() {
    let mut model = EditorModel::new();
    model.apply(EditorAction::SetView { width: 1, rows: 20 });
    model.apply(EditorAction::Paste(eleven_line_paste()));
    model.apply(EditorAction::PageUp);
    assert_eq!(model.snapshot().snapped_from_cursor_col, Some(13));
    model.apply(EditorAction::PageUp);
    assert_eq!(model.cursor().col, 0);
    assert_eq!(model.snapshot().snapped_from_cursor_col, Some(0));
}

fn editor_action(value: &serde_json::Value) -> EditorAction {
    let text = || value["text"].as_str().expect("action text").to_owned();
    match value["type"].as_str().expect("action type") {
        "set_text" => EditorAction::SetText(text()),
        "insert_text" => EditorAction::InsertText(text()),
        "type" => EditorAction::Type(text()),
        "paste" => EditorAction::Paste(text()),
        "new_line" => EditorAction::NewLine,
        "submit" => EditorAction::Submit,
        "backspace" => EditorAction::Backspace,
        "delete_forward" => EditorAction::DeleteForward,
        "line_start" => EditorAction::LineStart,
        "line_end" => EditorAction::LineEnd,
        "delete_line_start" => EditorAction::DeleteLineStart,
        "delete_line_end" => EditorAction::DeleteLineEnd,
        "delete_word_backward" => EditorAction::DeleteWordBackward,
        "delete_word_forward" => EditorAction::DeleteWordForward,
        "move_left" => EditorAction::MoveLeft,
        "move_right" => EditorAction::MoveRight,
        "move_up" => EditorAction::MoveUp,
        "move_down" => EditorAction::MoveDown,
        "move_word_backward" => EditorAction::MoveWordBackward,
        "move_word_forward" => EditorAction::MoveWordForward,
        "page_up" => EditorAction::PageUp,
        "page_down" => EditorAction::PageDown,
        "jump_backward" => EditorAction::JumpBackward(text()),
        "jump_forward" => EditorAction::JumpForward(text()),
        "add_history" => EditorAction::AddHistory(text()),
        "history_previous" => EditorAction::HistoryPrevious,
        "history_next" => EditorAction::HistoryNext,
        "yank" => EditorAction::Yank,
        "yank_pop" => EditorAction::YankPop,
        "undo" => EditorAction::Undo,
        "set_view" => EditorAction::SetView {
            width: value["width"].as_u64().expect("view width") as usize,
            rows: value["rows"].as_u64().expect("view rows") as usize,
        },
        other => panic!("unknown editor action: {other}"),
    }
}

fn editor_effects(value: &serde_json::Value) -> Vec<EditorEffect> {
    value
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| {
            let text = effect["text"].as_str().expect("effect text").to_owned();
            match effect["type"].as_str().expect("effect type") {
                "change" => EditorEffect::Change(text),
                "submit" => EditorEffect::Submit(text),
                other => panic!("unknown editor effect: {other}"),
            }
        })
        .collect()
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|entry| entry.as_str().expect("string").to_owned())
        .collect()
}

fn assert_editor_state(
    trace: &str,
    step: &str,
    actual: &EditorModelSnapshot,
    expected: &serde_json::Value,
) {
    assert_eq!(actual.text, expected["text"], "{trace} {step} text");
    assert_eq!(
        actual.expanded_text, expected["expandedText"],
        "{trace} {step} expanded text"
    );
    assert_eq!(
        actual.lines,
        strings(&expected["lines"]),
        "{trace} {step} lines"
    );
    assert_eq!(
        actual.cursor.line,
        expected["cursor"]["line"].as_u64().expect("cursor line") as usize,
        "{trace} {step} cursor line"
    );
    assert_eq!(
        actual.cursor.col,
        expected["cursor"]["col"].as_u64().expect("cursor col") as usize,
        "{trace} {step} UTF-16 cursor column"
    );
    let expected_pastes = expected["pastes"]
        .as_array()
        .expect("pastes")
        .iter()
        .map(|entry| {
            (
                entry[0].as_u64().expect("paste id") as u32,
                entry[1].as_str().expect("paste text").to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.pastes, expected_pastes, "{trace} {step} pastes");
    assert_eq!(
        actual.paste_counter,
        expected["pasteCounter"].as_u64().expect("paste counter") as u32,
        "{trace} {step} paste counter"
    );
    assert_eq!(
        actual.history,
        strings(&expected["history"]),
        "{trace} {step} history"
    );
    assert_eq!(
        actual.history_index,
        expected["historyIndex"].as_i64().expect("history index") as isize,
        "{trace} {step} history index"
    );
    assert_eq!(
        actual.kill_length,
        expected["killLength"].as_u64().expect("kill length") as usize,
        "{trace} {step} kill length"
    );
    assert_eq!(
        actual.kill_peek.as_deref(),
        expected["killPeek"].as_str(),
        "{trace} {step} kill peek"
    );
    assert_eq!(
        actual.undo_length,
        expected["undoLength"].as_u64().expect("undo length") as usize,
        "{trace} {step} undo length"
    );
    assert_eq!(
        actual.last_action.as_deref(),
        expected["lastAction"].as_str(),
        "{trace} {step} last action"
    );
    assert_eq!(
        actual.preferred_visual_col,
        expected["preferredVisualCol"]
            .as_u64()
            .map(|value| value as usize),
        "{trace} {step} preferred visual column"
    );
    assert_eq!(
        actual.snapped_from_cursor_col,
        expected["snappedFromCursorCol"]
            .as_u64()
            .map(|value| value as usize),
        "{trace} {step} pre-snap UTF-16 cursor column"
    );
    let expected_visual_lines = expected["visualLines"]
        .as_array()
        .expect("visual lines")
        .iter()
        .map(|line| EditorVisualLine {
            logical_line: line["logicalLine"].as_u64().expect("logical line") as usize,
            start_col: line["startCol"].as_u64().expect("visual start") as usize,
            length: line["length"].as_u64().expect("visual length") as usize,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual.visual_lines, expected_visual_lines,
        "{trace} {step} visual lines"
    );
}
