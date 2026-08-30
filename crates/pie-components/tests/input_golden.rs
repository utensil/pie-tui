use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pie_components::Input;
use pie_core::editor_model::EditorWordSegmenter;
use pie_core::keybindings::KeybindingsManager;
use pie_core::keybindings::global::set_keybindings;
use pie_core::word_navigation::default_word_segments;
use serde_json::{Value, json};

static KEYBINDING_TEST_LOCK: Mutex<()> = Mutex::new(());

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/editor-components.json")).expect("fixture JSON")
}

fn case<'a>(cases: &'a Value, name: &str) -> &'a Value {
    cases
        .as_array()
        .expect("case array")
        .iter()
        .find(|value| value["name"] == name)
        .unwrap_or_else(|| panic!("missing fixture case {name}"))
}

fn default_keys() {
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
}

#[test]
fn input_defaults_callbacks_paste_unicode_and_viewport_match_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    default_keys();
    let oracle = fixture();
    let cases = &oracle["inputCases"];

    let mut input = Input::new();
    let expected = case(cases, "defaults");
    assert_eq!(input.get_value(), expected["value"]);
    assert_eq!(input.focused, expected["focused"]);
    assert_eq!(input.cursor(), 0);
    assert_eq!(json!(input.render(8)), expected["render"]);

    input.set_value("abcdef");
    input.focused = true;
    let expected = case(cases, "set-value-cursor-zero");
    assert_eq!(input.get_value(), expected["value"]);
    assert_eq!(input.cursor(), 0);
    assert_eq!(json!(input.render(8)), expected["render"]);

    let events = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let mut input = Input::new();
    input.set_value("abcdef");
    let submit_events = Arc::clone(&events);
    input.set_on_submit(Some(Box::new(move |value| {
        submit_events
            .lock()
            .expect("event lock")
            .push(vec!["submit".into(), value]);
    })));
    let escape_events = Arc::clone(&events);
    input.set_on_escape(Some(Box::new(move || {
        escape_events
            .lock()
            .expect("event lock")
            .push(vec!["escape".into()]);
    })));
    input.handle_input("\r");
    input.handle_input("\x1b");
    let expected = case(cases, "submit-no-clear-escape");
    assert_eq!(
        json!(*events.lock().expect("event lock")),
        expected["events"]
    );
    assert_eq!(input.get_value(), expected["value"]);

    let mut input = Input::new();
    input.handle_input("A");
    input.handle_input("\x1b[200~b\r\nc\rd\ne\tf\x1b[201~");
    let expected = case(cases, "paste-flatten-tabs");
    assert_eq!(input.get_value(), expected["value"]);
    assert_eq!(json!(input.render(20)), expected["render"]);
    input.handle_input("\x1f");
    assert_eq!(input.get_value(), "A");

    let mut input = Input::new();
    input.focused = true;
    input.handle_input("a👩🏽‍💻éz");
    input.handle_input("\x1b[D");
    input.handle_input("\x1b[D");
    let expected = case(cases, "unicode-cursor-render");
    assert_eq!(input.get_value(), expected["value"]);
    assert_eq!(input.cursor(), 8);
    assert_eq!(json!(input.render(10)), expected["render"]);
    let expected = case(cases, "horizontal-viewport");
    assert_eq!(json!(input.render(6)), expected["render"]);

    default_keys();
}

#[test]
fn input_kill_yank_undo_live_keys_and_host_segmenter_are_surfaced() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    default_keys();

    let mut input = Input::new();
    input.set_value("alpha beta");
    input.handle_input("\x05");
    input.handle_input("\x01");
    input.handle_input("\x0b");
    assert_eq!(input.get_value(), "");
    input.handle_input("\x19");
    assert_eq!(input.get_value(), "alpha beta");
    input.handle_input("\x1f");
    assert_eq!(input.get_value(), "");

    let oracle = fixture();
    let expected = case(&oracle["inputCases"], "live-global-keybindings");
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut input = Input::new();
    input.set_value("held");
    let submit_events = Arc::clone(&events);
    input.set_on_submit(Some(Box::new(move |value| {
        submit_events.lock().expect("event lock").push(value);
    })));
    set_keybindings(KeybindingsManager::with_tui_defaults(vec![(
        "tui.input.submit".into(),
        vec!["x".into()],
    )]));
    input.handle_input("\r");
    assert_eq!(input.get_value(), expected["afterOld"]);
    assert!(events.lock().expect("event lock").is_empty());
    input.handle_input("x");
    assert_eq!(input.get_value(), expected["afterNew"]);
    assert_eq!(
        json!(*events.lock().expect("event lock")),
        expected["events"]
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let segmenter = EditorWordSegmenter::new(move |text| {
        observed.fetch_add(1, Ordering::SeqCst);
        default_word_segments(text)
    });
    default_keys();
    let mut input = Input::new();
    input.set_word_segmenter(Some(segmenter));
    input.set_value("alpha beta");
    input.handle_input("\x05");
    input.handle_input("\x1b[1;5D");
    assert_eq!(input.cursor(), 6);
    assert!(calls.load(Ordering::SeqCst) > 0);

    default_keys();
}
