//! Shared TuiBase lifecycle and controller state, before Main/Alt rendering.
//!
//! The controller owns terminal registration, focus and overlays. Scheduling
//! is inverted through [`TuiControllerHost`], so tests and runtimes drive the
//! same task facts without sleeps or a live event loop.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::{BTreeMap, VecDeque};
use std::rc::{Rc, Weak};

use pie_components::{
    BackgroundColorQueryCallback, ColorSchemeQueryCallback, ComponentRef, Container,
    ContainerChildId, DebugCallback, OverlayAnchor, OverlayControl, OverlayMargins, OverlayOptions,
    OverlayUnfocus, SubscriptionControl, TerminalColorSchemeListener, Tui, TuiInputListener,
    TuiMode, TuiStopOptions, ViewportTui,
};
use pie_core::keys::{is_key_release, matches_key};
use pie_core::screen::{CURSOR_MARKER, SEGMENT_RESET, composite_tui_line, is_image_line};
use pie_core::terminal_colors::{
    is_osc11_background_color_response, parse_osc11_background_color,
    parse_terminal_color_scheme_report,
};
use pie_core::terminal_image::CellDimensions;
use pie_core::text::visible_width;
use pie_core::wrap::{normalize_terminal_output, slice_by_column};
use pie_term::Terminal;

const MIN_RENDER_INTERVAL_MS: u64 = 16;
const ENABLE_COLOR_SCHEME_NOTIFICATIONS: &str = "\x1b[?2031h";
const DISABLE_COLOR_SCHEME_NOTIFICATIONS: &str = "\x1b[?2031l";
const QUERY_CELL_SIZE: &str = "\x1b[16t";
const QUERY_BACKGROUND_COLOR: &str = "\x1b]11;?\x07";
const QUERY_COLOR_SCHEME: &str = "\x1b[?996n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TuiTaskId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiHostTask {
    DrainTerminalEvents,
    ScheduleRender,
    ImmediateRender,
    RenderTimer,
    BackgroundQueryTimeout {
        query_id: u64,
    },
    ColorSchemeQueryTimeout {
        query_id: u64,
    },
    /// Application-owned alternate-screen flash expiry. Screen controllers
    /// route this before delegating base-controller tasks to [`run_task`].
    AltFlashTimeout {
        flash_id: u64,
    },
}

/// Runtime seam for clock, task, image-cell, and subclass render facts.
pub trait TuiControllerHost {
    fn now_ms(&self) -> u64;
    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId;
    fn cancel_task(&mut self, task: TuiTaskId);
    fn render(&mut self);
    fn reset_render_state(&mut self);
    fn images_supported(&self) -> bool;
    fn set_cell_dimensions(&mut self, dimensions: CellDimensions);
    fn before_terminal_start(&mut self) {}
    fn after_terminal_start(&mut self) {}
    fn before_terminal_stop(&mut self, _options: TuiStopOptions) {}
    fn after_terminal_stop(&mut self, _options: TuiStopOptions) {}
    fn controller_dropped(&mut self) {}
}

/// Inert host for callers that only need structural component ownership.
pub struct DetachedTuiControllerHost {
    now_ms: u64,
    next_task: u64,
    images_supported: bool,
    cell_dimensions: CellDimensions,
}

impl DetachedTuiControllerHost {
    pub fn new(images_supported: bool) -> Self {
        Self {
            now_ms: 0,
            next_task: 0,
            images_supported,
            cell_dimensions: CellDimensions::default(),
        }
    }

    pub fn set_now_ms(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    pub fn cell_dimensions(&self) -> CellDimensions {
        self.cell_dimensions
    }
}

impl Default for DetachedTuiControllerHost {
    fn default() -> Self {
        Self::new(false)
    }
}

impl TuiControllerHost for DetachedTuiControllerHost {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }

    fn schedule_task(&mut self, _delay_ms: u64, _task: TuiHostTask) -> TuiTaskId {
        self.next_task = self.next_task.wrapping_add(1);
        TuiTaskId(self.next_task)
    }

    fn cancel_task(&mut self, _task: TuiTaskId) {}
    fn render(&mut self) {}
    fn reset_render_state(&mut self) {}

    fn images_supported(&self) -> bool {
        self.images_supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        self.cell_dimensions = dimensions;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayLayout {
    pub width: usize,
    pub row: usize,
    pub col: usize,
    pub max_height: Option<usize>,
}

enum TerminalEvent {
    Input(String),
    Resize,
}

#[derive(Clone)]
struct OverlayEntry {
    id: u64,
    component: ComponentRef,
    options: OverlayOptions,
    pre_focus: Option<ComponentRef>,
    hidden: bool,
    focus_order: u64,
}

#[derive(Clone)]
enum OverlayResume {
    RestoreOverlay,
    FocusTarget(Option<ComponentRef>),
}

#[derive(Clone)]
enum OverlayFocusRestore {
    Inactive,
    Eligible {
        overlay_id: u64,
    },
    Blocked {
        overlay_id: u64,
        blocked_by: ComponentRef,
        resume: OverlayResume,
    },
}

struct FocusPlan {
    previous: Option<ComponentRef>,
    next: Option<ComponentRef>,
}

struct FocusFacts {
    previous_focus: Option<ComponentRef>,
    component: Option<ComponentRef>,
    clear_restore: bool,
    previous_overlay: Option<u64>,
    next_is_overlay: bool,
    restore: OverlayFocusRestore,
    blocked_component_mounted: bool,
}

struct BackgroundQuery {
    id: u64,
    settled: bool,
    timer: Option<u64>,
    callback: Option<BackgroundColorQueryCallback>,
}

#[derive(Clone)]
struct InputListenerEntry {
    order: u64,
    listener: TuiInputListener,
}

enum SchemeListenerEntry {
    Persistent {
        order: u64,
        listener: TerminalColorSchemeListener,
    },
    Query {
        order: u64,
        id: u64,
        timer: u64,
        callback: Option<ColorSchemeQueryCallback>,
    },
}

impl SchemeListenerEntry {
    fn order(&self) -> u64 {
        match self {
            Self::Persistent { order, .. } | Self::Query { order, .. } => *order,
        }
    }
}

type SharedDebugCallback = Rc<dyn Fn()>;

enum ControllerAction {
    ScheduleTask {
        token: u64,
        delay_ms: u64,
        task: TuiHostTask,
    },
    CancelTask(TuiTaskId),
    ResetRenderState,
    Render,
    ResolveRenderDelay,
    QueryRuntimeNow(Rc<Cell<Option<u64>>>),
    SetCellDimensionsAndInvalidate(CellDimensions),
    BeforeTerminalStart,
    AfterTerminalStart,
    BeforeTerminalStop(TuiStopOptions),
    AfterTerminalStop(TuiStopOptions),
    QueryImagesSupported,
    ControllerDropped,
    FinalizeTeardown,
    TerminalStart,
    TerminalStop,
    TerminalWrite(String),
    TerminalHideCursor,
    TerminalShowCursor,
}

struct PlannedTask {
    task: TuiHostTask,
    host_id: Option<TuiTaskId>,
}

struct TuiState {
    mode: TuiMode,
    terminal_events: Rc<RefCell<VecDeque<TerminalEvent>>>,
    roots: Container,
    root_components: Rc<RefCell<Vec<(ContainerChildId, ComponentRef)>>>,
    layout_root: Option<ComponentRef>,
    layout_cache_epoch: u64,
    focused_component: Option<ComponentRef>,
    input_listeners: Vec<InputListenerEntry>,
    debug_callback: Option<SharedDebugCallback>,
    terminal_event_task: Option<u64>,
    render_requested: bool,
    immediate_render_scheduled: bool,
    render_in_progress: bool,
    deferred_schedule_render: bool,
    deferred_immediate_render: bool,
    deferred_reset_render_state: bool,
    render_timer: Option<u64>,
    planned_tasks: BTreeMap<u64, PlannedTask>,
    active_tasks: BTreeMap<TuiTaskId, u64>,
    next_task_token: u64,
    actions: VecDeque<ControllerAction>,
    last_render_at: u64,
    last_runtime_now: u64,
    show_hardware_cursor: bool,
    clear_on_shrink: bool,
    full_redraw_count: usize,
    stopped: bool,
    started: bool,
    pending_background_replies: usize,
    background_queries: VecDeque<BackgroundQuery>,
    scheme_listeners: Vec<SchemeListenerEntry>,
    color_scheme_notifications_enabled: bool,
    color_scheme_notifications_active: bool,
    cursor_hidden: bool,
    next_query_id: u64,
    next_listener_order: u64,
    focus_order_counter: u64,
    next_overlay_id: u64,
    overlays: Vec<OverlayEntry>,
    overlay_focus_restore: OverlayFocusRestore,
    tearing_down: bool,
}

impl TuiState {
    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> u64 {
        self.next_task_token = self.next_task_token.wrapping_add(1);
        let token = self.next_task_token;
        self.planned_tasks.insert(
            token,
            PlannedTask {
                task,
                host_id: None,
            },
        );
        self.actions.push_back(ControllerAction::ScheduleTask {
            token,
            delay_ms,
            task,
        });
        token
    }

    fn cancel_task(&mut self, token: u64) {
        if let Some(task) = self.planned_tasks.remove(&token)
            && let Some(host_id) = task.host_id
        {
            self.active_tasks.remove(&host_id);
            self.actions
                .push_back(ControllerAction::CancelTask(host_id));
        }
    }

    fn claim_task(&mut self, host_id: TuiTaskId, task: TuiHostTask) -> Option<u64> {
        let token = *self.active_tasks.get(&host_id)?;
        let planned = self.planned_tasks.get(&token)?;
        if planned.host_id != Some(host_id) || planned.task != task {
            return None;
        }
        self.active_tasks.remove(&host_id);
        self.planned_tasks.remove(&token);
        Some(token)
    }

    fn cancel_render_timer(&mut self) {
        if let Some(timer) = self.render_timer.take() {
            self.cancel_task(timer);
        }
    }

    fn request_terminal_event_drain(&mut self) {
        if self.terminal_event_task.is_some() {
            return;
        }
        let task = self.schedule_task(0, TuiHostTask::DrainTerminalEvents);
        self.terminal_event_task = Some(task);
    }

    fn request_immediate_render(&mut self) {
        self.cancel_render_timer();
        self.render_requested = true;
        if self.render_in_progress {
            self.deferred_immediate_render = true;
            return;
        }
        if self.immediate_render_scheduled {
            return;
        }
        self.immediate_render_scheduled = true;
        self.schedule_task(0, TuiHostTask::ImmediateRender);
    }

    fn request_render(&mut self, force: bool) {
        if self.tearing_down {
            return;
        }
        if force {
            if self.render_in_progress {
                self.deferred_reset_render_state = true;
                self.deferred_immediate_render = true;
                self.render_requested = true;
                return;
            }
            self.actions.push_back(ControllerAction::ResetRenderState);
            self.request_immediate_render();
            return;
        }
        if self.render_requested {
            return;
        }
        self.render_requested = true;
        if self.render_in_progress {
            self.deferred_schedule_render = true;
            return;
        }
        self.schedule_task(0, TuiHostTask::ScheduleRender);
    }

    fn overlay_for_component(&self, component: &ComponentRef) -> Option<&OverlayEntry> {
        self.overlays
            .iter()
            .find(|entry| entry.component.ptr_eq(component))
    }

    fn is_overlay_focus_ancestor(&self, entry_id: u64, component: &ComponentRef) -> bool {
        let mut visited = Vec::new();
        let mut current = self
            .overlays
            .iter()
            .find(|entry| entry.id == entry_id)
            .and_then(|entry| entry.pre_focus.clone());
        while let Some(candidate) = current {
            if visited.contains(&candidate.identity()) {
                break;
            }
            visited.push(candidate.identity());
            if candidate.ptr_eq(component) {
                return true;
            }
            current = self
                .overlay_for_component(&candidate)
                .and_then(|entry| entry.pre_focus.clone());
        }
        false
    }

    fn resolve_blocked_resume(
        &mut self,
        overlay_id: u64,
        resume: OverlayResume,
    ) -> Option<ComponentRef> {
        match resume {
            OverlayResume::RestoreOverlay => self
                .overlays
                .iter()
                .find(|entry| entry.id == overlay_id)
                .map(|entry| entry.component.clone()),
            OverlayResume::FocusTarget(target) => {
                self.overlay_focus_restore = OverlayFocusRestore::Inactive;
                target
            }
        }
    }

    fn plan_focus(&mut self, facts: FocusFacts) -> FocusPlan {
        let FocusFacts {
            previous_focus,
            component,
            clear_restore,
            previous_overlay,
            next_is_overlay,
            restore,
            blocked_component_mounted,
        } = facts;
        let mut next_focus = component;

        if next_focus.is_some() && !next_is_overlay {
            if let OverlayFocusRestore::Blocked {
                overlay_id,
                blocked_by,
                resume,
            } = restore.clone()
            {
                if previous_focus
                    .as_ref()
                    .is_some_and(|focus| focus.ptr_eq(&blocked_by))
                {
                    let resume_is_target = matches!(resume, OverlayResume::FocusTarget(_));
                    if resume_is_target || !blocked_component_mounted {
                        next_focus = self.resolve_blocked_resume(overlay_id, resume);
                    } else if let Some(next) = next_focus.clone() {
                        self.overlay_focus_restore = OverlayFocusRestore::Blocked {
                            overlay_id,
                            blocked_by: next,
                            resume,
                        };
                    }
                }
            } else if let (Some(previous_overlay), Some(next)) =
                (previous_overlay, next_focus.as_ref())
                && !matches!(restore, OverlayFocusRestore::Inactive)
                && !self.is_overlay_focus_ancestor(previous_overlay, next)
            {
                self.overlay_focus_restore = OverlayFocusRestore::Blocked {
                    overlay_id: previous_overlay,
                    blocked_by: next.clone(),
                    resume: OverlayResume::RestoreOverlay,
                };
            }
        } else if next_focus.is_none() {
            if let OverlayFocusRestore::Blocked {
                overlay_id,
                blocked_by,
                resume,
            } = restore
                && previous_focus
                    .as_ref()
                    .is_some_and(|focus| focus.ptr_eq(&blocked_by))
            {
                next_focus = self.resolve_blocked_resume(overlay_id, resume);
            } else if clear_restore {
                self.overlay_focus_restore = OverlayFocusRestore::Inactive;
            }
        }

        FocusPlan {
            previous: previous_focus,
            next: next_focus,
        }
    }

    fn clear_focus_restore_for(&mut self, overlay_id: u64) {
        let matches = match self.overlay_focus_restore {
            OverlayFocusRestore::Inactive => false,
            OverlayFocusRestore::Eligible {
                overlay_id: current,
            }
            | OverlayFocusRestore::Blocked {
                overlay_id: current,
                ..
            } => current == overlay_id,
        };
        if matches {
            self.overlay_focus_restore = OverlayFocusRestore::Inactive;
        }
    }

    fn retarget_pre_focus(&mut self, removed: &OverlayEntry) {
        for overlay in &mut self.overlays {
            if overlay.id != removed.id
                && overlay
                    .pre_focus
                    .as_ref()
                    .is_some_and(|focus| focus.ptr_eq(&removed.component))
            {
                overlay.pre_focus.clone_from(&removed.pre_focus);
            }
        }
    }

    fn cancel_all_tasks(&mut self) {
        let tasks = self.planned_tasks.keys().copied().collect::<Vec<_>>();
        for task in tasks {
            self.cancel_task(task);
        }
        self.render_timer = None;
        self.terminal_event_task = None;
        self.immediate_render_scheduled = false;
        for query in &mut self.background_queries {
            query.timer = None;
            query.callback = None;
            query.settled = true;
        }
        for listener in &mut self.scheme_listeners {
            if let SchemeListenerEntry::Query { callback, .. } = listener {
                *callback = None;
            }
        }
    }

    fn plan_teardown(&mut self, notify_host: bool) -> Option<ComponentRef> {
        self.cancel_all_tasks();
        self.stopped = true;
        let focused = self.focused_component.take();
        self.input_listeners.clear();
        self.scheme_listeners.clear();
        self.background_queries.clear();
        self.overlays.clear();
        self.debug_callback = None;
        self.terminal_events.borrow_mut().clear();
        if self.color_scheme_notifications_active {
            self.actions.push_back(ControllerAction::TerminalWrite(
                DISABLE_COLOR_SCHEME_NOTIFICATIONS.into(),
            ));
            self.color_scheme_notifications_active = false;
        }
        if self.cursor_hidden {
            self.actions.push_back(ControllerAction::TerminalShowCursor);
            self.cursor_hidden = false;
        }
        if self.started {
            self.actions.push_back(ControllerAction::TerminalStop);
        }
        if notify_host {
            self.actions.push_back(ControllerAction::ControllerDropped);
        }
        focused
    }
}

struct TuiShared {
    state: RefCell<TuiState>,
    host: RefCell<Box<dyn TuiControllerHost>>,
    terminal: RefCell<Box<dyn Terminal>>,
    driving_actions: Cell<bool>,
}

impl TuiShared {
    fn borrow(&self) -> Ref<'_, TuiState> {
        self.state.borrow()
    }

    fn borrow_mut(&self) -> RefMut<'_, TuiState> {
        self.state.borrow_mut()
    }

    fn terminal_columns(&self) -> usize {
        self.terminal.borrow().columns()
    }

    fn terminal_rows(&self) -> usize {
        self.terminal.borrow().rows()
    }
}

/// Shared TuiBase controller. Main- and alternate-screen renderers plug into
/// its host later; this type deliberately contains no screen-specific planner.
pub struct TuiBaseController {
    inner: Rc<TuiShared>,
}

/// Cycle-free handle used by application-owned screen hosts. Keeping the
/// host's backlink weak is important: `TuiBaseController` owns its host and
/// must remain the sole strong owner which initiates terminal teardown.
#[derive(Clone)]
pub(crate) struct WeakTuiBaseController {
    inner: Weak<TuiShared>,
}

impl WeakTuiBaseController {
    pub(crate) fn upgrade(&self) -> Option<TuiBaseController> {
        self.inner
            .upgrade()
            .map(|inner| TuiBaseController { inner })
    }
}

impl TuiBaseController {
    pub fn new(
        terminal: Box<dyn Terminal>,
        host: Box<dyn TuiControllerHost>,
        mode: TuiMode,
        show_hardware_cursor: bool,
    ) -> Self {
        let terminal_events = Rc::new(RefCell::new(VecDeque::new()));
        Self {
            inner: Rc::new(TuiShared {
                state: RefCell::new(TuiState {
                    mode,
                    terminal_events,
                    roots: Container::new(),
                    root_components: Rc::new(RefCell::new(Vec::new())),
                    layout_root: None,
                    layout_cache_epoch: 0,
                    focused_component: None,
                    input_listeners: Vec::new(),
                    debug_callback: None,
                    terminal_event_task: None,
                    render_requested: false,
                    immediate_render_scheduled: false,
                    render_in_progress: false,
                    deferred_schedule_render: false,
                    deferred_immediate_render: false,
                    deferred_reset_render_state: false,
                    render_timer: None,
                    planned_tasks: BTreeMap::new(),
                    active_tasks: BTreeMap::new(),
                    next_task_token: 0,
                    actions: VecDeque::new(),
                    last_render_at: 0,
                    last_runtime_now: 0,
                    show_hardware_cursor,
                    clear_on_shrink: false,
                    full_redraw_count: 0,
                    stopped: false,
                    started: false,
                    pending_background_replies: 0,
                    background_queries: VecDeque::new(),
                    scheme_listeners: Vec::new(),
                    color_scheme_notifications_enabled: false,
                    color_scheme_notifications_active: false,
                    cursor_hidden: false,
                    next_query_id: 0,
                    next_listener_order: 0,
                    focus_order_counter: 0,
                    next_overlay_id: 0,
                    overlays: Vec::new(),
                    overlay_focus_restore: OverlayFocusRestore::Inactive,
                    tearing_down: false,
                }),
                host: RefCell::new(host),
                terminal: RefCell::new(terminal),
                driving_actions: Cell::new(false),
            }),
        }
    }

    pub(crate) fn downgrade(&self) -> WeakTuiBaseController {
        WeakTuiBaseController {
            inner: Rc::downgrade(&self.inner),
        }
    }

    /// Render ordinary document roots with JavaScript Array-iterator
    /// mutation semantics: retain the backing list, clone one entry at a
    /// time, and hold no controller borrow while invoking user code.
    pub(crate) fn render_document(&self, width: usize) -> Vec<String> {
        let roots = Rc::clone(&self.inner.borrow().root_components);
        let mut lines = Vec::new();
        let mut index = 0;
        loop {
            let component = roots
                .borrow()
                .get(index)
                .map(|(_, component)| component.clone());
            let Some(component) = component else {
                break;
            };
            lines.extend(component.render(width));
            index += 1;
        }
        lines
    }

    pub(crate) fn layout_root(&self) -> Option<ComponentRef> {
        self.inner.borrow().layout_root.clone()
    }

    /// Plan an application-owned screen task without crossing the host seam.
    /// The screen records the returned logical token before flushing so a
    /// synchronous host callback can safely cancel the still-pending task.
    pub(crate) fn plan_screen_task(&self, delay_ms: u64, task: TuiHostTask) -> u64 {
        self.inner.borrow_mut().schedule_task(delay_ms, task)
    }

    pub(crate) fn flush_screen_actions(&self) {
        self.flush_actions();
    }

    pub(crate) fn cancel_screen_tasks(&self, tokens: &[u64]) {
        {
            let mut state = self.inner.borrow_mut();
            for token in tokens {
                state.cancel_task(*token);
            }
        }
        self.flush_actions();
    }

    pub(crate) fn claim_screen_task(&self, id: TuiTaskId, task: TuiHostTask) -> Option<u64> {
        self.inner.borrow_mut().claim_task(id, task)
    }

    pub(crate) fn runtime_now_ms(&self) -> u64 {
        let result = Rc::new(Cell::new(None));
        self.inner
            .borrow_mut()
            .actions
            .push_back(ControllerAction::QueryRuntimeNow(Rc::clone(&result)));
        self.flush_actions();
        result
            .get()
            .unwrap_or_else(|| self.inner.borrow().last_runtime_now)
    }

    pub(crate) fn write_terminal(&self, data: impl Into<String>) {
        self.inner
            .borrow_mut()
            .actions
            .push_back(ControllerAction::TerminalWrite(data.into()));
        self.flush_actions();
    }

    fn wake_terminal_events(shared: &Rc<TuiShared>) {
        {
            let mut state = shared.borrow_mut();
            if state.tearing_down {
                return;
            }
            state.request_terminal_event_drain();
        }
        Self::flush_shared_actions(shared);
    }

    fn flush_actions(&self) {
        Self::flush_shared_actions(&self.inner);
    }

    fn flush_shared_actions(shared: &Rc<TuiShared>) {
        if shared.driving_actions.replace(true) {
            return;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                let action = shared.borrow_mut().actions.pop_front();
                let Some(action) = action else {
                    break;
                };
                match action {
                    ControllerAction::ScheduleTask {
                        token,
                        delay_ms,
                        task,
                    } => {
                        let host_id = shared.host.borrow_mut().schedule_task(delay_ms, task);
                        let mut state = shared.borrow_mut();
                        if let Some(planned) = state.planned_tasks.get_mut(&token)
                            && planned.task == task
                            && planned.host_id.is_none()
                        {
                            planned.host_id = Some(host_id);
                            state.active_tasks.insert(host_id, token);
                        } else {
                            state
                                .actions
                                .push_front(ControllerAction::CancelTask(host_id));
                        }
                    }
                    ControllerAction::CancelTask(task) => {
                        shared.host.borrow_mut().cancel_task(task);
                    }
                    ControllerAction::ResetRenderState => {
                        shared.host.borrow_mut().reset_render_state();
                    }
                    ControllerAction::Render => {
                        let now = shared.host.borrow().now_ms();
                        {
                            let mut state = shared.borrow_mut();
                            state.last_render_at = now;
                            state.last_runtime_now = now;
                        }
                        let render_result =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                shared.host.borrow_mut().render();
                            }));
                        {
                            let mut state = shared.borrow_mut();
                            state.render_in_progress = false;
                            if state.deferred_reset_render_state {
                                state.deferred_reset_render_state = false;
                                state.actions.push_back(ControllerAction::ResetRenderState);
                            }
                            if state.deferred_immediate_render {
                                state.deferred_immediate_render = false;
                                state.deferred_schedule_render = false;
                                state.request_immediate_render();
                            } else if state.deferred_schedule_render {
                                state.deferred_schedule_render = false;
                                if state.render_requested {
                                    state.schedule_task(0, TuiHostTask::ScheduleRender);
                                }
                            }
                        }
                        if let Err(payload) = render_result {
                            std::panic::resume_unwind(payload);
                        }
                    }
                    ControllerAction::ResolveRenderDelay => {
                        let now = shared.host.borrow().now_ms();
                        let mut state = shared.borrow_mut();
                        state.last_runtime_now = now;
                        if !state.stopped && state.render_timer.is_none() && state.render_requested
                        {
                            let elapsed = now.saturating_sub(state.last_render_at);
                            let delay = MIN_RENDER_INTERVAL_MS.saturating_sub(elapsed);
                            let timer = state.schedule_task(delay, TuiHostTask::RenderTimer);
                            state.render_timer = Some(timer);
                        }
                    }
                    ControllerAction::QueryRuntimeNow(result) => {
                        let now = shared.host.borrow().now_ms();
                        shared.borrow_mut().last_runtime_now = now;
                        result.set(Some(now));
                    }
                    ControllerAction::SetCellDimensionsAndInvalidate(dimensions) => {
                        shared.host.borrow_mut().set_cell_dimensions(dimensions);
                        let controller = TuiBaseController {
                            inner: Rc::clone(shared),
                        };
                        controller.invalidate();
                        controller.request_render(false);
                    }
                    ControllerAction::BeforeTerminalStart => {
                        shared.host.borrow_mut().before_terminal_start();
                    }
                    ControllerAction::AfterTerminalStart => {
                        shared.host.borrow_mut().after_terminal_start();
                    }
                    ControllerAction::BeforeTerminalStop(options) => {
                        shared.host.borrow_mut().before_terminal_stop(options);
                        let mut state = shared.borrow_mut();
                        state
                            .actions
                            .push_back(ControllerAction::TerminalShowCursor);
                        state.cursor_hidden = false;
                        state.actions.push_back(ControllerAction::TerminalStop);
                        state
                            .actions
                            .push_back(ControllerAction::AfterTerminalStop(options));
                    }
                    ControllerAction::AfterTerminalStop(options) => {
                        shared.host.borrow_mut().after_terminal_stop(options);
                    }
                    ControllerAction::QueryImagesSupported => {
                        let supported = shared.host.borrow().images_supported();
                        if supported && shared.borrow().started {
                            shared.borrow_mut().actions.push_front(
                                ControllerAction::TerminalWrite(QUERY_CELL_SIZE.into()),
                            );
                        }
                    }
                    ControllerAction::ControllerDropped => {
                        shared.host.borrow_mut().controller_dropped();
                        shared
                            .borrow_mut()
                            .actions
                            .push_back(ControllerAction::FinalizeTeardown);
                    }
                    ControllerAction::FinalizeTeardown => {
                        let mut state = shared.borrow_mut();
                        state.tearing_down = true;
                        state.plan_teardown(false);
                    }
                    ControllerAction::TerminalStart => {
                        let input_events = shared.borrow().terminal_events.clone();
                        let resize_events = Rc::clone(&input_events);
                        let input_shared = Rc::downgrade(shared);
                        let resize_shared = Rc::downgrade(shared);
                        shared.terminal.borrow_mut().start(
                            Box::new(move |data| {
                                input_events
                                    .borrow_mut()
                                    .push_back(TerminalEvent::Input(data.to_owned()));
                                if let Some(shared) = input_shared.upgrade() {
                                    TuiBaseController::wake_terminal_events(&shared);
                                }
                            }),
                            Box::new(move || {
                                resize_events.borrow_mut().push_back(TerminalEvent::Resize);
                                if let Some(shared) = resize_shared.upgrade() {
                                    TuiBaseController::wake_terminal_events(&shared);
                                }
                            }),
                        );
                        shared.borrow_mut().started = true;
                    }
                    ControllerAction::TerminalStop => {
                        shared.terminal.borrow_mut().stop();
                        shared.borrow_mut().started = false;
                    }
                    ControllerAction::TerminalWrite(data) => {
                        shared.terminal.borrow_mut().write(&data);
                    }
                    ControllerAction::TerminalHideCursor => {
                        shared.terminal.borrow_mut().hide_cursor();
                    }
                    ControllerAction::TerminalShowCursor => {
                        shared.terminal.borrow_mut().show_cursor();
                    }
                }
            }
        }));
        shared.driving_actions.set(false);
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    fn overlay_entry(&self, overlay_id: u64) -> Option<OverlayEntry> {
        self.inner
            .borrow()
            .overlays
            .iter()
            .find(|entry| entry.id == overlay_id)
            .cloned()
    }

    fn overlay_for_component_entry(&self, component: &ComponentRef) -> Option<OverlayEntry> {
        self.inner
            .borrow()
            .overlays
            .iter()
            .find(|entry| entry.component.ptr_eq(component))
            .cloned()
    }

    fn overlay_entry_is_visible(&self, entry: &OverlayEntry) -> bool {
        if entry.hidden {
            return false;
        }
        let Some(predicate) = entry.options.visible.clone() else {
            return true;
        };
        let width = self.inner.terminal_columns();
        let height = self.inner.terminal_rows();
        predicate(width, height)
    }

    fn overlay_is_visible(&self, overlay_id: u64) -> bool {
        self.overlay_entry(overlay_id)
            .as_ref()
            .is_some_and(|entry| self.overlay_entry_is_visible(entry))
    }

    fn visible_overlay_for_component(&self, component: &ComponentRef) -> Option<OverlayEntry> {
        self.overlay_for_component_entry(component)
            .filter(|entry| self.overlay_entry_is_visible(entry))
    }

    fn visible_focus_restore(&self) -> OverlayFocusRestore {
        let restore = self.inner.borrow().overlay_focus_restore.clone();
        let overlay_id = match &restore {
            OverlayFocusRestore::Inactive => return restore,
            OverlayFocusRestore::Eligible { overlay_id }
            | OverlayFocusRestore::Blocked { overlay_id, .. } => *overlay_id,
        };
        if self.overlay_is_visible(overlay_id) {
            restore
        } else {
            OverlayFocusRestore::Inactive
        }
    }

    fn topmost_visible_overlay(&self) -> Option<OverlayEntry> {
        let mut index = 0;
        let mut topmost: Option<OverlayEntry> = None;
        loop {
            let entry = self.inner.borrow().overlays.get(index).cloned();
            let Some(entry) = entry else {
                break;
            };
            index += 1;
            if entry.options.non_capturing || !self.overlay_entry_is_visible(&entry) {
                continue;
            }
            if topmost
                .as_ref()
                .is_none_or(|current| entry.focus_order > current.focus_order)
            {
                topmost = Some(entry);
            }
        }
        topmost
    }

    fn mounted_roots(&self) -> Vec<ComponentRef> {
        let state = self.inner.borrow();
        state.layout_root.clone().map_or_else(
            || {
                state
                    .root_components
                    .borrow()
                    .iter()
                    .map(|(_, component)| component.clone())
                    .collect()
            },
            |root| vec![root],
        )
    }

    fn component_is_mounted(&self, component: &ComponentRef) -> bool {
        self.mounted_roots()
            .into_iter()
            .any(|root| root.contains_component_ref(component))
    }

    fn set_focus_internal(&self, component: Option<ComponentRef>, clear_restore: bool) {
        let previous_focus = self.inner.borrow().focused_component.clone();
        let previous_overlay = previous_focus
            .as_ref()
            .and_then(|focus| self.visible_overlay_for_component(focus))
            .map(|entry| entry.id);
        let next_is_overlay = component
            .as_ref()
            .is_some_and(|next| self.overlay_for_component_entry(next).is_some());
        let restore = self.visible_focus_restore();
        let blocked_component_mounted = match (&component, next_is_overlay, &restore) {
            (Some(_), false, OverlayFocusRestore::Blocked { blocked_by, .. })
                if previous_focus
                    .as_ref()
                    .is_some_and(|focus| focus.ptr_eq(blocked_by)) =>
            {
                self.component_is_mounted(blocked_by)
            }
            _ => false,
        };
        let plan = self.inner.borrow_mut().plan_focus(FocusFacts {
            previous_focus,
            component,
            clear_restore,
            previous_overlay,
            next_is_overlay,
            restore,
            blocked_component_mounted,
        });

        if let Some(previous) = &plan.previous {
            previous.set_focused(false);
        }
        self.inner.borrow_mut().focused_component = plan.next.clone();
        if let Some(next) = &plan.next {
            next.set_focused(true);
        }

        let overlay_id = plan
            .next
            .as_ref()
            .and_then(|focus| self.visible_overlay_for_component(focus))
            .map(|entry| entry.id);
        if let Some(overlay_id) = overlay_id {
            self.inner.borrow_mut().overlay_focus_restore =
                OverlayFocusRestore::Eligible { overlay_id };
        }
    }

    fn perform_render(&self, force: bool) {
        {
            let mut state = self.inner.borrow_mut();
            if state.render_in_progress {
                state.request_render(force);
                drop(state);
                self.flush_actions();
                return;
            }
            if force {
                state.actions.push_back(ControllerAction::ResetRenderState);
            }
            state.render_requested = false;
            state.cancel_render_timer();
            state.render_in_progress = true;
            state.actions.push_back(ControllerAction::Render);
        }
        self.flush_actions();
    }

    pub fn mode(&self) -> TuiMode {
        self.inner.borrow().mode
    }

    pub fn terminal_columns(&self) -> usize {
        self.inner.terminal_columns()
    }

    pub fn terminal_rows(&self) -> usize {
        self.inner.terminal_rows()
    }

    pub fn full_redraws(&self) -> usize {
        self.inner.borrow().full_redraw_count
    }

    pub fn set_full_redraws(&self, count: usize) {
        self.inner.borrow_mut().full_redraw_count = count;
    }

    pub fn add_child(&self, component: ComponentRef) -> ContainerChildId {
        let mut state = self.inner.borrow_mut();
        let child = state.roots.add_component_ref(component.clone());
        state.root_components.borrow_mut().push((child, component));
        child
    }

    pub fn remove_child(&self, child: ContainerChildId) -> bool {
        let mut state = self.inner.borrow_mut();
        if !state.roots.remove_child(child) {
            return false;
        }
        state
            .root_components
            .borrow_mut()
            .retain(|(id, _)| *id != child);
        true
    }

    pub fn clear(&self) {
        let mut state = self.inner.borrow_mut();
        state.roots.clear();
        state.root_components = Rc::new(RefCell::new(Vec::new()));
    }

    pub fn show_hardware_cursor(&self) -> bool {
        self.inner.borrow().show_hardware_cursor
    }

    pub fn set_show_hardware_cursor(&self, enabled: bool) {
        {
            let mut state = self.inner.borrow_mut();
            if state.show_hardware_cursor == enabled {
                return;
            }
            state.show_hardware_cursor = enabled;
            if !enabled {
                state
                    .actions
                    .push_back(ControllerAction::TerminalHideCursor);
                state.cursor_hidden = true;
            }
            state.request_render(false);
        }
        self.flush_actions();
    }

    pub fn clear_on_shrink(&self) -> bool {
        self.inner.borrow().clear_on_shrink
    }

    pub fn set_clear_on_shrink(&self, enabled: bool) {
        self.inner.borrow_mut().clear_on_shrink = enabled;
    }

    pub fn focused_component(&self) -> Option<ComponentRef> {
        self.inner.borrow().focused_component.clone()
    }

    pub fn set_focus(&self, component: Option<ComponentRef>) {
        self.set_focus_internal(component, true);
    }

    pub fn has_overlay_entries(&self) -> bool {
        !self.inner.borrow().overlays.is_empty()
    }

    pub fn show_overlay(&self, component: ComponentRef, options: OverlayOptions) -> OverlayHandle {
        let (id, non_capturing) = {
            let mut state = self.inner.borrow_mut();
            state.next_overlay_id = state.next_overlay_id.wrapping_add(1);
            state.focus_order_counter = state.focus_order_counter.wrapping_add(1);
            let entry = OverlayEntry {
                id: state.next_overlay_id,
                component: component.clone(),
                options,
                pre_focus: state.focused_component.clone(),
                hidden: false,
                focus_order: state.focus_order_counter,
            };
            let facts = (entry.id, entry.options.non_capturing);
            state.overlays.push(entry);
            facts
        };
        let should_focus = !non_capturing && self.overlay_is_visible(id);
        if should_focus {
            self.set_focus_internal(Some(component), true);
        }
        {
            let mut state = self.inner.borrow_mut();
            state
                .actions
                .push_back(ControllerAction::TerminalHideCursor);
            state.cursor_hidden = true;
            state.request_render(false);
        }
        self.flush_actions();
        OverlayHandle {
            state: Rc::downgrade(&self.inner),
            overlay_id: id,
        }
    }

    pub fn hide_overlay(&self) {
        let (overlay, restore_focus) = {
            let mut state = self.inner.borrow_mut();
            let Some(overlay) = state.overlays.pop() else {
                return;
            };
            state.clear_focus_restore_for(overlay.id);
            state.retarget_pre_focus(&overlay);
            let restore_focus = state
                .focused_component
                .as_ref()
                .is_some_and(|focus| focus.ptr_eq(&overlay.component));
            (overlay, restore_focus)
        };
        if restore_focus {
            let target = self
                .topmost_visible_overlay()
                .map(|entry| entry.component)
                .or(overlay.pre_focus);
            self.set_focus_internal(target, true);
        }
        {
            let mut state = self.inner.borrow_mut();
            if state.overlays.is_empty() {
                state
                    .actions
                    .push_back(ControllerAction::TerminalHideCursor);
                state.cursor_hidden = true;
            }
            state.request_render(false);
        }
        self.flush_actions();
    }

    pub fn has_overlay(&self) -> bool {
        let length = self.inner.borrow().overlays.len();
        for index in 0..length {
            let entry = self.inner.borrow().overlays.get(index).cloned();
            if entry
                .as_ref()
                .is_some_and(|entry| self.overlay_entry_is_visible(entry))
            {
                return true;
            }
        }
        false
    }

    pub fn invalidate(&self) {
        let layout_root = self.inner.borrow().layout_root.clone();
        if let Some(layout_root) = layout_root {
            layout_root.invalidate();
        } else {
            // A JavaScript Array iterator re-reads the element at its current
            // index after every callback. Retain only that component identity,
            // and release TuiState before invalidating it.
            let root_components = Rc::clone(&self.inner.borrow().root_components);
            let mut index = 0;
            loop {
                let component = root_components
                    .borrow()
                    .get(index)
                    .map(|(_, component)| component.clone());
                let Some(component) = component else {
                    break;
                };
                component.invalidate();
                index += 1;
            }
        }

        let mut index = 0;
        loop {
            let component = {
                self.inner
                    .borrow()
                    .overlays
                    .get(index)
                    .map(|entry| entry.component.clone())
            };
            let Some(component) = component else {
                break;
            };
            component.invalidate();
            index += 1;
        }
    }

    pub fn set_layout_root(&self, component: Option<ComponentRef>) {
        {
            let mut state = self.inner.borrow_mut();
            let unchanged = match (&state.layout_root, &component) {
                (Some(current), Some(next)) => current.ptr_eq(next),
                (None, None) => true,
                _ => false,
            };
            if unchanged {
                return;
            }
            state.layout_root = component;
            state.layout_cache_epoch = state.layout_cache_epoch.wrapping_add(1);
            state.request_render(false);
        }
        self.flush_actions();
    }

    /// Monotonic invalidation token for renderer-owned layout caches.
    pub fn layout_cache_epoch(&self) -> u64 {
        self.inner.borrow().layout_cache_epoch
    }

    pub fn start(&self) {
        {
            let mut state = self.inner.borrow_mut();
            state.stopped = false;
            state
                .actions
                .push_back(ControllerAction::BeforeTerminalStart);
            state.actions.push_back(ControllerAction::TerminalStart);
            state
                .actions
                .push_back(ControllerAction::AfterTerminalStart);
            state
                .actions
                .push_back(ControllerAction::TerminalHideCursor);
            state.cursor_hidden = true;
            if state.color_scheme_notifications_enabled {
                state.actions.push_back(ControllerAction::TerminalWrite(
                    ENABLE_COLOR_SCHEME_NOTIFICATIONS.into(),
                ));
                state.color_scheme_notifications_active = true;
            }
            state
                .actions
                .push_back(ControllerAction::QueryImagesSupported);
            state.request_render(false);
        }
        self.flush_actions();
        let terminal_events_pending = !self.inner.borrow().terminal_events.borrow().is_empty();
        if terminal_events_pending {
            {
                self.inner.borrow_mut().request_terminal_event_drain();
            }
            self.flush_actions();
        }
    }

    pub fn stop(&self, options: TuiStopOptions) {
        {
            let mut state = self.inner.borrow_mut();
            state.stopped = true;
            state.cancel_render_timer();
            if state.color_scheme_notifications_enabled {
                state.actions.push_back(ControllerAction::TerminalWrite(
                    DISABLE_COLOR_SCHEME_NOTIFICATIONS.into(),
                ));
                state.color_scheme_notifications_active = false;
            }
            state
                .actions
                .push_back(ControllerAction::BeforeTerminalStop(options));
        }
        self.flush_actions();
    }

    pub fn render_now(&self, force: bool) {
        self.perform_render(force);
    }

    pub fn request_render(&self, force: bool) {
        self.inner.borrow_mut().request_render(force);
        self.flush_actions();
    }

    pub fn terminal_events_pending(&self) -> usize {
        self.inner.borrow().terminal_events.borrow().len()
    }

    pub fn drain_terminal_events(&self) {
        {
            let mut state = self.inner.borrow_mut();
            if let Some(task) = state.terminal_event_task.take() {
                state.cancel_task(task);
            }
        }
        self.flush_actions();
        loop {
            let event = {
                let state = self.inner.borrow();
                state.terminal_events.borrow_mut().pop_front()
            };
            match event {
                Some(TerminalEvent::Input(data)) => self.handle_terminal_input(&data),
                Some(TerminalEvent::Resize) => self.request_render(false),
                None => break,
            }
        }
    }

    pub fn add_input_listener(&self, listener: TuiInputListener) -> TuiSubscription {
        let mut state = self.inner.borrow_mut();
        if !state
            .input_listeners
            .iter()
            .any(|entry| entry.listener.identity() == listener.identity())
        {
            state.next_listener_order = state.next_listener_order.wrapping_add(1);
            let order = state.next_listener_order;
            state.input_listeners.push(InputListenerEntry {
                order,
                listener: listener.clone(),
            });
        }
        TuiSubscription {
            state: Rc::downgrade(&self.inner),
            kind: SubscriptionKind::Input(listener.identity()),
        }
    }

    pub fn remove_input_listener(&self, listener: &TuiInputListener) {
        self.inner
            .borrow_mut()
            .input_listeners
            .retain(|entry| entry.listener.identity() != listener.identity());
    }

    pub fn on_terminal_color_scheme_change(
        &self,
        listener: TerminalColorSchemeListener,
    ) -> TuiSubscription {
        let mut state = self.inner.borrow_mut();
        if !state.scheme_listeners.iter().any(|entry| {
            matches!(entry, SchemeListenerEntry::Persistent { listener: current, .. } if current.identity() == listener.identity())
        }) {
            state.next_listener_order = state.next_listener_order.wrapping_add(1);
            let order = state.next_listener_order;
            state.scheme_listeners.push(SchemeListenerEntry::Persistent {
                order,
                listener: listener.clone(),
            });
        }
        TuiSubscription {
            state: Rc::downgrade(&self.inner),
            kind: SubscriptionKind::Scheme(listener.identity()),
        }
    }

    pub fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        {
            let mut state = self.inner.borrow_mut();
            if state.color_scheme_notifications_enabled == enabled {
                return;
            }
            state.color_scheme_notifications_enabled = enabled;
            if !state.stopped {
                state.actions.push_back(ControllerAction::TerminalWrite(
                    if enabled {
                        ENABLE_COLOR_SCHEME_NOTIFICATIONS
                    } else {
                        DISABLE_COLOR_SCHEME_NOTIFICATIONS
                    }
                    .into(),
                ));
                state.color_scheme_notifications_active = enabled;
            }
        }
        self.flush_actions();
    }

    pub fn query_terminal_background_color(
        &self,
        timeout_ms: u64,
        callback: BackgroundColorQueryCallback,
    ) -> u64 {
        let id = {
            let mut state = self.inner.borrow_mut();
            state.next_query_id = state.next_query_id.wrapping_add(1);
            let id = state.next_query_id;
            let timer = state.schedule_task(
                timeout_ms,
                TuiHostTask::BackgroundQueryTimeout { query_id: id },
            );
            state.background_queries.push_back(BackgroundQuery {
                id,
                settled: false,
                timer: Some(timer),
                callback: Some(callback),
            });
            state.pending_background_replies += 1;
            state.actions.push_back(ControllerAction::TerminalWrite(
                QUERY_BACKGROUND_COLOR.into(),
            ));
            id
        };
        self.flush_actions();
        id
    }

    pub fn query_terminal_color_scheme(
        &self,
        timeout_ms: u64,
        callback: ColorSchemeQueryCallback,
    ) -> u64 {
        let id = {
            let mut state = self.inner.borrow_mut();
            state.next_query_id = state.next_query_id.wrapping_add(1);
            let id = state.next_query_id;
            let timer = state.schedule_task(
                timeout_ms,
                TuiHostTask::ColorSchemeQueryTimeout { query_id: id },
            );
            state.next_listener_order = state.next_listener_order.wrapping_add(1);
            let order = state.next_listener_order;
            state.scheme_listeners.push(SchemeListenerEntry::Query {
                order,
                id,
                timer,
                callback: Some(callback),
            });
            state
                .actions
                .push_back(ControllerAction::TerminalWrite(QUERY_COLOR_SCHEME.into()));
            id
        };
        self.flush_actions();
        id
    }

    pub fn set_debug_callback(&self, callback: Option<DebugCallback>) {
        self.inner.borrow_mut().debug_callback = callback.map(Rc::<dyn Fn()>::from);
    }

    pub fn handle_terminal_input(&self, original: &str) {
        if self.consume_background_response(original) {
            return;
        }
        if self.consume_color_scheme_report(original) {
            return;
        }

        let mut data = original.to_owned();
        let mut last_order = 0;
        loop {
            let next = {
                let state = self.inner.borrow();
                state
                    .input_listeners
                    .iter()
                    .filter(|entry| entry.order > last_order)
                    .min_by_key(|entry| entry.order)
                    .cloned()
            };
            let Some(entry) = next else {
                break;
            };
            last_order = entry.order;
            let listener = entry.listener;
            if let Some(result) = listener.invoke(&data) {
                if result.consume {
                    return;
                }
                if let Some(transformed) = result.data {
                    data = transformed;
                }
            }
        }
        if data.is_empty() {
            return;
        }
        if self.consume_cell_size_response(&data) {
            return;
        }

        if matches_key(&data, "shift+ctrl+d") {
            let callback = self.inner.borrow().debug_callback.clone();
            if let Some(debug) = callback {
                debug();
            } else {
                return self.forward_focused_input(&data);
            }
            return;
        }
        self.forward_focused_input(&data);
    }

    fn forward_focused_input(&self, data: &str) {
        let focused_overlay = self
            .inner
            .borrow()
            .focused_component
            .as_ref()
            .and_then(|focused| self.overlay_for_component_entry(focused));
        if let Some(overlay) = focused_overlay
            && !self.overlay_entry_is_visible(&overlay)
        {
            if let Some(topmost) = self.topmost_visible_overlay() {
                self.set_focus_internal(Some(topmost.component), true);
            } else {
                self.set_focus_internal(overlay.pre_focus, false);
            }
        }

        let focus_is_overlay = {
            let state = self.inner.borrow();
            state.focused_component.as_ref().is_some_and(|focused| {
                state
                    .overlays
                    .iter()
                    .any(|entry| entry.component.ptr_eq(focused))
            })
        };
        let restore = if focus_is_overlay {
            OverlayFocusRestore::Inactive
        } else {
            self.visible_focus_restore()
        };
        let restore_target = {
            let mut state = self.inner.borrow_mut();
            if focus_is_overlay {
                None
            } else {
                match restore {
                    OverlayFocusRestore::Eligible { overlay_id } => state
                        .overlays
                        .iter()
                        .find(|entry| entry.id == overlay_id)
                        .map(|entry| entry.component.clone())
                        .map(Some),
                    OverlayFocusRestore::Blocked {
                        overlay_id,
                        blocked_by,
                        resume,
                    } if state
                        .focused_component
                        .as_ref()
                        .is_none_or(|focused| !focused.ptr_eq(&blocked_by)) =>
                    {
                        let target = match resume {
                            OverlayResume::RestoreOverlay => state
                                .overlays
                                .iter()
                                .find(|entry| entry.id == overlay_id)
                                .map(|entry| entry.component.clone()),
                            OverlayResume::FocusTarget(target) => {
                                state.overlay_focus_restore = OverlayFocusRestore::Inactive;
                                target
                            }
                        };
                        Some(target)
                    }
                    _ => None,
                }
            }
        };
        if let Some(target) = restore_target {
            self.set_focus_internal(target, true);
        }

        let focus = self.inner.borrow().focused_component.clone();
        let Some(focus) = focus else {
            return;
        };
        if is_key_release(data) && !focus.wants_key_release() {
            return;
        }
        focus.handle_input(data);
        self.inner.borrow_mut().request_immediate_render();
        self.flush_actions();
    }

    fn consume_background_response(&self, data: &str) -> bool {
        let mut callback = None;
        let value;
        {
            let mut state = self.inner.borrow_mut();
            if state.pending_background_replies == 0 || !is_osc11_background_color_response(data) {
                return false;
            }
            value = parse_osc11_background_color(data);
            state.pending_background_replies -= 1;
            if let Some(mut query) = state.background_queries.pop_front()
                && !query.settled
            {
                query.settled = true;
                if let Some(timer) = query.timer.take() {
                    state.cancel_task(timer);
                }
                callback = query.callback.take();
            }
        }
        self.flush_actions();
        if let Some(callback) = callback {
            callback(value);
        }
        true
    }

    fn consume_color_scheme_report(&self, data: &str) -> bool {
        let Some(scheme) = parse_terminal_color_scheme_report(data) else {
            return false;
        };
        let mut last_order = 0;
        loop {
            enum Dispatch {
                Persistent(TerminalColorSchemeListener),
                Query(ColorSchemeQueryCallback),
            }
            let dispatch = {
                let mut state = self.inner.borrow_mut();
                let Some(index) = state
                    .scheme_listeners
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.order() > last_order)
                    .min_by_key(|(_, entry)| entry.order())
                    .map(|(index, _)| index)
                else {
                    break;
                };
                last_order = state.scheme_listeners[index].order();
                match &state.scheme_listeners[index] {
                    SchemeListenerEntry::Persistent { listener, .. } => {
                        Dispatch::Persistent(listener.clone())
                    }
                    SchemeListenerEntry::Query { .. } => {
                        let SchemeListenerEntry::Query {
                            timer,
                            mut callback,
                            ..
                        } = state.scheme_listeners.remove(index)
                        else {
                            unreachable!()
                        };
                        state.cancel_task(timer);
                        let Some(callback) = callback.take() else {
                            continue;
                        };
                        Dispatch::Query(callback)
                    }
                }
            };
            self.flush_actions();
            match dispatch {
                Dispatch::Persistent(listener) => listener.invoke(scheme),
                Dispatch::Query(callback) => callback(Some(scheme)),
            }
        }
        true
    }

    fn consume_cell_size_response(&self, data: &str) -> bool {
        let Some((height_px, width_px)) = parse_cell_size_response(data) else {
            return false;
        };
        if height_px == 0 || width_px == 0 {
            return true;
        }
        {
            self.inner.borrow_mut().actions.push_back(
                ControllerAction::SetCellDimensionsAndInvalidate(CellDimensions {
                    width_px: width_px as f64,
                    height_px: height_px as f64,
                }),
            );
        }
        self.flush_actions();
        true
    }

    pub fn run_task(&self, id: TuiTaskId, task: TuiHostTask) {
        let mut deferred_background = None;
        let mut deferred_scheme = None;
        let mut should_drain_terminal_events = false;
        let mut should_render = false;
        {
            let mut state = self.inner.borrow_mut();
            let Some(token) = state.claim_task(id, task) else {
                return;
            };
            match task {
                TuiHostTask::DrainTerminalEvents => {
                    if state.terminal_event_task != Some(token) {
                        return;
                    }
                    state.terminal_event_task = None;
                    should_drain_terminal_events = true;
                }
                TuiHostTask::ScheduleRender => {
                    if state.stopped || state.render_timer.is_some() || !state.render_requested {
                        return;
                    }
                    state
                        .actions
                        .push_back(ControllerAction::ResolveRenderDelay);
                }
                TuiHostTask::ImmediateRender => {
                    state.immediate_render_scheduled = false;
                    if state.stopped || !state.render_requested {
                        return;
                    }
                    state.cancel_render_timer();
                    should_render = true;
                }
                TuiHostTask::RenderTimer => {
                    if state.render_timer != Some(token) {
                        return;
                    }
                    state.render_timer = None;
                    if state.stopped || !state.render_requested {
                        return;
                    }
                    should_render = true;
                }
                TuiHostTask::BackgroundQueryTimeout { query_id } => {
                    if let Some(query) = state
                        .background_queries
                        .iter_mut()
                        .find(|query| query.id == query_id)
                        && !query.settled
                    {
                        query.settled = true;
                        query.timer = None;
                        deferred_background = query.callback.take();
                    }
                }
                TuiHostTask::ColorSchemeQueryTimeout { query_id } => {
                    if let Some(index) = state.scheme_listeners.iter().position(
                        |entry| matches!(entry, SchemeListenerEntry::Query { id, .. } if *id == query_id),
                    ) && let SchemeListenerEntry::Query { mut callback, .. } =
                        state.scheme_listeners.remove(index)
                    {
                        deferred_scheme = callback.take();
                    }
                }
                TuiHostTask::AltFlashTimeout { .. } => return,
            }
        }
        self.flush_actions();
        if let Some(callback) = deferred_background {
            callback(None);
        }
        if let Some(callback) = deferred_scheme {
            callback(None);
        }
        if should_drain_terminal_events {
            self.drain_terminal_events();
        }
        if should_render {
            self.perform_render(false);
        }
    }

    pub fn resolve_overlay_layout(
        options: &OverlayOptions,
        overlay_height: usize,
        term_width: usize,
        term_height: usize,
    ) -> OverlayLayout {
        resolve_overlay_layout(options, overlay_height, term_width, term_height)
    }

    pub fn composite_overlays(
        &self,
        mut lines: Vec<String>,
        term_width: usize,
        term_height: usize,
    ) -> Vec<String> {
        let length = self.inner.borrow().overlays.len();
        if length == 0 {
            return lines;
        }
        let mut entries = Vec::new();
        for index in 0..length {
            let entry = self.inner.borrow().overlays.get(index).cloned();
            if let Some(entry) = entry
                && !entry.hidden
                && entry
                    .options
                    .visible
                    .as_ref()
                    .is_none_or(|visible| visible(term_width, term_height))
            {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|entry| entry.focus_order);
        let mut rendered = Vec::new();
        let mut min_lines_needed = lines.len();
        for entry in entries {
            let initial = resolve_overlay_layout(&entry.options, 0, term_width, term_height);
            let mut overlay_lines = entry.component.render(initial.width);
            if let Some(max_height) = initial.max_height
                && overlay_lines.len() > max_height
            {
                overlay_lines.truncate(max_height);
            }
            let layout = resolve_overlay_layout(
                &entry.options,
                overlay_lines.len(),
                term_width,
                term_height,
            );
            min_lines_needed = min_lines_needed.max(layout.row.saturating_add(overlay_lines.len()));
            rendered.push((overlay_lines, layout));
        }
        let working_height = lines.len().max(term_height).max(min_lines_needed);
        lines.resize(working_height, String::new());
        let viewport_start = working_height.saturating_sub(term_height);
        for (overlay_lines, layout) in rendered {
            for (offset, overlay_line) in overlay_lines.into_iter().enumerate() {
                let index = viewport_start
                    .saturating_add(layout.row)
                    .saturating_add(offset);
                if index >= lines.len() {
                    continue;
                }
                let overlay_line = if visible_width(&overlay_line) > layout.width {
                    slice_by_column(&overlay_line, 0, layout.width, true)
                } else {
                    overlay_line
                };
                lines[index] = composite_tui_line(
                    &lines[index],
                    &overlay_line,
                    layout.col,
                    layout.width,
                    term_width,
                );
            }
        }
        lines
    }

    pub fn apply_line_resets(lines: &mut [String]) {
        for line in lines {
            if !is_image_line(line) {
                *line = format!("{}{SEGMENT_RESET}", normalize_terminal_output(line));
            }
        }
    }

    pub fn extract_cursor_position(lines: &mut [String], height: usize) -> Option<(usize, usize)> {
        let viewport_top = lines.len().saturating_sub(height);
        for row in (viewport_top..lines.len()).rev() {
            if let Some(marker) = lines[row].find(CURSOR_MARKER) {
                let col = visible_width(&lines[row][..marker]);
                lines[row].replace_range(marker..marker + CURSOR_MARKER.len(), "");
                return Some((row, col));
            }
        }
        None
    }
}

impl Drop for TuiBaseController {
    fn drop(&mut self) {
        if Rc::strong_count(&self.inner) != 1 {
            return;
        }
        let focused = {
            let mut state = self.inner.borrow_mut();
            state.plan_teardown(true)
        };
        self.flush_actions();
        if let Some(focused) = focused {
            focused.set_focused(false);
        }
    }
}

#[derive(Clone, Copy)]
enum SubscriptionKind {
    Input(u64),
    Scheme(u64),
}

pub struct TuiSubscription {
    state: Weak<TuiShared>,
    kind: SubscriptionKind,
}

impl SubscriptionControl for TuiSubscription {
    fn unsubscribe(&self) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut state = state.borrow_mut();
        match self.kind {
            SubscriptionKind::Input(identity) => state
                .input_listeners
                .retain(|listener| listener.listener.identity() != identity),
            SubscriptionKind::Scheme(identity) => state.scheme_listeners.retain(|listener| {
                !matches!(listener, SchemeListenerEntry::Persistent { listener: current, .. } if current.identity() == identity)
            }),
        }
    }

    fn is_active(&self) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        let state = state.borrow();
        match self.kind {
            SubscriptionKind::Input(identity) => state
                .input_listeners
                .iter()
                .any(|listener| listener.listener.identity() == identity),
            SubscriptionKind::Scheme(identity) => state.scheme_listeners.iter().any(|listener| {
                matches!(listener, SchemeListenerEntry::Persistent { listener: current, .. } if current.identity() == identity)
            }),
        }
    }
}

pub struct OverlayHandle {
    state: Weak<TuiShared>,
    overlay_id: u64,
}

impl OverlayHandle {
    fn controller(&self) -> Option<TuiBaseController> {
        self.state
            .upgrade()
            .map(|inner| TuiBaseController { inner })
    }

    pub fn hide(&self) {
        let Some(controller) = self.controller() else {
            return;
        };
        let removed = {
            let mut state = controller.inner.borrow_mut();
            let Some(index) = state
                .overlays
                .iter()
                .position(|entry| entry.id == self.overlay_id)
            else {
                return;
            };
            let entry = state.overlays.remove(index);
            state.clear_focus_restore_for(entry.id);
            state.retarget_pre_focus(&entry);
            let restore_focus = state
                .focused_component
                .as_ref()
                .is_some_and(|focus| focus.ptr_eq(&entry.component));
            (entry, restore_focus)
        };
        if removed.1 {
            let target = controller
                .topmost_visible_overlay()
                .map(|overlay| overlay.component)
                .or(removed.0.pre_focus);
            controller.set_focus_internal(target, true);
        }
        {
            let mut state = controller.inner.borrow_mut();
            if state.overlays.is_empty() {
                state
                    .actions
                    .push_back(ControllerAction::TerminalHideCursor);
                state.cursor_hidden = true;
            }
            state.request_render(false);
        }
        controller.flush_actions();
    }

    pub fn set_hidden(&self, hidden: bool) {
        let Some(controller) = self.controller() else {
            return;
        };
        let changed = {
            let mut state = controller.inner.borrow_mut();
            let Some(index) = state
                .overlays
                .iter()
                .position(|entry| entry.id == self.overlay_id)
            else {
                return;
            };
            if state.overlays[index].hidden == hidden {
                return;
            }
            state.overlays[index].hidden = hidden;
            let entry = state.overlays[index].clone();
            let is_focused = state
                .focused_component
                .as_ref()
                .is_some_and(|focus| focus.ptr_eq(&entry.component));
            if hidden {
                state.clear_focus_restore_for(self.overlay_id);
            }
            (entry, is_focused)
        };
        if hidden && changed.1 {
            let target = controller
                .topmost_visible_overlay()
                .map(|overlay| overlay.component)
                .or(changed.0.pre_focus);
            controller.set_focus_internal(target, true);
        } else if !hidden
            && !changed.0.options.non_capturing
            && controller.overlay_is_visible(self.overlay_id)
        {
            let component = {
                let mut state = controller.inner.borrow_mut();
                state.focus_order_counter = state.focus_order_counter.wrapping_add(1);
                let order = state.focus_order_counter;
                let Some(entry) = state
                    .overlays
                    .iter_mut()
                    .find(|entry| entry.id == self.overlay_id)
                else {
                    return;
                };
                entry.focus_order = order;
                entry.component.clone()
            };
            controller.set_focus_internal(Some(component), true);
        }
        controller.inner.borrow_mut().request_render(false);
        controller.flush_actions();
    }

    pub fn is_hidden(&self) -> bool {
        self.state.upgrade().is_none_or(|state| {
            state
                .borrow()
                .overlays
                .iter()
                .find(|entry| entry.id == self.overlay_id)
                .is_none_or(|entry| entry.hidden)
        })
    }

    pub fn focus(&self) {
        let Some(controller) = self.controller() else {
            return;
        };
        if !controller.overlay_is_visible(self.overlay_id) {
            return;
        }
        let component = {
            let mut state = controller.inner.borrow_mut();
            state.focus_order_counter = state.focus_order_counter.wrapping_add(1);
            let order = state.focus_order_counter;
            let Some(entry) = state
                .overlays
                .iter_mut()
                .find(|entry| entry.id == self.overlay_id)
            else {
                return;
            };
            entry.focus_order = order;
            entry.component.clone()
        };
        controller.set_focus_internal(Some(component), true);
        controller.inner.borrow_mut().request_render(false);
        controller.flush_actions();
    }

    pub fn unfocus(&self, target: OverlayUnfocus) {
        let Some(controller) = self.controller() else {
            return;
        };
        let (focus_target, pre_focus) = {
            let mut state = controller.inner.borrow_mut();
            let Some(index) = state
                .overlays
                .iter()
                .position(|entry| entry.id == self.overlay_id)
            else {
                return;
            };
            let component = state.overlays[index].component.clone();
            let pre_focus = state.overlays[index].pre_focus.clone();
            let is_focused = state
                .focused_component
                .as_ref()
                .is_some_and(|focus| focus.ptr_eq(&component));
            let restore = state.overlay_focus_restore.clone();
            let has_pending = match &restore {
                OverlayFocusRestore::Inactive => false,
                OverlayFocusRestore::Eligible { overlay_id }
                | OverlayFocusRestore::Blocked { overlay_id, .. } => *overlay_id == self.overlay_id,
            };
            if !is_focused && !has_pending {
                return;
            }
            if let OverlayFocusRestore::Blocked {
                overlay_id,
                blocked_by,
                resume: _,
            } = restore
                && overlay_id == self.overlay_id
                && state
                    .focused_component
                    .as_ref()
                    .is_some_and(|focus| focus.ptr_eq(&blocked_by))
            {
                state.overlay_focus_restore = match target {
                    OverlayUnfocus::Target(target) => OverlayFocusRestore::Blocked {
                        overlay_id,
                        blocked_by,
                        resume: OverlayResume::FocusTarget(target),
                    },
                    OverlayUnfocus::Restore => OverlayFocusRestore::Inactive,
                };
                state.request_render(false);
                drop(state);
                controller.flush_actions();
                return;
            }
            state.clear_focus_restore_for(self.overlay_id);
            if is_focused || matches!(target, OverlayUnfocus::Target(_)) {
                (
                    Some(match target {
                        OverlayUnfocus::Restore => None,
                        OverlayUnfocus::Target(target) => target,
                    }),
                    pre_focus,
                )
            } else {
                (None, pre_focus)
            }
        };
        if let Some(target) = focus_target {
            let target = match target {
                Some(target) => Some(target),
                None => controller
                    .topmost_visible_overlay()
                    .filter(|entry| entry.id != self.overlay_id)
                    .map(|entry| entry.component)
                    .or(pre_focus),
            };
            controller.set_focus_internal(target, true);
        }
        controller.inner.borrow_mut().request_render(false);
        controller.flush_actions();
    }

    pub fn is_focused(&self) -> bool {
        self.state.upgrade().is_some_and(|state| {
            let state = state.borrow();
            let Some(entry) = state
                .overlays
                .iter()
                .find(|entry| entry.id == self.overlay_id)
            else {
                return false;
            };
            state
                .focused_component
                .as_ref()
                .is_some_and(|focus| focus.ptr_eq(&entry.component))
        })
    }
}

impl OverlayControl for OverlayHandle {
    fn hide(&self) {
        Self::hide(self);
    }
    fn set_hidden(&self, hidden: bool) {
        Self::set_hidden(self, hidden);
    }
    fn is_hidden(&self) -> bool {
        Self::is_hidden(self)
    }
    fn focus(&self) {
        Self::focus(self);
    }
    fn unfocus(&self, target: OverlayUnfocus) {
        Self::unfocus(self, target);
    }
    fn is_focused(&self) -> bool {
        Self::is_focused(self)
    }
}

impl Tui for TuiBaseController {
    fn mode(&self) -> TuiMode {
        Self::mode(self)
    }
    fn terminal_columns(&self) -> usize {
        Self::terminal_columns(self)
    }
    fn terminal_rows(&self) -> usize {
        Self::terminal_rows(self)
    }
    fn full_redraws(&self) -> usize {
        Self::full_redraws(self)
    }
    fn add_child(&self, component: ComponentRef) -> ContainerChildId {
        Self::add_child(self, component)
    }
    fn remove_child(&self, child: ContainerChildId) -> bool {
        Self::remove_child(self, child)
    }
    fn clear(&self) {
        Self::clear(self);
    }
    fn show_hardware_cursor(&self) -> bool {
        Self::show_hardware_cursor(self)
    }
    fn set_show_hardware_cursor(&self, enabled: bool) {
        Self::set_show_hardware_cursor(self, enabled);
    }
    fn clear_on_shrink(&self) -> bool {
        Self::clear_on_shrink(self)
    }
    fn set_clear_on_shrink(&self, enabled: bool) {
        Self::set_clear_on_shrink(self, enabled);
    }
    fn focused_component(&self) -> Option<ComponentRef> {
        Self::focused_component(self)
    }
    fn set_focus(&self, component: Option<ComponentRef>) {
        Self::set_focus(self, component);
    }
    fn show_overlay(
        &self,
        component: ComponentRef,
        options: OverlayOptions,
    ) -> Box<dyn OverlayControl> {
        Box::new(Self::show_overlay(self, component, options))
    }
    fn hide_overlay(&self) {
        Self::hide_overlay(self);
    }
    fn has_overlay(&self) -> bool {
        Self::has_overlay(self)
    }
    fn start(&self) {
        Self::start(self);
    }
    fn stop(&self, options: TuiStopOptions) {
        Self::stop(self, options);
    }
    fn render_now(&self, force: bool) {
        Self::render_now(self, force);
    }
    fn request_render(&self, force: bool) {
        Self::request_render(self, force);
    }
    fn add_input_listener(&self, listener: TuiInputListener) -> Box<dyn SubscriptionControl> {
        Box::new(Self::add_input_listener(self, listener))
    }
    fn remove_input_listener(&self, listener: &TuiInputListener) {
        Self::remove_input_listener(self, listener);
    }
    fn on_terminal_color_scheme_change(
        &self,
        listener: TerminalColorSchemeListener,
    ) -> Box<dyn SubscriptionControl> {
        Box::new(Self::on_terminal_color_scheme_change(self, listener))
    }
    fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        Self::set_terminal_color_scheme_notifications(self, enabled);
    }
    fn query_terminal_background_color(
        &self,
        timeout_ms: u64,
        callback: BackgroundColorQueryCallback,
    ) {
        Self::query_terminal_background_color(self, timeout_ms, callback);
    }
    fn query_terminal_color_scheme(&self, timeout_ms: u64, callback: ColorSchemeQueryCallback) {
        Self::query_terminal_color_scheme(self, timeout_ms, callback);
    }
    fn set_debug_callback(&self, callback: Option<DebugCallback>) {
        Self::set_debug_callback(self, callback);
    }
}

impl ViewportTui for TuiBaseController {
    fn set_layout_root(&self, component: Option<ComponentRef>) {
        Self::set_layout_root(self, component);
    }
}

fn parse_cell_size_response(data: &str) -> Option<(u64, u64)> {
    let body = data.strip_prefix("\x1b[6;")?.strip_suffix('t')?;
    let (height, width) = body.split_once(';')?;
    if height.is_empty()
        || width.is_empty()
        || !height.bytes().all(|byte| byte.is_ascii_digit())
        || !width.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((height.parse().ok()?, width.parse().ok()?))
}

fn resolved_size(value: pie_components::SizeValue, reference: usize) -> i64 {
    let resolved = value.resolve(reference);
    if resolved.is_nan() {
        0
    } else if resolved >= i64::MAX as f64 {
        i64::MAX
    } else if resolved <= i64::MIN as f64 {
        i64::MIN
    } else {
        resolved.trunc() as i64
    }
}

fn percentage_position(value: pie_components::SizeValue, maximum: usize) -> i64 {
    match value {
        pie_components::SizeValue::Percent(percent) => {
            ((maximum as f64 * percent) / 100.0).floor() as i64
        }
        pie_components::SizeValue::Absolute(value) => value.trunc() as i64,
    }
}

fn resolve_anchor_row(
    anchor: OverlayAnchor,
    height: usize,
    available_height: usize,
    margin_top: usize,
) -> usize {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::TopCenter | OverlayAnchor::TopRight => margin_top,
        OverlayAnchor::BottomLeft | OverlayAnchor::BottomCenter | OverlayAnchor::BottomRight => {
            margin_top.saturating_add(available_height.saturating_sub(height))
        }
        OverlayAnchor::LeftCenter | OverlayAnchor::Center | OverlayAnchor::RightCenter => {
            margin_top.saturating_add(available_height.saturating_sub(height) / 2)
        }
    }
}

fn resolve_anchor_col(
    anchor: OverlayAnchor,
    width: usize,
    available_width: usize,
    margin_left: usize,
) -> usize {
    match anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::LeftCenter | OverlayAnchor::BottomLeft => {
            margin_left
        }
        OverlayAnchor::TopRight | OverlayAnchor::RightCenter | OverlayAnchor::BottomRight => {
            margin_left.saturating_add(available_width.saturating_sub(width))
        }
        OverlayAnchor::TopCenter | OverlayAnchor::Center | OverlayAnchor::BottomCenter => {
            margin_left.saturating_add(available_width.saturating_sub(width) / 2)
        }
    }
}

fn resolve_overlay_layout(
    options: &OverlayOptions,
    overlay_height: usize,
    term_width: usize,
    term_height: usize,
) -> OverlayLayout {
    let margin = match options.margin {
        None => pie_components::OverlayMargin::default(),
        Some(OverlayMargins::All(value)) => pie_components::OverlayMargin {
            top: value,
            right: value,
            bottom: value,
            left: value,
        },
        Some(OverlayMargins::Sides(value)) => value,
    };
    let margin_top = usize::try_from(margin.top.max(0)).unwrap_or(usize::MAX);
    let margin_right = usize::try_from(margin.right.max(0)).unwrap_or(usize::MAX);
    let margin_bottom = usize::try_from(margin.bottom.max(0)).unwrap_or(usize::MAX);
    let margin_left = usize::try_from(margin.left.max(0)).unwrap_or(usize::MAX);
    let available_width = term_width
        .saturating_sub(margin_left)
        .saturating_sub(margin_right)
        .max(1);
    let available_height = term_height
        .saturating_sub(margin_top)
        .saturating_sub(margin_bottom)
        .max(1);

    let mut width = options
        .width
        .map(|value| resolved_size(value, term_width))
        .unwrap_or_else(|| i64::try_from(80.min(available_width)).unwrap_or(i64::MAX));
    if let Some(minimum) = options.min_width {
        width = width.max(i64::try_from(minimum).unwrap_or(i64::MAX));
    }
    let width = usize::try_from(width.max(1))
        .unwrap_or(usize::MAX)
        .min(available_width);
    let max_height = options.max_height.map(|value| {
        usize::try_from(resolved_size(value, term_height).max(1))
            .unwrap_or(usize::MAX)
            .min(available_height)
    });
    let effective_height = max_height.map_or(overlay_height, |limit| overlay_height.min(limit));

    let base_row = options.row.map_or_else(
        || {
            resolve_anchor_row(
                options.anchor,
                effective_height,
                available_height,
                margin_top,
            )
        },
        |value| match value {
            pie_components::SizeValue::Percent(_) => margin_top.saturating_add(
                usize::try_from(percentage_position(
                    value,
                    available_height.saturating_sub(effective_height),
                ))
                .unwrap_or(usize::MAX),
            ),
            pie_components::SizeValue::Absolute(_) => {
                usize::try_from(percentage_position(value, 0).max(0)).unwrap_or(usize::MAX)
            }
        },
    );
    let base_col = options.col.map_or_else(
        || resolve_anchor_col(options.anchor, width, available_width, margin_left),
        |value| match value {
            pie_components::SizeValue::Percent(_) => margin_left.saturating_add(
                usize::try_from(percentage_position(
                    value,
                    available_width.saturating_sub(width),
                ))
                .unwrap_or(usize::MAX),
            ),
            pie_components::SizeValue::Absolute(_) => {
                usize::try_from(percentage_position(value, 0).max(0)).unwrap_or(usize::MAX)
            }
        },
    );
    let shifted_row = i64::try_from(base_row)
        .unwrap_or(i64::MAX)
        .saturating_add(options.offset_y);
    let shifted_col = i64::try_from(base_col)
        .unwrap_or(i64::MAX)
        .saturating_add(options.offset_x);
    let minimum_row = i64::try_from(margin_top).unwrap_or(i64::MAX);
    let maximum_row = i64::try_from(
        term_height
            .saturating_sub(margin_bottom)
            .saturating_sub(effective_height),
    )
    .unwrap_or(i64::MAX);
    let minimum_col = i64::try_from(margin_left).unwrap_or(i64::MAX);
    let maximum_col = i64::try_from(
        term_width
            .saturating_sub(margin_right)
            .saturating_sub(width),
    )
    .unwrap_or(i64::MAX);

    OverlayLayout {
        width,
        row: usize::try_from(shifted_row.clamp(minimum_row, maximum_row.max(minimum_row)))
            .unwrap_or(usize::MAX),
        col: usize::try_from(shifted_col.clamp(minimum_col, maximum_col.max(minimum_col)))
            .unwrap_or(usize::MAX),
        max_height,
    }
}
