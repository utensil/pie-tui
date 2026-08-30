//! Final independent-review vectors for source-preserving Markdown behavior.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use pie_components::{Component, Markdown, MarkdownOptions, MarkdownTheme, StyleFn};

fn identity() -> StyleFn {
    Box::new(str::to_string)
}

fn identity_theme() -> MarkdownTheme {
    MarkdownTheme {
        heading: identity(),
        link: identity(),
        link_url: identity(),
        code: identity(),
        code_block: identity(),
        code_block_border: identity(),
        quote: identity(),
        quote_border: identity(),
        hr: identity(),
        list_bullet: identity(),
        bold: identity(),
        italic: identity(),
        strikethrough: identity(),
        underline: identity(),
        highlight_code: None,
        code_block_indent: None,
    }
}

#[test]
fn final_review_source_preservation_vectors_are_exact_and_panic_free() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-markdown-final-review.json"))
            .expect("final-review fixture is valid JSON");
    assert_eq!(fixture["generator"], "gen-golden-m4-markdown.mjs");
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(fixture["reference"]["markedVersion"], "18.0.5");
    assert_eq!(
        fixture["reference"]["markdownJsSha256"],
        "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465"
    );
    assert_eq!(
        fixture["reference"]["markedEsmSha256"],
        "43e1fc0927b2d397bdc786c0a9efa8414ce18e7781d0b3490faceea35b7d0d15"
    );

    let cases = fixture["cases"].as_array().expect("final-review cases");
    assert_eq!(cases.len(), 10);
    let mut names = BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        assert!(names.insert(name));
        let source = case["source"].as_str().expect("source");
        let width = case["width"].as_u64().expect("width") as usize;
        let expected: Vec<Vec<String>> =
            serde_json::from_value(case["outputs"].clone()).expect("expected outputs");
        let mut component = Markdown::new(
            source,
            0,
            0,
            identity_theme(),
            None,
            MarkdownOptions::default(),
        );
        match catch_unwind(AssertUnwindSafe(|| component.render(width))) {
            Ok(actual) => {
                let actual = vec![actual];
                if actual != expected {
                    failures.push(format!("{name}: expected {expected:?}, actual {actual:?}"));
                }
            }
            Err(_) => failures.push(format!("{name}: render panicked")),
        }
    }
    assert_eq!(names.len(), 10);
    assert!(
        failures.is_empty(),
        "{} final-review mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn final_review_mutation_receipt_covers_each_repair_family() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/m4-markdown-final-review-mutation-receipt.json"
    ))
    .expect("final-review mutation receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "pie-tui-m4-markdown-final-review-mutation-receipt-v1"
    );
    let mutations = receipt["mutations"].as_array().expect("mutation rows");
    assert_eq!(mutations.len(), 4);
    let families = mutations
        .iter()
        .map(|mutation| mutation["family"].as_str().expect("mutation family"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        families,
        BTreeSet::from([
            "inline-math-fallback",
            "raw-entities-html",
            "task-markers",
            "unicode-source-boundaries",
        ])
    );
    assert!(mutations.iter().all(|mutation| mutation["exitCode"] == 101));
}
