use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use pie_app::{
    OverlayHandle, TuiBaseController, TuiControllerHost, TuiHostTask, TuiSubscription, TuiTaskId,
};
use pie_components::{
    Component, ComponentHandle, ContainerChildId, OverlayAnchor, OverlayMargin, OverlayMargins,
    OverlayOptions, OverlayUnfocus, SizeValue, SubscriptionControl, TerminalColorSchemeListener,
    Tui, TuiInputListener, TuiInputListenerResult, TuiMode, TuiStopOptions, ViewportTui,
};
use pie_core::terminal_colors::TerminalColorScheme;
use pie_core::terminal_image::CellDimensions;
use pie_term::{InputHandler, ResizeHandler, Terminal};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/tui-controller.json")).unwrap()
}

fn fixture_case(name: &str) -> serde_json::Value {
    fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap()
        .clone()
}

#[test]
fn oracle_is_exactly_pinned_and_non_vacuous() {
    let fixture = fixture();
    assert_eq!(fixture["reference"]["package"], "@earendil-works/pi-tui");
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(fixture["reference"]["node"], "24.4.1");
    assert_eq!(fixture["reference"]["icu"], "77.1");
    assert_eq!(fixture["reference"]["unicode"], "16.0");
    let digests = fixture["reference"]["sourceDigests"].as_object().unwrap();
    assert_eq!(digests.len(), 32);
    assert_eq!(
        digests["tuiJs"],
        "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a"
    );
    assert_eq!(
        digests["terminalColorsJs"],
        "e26c8c31d161d175817b3335baab4476737719c389a2a39312aa2ece67ccb119"
    );
    assert_eq!(
        digests["terminalImageJs"],
        "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2"
    );
    assert_eq!(
        digests["tuiAltScreenJs"],
        "2886260eca46a0a66cdc9f407777bd17f05200326d495ebf903578c49f298e3a"
    );
    assert_eq!(
        digests["tuiAltScreenDts"],
        "849b7307e671465a5a1dffc77b9381e782db08a6a16a6e3dd8d79b5faac329a6"
    );
    for (name, expected) in [
        (
            "altScreenFlashJs",
            "6ca2016101ca570a94fdaa18bfe8edbc6734243cb5363d21110e809fcd47db12",
        ),
        (
            "altScreenFlashDts",
            "b8e5800da49d8d88ed59d6fee341e64362d2b43e0b806b21d8c00562ad146a86",
        ),
        (
            "scrollViewJs",
            "796fdaa30bfb850df9d3e9647cd7b08c1bf3f3775335ce90a158c177c282f53f",
        ),
        (
            "scrollViewDts",
            "7ee8cf2eced7d5a3cc68f0f054c9813f93b43019bdd72745b56f0c603b13f097",
        ),
        (
            "stackJs",
            "02b4dafebc728f1c0e8d01b5cc330f82eb760c58bc71c5cc9bff6d98bf34dbf3",
        ),
        (
            "stackDts",
            "4a263287262dd550b75213d4234e84e9060ad4536d78d42bcb052e8012a7c212",
        ),
        (
            "keybindingsJs",
            "d27090a36394fc4f59350e7f3234c601082d950e179ba6742d9557aae2a72168",
        ),
        (
            "keybindingsDts",
            "93450b5ff2259c52767d4bc3dffb17d7c9341f866507cf00aba67cddf42b51b0",
        ),
        (
            "layoutJs",
            "fdc6c58b4245e735a0daabdc93201017e77cbbb01d7d440eda6427270556b2af",
        ),
        (
            "layoutDts",
            "cfa0950012579f3912d7f6887a2b24f8618fa5e7eb0df15447ec992cf806a40a",
        ),
        (
            "layoutNodeJs",
            "73c3942b68d52ed29072f1f78184c99d405f9259bb4e24a1b6b0e3688381f7f5",
        ),
        (
            "layoutNodeDts",
            "8a19dd70f320755c3793cbec86902ad037afedd4151f1d174c0175cc281cf77d",
        ),
    ] {
        assert_eq!(digests[name], expected);
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 29);
    let names = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "start-render-coalescing-stop",
            "terminal-callbacks-drive-input-and-resize",
            "listeners-transform-consume-release-debug",
            "input-listener-live-set-mutation",
            "debug-callback-replacement",
            "recursive-input-listener-dispatch",
            "recursive-debug-dispatch",
            "reentrant-focus-setter-request-render",
            "reentrant-visible-predicate-request-render",
            "reentrant-invalidate-request-render",
            "invalidate-live-root-deletion",
            "invalidate-live-root-insertion",
            "invalidate-root-clear-rebinds-array",
            "invalidate-live-overlay-deletion",
            "invalidate-live-overlay-insertion",
            "cell-size-priority-and-invalidation",
            "osc11-fifo-timeout-and-parse",
            "color-scheme-listeners-query-notifications",
            "scheme-listener-live-set-and-query-order",
            "recursive-scheme-listener-dispatch",
            "overlay-focus-stack-ownership",
            "has-overlay-short-circuits-later-visible",
            "topmost-skips-noncapturing-before-visible",
            "show-noncapturing-does-not-evaluate-visible",
            "live-reentrant-visibility-mutation-ordering",
            "alt-layout-root-identity-cache-and-mounted-roots",
            "overlay-layout-and-composition",
            "no-image-cell-query-and-repeated-stop",
            "reentrant-render-schedules-follow-up-frame",
        ]
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostEvent {
    Schedule(TuiTaskId, u64, TuiHostTask),
    Cancel(TuiTaskId),
    Run(TuiTaskId, TuiHostTask),
    Render(u64),
    Reset(u64),
    Cell(u64, u64),
    BeforeStart,
    AfterStart,
    BeforeStop(bool),
    AfterStop(bool),
    Dropped,
    Reenter(HostCallbackPoint),
}

#[derive(Clone, Copy)]
struct Scheduled {
    due: u64,
    task: TuiHostTask,
}

struct HostFacts {
    now: u64,
    next_task: u64,
    images_supported: bool,
    scheduled: BTreeMap<TuiTaskId, Scheduled>,
    events: Vec<HostEvent>,
    cell_dimensions: CellDimensions,
}

type RenderHook = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

impl HostFacts {
    fn new(now: u64, images_supported: bool) -> Self {
        Self {
            now,
            next_task: 0,
            images_supported,
            scheduled: BTreeMap::new(),
            events: Vec::new(),
            cell_dimensions: CellDimensions::default(),
        }
    }
}

struct FakeHost {
    facts: Rc<RefCell<HostFacts>>,
    render_hook: Option<RenderHook>,
}

impl TuiControllerHost for FakeHost {
    fn now_ms(&self) -> u64 {
        self.facts.borrow().now
    }

    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId {
        let mut facts = self.facts.borrow_mut();
        facts.next_task += 1;
        let id = TuiTaskId(facts.next_task);
        let due = facts.now.saturating_add(delay_ms);
        facts.scheduled.insert(id, Scheduled { due, task });
        facts.events.push(HostEvent::Schedule(id, delay_ms, task));
        id
    }

    fn cancel_task(&mut self, task: TuiTaskId) {
        let mut facts = self.facts.borrow_mut();
        facts.scheduled.remove(&task);
        facts.events.push(HostEvent::Cancel(task));
    }

    fn render(&mut self) {
        {
            let mut facts = self.facts.borrow_mut();
            let now = facts.now;
            facts.events.push(HostEvent::Render(now));
        }
        if let Some(hook) = &self.render_hook
            && let Some(callback) = hook.borrow_mut().as_mut()
        {
            callback();
        }
    }

    fn reset_render_state(&mut self) {
        let mut facts = self.facts.borrow_mut();
        let now = facts.now;
        facts.events.push(HostEvent::Reset(now));
    }

    fn images_supported(&self) -> bool {
        self.facts.borrow().images_supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        let mut facts = self.facts.borrow_mut();
        facts.cell_dimensions = dimensions;
        facts.events.push(HostEvent::Cell(
            dimensions.width_px as u64,
            dimensions.height_px as u64,
        ));
    }

    fn before_terminal_start(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::BeforeStart);
    }

    fn after_terminal_start(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::AfterStart);
    }

    fn before_terminal_stop(&mut self, options: TuiStopOptions) {
        self.facts
            .borrow_mut()
            .events
            .push(HostEvent::BeforeStop(options.preserve_screen));
    }

    fn after_terminal_stop(&mut self, options: TuiStopOptions) {
        self.facts
            .borrow_mut()
            .events
            .push(HostEvent::AfterStop(options.preserve_screen));
    }

    fn controller_dropped(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::Dropped);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostCallbackPoint {
    Now,
    Schedule,
    Cancel,
    Render,
    Reset,
    ImagesSupported,
    CellDimensions,
    BeforeStart,
    AfterStart,
    BeforeStop,
    AfterStop,
    Dropped,
}

struct ReentrantHost {
    facts: Rc<RefCell<HostFacts>>,
    target: HostCallbackPoint,
    controller: Rc<Cell<*const TuiBaseController>>,
    fired: Rc<Cell<bool>>,
    reads: Rc<RefCell<Vec<(TuiMode, usize)>>>,
}

impl ReentrantHost {
    fn reenter(&self, point: HostCallbackPoint) {
        if point != self.target || self.fired.replace(true) {
            return;
        }
        let controller = self.controller.get();
        assert!(!controller.is_null());
        // SAFETY: the test stores a pointer to a boxed controller. Host callbacks
        // are synchronous and run while that box is alive, including its Drop body.
        let controller = unsafe { &*controller };
        self.reads
            .borrow_mut()
            .push((controller.mode(), controller.terminal_columns()));
        self.facts
            .borrow_mut()
            .events
            .push(HostEvent::Reenter(point));
        controller.request_render(true);
    }
}

impl TuiControllerHost for ReentrantHost {
    fn now_ms(&self) -> u64 {
        let now = self.facts.borrow().now;
        self.reenter(HostCallbackPoint::Now);
        now
    }

    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId {
        let id = {
            let mut facts = self.facts.borrow_mut();
            facts.next_task += 1;
            let id = TuiTaskId(facts.next_task);
            let due = facts.now.saturating_add(delay_ms);
            facts.scheduled.insert(id, Scheduled { due, task });
            facts.events.push(HostEvent::Schedule(id, delay_ms, task));
            id
        };
        self.reenter(HostCallbackPoint::Schedule);
        id
    }

    fn cancel_task(&mut self, task: TuiTaskId) {
        {
            let mut facts = self.facts.borrow_mut();
            facts.scheduled.remove(&task);
            facts.events.push(HostEvent::Cancel(task));
        }
        self.reenter(HostCallbackPoint::Cancel);
    }

    fn render(&mut self) {
        {
            let mut facts = self.facts.borrow_mut();
            let now = facts.now;
            facts.events.push(HostEvent::Render(now));
        }
        self.reenter(HostCallbackPoint::Render);
    }

    fn reset_render_state(&mut self) {
        {
            let mut facts = self.facts.borrow_mut();
            let now = facts.now;
            facts.events.push(HostEvent::Reset(now));
        }
        self.reenter(HostCallbackPoint::Reset);
    }

    fn images_supported(&self) -> bool {
        let supported = self.facts.borrow().images_supported;
        self.reenter(HostCallbackPoint::ImagesSupported);
        supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        {
            let mut facts = self.facts.borrow_mut();
            facts.cell_dimensions = dimensions;
            facts.events.push(HostEvent::Cell(
                dimensions.width_px as u64,
                dimensions.height_px as u64,
            ));
        }
        self.reenter(HostCallbackPoint::CellDimensions);
    }

    fn before_terminal_start(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::BeforeStart);
        self.reenter(HostCallbackPoint::BeforeStart);
    }

    fn after_terminal_start(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::AfterStart);
        self.reenter(HostCallbackPoint::AfterStart);
    }

    fn before_terminal_stop(&mut self, options: TuiStopOptions) {
        self.facts
            .borrow_mut()
            .events
            .push(HostEvent::BeforeStop(options.preserve_screen));
        self.reenter(HostCallbackPoint::BeforeStop);
    }

    fn after_terminal_stop(&mut self, options: TuiStopOptions) {
        self.facts
            .borrow_mut()
            .events
            .push(HostEvent::AfterStop(options.preserve_screen));
        self.reenter(HostCallbackPoint::AfterStop);
    }

    fn controller_dropped(&mut self) {
        self.facts.borrow_mut().events.push(HostEvent::Dropped);
        self.reenter(HostCallbackPoint::Dropped);
    }
}

fn run_due(controller: &TuiBaseController, facts: &Rc<RefCell<HostFacts>>) {
    loop {
        let next = {
            let facts = facts.borrow();
            facts
                .scheduled
                .iter()
                .filter(|(_, scheduled)| scheduled.due <= facts.now)
                .map(|(id, scheduled)| (*id, *scheduled))
                .min_by_key(|(id, scheduled)| (scheduled.due, *id))
        };
        let Some((id, scheduled)) = next else {
            break;
        };
        {
            let mut facts = facts.borrow_mut();
            facts.scheduled.remove(&id);
            facts.events.push(HostEvent::Run(id, scheduled.task));
        }
        controller.run_task(id, scheduled.task);
    }
}

fn advance_to(controller: &TuiBaseController, facts: &Rc<RefCell<HostFacts>>, now: u64) {
    facts.borrow_mut().now = now;
    run_due(controller, facts);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalEvent {
    Start,
    Stop,
    Write(String),
    HideCursor,
    ShowCursor,
}

struct TerminalFacts {
    columns: usize,
    rows: usize,
    events: Vec<TerminalEvent>,
    input: Option<InputHandler>,
    resize: Option<ResizeHandler>,
}

#[derive(Clone)]
struct TerminalHandle {
    facts: Rc<RefCell<TerminalFacts>>,
}

impl TerminalHandle {
    fn events(&self) -> Vec<TerminalEvent> {
        self.facts.borrow().events.clone()
    }

    fn feed(&self, data: &str) {
        let callback = self.facts.borrow_mut().input.take();
        if let Some(mut callback) = callback {
            callback(data);
            self.facts.borrow_mut().input = Some(callback);
        }
    }

    fn resize(&self, columns: usize, rows: usize) {
        let callback = {
            let mut facts = self.facts.borrow_mut();
            facts.columns = columns;
            facts.rows = rows;
            facts.resize.take()
        };
        if let Some(mut callback) = callback {
            callback();
            self.facts.borrow_mut().resize = Some(callback);
        }
    }
}

struct FakeTerminal {
    facts: Rc<RefCell<TerminalFacts>>,
}

impl Terminal for FakeTerminal {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        let mut facts = self.facts.borrow_mut();
        facts.events.push(TerminalEvent::Start);
        facts.input = Some(on_input);
        facts.resize = Some(on_resize);
    }

    fn stop(&mut self) {
        let mut facts = self.facts.borrow_mut();
        facts.events.push(TerminalEvent::Stop);
        facts.input = None;
        facts.resize = None;
    }

    fn write(&mut self, data: &str) {
        self.facts
            .borrow_mut()
            .events
            .push(TerminalEvent::Write(data.to_owned()));
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

    fn move_by(&mut self, _lines: isize) {}

    fn hide_cursor(&mut self) {
        self.facts
            .borrow_mut()
            .events
            .push(TerminalEvent::HideCursor);
    }

    fn show_cursor(&mut self) {
        self.facts
            .borrow_mut()
            .events
            .push(TerminalEvent::ShowCursor);
    }

    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _title: &str) {}
    fn set_progress(&mut self, _active: bool) {}
}

fn harness(
    now: u64,
    columns: usize,
    rows: usize,
    images_supported: bool,
) -> (TuiBaseController, TerminalHandle, Rc<RefCell<HostFacts>>) {
    harness_with_cursor(now, columns, rows, images_supported, false)
}

fn harness_with_cursor(
    now: u64,
    columns: usize,
    rows: usize,
    images_supported: bool,
    show_hardware_cursor: bool,
) -> (TuiBaseController, TerminalHandle, Rc<RefCell<HostFacts>>) {
    let terminal_facts = Rc::new(RefCell::new(TerminalFacts {
        columns,
        rows,
        events: Vec::new(),
        input: None,
        resize: None,
    }));
    let terminal = TerminalHandle {
        facts: Rc::clone(&terminal_facts),
    };
    let host_facts = Rc::new(RefCell::new(HostFacts::new(now, images_supported)));
    let controller = TuiBaseController::new(
        Box::new(FakeTerminal {
            facts: terminal_facts,
        }),
        Box::new(FakeHost {
            facts: Rc::clone(&host_facts),
            render_hook: None,
        }),
        TuiMode::Regular,
        show_hardware_cursor,
    );
    (controller, terminal, host_facts)
}

#[derive(Default)]
struct ProbeFacts {
    focused: bool,
    wants_key_release: bool,
    inputs: Vec<String>,
    invalidations: usize,
    renders: Vec<usize>,
}

struct ProbeComponent {
    facts: Rc<RefCell<ProbeFacts>>,
    lines: Vec<String>,
}

impl Component for ProbeComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.facts.borrow_mut().renders.push(width);
        self.lines.clone()
    }

    fn invalidate(&mut self) {
        self.facts.borrow_mut().invalidations += 1;
    }

    fn handle_input(&mut self, data: &str) {
        self.facts.borrow_mut().inputs.push(data.to_owned());
    }

    fn wants_key_release(&self) -> bool {
        self.facts.borrow().wants_key_release
    }

    fn focused(&self) -> Option<bool> {
        Some(self.facts.borrow().focused)
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.facts.borrow_mut().focused = focused;
        true
    }
}

fn probe(lines: &[&str]) -> (ComponentHandle<ProbeComponent>, Rc<RefCell<ProbeFacts>>) {
    let facts = Rc::new(RefCell::new(ProbeFacts::default()));
    (
        ComponentHandle::new(ProbeComponent {
            facts: Rc::clone(&facts),
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
        }),
        facts,
    )
}

struct ReentrantProbeComponent {
    facts: Rc<RefCell<ProbeFacts>>,
    lines: Vec<String>,
    on_invalidate: Option<Box<dyn FnMut()>>,
    on_focused: Option<Box<dyn FnMut(bool)>>,
}

impl Component for ReentrantProbeComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.facts.borrow_mut().renders.push(width);
        self.lines.clone()
    }

    fn invalidate(&mut self) {
        self.facts.borrow_mut().invalidations += 1;
        if let Some(callback) = self.on_invalidate.as_mut() {
            callback();
        }
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.facts.borrow_mut().focused = focused;
        if let Some(callback) = self.on_focused.as_mut() {
            callback(focused);
        }
        true
    }

    fn focused(&self) -> Option<bool> {
        Some(self.facts.borrow().focused)
    }
}

fn invalidation_probe(
    label: &'static str,
    events: Rc<RefCell<Vec<&'static str>>>,
    mut reenter: impl FnMut() + 'static,
) -> ComponentHandle<ReentrantProbeComponent> {
    ComponentHandle::new(ReentrantProbeComponent {
        facts: Rc::new(RefCell::new(ProbeFacts::default())),
        lines: vec![label.into()],
        on_invalidate: Some(Box::new(move || {
            events.borrow_mut().push(label);
            reenter();
        })),
        on_focused: None,
    })
}

#[test]
fn start_render_coalescing_and_stop_follow_the_fake_clock() {
    let (controller, terminal, host) = harness(100, 90, 30, true);
    assert_eq!(controller.mode(), TuiMode::Regular);
    assert!(!controller.show_hardware_cursor());
    assert!(!controller.clear_on_shrink());
    controller.set_terminal_color_scheme_notifications(true);
    controller.start();
    assert_eq!(
        terminal.events(),
        [
            TerminalEvent::Write("\x1b[?2031h".into()),
            TerminalEvent::Start,
            TerminalEvent::HideCursor,
            TerminalEvent::Write("\x1b[?2031h".into()),
            TerminalEvent::Write("\x1b[16t".into()),
        ]
    );
    assert_eq!(
        host.borrow().events[..3],
        [
            HostEvent::BeforeStart,
            HostEvent::AfterStart,
            HostEvent::Schedule(TuiTaskId(1), 0, TuiHostTask::ScheduleRender),
        ]
    );
    run_due(&controller, &host);
    assert_eq!(
        host.borrow()
            .events
            .iter()
            .filter(|event| matches!(event, HostEvent::Render(_)))
            .count(),
        1
    );

    host.borrow_mut().now = 105;
    controller.request_render(false);
    controller.request_render(false);
    run_due(&controller, &host);
    assert_eq!(
        host.borrow()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    HostEvent::Schedule(_, _, TuiHostTask::ScheduleRender)
                )
            })
            .count(),
        2,
        "the two same-turn requests share one new next-tick task"
    );
    let pending = host
        .borrow()
        .scheduled
        .values()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].due, 116);
    assert_eq!(pending[0].task, TuiHostTask::RenderTimer);

    controller.request_render(true);
    run_due(&controller, &host);
    {
        let host = host.borrow();
        assert!(host.events.contains(&HostEvent::Reset(105)));
        assert!(host.events.contains(&HostEvent::Render(105)));
    }
    assert!(host.borrow().scheduled.is_empty());

    host.borrow_mut().now = 125;
    controller.request_render(false);
    run_due(&controller, &host);
    controller.request_render(false);
    run_due(&controller, &host);
    controller.stop(TuiStopOptions {
        preserve_screen: true,
    });
    assert!(
        host.borrow().scheduled.is_empty(),
        "stop cancels the owned render timer instead of leaving a stale task"
    );
    advance_to(&controller, &host, 200);
    assert!(host.borrow().scheduled.is_empty());
    assert!(host.borrow().events.contains(&HostEvent::BeforeStop(true)));
    assert!(host.borrow().events.contains(&HostEvent::AfterStop(true)));
    assert!(terminal.events().ends_with(&[
        TerminalEvent::Write("\x1b[?2031l".into()),
        TerminalEvent::ShowCursor,
        TerminalEvent::Stop,
    ]));
}

#[test]
fn host_render_can_request_a_follow_up_frame_reentrantly() {
    let case = fixture_case("reentrant-render-schedules-follow-up-frame");
    assert_eq!(
        case["renders"],
        serde_json::json!([["render", 100], ["render", 116]])
    );
    assert_eq!(
        case["afterFirstFrame"]["pendingTimers"],
        serde_json::json!([116])
    );

    let terminal_facts = Rc::new(RefCell::new(TerminalFacts {
        columns: 80,
        rows: 24,
        events: Vec::new(),
        input: None,
        resize: None,
    }));
    let host_facts = Rc::new(RefCell::new(HostFacts::new(100, false)));
    let render_hook = Rc::new(RefCell::new(None::<Box<dyn FnMut()>>));
    let controller = Rc::new(TuiBaseController::new(
        Box::new(FakeTerminal {
            facts: terminal_facts,
        }),
        Box::new(FakeHost {
            facts: Rc::clone(&host_facts),
            render_hook: Some(Rc::clone(&render_hook)),
        }),
        TuiMode::Regular,
        false,
    ));
    let weak = Rc::downgrade(&controller);
    let render_count = Rc::new(RefCell::new(0));
    let render_count_copy = Rc::clone(&render_count);
    *render_hook.borrow_mut() = Some(Box::new(move || {
        let mut count = render_count_copy.borrow_mut();
        *count += 1;
        if *count == 1
            && let Some(controller) = weak.upgrade()
        {
            controller.request_render(false);
        }
    }));

    controller.request_render(false);
    run_due(&controller, &host_facts);
    assert_eq!(*render_count.borrow(), 1);
    let pending = host_facts
        .borrow()
        .scheduled
        .values()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].due, 116);
    assert_eq!(pending[0].task, TuiHostTask::RenderTimer);

    advance_to(&controller, &host_facts, 116);
    assert_eq!(*render_count.borrow(), 2);
    let renders = host_facts
        .borrow()
        .events
        .iter()
        .filter_map(|event| match event {
            HostEvent::Render(now) => Some(*now),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(renders, [100, 116]);
}

#[test]
fn every_host_callback_can_reenter_a_read_and_host_requiring_operation() {
    for target in [
        HostCallbackPoint::Now,
        HostCallbackPoint::Schedule,
        HostCallbackPoint::Cancel,
        HostCallbackPoint::Render,
        HostCallbackPoint::Reset,
        HostCallbackPoint::ImagesSupported,
        HostCallbackPoint::CellDimensions,
        HostCallbackPoint::BeforeStart,
        HostCallbackPoint::AfterStart,
        HostCallbackPoint::BeforeStop,
        HostCallbackPoint::AfterStop,
        HostCallbackPoint::Dropped,
    ] {
        let terminal_facts = Rc::new(RefCell::new(TerminalFacts {
            columns: 80,
            rows: 24,
            events: Vec::new(),
            input: None,
            resize: None,
        }));
        let host_facts = Rc::new(RefCell::new(HostFacts::new(0, true)));
        let controller_pointer = Rc::new(Cell::new(std::ptr::null()));
        let fired = Rc::new(Cell::new(false));
        let reads = Rc::new(RefCell::new(Vec::new()));
        let controller = Box::new(TuiBaseController::new(
            Box::new(FakeTerminal {
                facts: terminal_facts,
            }),
            Box::new(ReentrantHost {
                facts: Rc::clone(&host_facts),
                target,
                controller: Rc::clone(&controller_pointer),
                fired: Rc::clone(&fired),
                reads: Rc::clone(&reads),
            }),
            TuiMode::Regular,
            false,
        ));
        controller_pointer.set(&*controller);

        match target {
            HostCallbackPoint::Now => {
                controller.request_render(false);
                run_due(&controller, &host_facts);
            }
            HostCallbackPoint::Schedule => controller.request_render(false),
            HostCallbackPoint::Cancel => {
                controller.request_render(false);
                run_due(&controller, &host_facts);
                controller.request_render(true);
            }
            HostCallbackPoint::Render => {
                controller.request_render(true);
                run_due(&controller, &host_facts);
            }
            HostCallbackPoint::Reset => controller.request_render(true),
            HostCallbackPoint::ImagesSupported
            | HostCallbackPoint::BeforeStart
            | HostCallbackPoint::AfterStart => controller.start(),
            HostCallbackPoint::CellDimensions => {
                controller.handle_terminal_input("\x1b[6;18;9t");
            }
            HostCallbackPoint::BeforeStop | HostCallbackPoint::AfterStop => {
                controller.stop(TuiStopOptions::default());
            }
            HostCallbackPoint::Dropped => {}
        }

        if target == HostCallbackPoint::Dropped {
            drop(controller);
        } else {
            assert!(fired.get(), "{target:?} callback did not run");
            assert_eq!(reads.borrow().as_slice(), [(TuiMode::Regular, 80)]);
            {
                let facts = host_facts.borrow();
                let marker = facts
                    .events
                    .iter()
                    .position(|event| *event == HostEvent::Reenter(target))
                    .unwrap();
                assert!(
                    facts.events[marker + 1..]
                        .iter()
                        .any(|event| matches!(event, HostEvent::Reset(_))),
                    "{target:?} reentrant host work did not reconcile: {:?}",
                    facts.events
                );
            }
            drop(controller);
        }
        assert!(fired.get(), "{target:?} callback did not run");
        assert_eq!(reads.borrow().as_slice(), [(TuiMode::Regular, 80)]);
        if target == HostCallbackPoint::Dropped {
            let facts = host_facts.borrow();
            let marker = facts
                .events
                .iter()
                .position(|event| *event == HostEvent::Reenter(target))
                .unwrap();
            assert!(
                facts.events[marker + 1..]
                    .iter()
                    .any(|event| matches!(event, HostEvent::Reset(_))),
                "{target:?} reentrant host work did not reconcile: {:?}",
                facts.events
            );
            assert!(
                facts.scheduled.is_empty(),
                "drop left reentrant tasks scheduled"
            );
        }
    }
}

#[test]
fn render_callback_can_schedule_a_background_query_reentrantly() {
    let terminal_facts = Rc::new(RefCell::new(TerminalFacts {
        columns: 80,
        rows: 24,
        events: Vec::new(),
        input: None,
        resize: None,
    }));
    let terminal = TerminalHandle {
        facts: Rc::clone(&terminal_facts),
    };
    let host_facts = Rc::new(RefCell::new(HostFacts::new(100, false)));
    let render_hook = Rc::new(RefCell::new(None::<Box<dyn FnMut()>>));
    let controller = Rc::new(TuiBaseController::new(
        Box::new(FakeTerminal {
            facts: terminal_facts,
        }),
        Box::new(FakeHost {
            facts: Rc::clone(&host_facts),
            render_hook: Some(Rc::clone(&render_hook)),
        }),
        TuiMode::Regular,
        false,
    ));
    let weak = Rc::downgrade(&controller);
    let fired = Rc::new(Cell::new(false));
    let hook_fired = Rc::clone(&fired);
    *render_hook.borrow_mut() = Some(Box::new(move || {
        if hook_fired.replace(true) {
            return;
        }
        if let Some(controller) = weak.upgrade() {
            controller.query_terminal_background_color(50, Box::new(|_| {}));
        }
    }));

    controller.request_render(true);
    run_due(&controller, &host_facts);
    assert!(fired.get());
    assert!(
        terminal
            .events()
            .contains(&TerminalEvent::Write("\x1b]11;?\x07".into()))
    );
    assert!(host_facts.borrow().scheduled.values().any(|scheduled| {
        scheduled.task == TuiHostTask::BackgroundQueryTimeout { query_id: 1 }
    }));
}

#[test]
fn terminal_callbacks_wake_and_drain_input_and_resize() {
    let case = fixture_case("terminal-callbacks-drive-input-and-resize");
    assert_eq!(
        case["beforeFlush"]["componentEvents"],
        serde_json::json!([["input", "x"]])
    );
    let (controller, terminal, host) = harness(100, 80, 24, false);
    let (focus, focus_facts) = probe(&["focus"]);
    controller.set_focus(Some(focus.as_component_ref()));
    controller.start();
    run_due(&controller, &host);
    host.borrow_mut().events.clear();

    terminal.feed("x");
    terminal.resize(81, 25);
    assert_eq!(controller.terminal_events_pending(), 2);
    run_due(&controller, &host);

    assert_eq!(focus_facts.borrow().inputs, ["x"]);
    assert_eq!(controller.terminal_columns(), 81);
    assert_eq!(controller.terminal_rows(), 25);
    assert_eq!(controller.terminal_events_pending(), 0);
    assert_eq!(
        host.borrow()
            .events
            .iter()
            .filter(|event| matches!(event, HostEvent::Render(100)))
            .count(),
        1
    );
}

#[test]
fn listener_transform_consume_release_and_debug_priority_match() {
    let (controller, _, host) = harness(0, 80, 24, false);
    let (focus, focus_facts) = probe(&["focus"]);
    controller.set_focus(Some(focus.as_component_ref()));
    let events = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let first_events = Rc::clone(&events);
    let first = TuiInputListener::new(move |data| {
        first_events
            .borrow_mut()
            .push(("first".into(), data.into()));
        Some(TuiInputListenerResult::transform(format!("<{data}>")))
    });
    let second_events = Rc::clone(&events);
    let second = TuiInputListener::new(move |data| {
        second_events
            .borrow_mut()
            .push(("second".into(), data.into()));
        if data.contains("block") {
            Some(TuiInputListenerResult::consume())
        } else {
            Some(TuiInputListenerResult::transform(format!("{data}!")))
        }
    });
    let third_events = Rc::clone(&events);
    let third = TuiInputListener::new(move |data| {
        third_events
            .borrow_mut()
            .push(("third".into(), data.into()));
        None
    });
    let remove_first = controller.add_input_listener(first.clone());
    controller.add_input_listener(first);
    controller.add_input_listener(second);
    controller.add_input_listener(third);
    controller.handle_terminal_input("x");
    run_due(&controller, &host);
    controller.handle_terminal_input("block");
    remove_first.unsubscribe();
    controller.handle_terminal_input("y");
    run_due(&controller, &host);
    assert_eq!(
        focus_facts.borrow().inputs,
        ["<x>!".to_owned(), "y!".to_owned()]
    );
    assert_eq!(
        host.borrow()
            .events
            .iter()
            .filter(|event| matches!(event, HostEvent::Render(0)))
            .count(),
        2,
        "accepted input preempts the throttled render path"
    );

    let empty = TuiInputListener::new(|_| Some(TuiInputListenerResult::transform("")));
    let empty_subscription = controller.add_input_listener(empty);
    controller.handle_terminal_input("z");
    empty_subscription.unsubscribe();
    assert_eq!(focus_facts.borrow().inputs.len(), 2);

    controller.handle_terminal_input("\x1b[97;1:3u");
    assert_eq!(focus_facts.borrow().inputs.len(), 2);
    focus_facts.borrow_mut().wants_key_release = true;
    controller.handle_terminal_input("\x1b[97;1:3u");
    assert_eq!(focus_facts.borrow().inputs.last().unwrap(), "\x1b[97;1:3u!");

    let debug_calls = Rc::new(RefCell::new(0));
    let debug_calls_copy = Rc::clone(&debug_calls);
    controller.set_debug_callback(Some(Box::new(move || {
        *debug_calls_copy.borrow_mut() += 1;
    })));
    controller.handle_terminal_input("\x1b[100;6u");
    assert_eq!(
        *debug_calls.borrow(),
        0,
        "transforms precede debug matching"
    );

    let (raw, _, _) = harness(0, 80, 24, false);
    let raw_calls = Rc::new(RefCell::new(0));
    let raw_calls_copy = Rc::clone(&raw_calls);
    raw.set_debug_callback(Some(Box::new(move || {
        *raw_calls_copy.borrow_mut() += 1;
    })));
    raw.handle_terminal_input("\x1b[100;6u");
    assert_eq!(*raw_calls.borrow(), 1);
}

#[test]
fn debug_callback_replacement_survives_current_dispatch() {
    let case = fixture_case("debug-callback-replacement");
    assert_eq!(case["events"], serde_json::json!(["first", "replacement"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let first_events = Rc::clone(&events);
    let weak = Rc::downgrade(&controller);
    controller.set_debug_callback(Some(Box::new(move || {
        first_events.borrow_mut().push("first".into());
        let replacement_events = Rc::clone(&first_events);
        if let Some(controller) = weak.upgrade() {
            controller.set_debug_callback(Some(Box::new(move || {
                replacement_events.borrow_mut().push("replacement".into());
            })));
        }
    })));

    controller.handle_terminal_input("\x1b[100;6u");
    controller.handle_terminal_input("\x1b[100;6u");
    assert_eq!(events.borrow().as_slice(), ["first", "replacement"]);
}

#[test]
fn input_listener_can_recursively_dispatch_the_same_listener() {
    let case = fixture_case("recursive-input-listener-dispatch");
    assert_eq!(
        case["events"],
        serde_json::json!([
            ["a", "outer"],
            ["a", "inner"],
            ["b", "inner"],
            ["b", "outer"]
        ])
    );
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let (focus, focus_facts) = probe(&["focus"]);
    controller.set_focus(Some(focus.as_component_ref()));
    let events = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let a_events = Rc::clone(&events);
    let weak = Rc::downgrade(&controller);
    controller.add_input_listener(TuiInputListener::new(move |data| {
        a_events.borrow_mut().push(("a".into(), data.into()));
        if data == "outer"
            && let Some(controller) = weak.upgrade()
        {
            controller.handle_terminal_input("inner");
        }
        None
    }));
    let b_events = Rc::clone(&events);
    controller.add_input_listener(TuiInputListener::new(move |data| {
        b_events.borrow_mut().push(("b".into(), data.into()));
        None
    }));

    controller.handle_terminal_input("outer");
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("a".into(), "outer".into()),
            ("a".into(), "inner".into()),
            ("b".into(), "inner".into()),
            ("b".into(), "outer".into()),
        ]
    );
    assert_eq!(focus_facts.borrow().inputs, ["inner", "outer"]);
}

#[test]
fn debug_callback_can_recursively_dispatch_itself() {
    let case = fixture_case("recursive-debug-dispatch");
    assert_eq!(case["events"], serde_json::json!(["outer", "inner"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let callback_events = Rc::clone(&events);
    let weak = Rc::downgrade(&controller);
    controller.set_debug_callback(Some(Box::new(move || {
        let first = callback_events.borrow().is_empty();
        callback_events
            .borrow_mut()
            .push(if first { "outer" } else { "inner" }.into());
        if first && let Some(controller) = weak.upgrade() {
            controller.handle_terminal_input("\x1b[100;6u");
        }
    })));

    controller.handle_terminal_input("\x1b[100;6u");
    assert_eq!(events.borrow().as_slice(), ["outer", "inner"]);
}

#[test]
fn focus_setter_can_request_render_reentrantly() {
    let case = fixture_case("reentrant-focus-setter-request-render");
    assert_eq!(case["events"], serde_json::json!([["focused", true]]));
    assert_eq!(case["renders"], serde_json::json!([["render", 16]]));
    let (controller, _, host) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let facts = Rc::new(RefCell::new(ProbeFacts::default()));
    let weak = Rc::downgrade(&controller);
    let component = ComponentHandle::new(ReentrantProbeComponent {
        facts: Rc::clone(&facts),
        lines: vec!["focus".into()],
        on_invalidate: None,
        on_focused: Some(Box::new(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.request_render(false);
            }
        })),
    });

    controller.set_focus(Some(component.as_component_ref()));
    assert!(facts.borrow().focused);
    advance_to(&controller, &host, 16);
    assert!(host.borrow().events.contains(&HostEvent::Render(16)));
}

#[test]
fn overlay_visibility_predicate_can_request_render_reentrantly() {
    let case = fixture_case("reentrant-visible-predicate-request-render");
    assert_eq!(
        case["events"],
        serde_json::json!([["visible", 81, 25], ["visible", 81, 25]])
    );
    let (controller, _, host) = harness(0, 81, 25, false);
    let controller = Rc::new(controller);
    let (overlay, overlay_facts) = probe(&["overlay"]);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let calls_copy = Rc::clone(&calls);
    let weak = Rc::downgrade(&controller);
    controller.show_overlay(
        overlay.as_component_ref(),
        OverlayOptions {
            visible: Some(Rc::new(move |width, height| {
                calls_copy.borrow_mut().push((width, height));
                if let Some(controller) = weak.upgrade() {
                    controller.request_render(false);
                }
                true
            })),
            ..OverlayOptions::default()
        },
    );

    assert_eq!(calls.borrow().as_slice(), [(81, 25), (81, 25)]);
    assert!(overlay_facts.borrow().focused);
    advance_to(&controller, &host, 16);
    assert!(host.borrow().events.contains(&HostEvent::Render(16)));
}

#[test]
fn component_invalidation_can_request_render_reentrantly() {
    let case = fixture_case("reentrant-invalidate-request-render");
    assert_eq!(case["events"], serde_json::json!(["invalidate"]));
    assert_eq!(case["renders"], serde_json::json!([["render", 16]]));
    let (controller, _, host) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let facts = Rc::new(RefCell::new(ProbeFacts::default()));
    let weak = Rc::downgrade(&controller);
    let component = ComponentHandle::new(ReentrantProbeComponent {
        facts: Rc::clone(&facts),
        lines: vec!["root".into()],
        on_invalidate: Some(Box::new(move || {
            if let Some(controller) = weak.upgrade() {
                controller.request_render(false);
            }
        })),
        on_focused: None,
    });
    controller.add_child(component.as_component_ref());

    controller.invalidate();
    assert_eq!(facts.borrow().invalidations, 1);
    advance_to(&controller, &host, 16);
    assert!(host.borrow().events.contains(&HostEvent::Render(16)));
}

#[test]
fn invalidation_root_iteration_observes_live_deletion() {
    let case = fixture_case("invalidate-live-root-deletion");
    assert_eq!(case["events"], serde_json::json!(["a"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::new()));
    let b_id = Rc::new(Cell::new(None::<ContainerChildId>));
    let weak = Rc::downgrade(&controller);
    let a_b_id = Rc::clone(&b_id);
    let a = invalidation_probe("a", Rc::clone(&events), move || {
        if let (Some(controller), Some(id)) = (weak.upgrade(), a_b_id.get()) {
            controller.remove_child(id);
        }
    });
    let b = invalidation_probe("b", Rc::clone(&events), || {});
    controller.add_child(a.as_component_ref());
    b_id.set(Some(controller.add_child(b.as_component_ref())));

    controller.invalidate();
    assert_eq!(events.borrow().as_slice(), ["a"]);
}

#[test]
fn invalidation_root_iteration_observes_live_insertion() {
    let case = fixture_case("invalidate-live-root-insertion");
    assert_eq!(case["events"], serde_json::json!(["a", "c"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::new()));
    let weak = Rc::downgrade(&controller);
    let appended = Rc::new(Cell::new(false));
    let a_appended = Rc::clone(&appended);
    let c = invalidation_probe("c", Rc::clone(&events), || {});
    let c_ref = c.as_component_ref();
    let a = invalidation_probe("a", Rc::clone(&events), move || {
        if !a_appended.replace(true)
            && let Some(controller) = weak.upgrade()
        {
            controller.add_child(c_ref.clone());
        }
    });
    controller.add_child(a.as_component_ref());

    controller.invalidate();
    assert_eq!(events.borrow().as_slice(), ["a", "c"]);
}

#[test]
fn invalidation_root_clear_rebinds_the_active_array_identity() {
    let case = fixture_case("invalidate-root-clear-rebinds-array");
    assert_eq!(case["events"], serde_json::json!(["a", "b"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::new()));
    let weak = Rc::downgrade(&controller);
    let d = invalidation_probe("d", Rc::clone(&events), || {});
    let e = invalidation_probe("e", Rc::clone(&events), || {});
    let d_ref = d.as_component_ref();
    let e_ref = e.as_component_ref();
    let a = invalidation_probe("a", Rc::clone(&events), move || {
        if let Some(controller) = weak.upgrade() {
            controller.clear();
            controller.add_child(d_ref.clone());
            controller.add_child(e_ref.clone());
        }
    });
    let b = invalidation_probe("b", Rc::clone(&events), || {});
    controller.add_child(a.as_component_ref());
    controller.add_child(b.as_component_ref());

    controller.invalidate();
    assert_eq!(events.borrow().as_slice(), ["a", "b"]);
}

#[test]
fn invalidation_overlay_iteration_observes_live_deletion() {
    let case = fixture_case("invalidate-live-overlay-deletion");
    assert_eq!(case["events"], serde_json::json!(["a"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::new()));
    let b_handle = Rc::new(RefCell::new(None::<OverlayHandle>));
    let a_b_handle = Rc::clone(&b_handle);
    let a = invalidation_probe("a", Rc::clone(&events), move || {
        if let Some(handle) = a_b_handle.borrow().as_ref() {
            handle.hide();
        }
    });
    let b = invalidation_probe("b", Rc::clone(&events), || {});
    controller.show_overlay(
        a.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );
    *b_handle.borrow_mut() = Some(controller.show_overlay(
        b.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            ..OverlayOptions::default()
        },
    ));

    controller.invalidate();
    assert_eq!(events.borrow().as_slice(), ["a"]);
}

#[test]
fn invalidation_overlay_iteration_observes_live_insertion() {
    let case = fixture_case("invalidate-live-overlay-insertion");
    assert_eq!(case["events"], serde_json::json!(["a", "c"]));
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::new()));
    let weak = Rc::downgrade(&controller);
    let appended = Rc::new(Cell::new(false));
    let a_appended = Rc::clone(&appended);
    let c = invalidation_probe("c", Rc::clone(&events), || {});
    let c_ref = c.as_component_ref();
    let a = invalidation_probe("a", Rc::clone(&events), move || {
        if !a_appended.replace(true)
            && let Some(controller) = weak.upgrade()
        {
            controller.show_overlay(
                c_ref.clone(),
                OverlayOptions {
                    non_capturing: true,
                    ..OverlayOptions::default()
                },
            );
        }
    });
    controller.show_overlay(
        a.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );

    controller.invalidate();
    assert_eq!(events.borrow().as_slice(), ["a", "c"]);
}

#[test]
fn input_listener_dispatch_uses_live_set_mutation_order() {
    let case = fixture_case("input-listener-live-set-mutation");
    assert_eq!(
        case["firstDispatch"],
        serde_json::json!([["a", "x"], ["c", "x"]])
    );
    let (controller, _, _) = harness(0, 80, 24, false);
    let (focus, focus_facts) = probe(&["focus"]);
    controller.set_focus(Some(focus.as_component_ref()));
    let events = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let b_subscription = Rc::new(RefCell::new(None::<TuiSubscription>));

    let a_events = Rc::clone(&events);
    let a_b_subscription = Rc::clone(&b_subscription);
    controller.add_input_listener(TuiInputListener::new(move |data| {
        a_events.borrow_mut().push(("a".into(), data.into()));
        if let Some(subscription) = a_b_subscription.borrow().as_ref() {
            subscription.unsubscribe();
        }
        None
    }));
    let b_events = Rc::clone(&events);
    *b_subscription.borrow_mut() = Some(controller.add_input_listener(TuiInputListener::new(
        move |data| {
            b_events.borrow_mut().push(("b".into(), data.into()));
            None
        },
    )));
    let c_events = Rc::clone(&events);
    controller.add_input_listener(TuiInputListener::new(move |data| {
        c_events.borrow_mut().push(("c".into(), data.into()));
        None
    }));

    controller.handle_terminal_input("x");
    assert_eq!(
        events.borrow().as_slice(),
        [("a".into(), "x".into()), ("c".into(), "x".into())]
    );
    controller.handle_terminal_input("y");
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("a".into(), "x".into()),
            ("c".into(), "x".into()),
            ("a".into(), "y".into()),
            ("c".into(), "y".into()),
        ]
    );
    assert_eq!(focus_facts.borrow().inputs, ["x", "y"]);
}

#[test]
fn terminal_event_queue_cell_size_and_invalidation_are_ordered() {
    let (controller, terminal, host) = harness(0, 80, 24, true);
    let (base, base_facts) = probe(&["base"]);
    let (overlay, overlay_facts) = probe(&["overlay"]);
    controller.add_child(base.as_component_ref());
    let overlay_handle = controller.show_overlay(
        overlay.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );
    let listener_data = Rc::new(RefCell::new(Vec::new()));
    let listener_data_copy = Rc::clone(&listener_data);
    let listener_cell_counts = Rc::new(RefCell::new(Vec::new()));
    let listener_cell_counts_copy = Rc::clone(&listener_cell_counts);
    let listener_host = Rc::clone(&host);
    controller.add_input_listener(TuiInputListener::new(move |data| {
        listener_data_copy.borrow_mut().push(data.to_owned());
        listener_cell_counts_copy.borrow_mut().push(
            listener_host
                .borrow()
                .events
                .iter()
                .filter(|event| matches!(event, HostEvent::Cell(_, _)))
                .count(),
        );
        None
    }));
    controller.start();
    terminal.feed("\x1b[6;0;7t");
    terminal.feed("\x1b[6;20;10t");
    terminal.resize(90, 30);
    assert_eq!(controller.terminal_events_pending(), 3);
    controller.drain_terminal_events();
    assert_eq!(controller.terminal_columns(), 90);
    assert_eq!(controller.terminal_rows(), 30);
    assert_eq!(
        *listener_data.borrow(),
        ["\x1b[6;0;7t".to_owned(), "\x1b[6;20;10t".to_owned()]
    );
    assert_eq!(*listener_cell_counts.borrow(), [0, 0]);
    assert_eq!(base_facts.borrow().invalidations, 1);
    assert_eq!(overlay_facts.borrow().invalidations, 1);
    assert_eq!(host.borrow().cell_dimensions.width_px, 10.0);
    assert_eq!(host.borrow().cell_dimensions.height_px, 20.0);
    assert!(host.borrow().events.contains(&HostEvent::Cell(10, 20)));
    overlay_handle.hide();
}

#[test]
fn osc11_timeout_keeps_the_stale_fifo_reply_slot() {
    let (controller, terminal, host) = harness(0, 80, 24, false);
    let results = Rc::new(RefCell::new(Vec::new()));
    let first_results = Rc::clone(&results);
    controller.query_terminal_background_color(
        10,
        Box::new(move |value| first_results.borrow_mut().push(("first", value))),
    );
    let second_results = Rc::clone(&results);
    controller.query_terminal_background_color(
        30,
        Box::new(move |value| second_results.borrow_mut().push(("second", value))),
    );
    advance_to(&controller, &host, 10);
    assert_eq!(results.borrow().as_slice(), [("first", None)]);

    controller.handle_terminal_input("\x1b]11;#112233\x07");
    assert_eq!(
        results.borrow().len(),
        1,
        "first reply consumes timed-out FIFO slot"
    );
    controller.handle_terminal_input("\x1b]11;rgb:ffff/0000/8080\x1b\\");
    let results = results.borrow();
    assert_eq!(results.len(), 2);
    let color = results[1].1.unwrap();
    assert_eq!((color.r, color.g, color.b), (255, 0, 128));
    drop(results);

    let invalid_results = Rc::new(RefCell::new(Vec::new()));
    let invalid_results_copy = Rc::clone(&invalid_results);
    controller.query_terminal_background_color(
        50,
        Box::new(move |value| invalid_results_copy.borrow_mut().push(value)),
    );
    controller.handle_terminal_input("\x1b]11;bogus\x07");
    assert_eq!(invalid_results.borrow().as_slice(), [None]);
    assert_eq!(
        terminal
            .events()
            .iter()
            .filter(|event| event == &&TerminalEvent::Write("\x1b]11;?\x07".into()))
            .count(),
        3
    );
}

#[test]
fn color_scheme_query_listener_order_and_notification_toggles_match() {
    let (controller, terminal, host) = harness(0, 80, 24, false);
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let a_notifications = Rc::clone(&notifications);
    let a = TerminalColorSchemeListener::new(move |scheme| {
        a_notifications.borrow_mut().push(("a", scheme));
    });
    let b_notifications = Rc::clone(&notifications);
    let b = TerminalColorSchemeListener::new(move |scheme| {
        b_notifications.borrow_mut().push(("b", scheme));
    });
    let remove_a = controller.on_terminal_color_scheme_change(a);
    controller.on_terminal_color_scheme_change(b);
    let query_results = Rc::new(RefCell::new(Vec::new()));
    let query_results_copy = Rc::clone(&query_results);
    controller.query_terminal_color_scheme(
        20,
        Box::new(move |scheme| query_results_copy.borrow_mut().push(scheme)),
    );
    controller.handle_terminal_input("\x1b[?997;1n\x1b[?997;2n");
    assert_eq!(
        notifications.borrow().as_slice(),
        [
            ("a", TerminalColorScheme::Light),
            ("b", TerminalColorScheme::Light),
        ]
    );
    assert_eq!(
        query_results.borrow().as_slice(),
        [Some(TerminalColorScheme::Light)]
    );
    remove_a.unsubscribe();
    controller.handle_terminal_input("\x1b[?997;1n");
    assert_eq!(
        notifications.borrow().last(),
        Some(&("b", TerminalColorScheme::Dark))
    );

    let timeout_results = Rc::new(RefCell::new(Vec::new()));
    let timeout_results_copy = Rc::clone(&timeout_results);
    controller.query_terminal_color_scheme(
        5,
        Box::new(move |scheme| timeout_results_copy.borrow_mut().push(scheme)),
    );
    advance_to(&controller, &host, 5);
    assert_eq!(timeout_results.borrow().as_slice(), [None]);
    controller.set_terminal_color_scheme_notifications(true);
    controller.set_terminal_color_scheme_notifications(true);
    controller.set_terminal_color_scheme_notifications(false);
    assert_eq!(
        terminal.events(),
        [
            TerminalEvent::Write("\x1b[?996n".into()),
            TerminalEvent::Write("\x1b[?996n".into()),
            TerminalEvent::Write("\x1b[?2031h".into()),
            TerminalEvent::Write("\x1b[?2031l".into()),
        ]
    );
}

#[test]
fn scheme_dispatch_is_live_and_queries_share_insertion_order() {
    let case = fixture_case("scheme-listener-live-set-and-query-order");
    assert_eq!(
        case["firstDispatch"],
        serde_json::json!([["a", "light"], ["query", "light"], ["c", "light"]])
    );
    let (controller, _, _) = harness(0, 80, 24, false);
    let events = Rc::new(RefCell::new(Vec::<(String, TerminalColorScheme)>::new()));
    let b_subscription = Rc::new(RefCell::new(None::<TuiSubscription>));

    let a_events = Rc::clone(&events);
    let a_b_subscription = Rc::clone(&b_subscription);
    controller.on_terminal_color_scheme_change(TerminalColorSchemeListener::new(move |scheme| {
        a_events.borrow_mut().push(("a".into(), scheme));
        if let Some(subscription) = a_b_subscription.borrow().as_ref() {
            subscription.unsubscribe();
        }
    }));
    let query_events = Rc::clone(&events);
    controller.query_terminal_color_scheme(
        20,
        Box::new(move |scheme| {
            query_events
                .borrow_mut()
                .push(("query".into(), scheme.expect("scheme report")));
        }),
    );
    let b_events = Rc::clone(&events);
    *b_subscription.borrow_mut() = Some(controller.on_terminal_color_scheme_change(
        TerminalColorSchemeListener::new(move |scheme| {
            b_events.borrow_mut().push(("b".into(), scheme));
        }),
    ));
    let c_events = Rc::clone(&events);
    controller.on_terminal_color_scheme_change(TerminalColorSchemeListener::new(move |scheme| {
        c_events.borrow_mut().push(("c".into(), scheme));
    }));

    controller.handle_terminal_input("\x1b[?997;2n");
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("a".into(), TerminalColorScheme::Light),
            ("query".into(), TerminalColorScheme::Light),
            ("c".into(), TerminalColorScheme::Light),
        ]
    );
    controller.handle_terminal_input("\x1b[?997;1n");
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("a".into(), TerminalColorScheme::Light),
            ("query".into(), TerminalColorScheme::Light),
            ("c".into(), TerminalColorScheme::Light),
            ("a".into(), TerminalColorScheme::Dark),
            ("c".into(), TerminalColorScheme::Dark),
        ]
    );
}

#[test]
fn scheme_listener_can_recursively_dispatch_the_same_listener() {
    let case = fixture_case("recursive-scheme-listener-dispatch");
    assert_eq!(
        case["events"],
        serde_json::json!([["a", "light"], ["a", "dark"], ["b", "dark"], ["b", "light"]])
    );
    let (controller, _, _) = harness(0, 80, 24, false);
    let controller = Rc::new(controller);
    let events = Rc::new(RefCell::new(Vec::<(String, TerminalColorScheme)>::new()));
    let a_events = Rc::clone(&events);
    let weak = Rc::downgrade(&controller);
    controller.on_terminal_color_scheme_change(TerminalColorSchemeListener::new(move |scheme| {
        a_events.borrow_mut().push(("a".into(), scheme));
        if scheme == TerminalColorScheme::Light
            && let Some(controller) = weak.upgrade()
        {
            controller.handle_terminal_input("\x1b[?997;1n");
        }
    }));
    let b_events = Rc::clone(&events);
    controller.on_terminal_color_scheme_change(TerminalColorSchemeListener::new(move |scheme| {
        b_events.borrow_mut().push(("b".into(), scheme));
    }));

    controller.handle_terminal_input("\x1b[?997;2n");
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("a".into(), TerminalColorScheme::Light),
            ("a".into(), TerminalColorScheme::Dark),
            ("b".into(), TerminalColorScheme::Dark),
            ("b".into(), TerminalColorScheme::Light),
        ]
    );
}

fn focus_tuple(
    root: &Rc<RefCell<ProbeFacts>>,
    a: &Rc<RefCell<ProbeFacts>>,
    b: &Rc<RefCell<ProbeFacts>>,
    hidden: &Rc<RefCell<ProbeFacts>>,
) -> (bool, bool, bool, bool) {
    (
        root.borrow().focused,
        a.borrow().focused,
        b.borrow().focused,
        hidden.borrow().focused,
    )
}

#[test]
fn overlay_focus_stack_handles_own_visibility_and_restore() {
    let (controller, _, _) = harness(0, 40, 12, false);
    let (root, root_facts) = probe(&["root"]);
    let (a, a_facts) = probe(&["AAAA"]);
    let (b, b_facts) = probe(&["BBBB"]);
    let (hidden, hidden_facts) = probe(&["NO"]);
    controller.add_child(root.as_component_ref());
    controller.set_focus(Some(root.as_component_ref()));
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (true, false, false, false)
    );
    let (passive, passive_facts) = probe(&["passive"]);
    let passive_handle = controller.show_overlay(
        passive.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );
    assert!(root_facts.borrow().focused);
    assert!(!passive_facts.borrow().focused);
    passive_handle.hide();
    let a_handle = controller.show_overlay(
        a.as_component_ref(),
        OverlayOptions {
            anchor: OverlayAnchor::TopLeft,
            width: Some(SizeValue::Absolute(4.0)),
            ..OverlayOptions::default()
        },
    );
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (false, true, false, false)
    );
    let b_handle = controller.show_overlay(
        b.as_component_ref(),
        OverlayOptions {
            anchor: OverlayAnchor::BottomRight,
            width: Some(SizeValue::Absolute(4.0)),
            ..OverlayOptions::default()
        },
    );
    let hidden_handle = controller.show_overlay(
        hidden.as_component_ref(),
        OverlayOptions {
            visible: Some(Rc::new(|_, _| false)),
            ..OverlayOptions::default()
        },
    );
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (false, false, true, false)
    );
    b_handle.unfocus(OverlayUnfocus::Restore);
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (false, true, false, false)
    );
    b_handle.focus();
    b_handle.set_hidden(true);
    assert!(b_handle.is_hidden());
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (false, true, false, false)
    );
    b_handle.set_hidden(false);
    assert!(b_handle.is_focused());
    b_handle.hide();
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (false, true, false, false)
    );
    a_handle.hide();
    assert_eq!(
        focus_tuple(&root_facts, &a_facts, &b_facts, &hidden_facts),
        (true, false, false, false)
    );
    assert!(!controller.has_overlay());
    assert!(controller.has_overlay_entries());
    hidden_handle.hide();
    assert!(!controller.has_overlay_entries());
}

#[test]
fn has_overlay_short_circuits_later_visibility_predicates() {
    let case = fixture_case("has-overlay-short-circuits-later-visible");
    assert_eq!(case["result"], true);
    assert_eq!(case["events"], serde_json::json!(["first"]));
    let (controller, _, _) = harness(0, 40, 12, false);
    let calls = Rc::new(RefCell::new(Vec::new()));
    for (name, visible) in [("first", true), ("second", true)] {
        let (component, _) = probe(&[name]);
        let calls = Rc::clone(&calls);
        controller.show_overlay(
            component.as_component_ref(),
            OverlayOptions {
                non_capturing: true,
                visible: Some(Rc::new(move |_, _| {
                    calls.borrow_mut().push(name);
                    visible
                })),
                ..OverlayOptions::default()
            },
        );
    }
    assert!(calls.borrow().is_empty(), "show must stay lazy");
    assert!(controller.has_overlay());
    assert_eq!(calls.borrow().as_slice(), ["first"]);
}

#[test]
fn topmost_skips_noncapturing_before_evaluating_visibility() {
    let case = fixture_case("topmost-skips-noncapturing-before-visible");
    assert_eq!(case["topmost"], "capturing");
    assert_eq!(case["events"], serde_json::json!(["capturing"]));
    let (controller, _, _) = harness(0, 40, 12, false);
    let (root, _) = probe(&["root"]);
    controller.set_focus(Some(root.as_component_ref()));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let (noncapturing, _) = probe(&["noncapturing"]);
    let noncapturing_calls = Rc::clone(&calls);
    controller.show_overlay(
        noncapturing.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            visible: Some(Rc::new(move |_, _| {
                noncapturing_calls.borrow_mut().push("noncapturing");
                true
            })),
            ..OverlayOptions::default()
        },
    );
    let (capturing, capturing_facts) = probe(&["capturing"]);
    let capturing_calls = Rc::clone(&calls);
    controller.show_overlay(
        capturing.as_component_ref(),
        OverlayOptions {
            visible: Some(Rc::new(move |_, _| {
                capturing_calls.borrow_mut().push("capturing");
                true
            })),
            ..OverlayOptions::default()
        },
    );
    let (trigger, _) = probe(&["trigger"]);
    let trigger = controller.show_overlay(trigger.as_component_ref(), OverlayOptions::default());
    calls.borrow_mut().clear();
    trigger.hide();
    assert_eq!(
        calls.borrow().as_slice(),
        ["capturing", "capturing"],
        "topmost selection precedes the selected overlay's focus eligibility check"
    );
    assert!(capturing_facts.borrow().focused);
}

#[test]
fn showing_a_noncapturing_overlay_never_evaluates_visibility() {
    let case = fixture_case("show-noncapturing-does-not-evaluate-visible");
    assert_eq!(case["events"], serde_json::json!([]));
    let (controller, _, _) = harness(0, 40, 12, false);
    let (component, _) = probe(&["noncapturing"]);
    let calls = Rc::new(Cell::new(0));
    let predicate_calls = Rc::clone(&calls);
    controller.show_overlay(
        component.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            visible: Some(Rc::new(move |_, _| {
                predicate_calls.set(predicate_calls.get() + 1);
                true
            })),
            ..OverlayOptions::default()
        },
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn visibility_iteration_observes_live_reentrant_overlay_removal() {
    let case = fixture_case("live-reentrant-visibility-mutation-ordering");
    assert_eq!(case["result"], true);
    assert_eq!(case["events"], serde_json::json!(["first", "third"]));
    let (controller, _, _) = harness(0, 40, 12, false);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let second = Rc::new(RefCell::new(None::<pie_app::OverlayHandle>));
    let (first_component, _) = probe(&["first"]);
    let first_calls = Rc::clone(&calls);
    let first_second = Rc::clone(&second);
    controller.show_overlay(
        first_component.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            visible: Some(Rc::new(move |_, _| {
                first_calls.borrow_mut().push("first");
                first_second.borrow().as_ref().unwrap().hide();
                false
            })),
            ..OverlayOptions::default()
        },
    );
    let (second_component, _) = probe(&["second"]);
    let second_calls = Rc::clone(&calls);
    *second.borrow_mut() = Some(controller.show_overlay(
        second_component.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            visible: Some(Rc::new(move |_, _| {
                second_calls.borrow_mut().push("second");
                false
            })),
            ..OverlayOptions::default()
        },
    ));
    let (third_component, _) = probe(&["third"]);
    let third_calls = Rc::clone(&calls);
    controller.show_overlay(
        third_component.as_component_ref(),
        OverlayOptions {
            non_capturing: true,
            visible: Some(Rc::new(move |_, _| {
                third_calls.borrow_mut().push("third");
                true
            })),
            ..OverlayOptions::default()
        },
    );
    assert!(controller.has_overlay());
    assert_eq!(calls.borrow().as_slice(), ["first", "third"]);
}

#[test]
fn layout_root_change_is_identity_aware_and_exclusive() {
    let case = fixture_case("alt-layout-root-identity-cache-and-mounted-roots");
    assert_eq!(
        case["events"],
        serde_json::json!([
            "render-request",
            "layout-invalidate",
            "render-request",
            "child-invalidate"
        ])
    );
    assert_eq!(case["mountedWithLayout"], serde_json::json!(["layout"]));
    assert_eq!(case["mountedAfterClear"], serde_json::json!(["child"]));

    let (controller, _, host) = harness(100, 80, 24, false);
    let (child, child_facts) = probe(&["child"]);
    let (layout, layout_facts) = probe(&["layout"]);
    controller.add_child(child.as_component_ref());
    let layout_ref = layout.as_component_ref();

    assert_eq!(controller.layout_cache_epoch(), 0);
    controller.set_layout_root(Some(layout_ref.clone()));
    assert_eq!(controller.layout_cache_epoch(), 1);
    controller.set_layout_root(Some(layout_ref));
    assert_eq!(controller.layout_cache_epoch(), 1);
    run_due(&controller, &host);
    controller.invalidate();
    assert_eq!(layout_facts.borrow().invalidations, 1);
    assert_eq!(child_facts.borrow().invalidations, 0);

    controller.set_layout_root(None);
    assert_eq!(controller.layout_cache_epoch(), 2);
    controller.invalidate();
    assert_eq!(layout_facts.borrow().invalidations, 1);
    assert_eq!(child_facts.borrow().invalidations, 1);
    assert_eq!(
        host.borrow()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    HostEvent::Schedule(_, _, TuiHostTask::ScheduleRender)
                )
            })
            .count(),
        2,
        "same layout identity is a no-op while set/clear each request a render"
    );
}

#[test]
fn overlay_layout_and_composition_match_every_oracle_row() {
    let default =
        TuiBaseController::resolve_overlay_layout(&OverlayOptions::default(), 10, 100, 30);
    assert_eq!(
        (default.width, default.row, default.col, default.max_height),
        (80, 10, 10, None)
    );
    let percent = TuiBaseController::resolve_overlay_layout(
        &OverlayOptions {
            width: Some(SizeValue::Percent(50.0)),
            max_height: Some(SizeValue::Percent(25.0)),
            row: Some(SizeValue::Percent(100.0)),
            col: Some(SizeValue::Percent(0.0)),
            margin: Some(OverlayMargins::Sides(OverlayMargin {
                top: 2,
                right: 3,
                bottom: 4,
                left: 5,
            })),
            ..OverlayOptions::default()
        },
        20,
        100,
        30,
    );
    assert_eq!(
        (percent.width, percent.row, percent.col, percent.max_height),
        (50, 19, 5, Some(7))
    );
    let clamped = TuiBaseController::resolve_overlay_layout(
        &OverlayOptions {
            width: Some(SizeValue::Absolute(2.0)),
            min_width: Some(200),
            anchor: OverlayAnchor::BottomRight,
            offset_x: 9,
            offset_y: 9,
            margin: Some(OverlayMargins::All(-3)),
            ..OverlayOptions::default()
        },
        5,
        20,
        8,
    );
    assert_eq!((clamped.width, clamped.row, clamped.col), (20, 3, 0));
    let absolute = TuiBaseController::resolve_overlay_layout(
        &OverlayOptions {
            width: Some(SizeValue::Absolute(6.0)),
            row: Some(SizeValue::Absolute(-20.0)),
            col: Some(SizeValue::Absolute(99.0)),
            ..OverlayOptions::default()
        },
        4,
        10,
        6,
    );
    assert_eq!((absolute.width, absolute.row, absolute.col), (6, 0, 4));

    let (controller, _, _) = harness(0, 100, 30, false);
    let (low, _) = probe(&["1111", "2222"]);
    let (high, _) = probe(&["XX"]);
    let low_handle = controller.show_overlay(
        low.as_component_ref(),
        OverlayOptions {
            width: Some(SizeValue::Absolute(4.0)),
            row: Some(SizeValue::Absolute(1.0)),
            col: Some(SizeValue::Absolute(2.0)),
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );
    let high_handle = controller.show_overlay(
        high.as_component_ref(),
        OverlayOptions {
            width: Some(SizeValue::Absolute(2.0)),
            row: Some(SizeValue::Absolute(1.0)),
            col: Some(SizeValue::Absolute(3.0)),
            non_capturing: true,
            ..OverlayOptions::default()
        },
    );
    let case = fixture_case("overlay-layout-and-composition");
    let expected = case["composited"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        controller.composite_overlays(vec!["abcdefghij".into()], 10, 4),
        expected
    );
    high_handle.set_hidden(true);
    let expected = case["withoutHigh"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        controller.composite_overlays(vec!["abcdefghij".into()], 10, 4),
        expected
    );
    low_handle.hide();
    high_handle.hide();

    for (field, visible, hidden) in [("invisibleOnly", false, false), ("hiddenOnly", true, true)] {
        let (controller, _, _) = harness(0, 10, 4, false);
        let (overlay, _) = probe(&[field]);
        let handle = controller.show_overlay(
            overlay.as_component_ref(),
            OverlayOptions {
                visible: Some(Rc::new(move |_, _| visible)),
                non_capturing: true,
                ..OverlayOptions::default()
            },
        );
        if hidden {
            handle.set_hidden(true);
        }
        let expected = case[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            controller.composite_overlays(vec!["x".into()], 10, 4),
            expected
        );
    }
}

#[test]
fn stop_repeats_like_the_oracle_but_drop_cancels_all_owned_work() {
    let (controller, terminal, host) = harness(0, 80, 24, false);
    controller.start();
    run_due(&controller, &host);
    controller.stop(TuiStopOptions::default());
    let after_first = terminal.events();
    assert_eq!(
        after_first,
        [
            TerminalEvent::Start,
            TerminalEvent::HideCursor,
            TerminalEvent::ShowCursor,
            TerminalEvent::Stop,
        ]
    );
    controller.stop(TuiStopOptions::default());
    assert!(
        terminal
            .events()
            .ends_with(&[TerminalEvent::ShowCursor, TerminalEvent::Stop,])
    );

    let (controller, terminal, host) = harness(0, 80, 24, false);
    let (focus, focus_facts) = probe(&["focus"]);
    controller.set_focus(Some(focus.as_component_ref()));
    controller.start();
    controller.request_render(true);
    let background = Rc::new(RefCell::new(Vec::new()));
    let background_copy = Rc::clone(&background);
    controller.query_terminal_background_color(
        50,
        Box::new(move |value| background_copy.borrow_mut().push(value)),
    );
    let scheme = Rc::new(RefCell::new(Vec::new()));
    let scheme_copy = Rc::clone(&scheme);
    controller.query_terminal_color_scheme(
        50,
        Box::new(move |value| scheme_copy.borrow_mut().push(value)),
    );
    let (_, overlay_facts) = probe(&["overlay"]);
    let overlay = ComponentHandle::new(ProbeComponent {
        facts: Rc::clone(&overlay_facts),
        lines: vec!["overlay".into()],
    });
    let handle = controller.show_overlay(overlay.as_component_ref(), OverlayOptions::default());
    assert!(!host.borrow().scheduled.is_empty());
    drop(controller);
    assert!(host.borrow().scheduled.is_empty());
    assert!(host.borrow().events.contains(&HostEvent::Dropped));
    assert!(
        terminal
            .events()
            .ends_with(&[TerminalEvent::ShowCursor, TerminalEvent::Stop,])
    );
    assert!(!focus_facts.borrow().focused);
    assert!(!overlay_facts.borrow().focused);
    assert!(background.borrow().is_empty());
    assert!(scheme.borrow().is_empty());
    handle.focus();
    handle.hide();
    assert!(!handle.is_focused());
}

#[test]
fn drop_restores_pre_start_terminal_side_effects_without_stopping() {
    let (notifications, terminal, host) = harness(0, 80, 24, false);
    notifications.set_terminal_color_scheme_notifications(true);
    drop(notifications);
    assert_eq!(
        terminal.events(),
        [
            TerminalEvent::Write("\x1b[?2031h".into()),
            TerminalEvent::Write("\x1b[?2031l".into()),
        ]
    );
    assert!(!terminal.events().contains(&TerminalEvent::Stop));
    assert!(host.borrow().events.contains(&HostEvent::Dropped));

    let (cursor, terminal, _) = harness_with_cursor(0, 80, 24, false, true);
    cursor.set_show_hardware_cursor(false);
    drop(cursor);
    assert_eq!(
        terminal.events(),
        [TerminalEvent::HideCursor, TerminalEvent::ShowCursor]
    );
    assert!(!terminal.events().contains(&TerminalEvent::Stop));

    let (combined, terminal, host) = harness_with_cursor(0, 80, 24, false, true);
    combined.set_terminal_color_scheme_notifications(true);
    combined.set_show_hardware_cursor(false);
    let listener = combined.add_input_listener(TuiInputListener::new(|_| None));
    drop(combined);
    assert_eq!(
        terminal.events(),
        [
            TerminalEvent::Write("\x1b[?2031h".into()),
            TerminalEvent::HideCursor,
            TerminalEvent::Write("\x1b[?2031l".into()),
            TerminalEvent::ShowCursor,
        ]
    );
    assert!(!terminal.events().contains(&TerminalEvent::Stop));
    assert!(!listener.is_active());
    assert!(host.borrow().scheduled.is_empty());
}

#[test]
fn structural_traits_and_empty_zero_default_edges_are_bounded() {
    fn accept_tui(_: &dyn Tui) {}
    fn accept_viewport(_: &dyn ViewportTui) {}
    let (controller, terminal, host) = harness(0, 0, 0, false);
    accept_tui(&controller);
    accept_viewport(&controller);
    controller.set_layout_root(None);
    assert_eq!(controller.full_redraws(), 0);
    controller.set_full_redraws(3);
    assert_eq!(controller.full_redraws(), 3);
    assert!(!controller.has_overlay());
    assert!(!controller.has_overlay_entries());
    controller.handle_terminal_input("");
    controller.handle_terminal_input("\x1b[6;;t");
    assert!(host.borrow().scheduled.is_empty());
    controller.set_show_hardware_cursor(false);
    assert!(terminal.events().is_empty());
    controller.set_show_hardware_cursor(true);
    controller.set_show_hardware_cursor(true);
    assert_eq!(host.borrow().scheduled.len(), 1);
    let layout = TuiBaseController::resolve_overlay_layout(&OverlayOptions::default(), 0, 0, 0);
    assert_eq!(layout.width, 1);
    assert_eq!(layout.row, 0);
    assert_eq!(layout.col, 0);
    let mut lines = vec![format!("a{}b", pie_core::screen::CURSOR_MARKER)];
    assert_eq!(
        TuiBaseController::extract_cursor_position(&mut lines, 1),
        Some((0, 1))
    );
    assert_eq!(lines, ["ab"]);
    TuiBaseController::apply_line_resets(&mut lines);
    assert_eq!(lines, [format!("ab{}", pie_core::screen::SEGMENT_RESET)]);
}
