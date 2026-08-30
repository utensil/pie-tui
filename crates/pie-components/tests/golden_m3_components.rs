//! M3 component/autocomplete differential harness.
//!
//! Replays scripted observations harvested from the pinned
//! `@earendil-works/pi-tui@0.84.1` compiled distribution. Regenerate with:
//! `PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-m3-components.mjs`.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use pie_components::{
    Align, ArgumentCompletionResult, AutocompleteCommand, AutocompleteItem, AutocompleteOptions,
    AutocompleteProvider, BoxComponent, CancellableLoader, CancellationController,
    CombinedAutocompleteProvider, Component, ComponentHandle, Container, HStack,
    LoaderIndicatorOptions, SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme,
    SelectListTruncateContext, SettingItem, SettingsList, SettingsListOptions, SettingsListTheme,
    SlashCommand, StackEntry, StackViewport, SubmenuDone, VStack, allocate_stack_sizes,
};
use serde_json::{Value, json};

static FIXTURE: OnceLock<Value> = OnceLock::new();

const AUTOCOMPLETE_LOCALE_ORDER_NAMES: &[&str] = &[
    "Zoo", "alpha", "Álpha", "äther", "Beta", "file10", "file2", "_under", "a-b", "a b",
];

struct ThreadWake(thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match Pin::as_mut(&mut future).poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park_timeout(Duration::from_millis(10)),
        }
    }
}

fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn fixture() -> &'static Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/m3-components.json"))
            .expect("m3-components fixture")
    })
}

fn assert_case_fold_unique(names: &[&str]) {
    for (index, name) in names.iter().enumerate() {
        let key = name.to_lowercase();
        assert!(
            !names[..index]
                .iter()
                .any(|previous| previous.to_lowercase() == key),
            "locale-order fixture names must remain case-fold unique: {name:?} conflicts with an earlier name"
        );
    }
}

fn case<'a>(section: &str, name: &str) -> &'a Value {
    fixture()[section]
        .as_array()
        .expect("fixture section array")
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing {section} case {name}"))
}

fn select_theme() -> SelectListTheme {
    SelectListTheme {
        selected_prefix: Box::new(|text| format!("\x1b[35m{text}\x1b[39m")),
        selected_text: Box::new(|text| format!("\x1b[7m{text}\x1b[27m")),
        description: Box::new(|text| format!("\x1b[2m{text}\x1b[22m")),
        scroll_info: Box::new(|text| format!("\x1b[36m{text}\x1b[39m")),
        no_match: Box::new(|text| format!("\x1b[31m{text}\x1b[39m")),
    }
}

fn select_items() -> Vec<SelectItem> {
    vec![
        SelectItem::with_description("config", "Configure", "Change the active configuration"),
        SelectItem::with_description("connect", "Connect", "Connect\r\nto a remote"),
        SelectItem::with_description("copy", "Copy", "Copy the selected value"),
        SelectItem::with_description("close", "Close", "Close this view"),
        SelectItem::with_description("commit", "Commit", "Commit current changes"),
        SelectItem::with_description("continue", "Continue", "Continue execution"),
        SelectItem::with_description("value-only", "", "Falls back to value"),
    ]
}

fn settings_theme() -> SettingsListTheme {
    SettingsListTheme {
        label: Box::new(|text, selected| {
            if selected {
                format!("\x1b[1m{text}\x1b[22m")
            } else {
                text.to_string()
            }
        }),
        value: Box::new(|text, selected| {
            if selected {
                format!("\x1b[36m{text}\x1b[39m")
            } else {
                format!("\x1b[2m{text}\x1b[22m")
            }
        }),
        description: Box::new(|text| format!("\x1b[3m{text}\x1b[23m")),
        cursor: "» ".to_string(),
        hint: Box::new(|text| format!("\x1b[2m{text}\x1b[22m")),
    }
}

fn make_settings() -> Vec<SettingItem> {
    vec![
        SettingItem::new("theme", "Theme", "dark")
            .with_values(vec!["dark".into(), "light".into()])
            .with_description("Color theme used for the interface."),
        SettingItem::new("language", "Language", "English")
            .with_values(vec!["English".into(), "中文".into(), "日本語".into()])
            .with_description("Language for generated text and labels."),
        SettingItem::new("format", "Output format", "compact")
            .with_values(vec!["compact".into(), "expanded".into()]),
        SettingItem::new("telemetry", "Telemetry", "off")
            .with_values(vec!["off".into(), "on".into()]),
        SettingItem::new("wrap", "Line wrapping", "auto")
            .with_values(vec!["auto".into(), "never".into()]),
    ]
}

#[test]
fn oracle_provenance_is_exact() {
    assert_eq!(fixture()["oracle"]["package"], "@earendil-works/pi-tui");
    assert_eq!(fixture()["oracle"]["version"], "0.84.1");
    assert_eq!(
        fixture()["oracle"]["files"]["components/select-list.js"],
        "ea14ebd2f64ed045563360b598eeccc816f7f9f252df6b7bc492309cfe49c545"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["components/settings-list.js"],
        "475f324eb9b077d3f2b90aed72f9972cc1fa6c53421d517514180812f95343f2"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["autocomplete.js"],
        "865133b63cfbf59bdc045f6ba9b8760c2489fb735312b8c791f8f56cf0c19191"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["components/input.js"],
        "4762edfaa75de102aabc00f8660c591f2d7de1ba6e7e212900dd81c48f231e63"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["components/cancellable-loader.js"],
        "9fcffa6a7faeafe4686d1b78f375d3b03c465552faaf308ee9a44169580b3467"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["components/loader.js"],
        "ac9b17681af40e9dc681fb0a2cbafb5d109ff5a529d6117756a35d7016dd4f4f"
    );
    assert_eq!(
        fixture()["oracle"]["files"]["tui.js"],
        "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a"
    );
    for (file, digest) in [
        (
            "components/box.js",
            "cb81f51bbb09035a1c33227064e558d3dbab72484943e0d1dc9dc0a479649ab7",
        ),
        (
            "components/stack.js",
            "02b4dafebc728f1c0e8d01b5cc330f82eb760c58bc71c5cc9bff6d98bf34dbf3",
        ),
        (
            "components/v-stack.js",
            "7b6e6f6e1fbb037f33a8238ab02ec9e6e5f65fe49029bb430acd59c40148bfa4",
        ),
        (
            "components/h-stack.js",
            "b4cf1819879bcbbf20f2cfd3a5d5960b92c7f69308310d2ad44f0dbae4ed9c0f",
        ),
        (
            "layout-node.js",
            "73c3942b68d52ed29072f1f78184c99d405f9259bb4e24a1b6b0e3688381f7f5",
        ),
    ] {
        assert_eq!(fixture()["oracle"]["files"][file], digest, "{file}");
    }
}

struct MutableContainerChild {
    value: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

impl Component for MutableContainerChild {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.events
            .lock()
            .expect("events")
            .push(format!("render:{}:{width}", self.value));
        vec![format!("{}:{width}", self.value)]
    }
}

#[test]
fn container_shared_identity_mutation_removal_and_readd_are_differential() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let shared = ComponentHandle::new(MutableContainerChild {
        value: "one",
        events: events.clone(),
    });
    let middle = ComponentHandle::new(MutableContainerChild {
        value: "middle",
        events: events.clone(),
    });
    let mut container = Container::new();
    container.add_shared_child(shared.clone());
    container.add_shared_child(middle);
    container.add_shared_child(shared.clone());
    let mut outputs = vec![container.render(11)];
    container.remove_component(&shared);
    shared.borrow_mut().value = "two";
    outputs.push(container.render(7));
    container.remove_component(&shared);
    outputs.push(container.render(5));
    container.clear();
    container.add_shared_child(shared.clone());
    outputs.push(container.render(3));
    assert_eq!(
        json!({
            "outputs": outputs,
            "events": events.lock().expect("events").clone(),
        }),
        fixture()["containerIdentity"]
    );
}

#[test]
fn container_mount_tokens_are_owner_safe_and_stale_safe() {
    let shared = ComponentHandle::new(MutableContainerChild {
        value: "state",
        events: Arc::new(Mutex::new(Vec::new())),
    });
    let mut first = Container::new();
    let token = first.add_shared_child(shared.clone());
    let mut second = Container::new();
    second.add_shared_child(shared.clone());
    second.remove_child(token);
    assert_eq!(second.len(), 1, "foreign owner token is a no-op");
    first.remove_child(token);
    first.remove_child(token);
    assert!(first.is_empty(), "stale token remains a no-op");
    shared.borrow_mut().value = "preserved";
    first.add_shared_child(shared.clone());
    assert_eq!(first.render(4), vec!["preserved:4"]);
}

struct OracleContainerChild {
    name: &'static str,
    rows: &'static [&'static str],
    events: Arc<Mutex<Vec<String>>>,
}

impl Component for OracleContainerChild {
    fn invalidate(&mut self) {
        self.events
            .lock()
            .expect("events")
            .push(format!("invalidate:{}", self.name));
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.events
            .lock()
            .expect("events")
            .push(format!("render:{}:{width}", self.name));
        self.rows
            .iter()
            .map(|row| format!("{}:{row}:{width}", self.name))
            .collect()
    }
}

#[test]
fn container_order_remove_clear_and_invalidate_are_differential() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut container = Container::new();
    let first = container.add_child(Box::new(OracleContainerChild {
        name: "first",
        rows: &["a", "b"],
        events: events.clone(),
    }));
    container.add_child(Box::new(OracleContainerChild {
        name: "second",
        rows: &["c"],
        events: events.clone(),
    }));
    let mut outputs = vec![container.render(17)];
    container.invalidate();
    container.remove_child(first);
    outputs.push(container.render(9));
    container.clear();
    outputs.push(container.render(4));
    assert_eq!(
        json!({
            "outputs": outputs,
            "events": events.lock().expect("events").clone(),
        }),
        fixture()["container"]
    );
}

#[test]
fn box_remove_clear_background_sampling_and_cache_are_differential() {
    struct PlainNameChild {
        name: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    }
    impl Component for PlainNameChild {
        fn render(&mut self, width: usize) -> Vec<String> {
            self.events
                .lock()
                .expect("events")
                .push(format!("render:{}:{width}", self.name));
            vec![self.name.to_owned()]
        }
        fn invalidate(&mut self) {
            self.events
                .lock()
                .expect("events")
                .push(format!("invalidate:{}", self.name));
        }
    }

    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let tone = Arc::new(Mutex::new("red"));
    let bg_events = events.clone();
    let bg_tone = tone.clone();
    let mut component = BoxComponent::with_bg_fn(
        1,
        1,
        Box::new(move |text| {
            let tone = *bg_tone.lock().expect("tone");
            bg_events
                .lock()
                .expect("events")
                .push(format!("bg:{tone}:{text}"));
            let code = if tone == "red" { 41 } else { 44 };
            format!("\x1b[{code}m{text}\x1b[49m")
        }),
    );
    let first = component.add_child(Box::new(PlainNameChild {
        name: "one",
        events: events.clone(),
    }));
    component.add_child(Box::new(PlainNameChild {
        name: "two",
        events: events.clone(),
    }));

    let mut outputs = vec![component.render(9), component.render(9)];
    *tone.lock().expect("tone") = "blue";
    outputs.push(component.render(9));
    component.remove_child(first);
    outputs.push(component.render(7));
    component.invalidate();
    component.clear();
    outputs.push(component.render(7));
    assert_eq!(
        json!({
            "outputs": outputs,
            "events": events.lock().expect("events").clone(),
        }),
        fixture()["boxLifecycle"]
    );
}

struct StackOracleChild {
    name: String,
    rows: Vec<String>,
    events: Arc<Mutex<Vec<String>>>,
}

impl Component for StackOracleChild {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.events
            .lock()
            .expect("events")
            .push(format!("render:{}:{width}", self.name));
        self.rows
            .iter()
            .map(|row| format!("{}:{row}", self.name))
            .collect()
    }
}

fn stack_child(
    name: impl Into<String>,
    rows: &[&str],
    events: &Arc<Mutex<Vec<String>>>,
) -> Box<dyn Component> {
    Box::new(StackOracleChild {
        name: name.into(),
        rows: rows.iter().map(|row| (*row).to_owned()).collect(),
        events: events.clone(),
    })
}

#[test]
fn direct_stack_defaults_visibility_lifecycle_and_allocation_are_differential() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let visible_events = events.clone();
    let visible: pie_components::StackVisibilityFn = Arc::new(move |viewport| {
        let result = viewport.width >= 8;
        visible_events
            .lock()
            .expect("events")
            .push(format!("visible:{}:{result}", viewport.width));
        result
    });

    let mut stack = VStack::new(1, Align::Start);
    let first = stack.add_child(stack_child("first", &["a", "b"], &events));
    stack.add_child_with_entry(
        stack_child("second", &["x"], &events),
        StackEntry {
            basis: Some(3),
            grow: 2,
            shrink: 4,
            min_size: 1,
            max_size: 5,
            visible: Some(visible),
        },
    );

    let default = &stack.data.entries[0];
    assert_eq!(default.basis, None);
    assert_eq!(default.grow, 0);
    assert_eq!(default.shrink, 1);
    assert_eq!(default.min_size, 0);
    assert_eq!(default.max_size, usize::MAX);
    assert!(default.visible.is_none());
    assert_eq!(
        allocate_stack_sizes(&[StackEntry::default()], &[5], None, 0),
        vec![5],
        "empty options use intrinsic sizing"
    );

    let configured = &stack.data.entries[1];
    let node = json!({
        "type": "vstack",
        "gap": stack.data.gap,
        "align": "start",
        "entryShapes": [
            {
                "keys": [],
                "basis": Value::Null,
                "grow": Value::Null,
                "shrink": Value::Null,
                "minSize": Value::Null,
                "maxSize": Value::Null,
                "visibleAt6": true,
                "visibleAt10": true,
            },
            {
                "keys": ["basis", "grow", "shrink", "minSize", "maxSize", "visible"],
                "basis": configured.basis,
                "grow": configured.grow,
                "shrink": configured.shrink,
                "minSize": configured.min_size,
                "maxSize": configured.max_size,
                "visibleAt6": configured.is_visible(StackViewport { width: 6, height: 9 }),
                "visibleAt10": configured.is_visible(StackViewport { width: 10, height: 9 }),
            },
        ],
    });

    let mut outputs = vec![stack.render(10), stack.render(6)];
    stack.remove_child(first);
    outputs.push(stack.render(10));
    stack.clear();
    outputs.push(stack.render(10));

    let mut align_outputs = serde_json::Map::new();
    for (name, align) in [
        ("stretch", Align::Stretch),
        ("start", Align::Start),
        ("center", Align::Center),
        ("end", Align::End),
    ] {
        let mut horizontal = HStack::new(1, align);
        horizontal.add_child(stack_child(
            format!("left-{name}"),
            &["1", "2", "3"],
            &events,
        ));
        horizontal.add_child(stack_child(format!("right-{name}"), &["x"], &events));
        align_outputs.insert(name.to_owned(), json!(horizontal.render(25)));
    }

    let allocation_cases = json!([
        {
            "name": "empty-options-intrinsic",
            "result": allocate_stack_sizes(
                &[StackEntry::default(), StackEntry::default()],
                &[5, 2],
                None,
                0,
            ),
        },
        {
            "name": "grow-gap-max",
            "result": allocate_stack_sizes(
                &[
                    StackEntry { basis: Some(2), grow: 1, max_size: 4, ..StackEntry::default() },
                    StackEntry { grow: 2, ..StackEntry::default() },
                ],
                &[9, 3],
                Some(12),
                1,
            ),
        },
        {
            "name": "shrink-min-weighted",
            "result": allocate_stack_sizes(
                &[
                    StackEntry { basis: Some(8), shrink: 1, min_size: 5, ..StackEntry::default() },
                    StackEntry { basis: Some(6), shrink: 3, min_size: 1, ..StackEntry::default() },
                ],
                &[0, 0],
                Some(9),
                0,
            ),
        },
    ]);

    assert_eq!(
        json!({
            "node": node,
            "outputs": outputs,
            "alignOutputs": Value::Object(align_outputs),
            "allocationCases": allocation_cases,
            "events": events.lock().expect("events").clone(),
        }),
        fixture()["stack"]
    );
}

#[test]
fn cancellable_loader_abort_state_callbacks_and_render_are_differential() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let render_events = events.clone();
    let mut loader = CancellableLoader::with_runtime(
        "Working",
        Some(Box::new(|text| format!("spinner:{text}"))),
        Some(Box::new(|text| format!("message:{text}"))),
        Some(LoaderIndicatorOptions {
            frames: Some(vec!["A".into(), "B".into()]),
            interval_ms: Some(10_000),
        }),
        Box::new(move || {
            render_events
                .lock()
                .expect("events")
                .push("request-render".into());
        }),
    );
    let abort_events = events.clone();
    loader.on_abort = Some(Box::new(move || {
        abort_events.lock().expect("events").push("abort".into());
    }));
    let retained_signal = loader.signal();
    let signal_events = events.clone();
    retained_signal.on_cancel(move || {
        signal_events
            .lock()
            .expect("events")
            .push("signal-abort".into());
    });
    let mut outputs = vec![loader.render(24)];
    let mut states = vec![json!({
        "aborted": loader.aborted(),
        "sameSignal": retained_signal.ptr_eq(&loader.signal()),
        "frame": 0,
        "scheduled": loader.is_animation_scheduled(),
        "frames": ["A", "B"],
    })];
    loader.advance_frame();
    outputs.push(loader.render(24));
    states.push(json!({ "frame": 1, "scheduled": loader.is_animation_scheduled() }));
    loader.stop();
    states.push(json!({ "frame": 1, "scheduled": loader.is_animation_scheduled() }));
    loader.start();
    states.push(json!({ "frame": 1, "scheduled": loader.is_animation_scheduled() }));
    loader.set_indicator(Some(LoaderIndicatorOptions {
        frames: Some(vec!["!".into()]),
        interval_ms: Some(5),
    }));
    states
        .push(json!({ "frame": 0, "scheduled": loader.is_animation_scheduled(), "frames": ["!"] }));
    loader.set_indicator(Some(LoaderIndicatorOptions {
        frames: Some(vec![]),
        interval_ms: Some(5),
    }));
    states.push(json!({ "frame": 0, "scheduled": loader.is_animation_scheduled(), "frames": [] }));
    loader.set_indicator(Some(LoaderIndicatorOptions {
        frames: None,
        interval_ms: Some(10_000),
    }));
    states.push(
        json!({ "frame": 0, "scheduled": loader.is_animation_scheduled(), "frameCount": 10 }),
    );
    loader.set_text("inherited");
    outputs.push(loader.render(12));
    loader.set_message("Working again");
    outputs.push(loader.render(12));
    loader.handle_input("x");
    states.push(json!({ "aborted": loader.aborted() }));
    loader.handle_input("\x1b");
    states.push(json!({ "aborted": loader.aborted() }));
    loader.handle_input("\x1b");
    states.push(json!({ "aborted": loader.aborted() }));
    outputs.push(loader.render(12));
    loader.dispose();
    assert_eq!(
        json!({
            "outputs": outputs,
            "states": states,
            "events": events.lock().expect("events").clone(),
        }),
        fixture()["cancellableLoader"]
    );
}

#[test]
fn select_list_navigation_filter_and_callbacks() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut list = SelectList::new(select_items(), 3, select_theme());
    let sink = events.clone();
    list.set_on_selection_change(Some(Box::new(move |item| {
        sink.lock()
            .expect("events")
            .push(format!("change:{}", item.value));
    })));
    let sink = events.clone();
    list.set_on_select(Some(Box::new(move |item| {
        sink.lock()
            .expect("events")
            .push(format!("select:{}", item.value));
    })));
    let sink = events.clone();
    list.set_on_cancel(Some(Box::new(move || {
        sink.lock().expect("events").push("cancel".into());
    })));
    let mut outputs = vec![list.render(64)];
    list.handle_input("\x1b[B");
    outputs.push(list.render(64));
    list.set_selected_index(5);
    outputs.push(list.render(64));
    list.handle_input("\x1b[B");
    outputs.push(list.render(64));
    list.handle_input("\x1b[A");
    list.handle_input("\r");
    list.handle_input("\x1b");
    let selected_before_filter = list.selected_item().map(|item| item.value.clone());
    list.set_filter("co");
    outputs.push(list.render(64));
    let selected_after_filter = list.selected_item().map(|item| item.value.clone());
    list.set_filter("zzz");
    outputs.push(list.render(18));
    let actual = json!({
        "name": "navigation-filter-callbacks",
        "outputs": outputs,
        "events": events.lock().expect("events").clone(),
        "selectedBeforeFilter": selected_before_filter,
        "selectedAfterFilter": selected_after_filter,
    });
    assert_eq!(&actual, case("selectList", "navigation-filter-callbacks"));
}

#[test]
fn select_list_layout_truncation_and_bounds() {
    let contexts = Arc::new(Mutex::new(Vec::<Value>::new()));
    let context_sink = contexts.clone();
    let layout = SelectListLayoutOptions {
        min_primary_column_width: Some(12),
        max_primary_column_width: Some(20),
        truncate_primary: Some(Box::new(move |context: &SelectListTruncateContext| {
            context_sink.lock().expect("contexts").push(json!({
                "text": context.text,
                "maxWidth": context.max_width,
                "columnWidth": context.column_width,
                "value": context.item.value,
                "isSelected": context.is_selected,
            }));
            context.text.to_uppercase()
        })),
    };
    let mut list = SelectList::with_layout(
        vec![
            SelectItem::with_description(
                "alpha-long-value",
                "Alpha primary column",
                "alpha description",
            ),
            SelectItem::with_description("beta", "Beta", "beta description"),
        ],
        5,
        select_theme(),
        layout,
    );
    let mut outputs = vec![list.render(58), list.render(22)];
    list.set_selected_index(-20);
    outputs.push(list.render(58));
    list.set_selected_index(99);
    outputs.push(list.render(58));
    let actual = json!({
        "name": "layout-truncation-bounds",
        "outputs": outputs,
        "contexts": contexts.lock().expect("contexts").clone(),
    });
    assert_eq!(&actual, case("selectList", "layout-truncation-bounds"));
}

#[test]
fn select_list_empty_normalized_descriptions_follow_js_truthiness() {
    let mut list = SelectList::new(
        vec![
            SelectItem::with_description("empty", "Empty description", ""),
            SelectItem::with_description("newlines", "Newline description", "\r\n\n"),
            SelectItem::with_description("spaces", "Whitespace description", " \r\n "),
        ],
        3,
        select_theme(),
    );
    let mut outputs = vec![list.render(64)];
    list.set_selected_index(1);
    outputs.push(list.render(64));
    list.set_selected_index(2);
    outputs.push(list.render(64));
    let actual = json!({
        "name": "description-truthiness",
        "outputs": outputs,
    });
    assert_eq!(&actual, case("selectList", "description-truthiness"));
}

#[test]
fn settings_list_navigation_cycle_and_external_update() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let change_sink = events.clone();
    let cancel_sink = events.clone();
    let mut list = SettingsList::new(
        make_settings(),
        3,
        settings_theme(),
        Box::new(move |id, value| {
            change_sink
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(move || cancel_sink.lock().expect("events").push("cancel".into())),
    );
    let mut outputs = vec![list.render(52)];
    list.handle_input("\x1b[B");
    outputs.push(list.render(52));
    list.handle_input("\r");
    outputs.push(list.render(52));
    list.update_value("theme", "solarized");
    list.handle_input("\x1b[A");
    outputs.push(list.render(34));
    list.handle_input(" ");
    outputs.push(list.render(34));
    list.handle_input("\x1b");
    let actual = json!({
        "name": "navigation-cycle-update",
        "outputs": outputs,
        "events": events.lock().expect("events").clone(),
    });
    assert_eq!(&actual, case("settingsList", "navigation-cycle-update"));
}

#[test]
fn settings_list_search_input_and_filter() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let change_sink = events.clone();
    let cancel_sink = events.clone();
    let mut list = SettingsList::with_options(
        make_settings(),
        4,
        settings_theme(),
        Box::new(move |id, value| {
            change_sink
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(move || cancel_sink.lock().expect("events").push("cancel".into())),
        SettingsListOptions {
            enable_search: true,
        },
    );
    let mut outputs = vec![list.render(46)];
    list.handle_input("l");
    list.handle_input("a");
    outputs.push(list.render(46));
    list.handle_input("\x7f");
    outputs.push(list.render(30));
    list.handle_input(" ");
    outputs.push(list.render(30));
    list.handle_input("\r");
    outputs.push(list.render(46));
    list.handle_input("\x1b");
    let actual = json!({
        "name": "search-input-filter",
        "outputs": outputs,
        "events": events.lock().expect("events").clone(),
    });
    assert_eq!(&actual, case("settingsList", "search-input-filter"));
}

fn run_search_steps(steps: &[(&str, usize)]) -> Vec<String> {
    let mut list = SettingsList::with_options(
        make_settings(),
        4,
        settings_theme(),
        Box::new(|_, _| {}),
        Box::new(|| {}),
        SettingsListOptions {
            enable_search: true,
        },
    );
    steps
        .iter()
        .map(|(input, width)| {
            list.handle_input(input);
            list.render(*width)
                .into_iter()
                .next()
                .expect("search input line")
        })
        .collect()
}

#[test]
fn settings_list_search_editor_key_subset_is_differential() {
    let actual = json!({
        "name": "search-editor-key-subset",
        "grapheme": run_search_steps(&[
            ("A👩‍💻e\u{301}Z", 20),
            ("\x1b[D", 20),
            ("\x1b[D", 20),
            ("\x04", 20),
            ("\x7f", 20),
        ]),
        "wordKillYankUndo": run_search_steps(&[
            ("alpha-beta gamma", 24),
            ("\x1bb", 24),
            ("\x1bb", 24),
            ("\x1bd", 24),
            ("\x17", 24),
            ("\x19", 24),
            ("\x01", 24),
            ("\x1bf", 24),
            ("\x0b", 24),
            ("\x19", 24),
            ("\x15", 24),
            ("\x19", 24),
            ("\x1by", 24),
            ("\x1f", 24),
            ("\x1f", 24),
        ]),
        "pasteViewport": run_search_steps(&[
            ("Q", 12),
            ("\x1b[200~one\r\n", 12),
            ("two\t三\n\x1b[201~Z", 12),
            ("\x01", 12),
            ("\x1bf", 12),
            ("\x05", 12),
        ]),
        "undoCoalescing": run_search_steps(&[
            ("a", 16),
            ("b", 16),
            (" ", 16),
            ("c", 16),
            ("\x1f", 16),
            ("\x1f", 16),
            ("\x1f", 16),
        ]),
        "wordBoundaries": run_search_steps(&[
            ("don't 3.14 word🙂!!", 32),
            ("\x01", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x1bf", 32),
            ("\x05", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
            ("\x1bb", 32),
        ]),
    });
    assert_eq!(&actual, case("settingsList", "search-editor-key-subset"));
}

#[test]
fn settings_list_empty_modes() {
    let mut no_search = SettingsList::new(
        vec![],
        3,
        settings_theme(),
        Box::new(|_, _| {}),
        Box::new(|| {}),
    );
    let mut search = SettingsList::with_options(
        vec![],
        3,
        settings_theme(),
        Box::new(|_, _| {}),
        Box::new(|| {}),
        SettingsListOptions {
            enable_search: true,
        },
    );
    let actual = json!({
        "name": "empty-lists",
        "outputs": [no_search.render(24), search.render(24)],
        "events": [],
    });
    assert_eq!(&actual, case("settingsList", "empty-lists"));
}

struct OracleSubmenu {
    current_value: String,
    events: Arc<Mutex<Vec<String>>>,
    done: Option<SubmenuDone>,
}

impl Component for OracleSubmenu {
    fn invalidate(&mut self) {
        self.events
            .lock()
            .expect("events")
            .push("submenu:invalidate".into());
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        vec![format!("submenu:{}:{width}", self.current_value)]
    }

    fn handle_input(&mut self, data: &str) {
        self.events.lock().expect("events").push(format!(
            "submenu:input:{}",
            serde_json::to_string(data).expect("json string")
        ));
        if data == "x"
            && let Some(done) = self.done.as_mut()
        {
            done(Some("new".into()));
        }
    }
}

#[test]
fn settings_list_submenu_delegation_and_close() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let submenu_events = events.clone();
    let item = SettingItem::new("backend", "Backend", "old").with_submenu(Box::new(
        move |current_value, done| {
            Box::new(OracleSubmenu {
                current_value: current_value.to_string(),
                events: submenu_events.clone(),
                done: Some(done),
            })
        },
    ));
    let change_sink = events.clone();
    let cancel_sink = events.clone();
    let mut list = SettingsList::new(
        vec![item],
        3,
        settings_theme(),
        Box::new(move |id, value| {
            change_sink
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(move || cancel_sink.lock().expect("events").push("cancel".into())),
    );
    let mut outputs = vec![list.render(30)];
    list.handle_input("\r");
    outputs.push(list.render(30));
    list.invalidate();
    list.handle_input("x");
    outputs.push(list.render(30));
    let actual = json!({
        "name": "submenu-delegation",
        "outputs": outputs,
        "events": events.lock().expect("events").clone(),
    });
    assert_eq!(&actual, case("settingsList", "submenu-delegation"));
}

struct ExternalDrainSubmenu {
    events: Arc<Mutex<Vec<String>>>,
}

impl Component for ExternalDrainSubmenu {
    fn invalidate(&mut self) {
        self.events
            .lock()
            .expect("events")
            .push("submenu:invalidate".into());
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        vec![format!("external-submenu:{width}")]
    }
}

struct RenderDrainSubmenu {
    events: Arc<Mutex<Vec<String>>>,
    done: Option<SubmenuDone>,
}

impl Component for RenderDrainSubmenu {
    fn invalidate(&mut self) {
        self.events
            .lock()
            .expect("events")
            .push("submenu:invalidate-after-render".into());
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.events
            .lock()
            .expect("events")
            .push(format!("submenu:render:{width}"));
        if let Some(done) = self.done.as_mut() {
            done(Some("rendered".into()));
        }
        vec![format!("render-submenu:{width}")]
    }
}

struct InvalidateDrainSubmenu {
    events: Arc<Mutex<Vec<String>>>,
    done: Option<SubmenuDone>,
}

impl Component for InvalidateDrainSubmenu {
    fn invalidate(&mut self) {
        self.events
            .lock()
            .expect("events")
            .push("submenu:invalidate".into());
        if let Some(done) = self.done.as_mut() {
            done(Some("invalidated".into()));
        }
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        vec![format!("invalidate-submenu:{width}")]
    }
}

#[test]
fn settings_list_drains_external_render_and_invalidate_submenu_completion() {
    let external_events = Arc::new(Mutex::new(Vec::<String>::new()));
    let external_done = Arc::new(Mutex::new(None::<SubmenuDone>));
    let done_sink = external_done.clone();
    let submenu_events = external_events.clone();
    let external_item = SettingItem::new("external", "External", "old").with_submenu(Box::new(
        move |_current, done| {
            *done_sink.lock().expect("external done") = Some(done);
            Box::new(ExternalDrainSubmenu {
                events: submenu_events.clone(),
            })
        },
    ));
    let change_events = external_events.clone();
    let mut external_list = SettingsList::new(
        vec![external_item],
        3,
        settings_theme(),
        Box::new(move |id, value| {
            change_events
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(|| {}),
    );
    let mut external_outputs = vec![external_list.render(28)];
    external_list.handle_input("\r");
    external_outputs.push(external_list.render(28));
    external_done
        .lock()
        .expect("external done")
        .take()
        .expect("external completion")(Some("async".into()));
    external_list.invalidate();
    external_outputs.push(external_list.render(28));

    let render_events = Arc::new(Mutex::new(Vec::<String>::new()));
    let submenu_events = render_events.clone();
    let render_item = SettingItem::new("render", "Render", "old").with_submenu(Box::new(
        move |_current, done| {
            Box::new(RenderDrainSubmenu {
                events: submenu_events.clone(),
                done: Some(done),
            })
        },
    ));
    let change_events = render_events.clone();
    let cancel_events = render_events.clone();
    let mut render_list = SettingsList::new(
        vec![render_item],
        3,
        settings_theme(),
        Box::new(move |id, value| {
            change_events
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(move || {
            cancel_events.lock().expect("events").push("cancel".into());
        }),
    );
    render_list.handle_input("\r");
    let mut render_outputs = vec![render_list.render(26)];
    render_list.handle_input("\x1b");
    render_list.invalidate();
    render_outputs.push(render_list.render(26));

    let invalidate_events = Arc::new(Mutex::new(Vec::<String>::new()));
    let submenu_events = invalidate_events.clone();
    let invalidate_item = SettingItem::new("invalidate", "Invalidate", "old").with_submenu(
        Box::new(move |_current, done| {
            Box::new(InvalidateDrainSubmenu {
                events: submenu_events.clone(),
                done: Some(done),
            })
        }),
    );
    let change_events = invalidate_events.clone();
    let cancel_events = invalidate_events.clone();
    let mut invalidate_list = SettingsList::new(
        vec![invalidate_item],
        3,
        settings_theme(),
        Box::new(move |id, value| {
            change_events
                .lock()
                .expect("events")
                .push(format!("change:{id}:{value}"));
        }),
        Box::new(move || {
            cancel_events.lock().expect("events").push("cancel".into());
        }),
    );
    invalidate_list.handle_input("\r");
    invalidate_list.invalidate();
    invalidate_list.handle_input("\x1b");
    let invalidate_outputs = vec![invalidate_list.render(30)];

    let actual = json!({
        "name": "submenu-completion-drainage",
        "external": {
            "outputs": external_outputs,
            "events": external_events.lock().expect("events").clone(),
        },
        "render": {
            "outputs": render_outputs,
            "events": render_events.lock().expect("events").clone(),
        },
        "invalidate": {
            "outputs": invalidate_outputs,
            "events": invalidate_events.lock().expect("events").clone(),
        },
    });
    assert_eq!(&actual, case("settingsList", "submenu-completion-drainage"));
}

fn autocomplete_item_json(item: &AutocompleteItem) -> Value {
    let mut value = json!({ "value": item.value, "label": item.label });
    if let Some(description) = &item.description {
        value["description"] = json!(description);
    }
    value
}

fn provider(temp_root: &std::path::Path, fake_fd: PathBuf) -> CombinedAutocompleteProvider {
    let mut config = SlashCommand::new("config");
    config.description = Some("Configure the application".into());
    config.argument_hint = Some("[scope]".into());
    let mut model = SlashCommand::new("model");
    model.description = Some("Select model".into());
    model.get_argument_completions = Some(Box::new(|prefix| {
        let mut pending_once = true;
        ArgumentCompletionResult::future(std::future::poll_fn(move |context| {
            if pending_once {
                pending_once = false;
                context.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(Some(
                vec![
                    AutocompleteItem::with_description("fast", "fast", "Fast model"),
                    AutocompleteItem::with_description("full", "full", "Full model"),
                    AutocompleteItem::new("reasoning", "reasoning"),
                ]
                .into_iter()
                .filter(|item| item.value.starts_with(&prefix))
                .collect(),
            ))
        }))
    }));
    let mut mode = SlashCommand::new("mode");
    mode.get_argument_completions = Some(Box::new(|prefix| {
        ArgumentCompletionResult::ready(Some(
            vec![
                AutocompleteItem::new("safe", "safe"),
                AutocompleteItem::new("speed", "speed"),
            ]
            .into_iter()
            .filter(|item| item.value.starts_with(&prefix))
            .collect(),
        ))
    }));
    CombinedAutocompleteProvider::new(
        vec![
            AutocompleteCommand::Slash(config),
            AutocompleteCommand::Item(AutocompleteItem::with_description(
                "commit",
                "Commit",
                "Commit current changes",
            )),
            AutocompleteCommand::Slash(model),
            AutocompleteCommand::Slash(mode),
        ],
        temp_root,
        Some(fake_fd),
    )
}

fn with_autocomplete_fixture(test: impl FnOnce(&CombinedAutocompleteProvider)) {
    // The pinned reference reads real directory entries before sorting them.
    // Guard before any filesystem writes so the oracle and Rust harness receive
    // identical input on both case-sensitive and case-insensitive hosts.
    assert_case_fold_unique(AUTOCOMPLETE_LOCALE_ORDER_NAMES);

    let temp_root = std::env::temp_dir().join(format!(
        "pie-tui-m3-rust-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&temp_root);
    std::fs::create_dir_all(temp_root.join("alpha-dir")).expect("alpha dir");
    std::fs::create_dir_all(temp_root.join("space dir")).expect("space dir");
    std::fs::create_dir_all(temp_root.join("locale-order")).expect("locale dir");
    std::fs::write(temp_root.join("alpha.txt"), "alpha").expect("alpha file");
    std::fs::write(temp_root.join("beta.md"), "beta").expect("beta file");
    std::fs::write(temp_root.join("space file.txt"), "space").expect("space file");
    std::fs::write(temp_root.join("🎉.txt"), "astral").expect("astral file");
    for name in AUTOCOMPLETE_LOCALE_ORDER_NAMES {
        std::fs::write(temp_root.join("locale-order").join(name), name).expect("locale file");
    }
    let fake_fd = temp_root.join("fake-fd.mjs");
    let common_fd_args = json!([
        "--base-directory",
        temp_root.to_string_lossy(),
        "--max-results",
        "100",
        "--type",
        "f",
        "--type",
        "d",
        "--follow",
        "--hidden",
        "--exclude",
        ".git",
        "--exclude",
        ".git/*",
        "--exclude",
        ".git/**",
    ]);
    let common = common_fd_args.as_array().expect("common fd args");
    let allowed_fd_args = json!([
        common
            .iter()
            .cloned()
            .chain([json!("li")])
            .collect::<Vec<_>>(),
        common
            .iter()
            .cloned()
            .chain([json!("sp")])
            .collect::<Vec<_>>(),
        common
            .iter()
            .cloned()
            .chain([json!("--full-path"), json!(r"src[\\/]li")])
            .collect::<Vec<_>>(),
        common
            .iter()
            .cloned()
            .chain([json!("--full-path"), json!(r"src\.\[x\][\\/]li\+"),])
            .collect::<Vec<_>>(),
    ]);
    let fake_fd_source = r#"#!/usr/bin/env node
const actual = process.argv.slice(2);
const allowed = __ALLOWED__;
if (!allowed.some((expected) => JSON.stringify(expected) === JSON.stringify(actual))) {
  process.stderr.write(`unexpected argv: ${JSON.stringify(actual)}\n`);
  process.exit(64);
}
const escapedQuery = "src\\.\\[x\\][\\\\/]li\\+";
process.stdout.write(actual.at(-1) === escapedQuery
  ? "src.[x]/li+.rs\n"
  : "src/lib.rs\nsrc/tools/\ndocs/readme.md\nlib-top.txt\nspace dir/\nspace file.txt\n");
"#
    .replace("__ALLOWED__", &allowed_fd_args.to_string());
    std::fs::write(&fake_fd, fake_fd_source).expect("fake fd");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_fd, std::fs::Permissions::from_mode(0o755))
            .expect("fake fd executable");
    }
    test(&provider(&temp_root, fake_fd));
    std::fs::remove_dir_all(temp_root).expect("autocomplete fixture cleanup");
}

#[test]
fn autocomplete_locale_order_fixture_is_case_fold_unique() {
    assert_case_fold_unique(AUTOCOMPLETE_LOCALE_ORDER_NAMES);
}

#[test]
fn autocomplete_suggestion_modes_are_differential() {
    with_autocomplete_fixture(|provider| {
        let expected = &fixture()["autocomplete"]["suggestions"];
        let scenarios = [
            ("slash-empty", vec!["/"], 1, false, false),
            ("slash-fuzzy", vec!["/cf"], 3, false, false),
            ("slash-arguments", vec!["/model f"], 8, false, false),
            ("slash-arguments-ready", vec!["/mode s"], 7, false, false),
            ("local-prefix", vec!["open ./a"], 8, false, false),
            ("forced-empty-token", vec!["open "], 5, true, false),
            ("quoted-space", vec!["open \"sp"], 8, false, false),
            ("at-fuzzy", vec!["attach @li"], 10, false, false),
            ("at-fuzzy-quoted", vec!["attach @\"sp"], 11, false, false),
            (
                "at-fuzzy-multi-segment",
                vec!["attach @src/li"],
                js_len("attach @src/li"),
                false,
                false,
            ),
            (
                "at-fuzzy-escaped-multi",
                vec!["attach @src.[x]/li+"],
                js_len("attach @src.[x]/li+"),
                false,
                false,
            ),
            (
                "astral-cursor-input",
                vec!["🎉 attach @li"],
                js_len("🎉 attach @li"),
                false,
                false,
            ),
            (
                "astral-local-prefix",
                vec!["🎉 open ./🎉"],
                js_len("🎉 open ./🎉"),
                false,
                false,
            ),
            (
                "locale-order",
                vec!["open ./locale-order/"],
                js_len("open ./locale-order/"),
                false,
                false,
            ),
            ("aborted-at-fuzzy", vec!["@li"], 3, false, true),
        ];
        let actual = scenarios
            .into_iter()
            .map(|(name, lines, col, force, aborted)| {
                let lines = lines.into_iter().map(str::to_string).collect::<Vec<_>>();
                let controller = CancellationController::new();
                if aborted {
                    controller.cancel();
                }
                let result = block_on(provider.get_suggestions(
                    &lines,
                    0,
                    col,
                    AutocompleteOptions {
                        force,
                        signal: controller.signal(),
                    },
                ));
                json!({
                    "name": name,
                    "lines": lines,
                    "line": 0,
                    "col": col,
                    "force": force,
                    "result": result.map(|result| json!({
                        "items": result.items.iter().map(autocomplete_item_json).collect::<Vec<_>>(),
                        "prefix": result.prefix,
                    })),
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(&json!(actual), expected);
        assert_eq!(
            AutocompleteProvider::trigger_characters(provider),
            None,
            "CombinedAutocompleteProvider leaves optional triggerCharacters absent"
        );
        assert_eq!(fixture()["autocomplete"]["triggerCharacters"], Value::Null);
    });
}

#[test]
fn autocomplete_completion_application_is_differential() {
    with_autocomplete_fixture(|provider| {
        let expected = &fixture()["autocomplete"]["completionCases"];
        let scenarios = [
            ("slash", "/cf tail", 3, "config", "config", "/cf"),
            (
                "attachment-file",
                "see @al now",
                7,
                "@alpha.txt",
                "alpha.txt",
                "@al",
            ),
            (
                "attachment-dir",
                "see @al",
                7,
                "@alpha-dir/",
                "alpha-dir/",
                "@al",
            ),
            (
                "quoted-existing-close",
                "open \"sp\" tail",
                8,
                "\"space file.txt\"",
                "space file.txt",
                "\"sp",
            ),
            ("argument", "/model f tail", 8, "fast", "fast", "f"),
            (
                "plain-path",
                "open ./a tail",
                8,
                "./alpha.txt",
                "alpha.txt",
                "./a",
            ),
            (
                "astral-input-output",
                "🎉 see @x tail",
                js_len("🎉 see @x"),
                "@🎉.txt",
                "🎉.txt",
                "@x",
            ),
        ];
        let actual = scenarios
            .into_iter()
            .map(|(name, line, col, value, label, prefix)| {
                let lines = vec![line.to_string()];
                let item = AutocompleteItem::new(value, label);
                let result = provider.apply_completion(&lines, 0, col, &item, prefix);
                json!({
                    "name": name,
                    "lines": lines,
                    "line": 0,
                    "col": col,
                    "item": { "value": value, "label": label },
                    "prefix": prefix,
                    "result": {
                        "lines": result.lines,
                        "cursorLine": result.cursor_line,
                        "cursorCol": result.cursor_col,
                    },
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(&json!(actual), expected);
    });
}

#[test]
fn autocomplete_file_trigger_context_is_differential() {
    with_autocomplete_fixture(|provider| {
        let expected = &fixture()["autocomplete"]["triggerCases"];
        let scenarios = [
            ("slash-command", "/conf", 5),
            ("slash-argument", "/conf path", 10),
            ("plain", "hello", 5),
        ];
        let actual = scenarios
            .into_iter()
            .map(|(name, line, col)| {
                let lines = vec![line.to_string()];
                json!({
                    "name": name,
                    "lines": lines,
                    "line": 0,
                    "col": col,
                    "result": provider.should_trigger_file_completion(&lines, 0, col),
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(&json!(actual), expected);
    });
}

#[test]
fn autocomplete_live_cancellation_kills_and_reaps_fd_child() {
    let temp_root = std::env::temp_dir().join(format!(
        "pie-tui-m3-cancel-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&temp_root);
    std::fs::create_dir_all(&temp_root).expect("cancellation fixture");
    let started = temp_root.join("started");
    let completed = temp_root.join("completed");
    let fake_fd = temp_root.join("sleeping-fd.mjs");
    let expected_args = json!([
        "--base-directory",
        temp_root.to_string_lossy(),
        "--max-results",
        "100",
        "--type",
        "f",
        "--type",
        "d",
        "--follow",
        "--hidden",
        "--exclude",
        ".git",
        "--exclude",
        ".git/*",
        "--exclude",
        ".git/**",
        "slow",
    ]);
    let sleeping_source = r#"#!/usr/bin/env node
import { writeFileSync } from "node:fs";
const actual = process.argv.slice(2);
const expected = __EXPECTED__;
if (JSON.stringify(actual) !== JSON.stringify(expected)) {
  process.stderr.write(`unexpected argv: ${JSON.stringify(actual)}\n`);
  process.exit(64);
}
writeFileSync(__STARTED__, String(process.pid));
setTimeout(() => {
  writeFileSync(__COMPLETED__, "completed");
  process.stdout.write("slow.txt\n");
}, 5000);
"#
    .replace("__EXPECTED__", &expected_args.to_string())
    .replace("__STARTED__", &json!(started.to_string_lossy()).to_string())
    .replace(
        "__COMPLETED__",
        &json!(completed.to_string_lossy()).to_string(),
    );
    std::fs::write(&fake_fd, sleeping_source).expect("sleeping fake fd");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_fd, std::fs::Permissions::from_mode(0o755))
            .expect("sleeping fake fd executable");
    }

    let provider = CombinedAutocompleteProvider::new(Vec::new(), &temp_root, Some(fake_fd));
    let controller = CancellationController::new();
    let request_signal = controller.signal();
    let retained_signal = request_signal.clone();
    assert!(request_signal.ptr_eq(&retained_signal));
    let callback_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let callback_sink = callback_count.clone();
    retained_signal.on_cancel(move || {
        callback_sink.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let lines = vec!["@slow".to_string()];
    let mut future = Box::pin(provider.get_suggestions(
        &lines,
        0,
        5,
        AutocompleteOptions {
            force: false,
            signal: request_signal,
        },
    ));
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    assert!(
        Pin::as_mut(&mut future).poll(&mut context).is_pending(),
        "fd work yields instead of blocking the async caller"
    );

    // A cold shared runner can defer the Node child while the Rust test process
    // is still contending for CPU. The marker is the synchronization contract;
    // give exec a bounded window and always reap the child on timeout.
    let start_deadline = Instant::now() + Duration::from_secs(10);
    while !started.exists() && Instant::now() < start_deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if !started.exists() {
        assert!(controller.cancel(), "timed-out fd child can be cancelled");
        let _ = block_on(future);
        std::fs::remove_dir_all(&temp_root).expect("timed-out cancellation fixture cleanup");
        panic!("sleeping fd child reached its wait state");
    }
    let child_pid = std::fs::read_to_string(&started).expect("started pid");
    let cancel_started = Instant::now();
    assert!(controller.cancel());
    assert!(!controller.cancel(), "cancellation is one-shot");
    let result = block_on(future);
    assert!(result.is_none());
    assert!(
        cancel_started.elapsed() < Duration::from_secs(2),
        "cancellation must not wait for the five-second child"
    );
    assert!(retained_signal.aborted());
    assert_eq!(callback_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!completed.exists(), "SIGKILL prevented delayed completion");
    #[cfg(unix)]
    {
        let process_state = std::process::Command::new("ps")
            .args(["-p", child_pid.trim(), "-o", "stat="])
            .output()
            .expect("ps child state");
        assert!(
            String::from_utf8_lossy(&process_state.stdout)
                .trim()
                .is_empty(),
            "killed fd child was waited and is not a zombie"
        );
    }
    std::fs::remove_dir_all(temp_root).expect("cancellation fixture cleanup");
}
