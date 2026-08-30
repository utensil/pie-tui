use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex, MutexGuard};

use pie_app::{
    TuiAltScreen, TuiAltScreenEnvironment, TuiAltScreenOptions, TuiHostTask, TuiMainScreen,
    TuiScreenRuntime, TuiTaskId,
};
use pie_components::{
    Component, ComponentHandle, OverlayAnchor, OverlayOptions, SizeValue, Tui, TuiStopOptions,
};
use pie_core::keybindings::{KeybindingsManager, global::set_keybindings};
use pie_core::screen::CURSOR_MARKER;
use pie_core::terminal_image::{CellDimensions, ImageProtocol, KittyImageMetadata};
use pie_term::capabilities::{TerminalCapabilities, get_capabilities, set_capabilities};
use pie_term::renderer::RenderState;
use pie_term::{InputHandler, ResizeHandler, Terminal};
use serde_json::{Value, json};

type Trace = Rc<RefCell<Vec<Value>>>;
type ProbeHarness = (
    ComponentHandle<Probe>,
    Rc<RefCell<Vec<String>>>,
    Rc<RefCell<bool>>,
);
type MainHarness = (TuiMainScreen, FakeTerminal, Rc<RefCell<ClockFacts>>, Trace);
type AltHarness = (TuiAltScreen, FakeTerminal, Rc<RefCell<ClockFacts>>, Trace);
type OneShotCallback = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

static GLOBAL_STATE_LOCK: Mutex<()> = Mutex::new(());

fn global_state_lock() -> MutexGuard<'static, ()> {
    GLOBAL_STATE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/main-alt-controller.json")).unwrap()
}

fn fixture_case(name: &str) -> Value {
    fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap()
        .clone()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskKind {
    Tick,
    Timer,
}

#[derive(Clone, Copy)]
struct Scheduled {
    due: u64,
    task: TuiHostTask,
    kind: TaskKind,
}

struct ClockFacts {
    now: u64,
    next_id: u64,
    scheduled: BTreeMap<TuiTaskId, Scheduled>,
    facts: Vec<Value>,
    images_supported: bool,
    cell_dimensions: CellDimensions,
}

impl ClockFacts {
    fn new(now: u64, images_supported: bool) -> Self {
        Self {
            now,
            next_id: 0,
            scheduled: BTreeMap::new(),
            facts: Vec::new(),
            images_supported,
            cell_dimensions: CellDimensions::default(),
        }
    }
}

struct FakeRuntime {
    facts: Rc<RefCell<ClockFacts>>,
}

impl TuiScreenRuntime for FakeRuntime {
    fn now_ms(&self) -> u64 {
        self.facts.borrow().now
    }

    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId {
        let mut facts = self.facts.borrow_mut();
        facts.next_id += 1;
        let id = TuiTaskId(facts.next_id);
        let kind = match task {
            TuiHostTask::DrainTerminalEvents
            | TuiHostTask::ScheduleRender
            | TuiHostTask::ImmediateRender => TaskKind::Tick,
            _ => TaskKind::Timer,
        };
        let now = facts.now;
        let due = now.saturating_add(delay_ms);
        facts.scheduled.insert(id, Scheduled { due, task, kind });
        if kind == TaskKind::Tick {
            facts.facts.push(json!(["schedule-tick", id.0, now]));
        } else {
            facts
                .facts
                .push(json!(["schedule-timer", id.0, delay_ms, due]));
        }
        id
    }

    fn cancel_task(&mut self, task: TuiTaskId) {
        let mut facts = self.facts.borrow_mut();
        if facts
            .scheduled
            .remove(&task)
            .is_some_and(|task| task.kind == TaskKind::Timer)
        {
            let now = facts.now;
            facts.facts.push(json!(["cancel-timer", task.0, now]));
        }
    }

    fn images_supported(&self) -> bool {
        self.facts.borrow().images_supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        self.facts.borrow_mut().cell_dimensions = dimensions;
    }
}

struct ReentrantFlashRuntime {
    facts: Rc<RefCell<ClockFacts>>,
    trace: Trace,
    target: Rc<RefCell<Option<Weak<TuiAltScreen>>>>,
    flash_schedules: usize,
}

struct ReentrantWriteTerminal {
    inner: FakeTerminal,
    trigger: &'static str,
    callback: OneShotCallback,
}

impl Terminal for ReentrantWriteTerminal {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        self.inner.start(on_input, on_resize);
    }

    fn stop(&mut self) {
        self.inner.stop();
    }

    fn write(&mut self, data: &str) {
        self.inner.write(data);
        if data.contains(self.trigger)
            && let Some(callback) = self.callback.borrow_mut().take()
        {
            callback();
        }
    }

    fn columns(&self) -> usize {
        self.inner.columns()
    }

    fn rows(&self) -> usize {
        self.inner.rows()
    }

    fn kitty_protocol_active(&self) -> bool {
        self.inner.kitty_protocol_active()
    }

    fn move_by(&mut self, lines: isize) {
        self.inner.move_by(lines);
    }

    fn hide_cursor(&mut self) {
        self.inner.hide_cursor();
    }

    fn show_cursor(&mut self) {
        self.inner.show_cursor();
    }

    fn clear_line(&mut self) {
        self.inner.clear_line();
    }

    fn clear_from_cursor(&mut self) {
        self.inner.clear_from_cursor();
    }

    fn clear_screen(&mut self) {
        self.inner.clear_screen();
    }

    fn set_title(&mut self, title: &str) {
        self.inner.set_title(title);
    }

    fn set_progress(&mut self, active: bool) {
        self.inner.set_progress(active);
    }
}

impl TuiScreenRuntime for ReentrantFlashRuntime {
    fn now_ms(&self) -> u64 {
        self.facts.borrow().now
    }

    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId {
        let (id, due, kind) = {
            let mut facts = self.facts.borrow_mut();
            facts.next_id += 1;
            let id = TuiTaskId(facts.next_id);
            let kind = match task {
                TuiHostTask::DrainTerminalEvents
                | TuiHostTask::ScheduleRender
                | TuiHostTask::ImmediateRender => TaskKind::Tick,
                _ => TaskKind::Timer,
            };
            let due = facts.now.saturating_add(delay_ms);
            facts.scheduled.insert(id, Scheduled { due, task, kind });
            (id, due, kind)
        };
        {
            let mut facts = self.facts.borrow_mut();
            let now = facts.now;
            if kind == TaskKind::Tick {
                facts.facts.push(json!(["schedule-tick", id.0, now]));
            } else {
                facts
                    .facts
                    .push(json!(["schedule-timer", id.0, delay_ms, due]));
            }
        }
        if let TuiHostTask::AltFlashTimeout { flash_id } = task {
            self.trace
                .borrow_mut()
                .push(json!(["runtime", "schedule-flash", flash_id, id.0]));
            self.flash_schedules += 1;
            if self.flash_schedules == 2 {
                self.trace
                    .borrow_mut()
                    .push(json!(["runtime", "reentrant-stop", flash_id]));
                let target = self.target.borrow().as_ref().and_then(Weak::upgrade);
                if let Some(target) = target {
                    target.stop(TuiStopOptions {
                        preserve_screen: true,
                    });
                }
            }
        }
        id
    }

    fn cancel_task(&mut self, task: TuiTaskId) {
        let scheduled = self.facts.borrow_mut().scheduled.remove(&task);
        if let Some(Scheduled {
            task: TuiHostTask::AltFlashTimeout { flash_id },
            ..
        }) = scheduled
        {
            self.trace
                .borrow_mut()
                .push(json!(["runtime", "cancel-flash", flash_id, task.0]));
        }
    }

    fn images_supported(&self) -> bool {
        self.facts.borrow().images_supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        self.facts.borrow_mut().cell_dimensions = dimensions;
    }
}

fn run_current(clock: &Rc<RefCell<ClockFacts>>, mut dispatch: impl FnMut(TuiTaskId, TuiHostTask)) {
    loop {
        let next = {
            let facts = clock.borrow();
            facts
                .scheduled
                .iter()
                .filter(|(_, task)| task.kind == TaskKind::Tick)
                .map(|(id, task)| (*id, *task))
                .min_by_key(|(id, _)| *id)
                .or_else(|| {
                    facts
                        .scheduled
                        .iter()
                        .filter(|(_, task)| task.kind == TaskKind::Timer && task.due <= facts.now)
                        .map(|(id, task)| (*id, *task))
                        .min_by_key(|(id, task)| (task.due, *id))
                })
        };
        let Some((id, scheduled)) = next else {
            break;
        };
        {
            let mut facts = clock.borrow_mut();
            facts.scheduled.remove(&id);
            let now = facts.now;
            facts.facts.push(if scheduled.kind == TaskKind::Tick {
                json!(["run-tick", id.0, now])
            } else {
                json!(["run-timer", id.0, scheduled.due])
            });
        }
        dispatch(id, scheduled.task);
    }
}

fn advance(clock: &Rc<RefCell<ClockFacts>>, ms: u64, dispatch: impl FnMut(TuiTaskId, TuiHostTask)) {
    clock.borrow_mut().now += ms;
    run_current(clock, dispatch);
}

struct TerminalFacts {
    columns: usize,
    rows: usize,
    events: Vec<Value>,
    trace: Trace,
    on_input: Option<InputHandler>,
    on_resize: Option<ResizeHandler>,
}

#[derive(Clone)]
struct FakeTerminal {
    facts: Rc<RefCell<TerminalFacts>>,
}

impl FakeTerminal {
    fn new(columns: usize, rows: usize, trace: Trace) -> Self {
        Self {
            facts: Rc::new(RefCell::new(TerminalFacts {
                columns,
                rows,
                events: Vec::new(),
                trace,
                on_input: None,
                on_resize: None,
            })),
        }
    }

    fn record(&self, event: Value) {
        let mut facts = self.facts.borrow_mut();
        facts.events.push(event.clone());
        let mut trace_event = vec![json!("terminal")];
        trace_event.extend(event.as_array().unwrap().iter().cloned());
        facts.trace.borrow_mut().push(Value::Array(trace_event));
    }

    fn input(&self, data: &str) {
        self.facts
            .borrow()
            .trace
            .borrow_mut()
            .push(json!(["terminal", "input", data]));
    }

    fn feed_input_callback(&self, data: &str) {
        self.input(data);
        let mut callback = self
            .facts
            .borrow_mut()
            .on_input
            .take()
            .expect("terminal input callback must be registered");
        callback(data);
        self.facts.borrow_mut().on_input = Some(callback);
    }

    fn resize(&self, columns: usize, rows: usize) {
        let mut facts = self.facts.borrow_mut();
        facts.columns = columns;
        facts.rows = rows;
        facts
            .trace
            .borrow_mut()
            .push(json!(["terminal", "resize", columns, rows]));
    }
}

impl Terminal for FakeTerminal {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        self.record(json!(["start"]));
        let mut facts = self.facts.borrow_mut();
        facts.on_input = Some(on_input);
        facts.on_resize = Some(on_resize);
    }

    fn stop(&mut self) {
        self.record(json!(["stop"]));
        let mut facts = self.facts.borrow_mut();
        facts.on_input = None;
        facts.on_resize = None;
    }

    fn write(&mut self, data: &str) {
        self.record(json!(["write", data]));
    }

    fn columns(&self) -> usize {
        self.facts.borrow().columns
    }

    fn rows(&self) -> usize {
        self.facts.borrow().rows
    }

    fn kitty_protocol_active(&self) -> bool {
        false
    }

    fn move_by(&mut self, lines: isize) {
        self.record(json!(["move-by", lines]));
    }

    fn hide_cursor(&mut self) {
        self.record(json!(["hide-cursor"]));
    }

    fn show_cursor(&mut self) {
        self.record(json!(["show-cursor"]));
    }

    fn clear_line(&mut self) {
        self.record(json!(["clear-line"]));
    }

    fn clear_from_cursor(&mut self) {
        self.record(json!(["clear-from-cursor"]));
    }

    fn clear_screen(&mut self) {
        self.record(json!(["clear-screen"]));
    }

    fn set_title(&mut self, title: &str) {
        self.record(json!(["set-title", title]));
    }

    fn set_progress(&mut self, active: bool) {
        self.record(json!(["set-progress", active]));
    }
}

struct Probe {
    name: String,
    lines: Rc<RefCell<Vec<String>>>,
    trace: Trace,
    focused: Rc<RefCell<bool>>,
    wants_release: bool,
}

struct DropProbe {
    drops: Rc<Cell<usize>>,
}

impl Component for DropProbe {
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec!["drop".into()]
    }
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct ReentrantProbe {
    screen: Weak<TuiMainScreen>,
    renders: Rc<Cell<usize>>,
}

impl Component for ReentrantProbe {
    fn render(&mut self, _width: usize) -> Vec<String> {
        let renders = self.renders.get() + 1;
        self.renders.set(renders);
        if renders == 1
            && let Some(screen) = self.screen.upgrade()
        {
            screen.request_render(false);
        }
        vec![format!("render-{renders}")]
    }
}

impl Probe {
    fn new(name: &str, lines: &[&str], trace: Trace) -> ProbeHarness {
        let lines = Rc::new(RefCell::new(
            lines.iter().map(|line| (*line).to_owned()).collect(),
        ));
        let focused = Rc::new(RefCell::new(false));
        let handle = ComponentHandle::new(Self {
            name: name.into(),
            lines: lines.clone(),
            trace,
            focused: focused.clone(),
            wants_release: false,
        });
        (handle, lines, focused)
    }
}

impl Component for Probe {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.trace
            .borrow_mut()
            .push(json!([self.name, "render", width]));
        self.lines.borrow().clone()
    }

    fn invalidate(&mut self) {
        self.trace
            .borrow_mut()
            .push(json!([self.name, "invalidate"]));
    }

    fn handle_input(&mut self, data: &str) {
        self.trace
            .borrow_mut()
            .push(json!([self.name, "input", data]));
    }

    fn wants_key_release(&self) -> bool {
        self.wants_release
    }

    fn focused(&self) -> Option<bool> {
        Some(*self.focused.borrow())
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        *self.focused.borrow_mut() = focused;
        self.trace
            .borrow_mut()
            .push(json!([self.name, "focus", focused]));
        true
    }
}

fn phase(terminal: &FakeTerminal, trace: &Trace, clock: &Rc<RefCell<ClockFacts>>) -> Value {
    let terminal_events = std::mem::take(&mut terminal.facts.borrow_mut().events);
    let trace = std::mem::take(&mut *trace.borrow_mut());
    let clock = std::mem::take(&mut clock.borrow_mut().facts);
    json!({ "terminal": terminal_events, "trace": trace, "clock": clock })
}

fn render_state(state: RenderState) -> Value {
    json!({
        "previousLines": state.previous_lines,
        "previousWidth": state.previous_width,
        "previousHeight": state.previous_height,
        "cursorRow": state.cursor_row,
        "hardwareCursorRow": state.hardware_cursor_row,
        "maxLinesRendered": state.max_lines_rendered,
        "previousViewportTop": state.previous_viewport_top,
    })
}

fn set_caps(images: Option<ImageProtocol>, true_color: bool, hyperlinks: bool) {
    set_capabilities(Arc::new(TerminalCapabilities {
        images,
        true_color,
        hyperlinks,
    }));
}

fn capabilities() -> Value {
    let capabilities = get_capabilities();
    json!({
        "images": match capabilities.images {
            Some(ImageProtocol::Kitty) => Some("kitty"),
            Some(ImageProtocol::ITerm2) => Some("iterm2"),
            None => None,
        },
        "trueColor": capabilities.true_color,
        "hyperlinks": capabilities.hyperlinks,
    })
}

fn main_harness(width: usize, height: usize, cursor: bool, images: bool) -> MainHarness {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(width, height, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, images)));
    let controller = TuiMainScreen::new(
        Box::new(terminal.clone()),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        cursor,
    );
    (controller, terminal, clock, trace)
}

fn alt_harness(
    width: usize,
    height: usize,
    cursor: bool,
    images: bool,
    now: u64,
    options: TuiAltScreenOptions,
) -> AltHarness {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(width, height, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(now, images)));
    let controller = TuiAltScreen::new(
        Box::new(terminal.clone()),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        cursor,
        options,
    );
    (controller, terminal, clock, trace)
}

#[test]
fn main_screen_products_equal_all_three_oracle_cases() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (main, terminal, clock, trace) = main_harness(8, 3, true, false);
    let (root, lines, _) = Probe::new(
        "root",
        &["one", &format!("tw{CURSOR_MARKER}o")],
        trace.clone(),
    );
    main.add_child(root.as_component_ref());
    main.set_focus(Some(root.as_component_ref()));
    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));
    let initial = phase(&terminal, &trace, &clock);
    *lines.borrow_mut() = vec!["one".into(), format!("T{CURSOR_MARKER}WO")];
    main.render_now(false);
    let differential = phase(&terminal, &trace, &clock);
    terminal.resize(10, 4);
    main.request_render(false);
    run_current(&clock, |id, task| main.run_task(id, task));
    advance(&clock, 16, |id, task| main.run_task(id, task));
    let resize = phase(&terminal, &trace, &clock);
    main.stop(TuiStopOptions::default());
    let actual = json!({
        "name": "main-lifecycle-diff-resize-cursor-stop",
        "initial": initial,
        "differential": differential,
        "resize": resize,
        "stop": phase(&terminal, &trace, &clock),
        "mode": "regular",
        "fullRedraws": main.full_redraws(),
    });
    assert_eq!(
        actual,
        fixture_case("main-lifecycle-diff-resize-cursor-stop")
    );

    set_caps(None, false, false);
    let (main, terminal, clock, trace) = main_harness(5, 3, false, false);
    let (long, lines, _) = Probe::new("long", &["0", "1", "2", "3", "4"], trace.clone());
    main.add_child(long.as_component_ref());
    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));
    let mut initial = phase(&terminal, &trace, &clock);
    initial["state"] = render_state(main.capture_render_state());
    lines.borrow_mut()[4] = "X".into();
    main.render_now(false);
    let mut visible_change = phase(&terminal, &trace, &clock);
    visible_change["state"] = render_state(main.capture_render_state());
    lines.borrow_mut().extend(["5".into(), "6".into()]);
    main.render_now(false);
    let mut append = phase(&terminal, &trace, &clock);
    append["state"] = render_state(main.capture_render_state());
    lines.borrow_mut()[0] = "Z".into();
    main.render_now(false);
    let mut above_viewport = phase(&terminal, &trace, &clock);
    above_viewport["state"] = render_state(main.capture_render_state());
    main.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "main-long-document-viewport-and-preserved-stop",
        "initial": initial,
        "visibleChange": visible_change,
        "append": append,
        "aboveViewport": above_viewport,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("main-long-document-viewport-and-preserved-stop")
    );

    set_caps(Some(ImageProtocol::Kitty), true, true);
    let (main, terminal, clock, trace) = main_harness(8, 4, false, true);
    let image = "\x1b_Ga=T,f=100,q=2,c=2,r=2,i=7;QUJDRA==\x1b\\";
    let (component, lines, _) = Probe::new("image", &[image, "", "tail"], trace.clone());
    main.add_child(component.as_component_ref());
    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));
    let captured = main.capture_render_state();
    let initial = phase(&terminal, &trace, &clock);
    *lines.borrow_mut() = vec!["gone".into(), "tail".into()];
    main.render_now(false);
    let removal = phase(&terminal, &trace, &clock);
    let (restored, _, _, _) = main_harness(8, 4, false, true);
    restored.restore_render_state(captured.clone());
    main.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "main-kitty-ownership-and-render-state-restore",
        "image": { "columns": 2, "rows": 2, "imageId": 7 },
        "initial": initial,
        "removal": removal,
        "captured": render_state(captured),
        "restored": render_state(restored.capture_render_state()),
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("main-kitty-ownership-and-render-state-restore")
    );
}

#[test]
fn alt_screen_lifecycle_product_equals_oracle() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (alt, terminal, clock, trace) = alt_harness(
        6,
        3,
        true,
        false,
        100,
        TuiAltScreenOptions {
            mouse: true,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let (doc, lines, _) = Probe::new(
        "doc",
        &["a", "b", "c", "d", &format!("e{CURSOR_MARKER}")],
        trace.clone(),
    );
    alt.add_child(doc.as_component_ref());
    alt.set_focus(Some(doc.as_component_ref()));
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    let mut initial = phase(&terminal, &trace, &clock);
    initial["viewportTop"] = json!(alt.viewport_top());
    initial["following"] = json!(alt.is_following_output());

    lines.borrow_mut()[4] = format!("E{CURSOR_MARKER}");
    alt.render_now(false);
    let differential = phase(&terminal, &trace, &clock);

    terminal.resize(7, 4);
    alt.request_render(false);
    run_current(&clock, |id, task| alt.run_task(id, task));
    advance(&clock, 16, |id, task| alt.run_task(id, task));
    let mut resize = phase(&terminal, &trace, &clock);
    resize["viewportTop"] = json!(alt.viewport_top());
    resize["following"] = json!(alt.is_following_output());

    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-lifecycle-diff-resize-preserved-stop",
        "initial": initial,
        "differential": differential,
        "resize": resize,
        "stop": phase(&terminal, &trace, &clock),
        "mode": "fullscreen",
        "fullRedraws": alt.full_redraws(),
    });
    assert_eq!(
        actual,
        fixture_case("alt-lifecycle-diff-resize-preserved-stop")
    );
}

#[test]
fn alt_layout_focus_overlay_and_main_screen_restore_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (alt, terminal, clock, trace) = alt_harness(
        8,
        3,
        false,
        false,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let (child, _, child_focused) = Probe::new("child", &["child"], trace.clone());
    let (layout, _, _) = Probe::new("layout", &["layout-0", "layout-1"], trace.clone());
    let (overlay, _, overlay_focused) = Probe::new("overlay", &["OV"], trace.clone());
    alt.add_child(child.as_component_ref());
    alt.set_focus(Some(child.as_component_ref()));
    alt.set_layout_root(Some(layout.as_component_ref()));
    let visible_trace = trace.clone();
    let handle = alt.show_overlay(
        overlay.as_component_ref(),
        OverlayOptions {
            anchor: OverlayAnchor::TopLeft,
            width: Some(SizeValue::Absolute(3.0)),
            visible: Some(Rc::new(move |width, height| {
                visible_trace
                    .borrow_mut()
                    .push(json!(["overlay-visible", width, height]));
                true
            })),
            ..OverlayOptions::default()
        },
    );
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    let mut with_layout = phase(&terminal, &trace, &clock);
    with_layout["childFocused"] = json!(*child_focused.borrow());
    with_layout["overlayFocused"] = json!(*overlay_focused.borrow());
    with_layout["handleFocused"] = json!(handle.is_focused());

    alt.set_layout_root(Some(layout.as_component_ref()));
    run_current(&clock, |id, task| alt.run_task(id, task));
    let identity_noop = phase(&terminal, &trace, &clock);

    alt.invalidate();
    let invalidate = phase(&terminal, &trace, &clock);

    handle.hide();
    alt.set_layout_root(None);
    alt.render_now(false);
    let mut restored_children = phase(&terminal, &trace, &clock);
    restored_children["childFocused"] = json!(*child_focused.borrow());
    restored_children["overlayFocused"] = json!(*overlay_focused.borrow());

    alt.stop(TuiStopOptions::default());
    let actual = json!({
        "name": "alt-layout-root-focus-overlay-and-main-screen-restore",
        "withLayoutAndOverlay": with_layout,
        "identityNoop": identity_noop,
        "invalidate": invalidate,
        "restoredChildren": restored_children,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-layout-root-focus-overlay-and-main-screen-restore")
    );
}

fn feed_alt(alt: &TuiAltScreen, terminal: &FakeTerminal, data: &str) {
    terminal.input(data);
    alt.handle_terminal_input(data);
}

fn capture_scroll_state(
    name: &str,
    alt: &TuiAltScreen,
    terminal: &FakeTerminal,
    trace: &Trace,
    clock: &Rc<RefCell<ClockFacts>>,
) -> Value {
    json!({
        "name": name,
        "viewportTop": alt.viewport_top(),
        "following": alt.is_following_output(),
        "phase": phase(terminal, trace, clock),
    })
}

#[test]
fn alt_scroll_mouse_release_and_live_keybindings_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let (alt, terminal, clock, trace) = alt_harness(
        10,
        3,
        false,
        false,
        100,
        TuiAltScreenOptions {
            wheel_scroll_lines: 2,
            mouse: true,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let (scroll, _, _) = Probe::new(
        "scroll",
        &["zero", "one", "two", "three", "four", "five", "six"],
        trace.clone(),
    );
    alt.add_child(scroll.as_component_ref());
    alt.set_focus(Some(scroll.as_component_ref()));
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    phase(&terminal, &trace, &clock);

    let mut states = Vec::new();
    feed_alt(&alt, &terminal, "\x1b[<64;1;1M");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "wheel-up", &alt, &terminal, &trace, &clock,
    ));

    feed_alt(&alt, &terminal, "\x1b[5;1:3~");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "page-up-release",
        &alt,
        &terminal,
        &trace,
        &clock,
    ));

    feed_alt(&alt, &terminal, "\x1b[5~");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "page-up-press",
        &alt,
        &terminal,
        &trace,
        &clock,
    ));

    set_keybindings(KeybindingsManager::with_tui_defaults(vec![(
        "tui.altScreen.pageUp".into(),
        vec!["f1".into()],
    )]));
    feed_alt(&alt, &terminal, "\x1b[5~");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "old-binding-after-replacement",
        &alt,
        &terminal,
        &trace,
        &clock,
    ));
    feed_alt(&alt, &terminal, "\x1bOP");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "new-binding-after-replacement",
        &alt,
        &terminal,
        &trace,
        &clock,
    ));
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));

    feed_alt(&alt, &terminal, "\x1b[F");
    alt.render_now(false);
    states.push(capture_scroll_state(
        "bottom", &alt, &terminal, &trace, &clock,
    ));
    feed_alt(&alt, &terminal, "\x1b[H");
    alt.render_now(false);
    states.push(capture_scroll_state("top", &alt, &terminal, &trace, &clock));

    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-scroll-mouse-release-filter-and-live-keybindings",
        "states": states,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-scroll-mouse-release-filter-and-live-keybindings")
    );
}

#[test]
fn alt_selection_clipboard_granularity_and_focus_out_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let (alt, terminal, clock, trace) = alt_harness(
        12,
        3,
        false,
        false,
        1_000,
        TuiAltScreenOptions {
            mouse: true,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let (select, _, _) = Probe::new("select", &["alpha beta", "second", "third"], trace.clone());
    alt.add_child(select.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    phase(&terminal, &trace, &clock);

    let mut clicks = Vec::new();
    for name in ["single", "double-word", "triple-line"] {
        feed_alt(&alt, &terminal, "\x1b[<0;2;1M");
        feed_alt(&alt, &terminal, "\x1b[<0;2;1m");
        alt.render_now(false);
        clicks.push(json!({
            "name": name,
            "phase": phase(&terminal, &trace, &clock),
        }));
        clock.borrow_mut().now += 10;
    }

    feed_alt(&alt, &terminal, "\x1b[<0;1;2M");
    feed_alt(&alt, &terminal, "\x1b[<32;4;2M");
    feed_alt(&alt, &terminal, "\x1b[O");
    feed_alt(&alt, &terminal, "\x1b[<0;4;2m");
    alt.render_now(false);
    let focus_out_cancellation = phase(&terminal, &trace, &clock);

    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-selection-clipboard-granularity-and-focus-out",
        "clicks": clicks,
        "focusOutCancellation": focus_out_cancellation,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-selection-clipboard-granularity-and-focus-out")
    );
}

#[test]
fn alt_raw_unregistered_kitty_lines_are_not_owned_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(Some(ImageProtocol::Kitty), true, true);
    let (alt, terminal, clock, trace) = alt_harness(
        8,
        3,
        false,
        true,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let raw_image =
        |image_id: u32| format!("\x1b_Ga=T,f=100,q=2,c=1,r=1,i={image_id};QUFBQQ==\x1b\\");
    let first = raw_image(1);
    let (raw_kitty, lines, _) = Probe::new("raw-kitty", &[&first], trace.clone());
    alt.add_child(raw_kitty.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    phase(&terminal, &trace, &clock);

    let mut render_writes = Vec::new();
    for image_id in 2..=18 {
        lines.borrow_mut()[0] = raw_image(image_id);
        alt.render_now(false);
        let rendered = phase(&terminal, &trace, &clock);
        render_writes.extend(
            rendered["terminal"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event[0] == "write")
                .map(|event| event[1].clone()),
        );
    }
    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-raw-unregistered-kitty-lines-are-not-owned",
        "renderWrites": render_writes,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-raw-unregistered-kitty-lines-are-not-owned")
    );
}

#[test]
fn alt_kitty_transmission_placement_and_teardown_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(Some(ImageProtocol::Kitty), true, true);
    let (alt, terminal, clock, trace) = alt_harness(
        8,
        3,
        false,
        true,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    alt.register_kitty_image_metadata(KittyImageMetadata {
        image_id: 9,
        columns: 1,
        rows: 1,
        width_px: 9,
        height_px: 18,
    });
    let first_image = "\x1b_Ga=T,f=100,q=2,c=1,r=1,i=9;QUFBQQ==\x1b\\";
    let (kitty, lines, _) = Probe::new("kitty", &[first_image, "tail"], trace.clone());
    alt.add_child(kitty.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    let first = phase(&terminal, &trace, &clock);

    lines.borrow_mut()[1] = "TAIL".into();
    alt.render_now(false);
    let text_only_change = phase(&terminal, &trace, &clock);

    alt.register_kitty_image_metadata(KittyImageMetadata {
        image_id: 9,
        columns: 1,
        rows: 1,
        width_px: 9,
        height_px: 18,
    });
    lines.borrow_mut()[0] = "\x1b_Ga=T,f=100,q=2,c=1,r=1,i=9;QkJCQg==\x1b\\".into();
    alt.render_now(false);
    let retransmit = phase(&terminal, &trace, &clock);

    lines.borrow_mut()[0] = "plain".into();
    alt.render_now(false);
    let removal = phase(&terminal, &trace, &clock);

    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-kitty-transmission-placement-and-teardown-ownership",
        "first": first,
        "textOnlyChange": text_only_change,
        "retransmit": retransmit,
        "removal": removal,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-kitty-transmission-placement-and-teardown-ownership")
    );
}

#[test]
fn alt_kitty_offscreen_cache_eviction_and_revisit_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(Some(ImageProtocol::Kitty), true, true);
    let (alt, terminal, clock, trace) = alt_harness(
        8,
        3,
        false,
        true,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    for image_id in 1..=18 {
        alt.register_kitty_image_metadata(KittyImageMetadata {
            image_id,
            columns: 1,
            rows: 1,
            width_px: 9,
            height_px: 18,
        });
    }
    let image = |image_id: u32| format!("\x1b_Ga=T,f=100,q=2,c=1,r=1,i={image_id};QUFBQQ==\x1b\\");
    let first = image(1);
    let (kitty, lines, _) = Probe::new("kitty-cache", &[&first], trace.clone());
    alt.add_child(kitty.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    phase(&terminal, &trace, &clock);

    let mut at_bound = Value::Null;
    let mut eviction = Value::Null;
    for image_id in 2..=18 {
        lines.borrow_mut()[0] = image(image_id);
        alt.render_now(false);
        let rendered = phase(&terminal, &trace, &clock);
        if image_id == 17 {
            at_bound = rendered;
        } else if image_id == 18 {
            eviction = rendered;
        }
    }

    lines.borrow_mut()[0] = image(1);
    alt.render_now(false);
    let revisit = phase(&terminal, &trace, &clock);
    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-kitty-offscreen-cache-eviction-and-revisit",
        "atBound": at_bound,
        "eviction": eviction,
        "revisit": revisit,
        "stop": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-kitty-offscreen-cache-eviction-and-revisit")
    );
}

#[test]
fn alt_iterm_capability_suspension_and_unpreserved_stop_equal_oracle() {
    let _global_state = global_state_lock();
    set_caps(Some(ImageProtocol::ITerm2), true, true);
    let (alt, terminal, clock, trace) = alt_harness(
        5,
        3,
        false,
        true,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let image = "\x1b]1337;File=inline=1;size=4;width=1;height=auto:QUJDRA==\x07";
    let (iterm, _, _) = Probe::new(
        "iterm",
        &[image, "\x1b]133;A\x07abcdef", &format!("x{CURSOR_MARKER}")],
        trace.clone(),
    );
    alt.add_child(iterm.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    let mut during = phase(&terminal, &trace, &clock);
    during["capabilities"] = capabilities();

    alt.stop(TuiStopOptions::default());
    let actual = json!({
        "name": "alt-iterm-capability-suspension-and-unpreserved-stop",
        "image": { "columns": 1, "rows": 1 },
        "during": during,
        "stop": phase(&terminal, &trace, &clock),
        "restoredCapabilities": capabilities(),
    });
    assert_eq!(
        actual,
        fixture_case("alt-iterm-capability-suspension-and-unpreserved-stop")
    );
}

#[test]
fn alt_multiplexer_button_motion_lifecycle_equals_oracle() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (alt, terminal, clock, trace) = alt_harness(
        4,
        2,
        false,
        false,
        100,
        TuiAltScreenOptions {
            mouse: true,
            environment: TuiAltScreenEnvironment {
                multiplexer: true,
                is_windows: false,
            },
            ..TuiAltScreenOptions::default()
        },
    );
    let (mux, _, _) = Probe::new("mux", &["x"], trace.clone());
    alt.add_child(mux.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let actual = json!({
        "name": "alt-multiplexer-button-motion-lifecycle",
        "lifecycle": phase(&terminal, &trace, &clock),
    });
    assert_eq!(
        actual,
        fixture_case("alt-multiplexer-button-motion-lifecycle")
    );
}

#[test]
fn terminal_callback_drain_routes_each_input_once() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (main, terminal, clock, trace) = main_harness(8, 3, false, false);
    let (input, _, _) = Probe::new("input", &["ready"], trace.clone());
    main.add_child(input.as_component_ref());
    main.set_focus(Some(input.as_component_ref()));
    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));
    phase(&terminal, &trace, &clock);

    terminal.feed_input_callback("a");
    terminal.feed_input_callback("b");
    assert_eq!(main.terminal_events_pending(), 2);
    assert_eq!(
        clock
            .borrow()
            .scheduled
            .values()
            .filter(|scheduled| scheduled.task == TuiHostTask::DrainTerminalEvents)
            .count(),
        1,
        "multiple terminal callbacks must share one drain task"
    );

    run_current(&clock, |id, task| main.run_task(id, task));
    assert_eq!(main.terminal_events_pending(), 0);
    let input_events = trace
        .borrow()
        .iter()
        .filter(|event| {
            event.get(0) == Some(&json!("input")) && event.get(1) == Some(&json!("input"))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        input_events,
        vec![
            json!(["input", "input", "a"]),
            json!(["input", "input", "b"])
        ]
    );
}

#[test]
fn reentrant_component_render_defers_without_losing_follow_up() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(8, 3, trace);
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, false)));
    let main = Rc::new(TuiMainScreen::new(
        Box::new(terminal),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        false,
    ));
    let renders = Rc::new(Cell::new(0));
    let reentrant = ComponentHandle::new(ReentrantProbe {
        screen: Rc::downgrade(&main),
        renders: renders.clone(),
    });
    main.add_child(reentrant.as_component_ref());
    drop(reentrant);

    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));
    assert_eq!(renders.get(), 1);
    assert!(
        clock.borrow().scheduled.values().any(|scheduled| {
            scheduled.task == TuiHostTask::RenderTimer && scheduled.due == 116
        })
    );
    advance(&clock, 16, |id, task| main.run_task(id, task));
    assert_eq!(renders.get(), 2);
}

#[test]
fn alt_flash_tasks_require_matching_route_identity() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let (alt, terminal, clock, trace) = alt_harness(
        10,
        3,
        false,
        false,
        100,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    );
    let (root, _, _) = Probe::new("root", &["base"], trace.clone());
    alt.add_child(root.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    phase(&terminal, &trace, &clock);

    alt.flash("notice", Some(50));
    advance(&clock, 16, |id, task| alt.run_task(id, task));
    let shown = phase(&terminal, &trace, &clock);
    assert!(shown.to_string().contains("notice"));
    let (task_id, task, flash_id) = clock
        .borrow()
        .scheduled
        .iter()
        .find_map(|(id, scheduled)| match scheduled.task {
            TuiHostTask::AltFlashTimeout { flash_id } => Some((*id, scheduled.task, flash_id)),
            _ => None,
        })
        .expect("flash timer must remain scheduled");

    alt.run_task(TuiTaskId(task_id.0 + 1_000), task);
    alt.run_task(
        task_id,
        TuiHostTask::AltFlashTimeout {
            flash_id: flash_id + 1,
        },
    );
    alt.render_now(true);
    let rejected = phase(&terminal, &trace, &clock);
    assert!(rejected.to_string().contains("notice"));

    clock.borrow_mut().scheduled.remove(&task_id);
    alt.run_task(task_id, task);
    alt.render_now(true);
    let expired = phase(&terminal, &trace, &clock);
    assert!(!expired.to_string().contains("notice"));
}

#[test]
fn second_flash_reentrant_stop_is_borrow_free_and_tears_down_exactly() {
    let _global_state = global_state_lock();
    set_caps(Some(ImageProtocol::ITerm2), true, true);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(10, 3, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, true)));
    let target = Rc::new(RefCell::new(None));
    let alt = Rc::new(TuiAltScreen::new(
        Box::new(terminal.clone()),
        Box::new(ReentrantFlashRuntime {
            facts: clock.clone(),
            trace: trace.clone(),
            target: target.clone(),
            flash_schedules: 0,
        }),
        false,
        TuiAltScreenOptions {
            mouse: true,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    ));
    *target.borrow_mut() = Some(Rc::downgrade(&alt));
    let (root, _, _) = Probe::new("root", &["base"], trace.clone());
    alt.add_child(root.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));
    assert_eq!(
        capabilities(),
        json!({ "images": null, "trueColor": true, "hyperlinks": true }),
        "iTerm images must be suspended while the alternate screen is active"
    );
    terminal.facts.borrow_mut().events.clear();
    trace.borrow_mut().clear();

    alt.flash("first", Some(1_000));
    advance(&clock, 16, |id, task| alt.run_task(id, task));
    terminal.facts.borrow_mut().events.clear();

    alt.flash("second", Some(1_000));

    let relevant = trace
        .borrow()
        .iter()
        .filter(|event| {
            if event.get(0) == Some(&json!("runtime")) {
                return true;
            }
            if event.get(0) != Some(&json!("terminal")) {
                return false;
            }
            match event.get(1).and_then(Value::as_str) {
                Some("show-cursor" | "stop") => true,
                Some("write") => event.get(2).and_then(Value::as_str).is_some_and(|write| {
                    write.contains("\x1b[?1006l") || write.contains("\x1b[?1049l")
                }),
                _ => false,
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    let first_task = relevant[0][3].as_u64().unwrap();
    let second_task = relevant[1][3].as_u64().unwrap();
    assert_eq!(
        relevant,
        vec![
            json!(["runtime", "schedule-flash", 0, first_task]),
            json!(["runtime", "schedule-flash", 1, second_task]),
            json!(["runtime", "reentrant-stop", 1]),
            json!(["runtime", "cancel-flash", 0, first_task]),
            json!(["runtime", "cancel-flash", 1, second_task]),
            json!([
                "terminal",
                "write",
                "\x1b[?2026h\x1b[?1006l\x1b[?1004l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?7h\x1b[?2026l"
            ]),
            json!(["terminal", "show-cursor"]),
            json!(["terminal", "stop"]),
            json!([
                "terminal",
                "write",
                "\x1b[?2026h\x1b[?1049l\x1b[?25h\x1b[?2026l"
            ]),
        ],
        "both flash tasks must reconcile before the complete terminal teardown"
    );
    assert!(
        clock
            .borrow()
            .scheduled
            .values()
            .all(|scheduled| !matches!(scheduled.task, TuiHostTask::AltFlashTimeout { .. })),
        "no flash task may survive reentrant stop"
    );
    assert_eq!(
        capabilities(),
        json!({ "images": "iterm2", "trueColor": true, "hyperlinks": true }),
        "the suspended process-wide capability set must be restored"
    );
    let terminal = terminal.facts.borrow();
    assert!(terminal.on_input.is_none() && terminal.on_resize.is_none());
}

#[test]
fn terminal_write_reentrant_stop_is_queued_until_alt_render_releases_the_terminal() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(10, 3, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, false)));
    let callback = Rc::new(RefCell::new(None));
    let alt = Rc::new(TuiAltScreen::new(
        Box::new(ReentrantWriteTerminal {
            inner: terminal.clone(),
            trigger: "terminal-r",
            callback: callback.clone(),
        }),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        false,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    ));
    let weak_alt = Rc::downgrade(&alt);
    *callback.borrow_mut() = Some(Box::new(move || {
        if let Some(alt) = weak_alt.upgrade() {
            alt.stop(TuiStopOptions {
                preserve_screen: true,
            });
        }
    }));
    let (root, _, _) = Probe::new("root", &["terminal-reentrant"], trace.clone());
    alt.add_child(root.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));

    let events = &terminal.facts.borrow().events;
    assert_eq!(
        &events[events.len() - 4..],
        [
            json!(["write", "\x1b[?2026h\x1b[?7h\x1b[?2026l"]),
            json!(["show-cursor"]),
            json!(["stop"]),
            json!(["write", "\x1b[?2026h\x1b[?1049l\x1b[?25h\x1b[?2026l"]),
        ]
    );
    let terminal = terminal.facts.borrow();
    assert!(terminal.on_input.is_none() && terminal.on_resize.is_none());
}

#[test]
fn terminal_write_reentrant_alt_geometry_reads_use_last_safe_snapshot() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(10, 3, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, false)));
    let callback = Rc::new(RefCell::new(None));
    let observed = Rc::new(RefCell::new(None));
    let alt = Rc::new(TuiAltScreen::new(
        Box::new(ReentrantWriteTerminal {
            inner: terminal,
            trigger: "\x1b[?2026h",
            callback: callback.clone(),
        }),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        false,
        TuiAltScreenOptions {
            mouse: false,
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    ));
    let weak_alt = Rc::downgrade(&alt);
    let callback_observed = observed.clone();
    *callback.borrow_mut() = Some(Box::new(move || {
        let alt = weak_alt.upgrade().expect("alternate screen remains alive");
        *callback_observed.borrow_mut() = Some((alt.terminal_columns(), alt.terminal_rows()));
    }));
    let (root, _, _) = Probe::new("root", &["alt-geometry"], trace);
    alt.add_child(root.as_component_ref());
    alt.start();
    run_current(&clock, |id, task| alt.run_task(id, task));

    assert_eq!(*observed.borrow(), Some((10, 3)));
    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
}

#[test]
fn terminal_write_reentrant_main_capture_reads_last_committed_snapshot() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let terminal = FakeTerminal::new(10, 3, trace.clone());
    let clock = Rc::new(RefCell::new(ClockFacts::new(100, false)));
    let callback = Rc::new(RefCell::new(None));
    let observed = Rc::new(RefCell::new(None));
    let main = Rc::new(TuiMainScreen::new(
        Box::new(ReentrantWriteTerminal {
            inner: terminal,
            trigger: "\x1b[?2026h",
            callback: callback.clone(),
        }),
        Box::new(FakeRuntime {
            facts: clock.clone(),
        }),
        false,
    ));
    let weak_main = Rc::downgrade(&main);
    let callback_observed = observed.clone();
    *callback.borrow_mut() = Some(Box::new(move || {
        let main = weak_main.upgrade().expect("main screen remains alive");
        *callback_observed.borrow_mut() = Some(main.capture_render_state());
    }));
    let (root, _, _) = Probe::new("root", &["main"], trace);
    main.add_child(root.as_component_ref());
    main.start();
    run_current(&clock, |id, task| main.run_task(id, task));

    assert_eq!(
        *observed.borrow(),
        Some(RenderState {
            previous_lines: Vec::new(),
            previous_width: 0,
            previous_height: 0,
            cursor_row: 0,
            hardware_cursor_row: 0,
            max_lines_rendered: 0,
            previous_viewport_top: 0,
        }),
        "capture during Terminal::write must observe the last committed state"
    );
    assert_eq!(
        main.capture_render_state().previous_lines,
        vec!["main\x1b[0m\x1b]8;;\x07"],
        "the planned render commits after the terminal callback returns"
    );
    main.stop(TuiStopOptions {
        preserve_screen: true,
    });
}

#[test]
fn dropping_active_screens_breaks_weak_cycles_and_cancels_tasks() {
    let _global_state = global_state_lock();
    set_caps(None, false, false);

    let main_drops = Rc::new(Cell::new(0));
    let (terminal, clock) = {
        let (main, terminal, clock, _) = main_harness(8, 3, false, false);
        let component = ComponentHandle::new(DropProbe {
            drops: main_drops.clone(),
        });
        main.add_child(component.as_component_ref());
        drop(component);
        main.start();
        run_current(&clock, |id, task| main.run_task(id, task));
        main.request_render(false);
        assert!(!clock.borrow().scheduled.is_empty());
        (terminal, clock)
    };
    assert_eq!(main_drops.get(), 1);
    assert!(clock.borrow().scheduled.is_empty());
    let facts = terminal.facts.borrow();
    assert!(facts.on_input.is_none() && facts.on_resize.is_none());
    assert!(facts.events.iter().any(|event| event == &json!(["stop"])));
    drop(facts);

    let alt_drops = Rc::new(Cell::new(0));
    let (terminal, clock) = {
        let (alt, terminal, clock, _) = alt_harness(
            8,
            3,
            false,
            false,
            100,
            TuiAltScreenOptions {
                mouse: false,
                environment: TuiAltScreenEnvironment::default(),
                ..TuiAltScreenOptions::default()
            },
        );
        let component = ComponentHandle::new(DropProbe {
            drops: alt_drops.clone(),
        });
        alt.add_child(component.as_component_ref());
        drop(component);
        alt.start();
        run_current(&clock, |id, task| alt.run_task(id, task));
        alt.flash("pending", Some(1_000));
        assert!(!clock.borrow().scheduled.is_empty());
        (terminal, clock)
    };
    assert_eq!(alt_drops.get(), 1);
    assert!(clock.borrow().scheduled.is_empty());
    let facts = terminal.facts.borrow();
    assert!(facts.on_input.is_none() && facts.on_resize.is_none());
    assert!(facts.events.iter().any(|event| event == &json!(["stop"])));
    assert!(facts.events.iter().any(|event| {
        event
            .get(1)
            .and_then(Value::as_str)
            .is_some_and(|write| write.contains("\x1b[?1049l"))
    }));
}

fn _alt_compile_surface(
    terminal: Box<dyn Terminal>,
    runtime: Box<dyn TuiScreenRuntime>,
) -> TuiAltScreen {
    TuiAltScreen::new(
        terminal,
        runtime,
        false,
        TuiAltScreenOptions {
            environment: TuiAltScreenEnvironment::default(),
            ..TuiAltScreenOptions::default()
        },
    )
}
