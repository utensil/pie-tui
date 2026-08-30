//! Runtime and terminal seams shared by the concrete screen controllers.

use std::cell::{Cell, RefCell, RefMut};
use std::rc::Rc;

use pie_components::TuiStopOptions;
use pie_core::terminal_image::CellDimensions;
use pie_term::{InputHandler, ResizeHandler, Terminal};

use crate::tui_controller::{TuiControllerHost, TuiHostTask, TuiTaskId};

/// Injected clock/task seam for concrete Main/Alt screen controllers.
///
/// The runtime owns task delivery. Callers feed a delivered `(id, task)` back
/// to the controller's `run_task` method, which keeps tests deterministic and
/// avoids binding this rank-3 layer to a process event loop.
pub trait TuiScreenRuntime {
    fn now_ms(&self) -> u64;
    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId;
    fn cancel_task(&mut self, task: TuiTaskId);
    fn images_supported(&self) -> bool;
    fn set_cell_dimensions(&mut self, dimensions: CellDimensions);
}

/// Inert runtime for structural embedders which drive rendering explicitly.
pub struct DetachedTuiScreenRuntime {
    now_ms: u64,
    next_task: u64,
    images_supported: bool,
    cell_dimensions: CellDimensions,
}

impl DetachedTuiScreenRuntime {
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

impl Default for DetachedTuiScreenRuntime {
    fn default() -> Self {
        Self::new(false)
    }
}

impl TuiScreenRuntime for DetachedTuiScreenRuntime {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }

    fn schedule_task(&mut self, _delay_ms: u64, _task: TuiHostTask) -> TuiTaskId {
        self.next_task = self.next_task.wrapping_add(1);
        TuiTaskId(self.next_task)
    }

    fn cancel_task(&mut self, _task: TuiTaskId) {}

    fn images_supported(&self) -> bool {
        self.images_supported
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        self.cell_dimensions = dimensions;
    }
}

pub(crate) type SharedScreenRuntime = Rc<RefCell<Box<dyn TuiScreenRuntime>>>;

/// Cloneable proxy around the single injected terminal. One clone is moved
/// into `TuiBaseController`; screen render/lifecycle hooks keep another.
#[derive(Clone)]
pub(crate) struct SharedTerminal(Rc<SharedTerminalState>);

struct SharedTerminalState {
    terminal: RefCell<Box<dyn Terminal>>,
    columns: Cell<usize>,
    rows: Cell<usize>,
    kitty_protocol_active: Cell<bool>,
}

impl SharedTerminal {
    pub(crate) fn new(terminal: Box<dyn Terminal>) -> Self {
        let columns = terminal.columns();
        let rows = terminal.rows();
        let kitty_protocol_active = terminal.kitty_protocol_active();
        Self(Rc::new(SharedTerminalState {
            terminal: RefCell::new(terminal),
            columns: Cell::new(columns),
            rows: Cell::new(rows),
            kitty_protocol_active: Cell::new(kitty_protocol_active),
        }))
    }

    pub(crate) fn borrow_mut(&self) -> RefMut<'_, Box<dyn Terminal>> {
        self.0.terminal.borrow_mut()
    }

    fn columns_snapshot(&self) -> usize {
        if let Ok(terminal) = self.0.terminal.try_borrow() {
            self.0.columns.set(terminal.columns());
        }
        self.0.columns.get()
    }

    fn rows_snapshot(&self) -> usize {
        if let Ok(terminal) = self.0.terminal.try_borrow() {
            self.0.rows.set(terminal.rows());
        }
        self.0.rows.get()
    }

    fn kitty_protocol_snapshot(&self) -> bool {
        if let Ok(terminal) = self.0.terminal.try_borrow() {
            self.0
                .kitty_protocol_active
                .set(terminal.kitty_protocol_active());
        }
        self.0.kitty_protocol_active.get()
    }
}

impl Terminal for SharedTerminal {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        self.0.terminal.borrow_mut().start(on_input, on_resize);
    }

    fn stop(&mut self) {
        self.0.terminal.borrow_mut().stop();
    }

    fn write(&mut self, data: &str) {
        self.0.terminal.borrow_mut().write(data);
    }

    fn columns(&self) -> usize {
        self.columns_snapshot()
    }

    fn rows(&self) -> usize {
        self.rows_snapshot()
    }

    fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_snapshot()
    }

    fn move_by(&mut self, lines: isize) {
        self.0.terminal.borrow_mut().move_by(lines);
    }

    fn hide_cursor(&mut self) {
        self.0.terminal.borrow_mut().hide_cursor();
    }

    fn show_cursor(&mut self) {
        self.0.terminal.borrow_mut().show_cursor();
    }

    fn clear_line(&mut self) {
        self.0.terminal.borrow_mut().clear_line();
    }

    fn clear_from_cursor(&mut self) {
        self.0.terminal.borrow_mut().clear_from_cursor();
    }

    fn clear_screen(&mut self) {
        self.0.terminal.borrow_mut().clear_screen();
    }

    fn set_title(&mut self, title: &str) {
        self.0.terminal.borrow_mut().set_title(title);
    }

    fn set_progress(&mut self, active: bool) {
        self.0.terminal.borrow_mut().set_progress(active);
    }
}

pub(crate) trait ScreenLifecycle {
    fn render(&self);
    fn reset_render_state(&self);
    fn images_supported(&self) -> bool;
    fn before_terminal_start(&self) {}
    fn after_terminal_start(&self) {}
    fn before_terminal_stop(&self, _options: TuiStopOptions) {}
    fn after_terminal_stop(&self, _options: TuiStopOptions) {}
    fn controller_dropped(&self) {}
}

pub(crate) struct ScreenControllerHost {
    runtime: SharedScreenRuntime,
    lifecycle: Rc<dyn ScreenLifecycle>,
}

impl ScreenControllerHost {
    pub(crate) fn new(runtime: SharedScreenRuntime, lifecycle: Rc<dyn ScreenLifecycle>) -> Self {
        Self { runtime, lifecycle }
    }
}

impl TuiControllerHost for ScreenControllerHost {
    fn now_ms(&self) -> u64 {
        self.runtime.borrow().now_ms()
    }

    fn schedule_task(&mut self, delay_ms: u64, task: TuiHostTask) -> TuiTaskId {
        self.runtime.borrow_mut().schedule_task(delay_ms, task)
    }

    fn cancel_task(&mut self, task: TuiTaskId) {
        self.runtime.borrow_mut().cancel_task(task);
    }

    fn render(&mut self) {
        self.lifecycle.render();
    }

    fn reset_render_state(&mut self) {
        self.lifecycle.reset_render_state();
    }

    fn images_supported(&self) -> bool {
        self.lifecycle.images_supported() && self.runtime.borrow().images_supported()
    }

    fn set_cell_dimensions(&mut self, dimensions: CellDimensions) {
        self.runtime.borrow_mut().set_cell_dimensions(dimensions);
    }

    fn before_terminal_start(&mut self) {
        self.lifecycle.before_terminal_start();
    }

    fn after_terminal_start(&mut self) {
        self.lifecycle.after_terminal_start();
    }

    fn before_terminal_stop(&mut self, options: TuiStopOptions) {
        self.lifecycle.before_terminal_stop(options);
    }

    fn after_terminal_stop(&mut self, options: TuiStopOptions) {
        self.lifecycle.after_terminal_stop(options);
    }

    fn controller_dropped(&mut self) {
        self.lifecycle.controller_dropped();
    }
}

pub(crate) fn shared_runtime(runtime: Box<dyn TuiScreenRuntime>) -> SharedScreenRuntime {
    Rc::new(RefCell::new(runtime))
}

macro_rules! delegate_tui {
    ($screen:ty) => {
        impl pie_components::Tui for $screen {
            fn mode(&self) -> pie_components::TuiMode {
                self.base().mode()
            }
            fn terminal_columns(&self) -> usize {
                self.base().terminal_columns()
            }
            fn terminal_rows(&self) -> usize {
                self.base().terminal_rows()
            }
            fn full_redraws(&self) -> usize {
                self.base().full_redraws()
            }
            fn add_child(
                &self,
                component: pie_components::ComponentRef,
            ) -> pie_components::ContainerChildId {
                self.base().add_child(component)
            }
            fn remove_child(&self, child: pie_components::ContainerChildId) -> bool {
                self.base().remove_child(child)
            }
            fn clear(&self) {
                self.base().clear();
            }
            fn show_hardware_cursor(&self) -> bool {
                self.base().show_hardware_cursor()
            }
            fn set_show_hardware_cursor(&self, enabled: bool) {
                self.base().set_show_hardware_cursor(enabled);
            }
            fn clear_on_shrink(&self) -> bool {
                self.base().clear_on_shrink()
            }
            fn set_clear_on_shrink(&self, enabled: bool) {
                self.base().set_clear_on_shrink(enabled);
            }
            fn focused_component(&self) -> Option<pie_components::ComponentRef> {
                self.base().focused_component()
            }
            fn set_focus(&self, component: Option<pie_components::ComponentRef>) {
                self.base().set_focus(component);
            }
            fn show_overlay(
                &self,
                component: pie_components::ComponentRef,
                options: pie_components::OverlayOptions,
            ) -> Box<dyn pie_components::OverlayControl> {
                Box::new(self.base().show_overlay(component, options))
            }
            fn hide_overlay(&self) {
                self.base().hide_overlay();
            }
            fn has_overlay(&self) -> bool {
                self.base().has_overlay()
            }
            fn start(&self) {
                self.base().start();
            }
            fn stop(&self, options: pie_components::TuiStopOptions) {
                self.base().stop(options);
            }
            fn render_now(&self, force: bool) {
                self.base().render_now(force);
            }
            fn request_render(&self, force: bool) {
                self.base().request_render(force);
            }
            fn add_input_listener(
                &self,
                listener: pie_components::TuiInputListener,
            ) -> Box<dyn pie_components::SubscriptionControl> {
                Box::new(self.base().add_input_listener(listener))
            }
            fn remove_input_listener(&self, listener: &pie_components::TuiInputListener) {
                self.base().remove_input_listener(listener);
            }
            fn on_terminal_color_scheme_change(
                &self,
                listener: pie_components::TerminalColorSchemeListener,
            ) -> Box<dyn pie_components::SubscriptionControl> {
                Box::new(self.base().on_terminal_color_scheme_change(listener))
            }
            fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
                self.base().set_terminal_color_scheme_notifications(enabled);
            }
            fn query_terminal_background_color(
                &self,
                timeout_ms: u64,
                callback: pie_components::BackgroundColorQueryCallback,
            ) {
                self.base()
                    .query_terminal_background_color(timeout_ms, callback);
            }
            fn query_terminal_color_scheme(
                &self,
                timeout_ms: u64,
                callback: pie_components::ColorSchemeQueryCallback,
            ) {
                self.base()
                    .query_terminal_color_scheme(timeout_ms, callback);
            }
            fn set_debug_callback(&self, callback: Option<pie_components::DebugCallback>) {
                self.base().set_debug_callback(callback);
            }
        }
    };
}

pub(crate) use delegate_tui;
