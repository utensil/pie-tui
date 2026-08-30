//! Private native screen-planning adapters.

use napi::{Error, Result, Status};
use napi_derive::napi;
use pie_core::frame::LogicalFrame;
use pie_term::renderer::{MainScreenRenderer, RenderState};
use pie_term::{InputHandler, ResizeHandler, Terminal};

#[napi(object)]
pub struct NativeMainScreenRenderState {
    pub previous_lines: Vec<String>,
    pub previous_width: i32,
    pub previous_height: i32,
    pub cursor_row: i32,
    pub hardware_cursor_row: i32,
    pub max_lines_rendered: u32,
    pub previous_viewport_top: u32,
}

impl From<RenderState> for NativeMainScreenRenderState {
    fn from(state: RenderState) -> Self {
        Self {
            previous_lines: state.previous_lines,
            previous_width: state
                .previous_width
                .clamp(i32::MIN as isize, i32::MAX as isize) as i32,
            previous_height: state
                .previous_height
                .clamp(i32::MIN as isize, i32::MAX as isize) as i32,
            cursor_row: state.cursor_row.clamp(i32::MIN as isize, i32::MAX as isize) as i32,
            hardware_cursor_row: state
                .hardware_cursor_row
                .clamp(i32::MIN as isize, i32::MAX as isize)
                as i32,
            max_lines_rendered: state.max_lines_rendered.min(u32::MAX as usize) as u32,
            previous_viewport_top: state.previous_viewport_top.min(u32::MAX as usize) as u32,
        }
    }
}

impl From<NativeMainScreenRenderState> for RenderState {
    fn from(state: NativeMainScreenRenderState) -> Self {
        Self {
            previous_lines: state.previous_lines,
            previous_width: state.previous_width as isize,
            previous_height: state.previous_height as isize,
            cursor_row: state.cursor_row as isize,
            hardware_cursor_row: state.hardware_cursor_row as isize,
            max_lines_rendered: state.max_lines_rendered as usize,
            previous_viewport_top: state.previous_viewport_top as usize,
        }
    }
}

struct NativePlanTerminal {
    actions: Vec<NativeTerminalAction>,
    columns: usize,
    rows: usize,
}

#[napi(object)]
pub struct NativeTerminalAction {
    pub kind: String,
    pub data: Option<String>,
}

impl Terminal for NativePlanTerminal {
    fn start(&mut self, _on_input: InputHandler, _on_resize: ResizeHandler) {}
    fn stop(&mut self) {}
    fn write(&mut self, data: &str) {
        self.actions.push(NativeTerminalAction {
            kind: "write".to_owned(),
            data: Some(data.to_owned()),
        });
    }
    fn columns(&self) -> usize {
        self.columns
    }
    fn rows(&self) -> usize {
        self.rows
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.write(&format!("\x1b[{lines}B"));
        } else if lines < 0 {
            self.write(&format!("\x1b[{}A", -lines));
        }
    }
    fn hide_cursor(&mut self) {
        self.actions.push(NativeTerminalAction {
            kind: "hideCursor".to_owned(),
            data: None,
        });
    }
    fn show_cursor(&mut self) {
        self.actions.push(NativeTerminalAction {
            kind: "showCursor".to_owned(),
            data: None,
        });
    }
    fn clear_line(&mut self) {
        self.write("\x1b[K");
    }
    fn clear_from_cursor(&mut self) {
        self.write("\x1b[J");
    }
    fn clear_screen(&mut self) {
        self.write("\x1b[2J\x1b[H");
    }
    fn set_title(&mut self, title: &str) {
        self.write(&format!("\x1b]0;{title}\x07"));
    }
    fn set_progress(&mut self, active: bool) {
        self.write(if active {
            "\x1b]9;4;3\x07"
        } else {
            "\x1b]9;4;0\x07"
        });
    }
}

#[napi]
pub struct NativeMainScreenPlanner {
    inner: MainScreenRenderer,
}

#[napi]
pub struct NativeAltScreenPlanner {
    previous_screen: Vec<String>,
    previous_width: u32,
    previous_height: u32,
    full_redraws: u32,
}

#[napi]
impl NativeAltScreenPlanner {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            previous_screen: Vec::new(),
            previous_width: 0,
            previous_height: 0,
            full_redraws: 0,
        }
    }

    #[napi]
    pub fn reset(&mut self) {
        self.previous_screen.clear();
        self.previous_width = 0;
        self.previous_height = 0;
    }

    #[napi(getter)]
    pub fn full_redraws(&self) -> u32 {
        self.full_redraws
    }

    #[napi]
    pub fn render(&mut self, mut screen: Vec<String>, columns: u32, rows: u32) -> String {
        let width = columns.max(1);
        let height = rows.max(1);
        screen.truncate(height as usize);
        let full_redraw = self.previous_screen.is_empty()
            || self.previous_width != width
            || self.previous_height != height;
        let mut buffer = String::from("\x1b[?2026h");
        if full_redraw {
            self.full_redraws = self.full_redraws.saturating_add(1);
            buffer.push_str("\x1b[2J");
        }
        for row in 0..height as usize {
            let line = screen.get(row).map_or("", String::as_str);
            if !full_redraw && self.previous_screen.get(row).map_or("", String::as_str) == line {
                continue;
            }
            buffer.push_str(&format!(
                "\x1b[{};1H\x1b[2K{line}\x1b[0m\x1b]8;;\x07",
                row + 1
            ));
        }
        buffer.push_str("\x1b[?25l\x1b[?2026l");
        self.previous_screen = screen;
        self.previous_width = width;
        self.previous_height = height;
        buffer
    }
}

#[napi]
impl NativeMainScreenPlanner {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: MainScreenRenderer::new(),
        }
    }

    #[napi]
    pub fn render(
        &mut self,
        lines: Vec<String>,
        columns: u32,
        rows: u32,
        force: bool,
        clear_on_shrink: bool,
        show_hardware_cursor: bool,
    ) -> Result<Vec<NativeTerminalAction>> {
        let width = columns.max(1) as usize;
        let height = rows.max(1) as usize;
        let frame = LogicalFrame::new(lines, width, height).map_err(|error| {
            Error::new(
                Status::InvalidArg,
                format!("invalid logical frame: {error:?}"),
            )
        })?;
        self.inner.set_clear_on_shrink(clear_on_shrink);
        self.inner.set_show_hardware_cursor(show_hardware_cursor);
        let mut terminal = NativePlanTerminal {
            actions: Vec::new(),
            columns: width,
            rows: height,
        };
        self.inner
            .render_frame(&mut terminal, &frame, force)
            .map_err(|error| {
                Error::new(
                    Status::GenericFailure,
                    format!("main-screen render failed: {error:?}"),
                )
            })?;
        Ok(terminal.actions)
    }

    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset_render_state();
    }

    #[napi]
    pub fn set_stopped(&mut self, stopped: bool) {
        self.inner.set_stopped(stopped);
    }

    #[napi(getter)]
    pub fn full_redraws(&self) -> u32 {
        self.inner.full_redraws().min(u32::MAX as usize) as u32
    }

    #[napi]
    pub fn capture(&self) -> NativeMainScreenRenderState {
        self.inner.capture_render_state().into()
    }

    #[napi]
    pub fn restore(&mut self, state: NativeMainScreenRenderState) {
        self.inner.restore_render_state(state.into());
    }
}

#[cfg(test)]
mod screen_planner_tests {
    use super::{NativeAltScreenPlanner, NativeMainScreenPlanner};
    use napi::Status;

    const RESET: &str = "\x1b[0m\x1b]8;;\x07";
    const CURSOR_MARKER: &str = "\x1b_pi:c\x07";

    fn action_pairs(actions: &[super::NativeTerminalAction]) -> Vec<(&str, Option<&str>)> {
        actions
            .iter()
            .map(|action| (action.kind.as_str(), action.data.as_deref()))
            .collect()
    }

    #[test]
    fn alt_planner_emits_exact_full_and_differential_frames() {
        let mut planner = NativeAltScreenPlanner::new();

        let initial = planner.render(vec!["one".into(), "two".into()], 8, 2);
        assert_eq!(
            initial,
            "\x1b[?2026h\x1b[2J\x1b[1;1H\x1b[2Kone\x1b[0m\x1b]8;;\x07\x1b[2;1H\x1b[2Ktwo\x1b[0m\x1b]8;;\x07\x1b[?25l\x1b[?2026l"
        );
        assert_eq!(planner.full_redraws(), 1);

        assert_eq!(
            planner.render(vec!["one".into(), "two".into()], 8, 2),
            "\x1b[?2026h\x1b[?25l\x1b[?2026l"
        );
        assert_eq!(planner.full_redraws(), 1);

        assert_eq!(
            planner.render(vec!["one".into(), "TWO".into()], 8, 2),
            "\x1b[?2026h\x1b[2;1H\x1b[2KTWO\x1b[0m\x1b]8;;\x07\x1b[?25l\x1b[?2026l"
        );
        assert_eq!(planner.full_redraws(), 1);
    }

    #[test]
    fn alt_planner_resize_reset_clamp_and_truncation_are_stateful() {
        let mut planner = NativeAltScreenPlanner::new();
        planner.render(vec!["a".into(), "b".into(), "discarded".into()], 3, 2);
        assert_eq!(planner.previous_screen, vec!["a", "b"]);
        assert_eq!(planner.full_redraws(), 1);

        let resized = planner.render(vec!["a".into(), "b".into(), "discarded".into()], 4, 2);
        assert!(resized.starts_with("\x1b[?2026h\x1b[2J"));
        assert!(!resized.contains("discarded"));
        assert_eq!(planner.full_redraws(), 2);

        planner.reset();
        assert!(planner.previous_screen.is_empty());
        assert_eq!((planner.previous_width, planner.previous_height), (0, 0));
        let after_reset = planner.render(vec!["z".into(), "discarded".into()], 0, 0);
        assert_eq!(
            after_reset,
            "\x1b[?2026h\x1b[2J\x1b[1;1H\x1b[2Kz\x1b[0m\x1b]8;;\x07\x1b[?25l\x1b[?2026l"
        );
        assert_eq!(planner.previous_screen, vec!["z"]);
        assert_eq!((planner.previous_width, planner.previous_height), (1, 1));
        assert_eq!(planner.full_redraws(), 3);
    }

    #[test]
    fn main_planner_preserves_exact_action_order_and_restorable_state() {
        let mut planner = NativeMainScreenPlanner::new();
        let lines = vec!["one".into(), format!("t{CURSOR_MARKER}")];
        let actions = planner
            .render(lines.clone(), 8, 3, false, false, true)
            .expect("initial frame is valid");
        let initial_write = format!("\x1b[?2026hone{RESET}\r\nt{RESET}\x1b[?2026l");
        assert_eq!(
            action_pairs(&actions),
            vec![
                ("write", Some(initial_write.as_str())),
                ("write", Some("\x1b[2G")),
                ("showCursor", None),
            ]
        );
        assert_eq!(planner.full_redraws(), 1);

        let state = planner.capture();
        assert_eq!(
            state.previous_lines,
            vec![format!("one{RESET}"), format!("t{RESET}")]
        );
        assert_eq!((state.previous_width, state.previous_height), (8, 3));
        assert_eq!((state.cursor_row, state.hardware_cursor_row), (1, 1));
        assert_eq!(state.max_lines_rendered, 2);

        let unchanged = planner
            .render(lines.clone(), 8, 3, false, false, true)
            .expect("unchanged frame is valid");
        assert_eq!(
            action_pairs(&unchanged),
            vec![("write", Some("\x1b[2G")), ("showCursor", None)]
        );
        assert_eq!(planner.full_redraws(), 1);

        let mut restored = NativeMainScreenPlanner::new();
        restored.restore(state);
        let restored_actions = restored
            .render(lines, 8, 3, false, false, true)
            .expect("restored frame is valid");
        assert_eq!(
            action_pairs(&restored_actions),
            vec![("write", Some("\x1b[2G")), ("showCursor", None)]
        );
        assert_eq!(restored.full_redraws(), 0);
    }

    #[test]
    fn main_planner_reset_stopped_clamp_and_invalid_frame_are_bounded() {
        let mut planner = NativeMainScreenPlanner::new();
        planner
            .render(vec!["ok".into()], 4, 2, false, false, false)
            .expect("seed frame is valid");
        assert_eq!(planner.full_redraws(), 1);

        planner.set_stopped(true);
        assert!(
            planner
                .render(vec!["held".into()], 4, 2, true, false, false)
                .expect("stopped planner accepts a frame")
                .is_empty()
        );
        assert_eq!(planner.capture().previous_lines, vec![format!("ok{RESET}")]);

        planner.set_stopped(false);
        planner.reset();
        let reset_state = planner.capture();
        assert!(reset_state.previous_lines.is_empty());
        assert_eq!(
            (reset_state.previous_width, reset_state.previous_height),
            (-1, -1)
        );
        let reset_actions = planner
            .render(Vec::new(), 0, 0, false, false, false)
            .expect("zero dimensions clamp to one cell");
        assert_eq!(
            action_pairs(&reset_actions),
            vec![
                ("write", Some("\x1b[?2026h\x1b[2J\x1b[H\x1b[3J\x1b[?2026l")),
                ("hideCursor", None),
            ]
        );
        assert_eq!(
            (
                planner.capture().previous_width,
                planner.capture().previous_height
            ),
            (1, 1)
        );
        assert_eq!(planner.full_redraws(), 2);

        let state_before_error = planner.capture();
        let redraws_before_error = planner.full_redraws();
        let error = match planner.render(vec!["xx".into()], 0, 0, false, false, false) {
            Ok(_) => panic!("two visible cells must not fit the clamped width"),
            Err(error) => error,
        };
        assert_eq!(error.status, Status::InvalidArg);
        assert!(error.reason.contains("invalid logical frame: LineTooWide"));
        assert_eq!(
            planner.capture().previous_lines,
            state_before_error.previous_lines
        );
        assert_eq!(planner.full_redraws(), redraws_before_error);
    }
}
