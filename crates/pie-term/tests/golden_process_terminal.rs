//! ProcessTerminal black-box differential using a recording backend.
//!
//! No test touches the controlling terminal: raw mode, streams, signals, and
//! timers are all represented by `FakeBackend` plus explicit scheduler ticks.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pie_core::keys::{is_kitty_protocol_active, set_kitty_protocol_active};
use pie_term::process_terminal::{
    ProcessTerminal, ProcessTerminalBackend, normalize_apple_terminal_input,
    normalize_native_shift_enter_input,
};
use pie_term::{KeyboardProtocolNegotiation, Terminal, parse_keyboard_protocol_negotiation};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/process-terminal.json")).unwrap()
}

#[derive(Debug, Default)]
struct FakeState {
    operations: Vec<String>,
    raw: bool,
    input_listeners: usize,
    resize_listeners: usize,
}

#[derive(Clone)]
struct FakeBackend {
    shared: Rc<RefCell<FakeState>>,
    columns: Option<usize>,
    rows: Option<usize>,
    environment_columns: Option<usize>,
    environment_rows: Option<usize>,
    windows: bool,
    apple_terminal: bool,
    shift_pressed: bool,
}

impl FakeBackend {
    fn new(raw: bool) -> (Self, Rc<RefCell<FakeState>>) {
        let shared = Rc::new(RefCell::new(FakeState {
            raw,
            ..FakeState::default()
        }));
        (
            Self {
                shared: Rc::clone(&shared),
                columns: Some(80),
                rows: Some(24),
                environment_columns: None,
                environment_rows: None,
                windows: false,
                apple_terminal: false,
                shift_pressed: false,
            },
            shared,
        )
    }

    fn operation(&self, operation: impl Into<String>) {
        self.shared.borrow_mut().operations.push(operation.into());
    }
}

impl ProcessTerminalBackend for FakeBackend {
    fn raw_mode(&self) -> bool {
        self.shared.borrow().raw
    }

    fn set_raw_mode(&mut self, active: bool) {
        let mut state = self.shared.borrow_mut();
        state.raw = active;
        state.operations.push(format!("stdin.setRawMode:{active}"));
    }

    fn set_utf8_encoding(&mut self) {
        self.operation("stdin.setEncoding:utf8");
    }

    fn resume_input(&mut self) {
        self.operation("stdin.resume");
    }

    fn pause_input(&mut self) {
        self.operation("stdin.pause");
    }

    fn subscribe_input(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.input_listeners += 1;
        state.operations.push("stdin.on:data".to_owned());
    }

    fn unsubscribe_input(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.input_listeners -= 1;
        state
            .operations
            .push("stdin.removeListener:data".to_owned());
    }

    fn subscribe_drain_input(&mut self) {
        self.subscribe_input();
    }

    fn unsubscribe_drain_input(&mut self) {
        self.unsubscribe_input();
    }

    fn subscribe_resize(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.resize_listeners += 1;
        state.operations.push("stdout.on:resize".to_owned());
    }

    fn unsubscribe_resize(&mut self) {
        let mut state = self.shared.borrow_mut();
        state.resize_listeners -= 1;
        state
            .operations
            .push("stdout.removeListener:resize".to_owned());
    }

    fn signal_winch(&mut self) {
        self.operation("process.kill:SIGWINCH");
    }

    fn enable_windows_vt_input(&mut self) {
        self.operation("windows.enableVirtualTerminalInput");
    }

    fn write(&mut self, data: &str) {
        self.operation(format!("stdout.write:{}", js_display(data)));
    }

    fn columns(&self) -> Option<usize> {
        self.columns
    }

    fn rows(&self) -> Option<usize> {
        self.rows
    }

    fn environment_columns(&self) -> Option<usize> {
        self.environment_columns
    }

    fn environment_rows(&self) -> Option<usize> {
        self.environment_rows
    }

    fn is_windows(&self) -> bool {
        self.windows
    }

    fn is_apple_terminal(&self) -> bool {
        self.apple_terminal
    }

    fn shift_pressed(&self) -> bool {
        self.shift_pressed
    }
}

#[test]
fn oracle_is_exactly_pinned() {
    let root = fixture();
    assert_eq!(root["oracle"]["package"], "@earendil-works/pi-tui");
    assert_eq!(root["oracle"]["version"], "0.84.1");
    assert_eq!(
        root["oracle"]["files"]["terminal.js"],
        "30bf4f78daf561e7b222f1a52c96a9e4f5805bfd5436c5d4c484375e0df7f79c"
    );
}

#[test]
fn negotiation_and_shift_enter_normalizers_match() {
    for case in fixture()["normalizers"].as_array().unwrap() {
        let sequence = case["sequence"].as_str().unwrap();
        assert_eq!(
            negotiation_json(parse_keyboard_protocol_negotiation(sequence)),
            case["negotiation"]
        );
        assert_eq!(
            normalize_native_shift_enter_input(sequence, false, true),
            case["nativeFalse"].as_str().unwrap()
        );
        assert_eq!(
            normalize_native_shift_enter_input(sequence, true, false),
            case["nativeNoShift"].as_str().unwrap()
        );
        assert_eq!(
            normalize_native_shift_enter_input(sequence, true, true),
            case["nativeShift"].as_str().unwrap()
        );
        assert_eq!(
            normalize_apple_terminal_input(sequence, true, true),
            case["appleShift"].as_str().unwrap()
        );
    }
}

#[test]
fn lifecycle_protocol_drain_progress_and_geometry_match() {
    let root = fixture();
    set_kitty_protocol_active(false);

    // Start, resize, Kitty negotiation, ordinary input, and stop cleanup.
    {
        let expected = &root["startupKittyStop"];
        let (backend, shared) = FakeBackend::new(false);
        let input = Rc::new(RefCell::new(Vec::<String>::new()));
        let resize_count = Rc::new(Cell::new(0_u32));
        let mut terminal = ProcessTerminal::new(backend);
        let input_sink = Rc::clone(&input);
        let resize_sink = Rc::clone(&resize_count);
        terminal.start_at(
            0,
            Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
            Box::new(move || resize_sink.set(resize_sink.get() + 1)),
        );
        assert_snapshot(&terminal, &shared, &expected["afterStart"]);
        terminal.receive_resize();
        terminal.receive_stdin("\x1b[?7u", 0);
        terminal.receive_stdin("a", 0);
        assert_snapshot(&terminal, &shared, &expected["afterInput"]);
        assert_eq!(
            strings_json(&input.borrow()),
            expected["afterInput"]["input"]
        );
        assert_eq!(resize_count.get(), 1);
        terminal.stop_process();
        assert_snapshot(&terminal, &shared, &expected["afterStop"]);
        assert!(!is_kitty_protocol_active());
    }

    // DA fallback enables modifyOtherKeys; a later Kitty reply reverses it.
    {
        let expected = &root["fallbackThenKitty"];
        let (backend, shared) = FakeBackend::new(false);
        let input = Rc::new(RefCell::new(Vec::<String>::new()));
        let input_sink = Rc::clone(&input);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(
            0,
            Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
            Box::new(|| {}),
        );
        terminal.receive_stdin("\x1b[?1;2c", 0);
        assert_snapshot(&terminal, &shared, &expected["afterDa"]);
        terminal.receive_stdin("\x1b[?7u", 0);
        terminal.receive_stdin("\x1b[200~paste\ntext\x1b[201~", 0);
        assert_snapshot(&terminal, &shared, &expected["afterKitty"]);
        assert_eq!(
            strings_json(&input.borrow()),
            expected["afterKitty"]["input"]
        );
        terminal.stop_process();
        assert_snapshot(&terminal, &shared, &expected["afterStop"]);
    }

    // StdinBuffer's 10 ms flush feeds the 150 ms negotiation reassembler.
    {
        let expected = &root["splitNegotiation"];
        let (backend, shared) = FakeBackend::new(false);
        let input = Rc::new(RefCell::new(Vec::<String>::new()));
        let input_sink = Rc::clone(&input);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(
            0,
            Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
            Box::new(|| {}),
        );
        terminal.receive_stdin("\x1b[", 0);
        terminal.tick(10);
        assert_snapshot(&terminal, &shared, &expected["afterFragmentFlush"]);
        assert!(input.borrow().is_empty());
        terminal.receive_stdin("?7u", 10);
        assert_snapshot(&terminal, &shared, &expected["afterCompletion"]);
        assert!(input.borrow().is_empty());
    }

    // An incomplete negotiation prefix becomes ordinary input at 150 ms.
    {
        let expected = &root["negotiationTimeout"];
        let (backend, _) = FakeBackend::new(false);
        let input = Rc::new(RefCell::new(Vec::<String>::new()));
        let input_sink = Rc::clone(&input);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(
            0,
            Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
            Box::new(|| {}),
        );
        terminal.receive_stdin("\x1b[", 0);
        terminal.tick(10);
        terminal.tick(159);
        assert_eq!(strings_json(&input.borrow()), expected["before"]["input"]);
        terminal.tick(160);
        assert_eq!(strings_json(&input.borrow()), expected["after"]["input"]);
    }

    // Drain disables protocols first, suppresses delivery, and restores handler at idle.
    {
        let expected = &root["drain"];
        let (backend, shared) = FakeBackend::new(false);
        let input = Rc::new(RefCell::new(Vec::<String>::new()));
        let input_sink = Rc::clone(&input);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(
            0,
            Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
            Box::new(|| {}),
        );
        terminal.receive_stdin("\x1b[?1;2c", 0);
        terminal.begin_drain_input(0, 120, 50);
        assert_snapshot(&terminal, &shared, &expected["afterBegin"]);
        assert!(!terminal.poll_drain_input(49));
        assert!(terminal.poll_drain_input(50));
        assert_snapshot(&terminal, &shared, &expected["afterDone"]);
    }

    // Progress keepalive has one interval, but every explicit call writes.
    {
        let expected = &root["progress"];
        let (backend, shared) = FakeBackend::new(false);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.set_progress_at(true, 0);
        terminal.set_progress_at(true, 0);
        terminal.tick(1000);
        assert_eq!(operations_json(&shared), expected["active"]);
        terminal.set_progress_at(false, 1000);
        terminal.set_progress_at(false, 1000);
        terminal.set_progress_at(true, 1000);
        terminal.stop_process();
        assert_eq!(operations_json(&shared), expected["complete"]);
    }

    // Geometry fallbacks and terminal write helpers are byte exact.
    {
        let expected = &root["geometryAndWrites"];
        let (mut backend, shared) = FakeBackend::new(false);
        backend.columns = Some(0);
        backend.rows = Some(0);
        backend.environment_columns = Some(101);
        backend.environment_rows = Some(37);
        let mut terminal = ProcessTerminal::new(backend);
        assert_eq!(terminal.columns(), expected["geometry"]["columns"]);
        assert_eq!(terminal.rows(), expected["geometry"]["rows"]);
        terminal.move_by(2);
        terminal.move_by(-3);
        terminal.move_by(0);
        terminal.hide_cursor();
        terminal.show_cursor();
        terminal.clear_line();
        terminal.clear_from_cursor();
        terminal.clear_screen();
        terminal.set_title("hello");
        assert_eq!(operations_json(&shared), expected["operations"]);
    }

    // A pre-existing raw state is restored exactly.
    {
        let expected = &root["restoreRaw"];
        let (backend, shared) = FakeBackend::new(true);
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(0, Box::new(|_| {}), Box::new(|| {}));
        terminal.stop_process();
        assert_snapshot(&terminal, &shared, expected);
    }
}

#[test]
fn drain_idle_resets_on_input_and_drop_restores_every_resource() {
    set_kitty_protocol_active(false);
    let (backend, shared) = FakeBackend::new(false);
    {
        let mut terminal = ProcessTerminal::new(backend);
        terminal.start_at(0, Box::new(|_| {}), Box::new(|| {}));
        terminal.begin_drain_input(0, 200, 50);
        terminal.receive_stdin("late", 40);
        assert!(!terminal.poll_drain_input(50));
        assert!(terminal.poll_drain_input(90));
        // No explicit stop: Drop is the cleanup guarantee.
    }
    let state = shared.borrow();
    assert!(!state.raw);
    assert_eq!(state.input_listeners, 0);
    assert_eq!(state.resize_listeners, 0);
    assert!(
        state
            .operations
            .iter()
            .any(|operation| operation == "stdout.write:\\u001b[?2004l")
    );
    assert_eq!(state.operations.last().unwrap(), "stdin.setRawMode:false");
    assert!(!is_kitty_protocol_active());
}

#[test]
fn coarse_tick_replays_descendant_timer_at_its_due_time() {
    let expected = &fixture()["coarseTick"];
    let expected_event = &expected["input"][0];
    assert_eq!(expected_event["at"], 160);
    assert_eq!(expected["target"], 1000);

    let (backend, _) = FakeBackend::new(false);
    let input = Rc::new(RefCell::new(Vec::<String>::new()));
    let input_sink = Rc::clone(&input);
    let mut terminal = ProcessTerminal::new(backend);
    terminal.start_at(
        0,
        Box::new(move |data| input_sink.borrow_mut().push(data.to_owned())),
        Box::new(|| {}),
    );
    terminal.receive_stdin("\x1b[", 0);
    terminal.tick(expected["target"].as_u64().unwrap());

    assert_eq!(
        strings_json(&input.borrow()),
        serde_json::json!([expected_event["data"]])
    );
}

#[test]
fn windows_start_enables_vt_input_before_protocol_negotiation() {
    let expected = &fixture()["windowsStart"];
    let (mut backend, shared) = FakeBackend::new(false);
    backend.windows = true;
    let mut terminal = ProcessTerminal::new(backend);
    terminal.start_at(0, Box::new(|_| {}), Box::new(|| {}));
    assert_snapshot(&terminal, &shared, &expected["afterStart"]);
    terminal.stop_process();
    assert_snapshot(&terminal, &shared, &expected["afterStop"]);
}

fn assert_snapshot(
    terminal: &ProcessTerminal<FakeBackend>,
    shared: &Rc<RefCell<FakeState>>,
    expected: &serde_json::Value,
) {
    assert_eq!(operations_json(shared), expected["operations"]);
    let state = shared.borrow();
    let actual = serde_json::json!({
        "kittyProtocolActive": terminal.kitty_protocol_active(),
        "modifyOtherKeysActive": terminal.modify_other_keys_active(),
        "raw": state.raw,
        "listenerCount": state.input_listeners,
        "resizeListenerCount": state.resize_listeners,
    });
    assert_eq!(actual, expected["state"]);
}

fn operations_json(shared: &Rc<RefCell<FakeState>>) -> serde_json::Value {
    strings_json(&shared.borrow().operations)
}

fn strings_json(values: &[String]) -> serde_json::Value {
    serde_json::Value::Array(values.iter().cloned().map(Into::into).collect())
}

fn negotiation_json(value: Option<KeyboardProtocolNegotiation>) -> serde_json::Value {
    match value {
        Some(KeyboardProtocolNegotiation::KittyFlags { flags }) => {
            serde_json::json!({ "type": "kitty-flags", "flags": flags })
        }
        Some(KeyboardProtocolNegotiation::DeviceAttributes) => {
            serde_json::json!({ "type": "device-attributes" })
        }
        None => serde_json::Value::Null,
    }
}

fn js_display(data: &str) -> String {
    let mut result = String::new();
    for character in data.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            character if character <= '\u{1f}' => {
                result.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => result.push(character),
        }
    }
    result
}
