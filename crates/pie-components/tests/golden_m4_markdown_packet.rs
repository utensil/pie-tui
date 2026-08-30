//! Independent adversarial packet for the M4 Markdown component.

use std::sync::{Arc, Mutex};

use pie_components::{
    Component, DefaultTextStyle, Markdown, MarkdownOptions, MarkdownTheme, StyleFn,
};

const PACKET_SHA256: &str = "46c35adecf350e79f8c37163c2d88b38a32672da03d30c21066e5149f23468d5";

fn packet() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/m4-markdown-evidence-packet.json"))
        .expect("Markdown evidence packet is valid JSON")
}

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

fn encode_callback_text(text: &str) -> String {
    text.replace('\0', "<NUL>")
        .replace('\n', "<LF>")
        .replace('\t', "<TAB>")
}

fn logged_identity(events: Arc<Mutex<Vec<String>>>, name: &'static str) -> StyleFn {
    Box::new(move |text| {
        events
            .lock()
            .unwrap()
            .push(format!("{name}:{}", encode_callback_text(text)));
        text.to_string()
    })
}

fn logged_theme(events: Arc<Mutex<Vec<String>>>) -> MarkdownTheme {
    MarkdownTheme {
        heading: logged_identity(events.clone(), "heading"),
        link: logged_identity(events.clone(), "link"),
        link_url: logged_identity(events.clone(), "linkUrl"),
        code: logged_identity(events.clone(), "code"),
        code_block: logged_identity(events.clone(), "codeBlock"),
        code_block_border: logged_identity(events.clone(), "border"),
        quote: logged_identity(events.clone(), "quote"),
        quote_border: logged_identity(events.clone(), "quoteBorder"),
        hr: logged_identity(events.clone(), "hr"),
        list_bullet: logged_identity(events.clone(), "bullet"),
        bold: logged_identity(events.clone(), "bold"),
        italic: logged_identity(events.clone(), "italic"),
        strikethrough: logged_identity(events.clone(), "strike"),
        underline: logged_identity(events, "underline"),
        highlight_code: None,
        code_block_indent: None,
    }
}

fn expected_outputs(case: &serde_json::Value) -> Vec<Vec<String>> {
    serde_json::from_value(case["expectedOutputs"].clone()).expect("expected output matrix")
}

fn run_operations(
    component: &mut Markdown,
    source: &str,
    operations: &serde_json::Value,
    events: Option<&Arc<Mutex<Vec<String>>>>,
) -> Vec<Vec<String>> {
    let mut outputs = Vec::new();
    for operation in operations.as_array().expect("operations array") {
        match operation["op"].as_str().expect("operation name") {
            "construct" => {}
            "pushEvent" => events
                .expect("pushEvent requires event sink")
                .lock()
                .unwrap()
                .push(operation["event"].as_str().unwrap().to_string()),
            "render" => {
                outputs.push(component.render(operation["width"].as_u64().unwrap() as usize))
            }
            "invalidate" => component.invalidate(),
            "setText" => {
                let value = operation["value"].as_str().unwrap();
                component.set_text(if value == "same source" {
                    source
                } else {
                    value
                });
            }
            other => panic!("unsupported packet operation {other}"),
        }
    }
    outputs
}

#[test]
fn packet_schema_provenance_and_cardinality_are_exact() {
    let packet = packet();
    assert_eq!(
        packet["schema"],
        "pie-tui-m4-markdown-adversarial-evidence-v1"
    );
    assert_eq!(packet["reference"]["version"], "0.84.1");
    assert_eq!(packet["reference"]["markedVersion"], "18.0.5");
    assert_eq!(
        packet["reference"]["markdownJsSha256"],
        "bbffe68aa6bb6968e9eca2681e19b0ea7787fff8124ef996fae282a4c2201465"
    );
    assert_eq!(
        packet["reference"]["markedEsmSha256"],
        "43e1fc0927b2d397bdc786c0a9efa8414ce18e7781d0b3490faceea35b7d0d15"
    );
    assert_eq!(packet["priorThreeOfThirtyTwo"].as_array().unwrap().len(), 3);
    assert_eq!(
        packet["additionalTenOfFifteen"].as_array().unwrap().len(),
        10
    );
    assert_eq!(
        packet["constructionTransformCallbackLifecycle"]["expectedOutputs"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        packet["constructionTransformCallbackLifecycle"]["expectedEvents"]
            .as_array()
            .unwrap()
            .len(),
        231
    );
    assert_eq!(
        PACKET_SHA256.len(),
        64,
        "packet SHA-256 is pinned by the verifier"
    );
}

#[test]
fn mutation_receipt_covers_required_semantic_families() {
    let receipt: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-markdown-mutation-receipt.json"))
            .expect("mutation receipt is valid JSON");
    assert_eq!(receipt["schema"], "pie-tui-m4-markdown-mutation-receipt-v1");
    let mutations = receipt["mutations"].as_array().expect("mutation rows");
    assert_eq!(mutations.len(), 4);
    let families = mutations
        .iter()
        .map(|mutation| mutation["family"].as_str().expect("mutation family"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        families,
        std::collections::BTreeSet::from([
            "callback-lifecycle",
            "default-options",
            "list-compatibility",
            "parser-model",
        ])
    );
    assert!(mutations.iter().all(|mutation| mutation["exitCode"] == 101));
}

#[test]
fn packet_identity_and_default_marker_cases_match_reference() {
    let packet = packet();
    let mut failures = Vec::new();
    let mut seen = 0;
    for case in packet["priorThreeOfThirtyTwo"]
        .as_array()
        .unwrap()
        .iter()
        .chain(packet["additionalTenOfFifteen"].as_array().unwrap())
        .chain(std::iter::once(&packet["defaultOptionsMutationGuard"]))
    {
        let source = case["source"].as_str().unwrap();
        let mut component = Markdown::new(
            source,
            case["paddingX"].as_u64().unwrap() as usize,
            case["paddingY"].as_u64().unwrap() as usize,
            identity_theme(),
            None,
            MarkdownOptions::default(),
        );
        let actual = run_operations(&mut component, source, &case["operations"], None);
        let expected = expected_outputs(case);
        if actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, actual {actual:?}",
                case.get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("default-options-marker")
            ));
        }
        seen += 1;
    }
    assert_eq!(seen, 14);
    assert!(
        failures.is_empty(),
        "{} exact packet output mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn packet_lazy_transform_callback_and_cache_lifecycle_is_exact() {
    let packet = packet();
    let case = &packet["constructionTransformCallbackLifecycle"];
    let source = case["source"].as_str().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transform_events = events.clone();
    let mut component = Markdown::new(
        source,
        case["paddingX"].as_u64().unwrap() as usize,
        case["paddingY"].as_u64().unwrap() as usize,
        logged_theme(events.clone()),
        Some(DefaultTextStyle {
            color: Some(logged_identity(events.clone(), "color")),
            bg_color: Some(logged_identity(events.clone(), "bg")),
            bold: true,
            italic: true,
            strikethrough: true,
            underline: true,
        }),
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
    let outputs = run_operations(&mut component, source, &case["operations"], Some(&events));
    let actual_events = events.lock().unwrap().clone();
    let expected_events: Vec<String> =
        serde_json::from_value(case["expectedEvents"].clone()).unwrap();
    assert_eq!(outputs, expected_outputs(case), "five lifecycle renders");
    assert_eq!(actual_events.len(), 231, "full callback/event receipt");
    assert_eq!(actual_events, expected_events, "callback/cache order");
}
