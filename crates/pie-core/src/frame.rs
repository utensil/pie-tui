//! Pure logical-frame construction and line-diff classification.
//!
//! This module deliberately stops at terminal-independent state.  It turns a
//! component render into normalized logical lines, extracts the hardware
//! cursor marker before line resets are appended, validates terminal width,
//! and classifies the smallest changed range.  ANSI byte planning and writes
//! belong to `pie-term`.

use crate::screen::{CursorPos, apply_line_resets, extract_cursor_position, is_image_line};
use crate::text::visible_width;

/// A complete component render after cursor extraction and line normalization.
///
/// Construction is sealed so terminal executors receive only lines processed
/// by the reference cursor-extraction and reset-normalization pipeline.
///
/// ```compile_fail
/// use pie_core::frame::LogicalFrame;
///
/// let mut frame = LogicalFrame::new(vec!["safe".into()], 20, 5).unwrap();
/// frame.lines.push("raw line bypassing normalization".into());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalFrame {
    lines: Vec<String>,
    width: usize,
    height: usize,
    cursor: Option<CursorPos>,
}

/// Pure validation failures discovered while constructing a logical frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalFrameError {
    LineTooWide {
        index: usize,
        visible: usize,
        width: usize,
    },
}

impl LogicalFrame {
    /// Build a frame from raw component lines.
    ///
    /// Cursor extraction must precede resets because the marker is an APC
    /// control sequence embedded in a rendered line.  Every non-image line is
    /// then normalized/reset and the whole frame is width-checked before it
    /// can reach a terminal planner.
    pub fn new(
        mut lines: Vec<String>,
        width: usize,
        height: usize,
    ) -> Result<Self, LogicalFrameError> {
        let cursor = extract_cursor_position(&mut lines, height);
        let frame = Self {
            lines: apply_line_resets(lines),
            width,
            height,
            cursor,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cursor(&self) -> Option<CursorPos> {
        self.cursor
    }

    /// Revalidate a frame before planning terminal output.
    pub fn validate(&self) -> Result<(), LogicalFrameError> {
        for (index, line) in self.lines.iter().enumerate() {
            if is_image_line(line) {
                continue;
            }
            let visible = visible_width(line);
            if visible > self.width {
                return Err(LogicalFrameError::LineTooWide {
                    index,
                    visible,
                    width: self.width,
                });
            }
        }
        Ok(())
    }

    pub fn diff_from(&self, previous_lines: &[String]) -> FrameDiff {
        FrameDiff::between(previous_lines, &self.lines)
    }
}

/// The minimal changed-line range between two normalized logical frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDiff {
    pub first_changed: Option<usize>,
    pub last_changed: Option<usize>,
    pub appended: bool,
    pub append_start: bool,
    pub deleted_only: bool,
}

impl FrameDiff {
    pub fn between(previous_lines: &[String], new_lines: &[String]) -> Self {
        let mut first_changed = None;
        let mut last_changed = None;
        let max_lines = new_lines.len().max(previous_lines.len());

        for index in 0..max_lines {
            let previous = previous_lines.get(index).map(String::as_str).unwrap_or("");
            let current = new_lines.get(index).map(String::as_str).unwrap_or("");
            if previous != current {
                first_changed.get_or_insert(index);
                last_changed = Some(index);
            }
        }

        let appended = new_lines.len() > previous_lines.len();
        if appended {
            first_changed.get_or_insert(previous_lines.len());
            last_changed = Some(new_lines.len() - 1);
        }

        let append_start =
            appended && first_changed == Some(previous_lines.len()) && !previous_lines.is_empty();
        let deleted_only = first_changed.is_some_and(|index| index >= new_lines.len());

        Self {
            first_changed,
            last_changed,
            appended,
            append_start,
            deleted_only,
        }
    }
}
