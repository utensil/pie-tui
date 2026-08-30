//! Exact default-style callback behavior for Markdown LaTeX routes.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use pie_components::{
    Component, DefaultTextStyle, Markdown, MarkdownOptions, MarkdownTheme, StyleFn,
};

fn identity() -> StyleFn {
    Box::new(str::to_string)
}

fn encode_callback_text(text: &str) -> String {
    text.replace('\0', "<NUL>")
        .replace('\n', "<LF>")
        .replace('\t', "<TAB>")
        .replace('\x1b', "<ESC>")
}

fn logged_ansi(
    events: Arc<Mutex<Vec<String>>>,
    name: &'static str,
    open: &'static str,
    close: &'static str,
) -> StyleFn {
    Box::new(move |text| {
        events
            .lock()
            .unwrap()
            .push(format!("{name}:{}", encode_callback_text(text)));
        format!("\x1b[{open}m{text}\x1b[{close}m")
    })
}

fn math_style_theme(events: Arc<Mutex<Vec<String>>>) -> MarkdownTheme {
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
        bold: logged_ansi(events, "bold", "1", "22"),
        italic: identity(),
        strikethrough: identity(),
        underline: identity(),
        highlight_code: None,
        code_block_indent: None,
    }
}

#[test]
fn markdown_math_routes_apply_default_style_with_exact_lazy_prefix_order() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-markdown-math-style.json"))
            .expect("math-style fixture is valid JSON");
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

    let cases = fixture["cases"].as_array().expect("math-style cases");
    assert_eq!(cases.len(), 4);
    let mut names = BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        assert!(names.insert(name));
        let source = case["source"].as_str().expect("source");
        let width = case["width"].as_u64().expect("width") as usize;
        let expected_outputs: Vec<Vec<String>> =
            serde_json::from_value(case["outputs"].clone()).expect("expected outputs");
        let expected_events: Vec<String> =
            serde_json::from_value(case["events"].clone()).expect("expected events");
        let events = Arc::new(Mutex::new(Vec::new()));
        let transform_events = events.clone();
        let mut component = Markdown::new(
            source,
            0,
            0,
            math_style_theme(events.clone()),
            Some(DefaultTextStyle {
                color: Some(logged_ansi(events.clone(), "color", "37", "39")),
                bold: true,
                ..DefaultTextStyle::default()
            }),
            MarkdownOptions {
                transform: Some(Box::new(move |markdown, available_width| {
                    transform_events.lock().unwrap().push(format!(
                        "transform:{available_width}:{}",
                        encode_callback_text(markdown)
                    ));
                    markdown.to_string()
                })),
                ..MarkdownOptions::default()
            },
        );

        let actual_outputs = vec![component.render(width)];
        let actual_events = events.lock().unwrap().clone();
        if actual_outputs != expected_outputs {
            failures.push(format!(
                "{name} output: expected {expected_outputs:?}, actual {actual_outputs:?}"
            ));
        }
        if actual_events != expected_events {
            failures.push(format!(
                "{name} callbacks: expected {expected_events:?}, actual {actual_events:?}"
            ));
        }
    }
    assert_eq!(names.len(), 4);
    assert!(
        failures.is_empty(),
        "{} math-style mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn markdown_math_style_mutation_receipt_covers_both_repair_families() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/m4-markdown-math-style-mutation-receipt.json"
    ))
    .expect("math-style mutation receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "pie-tui-m4-markdown-math-style-mutation-receipt-v1"
    );
    assert_eq!(receipt["oracleCommit"], "a6af758");
    assert_eq!(receipt["productCommit"], "6b88ffe");
    let mutations = receipt["mutations"].as_array().expect("mutation rows");
    assert_eq!(mutations.len(), 2);
    assert!(mutations.iter().all(|mutation| mutation["exitCode"] == 101));
    assert_eq!(
        mutations
            .iter()
            .map(|mutation| mutation["family"].as_str().expect("mutation family"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["lazy-restoration-prefix", "math-default-style"])
    );
    assert_eq!(mutations[0]["mismatchCount"], 8);
    assert_eq!(mutations[1]["mismatchCount"], 2);
}
