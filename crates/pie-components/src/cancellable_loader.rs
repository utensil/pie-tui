//! CancellableLoader — Loader with select-cancel/abort state.

use pie_core::keybindings::global::get_keybindings;

use crate::{CancellationSignal, Component, Loader, LoaderIndicatorOptions, StyleFn};

pub struct CancellableLoader {
    loader: Loader,
    signal: CancellationSignal,
    pub on_abort: Option<Box<dyn FnMut() + Send>>,
}

impl CancellableLoader {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            loader: Loader::new(message),
            signal: CancellationSignal::new(),
            on_abort: None,
        }
    }

    pub fn with_colors(
        message: impl Into<String>,
        spinner_color_fn: Option<StyleFn>,
        message_color_fn: Option<StyleFn>,
        indicator: Option<LoaderIndicatorOptions>,
    ) -> Self {
        Self {
            loader: Loader::with_colors(message, spinner_color_fn, message_color_fn, indicator),
            signal: CancellationSignal::new(),
            on_abort: None,
        }
    }

    pub fn with_runtime(
        message: impl Into<String>,
        spinner_color_fn: Option<StyleFn>,
        message_color_fn: Option<StyleFn>,
        indicator: Option<LoaderIndicatorOptions>,
        request_render: Box<dyn FnMut() + Send>,
    ) -> Self {
        Self {
            loader: Loader::with_runtime(
                message,
                spinner_color_fn,
                message_color_fn,
                indicator,
                request_render,
            ),
            signal: CancellationSignal::new(),
            on_abort: None,
        }
    }

    pub fn aborted(&self) -> bool {
        self.signal.aborted()
    }

    pub fn signal(&self) -> CancellationSignal {
        self.signal.clone()
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.loader.set_message(message);
    }

    pub fn set_indicator(&mut self, indicator: Option<LoaderIndicatorOptions>) {
        self.loader.set_indicator(indicator);
    }

    pub fn start(&mut self) {
        self.loader.start();
    }

    pub fn stop(&mut self) {
        self.loader.stop();
    }

    pub fn advance_frame(&mut self) {
        self.loader.advance_frame();
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.loader.set_text(text);
    }

    pub fn text(&self) -> &str {
        self.loader.text()
    }

    pub fn set_custom_bg_fn(&mut self, bg_fn: Option<StyleFn>) {
        self.loader.set_custom_bg_fn(bg_fn);
    }

    pub fn interval_ms(&self) -> u64 {
        self.loader.interval_ms()
    }

    pub fn is_animation_scheduled(&self) -> bool {
        self.loader.is_animation_scheduled()
    }

    pub fn dispose(&mut self) {
        self.loader.stop();
    }
}

impl Component for CancellableLoader {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.loader.render(width)
    }

    fn invalidate(&mut self) {
        self.loader.invalidate();
    }

    fn handle_input(&mut self, data: &str) {
        if get_keybindings().matches(data, "tui.select.cancel") {
            self.signal.cancel();
            if let Some(on_abort) = self.on_abort.as_mut() {
                on_abort();
            }
        }
    }
}
