//! Backend-injected `ProcessTerminal` lifecycle and protocol negotiation.
//!
//! The state machine owns ordering, input buffering, timers, negotiation, and
//! cleanup. Platform bindings implement [`ProcessTerminalBackend`]; tests use a
//! recording fake, so no test changes raw mode or writes to the controlling TTY.

use pie_core::keys::set_kitty_protocol_active;
use pie_core::stdin_buffer::{StdinBuffer, StdinEvent};

use crate::{
    InputHandler, KeyboardProtocolNegotiation, ResizeHandler, Terminal, is_negotiation_prefix,
    parse_keyboard_protocol_negotiation,
};

pub const TERMINAL_PROGRESS_KEEPALIVE_MS: u64 = 1000;
pub const KEYBOARD_PROTOCOL_RESPONSE_FRAGMENT_TIMEOUT_MS: u64 = 150;
pub const STDIN_SEQUENCE_TIMEOUT_MS: u64 = 10;
pub const KITTY_KEYBOARD_PROTOCOL_QUERY: &str = "\x1b[>7u\x1b[?u\x1b[c";
pub const BRACKETED_PASTE_ON: &str = "\x1b[?2004h";
pub const BRACKETED_PASTE_OFF: &str = "\x1b[?2004l";
pub const KITTY_KEYBOARD_PROTOCOL_OFF: &str = "\x1b[<u";
pub const MODIFY_OTHER_KEYS_ON: &str = "\x1b[>4;2m";
pub const MODIFY_OTHER_KEYS_OFF: &str = "\x1b[>4;0m";
pub const TERMINAL_PROGRESS_ACTIVE_SEQUENCE: &str = "\x1b]9;4;3\x07";
pub const TERMINAL_PROGRESS_CLEAR_SEQUENCE: &str = "\x1b]9;4;0\x07";
pub const NATIVE_SHIFT_ENTER_SEQUENCE: &str = "\x1b[13;2u";

/// OS/stream operations used by [`ProcessTerminal`].
///
/// Listener methods register the host bridge; the bridge feeds bytes back with
/// [`ProcessTerminal::receive_stdin`] and resize events with
/// [`ProcessTerminal::receive_resize`].
pub trait ProcessTerminalBackend {
    fn raw_mode(&self) -> bool;
    fn supports_raw_mode(&self) -> bool {
        true
    }
    fn set_raw_mode(&mut self, active: bool);
    fn set_utf8_encoding(&mut self);
    fn resume_input(&mut self);
    fn pause_input(&mut self);
    fn subscribe_input(&mut self);
    fn unsubscribe_input(&mut self);
    fn subscribe_drain_input(&mut self);
    fn unsubscribe_drain_input(&mut self);
    fn subscribe_resize(&mut self);
    fn unsubscribe_resize(&mut self);
    fn signal_winch(&mut self);
    fn enable_windows_vt_input(&mut self);
    fn write(&mut self, data: &str);
    fn columns(&self) -> Option<usize>;
    fn rows(&self) -> Option<usize>;
    fn environment_columns(&self) -> Option<usize>;
    fn environment_rows(&self) -> Option<usize>;
    fn is_windows(&self) -> bool {
        false
    }
    fn is_apple_terminal(&self) -> bool {
        false
    }
    fn shift_pressed(&self) -> bool {
        false
    }
}

struct DrainState {
    previous_handler: Option<InputHandler>,
    end_ms: u64,
    idle_ms: u64,
    last_data_ms: u64,
}

enum NegotiationRead {
    Pending,
    Complete(KeyboardProtocolNegotiation),
    None,
}

/// Process terminal lifecycle with all timing driven by explicit millisecond ticks.
pub struct ProcessTerminal<B: ProcessTerminalBackend> {
    backend: B,
    was_raw: bool,
    input_handler: Option<InputHandler>,
    resize_handler: Option<ResizeHandler>,
    kitty_protocol_active: bool,
    modify_other_keys_active: bool,
    keyboard_protocol_pushed: bool,
    negotiation_buffer: String,
    negotiation_deadline_ms: Option<u64>,
    stdin_buffer: Option<StdinBuffer>,
    stdin_deadline_ms: Option<u64>,
    progress_next_ms: Option<u64>,
    drain: Option<DrainState>,
    input_subscribed: bool,
    resize_subscribed: bool,
    started: bool,
    cleanup_done: bool,
    now_ms: u64,
}

impl<B: ProcessTerminalBackend> ProcessTerminal<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            was_raw: false,
            input_handler: None,
            resize_handler: None,
            kitty_protocol_active: false,
            modify_other_keys_active: false,
            keyboard_protocol_pushed: false,
            negotiation_buffer: String::new(),
            negotiation_deadline_ms: None,
            stdin_buffer: None,
            stdin_deadline_ms: None,
            progress_next_ms: None,
            drain: None,
            input_subscribed: false,
            resize_subscribed: false,
            started: false,
            cleanup_done: false,
            now_ms: 0,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_active
    }

    pub fn modify_other_keys_active(&self) -> bool {
        self.modify_other_keys_active
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// Start at an explicit scheduler time (milliseconds).
    pub fn start_at(&mut self, now_ms: u64, on_input: InputHandler, on_resize: ResizeHandler) {
        self.now_ms = now_ms;
        self.cleanup_done = false;
        self.input_handler = Some(on_input);
        self.resize_handler = Some(on_resize);
        self.was_raw = self.backend.raw_mode();
        if self.backend.supports_raw_mode() {
            self.backend.set_raw_mode(true);
        }
        self.backend.set_utf8_encoding();
        self.backend.resume_input();
        self.backend.write(BRACKETED_PASTE_ON);
        self.backend.subscribe_resize();
        self.resize_subscribed = true;
        if self.backend.is_windows() {
            self.backend.enable_windows_vt_input();
        } else {
            self.backend.signal_winch();
        }

        self.stdin_buffer = Some(StdinBuffer::new(STDIN_SEQUENCE_TIMEOUT_MS));
        self.stdin_deadline_ms = None;
        self.backend.subscribe_input();
        self.input_subscribed = true;
        self.keyboard_protocol_pushed = true;
        self.clear_negotiation_buffer();
        self.backend.write(KITTY_KEYBOARD_PROTOCOL_QUERY);
        self.started = true;
    }

    /// Feed one host stdin chunk through `StdinBuffer` and protocol negotiation.
    pub fn receive_stdin(&mut self, data: &str, now_ms: u64) {
        self.now_ms = now_ms;
        let events = self
            .stdin_buffer
            .as_mut()
            .map(|buffer| buffer.process(data))
            .unwrap_or_default();
        self.refresh_stdin_deadline(now_ms);
        self.process_stdin_events(events, now_ms);
        if let Some(drain) = self.drain.as_mut() {
            drain.last_data_ms = now_ms;
        }
    }

    /// Deliver the subscribed resize event.
    pub fn receive_resize(&mut self) {
        if let Some(handler) = self.resize_handler.as_mut() {
            handler();
        }
    }

    /// Advance stdin, negotiation, and progress timers.
    pub fn tick(&mut self, now_ms: u64) {
        while let Some(deadline) = [
            self.stdin_deadline_ms,
            self.negotiation_deadline_ms,
            self.progress_next_ms,
        ]
        .into_iter()
        .flatten()
        .filter(|deadline| *deadline <= now_ms)
        .min()
        {
            self.now_ms = deadline;
            if self.stdin_deadline_ms == Some(deadline) {
                self.stdin_deadline_ms = None;
                let events = self
                    .stdin_buffer
                    .as_mut()
                    .map(StdinBuffer::flush)
                    .unwrap_or_default();
                self.process_stdin_events(events, deadline);
            } else if self.negotiation_deadline_ms == Some(deadline) {
                self.flush_negotiation_buffer_as_input();
            } else {
                self.backend.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
                self.progress_next_ms = deadline.checked_add(TERMINAL_PROGRESS_KEEPALIVE_MS);
            }
        }
        self.now_ms = now_ms;
    }

    /// Begin the asynchronous drain protocol. Call [`Self::poll_drain_input`]
    /// after scheduler advances and [`Self::receive_stdin`] for arriving bytes.
    pub fn begin_drain_input(&mut self, now_ms: u64, max_ms: u64, idle_ms: u64) {
        self.now_ms = now_ms;
        let should_disable_kitty = self.keyboard_protocol_pushed || self.kitty_protocol_active;
        self.clear_negotiation_buffer();
        if should_disable_kitty {
            self.backend.write(KITTY_KEYBOARD_PROTOCOL_OFF);
            self.keyboard_protocol_pushed = false;
            self.kitty_protocol_active = false;
            set_kitty_protocol_active(false);
        }
        self.disable_modify_other_keys();
        let previous_handler = self.input_handler.take();
        self.backend.subscribe_drain_input();
        self.drain = Some(DrainState {
            previous_handler,
            end_ms: now_ms.saturating_add(max_ms),
            idle_ms,
            last_data_ms: now_ms,
        });
    }

    /// Complete a drain once its maximum or idle deadline is reached.
    pub fn poll_drain_input(&mut self, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        let complete = self.drain.as_ref().is_some_and(|drain| {
            now_ms >= drain.end_ms || now_ms.saturating_sub(drain.last_data_ms) >= drain.idle_ms
        });
        if complete {
            let drain = self.drain.take().expect("drain was present");
            self.backend.unsubscribe_drain_input();
            self.input_handler = drain.previous_handler;
        }
        complete
    }

    pub fn set_progress_at(&mut self, active: bool, now_ms: u64) {
        self.now_ms = now_ms;
        if active {
            self.backend.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
            if self.progress_next_ms.is_none() {
                self.progress_next_ms = Some(now_ms.saturating_add(TERMINAL_PROGRESS_KEEPALIVE_MS));
            }
        } else {
            self.progress_next_ms = None;
            self.backend.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
        }
    }

    pub fn stop_process(&mut self) {
        if self.cleanup_done {
            return;
        }
        if self.progress_next_ms.take().is_some() {
            self.backend.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
        }
        self.backend.write(BRACKETED_PASTE_OFF);
        let should_disable_kitty = self.keyboard_protocol_pushed || self.kitty_protocol_active;
        self.clear_negotiation_buffer();
        if should_disable_kitty {
            self.backend.write(KITTY_KEYBOARD_PROTOCOL_OFF);
            self.keyboard_protocol_pushed = false;
            self.kitty_protocol_active = false;
            set_kitty_protocol_active(false);
        }
        self.disable_modify_other_keys();
        if let Some(mut buffer) = self.stdin_buffer.take() {
            buffer.clear();
        }
        self.stdin_deadline_ms = None;
        if self.input_subscribed {
            self.backend.unsubscribe_input();
            self.input_subscribed = false;
        }
        self.input_handler = None;
        if self.drain.take().is_some() {
            self.backend.unsubscribe_drain_input();
        }
        if self.resize_subscribed {
            self.backend.unsubscribe_resize();
            self.resize_subscribed = false;
        }
        self.resize_handler = None;
        self.backend.pause_input();
        if self.backend.supports_raw_mode() {
            self.backend.set_raw_mode(self.was_raw);
        }
        self.started = false;
        self.cleanup_done = true;
    }

    fn refresh_stdin_deadline(&mut self, now_ms: u64) {
        self.stdin_deadline_ms = self.stdin_buffer.as_ref().and_then(|buffer| {
            (!buffer.get_buffer().is_empty()).then_some(now_ms.saturating_add(buffer.timeout_ms()))
        });
    }

    fn process_stdin_events(&mut self, events: Vec<StdinEvent>, now_ms: u64) {
        for event in events {
            match event {
                StdinEvent::Paste(content) => {
                    if let Some(handler) = self.input_handler.as_mut() {
                        handler(&format!("\x1b[200~{content}\x1b[201~"));
                    }
                }
                StdinEvent::Data(sequence) => {
                    let negotiation = self.read_negotiation_sequence(&sequence);
                    match negotiation {
                        NegotiationRead::Pending => {
                            if self.negotiation_deadline_ms.is_none() {
                                self.negotiation_deadline_ms = Some(now_ms.saturating_add(
                                    KEYBOARD_PROTOCOL_RESPONSE_FRAGMENT_TIMEOUT_MS,
                                ));
                            }
                        }
                        NegotiationRead::Complete(negotiation) => {
                            self.handle_negotiation(negotiation);
                        }
                        NegotiationRead::None => self.forward_input_sequence(&sequence),
                    }
                }
            }
        }
    }

    fn read_negotiation_sequence(&mut self, sequence: &str) -> NegotiationRead {
        if !self.negotiation_buffer.is_empty() {
            let buffered = format!("{}{}", self.negotiation_buffer, sequence);
            if let Some(negotiation) = parse_keyboard_protocol_negotiation(&buffered) {
                self.clear_negotiation_buffer();
                return NegotiationRead::Complete(negotiation);
            }
            if is_negotiation_prefix(&buffered) {
                self.set_negotiation_buffer(buffered);
                return NegotiationRead::Pending;
            }
            self.flush_negotiation_buffer_as_input();
        }
        if let Some(negotiation) = parse_keyboard_protocol_negotiation(sequence) {
            return NegotiationRead::Complete(negotiation);
        }
        if is_negotiation_prefix(sequence) {
            self.set_negotiation_buffer(sequence.to_owned());
            return NegotiationRead::Pending;
        }
        NegotiationRead::None
    }

    fn handle_negotiation(&mut self, negotiation: KeyboardProtocolNegotiation) {
        self.clear_negotiation_buffer();
        match negotiation {
            KeyboardProtocolNegotiation::KittyFlags { flags } => {
                if flags == 0 {
                    self.enable_modify_other_keys();
                } else {
                    self.disable_modify_other_keys();
                    if !self.kitty_protocol_active {
                        self.kitty_protocol_active = true;
                        set_kitty_protocol_active(true);
                    }
                }
            }
            KeyboardProtocolNegotiation::DeviceAttributes => {
                if !self.kitty_protocol_active {
                    self.enable_modify_other_keys();
                }
            }
        }
    }

    fn set_negotiation_buffer(&mut self, sequence: String) {
        self.negotiation_deadline_ms = None;
        self.negotiation_buffer = sequence;
    }

    fn clear_negotiation_buffer(&mut self) {
        self.negotiation_deadline_ms = None;
        self.negotiation_buffer.clear();
    }

    fn flush_negotiation_buffer_as_input(&mut self) {
        if self.negotiation_buffer.is_empty() {
            return;
        }
        let sequence = std::mem::take(&mut self.negotiation_buffer);
        self.negotiation_deadline_ms = None;
        self.forward_input_sequence(&sequence);
    }

    fn forward_input_sequence(&mut self, sequence: &str) {
        let should_detect =
            sequence == "\r" && (self.backend.is_apple_terminal() || self.backend.is_windows());
        let shift_pressed = should_detect && self.backend.shift_pressed();
        let input = normalize_native_shift_enter_input(sequence, should_detect, shift_pressed);
        if let Some(handler) = self.input_handler.as_mut() {
            handler(&input);
        }
    }

    fn enable_modify_other_keys(&mut self) {
        if self.kitty_protocol_active || self.modify_other_keys_active {
            return;
        }
        self.backend.write(MODIFY_OTHER_KEYS_ON);
        self.modify_other_keys_active = true;
    }

    fn disable_modify_other_keys(&mut self) {
        if !self.modify_other_keys_active {
            return;
        }
        self.backend.write(MODIFY_OTHER_KEYS_OFF);
        self.modify_other_keys_active = false;
    }
}

impl<B: ProcessTerminalBackend> Terminal for ProcessTerminal<B> {
    fn start(&mut self, on_input: InputHandler, on_resize: ResizeHandler) {
        self.start_at(0, on_input, on_resize);
    }

    fn stop(&mut self) {
        self.stop_process();
    }

    fn write(&mut self, data: &str) {
        self.backend.write(data);
    }

    fn columns(&self) -> usize {
        self.backend
            .columns()
            .filter(|value| *value != 0)
            .or_else(|| {
                self.backend
                    .environment_columns()
                    .filter(|value| *value != 0)
            })
            .unwrap_or(80)
    }

    fn rows(&self) -> usize {
        self.backend
            .rows()
            .filter(|value| *value != 0)
            .or_else(|| self.backend.environment_rows().filter(|value| *value != 0))
            .unwrap_or(24)
    }

    fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_active
    }

    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.backend.write(&format!("\x1b[{lines}B"));
        } else if lines < 0 {
            self.backend.write(&format!("\x1b[{}A", -lines));
        }
    }

    fn hide_cursor(&mut self) {
        self.backend.write("\x1b[?25l");
    }

    fn show_cursor(&mut self) {
        self.backend.write("\x1b[?25h");
    }

    fn clear_line(&mut self) {
        self.backend.write("\x1b[K");
    }

    fn clear_from_cursor(&mut self) {
        self.backend.write("\x1b[J");
    }

    fn clear_screen(&mut self) {
        self.backend.write("\x1b[2J\x1b[H");
    }

    fn set_title(&mut self, title: &str) {
        self.backend.write(&format!("\x1b]0;{title}\x07"));
    }

    fn set_progress(&mut self, active: bool) {
        self.set_progress_at(active, self.now_ms);
    }
}

impl<B: ProcessTerminalBackend> Drop for ProcessTerminal<B> {
    fn drop(&mut self) {
        if !self.cleanup_done
            && (self.started
                || self.input_subscribed
                || self.resize_subscribed
                || self.progress_next_ms.is_some()
                || self.drain.is_some())
        {
            self.stop_process();
        }
    }
}

/// Normalize native Shift+Enter to Kitty CSI-u form when detection is enabled.
pub fn normalize_native_shift_enter_input(
    data: &str,
    should_detect_native_shift_enter: bool,
    is_shift_pressed: bool,
) -> String {
    if should_detect_native_shift_enter && data == "\r" && is_shift_pressed {
        NATIVE_SHIFT_ENTER_SEQUENCE.to_owned()
    } else {
        data.to_owned()
    }
}

pub fn normalize_apple_terminal_input(
    data: &str,
    is_apple_terminal: bool,
    is_shift_pressed: bool,
) -> String {
    normalize_native_shift_enter_input(data, is_apple_terminal, is_shift_pressed)
}
