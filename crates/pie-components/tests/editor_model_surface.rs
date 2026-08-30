use std::sync::{Arc, Mutex};

use pie_components::{DetachedEditorHost, Editor, EditorOptions, EditorTheme};
use pie_core::keybindings::KeybindingsManager;
use pie_core::keybindings::global::set_keybindings;
use serde_json::{Value, json};

static KEYBINDING_TEST_LOCK: Mutex<()> = Mutex::new(());

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../pie-core/tests/fixtures/editor-state.json"
    ))
    .expect("editor model fixture")
}

fn trace<'a>(fixture: &'a Value, label: &str) -> &'a Value {
    fixture["editor"]
        .as_array()
        .expect("editor traces")
        .iter()
        .find(|trace| trace["label"] == label)
        .unwrap_or_else(|| panic!("missing editor trace {label}"))
}

fn configure_surface_keys() {
    set_keybindings(KeybindingsManager::with_tui_defaults(vec![
        ("tui.editor.historyPrevious".into(), vec!["ctrl+p".into()]),
        ("tui.editor.historyNext".into(), vec!["ctrl+n".into()]),
        ("tui.editor.jumpForward".into(), vec!["ctrl+o".into()]),
        ("tui.editor.jumpBackward".into(), vec!["ctrl+g".into()]),
    ]));
}

fn run_action(editor: &mut Editor, action: &Value) {
    let text = || action["text"].as_str().expect("action text");
    match action["type"].as_str().expect("action type") {
        "set_text" => editor.set_text(text()),
        "insert_text" => editor.insert_text_at_cursor(text()),
        "type" => editor.handle_input(text()),
        "undo" => editor.handle_input("\x1f"),
        "line_start" => editor.handle_input("\x01"),
        "add_history" => editor.add_to_history(text()),
        "history_previous" => editor.handle_input("\x10"),
        "history_next" => editor.handle_input("\x0e"),
        "delete_word_backward" => editor.handle_input("\x17"),
        "delete_line_start" => editor.handle_input("\x15"),
        "move_word_backward" => editor.handle_input("\x1b[1;5D"),
        "yank" => editor.handle_input("\x19"),
        "yank_pop" => editor.handle_input("\x1by"),
        "jump_forward" => {
            editor.handle_input("\x0f");
            editor.handle_input(text());
        }
        "jump_backward" => {
            editor.handle_input("\x07");
            editor.handle_input(text());
        }
        "move_up" => editor.handle_input("\x1b[A"),
        "move_down" => editor.handle_input("\x1b[B"),
        "page_up" => editor.handle_input("\x1b[5~"),
        "page_down" => editor.handle_input("\x1b[6~"),
        "new_line" => editor.handle_input("\n"),
        "paste" => editor.handle_input(&format!("\x1b[200~{}\x1b[201~", text())),
        "set_view" => {
            let model_width = action["width"].as_u64().expect("view width") as usize;
            // With default zero padding Editor reserves one terminal column for
            // the cursor, so terminal width N+1 exposes model layout width N.
            editor.render(model_width + 1);
        }
        other => panic!("unmapped surface action {other}"),
    }
}

fn assert_trace(label: &str, rows: usize) {
    let fixture = fixture();
    let trace = trace(&fixture, label);
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut editor = Editor::new(
        Box::new(DetachedEditorHost::new(rows)),
        EditorTheme::plain(),
        EditorOptions::default(),
    );
    let change_events = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        change_events
            .lock()
            .expect("event lock")
            .push(json!({ "type": "change", "text": text }));
    })));
    let submit_events = Arc::clone(&events);
    editor.set_on_submit(Some(Box::new(move |text| {
        submit_events
            .lock()
            .expect("event lock")
            .push(json!({ "type": "submit", "text": text }));
    })));

    for (index, step) in trace["steps"]
        .as_array()
        .expect("trace steps")
        .iter()
        .enumerate()
    {
        events.lock().expect("event lock").clear();
        run_action(&mut editor, &step["action"]);
        let expected = &step["state"];
        let cursor = editor.get_cursor();
        assert_eq!(editor.get_text(), expected["text"], "{label} step {index}");
        assert_eq!(
            editor.get_expanded_text(),
            expected["expandedText"],
            "{label} step {index} expanded"
        );
        assert_eq!(
            json!(editor.get_lines()),
            expected["lines"],
            "{label} step {index} lines"
        );
        assert_eq!(
            json!({ "line": cursor.line, "col": cursor.col }),
            expected["cursor"],
            "{label} step {index} cursor"
        );
        assert_eq!(
            json!(*events.lock().expect("event lock")),
            expected["effects"],
            "{label} step {index} effects"
        );
    }
}

#[test]
fn public_editor_surface_replays_pinned_model_atoms() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    configure_surface_keys();

    for label in [
        "normalization-and-set-undo",
        "typed-word-coalescence",
        "atomic-programmatic-insert",
        "history-draft-and-placement",
        "kill-accumulate-yank-undo",
        "yank-pop-cycle",
        "multi-line-character-jump",
        "preferred-logical-column",
        "preferred-wrapped-column",
        "wide-paste-marker-resize-continuation",
    ] {
        assert_trace(label, 24);
    }
    assert_trace("page-actions", 20);

    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
}
