//! StdinBuffer spec conformance — executes the normative vectors of
//! docs/specs/stdin-buffer.md §8 against fixtures harvested from the pinned
//! reference build (differential: same fixture validates both impls).
use pie_core::stdin_buffer::{StdinBuffer, StdinEvent};

fn vectors() -> Vec<serde_json::Value> {
    let raw = include_str!("fixtures/stdin-golden.json");
    serde_json::from_str::<serde_json::Value>(raw).expect("fixture json")["vectors"]
        .as_array()
        .cloned()
        .unwrap()
}

fn to_event(pair: &serde_json::Value) -> StdinEvent {
    let kind = pair[0].as_str().unwrap();
    let payload = pair[1].as_str().unwrap();
    match kind {
        "data" => StdinEvent::Data(payload.to_string()),
        "paste" => StdinEvent::Paste(payload.to_string()),
        other => panic!("unknown event kind {other}"),
    }
}

#[test]
fn stdin_buffer_matches_reference_vectors() {
    for v in vectors() {
        let id = v["id"].as_str().unwrap();
        let chunks: Vec<&str> = v["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        let expected: Vec<StdinEvent> = v["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(to_event)
            .collect();

        let mut buf = StdinBuffer::new(10);
        let mut got = Vec::new();
        for chunk in chunks {
            got.extend(buf.process(chunk));
        }
        got.extend(buf.flush());
        assert_eq!(got, expected, "stdin spec vector {id}");
    }
}

#[test]
fn esc_timeout_flushes_single_sequence() {
    // Spec §4: lone ESC + silence => ONE data("\x1b") event.
    let mut buf = StdinBuffer::new(10);
    assert!(buf.process("\x1b").is_empty());
    assert_eq!(buf.get_buffer(), "\x1b");
    assert_eq!(buf.flush(), vec![StdinEvent::Data("\x1b".to_string())]);
    assert_eq!(buf.get_buffer(), "");
}

#[test]
fn clear_resets_everything() {
    let mut buf = StdinBuffer::new(10);
    buf.process("\x1b[<1;2"); // buffered remainder
    buf.clear();
    assert_eq!(buf.get_buffer(), "");
    assert_eq!(buf.flush(), vec![]);
}

#[test]
fn paste_split_across_three_chunks_with_leading_input() {
    // "x" then paste split across chunks; leading data emitted before paste starts.
    let mut buf = StdinBuffer::new(10);
    assert_eq!(buf.process("x"), vec![StdinEvent::Data("x".to_string())]);
    assert!(buf.process("\x1b[200~ab").is_empty());
    assert_eq!(
        buf.process("\x1b[201~y"),
        vec![
            StdinEvent::Paste("ab".to_string()),
            StdinEvent::Data("y".to_string())
        ]
    );
}
