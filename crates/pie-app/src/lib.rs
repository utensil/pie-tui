//! Application-owned TUI controllers.
//!
//! Controllers bridge component rendering to the pure `pie-core` frame model
//! and the `pie-term` ANSI planner/executor.  Scheduling and overlay assembly
//! can grow here without pulling component/runtime concerns into lower ranks.

mod screen_runtime;
mod tui_alt_screen;
mod tui_controller;
mod tui_main_screen;

pub use screen_runtime::{DetachedTuiScreenRuntime, TuiScreenRuntime};
pub use tui_alt_screen::{
    OpenUrlCallback, RightClickPasteCallback, TuiAltScreen, TuiAltScreenEnvironment,
    TuiAltScreenOptions,
};
pub use tui_controller::{
    DetachedTuiControllerHost, OverlayHandle, OverlayLayout, TuiBaseController, TuiControllerHost,
    TuiHostTask, TuiSubscription, TuiTaskId,
};
pub use tui_main_screen::TuiMainScreen;

use pie_core::frame::LogicalFrame;
use pie_term::Terminal;
pub use pie_term::renderer::LineSource;
use pie_term::renderer::{MainScreenRenderer, RenderError, RenderState};

/// Main-screen render controller owned by the application layer.
#[derive(Debug, Clone, Default)]
pub struct MainScreenController {
    renderer: MainScreenRenderer,
}

impl MainScreenController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn renderer(&self) -> &MainScreenRenderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut MainScreenRenderer {
        &mut self.renderer
    }

    pub fn stopped(&self) -> bool {
        self.renderer.stopped()
    }

    pub fn set_stopped(&mut self, stopped: bool) {
        self.renderer.set_stopped(stopped);
    }

    pub fn set_clear_on_shrink(&mut self, enabled: bool) {
        self.renderer.set_clear_on_shrink(enabled);
    }

    pub fn full_redraws(&self) -> usize {
        self.renderer.full_redraws()
    }

    pub fn capture_render_state(&self) -> RenderState {
        self.renderer.capture_render_state()
    }

    pub fn restore_render_state(&mut self, state: RenderState) {
        self.renderer.restore_render_state(state);
    }

    /// Render the component source into a pure logical frame, then execute the
    /// terminal plan.  A stopped controller is a strict no-op and does not ask
    /// the component tree to render.
    pub fn render_now(
        &mut self,
        terminal: &mut dyn Terminal,
        source: &mut dyn LineSource,
        force: bool,
    ) -> Result<(), RenderError> {
        if self.renderer.stopped() {
            return Ok(());
        }
        let width = terminal.columns();
        let height = terminal.rows();
        let frame = LogicalFrame::new(source.render_lines(width), width, height)?;
        self.renderer.render_frame(terminal, &frame, force)
    }
}
