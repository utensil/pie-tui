//! Text — multi-line text display with word wrapping, padding, and optional
//! background (reference `components/text.js`).

use pie_core::text::visible_width;
use pie_core::wrap::{apply_background_to_line, wrap_text_with_ansi};

use crate::{Component, StyleFn};

#[derive(Default)]
struct TextCache {
    lines: Option<Vec<String>>,
    cached_text: String,
    cached_width: usize,
}

/// Text component — displays multi-line text with word wrapping.
#[derive(Default)]
pub struct Text {
    text: String,
    padding_x: usize,
    padding_y: usize,
    custom_bg_fn: Option<StyleFn>,
    cache: TextCache,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Text {
            text: text.into(),
            padding_x: 1,
            padding_y: 1,
            custom_bg_fn: None,
            cache: TextCache::default(),
        }
    }

    pub fn with_padding(text: impl Into<String>, padding_x: usize, padding_y: usize) -> Self {
        Text {
            text: text.into(),
            padding_x,
            padding_y,
            custom_bg_fn: None,
            cache: TextCache::default(),
        }
    }

    pub fn with_bg_fn(
        text: impl Into<String>,
        padding_x: usize,
        padding_y: usize,
        bg_fn: StyleFn,
    ) -> Self {
        Text {
            text: text.into(),
            padding_x,
            padding_y,
            custom_bg_fn: Some(bg_fn),
            cache: TextCache::default(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cache = TextCache::default();
    }

    pub fn set_custom_bg_fn(&mut self, bg_fn: Option<StyleFn>) {
        self.custom_bg_fn = bg_fn;
        self.cache = TextCache::default();
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Component for Text {
    fn invalidate(&mut self) {
        self.cache = TextCache::default();
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        if let Some(lines) = &self.cache.lines
            && self.cache.cached_text == self.text
            && self.cache.cached_width == width
        {
            return lines.clone();
        }
        // Don't render anything if there's no actual text (JS trim semantics).
        if pie_core::wrap::js_trim_is_empty(&self.text) {
            self.cache = TextCache {
                lines: Some(Vec::new()),
                cached_text: self.text.clone(),
                cached_width: width,
            };
            return Vec::new();
        }
        // Tabs render as three spaces.
        let normalized_text = self.text.replace('\t', "   ");
        let content_width = (width.saturating_sub(self.padding_x * 2)).max(1);
        let wrapped_lines = wrap_text_with_ansi(&normalized_text, content_width);
        let left_margin = " ".repeat(self.padding_x);
        let right_margin = " ".repeat(self.padding_x);
        let mut content_lines = Vec::with_capacity(wrapped_lines.len());
        for line in wrapped_lines {
            let line_with_margins = format!("{left_margin}{line}{right_margin}");
            if let Some(bg) = &self.custom_bg_fn {
                content_lines.push(apply_background_to_line(&line_with_margins, width, bg));
            } else {
                let visible_len = visible_width(&line_with_margins);
                let padding_needed = width.saturating_sub(visible_len);
                content_lines.push(format!("{line_with_margins}{}", " ".repeat(padding_needed)));
            }
        }
        let empty_line = " ".repeat(width);
        let empty_line_padded = |line: &str| match &self.custom_bg_fn {
            Some(bg) => apply_background_to_line(line, width, bg),
            None => line.to_string(),
        };
        let mut result = Vec::with_capacity(self.padding_y * 2 + content_lines.len());
        for _ in 0..self.padding_y {
            result.push(empty_line_padded(&empty_line));
        }
        result.extend(content_lines);
        for _ in 0..self.padding_y {
            result.push(empty_line_padded(&empty_line));
        }
        self.cache = TextCache {
            lines: Some(result.clone()),
            cached_text: self.text.clone(),
            cached_width: width,
        };
        result
    }
}
