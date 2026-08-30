//! Concrete main-screen controller over the shared TUI lifecycle.

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;

use pie_components::{TuiMode, TuiStopOptions};
use pie_core::frame::LogicalFrame;
use pie_term::Terminal;
use pie_term::renderer::{MainScreenRenderer, RenderState};

use crate::screen_runtime::{
    ScreenControllerHost, ScreenLifecycle, SharedTerminal, TuiScreenRuntime, delegate_tui,
    shared_runtime,
};
use crate::tui_controller::{TuiBaseController, TuiHostTask, TuiTaskId, WeakTuiBaseController};

struct MainScreenState {
    base: RefCell<Option<WeakTuiBaseController>>,
    terminal: SharedTerminal,
    renderer: RefCell<MainScreenRenderer>,
    render_snapshot: RefCell<RenderState>,
    active: Cell<bool>,
}

impl MainScreenState {
    fn base(&self) -> Option<TuiBaseController> {
        self.base.borrow().as_ref()?.upgrade()
    }

    fn set_base(&self, base: WeakTuiBaseController) {
        *self.base.borrow_mut() = Some(base);
    }
}

impl ScreenLifecycle for MainScreenState {
    fn render(&self) {
        let Some(base) = self.base() else {
            return;
        };
        let width = base.terminal_columns();
        let height = base.terminal_rows();
        let mut lines = base.render_document(width);
        if base.has_overlay_entries() {
            lines = base.composite_overlays(lines, width, height);
        }
        let frame = LogicalFrame::new(lines, width, height)
            .unwrap_or_else(|error| panic!("main-screen frame rejected: {error:?}"));
        let (snapshot, full_redraws) = {
            let mut renderer = self.renderer.borrow_mut();
            renderer.set_stopped(false);
            renderer.set_clear_on_shrink(base.clear_on_shrink());
            renderer.set_show_hardware_cursor(base.show_hardware_cursor());
            // Lifecycle rendering is entered only from TuiBase's action
            // driver. Public capture reads use the last committed snapshot
            // while the renderer and terminal are executing this plan.
            renderer
                .render_frame(&mut **self.terminal.borrow_mut(), &frame, false)
                .unwrap_or_else(|error| panic!("main-screen render failed: {error:?}"));
            (renderer.capture_render_state(), renderer.full_redraws())
        };
        *self.render_snapshot.borrow_mut() = snapshot;
        base.set_full_redraws(full_redraws);
    }

    fn reset_render_state(&self) {
        let snapshot = {
            let mut renderer = self.renderer.borrow_mut();
            renderer.reset_render_state();
            renderer.capture_render_state()
        };
        *self.render_snapshot.borrow_mut() = snapshot;
    }

    fn images_supported(&self) -> bool {
        true
    }

    fn before_terminal_start(&self) {
        self.active.set(true);
        self.renderer.borrow_mut().set_stopped(false);
    }

    fn before_terminal_stop(&self, options: TuiStopOptions) {
        let state = self.render_snapshot.borrow().clone();
        if !options.preserve_screen
            && !state.previous_lines.is_empty()
            && let Some(base) = self.base()
        {
            base.write_terminal(" ");
            let target_row = isize::try_from(state.previous_lines.len()).unwrap_or(isize::MAX);
            let line_diff = target_row.saturating_sub(state.hardware_cursor_row);
            if line_diff > 0 {
                base.write_terminal(format!("\x1b[{line_diff}B"));
            } else if line_diff < 0 {
                base.write_terminal(format!("\x1b[{}A", line_diff.saturating_abs()));
            }
            base.write_terminal("\r\n");
        }
        self.renderer.borrow_mut().set_stopped(true);
        self.active.set(false);
    }

    fn controller_dropped(&self) {
        self.active.set(false);
        self.renderer.borrow_mut().set_stopped(true);
        self.base.borrow_mut().take();
    }
}

/// Main-screen TUI with scrollback-preserving differential rendering.
///
/// Runtime/task delivery and terminal IO are injected. This controller does
/// not select or own a process event loop.
pub struct TuiMainScreen {
    // Drop the base first so its host can notify `MainScreenState` while the
    // controller's explicit state owner is still alive.
    base: TuiBaseController,
    state: Rc<MainScreenState>,
}

impl TuiMainScreen {
    pub fn new(
        terminal: Box<dyn Terminal>,
        runtime: Box<dyn TuiScreenRuntime>,
        show_hardware_cursor: bool,
    ) -> Self {
        let terminal = SharedTerminal::new(terminal);
        let runtime = shared_runtime(runtime);
        let renderer = MainScreenRenderer::new();
        let render_snapshot = renderer.capture_render_state();
        let state = Rc::new(MainScreenState {
            base: RefCell::new(None),
            terminal: terminal.clone(),
            renderer: RefCell::new(renderer),
            render_snapshot: RefCell::new(render_snapshot),
            active: Cell::new(false),
        });
        let lifecycle: Rc<dyn ScreenLifecycle> = state.clone();
        let host = ScreenControllerHost::new(runtime, lifecycle);
        let base = TuiBaseController::new(
            Box::new(terminal),
            Box::new(host),
            TuiMode::Regular,
            show_hardware_cursor,
        );
        state.set_base(base.downgrade());
        Self { base, state }
    }

    pub(crate) fn base(&self) -> &TuiBaseController {
        &self.base
    }

    pub fn capture_render_state(&self) -> RenderState {
        self.state.render_snapshot.borrow().clone()
    }

    pub fn restore_render_state(&self, state: RenderState) {
        let snapshot = {
            let mut renderer = self.state.renderer.borrow_mut();
            renderer.restore_render_state(state);
            renderer.capture_render_state()
        };
        *self.state.render_snapshot.borrow_mut() = snapshot;
    }

    pub fn run_task(&self, id: TuiTaskId, task: TuiHostTask) {
        self.base.run_task(id, task);
    }
}

impl Deref for TuiMainScreen {
    type Target = TuiBaseController;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl Drop for TuiMainScreen {
    fn drop(&mut self) {
        if self.state.active.get() {
            self.base.stop(TuiStopOptions {
                preserve_screen: true,
            });
        }
    }
}

delegate_tui!(TuiMainScreen);
