//! Black-box differential vectors for the M4 Markdown component.

use std::sync::{Arc, Mutex};

use pie_components::{
    Component, DefaultTextStyle, Markdown, MarkdownOptions, MarkdownTheme, StyleFn,
};

fn identity() -> StyleFn {
    Box::new(str::to_string)
}

fn ansi(open: &'static str, close: &'static str) -> StyleFn {
    Box::new(move |text| format!("\x1b[{open}m{text}\x1b[{close}m"))
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

fn styled_theme(events: Arc<Mutex<Vec<String>>>) -> MarkdownTheme {
    MarkdownTheme {
        heading: ansi("31", "39"),
        link: ansi("36", "39"),
        link_url: ansi("34", "39"),
        code: ansi("33", "39"),
        code_block: ansi("35", "39"),
        code_block_border: ansi("2", "22"),
        quote: ansi("32", "39"),
        quote_border: ansi("92", "39"),
        hr: ansi("90", "39"),
        list_bullet: ansi("95", "39"),
        bold: ansi("1", "22"),
        italic: ansi("3", "23"),
        strikethrough: ansi("9", "29"),
        underline: ansi("4", "24"),
        highlight_code: Some(Box::new(move |code, language| {
            events
                .lock()
                .unwrap()
                .push(format!("highlight:{}:{code}", language.unwrap_or_default()));
            code.split('\n')
                .map(|line| format!("\x1b[96m{line}\x1b[39m"))
                .collect()
        })),
        code_block_indent: Some("» ".to_string()),
    }
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/m4-markdown.json"))
        .expect("m4-markdown.json is valid JSON")
}

fn adversarial_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/m4-markdown-adversarial.json"))
        .expect("m4-markdown-adversarial.json is valid JSON")
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

fn record_adversarial(
    name: &str,
    outputs: Vec<Vec<String>>,
    events: Option<Vec<String>>,
    failures: &mut Vec<String>,
    seen: &mut Vec<String>,
) {
    let fixture = adversarial_fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing adversarial fixture case {name}"));
    let expected_outputs: Vec<Vec<String>> =
        serde_json::from_value(case["outputs"].clone()).unwrap();
    if outputs != expected_outputs {
        failures.push(format!(
            "{name} output: expected {expected_outputs:?}, actual {outputs:?}"
        ));
    }
    if let Some(events) = events {
        let expected_events: Vec<String> = serde_json::from_value(case["events"].clone()).unwrap();
        if events != expected_events {
            failures.push(format!(
                "{name} events: expected {expected_events:?}, actual {events:?}"
            ));
        }
    }
    seen.push(name.to_string());
}

fn check(name: &str, outputs: Vec<Vec<String>>, events: Option<Vec<String>>) {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing fixture case {name}"));
    let expected: Vec<Vec<String>> = serde_json::from_value(case["outputs"].clone()).unwrap();
    assert_eq!(outputs, expected, "{name}: render differential");
    if let Some(events) = events {
        let expected: Vec<String> = serde_json::from_value(case["events"].clone()).unwrap();
        assert_eq!(events, expected, "{name}: callback order");
    }
}

#[test]
fn markdown_reference_provenance_is_exact() {
    let fixture = fixture();
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(fixture["reference"]["markedVersion"], "18.0.5");
    assert_eq!(
        fixture["reference"]["indexDtsSha256"],
        "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3"
    );
    assert_eq!(
        fixture["reference"]["markdownDtsSha256"],
        "9c21b4bcb0b0b047438616cb2302b6b6df7630e73e0ee8da3f9f6bae7e565f66"
    );
    assert_eq!(
        fixture["reference"]["markdownJsSha256"],
        "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465"
    );
    assert_eq!(
        fixture["reference"]["markedEsmSha256"],
        "43e1fc0927b2d397bdc786c0a9efa8414ce18e7781d0b3490faceea35b7d0d15"
    );
}

#[test]
fn markdown_plain_blocks_lists_options_and_tables_match_reference() {
    let mut empty = Markdown::new("", 0, 0, identity_theme(), None, MarkdownOptions::default());
    check("empty-default", vec![empty.render(20)], None);

    let mut paragraphs = Markdown::new(
        "Alpha beta gamma delta epsilon.\n\nSecond paragraph.",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    check(
        "paragraph-wrap-resize",
        vec![paragraphs.render(18), paragraphs.render(28)],
        None,
    );

    let mut list = Markdown::new(
        "- first item\n  - nested item\n- final item with wrapping words",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    check("nested-unordered-list", vec![list.render(22)], None);

    for (name, preserve) in [("ordered-normalized", false), ("ordered-preserved", true)] {
        let mut component = Markdown::new(
            "3. third\n7. seventh",
            0,
            0,
            identity_theme(),
            None,
            MarkdownOptions {
                preserve_ordered_list_markers: preserve,
                ..MarkdownOptions::default()
            },
        );
        check(name, vec![component.render(24)], None);
    }

    for (name, preserve) in [
        ("backslash-escape-default", false),
        ("backslash-escape-preserved", true),
    ] {
        let mut component = Markdown::new(
            "Escaped \\*star\\* and \\[bracket\\].",
            0,
            0,
            identity_theme(),
            None,
            MarkdownOptions {
                preserve_backslash_escapes: preserve,
                ..MarkdownOptions::default()
            },
        );
        check(name, vec![component.render(40)], None);
    }

    let mut table = Markdown::new(
        "| Name | Description |\n| --- | --- |\n| alpha | a description with wrapping words |\n| beta | short |",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    check(
        "table-wide-narrow",
        vec![table.render(46), table.render(25)],
        None,
    );
}

#[test]
fn markdown_styling_quotes_code_and_latex_match_reference() {
    let mut default_code = Markdown::new(
        "```\nraw code\n```",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    check(
        "fenced-code-default-indent",
        vec![default_code.render(20)],
        None,
    );

    let no_events = Arc::new(Mutex::new(Vec::new()));
    let mut styled = Markdown::new(
        "# Heading one\n\nText with **bold**, *italic*, ~~strike~~, `code`, and [link](https://example.test).",
        0,
        0,
        styled_theme(no_events),
        None,
        MarkdownOptions::default(),
    );
    check("headings-inline-link", vec![styled.render(72)], None);

    let no_events = Arc::new(Mutex::new(Vec::new()));
    let mut quote = Markdown::new(
        "> quoted text that wraps across rows\n> second line\n\n---",
        0,
        0,
        styled_theme(no_events),
        None,
        MarkdownOptions::default(),
    );
    check("quote-rule", vec![quote.render(24)], None);

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut code = Markdown::new(
        "```js\nconst x = 1;\nconsole.log(x);\n```",
        0,
        0,
        styled_theme(events.clone()),
        None,
        MarkdownOptions::default(),
    );
    let outputs = vec![code.render(36)];
    let captured = events.lock().unwrap().clone();
    check("fenced-code-highlight", outputs, Some(captured));

    let mut latex = Markdown::new(
        "Inline $x_{i+1}^2$ and display:\n\n$$\\frac{a+b}{c-d}$$",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    check("latex-default", vec![latex.render(38)], None);

    let mut latex_disabled = Markdown::new(
        "Inline $x_{i+1}^2$ and $$\\frac{a}{b}$$.",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions {
            render_latex: false,
            ..MarkdownOptions::default()
        },
    );
    check("latex-disabled", vec![latex_disabled.render(50)], None);
}

#[test]
fn markdown_default_style_background_cache_and_callback_order_match_reference() {
    let no_events = Arc::new(Mutex::new(Vec::new()));
    let mut styled = Markdown::new(
        "Styled **body**.",
        2,
        1,
        styled_theme(no_events),
        Some(DefaultTextStyle {
            color: Some(ansi("37", "39")),
            bg_color: Some(ansi("44", "49")),
            bold: true,
            italic: true,
            strikethrough: true,
            underline: true,
        }),
        MarkdownOptions::default(),
    );
    check(
        "padding-default-style-background",
        vec![styled.render(28)],
        None,
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let transform_events = events.clone();
    let mut cached = Markdown::new(
        "```txt\nseed\n```",
        1,
        0,
        styled_theme(events.clone()),
        None,
        MarkdownOptions {
            transform: Some(Box::new(move |markdown, available_width| {
                transform_events
                    .lock()
                    .unwrap()
                    .push(format!("transform:{available_width}:{markdown}"));
                markdown.to_string()
            })),
            ..MarkdownOptions::default()
        },
    );
    let mut outputs = vec![cached.render(20), cached.render(20), cached.render(16)];
    cached.invalidate();
    outputs.push(cached.render(16));
    cached.set_text("plain replacement");
    outputs.push(cached.render(16));
    let captured = events.lock().unwrap().clone();
    check("transform-cache-invalidate-order", outputs, Some(captured));
}

#[test]
fn markdown_adversarial_matrix_matches_pinned_reference() {
    let fixture = adversarial_fixture();
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

    let mut failures = Vec::new();
    let mut seen = Vec::new();

    let mut component = Markdown::new(
        "alpha\tbeta\tomega",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "tabs-paragraph",
        vec![component.render(30)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "-\talpha\tbeta\n-\tomega",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "tabs-list",
        vec![component.render(30)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "| Key\t| Value |\n| --- | --- |\n| a\tb | c\td |",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "tabs-table",
        vec![component.render(32)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "```txt\n\talpha\tbeta\n```",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "tabs-fence",
        vec![component.render(28)],
        None,
        &mut failures,
        &mut seen,
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let transform_events = events.clone();
    let mut component = Markdown::new(
        "seed",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions {
            transform: Some(Box::new(move |_markdown, width| {
                transform_events
                    .lock()
                    .unwrap()
                    .push(format!("transform:{width}"));
                "alpha\tbeta".to_string()
            })),
            ..MarkdownOptions::default()
        },
    );
    let outputs = vec![component.render(20)];
    let captured = events.lock().unwrap().clone();
    record_adversarial(
        "tabs-after-transform",
        outputs,
        Some(captured),
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "Primary\n=======\n\nSecondary\n---------",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "setext-headings",
        vec![component.render(28)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "__bold__ _italic_ intraword_a_b",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "underscore-emphasis",
        vec![component.render(44)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "<https://example.test> <user@example.test>",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "autolinks-url-email",
        vec![component.render(72)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "[label](https://example.test \"Example title\")",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "link-title",
        vec![component.render(60)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "![diagram](image.png \"Caption\")",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "image-alt-title",
        vec![component.render(50)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "` x ` and ``a`b`` and `a  b`",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "code-span-delimiters",
        vec![component.render(48)],
        None,
        &mut failures,
        &mut seen,
    );

    for (name, text) in [
        ("soft-break", "soft line\nnext line"),
        ("hard-break-spaces", "hard line  \nnext line"),
        ("hard-break-backslash", "slash line\\\nnext line"),
    ] {
        let mut component = Markdown::new(
            text,
            0,
            0,
            identity_theme(),
            None,
            MarkdownOptions::default(),
        );
        record_adversarial(
            name,
            vec![component.render(24)],
            None,
            &mut failures,
            &mut seen,
        );
    }

    let mut component = Markdown::new(
        "    alpha\n    beta",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "indented-code",
        vec![component.render(28)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "````js\nconst ticks = ```;\n````",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "long-fence",
        vec![component.render(36)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "1. outer\n   - inner\n     4. deep\n2. tail",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "nested-mixed-lists",
        vec![component.render(28)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "> outer\n>> inner\n> tail",
        0,
        0,
        styled_theme(Arc::new(Mutex::new(Vec::new()))),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "nested-quotes",
        vec![component.render(28)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "| Key | Value |\n| --- | --- |\n| a\\|b | `c|d` |",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "escaped-pipe-table",
        vec![component.render(30)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut component = Markdown::new(
        "before\n\n$$\n\\frac{a+b}{c-d}\n+ \\sum_{i=1}^{n} i\n$$\n\nafter",
        0,
        0,
        identity_theme(),
        None,
        MarkdownOptions::default(),
    );
    record_adversarial(
        "multiline-display-math",
        vec![component.render(36)],
        None,
        &mut failures,
        &mut seen,
    );

    let mut theme = identity_theme();
    theme.code = ansi("33", "39");
    let mut component = Markdown::new(
        "plain `code` tail",
        0,
        0,
        theme,
        Some(DefaultTextStyle {
            color: Some(ansi("37", "39")),
            ..DefaultTextStyle::default()
        }),
        MarkdownOptions::default(),
    );
    record_adversarial(
        "inline-code-theme-only",
        vec![component.render(32)],
        None,
        &mut failures,
        &mut seen,
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut theme = identity_theme();
    theme.code_block_border = logged_ansi(events.clone(), "border", "2", "22");
    let highlight_events = events.clone();
    theme.highlight_code = Some(Box::new(move |code, language| {
        highlight_events.lock().unwrap().push(format!(
            "highlight:{}:{}",
            language.unwrap_or_default(),
            encode_callback_text(code)
        ));
        code.split('\n').map(str::to_string).collect()
    }));
    let mut component = Markdown::new(
        "```js\none\ntwo\n```",
        0,
        0,
        theme,
        None,
        MarkdownOptions::default(),
    );
    let outputs = vec![component.render(24)];
    let captured = events.lock().unwrap().clone();
    record_adversarial(
        "code-callback-order",
        outputs,
        Some(captured),
        &mut failures,
        &mut seen,
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut component = Markdown::new(
        "one\n\ntwo",
        1,
        1,
        identity_theme(),
        Some(DefaultTextStyle {
            bg_color: Some(logged_ansi(events.clone(), "background", "44", "49")),
            ..DefaultTextStyle::default()
        }),
        MarkdownOptions::default(),
    );
    let outputs = vec![component.render(12)];
    let captured = events.lock().unwrap().clone();
    record_adversarial(
        "background-callback-order",
        outputs,
        Some(captured),
        &mut failures,
        &mut seen,
    );

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut theme = identity_theme();
    theme.bold = logged_ansi(events.clone(), "bold", "1", "22");
    let mut component = Markdown::new(
        "a **b** c",
        0,
        0,
        theme,
        Some(DefaultTextStyle {
            color: Some(logged_ansi(events.clone(), "color", "37", "39")),
            bold: true,
            ..DefaultTextStyle::default()
        }),
        MarkdownOptions::default(),
    );
    let outputs = vec![
        component.render(24),
        component.render(24),
        component.render(20),
    ];
    let captured = events.lock().unwrap().clone();
    record_adversarial(
        "default-prefix-nul-cache",
        outputs,
        Some(captured),
        &mut failures,
        &mut seen,
    );

    let mut expected_names: Vec<String> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["name"].as_str().unwrap().to_string())
        .collect();
    expected_names.sort();
    seen.sort();
    assert_eq!(
        seen, expected_names,
        "every adversarial fixture is exercised"
    );
    assert!(
        failures.is_empty(),
        "{} adversarial Markdown mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
