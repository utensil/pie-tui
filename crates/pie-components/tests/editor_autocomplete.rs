use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use pie_components::{
    AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
    AutocompleteSuggestionsFuture, CompletionResult, Editor, EditorAutocompleteFuture, EditorHost,
    EditorHostTask, EditorOptions, EditorTaskId, EditorTheme,
};
use pie_core::keybindings::KeybindingsManager;
use pie_core::keybindings::global::set_keybindings;
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

#[derive(Clone)]
struct ManualFuture {
    state: Arc<Mutex<ManualFutureState>>,
}

#[derive(Default)]
struct ManualFutureState {
    value: Option<Option<AutocompleteSuggestions>>,
    waker: Option<Waker>,
}

impl Future for ManualFuture {
    type Output = Option<AutocompleteSuggestions>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().expect("manual future lock");
        if let Some(value) = state.value.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

#[derive(Clone)]
struct ManualResolver {
    state: Arc<Mutex<ManualFutureState>>,
}

impl ManualResolver {
    fn resolve(&self, value: Option<AutocompleteSuggestions>) {
        let waker = {
            let mut state = self.state.lock().expect("manual future lock");
            state.value = Some(value);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

fn deferred() -> (ManualFuture, ManualResolver) {
    let state = Arc::new(Mutex::new(ManualFutureState::default()));
    (
        ManualFuture {
            state: Arc::clone(&state),
        },
        ManualResolver { state },
    )
}

#[derive(Clone)]
struct ProviderCall {
    id: usize,
    text: String,
    line: usize,
    col: usize,
    force: bool,
    signal: pie_components::CancellationSignal,
}

impl ProviderCall {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "text": self.text,
            "line": self.line,
            "col": self.col,
            "force": self.force,
            "aborted": self.signal.aborted(),
        })
    }
}

#[derive(Default)]
struct ProviderState {
    calls: Vec<ProviderCall>,
    resolvers: Vec<ManualResolver>,
    events: Vec<Value>,
    predicate: bool,
}

struct ManualProvider {
    state: Arc<Mutex<ProviderState>>,
    triggers: Vec<String>,
}

impl ManualProvider {
    fn new(triggers: &[&str]) -> (Arc<Self>, Arc<Mutex<ProviderState>>) {
        let state = Arc::new(Mutex::new(ProviderState {
            predicate: true,
            ..ProviderState::default()
        }));
        (
            Arc::new(Self {
                state: Arc::clone(&state),
                triggers: triggers.iter().map(|value| (*value).to_owned()).collect(),
            }),
            state,
        )
    }
}

impl AutocompleteProvider for ManualProvider {
    fn trigger_characters(&self) -> Option<&[String]> {
        Some(&self.triggers)
    }

    fn get_suggestions<'a>(
        &'a self,
        lines: &'a [String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteSuggestionsFuture<'a> {
        let (future, resolver) = deferred();
        let mut state = self.state.lock().expect("provider lock");
        let id = state.calls.len() + 1;
        state.events.push(json!([
            "request",
            options.force,
            lines.join("\n"),
            cursor_line,
            cursor_col
        ]));
        state.calls.push(ProviderCall {
            id,
            text: lines.join("\n"),
            line: cursor_line,
            col: cursor_col,
            force: options.force,
            signal: options.signal,
        });
        state.resolvers.push(resolver);
        Box::pin(future)
    }

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        self.state
            .lock()
            .expect("provider lock")
            .events
            .push(json!(["apply", item.value]));
        let mut result = lines.to_vec();
        let line = result.get(cursor_line).cloned().unwrap_or_default();
        let start = cursor_col.saturating_sub(prefix.encode_utf16().count());
        let start_byte = utf16_to_byte(&line, start);
        let cursor_byte = utf16_to_byte(&line, cursor_col);
        result[cursor_line] = format!(
            "{}{}{}",
            &line[..start_byte],
            item.value,
            &line[cursor_byte..]
        );
        CompletionResult {
            lines: result,
            cursor_line,
            cursor_col: start + item.value.encode_utf16().count(),
        }
    }

    fn should_trigger_file_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) -> bool {
        let mut state = self.state.lock().expect("provider lock");
        state.events.push(json!([
            "predicate",
            lines.join("\n"),
            cursor_line,
            cursor_col
        ]));
        state.predicate
    }
}

#[derive(Debug, Clone)]
struct Scheduled {
    at: u64,
    task: EditorHostTask,
}

#[derive(Default)]
struct ClockState {
    now: u64,
    next_task: u64,
    scheduled: BTreeMap<EditorTaskId, Scheduled>,
    schedule_facts: Vec<Value>,
    cancel_facts: Vec<u64>,
    futures: BTreeMap<u64, EditorAutocompleteFuture>,
    renders: Vec<bool>,
    discarded: Vec<u64>,
}

#[derive(Clone, Default)]
struct FakeClock {
    state: Arc<Mutex<ClockState>>,
}

impl EditorHost for FakeClock {
    fn terminal_rows(&self) -> usize {
        24
    }

    fn request_render(&mut self, force: bool) {
        self.state.lock().expect("clock lock").renders.push(force);
    }

    fn schedule_task(&mut self, delay_ms: u64, task: EditorHostTask) -> EditorTaskId {
        let mut state = self.state.lock().expect("clock lock");
        state.next_task += 1;
        let id = EditorTaskId(state.next_task);
        let at = state.now + delay_ms;
        state
            .schedule_facts
            .push(json!(["schedule", id.0, delay_ms, format!("{task:?}")]));
        state.scheduled.insert(id, Scheduled { at, task });
        id
    }

    fn cancel_task(&mut self, task: EditorTaskId) {
        let mut state = self.state.lock().expect("clock lock");
        state.scheduled.remove(&task);
        state.cancel_facts.push(task.0);
    }

    fn spawn_autocomplete(&mut self, request_id: u64, future: EditorAutocompleteFuture) {
        self.state
            .lock()
            .expect("clock lock")
            .futures
            .insert(request_id, future);
    }

    fn discard_autocomplete(&mut self, request_id: u64) {
        let mut state = self.state.lock().expect("clock lock");
        state.futures.remove(&request_id);
        state.discarded.push(request_id);
    }
}

impl FakeClock {
    fn advance(&self, editor: &mut Editor, milliseconds: u64) {
        let target = {
            let state = self.state.lock().expect("clock lock");
            state.now + milliseconds
        };
        loop {
            let next = {
                let state = self.state.lock().expect("clock lock");
                state
                    .scheduled
                    .iter()
                    .filter(|(_, scheduled)| scheduled.at <= target)
                    .min_by_key(|(id, scheduled)| (scheduled.at, id.0))
                    .map(|(id, scheduled)| (*id, scheduled.clone()))
            };
            let Some((id, scheduled)) = next else {
                break;
            };
            {
                let mut state = self.state.lock().expect("clock lock");
                state.scheduled.remove(&id);
                state.now = scheduled.at;
            }
            editor.handle_host_task(scheduled.task);
            self.poll(editor);
        }
        self.state.lock().expect("clock lock").now = target;
        self.poll(editor);
    }

    fn poll(&self, editor: &mut Editor) {
        loop {
            let ready = {
                let mut state = self.state.lock().expect("clock lock");
                let ids = state.futures.keys().copied().collect::<Vec<_>>();
                let mut ready = None;
                for id in ids {
                    let future = state.futures.get_mut(&id).expect("future exists");
                    let waker = Waker::noop();
                    let mut context = Context::from_waker(waker);
                    if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                        ready = Some((id, value));
                        break;
                    }
                }
                if let Some((id, _)) = ready.as_ref() {
                    state.futures.remove(id);
                }
                ready
            };
            let Some((id, value)) = ready else {
                break;
            };
            editor.complete_autocomplete(id, value);
        }
    }
}

fn new_editor(clock: &FakeClock) -> Editor {
    Editor::new(
        Box::new(clock.clone()),
        EditorTheme::plain(),
        EditorOptions::default(),
    )
}

fn suggestion(values: &[&str], prefix: &str) -> Option<AutocompleteSuggestions> {
    Some(AutocompleteSuggestions {
        items: values
            .iter()
            .map(|value| AutocompleteItem::new(*value, *value))
            .collect(),
        prefix: prefix.to_owned(),
    })
}

fn calls_json(state: &Arc<Mutex<ProviderState>>) -> Value {
    json!(
        state
            .lock()
            .expect("provider lock")
            .calls
            .iter()
            .map(ProviderCall::json)
            .collect::<Vec<_>>()
    )
}

fn canonical_calls_json(state: &Arc<Mutex<ProviderState>>, include_aborted: bool) -> Value {
    let calls = state.lock().expect("provider lock").calls.clone();
    json!(
        calls
            .iter()
            .map(|call| {
                if include_aborted {
                    json!({
                        "text": call.text,
                        "line": call.line,
                        "col": call.col,
                        "force": call.force,
                        "aborted": call.signal.aborted(),
                    })
                } else {
                    json!({
                        "text": call.text,
                        "line": call.line,
                        "col": call.col,
                        "force": call.force,
                    })
                }
            })
            .collect::<Vec<_>>()
    )
}

fn canonical_call_json(call: &ProviderCall) -> Value {
    json!({
        "text": call.text,
        "line": call.line,
        "col": call.col,
        "force": call.force,
        "aborted": call.signal.aborted(),
    })
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

fn resolve_latest(
    clock: &FakeClock,
    editor: &mut Editor,
    state: &Arc<Mutex<ProviderState>>,
    value: Option<AutocompleteSuggestions>,
) {
    let resolver = state
        .lock()
        .expect("provider lock")
        .resolvers
        .last()
        .expect("latest resolver")
        .clone();
    resolver.resolve(value);
    clock.poll(editor);
}

#[test]
fn autocomplete_serializes_supersession_and_stale_results_cannot_win() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();
    let expected = case(
        &oracle["autocompleteCases"],
        "serialized-supersession-stale-cannot-win",
    );
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&["#"]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);

    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    assert_eq!(state.lock().expect("provider lock").calls.len(), 1);
    assert_eq!(
        clock
            .state
            .lock()
            .expect("clock lock")
            .futures
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![1]
    );
    editor.handle_input("a");
    clock.advance(&mut editor, 0);
    assert_eq!(calls_json(&state), expected["beforeASettles"]);
    assert_eq!(
        clock
            .state
            .lock()
            .expect("clock lock")
            .futures
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![1]
    );
    let first = state.lock().expect("provider lock").resolvers[0].clone();
    first.resolve(suggestion(&["/alpha"], "/"));
    clock.poll(&mut editor);
    assert_eq!(calls_json(&state), expected["afterASettles"]);
    assert_eq!(
        clock
            .state
            .lock()
            .expect("clock lock")
            .futures
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
    let second = state.lock().expect("provider lock").resolvers[1].clone();
    second.resolve(suggestion(&["/about", "/alpha"], "/a"));
    clock.poll(&mut editor);
    assert_eq!(calls_json(&state), expected["finalCalls"]);
    assert_eq!(editor.get_text(), "/a");
    assert!(editor.is_showing_autocomplete());
    assert_eq!(json!(editor.render(24)), expected["render"]);

    // Prefix selection is case-sensitive: exact beats an earlier prefix.
    editor.handle_input("\x1b");
    editor.set_text("");
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let resolver = state.lock().expect("provider lock").resolvers[2].clone();
    resolver.resolve(suggestion(&["/about", "/"], "/"));
    clock.poll(&mut editor);
    editor.handle_input("\t");
    assert_eq!(editor.get_text(), "/");
}

#[test]
fn custom_trigger_requires_a_token_boundary_and_resets_at_exactly_20ms() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&["#"]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);

    editor.handle_input("x");
    editor.handle_input("#");
    clock.advance(&mut editor, 100);
    assert!(state.lock().expect("provider lock").calls.is_empty());
    assert!(
        clock
            .state
            .lock()
            .expect("clock lock")
            .schedule_facts
            .is_empty()
    );

    editor.set_text("");
    editor.handle_input("#");
    {
        let clock = clock.state.lock().expect("clock lock");
        assert_eq!(clock.schedule_facts.len(), 1);
        assert_eq!(clock.schedule_facts[0][2], 20);
    }
    clock.advance(&mut editor, 19);
    assert!(state.lock().expect("provider lock").calls.is_empty());
    editor.handle_input("a");
    {
        let clock = clock.state.lock().expect("clock lock");
        assert_eq!(clock.schedule_facts.len(), 2);
        assert_eq!(clock.schedule_facts[1][2], 20);
        assert_eq!(clock.cancel_facts, vec![1]);
    }
    clock.advance(&mut editor, 19);
    assert!(state.lock().expect("provider lock").calls.is_empty());
    clock.advance(&mut editor, 1);
    let calls = state.lock().expect("provider lock").calls.clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].text, "#a");
    assert_eq!(calls[0].col, 2);
    assert!(!calls[0].force);
}

#[test]
fn debounce_escape_force_drop_and_provider_replacement_are_causal() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&["#"]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);

    editor.handle_input("#");
    clock.advance(&mut editor, 19);
    assert_eq!(state.lock().expect("provider lock").calls.len(), 0);
    editor.handle_input("a");
    clock.advance(&mut editor, 19);
    assert_eq!(state.lock().expect("provider lock").calls.len(), 0);
    clock.advance(&mut editor, 1);
    let expected = case(
        &oracle["autocompleteCases"],
        "custom-trigger-debounce-reset",
    );
    assert_eq!(
        state.lock().expect("provider lock").calls.len(),
        expected["atReset20"]
    );
    assert_eq!(state.lock().expect("provider lock").calls[0].text, "#a");
    state.lock().expect("provider lock").resolvers[0]
        .clone()
        .resolve(None);
    clock.poll(&mut editor);

    editor.set_text("");
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let call = state.lock().expect("provider lock").calls[1].clone();
    editor.handle_input("\x1b");
    let expected = case(
        &oracle["autocompleteCases"],
        "escape-before-visible-does-not-abort",
    );
    assert_eq!(call.signal.aborted(), expected["afterEscape"]["aborted"]);
    assert!(!editor.is_showing_autocomplete());
    state.lock().expect("provider lock").resolvers[1]
        .clone()
        .resolve(suggestion(&["/help"], "/"));
    clock.poll(&mut editor);
    assert!(editor.is_showing_autocomplete());

    editor.handle_input("\x1b");
    editor.set_text("");
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let callbacks = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        callbacks
            .lock()
            .expect("callback lock")
            .push(json!(["change", text]));
    })));
    editor.handle_input("\t");
    clock.advance(&mut editor, 0);
    let force_call = state.lock().expect("provider lock").calls[2].clone();
    assert!(force_call.force);
    state.lock().expect("provider lock").resolvers[2]
        .clone()
        .resolve(suggestion(&["file.txt"], ""));
    clock.poll(&mut editor);
    assert_eq!(editor.get_text(), "file.txt");
    assert!(!editor.is_showing_autocomplete());
    assert_eq!(
        *events.lock().expect("callback lock"),
        vec![json!(["change", "file.txt"])]
    );
    let provider_events = state.lock().expect("provider lock").events.clone();
    assert_eq!(provider_events[2], json!(["predicate", "", 0, 0]));
    assert_eq!(provider_events[3], json!(["request", true, "", 0, 0]));
    assert_eq!(provider_events[4], json!(["apply", "file.txt"]));
    assert_eq!(
        clock.state.lock().expect("clock lock").renders.last(),
        Some(&false)
    );

    // Replacement and setText abort the exact active signal. The replacement
    // request remains serialized until the abandoned provider settles.
    editor.set_text("");
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let active = state.lock().expect("provider lock").calls[3].clone();
    let (replacement, _) = ManualProvider::new(&[]);
    editor.set_autocomplete_provider(replacement);
    assert!(active.signal.aborted());
    state.lock().expect("provider lock").resolvers[3]
        .clone()
        .resolve(suggestion(&["stale"], "/"));
    clock.poll(&mut editor);
    assert!(!editor.is_showing_autocomplete());

    // Dropping a debounced editor cancels the timer; dropping an active one
    // aborts the token and discards its host future, so no later callback can run.
    let drop_clock = FakeClock::default();
    let (drop_provider, drop_state) = ManualProvider::new(&["#"]);
    let mut dropped = new_editor(&drop_clock);
    dropped.set_autocomplete_provider(drop_provider);
    dropped.handle_input("#");
    drop(dropped);
    assert_eq!(drop_state.lock().expect("provider lock").calls.len(), 0);
    assert_eq!(
        drop_clock
            .state
            .lock()
            .expect("clock lock")
            .cancel_facts
            .len(),
        1
    );

    let mut dropped = new_editor(&drop_clock);
    let (drop_provider, drop_state) = ManualProvider::new(&[]);
    dropped.set_autocomplete_provider(drop_provider);
    dropped.handle_input("/");
    drop_clock.advance(&mut dropped, 0);
    let signal = drop_state.lock().expect("provider lock").calls[0]
        .signal
        .clone();
    let resolver = drop_state.lock().expect("provider lock").resolvers[0].clone();
    let renders_before_drop = drop_clock.state.lock().expect("clock lock").renders.len();
    drop(dropped);
    assert!(signal.aborted());
    assert_eq!(
        drop_clock.state.lock().expect("clock lock").discarded,
        vec![1]
    );
    resolver.resolve(suggestion(&["must-not-apply"], "/"));
    assert_eq!(
        drop_clock.state.lock().expect("clock lock").renders.len(),
        renders_before_drop
    );
    assert!(
        !drop_state
            .lock()
            .expect("provider lock")
            .events
            .iter()
            .any(|event| event[0] == "apply")
    );
}

#[test]
fn set_text_submit_and_provider_replacement_abort_without_stale_effects() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);

    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let set_text_call = state.lock().expect("provider lock").calls[0].clone();
    editor.set_text("replacement");
    assert!(set_text_call.signal.aborted());
    state.lock().expect("provider lock").resolvers[0]
        .clone()
        .resolve(suggestion(&["stale-set-text"], "/"));
    clock.poll(&mut editor);
    assert_eq!(editor.get_text(), "replacement");
    assert!(!editor.is_showing_autocomplete());

    editor.set_text("");
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let submit_call = state.lock().expect("provider lock").calls[1].clone();
    editor.handle_input("\r");
    assert!(submit_call.signal.aborted());
    state.lock().expect("provider lock").resolvers[1]
        .clone()
        .resolve(suggestion(&["stale-submit"], "/"));
    clock.poll(&mut editor);
    assert_eq!(editor.get_text(), "");
    assert!(!editor.is_showing_autocomplete());

    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    let replacement_call = state.lock().expect("provider lock").calls[2].clone();
    let (replacement, _) = ManualProvider::new(&[]);
    editor.set_autocomplete_provider(replacement);
    assert!(replacement_call.signal.aborted());
    state.lock().expect("provider lock").resolvers[2]
        .clone()
        .resolve(suggestion(&["stale-provider"], "/"));
    clock.poll(&mut editor);
    assert_eq!(editor.get_text(), "/");
    assert!(!editor.is_showing_autocomplete());
}

#[test]
fn forced_multiple_items_open_a_menu_whose_keys_take_precedence() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);

    editor.handle_input("\t");
    clock.advance(&mut editor, 0);
    {
        let state = state.lock().expect("provider lock");
        assert_eq!(
            state.events,
            vec![
                json!(["predicate", "", 0, 0]),
                json!(["request", true, "", 0, 0]),
            ]
        );
        assert!(state.calls[0].force);
    }
    state.lock().expect("provider lock").resolvers[0]
        .clone()
        .resolve(suggestion(&["first.txt", "second.txt"], ""));
    clock.poll(&mut editor);
    assert!(editor.is_showing_autocomplete());
    assert_eq!(editor.get_text(), "");

    // Down belongs to the visible menu, not the underlying editor cursor.
    editor.handle_input("\x1b[B");
    editor.handle_input("\t");
    assert_eq!(editor.get_text(), "second.txt");
    assert!(!editor.is_showing_autocomplete());
    assert_eq!(
        state.lock().expect("provider lock").events.last(),
        Some(&json!(["apply", "second.txt"]))
    );

    editor.set_text("");
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    state.lock().expect("provider lock").resolvers[1]
        .clone()
        .resolve(suggestion(&["/one", "/two"], "/"));
    clock.poll(&mut editor);
    assert!(editor.is_showing_autocomplete());
    editor.handle_input("\x1b");
    assert!(!editor.is_showing_autocomplete());
    assert_eq!(editor.get_text(), "/");
}

#[test]
fn pending_and_history_autocomplete_lifecycle_match_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();

    let expected = case(
        &oracle["autocompleteCases"],
        "pending-slash-backspace-retrigger",
    );
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    assert_eq!(canonical_calls_json(&state, true), expected["afterSlash"]);
    editor.handle_input("a");
    clock.advance(&mut editor, 0);
    assert_eq!(canonical_calls_json(&state, true), expected["afterA"]);
    editor.handle_input("\x7f");
    clock.advance(&mut editor, 0);
    assert_eq!(
        canonical_calls_json(&state, true),
        expected["afterBackspace"]["calls"]
    );
    assert_eq!(editor_state(&editor), expected["afterBackspace"]["state"]);
    let first = state.lock().expect("provider lock").resolvers[0].clone();
    first.resolve(None);
    clock.poll(&mut editor);
    assert_eq!(
        canonical_calls_json(&state, true),
        expected["afterFirstSettles"]
    );
    resolve_latest(
        &clock,
        &mut editor,
        &state,
        suggestion(&["/help", "/history"], "/"),
    );
    assert_eq!(canonical_calls_json(&state, true), expected["finalCalls"]);
    assert_eq!(editor_state(&editor), expected["state"]);

    set_keybindings(KeybindingsManager::with_tui_defaults(vec![
        ("tui.editor.historyPrevious".into(), vec!["ctrl+p".into()]),
        ("tui.editor.historyNext".into(), vec!["ctrl+n".into()]),
    ]));
    let expected = case(&oracle["autocompleteCases"], "history-actions-abort-active");
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut previous = new_editor(&clock);
    previous.add_to_history("older");
    previous.set_autocomplete_provider(provider);
    previous.handle_input("/");
    clock.advance(&mut previous, 0);
    previous.handle_input("\x10");
    let previous_call = state.lock().expect("provider lock").calls[0].clone();
    assert_eq!(
        canonical_call_json(&previous_call),
        expected["afterPrevious"]["call"]
    );
    assert_eq!(editor_state(&previous), expected["afterPrevious"]["state"]);
    state.lock().expect("provider lock").resolvers[0]
        .clone()
        .resolve(None);
    clock.poll(&mut previous);

    let (provider, state) = ManualProvider::new(&[]);
    let mut next = new_editor(&clock);
    next.add_to_history("older");
    next.set_text("draft");
    next.handle_input("\x10");
    next.set_autocomplete_provider(provider);
    next.handle_input("\t");
    clock.advance(&mut next, 0);
    next.handle_input("\x0e");
    let next_call = state.lock().expect("provider lock").calls[0].clone();
    assert_eq!(
        canonical_call_json(&next_call),
        expected["afterNext"]["call"]
    );
    assert_eq!(editor_state(&next), expected["afterNext"]["state"]);
    state.lock().expect("provider lock").resolvers[0]
        .clone()
        .resolve(None);
    clock.poll(&mut next);
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
}

#[test]
fn open_menu_horizontal_requery_matches_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();

    let expected = case(&oracle["autocompleteCases"], "open-menu-horizontal-requery");
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    resolve_latest(
        &clock,
        &mut editor,
        &state,
        suggestion(&["/alpha", "/about"], "/"),
    );
    assert_eq!(
        state.lock().expect("provider lock").calls.len(),
        expected["beforeLeft"]
    );
    editor.handle_input("\x1b[D");
    clock.advance(&mut editor, 0);
    assert_eq!(canonical_calls_json(&state, false), expected["calls"]);
    assert_eq!(editor_state(&editor), expected["state"]);
}

#[test]
fn deletion_retrigger_matches_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();
    let expected = case(
        &oracle["autocompleteCases"],
        "backspace-forward-delete-retrigger",
    );
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);
    for character in ["/", "a"] {
        editor.handle_input(character);
        clock.advance(&mut editor, 0);
        resolve_latest(&clock, &mut editor, &state, None);
    }
    let before_backspace = state.lock().expect("provider lock").calls.len();
    editor.handle_input("\x7f");
    clock.advance(&mut editor, 0);
    resolve_latest(&clock, &mut editor, &state, None);
    assert_eq!(
        state.lock().expect("provider lock").calls.len() - before_backspace,
        expected["afterBackspace"]["requestDelta"]
    );
    assert_eq!(
        canonical_calls_json(&state, false),
        expected["afterBackspace"]["calls"]
    );
    assert_eq!(editor_state(&editor), expected["afterBackspace"]["state"]);

    editor.set_text("");
    for character in ["/", "a", "b"] {
        editor.handle_input(character);
        clock.advance(&mut editor, 0);
        resolve_latest(&clock, &mut editor, &state, None);
    }
    editor.handle_input("\x1b[D");
    let before_delete = state.lock().expect("provider lock").calls.len();
    editor.handle_input("\x1b[3~");
    clock.advance(&mut editor, 0);
    resolve_latest(&clock, &mut editor, &state, None);
    assert_eq!(
        state.lock().expect("provider lock").calls.len() - before_delete,
        expected["afterDelete"]["requestDelta"]
    );
    assert_eq!(
        canonical_calls_json(&state, false),
        expected["afterDelete"]["calls"]
    );
    assert_eq!(editor_state(&editor), expected["afterDelete"]["state"]);
}

#[test]
fn non_refresh_actions_match_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();

    let expected = case(&oracle["autocompleteCases"], "non-refresh-editor-actions");
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let mut editor = new_editor(&clock);
    editor.set_autocomplete_provider(provider);
    for character in ["/", "a", "b", "c"] {
        editor.handle_input(character);
        clock.advance(&mut editor, 0);
        let prefix = editor.get_text();
        resolve_latest(
            &clock,
            &mut editor,
            &state,
            suggestion(&["/alpha", "/about"], &prefix),
        );
    }
    assert_eq!(
        state.lock().expect("provider lock").calls.len(),
        expected["baselineCalls"]
    );
    assert_eq!(canonical_calls_json(&state, false), expected["calls"]);
    for (name, data) in [
        ("line-start", "\x01"),
        ("word-right", "\x1bf"),
        ("word-left", "\x1bb"),
        ("line-end", "\x05"),
        ("kill-line-start", "\x15"),
        ("yank", "\x19"),
        ("undo", "\x1f"),
    ] {
        let expected_action = expected["actions"]
            .as_array()
            .expect("action array")
            .iter()
            .find(|value| value["name"] == name)
            .expect("expected action");
        let before = state.lock().expect("provider lock").calls.len();
        editor.handle_input(data);
        clock.advance(&mut editor, 0);
        assert_eq!(
            state.lock().expect("provider lock").calls.len() - before,
            expected_action["requestDelta"],
            "{name} request delta"
        );
        assert_eq!(
            editor_state(&editor),
            expected_action["state"],
            "{name} state"
        );
    }
}

#[test]
fn continued_trigger_js_whitespace_matches_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();
    let expected = case(
        &oracle["autocompleteCases"],
        "continued-trigger-js-whitespace",
    );
    for (index, whitespace) in ["\u{00a0}", "\u{feff}"].into_iter().enumerate() {
        let expected_case = &expected["cases"][index];
        let clock = FakeClock::default();
        let (provider, state) = ManualProvider::new(&["#"]);
        let mut editor = new_editor(&clock);
        editor.set_autocomplete_provider(provider);
        editor.handle_input(whitespace);
        editor.handle_input("#");
        clock.advance(&mut editor, 0);
        assert_eq!(
            canonical_calls_json(&state, false),
            expected_case["afterTrigger"]["calls"]
        );
        assert_eq!(
            clock.state.lock().expect("clock lock").scheduled.len(),
            expected_case["afterTrigger"]["pendingTimers"]
        );
        editor.handle_input("a");
        clock.advance(&mut editor, 0);
        assert_eq!(
            canonical_calls_json(&state, false),
            expected_case["afterContinuation"]["calls"]
        );
        assert_eq!(
            clock.state.lock().expect("clock lock").scheduled.len(),
            expected_case["afterContinuation"]["pendingTimers"]
        );
        assert_eq!(editor_state(&editor), expected_case["state"]);
    }
}

#[test]
fn autocomplete_confirmation_callbacks_match_oracle() {
    let _guard = KEYBINDING_TEST_LOCK.lock().expect("keybinding test lock");
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let oracle = fixture();

    let expected = case(&oracle["autocompleteCases"], "slash-confirm-silent-apply");
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&[]);
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut editor = new_editor(&clock);
    let changes = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        changes
            .lock()
            .expect("callback lock")
            .push(json!(["change", text]));
    })));
    let submits = Arc::clone(&events);
    editor.set_on_submit(Some(Box::new(move |text| {
        submits
            .lock()
            .expect("callback lock")
            .push(json!(["submit", text]));
    })));
    editor.set_autocomplete_provider(provider);
    editor.handle_input("/");
    clock.advance(&mut editor, 0);
    resolve_latest(
        &clock,
        &mut editor,
        &state,
        suggestion(&["/help", "/history"], "/"),
    );
    events.lock().expect("callback lock").clear();
    editor.handle_input("\r");
    assert_eq!(
        json!(&*events.lock().expect("callback lock")),
        expected["events"]
    );
    assert_eq!(editor_state(&editor), expected["state"]);

    let expected = case(
        &oracle["autocompleteCases"],
        "ordinary-confirm-emits-change",
    );
    let clock = FakeClock::default();
    let (provider, state) = ManualProvider::new(&["#"]);
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut editor = new_editor(&clock);
    let changes = Arc::clone(&events);
    editor.set_on_change(Some(Box::new(move |text| {
        changes
            .lock()
            .expect("callback lock")
            .push(json!(["change", text]));
    })));
    editor.set_autocomplete_provider(provider);
    editor.handle_input("#");
    clock.advance(&mut editor, 20);
    resolve_latest(
        &clock,
        &mut editor,
        &state,
        suggestion(&["#alice", "#alex"], "#"),
    );
    events.lock().expect("callback lock").clear();
    editor.handle_input("\r");
    assert_eq!(
        json!(&*events.lock().expect("callback lock")),
        expected["events"]
    );
    assert_eq!(editor_state(&editor), expected["state"]);
}

fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut consumed = 0;
    for (byte, character) in text.char_indices() {
        if consumed >= units || consumed + character.len_utf16() > units {
            return byte;
        }
        consumed += character.len_utf16();
    }
    text.len()
}
