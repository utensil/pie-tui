//! Main-screen ANSI planner/executor — the `TuiMainScreen` terminal seam.
//!
//! Faithful port of the pinned pi-tui `dist/tui-main-screen.js` doRender
//! pipeline (scrollback-preserving, CSI 2026 synchronized output). Pure
//! logical-frame construction and diff classification live in `pie-core`;
//! scheduling and component-tree ownership live in `pie-app`.

use pie_core::frame::{FrameDiff, LogicalFrame, LogicalFrameError};
use pie_core::screen::{CursorPos, is_image_line};
use pie_core::terminal_image::delete_kitty_image;
use pie_core::text::visible_width;

use crate::Terminal;

/// State carried across renders (reference `captureRenderState` shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderState {
    pub previous_lines: Vec<String>,
    pub previous_width: isize,
    pub previous_height: isize,
    pub cursor_row: isize,
    pub hardware_cursor_row: isize,
    pub max_lines_rendered: usize,
    pub previous_viewport_top: usize,
}

/// Error surface of the renderer. The reference crashes the process when a
/// rendered line exceeds the terminal width; we return an error instead so
/// tests and embedders can decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    LineTooWide {
        index: usize,
        visible: usize,
        width: usize,
    },
    TerminalGeometryChanged {
        frame_width: usize,
        frame_height: usize,
        terminal_width: usize,
        terminal_height: usize,
    },
}

impl From<LogicalFrameError> for RenderError {
    fn from(error: LogicalFrameError) -> Self {
        match error {
            LogicalFrameError::LineTooWide {
                index,
                visible,
                width,
            } => Self::LineTooWide {
                index,
                visible,
                width,
            },
        }
    }
}

/// What produces content lines for a frame (the `render(width)` seam; the
/// component tree plugs in here in M3).
pub trait LineSource {
    fn render_lines(&mut self, width: usize) -> Vec<String>;
}

/// Simple fixed-lines source for tests and the golden runner.
pub struct StaticLines(pub Vec<String>);

impl LineSource for StaticLines {
    fn render_lines(&mut self, _width: usize) -> Vec<String> {
        self.0.clone()
    }
}

/// A complete, already-validated terminal emission transaction.
///
/// Writes remain split exactly as the reference terminal calls split them.
/// The renderer state is committed only after all infallible `Terminal::write`
/// calls have been issued.
#[derive(Debug, Clone)]
pub struct AnsiRenderPlan {
    writes: Vec<String>,
    actions: Vec<PlannedTerminalAction>,
    next_renderer: MainScreenRenderer,
}

#[derive(Debug, Clone)]
enum PlannedTerminalAction {
    Write(String),
    HideCursor,
    ShowCursor,
}

impl AnsiRenderPlan {
    pub fn writes(&self) -> &[String] {
        &self.writes
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

/// Main-screen terminal planner and state owner.
#[derive(Debug, Clone)]
pub struct MainScreenRenderer {
    previous_lines: Vec<String>,
    previous_kitty_image_ids: Vec<u32>,
    previous_width: isize,
    previous_height: isize,
    cursor_row: isize,
    hardware_cursor_row: isize,
    max_lines_rendered: usize,
    previous_viewport_top: usize,
    stopped: bool,
    /// Reference env `PI_CLEAR_ON_SHRINK=1` (default off).
    clear_on_shrink: bool,
    /// Reference env `PI_HARDWARE_CURSOR=1` (default off).
    show_hardware_cursor: bool,
    full_redraw_count: usize,
}

impl Default for MainScreenRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MainScreenRenderer {
    /// Constructor state mirrors the reference field initializers
    /// (`previousWidth/Height = 0`, `previousLines = []`).
    pub fn new() -> Self {
        MainScreenRenderer {
            previous_lines: Vec::new(),
            previous_kitty_image_ids: Vec::new(),
            previous_width: 0,
            previous_height: 0,
            cursor_row: 0,
            hardware_cursor_row: 0,
            max_lines_rendered: 0,
            previous_viewport_top: 0,
            stopped: false,
            clear_on_shrink: false,
            show_hardware_cursor: false,
            full_redraw_count: 0,
        }
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn set_stopped(&mut self, stopped: bool) {
        self.stopped = stopped;
    }

    pub fn set_clear_on_shrink(&mut self, enabled: bool) {
        self.clear_on_shrink = enabled;
    }

    pub fn set_show_hardware_cursor(&mut self, enabled: bool) {
        self.show_hardware_cursor = enabled;
    }

    pub fn full_redraws(&self) -> usize {
        self.full_redraw_count
    }

    /// Reference `resetRenderState` (note the -1 sentinels — they make the
    /// next render take the width/height-changed full-render path).
    pub fn reset_render_state(&mut self) {
        self.previous_lines = Vec::new();
        self.previous_width = -1;
        self.previous_height = -1;
        self.cursor_row = 0;
        self.hardware_cursor_row = 0;
        self.max_lines_rendered = 0;
        self.previous_viewport_top = 0;
    }

    pub fn capture_render_state(&self) -> RenderState {
        RenderState {
            previous_lines: self.previous_lines.clone(),
            previous_width: self.previous_width,
            previous_height: self.previous_height,
            cursor_row: self.cursor_row,
            hardware_cursor_row: self.hardware_cursor_row,
            max_lines_rendered: self.max_lines_rendered,
            previous_viewport_top: self.previous_viewport_top,
        }
    }

    pub fn restore_render_state(&mut self, state: RenderState) {
        self.previous_lines = state
            .previous_lines
            .into_iter()
            .map(|line| {
                if is_image_line(&line) {
                    String::new()
                } else {
                    line
                }
            })
            .collect();
        self.previous_kitty_image_ids.clear();
        self.previous_width = state.previous_width;
        self.previous_height = state.previous_height;
        self.cursor_row = state.cursor_row;
        self.hardware_cursor_row = state.hardware_cursor_row;
        self.max_lines_rendered = state.max_lines_rendered;
        self.previous_viewport_top = state.previous_viewport_top;
    }

    /// Compatibility wrapper for the pre-M4 `render_now` surface.
    ///
    /// New application code should build frames in its controller and call
    /// [`Self::render_frame`].  Keeping this wrapper makes the pinned M2
    /// differential runner and existing embedders source-compatible.
    pub fn render_now(
        &mut self,
        term: &mut dyn Terminal,
        source: &mut dyn LineSource,
        force: bool,
    ) -> Result<(), RenderError> {
        if self.stopped {
            // Legacy TuiBase::renderNow(force) resets before doRender observes
            // `stopped`.  Preserve that wrapper-only ordering without making
            // the frame/planner APIs mutate stopped state.
            if force {
                self.reset_render_state();
            }
            return Ok(());
        }
        let width = term.columns();
        let height = term.rows();
        let frame = LogicalFrame::new(source.render_lines(width), width, height)?;
        self.render_frame(term, &frame, force)
    }

    /// Validate, plan, emit, and atomically advance renderer state for one
    /// logical frame.
    pub fn render_frame(
        &mut self,
        term: &mut dyn Terminal,
        frame: &LogicalFrame,
        force: bool,
    ) -> Result<(), RenderError> {
        if self.stopped {
            return Ok(());
        }

        frame.validate()?;
        let terminal_width = term.columns();
        let terminal_height = term.rows();
        if (frame.width(), frame.height()) != (terminal_width, terminal_height) {
            return Err(RenderError::TerminalGeometryChanged {
                frame_width: frame.width(),
                frame_height: frame.height(),
                terminal_width,
                terminal_height,
            });
        }

        let plan = self.plan_frame(frame, force)?;
        for action in &plan.actions {
            match action {
                PlannedTerminalAction::Write(write) => term.write(write),
                PlannedTerminalAction::HideCursor => term.hide_cursor(),
                PlannedTerminalAction::ShowCursor => term.show_cursor(),
            }
        }
        *self = plan.next_renderer;
        Ok(())
    }

    /// Produce exact ANSI write batches and the next renderer state without
    /// touching a real terminal or mutating `self`.
    pub fn plan_frame(
        &self,
        frame: &LogicalFrame,
        force: bool,
    ) -> Result<AnsiRenderPlan, RenderError> {
        if self.stopped {
            return Ok(AnsiRenderPlan {
                writes: Vec::new(),
                actions: Vec::new(),
                next_renderer: self.clone(),
            });
        }
        frame.validate()?;

        let mut next_renderer = self.clone();
        if force {
            next_renderer.reset_render_state();
        }
        let mut terminal = PlanTerminal::new(frame.width(), frame.height());
        next_renderer.render_frame_unchecked(&mut terminal, frame)?;
        Ok(AnsiRenderPlan {
            writes: terminal.writes,
            actions: terminal.actions,
            next_renderer,
        })
    }

    fn render_frame_unchecked(
        &mut self,
        term: &mut dyn Terminal,
        frame: &LogicalFrame,
    ) -> Result<(), RenderError> {
        let width = frame.width();
        let height = frame.height();
        let width_changed = self.previous_width != 0 && self.previous_width != width as isize;
        let height_changed = self.previous_height != 0 && self.previous_height != height as isize;
        let previous_buffer_length = if self.previous_height > 0 {
            self.previous_viewport_top + self.previous_height as usize
        } else {
            height
        };
        let mut prev_viewport_top = if height_changed {
            previous_buffer_length.saturating_sub(height)
        } else {
            self.previous_viewport_top
        };
        let mut viewport_top = prev_viewport_top;
        let mut hardware_cursor_row = self.hardware_cursor_row;

        let compute_line_diff =
            move |prev_vt: usize, vt: usize, hw_row: isize, target_row: usize| -> isize {
                let current_screen_row = hw_row - prev_vt as isize;
                let target_screen_row = target_row as isize - vt as isize;
                target_screen_row - current_screen_row
            };

        let new_lines = frame.lines().to_vec();
        let cursor_pos = frame.cursor();

        // Clear scrollback and viewport and render all new lines.

        // First render — output everything without clearing (assumes clean screen).
        if self.previous_lines.is_empty() && !width_changed && !height_changed {
            return self.full_render(term, &new_lines, false, width, height, cursor_pos);
        }

        // Width changes always need a full re-render because wrapping changes.
        if width_changed {
            return self.full_render(term, &new_lines, true, width, height, cursor_pos);
        }

        // Height changes normally need a full re-render to keep the visible
        // viewport aligned (Termux keyboard exception not modeled; env-clean).
        if height_changed {
            return self.full_render(term, &new_lines, true, width, height, cursor_pos);
        }

        // Content shrunk below the working area and no overlays.
        if self.clear_on_shrink && new_lines.len() < self.max_lines_rendered {
            return self.full_render(term, &new_lines, true, width, height, cursor_pos);
        }

        let FrameDiff {
            first_changed,
            last_changed,
            append_start,
            ..
        } = frame.diff_from(&self.previous_lines);

        // No changes — still update hardware cursor position if it moved.
        let Some(mut first_changed) = first_changed else {
            self.position_hardware_cursor(term, cursor_pos, new_lines.len());
            self.previous_viewport_top = prev_viewport_top;
            self.previous_height = height as isize;
            return Ok(());
        };
        let mut last_changed = last_changed.expect("a changed frame has a last changed line");
        (first_changed, last_changed) =
            self.expand_changed_range_for_kitty_images(first_changed, last_changed, &new_lines);

        // All changes are in deleted lines (nothing to render, just clear).
        if first_changed >= new_lines.len() {
            if self.previous_lines.len() > new_lines.len() {
                let mut buffer = String::from("\x1b[?2026h");
                buffer.push_str(&self.delete_changed_kitty_images(first_changed, last_changed));
                let target_row = new_lines.len().saturating_sub(1);
                if target_row < prev_viewport_top {
                    return self.full_render(term, &new_lines, true, width, height, cursor_pos);
                }
                let line_diff = compute_line_diff(
                    prev_viewport_top,
                    viewport_top,
                    hardware_cursor_row,
                    target_row,
                );
                if line_diff > 0 {
                    buffer.push_str(&format!("\x1b[{line_diff}B"));
                } else if line_diff < 0 {
                    buffer.push_str(&format!("\x1b[{}A", -line_diff));
                }
                buffer.push('\r');
                // Clear extra lines without scrolling.
                let extra_lines = self.previous_lines.len() - new_lines.len();
                if extra_lines > height {
                    return self.full_render(term, &new_lines, true, width, height, cursor_pos);
                }
                let clear_start_offset = if new_lines.is_empty() { 0 } else { 1 };
                if extra_lines > 0 && clear_start_offset > 0 {
                    buffer.push_str(&format!("\x1b[{clear_start_offset}B"));
                }
                for i in 0..extra_lines {
                    buffer.push_str("\r\x1b[2K");
                    if i + 1 < extra_lines {
                        buffer.push_str("\x1b[1B");
                    }
                }
                let move_back =
                    (extra_lines as isize - 1 + clear_start_offset as isize).max(0) as usize;
                if move_back > 0 {
                    buffer.push_str(&format!("\x1b[{move_back}A"));
                }
                buffer.push_str("\x1b[?2026l");
                term.write(&buffer);
                self.cursor_row = target_row as isize;
                self.hardware_cursor_row = target_row as isize;
            }
            self.position_hardware_cursor(term, cursor_pos, new_lines.len());
            self.previous_lines = new_lines;
            self.previous_kitty_image_ids = self.collect_kitty_image_ids(&self.previous_lines);
            self.previous_width = width as isize;
            self.previous_height = height as isize;
            self.previous_viewport_top = prev_viewport_top;
            return Ok(());
        }

        // Differential rendering can only touch what was actually visible.
        if first_changed < prev_viewport_top {
            return self.full_render(term, &new_lines, true, width, height, cursor_pos);
        }

        // Render from first changed line to last changed line, synced.
        let mut buffer = String::from("\x1b[?2026h");
        buffer.push_str(&self.delete_changed_kitty_images(first_changed, last_changed));
        let prev_viewport_bottom = prev_viewport_top + height - 1;
        let move_target_row = if append_start {
            first_changed - 1
        } else {
            first_changed
        };
        if move_target_row > prev_viewport_bottom {
            let current_screen_row = (hardware_cursor_row - prev_viewport_top as isize)
                .clamp(0, height as isize - 1) as usize;
            let move_to_bottom = (height - 1).saturating_sub(current_screen_row);
            if move_to_bottom > 0 {
                buffer.push_str(&format!("\x1b[{move_to_bottom}B"));
            }
            let scroll = move_target_row - prev_viewport_bottom;
            buffer.push_str(&"\r\n".repeat(scroll));
            prev_viewport_top += scroll;
            viewport_top += scroll;
            hardware_cursor_row = move_target_row as isize;
        }

        // Move cursor to first changed line.
        let line_diff = compute_line_diff(
            prev_viewport_top,
            viewport_top,
            hardware_cursor_row,
            move_target_row,
        );
        if line_diff > 0 {
            buffer.push_str(&format!("\x1b[{line_diff}B"));
        } else if line_diff < 0 {
            buffer.push_str(&format!("\x1b[{}A", -line_diff));
        }
        buffer.push_str(if append_start { "\r\n" } else { "\r" });

        let render_end = last_changed.min(new_lines.len().saturating_sub(1));
        let mut i = first_changed;
        while i <= render_end {
            if i > first_changed {
                buffer.push_str("\r\n");
            }
            let line = &new_lines[i];
            let image_reserved_rows = if is_image_line(line) {
                self.kitty_image_reserved_rows(&new_lines, i, render_end)
            } else {
                1
            };
            if image_reserved_rows > 1 {
                let image_start_screen_row = i as isize - viewport_top as isize;
                if image_start_screen_row < 0
                    || image_start_screen_row + image_reserved_rows as isize > height as isize
                {
                    return self.full_render(term, &new_lines, true, width, height, cursor_pos);
                }
                buffer.push_str("\x1b[2K");
                for _ in 1..image_reserved_rows {
                    buffer.push_str("\r\n\x1b[2K");
                }
                buffer.push_str(&format!("\x1b[{}A", image_reserved_rows - 1));
                buffer.push_str(line);
                buffer.push_str(&format!("\x1b[{}B", image_reserved_rows - 1));
                i += image_reserved_rows;
                continue;
            }
            buffer.push_str("\x1b[2K");
            buffer.push_str(line);
            i += 1;
        }

        // Track where the cursor ended up after rendering.
        let mut final_cursor_row = render_end;

        // If we had more lines before, clear them and move cursor back.
        if self.previous_lines.len() > new_lines.len() {
            if render_end + 1 < new_lines.len() {
                let move_down = new_lines.len() - 1 - render_end;
                buffer.push_str(&format!("\x1b[{move_down}B"));
                final_cursor_row = new_lines.len() - 1;
            }
            let extra_lines = self.previous_lines.len() - new_lines.len();
            for _ in new_lines.len()..self.previous_lines.len() {
                buffer.push_str("\r\n\x1b[2K");
            }
            buffer.push_str(&format!("\x1b[{extra_lines}A"));
        }
        buffer.push_str("\x1b[?2026l");

        term.write(&buffer);

        self.cursor_row = (new_lines.len() as isize - 1).max(0);
        self.hardware_cursor_row = final_cursor_row as isize;
        self.max_lines_rendered = self.max_lines_rendered.max(new_lines.len());
        self.previous_viewport_top = (prev_viewport_top as isize)
            .max(final_cursor_row as isize - height as isize + 1)
            .max(0) as usize;
        self.position_hardware_cursor(term, cursor_pos, new_lines.len());
        self.previous_lines = new_lines;
        self.previous_kitty_image_ids = self.collect_kitty_image_ids(&self.previous_lines);
        self.previous_width = width as isize;
        self.previous_height = height as isize;
        Ok(())
    }

    fn full_render(
        &mut self,
        term: &mut dyn Terminal,
        new_lines: &[String],
        clear: bool,
        width: usize,
        height: usize,
        cursor_pos: Option<CursorPos>,
    ) -> Result<(), RenderError> {
        self.full_redraw_count += 1;
        let mut buffer = String::from("\x1b[?2026h");
        if clear {
            buffer.push_str(&self.delete_kitty_images(&self.previous_kitty_image_ids));
            buffer.push_str("\x1b[2J\x1b[H\x1b[3J");
        }
        let mut i = 0usize;
        while i < new_lines.len() {
            if i > 0 {
                buffer.push_str("\r\n");
            }
            let line = &new_lines[i];
            let image_reserved_rows = if is_image_line(line) {
                self.kitty_image_reserved_rows(new_lines, i, new_lines.len().saturating_sub(1))
            } else {
                1
            };
            if image_reserved_rows > 1 && image_reserved_rows <= height {
                for _ in 1..image_reserved_rows {
                    buffer.push_str("\r\n");
                }
                buffer.push_str(&format!("\x1b[{}A", image_reserved_rows - 1));
                buffer.push_str(line);
                buffer.push_str(&format!("\x1b[{}B", image_reserved_rows - 1));
                i += image_reserved_rows;
                continue;
            }
            buffer.push_str(line);
            i += 1;
        }
        buffer.push_str("\x1b[?2026l");
        term.write(&buffer);
        self.cursor_row = (new_lines.len() as isize - 1).max(0);
        self.hardware_cursor_row = self.cursor_row;
        if clear {
            self.max_lines_rendered = new_lines.len();
        } else {
            self.max_lines_rendered = self.max_lines_rendered.max(new_lines.len());
        }
        let buffer_length = height.max(new_lines.len());
        self.previous_viewport_top = buffer_length.saturating_sub(height);
        self.position_hardware_cursor(term, cursor_pos, new_lines.len());
        self.previous_lines = new_lines.to_vec();
        self.previous_kitty_image_ids = self.collect_kitty_image_ids(new_lines);
        self.previous_width = width as isize;
        self.previous_height = height as isize;
        Ok(())
    }

    fn collect_kitty_image_ids(&self, lines: &[String]) -> Vec<u32> {
        let mut ids = Vec::new();
        for line in lines {
            for id in parse_kitty_image_header(line).ids {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    fn delete_kitty_images(&self, ids: &[u32]) -> String {
        ids.iter()
            .map(|id| delete_kitty_image(*id))
            .collect::<String>()
    }

    fn kitty_image_reserved_rows(&self, lines: &[String], index: usize, max_index: usize) -> usize {
        let rows = lines
            .get(index)
            .map(|line| parse_kitty_image_header(line).rows)
            .unwrap_or(1);
        if rows <= 1 {
            return 1;
        }
        let max_rows = rows
            .min(max_index.saturating_sub(index).saturating_add(1))
            .min(lines.len().saturating_sub(index));
        let mut reserved_rows = 1;
        while reserved_rows < max_rows {
            let line = lines.get(index + reserved_rows).map_or("", String::as_str);
            if is_image_line(line) || visible_width(line) > 0 {
                break;
            }
            reserved_rows += 1;
        }
        reserved_rows
    }

    fn expand_changed_range_for_kitty_images(
        &self,
        first_changed: usize,
        last_changed: usize,
        new_lines: &[String],
    ) -> (usize, usize) {
        let mut expanded_first = first_changed;
        let mut expanded_last = last_changed;
        for lines in [&self.previous_lines[..], new_lines] {
            for (index, line) in lines.iter().enumerate() {
                if parse_kitty_image_header(line).ids.is_empty() {
                    continue;
                }
                let block_end = index
                    + self.kitty_image_reserved_rows(lines, index, lines.len().saturating_sub(1))
                    - 1;
                if index >= first_changed || (index <= last_changed && block_end >= first_changed) {
                    expanded_first = expanded_first.min(index);
                    expanded_last = expanded_last.max(block_end);
                }
            }
        }
        (expanded_first, expanded_last)
    }

    fn delete_changed_kitty_images(&self, first_changed: usize, last_changed: usize) -> String {
        if last_changed < first_changed {
            return String::new();
        }
        let mut ids = Vec::new();
        let max_line = last_changed.min(self.previous_lines.len().saturating_sub(1));
        for index in first_changed..=max_line {
            if let Some(line) = self.previous_lines.get(index) {
                for id in parse_kitty_image_header(line).ids {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        self.delete_kitty_images(&ids)
    }

    /// Position the hardware cursor for IME (reference
    /// `positionHardwareCursor`).
    fn position_hardware_cursor(
        &mut self,
        term: &mut dyn Terminal,
        cursor_pos: Option<CursorPos>,
        total_lines: usize,
    ) {
        let Some(pos) = cursor_pos else {
            term.hide_cursor();
            return;
        };
        if total_lines == 0 {
            term.hide_cursor();
            return;
        }
        let target_row = pos.row.min(total_lines - 1);
        let target_col = pos.col;
        let row_delta = target_row as isize - self.hardware_cursor_row;
        let mut buffer = String::new();
        if row_delta > 0 {
            buffer.push_str(&format!("\x1b[{row_delta}B"));
        } else if row_delta < 0 {
            buffer.push_str(&format!("\x1b[{}A", -row_delta));
        }
        buffer.push_str(&format!("\x1b[{}G", target_col + 1));
        if !buffer.is_empty() {
            term.write(&buffer);
        }
        self.hardware_cursor_row = target_row as isize;
        if self.show_hardware_cursor {
            term.show_cursor();
        } else {
            term.hide_cursor();
        }
    }
}

#[derive(Default)]
struct KittyImageHeader {
    ids: Vec<u32>,
    rows: usize,
}

fn parse_kitty_image_header(line: &str) -> KittyImageHeader {
    let Some(sequence_start) = line.find("\x1b_G") else {
        return KittyImageHeader {
            rows: 1,
            ..KittyImageHeader::default()
        };
    };
    let params_start = sequence_start + "\x1b_G".len();
    let Some(relative_end) = line[params_start..].find(';') else {
        return KittyImageHeader {
            rows: 1,
            ..KittyImageHeader::default()
        };
    };
    let params_end = params_start + relative_end;
    let mut header = KittyImageHeader {
        rows: 1,
        ..KittyImageHeader::default()
    };
    for param in line[params_start..params_end].split(',') {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        let Ok(value) = value.parse::<u32>() else {
            continue;
        };
        if value == 0 {
            continue;
        }
        match key {
            "i" => header.ids.push(value),
            "r" => header.rows = value as usize,
            _ => {}
        }
    }
    header
}

/// Terminal-shaped byte collector used only while preparing an ANSI plan.
/// It mirrors the write boundaries of a real backend without performing I/O.
struct PlanTerminal {
    writes: Vec<String>,
    actions: Vec<PlannedTerminalAction>,
    width: usize,
    height: usize,
}

impl PlanTerminal {
    fn new(width: usize, height: usize) -> Self {
        Self {
            writes: Vec::new(),
            actions: Vec::new(),
            width,
            height,
        }
    }
}

impl Terminal for PlanTerminal {
    fn start(&mut self, _on_input: crate::InputHandler, _on_resize: crate::ResizeHandler) {}

    fn stop(&mut self) {}

    fn write(&mut self, data: &str) {
        self.writes.push(data.to_string());
        self.actions
            .push(PlannedTerminalAction::Write(data.to_string()));
    }

    fn columns(&self) -> usize {
        self.width
    }

    fn rows(&self) -> usize {
        self.height
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
        self.writes.push("\x1b[?25l".into());
        self.actions.push(PlannedTerminalAction::HideCursor);
    }

    fn show_cursor(&mut self) {
        self.writes.push("\x1b[?25h".into());
        self.actions.push(PlannedTerminalAction::ShowCursor);
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
        if active {
            self.write("\x1b]9;4;3\x07");
        } else {
            self.write("\x1b]9;4;0\x07");
        }
    }
}
