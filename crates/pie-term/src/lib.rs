//! Terminal abstraction — the IO seam of the TUI.
//!
//! Port of the pinned pi-tui `Terminal` interface (dist/terminal.d.ts). The
//! trait is intentionally synchronous for writes and size queries; input
//! delivery is a closure registered at `start` (Rust side uses `Box<dyn FnMut>`
//! instead of Node event emitters). `TestRecorder` is the deterministic
//! backend used by golden/differential tests.

pub mod capabilities;
pub mod process_terminal;
pub mod renderer;

/// A backend the renderer talks to (real pty, test recorder, …).
pub trait Terminal {
    /// Begin delivering input events and resize notifications.
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler);
    /// Restore the terminal to its pre-TUI state.
    fn stop(&mut self);
    /// Write raw bytes/escape sequences to the terminal.
    fn write(&mut self, data: &str);
    /// Current viewport width in columns.
    fn columns(&self) -> usize;
    /// Current viewport height in rows.
    fn rows(&self) -> usize;
    /// Kitty keyboard protocol negotiated and active?
    fn kitty_protocol_active(&self) -> bool;
    /// Move cursor vertically (positive down, negative up, 0 = no-op).
    fn move_by(&mut self, lines: isize);
    fn hide_cursor(&mut self);
    fn show_cursor(&mut self);
    /// Clear from cursor to end of line (`\x1b[K`).
    fn clear_line(&mut self);
    /// Clear from cursor to end of screen (`\x1b[J`).
    fn clear_from_cursor(&mut self);
    /// Clear screen and home cursor (`\x1b[2J\x1b[H`).
    fn clear_screen(&mut self);
    /// Set window title (OSC 0).
    fn set_title(&mut self, title: &str);
    /// Indeterminate progress indicator (OSC 9;4).
    fn set_progress(&mut self, active: bool);
}

/// Input callback: one complete input sequence per call.
pub type InputHandler = Box<dyn FnMut(&str)>;
/// Resize notification callback.
pub type ResizeHandler = Box<dyn FnMut()>;

/// Deterministic in-memory terminal: scripted size, scripted input delivery,
/// and every `write` captured verbatim so render tests can assert exact byte
/// output (the reference's TestTerminal pattern, made recording-first).
#[derive(Default)]
pub struct TestRecorder {
    /// All data ever passed to [`Terminal::write`], concatenated.
    pub written: String,
    /// Per-write boundaries (byte offsets into `written`) for batch assertions.
    pub write_boundaries: Vec<usize>,
    width: usize,
    height: usize,
    kitty_protocol_active: bool,
    on_input: Option<InputHandler>,
    on_resize: Option<ResizeHandler>,
    cursor_visible: bool,
    progress_active: bool,
    started: bool,
    stopped: bool,
}

impl TestRecorder {
    pub fn new(width: usize, height: usize) -> Self {
        TestRecorder {
            width,
            height,
            ..TestRecorder::default()
        }
    }

    /// Deliver one input sequence to the registered handler (scripted stdin).
    pub fn feed_input(&mut self, data: &str) {
        if let Some(handler) = self.on_input.as_mut() {
            handler(data);
        }
    }

    /// Fire the scripted resize notification.
    pub fn fire_resize(&mut self) {
        if let Some(handler) = self.on_resize.as_mut() {
            handler();
        }
    }

    pub fn set_size(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }

    pub fn set_kitty_protocol_active(&mut self, active: bool) {
        self.kitty_protocol_active = active;
    }

    /// Data written since the given byte offset (for incremental assertions).
    pub fn written_since(&self, offset: usize) -> &str {
        &self.written[offset..]
    }

    pub fn clear_written(&mut self) {
        self.written.clear();
        self.write_boundaries.clear();
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn progress_active(&self) -> bool {
        self.progress_active
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

impl Terminal for TestRecorder {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        self.on_input = Some(on_input);
        self.on_resize = Some(on_resize);
        self.started = true;
        self.stopped = false;
    }

    fn stop(&mut self) {
        self.on_input = None;
        self.on_resize = None;
        self.started = false;
        self.stopped = true;
    }

    fn write(&mut self, data: &str) {
        self.write_boundaries.push(self.written.len());
        self.written.push_str(data);
    }

    fn columns(&self) -> usize {
        self.width
    }

    fn rows(&self) -> usize {
        self.height
    }

    fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_active
    }

    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.write(&format!("\x1b[{lines}B"));
        } else if lines < 0 {
            self.write(&format!("\x1b[{}A", -lines));
        }
    }

    fn hide_cursor(&mut self) {
        self.cursor_visible = false;
        self.write("\x1b[?25l");
    }

    fn show_cursor(&mut self) {
        self.cursor_visible = true;
        self.write("\x1b[?25h");
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
        self.progress_active = active;
        if active {
            self.write("\x1b]9;4;3\x07");
        } else {
            self.write("\x1b]9;4;0\x07");
        }
    }
}

/// Parse a keyboard-protocol negotiation response (reference
/// `parseKeyboardProtocolNegotiationSequence`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardProtocolNegotiation {
    /// `\x1b[?Nu` — Kitty protocol flags response.
    KittyFlags { flags: u32 },
    /// `\x1b[?...c` — Device Attributes response (kitty unsupported).
    DeviceAttributes,
}

pub fn parse_keyboard_protocol_negotiation(sequence: &str) -> Option<KeyboardProtocolNegotiation> {
    if let Some(rest) = sequence.strip_prefix("\x1b[?") {
        if let Some(flags) = rest.strip_suffix('u')
            && !flags.is_empty()
            && flags.bytes().all(|b| b.is_ascii_digit())
        {
            return Some(KeyboardProtocolNegotiation::KittyFlags {
                flags: flags.parse().ok()?,
            });
        }
        if let Some(body) = rest.strip_suffix('c')
            && body.bytes().all(|b| b.is_ascii_digit() || b == b';')
        {
            return Some(KeyboardProtocolNegotiation::DeviceAttributes);
        }
    }
    None
}

/// Is `sequence` a strict prefix of a negotiation response (keep buffering)?
pub fn is_negotiation_prefix(sequence: &str) -> bool {
    sequence == "\x1b[" || {
        if let Some(rest) = sequence.strip_prefix("\x1b[?") {
            rest.bytes().all(|b| b.is_ascii_digit() || b == b';')
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_parsing() {
        assert_eq!(
            parse_keyboard_protocol_negotiation("\x1b[?7u"),
            Some(KeyboardProtocolNegotiation::KittyFlags { flags: 7 })
        );
        assert_eq!(
            parse_keyboard_protocol_negotiation("\x1b[?1;0c"),
            Some(KeyboardProtocolNegotiation::DeviceAttributes)
        );
        assert_eq!(parse_keyboard_protocol_negotiation("\x1b[?u"), None);
        assert_eq!(parse_keyboard_protocol_negotiation("hello"), None);
        assert!(is_negotiation_prefix("\x1b["));
        assert!(is_negotiation_prefix("\x1b[?1;2"));
        assert!(!is_negotiation_prefix("\x1b[?1x"));
    }

    #[test]
    fn recorder_captures_writes_and_boundaries() {
        let mut t = TestRecorder::new(80, 24);
        let input_count = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let counter = std::rc::Rc::clone(&input_count);
        t.start(
            Box::new(move |_data| {
                counter.set(counter.get() + 1);
            }),
            Box::new(|| {}),
        );
        assert!(t.started());
        t.write("hello");
        t.write("\x1b[2J");
        assert_eq!(t.written, "hello\x1b[2J");
        assert_eq!(t.write_boundaries, vec![0, 5]);
        t.feed_input("a");
        assert_eq!(input_count.get(), 1);
        t.set_size(100, 30);
        assert_eq!((t.columns(), t.rows()), (100, 30));
        t.stop();
        assert!(t.stopped());
        t.feed_input("a"); // no handler after stop — no panic, no count
        assert_eq!(input_count.get(), 1);
    }
}
