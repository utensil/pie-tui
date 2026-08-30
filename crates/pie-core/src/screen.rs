//! Screen-level line helpers shared by the renderers: the zero-width cursor
//! marker, segment resets applied to every rendered line, cursor-position
//! extraction, and the overlay compositing primitive.
//!
//! Ported from the pinned pi-tui `dist/tui.js`.

use crate::text::visible_width;
use crate::wrap::{extract_segments, normalize_terminal_output, slice_by_column, slice_with_width};

/// Zero-width APC marker components emit at the hardware-cursor position
/// (reference `CURSOR_MARKER`).
pub const CURSOR_MARKER: &str = "\x1b_pi:c\x07";

/// Reset applied between composited segments and at every line end
/// (reference `SEGMENT_RESET`).
pub const SEGMENT_RESET: &str = "\x1b[0m\x1b]8;;\x07";

/// Kitty graphics prefix — a line containing it is an image placeholder row
/// (reference `isImageLine`; image encode/decode lands in M5).
pub fn is_image_line(line: &str) -> bool {
    line.starts_with("\x1b_G") || line.contains("\x1b_G") || line.contains("\x1b]1337;File=")
}

/// Rendered cursor position (0-based row within the full line list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
}

/// Find CURSOR_MARKER scanning the bottom `height` lines bottom-up; strip it
/// from the line and return the position (reference `extractCursorPosition`).
pub fn extract_cursor_position(lines: &mut [String], height: usize) -> Option<CursorPos> {
    if lines.is_empty() {
        return None;
    }
    let viewport_top = lines.len().saturating_sub(height);
    for row in (viewport_top..lines.len()).rev() {
        let line = &lines[row];
        if let Some(marker_index) = line.find(CURSOR_MARKER) {
            let before_marker = &line[..marker_index];
            let col = visible_width(before_marker);
            let stripped = format!(
                "{}{}",
                &line[..marker_index],
                &line[marker_index + CURSOR_MARKER.len()..]
            );
            lines[row] = stripped;
            return Some(CursorPos { row, col });
        }
    }
    None
}

/// Append the segment reset to every (non-image) line after normalizing
/// terminal output (reference `applyLineResets`).
pub fn apply_line_resets(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| {
            if is_image_line(&line) {
                line
            } else {
                format!("{}{}", normalize_terminal_output(&line), SEGMENT_RESET)
            }
        })
        .collect()
}

/// Composite overlay content into a base terminal line at a fixed column
/// (reference `compositeTuiLine`).
pub fn composite_tui_line(
    base_line: &str,
    overlay_line: &str,
    start_col: usize,
    overlay_width: usize,
    total_width: usize,
) -> String {
    if is_image_line(base_line) {
        return base_line.to_string();
    }
    let after_start = start_col + overlay_width;
    let base = extract_segments(
        base_line,
        start_col,
        after_start,
        total_width.saturating_sub(after_start),
        true,
    );
    let overlay = slice_with_width(overlay_line, 0, overlay_width, true);
    let before_pad = start_col.saturating_sub(base.before_width);
    let overlay_pad = overlay_width.saturating_sub(overlay.width);
    let actual_before_width = start_col.max(base.before_width);
    let actual_overlay_width = overlay_width.max(overlay.width);
    let after_target = total_width.saturating_sub(actual_before_width + actual_overlay_width);
    let after_pad = after_target.saturating_sub(base.after_width);
    let result = format!(
        "{}{}{}{}{}{}{}{}",
        base.before,
        " ".repeat(before_pad),
        SEGMENT_RESET,
        overlay.text,
        " ".repeat(overlay_pad),
        SEGMENT_RESET,
        base.after,
        " ".repeat(after_pad),
    );
    if visible_width(&result) <= total_width {
        result
    } else {
        slice_by_column(&result, 0, total_width, true)
    }
}
