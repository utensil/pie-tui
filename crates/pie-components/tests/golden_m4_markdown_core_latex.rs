//! Markdown routes through the canonical LaTeX model instead of a local parser.

use std::collections::BTreeSet;

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
fn markdown_routes_scalar_safe_math_through_the_canonical_latex_model() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-markdown-core-latex.json"))
            .expect("core-LaTeX fixture is valid JSON");
    assert_eq!(fixture["generator"], "gen-golden-m4-markdown.mjs");
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(fixture["reference"]["markedVersion"], "18.0.5");
    assert_eq!(
        fixture["reference"]["markdownJsSha256"],
        "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465"
    );
    assert_eq!(
        fixture["reference"]["latexJsSha256"],
        "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716"
    );
    assert_eq!(
        fixture["reference"]["utilsJsSha256"],
        "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052"
    );
    assert_eq!(
        fixture["reference"]["terminalImageJsSha256"],
        "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2"
    );
    assert_eq!(
        fixture["reference"]["eastAsianWidthDataSha256"],
        "f6b40f86c9a2a6808ec808fa8ddcb8da261254cc6121d37ffaeb2bf35dad1d5b"
    );
    assert_eq!(fixture["reference"]["node"], "24.4.1");
    assert_eq!(fixture["reference"]["icu"], "77.1");
    assert_eq!(fixture["reference"]["unicode"], "16.0");

    let cases = fixture["cases"].as_array().expect("core-LaTeX cases");
    assert_eq!(cases.len(), 7);
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
        let actual = vec![component.render(width)];
        if actual != expected {
            failures.push(format!("{name}: expected {expected:?}, actual {actual:?}"));
        }
    }
    assert_eq!(names.len(), 7);
    assert!(
        failures.is_empty(),
        "{} canonical-LaTeX route mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn canonical_latex_route_mutation_receipt_is_non_vacuous() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/m4-markdown-core-latex-mutation-receipt.json"
    ))
    .expect("core-LaTeX mutation receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "pie-tui-m4-markdown-core-latex-mutation-receipt-v1"
    );
    let mutations = receipt["mutations"].as_array().expect("mutation rows");
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0]["family"], "bypass-canonical-latex");
    assert_eq!(mutations[0]["exitCode"], 101);
    assert_eq!(mutations[0]["mismatchCount"], 6);
    assert_eq!(mutations[1]["family"], "latex-import-provenance");
    assert_eq!(mutations[1]["exitCode"], 1);
}
