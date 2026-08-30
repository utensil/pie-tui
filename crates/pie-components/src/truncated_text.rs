//! TruncatedText — single-line text truncated to the viewport width
//! (reference `components/truncated-text.js`).

use pie_core::text::visible_width;
use pie_core::wrap::truncate_to_width;

use crate::Component;

/// Text component that truncates to fit viewport width.
#[derive(Default)]
pub struct TruncatedText {
    text: String,
    padding_x: usize,
    padding_y: usize,
}

impl TruncatedText {
    pub fn new(text: impl Into<String>) -> Self {
        TruncatedText {
            text: text.into(),
            padding_x: 0,
            padding_y: 0,
        }
    }

    pub fn with_padding(text: impl Into<String>, padding_x: usize, padding_y: usize) -> Self {
        TruncatedText {
            text: text.into(),
            padding_x,
            padding_y,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Component for TruncatedText {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut result = Vec::new();
        let empty_line = " ".repeat(width);
        for _ in 0..self.padding_y {
            result.push(empty_line.clone());
        }
        let available_width = (width.saturating_sub(self.padding_x * 2)).max(1);
        // Take only the first line (stop at newline).
        let single_line_text = match self.text.find('\n') {
            Some(idx) => &self.text[..idx],
            None => &self.text[..],
        };
        let display_text = truncate_to_width(single_line_text, available_width, "...", false);
        let left_padding = " ".repeat(self.padding_x);
        let right_padding = " ".repeat(self.padding_x);
        let line_with_padding = format!("{left_padding}{display_text}{right_padding}");
        let line_visible_width = visible_width(&line_with_padding);
        let padding_needed = width.saturating_sub(line_visible_width);
        result.push(format!("{line_with_padding}{}", " ".repeat(padding_needed)));
        for _ in 0..self.padding_y {
            result.push(empty_line.clone());
        }
        result
    }
}
