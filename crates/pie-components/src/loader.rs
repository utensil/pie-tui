//! Loader — spinner + message line (reference `components/loader.js`).
//!
//! The animation timer lives with the runtime adapter; the component exposes
//! `advance_frame()` for the tick and `update_display()` to rebuild its Text.

use crate::text::Text;
use crate::{Component, StyleFn};

pub const DEFAULT_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const DEFAULT_INTERVAL_MS: u64 = 80;

/// Spinner + message component. Render output is one empty line above the
/// padded text (reference `render` override).
pub struct Loader {
    text: Text,
    frames: Vec<String>,
    interval_ms: u64,
    current_frame: usize,
    render_indicator_verbatim: bool,
    spinner_color_fn: Option<StyleFn>,
    message_color_fn: Option<StyleFn>,
    message: String,
    animation_scheduled: bool,
    request_render: Option<Box<dyn FnMut() + Send>>,
}

impl Loader {
    pub fn new(message: impl Into<String>) -> Self {
        let mut loader = Loader {
            text: Text::with_padding("", 1, 0),
            frames: DEFAULT_FRAMES.iter().map(|s| s.to_string()).collect(),
            interval_ms: DEFAULT_INTERVAL_MS,
            current_frame: 0,
            render_indicator_verbatim: false,
            spinner_color_fn: None,
            message_color_fn: None,
            message: message.into(),
            animation_scheduled: false,
            request_render: None,
        };
        // Reference constructor ends with setIndicator(undefined) -> start().
        loader.start();
        loader
    }

    pub fn with_colors(
        message: impl Into<String>,
        spinner_color_fn: Option<StyleFn>,
        message_color_fn: Option<StyleFn>,
        indicator: Option<SpinnerIndicator>,
    ) -> Self {
        let mut loader = Loader::new(message);
        loader.spinner_color_fn = spinner_color_fn;
        loader.message_color_fn = message_color_fn;
        loader.set_indicator(indicator);
        loader
    }

    pub fn with_runtime(
        message: impl Into<String>,
        spinner_color_fn: Option<StyleFn>,
        message_color_fn: Option<StyleFn>,
        indicator: Option<SpinnerIndicator>,
        request_render: Box<dyn FnMut() + Send>,
    ) -> Self {
        let mut loader = Loader::new(message);
        loader.spinner_color_fn = spinner_color_fn;
        loader.message_color_fn = message_color_fn;
        loader.request_render = Some(request_render);
        loader.set_indicator(indicator);
        loader
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.update_display();
    }

    pub fn set_indicator(&mut self, indicator: Option<SpinnerIndicator>) {
        self.render_indicator_verbatim = indicator.is_some();
        match indicator {
            Some(ind) => {
                self.frames = ind.frames.unwrap_or_else(default_frames);
                if ind.interval_ms.is_some_and(|ms| ms > 0) {
                    self.interval_ms = ind.interval_ms.unwrap();
                } else {
                    self.interval_ms = DEFAULT_INTERVAL_MS;
                }
            }
            None => {
                self.frames = DEFAULT_FRAMES.iter().map(|s| s.to_string()).collect();
                self.interval_ms = DEFAULT_INTERVAL_MS;
            }
        }
        self.current_frame = 0;
        self.start();
    }

    /// Timer tick (runtime adapter calls this every `interval_ms`).
    pub fn advance_frame(&mut self) {
        if self.animation_scheduled {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            self.update_display();
        }
    }

    /// Rebuild the underlying text from the current frame + message
    /// (reference `updateDisplay`).
    pub fn update_display(&mut self) {
        let frame = self
            .frames
            .get(self.current_frame)
            .cloned()
            .unwrap_or_default();
        let identity = |s: &str| s.to_string();
        let color_frame = self.spinner_color_fn.as_deref().unwrap_or(&identity);
        let color_message = self.message_color_fn.as_deref().unwrap_or(&identity);
        let rendered_frame = if self.render_indicator_verbatim {
            frame.clone()
        } else {
            color_frame(&frame)
        };
        let indicator = if !frame.is_empty() {
            format!("{rendered_frame} ")
        } else {
            String::new()
        };
        let composed = format!("{indicator}{}", color_message(&self.message));
        self.text.set_text(composed);
        if let Some(request_render) = self.request_render.as_mut() {
            request_render();
        }
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn current_frame(&self) -> usize {
        self.current_frame
    }

    pub fn frames(&self) -> &[String] {
        &self.frames
    }

    /// Forward the inherited `Text.setText` behavior. A later spinner update
    /// will replace this display text, just like the reference subclass.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text.set_text(text);
    }

    pub fn set_custom_bg_fn(&mut self, bg_fn: Option<StyleFn>) {
        self.text.set_custom_bg_fn(bg_fn);
    }

    pub fn text(&self) -> &str {
        self.text.text()
    }

    /// Start the runtime-owned animation clock.
    pub fn start(&mut self) {
        self.update_display();
        self.restart_animation();
    }

    /// Stop future runtime-owned animation ticks.
    pub fn stop(&mut self) {
        self.animation_scheduled = false;
    }

    pub fn is_running(&self) -> bool {
        self.animation_scheduled
    }

    pub fn is_animation_scheduled(&self) -> bool {
        self.animation_scheduled
    }

    fn restart_animation(&mut self) {
        self.stop();
        self.animation_scheduled = self.frames.len() > 1;
    }
}

impl Component for Loader {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = vec![String::new()];
        lines.extend(self.text.render(width));
        lines
    }

    fn invalidate(&mut self) {
        self.text.invalidate();
    }
}

/// Custom spinner definition (reference `indicator` option).
pub struct LoaderIndicatorOptions {
    /// `None` means omitted/default frames; `Some([])` hides the indicator.
    pub frames: Option<Vec<String>>,
    pub interval_ms: Option<u64>,
}

/// Backward-compatible name used by the initial Rust wave-1 port.
pub type SpinnerIndicator = LoaderIndicatorOptions;

fn default_frames() -> Vec<String> {
    DEFAULT_FRAMES
        .iter()
        .map(|frame| (*frame).to_owned())
        .collect()
}
