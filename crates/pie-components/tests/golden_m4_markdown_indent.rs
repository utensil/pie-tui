//! Parameterized marked18 list-indentation compatibility matrix.

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
fn marked_ordered_interrupt_indentation_matrix_is_exact() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-markdown-indent-matrix.json"))
            .expect("indentation matrix fixture is valid JSON");
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

    let cases = fixture["cases"].as_array().expect("matrix cases");
    assert_eq!(cases.len(), 21);
    let mut coordinates = BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let indent = case["indent"].as_u64().expect("indent") as usize;
        let marker = case["marker"].as_str().expect("marker");
        assert!((3..=9).contains(&indent));
        assert!(matches!(marker, "4" | "12" | "321"));
        assert!(coordinates.insert((indent, marker)));

        let source = case["source"].as_str().expect("source");
        let mut component = Markdown::new(
            source,
            0,
            0,
            identity_theme(),
            None,
            MarkdownOptions::default(),
        );
        let actual = vec![component.render(40)];
        let expected: Vec<Vec<String>> =
            serde_json::from_value(case["outputs"].clone()).expect("expected outputs");
        if actual != expected {
            failures.push(format!(
                "indent {indent}, marker {marker}: expected {expected:?}, actual {actual:?}"
            ));
        }
    }
    assert_eq!(coordinates.len(), 21);
    assert!(
        failures.is_empty(),
        "{} marked indentation mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
