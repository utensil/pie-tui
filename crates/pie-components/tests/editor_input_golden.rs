use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pie_components::{
    Editor, EditorAutocompleteFuture, EditorComponent, EditorHost, EditorHostTask, EditorOptions,
    EditorTaskId, EditorTheme,
};
use pie_core::editor_model::EditorWordSegmenter;
use pie_core::keybindings::KeybindingsManager;
use pie_core::keybindings::global::set_keybindings;
use pie_core::word_navigation::default_word_segments;
use serde_json::{Value, json};

static KEYBINDING_TEST_LOCK: Mutex<()> = Mutex::new(());

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/editor-components.json")).expect("fixture JSON")
}

fn case<'a>(group: &'a Value, name: &str) -> &'a Value {
    group
        .as_array()
        .expect("case array")
        .iter()
        .find(|value| value["name"] == name)
        .unwrap_or_else(|| panic!("missing fixture case {name}"))
}

fn editor_state(editor: &Editor) -> Value {
    let cursor = editor.get_cursor();
    json!({
        "text": editor.get_text(),
        "expandedText": editor.get_expanded_text(),
        "lines": editor.get_lines(),
        "cursor": { "line": cursor.line, "col": cursor.col },
        "paddingX": editor.get_padding_x(),
        "autocompleteMaxVisible": editor.get_autocomplete_max_visible(),
        "focused": editor.focused,
        "disableSubmit": editor.disable_submit,
        "showingAutocomplete": editor.is_showing_autocomplete(),
    })
}

#[derive(Default)]
struct RenderHost {
    rows: usize,
    renders: Arc<Mutex<Vec<bool>>>,
    next: u64,
}

impl EditorHost for RenderHost {
    fn terminal_rows(&self) -> usize {
        if self.rows == 0 { 24 } else { self.rows }
    }

    fn request_render(&mut self, force: bool) {
        self.renders.lock().expect("render lock").push(force);
    }

    fn schedule_task(&mut self, _delay_ms: u64, _task: EditorHostTask) -> EditorTaskId {
        self.next += 1;
        EditorTaskId(self.next)
    }

    fn cancel_task(&mut self, _task: EditorTaskId) {}

    fn spawn_autocomplete(&mut self, _request_id: u64, _future: EditorAutocompleteFuture) {}

    fn discard_autocomplete(&mut self, _request_id: u64) {}
}

fn host(rows: usize) -> Box<dyn EditorHost> {
    Box::new(RenderHost {
        rows,
        ..RenderHost::default()
    })
}

fn default_keys() {
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
}

#[test]
fn oracle_provenance_and_canonical_trait_are_exact() {
    let oracle = fixture();
    assert_eq!(oracle["reference"], "0.84.1");
    assert_eq!(
        oracle["referencePackage"],
        json!({ "name": "@earendil-works/pi-tui", "version": "0.84.1" })
    );
    assert_eq!(
        oracle["sourceDigests"],
        json!({
            "packageJson": "f7f8f42f7cfa8c53c4f00bdc12c14cb035aa62d9fb73555661ce08b68da61290",
            "editorJs": "a384c140d84e5352605250fab0e1284add133dbdda1e986419c4a0778ffa0853",
            "editorDts": "fc3b400c5c965e4906971df836c1fae0a62c89524cda44d85b3cca86782332be",
            "inputJs": "4762edfaa75de102aabc00f8660c591f2d7de1ba6e7e212900dd81c48f231e63",
            "inputDts": "f2a2bab62c83d6e4b2b69a243c96df26f81ac98963031a67c92a083c57309871",
            "editorComponentDts": "5de081a8879f096e0bb0f7efb1f4751bab259b1a77663be041938758d22e50c6",
            "selectListJs": "ea14ebd2f64ed045563360b598eeccc816f7f9f252df6b7bc492309cfe49c545",
            "keybindingsJs": "d27090a36394fc4f59350e7f3234c601082d950e179ba6742d9557aae2a72168",
            "keysJs": "14b18205fd5e56ed3b183392c82bd72e41ba3dab1d345e47b2b17af6988493cc",
            "killRingJs": "52212d532f2c5b85ed8977b0f4431f43998c6dc7746d26efc81eb7975b119122",
            "tuiJs": "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a",
            "undoStackJs": "7fbb318db3521aa1fa6804ffe50245c18d9e9f210a85a48e175fae6a629259cb",
            "utilsJs": "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
            "wordNavigationJs": "72618be2d05d6c20d9987d0d74de487335056fa0a00a145687f6106a6ae6b9d0",
            "keybindingsDts": "93450b5ff2259c52767d4bc3dffb17d7c9341f866507cf00aba67cddf42b51b0",
            "keysDts": "58d05b6227c8657e2109931eb2875de3a675e7bccc7f5eafde5467d539636344",
            "utilsDts": "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
            "eastAsianWidthPackageJson": "d263e50dd1a43aee9acda4d7f066e66b0d0bde1f2852ea6e7153750a5e3a3e52",
            "eastAsianWidthIndexJs": "d7b1ba05914c0fc311c20e5618bf8d0893c9c74078a07975e2df981445e64887",
            "eastAsianWidthLookupJs": "c80ecc22b120b27ef5ea9facb7000b8fd4ec037a84d9231d215f1c44bc9c21d0",
            "eastAsianWidthLookupDataJs": "f6b40f86c9a2a6808ec808fa8ddcb8da261254cc6121d37ffaeb2bf35dad1d5b",
            "eastAsianWidthUtilitiesJs": "4b08a7e9e3ffacbcf198a6abceb2338d52ac671899e52ccc2851c898bfccac42",
        })
    );
    assert_eq!(
        oracle["dependencies"]["getEastAsianWidth"],
        json!({ "name": "get-east-asian-width", "version": "1.6.0", "entry": "index.js" })
    );
    assert_eq!(oracle["runtime"]["node"], "24.4.1");
    assert_eq!(oracle["runtime"]["icu"], "77.1");
    assert_eq!(oracle["runtime"]["unicode"], "16.0");

    let editor = Editor::detached(EditorTheme::plain(), EditorOptions::default());
    let component: Box<dyn EditorComponent> = Box::new(editor);
    assert_eq!(component.get_text(), "");
}

#[test]
fn editor_render_input_paste_history_and_effects_match_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    default_keys();
    let oracle = fixture();
    let cases = &oracle["editorCases"];

    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    let expected = case(cases, "defaults-empty");
    assert_eq!(editor_state(&editor), expected["state"]);
    assert_eq!(json!(editor.render(8)), expected["render"]);
    editor.focused = true;
    let expected = case(cases, "focused-empty");
    assert_eq!(editor_state(&editor), expected["state"]);
    assert_eq!(json!(editor.render(8)), expected["render"]);

    let mut editor = Editor::new(
        host(24),
        EditorTheme::plain(),
        EditorOptions {
            padding_x: 2,
            autocomplete_max_visible: Some(99),
            word_segmenter: None,
        },
    );
    editor.focused = true;
    editor.set_text("A\r\nB\t👩🏽‍💻é");
    let expected = case(cases, "options-normalize-unicode");
    assert_eq!(editor_state(&editor), expected["state"]);
    assert_eq!(json!(editor.render(14)), expected["render"]);
    editor.insert_text_at_cursor("\rX\tY");
    let expected = case(cases, "insert-normalize-unicode");
    assert_eq!(editor_state(&editor), expected["state"]);
    assert_eq!(json!(editor.render(14)), expected["render"]);

    let events = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    let change_events = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        change_events
            .lock()
            .expect("effects lock")
            .push(vec!["change".into(), text]);
    })));
    let submit_events = Arc::clone(&events);
    editor.set_on_submit(Some(Box::new(move |text| {
        submit_events
            .lock()
            .expect("effects lock")
            .push(vec!["submit".into(), text]);
    })));
    editor.set_text("  old  ");
    events.lock().expect("effects lock").clear();
    editor.handle_input("\r");
    let expected = case(cases, "submit-effect-order");
    assert_eq!(
        json!(*events.lock().expect("effects lock")),
        expected["effects"]
    );
    assert_eq!(editor_state(&editor), expected["state"]);

    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    editor.handle_input("👩🏽‍💻é");
    let expected = case(cases, "grapheme-atomic-input");
    assert_eq!(editor_state(&editor), expected["before"]);
    editor.handle_input("\x1b[D");
    assert_eq!(editor_state(&editor), expected["afterLeft"]);
    editor.handle_input("\x7f");
    assert_eq!(editor_state(&editor), expected["afterBackspace"]);

    for count in [10, 11] {
        let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
        let text = (1..=count)
            .map(|index| format!("line-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        editor.handle_input(&format!("\x1b[200~{text}\x1b[201~"));
        let expected = case(cases, &format!("paste-{count}-lines"));
        assert_eq!(editor_state(&editor), expected["afterPaste"]);
        editor.handle_input("\x1f");
        assert_eq!(editor_state(&editor), expected["afterUndo"]);
    }

    let paste = (1..=11)
        .map(|index| format!("line-{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut owned = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    owned.focused = true;
    owned.handle_input(&format!("\x1b[200~{paste}\x1b[201~"));
    let expected = case(cases, "owned-paste-marker-render");
    assert_eq!(json!(owned.render(6)), expected["wrapped"]);
    owned.handle_input("\x01");
    assert_eq!(json!(owned.render(24)), expected["ownedAtStart"]);

    let mut literal = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    literal.focused = true;
    literal.set_text("[paste #1 +11 lines]");
    literal.handle_input("\x01");
    assert_eq!(json!(literal.render(24)), expected["literalAtStart"]);

    let mut editor = Editor::new(host(10), EditorTheme::plain(), EditorOptions::default());
    editor.focused = true;
    editor.set_text(
        (1..=9)
            .map(|index| format!("row-{index}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let expected = case(cases, "long-document-scroll-top-marker");
    assert_eq!(editor_state(&editor), expected["state"]);
    assert_eq!(json!(editor.render(12)), expected["render"]);
    let expected = case(cases, "resize-rewrap");
    assert_eq!(json!(editor.render(20)), expected["renderWide"]);
    assert_eq!(json!(editor.render(8)), expected["renderNarrow"]);

    // The public component delegates the already-pinned model's history,
    // kill/yank, and undo atoms without creating a second state machine.
    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    editor.add_to_history("one");
    editor.add_to_history("two");
    editor.handle_input("\x1b[A");
    assert_eq!(editor.get_text(), "two");
    editor.handle_input("\x1b[B");
    assert_eq!(editor.get_text(), "");
    editor.set_text("alpha beta");
    editor.handle_input("\x01");
    editor.handle_input("\x0b");
    assert_eq!(editor.get_text(), "");
    editor.handle_input("\x19");
    assert_eq!(editor.get_text(), "alpha beta");
    editor.handle_input("\x1f");
    assert_eq!(editor.get_text(), "");

    let submissions = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    editor.set_text("held");
    let callback = Arc::clone(&submissions);
    editor.set_on_submit(Some(Box::new(move |text| {
        callback.lock().expect("submission lock").push(text);
    })));
    set_keybindings(KeybindingsManager::with_tui_defaults(vec![(
        "tui.input.submit".into(),
        vec!["x".into()],
    )]));
    editor.handle_input("\r");
    assert!(submissions.lock().expect("submission lock").is_empty());
    assert_eq!(editor.get_text(), "held");
    editor.handle_input("x");
    assert_eq!(*submissions.lock().expect("submission lock"), vec!["held"]);

    let segment_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&segment_calls);
    let segmenter = EditorWordSegmenter::new(move |text| {
        observed.fetch_add(1, Ordering::SeqCst);
        default_word_segments(text)
    });
    default_keys();
    let mut editor = Editor::new(
        host(24),
        EditorTheme::plain(),
        EditorOptions {
            word_segmenter: Some(segmenter),
            ..EditorOptions::default()
        },
    );
    editor.set_text("alpha beta");
    editor.handle_input("\x1b[1;5D");
    assert!(segment_calls.load(Ordering::SeqCst) > 0);
    default_keys();
}

#[test]
fn editor_jump_chunk_matches_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    default_keys();
    let oracle = fixture();
    let cases = &oracle["editorCases"];

    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    editor.set_text("aeXé");
    editor.handle_input("\x01");
    editor.handle_input("\x1d");
    editor.handle_input("é");
    let expected = case(cases, "multi-codepoint-jump-target");
    assert_eq!(editor_state(&editor), expected["state"]);
}

#[test]
fn editor_live_newline_binding_matches_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    default_keys();
    let oracle = fixture();
    let cases = &oracle["editorCases"];
    let events = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let mut editor = Editor::new(host(24), EditorTheme::plain(), EditorOptions::default());
    let changes = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        changes
            .lock()
            .expect("effects lock")
            .push(vec!["change".into(), text]);
    })));
    let submits = Arc::clone(&events);
    editor.set_on_submit(Some(Box::new(move |text| {
        submits
            .lock()
            .expect("effects lock")
            .push(vec!["submit".into(), text]);
    })));
    editor.set_text("\\");
    events.lock().expect("effects lock").clear();
    set_keybindings(KeybindingsManager::with_tui_defaults(vec![
        ("tui.input.newLine".into(), vec!["enter".into()]),
        ("tui.input.submit".into(), vec!["shift+enter".into()]),
    ]));
    editor.handle_input("\r");
    let expected = case(cases, "live-newline-submit-backslash-enter");
    assert_eq!(
        json!(*events.lock().expect("effects lock")),
        expected["effects"]
    );
    assert_eq!(editor_state(&editor), expected["state"]);
    default_keys();
}
