//! pie-demo — minimal real-terminal driver for the tmux smoke test.
//!
//! Renders a few lines through the main-screen differential renderer, reads
//! stdin through the ported StdinBuffer (escape-timeout + paste semantics),
//! and redraws on input. Keys: space/j increment, k decrement, q or ctrl+c
//! quits. Raw mode via termios; all terminal I/O goes through the Terminal
//! trait so the same renderer paths proven by goldens run against a real pty.

use std::io::{Read, Write};
use std::process::ExitCode;

use pie_app::{LineSource, MainScreenController};
use pie_core::keys::parse_key;
use pie_core::stdin_buffer::{StdinBuffer, StdinEvent};
use pie_core::wrap::wrap_text_with_ansi;
use pie_term::{InputHandler, ResizeHandler, Terminal};

struct DemoLines {
    count: u32,
    width: usize,
    height: usize,
}

impl LineSource for DemoLines {
    fn render_lines(&mut self, width: usize) -> Vec<String> {
        self.width = width;
        let mut lines = vec![
            "pie-demo — pie-tui Rust port smoke".to_string(),
            format!("count: {}", self.count),
            format!("viewport: {}x{}", self.width, self.height),
            "space/j: +1   k: -1   q: quit".to_string(),
        ];
        lines.extend(wrap_text_with_ansi(
            "resize-probe: 012345678901234567890123456789012345678901234567890123456789",
            width.max(1),
        ));
        lines
    }
}

/// Real stdout backend (writes to stdout; size refreshed from the pty).
struct StdoutTerminal {
    width: usize,
    height: usize,
}

impl StdoutTerminal {
    fn refresh_size(&mut self) -> bool {
        let (width, height) = terminal_size();
        if (width, height) == (self.width, self.height) {
            return false;
        }
        self.width = width;
        self.height = height;
        true
    }
}

impl Terminal for StdoutTerminal {
    fn start(&mut self, _on_input: InputHandler, _on_resize: ResizeHandler) {}
    fn stop(&mut self) {}
    fn write(&mut self, data: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(data.as_bytes());
        let _ = out.flush();
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
        self.write("\x1b[?25l");
    }
    fn show_cursor(&mut self) {
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
    fn set_progress(&mut self, _active: bool) {}
}

fn terminal_size() -> (usize, usize) {
    #[cfg(unix)]
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0
            && size.ws_col > 0
            && size.ws_row > 0
        {
            return (size.ws_col as usize, size.ws_row as usize);
        }
    }
    // Non-pty fallback for direct invocation and non-Unix builds.
    let cols = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(80);
    let rows = std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    (cols, rows)
}

fn main() -> ExitCode {
    let (width, height) = terminal_size();
    let mut term = StdoutTerminal { width, height };

    // Raw mode (Unix termios).
    let stdin_fd = 0;
    let original_termios = enable_raw_mode(stdin_fd);

    let mut renderer = MainScreenController::new();
    let mut demo = DemoLines {
        count: 0,
        width,
        height,
    };
    let mut stdin_buffer = StdinBuffer::new(10);

    redraw(&mut renderer, &mut term, &mut demo);

    let exit_code = run_input_loop(&mut stdin_buffer, &mut renderer, &mut term, &mut demo);

    restore_raw_mode(stdin_fd, original_termios);
    exit_code
}

fn redraw(renderer: &mut MainScreenController, term: &mut StdoutTerminal, demo: &mut DemoLines) {
    term.refresh_size();
    demo.height = term.height;
    let _ = renderer.render_now(term, demo, false);
}

fn run_input_loop(
    stdin_buffer: &mut StdinBuffer,
    renderer: &mut MainScreenController,
    term: &mut StdoutTerminal,
    demo: &mut DemoLines,
) -> ExitCode {
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 1024];
    let mut last_input = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(stdin_buffer.timeout_ms());
    loop {
        // A bounded readiness wait keeps resize detection and ESC flushing live
        // even when stdin is silent.
        let ready = stdin_ready(timeout);
        if term.refresh_size() {
            demo.height = term.height;
            redraw(renderer, term, demo);
        }
        let mut poll = [0u8; 1];
        let got = ready
            && match stdin.read(&mut poll) {
                Ok(0) => false,
                Ok(_) => {
                    buf[0] = poll[0];
                    true
                }
                Err(_) => false,
            };
        if got {
            let chunk = String::from_utf8_lossy(&buf[..1]).to_string();
            last_input = std::time::Instant::now();
            if handle_chunk(stdin_buffer, renderer, term, demo, &chunk) {
                return ExitCode::SUCCESS;
            }
            continue;
        }
        // Silence for >= timeout: flush the escape buffer (spec §4).
        if last_input.elapsed() >= timeout {
            for event in stdin_buffer.flush() {
                if handle_event(renderer, term, demo, &event) {
                    return ExitCode::SUCCESS;
                }
            }
            last_input = std::time::Instant::now(); // avoid busy-spinning
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[cfg(unix)]
fn stdin_ready(timeout: std::time::Duration) -> bool {
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let mut descriptor = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
    result > 0 && descriptor.revents & (libc::POLLIN | libc::POLLHUP) != 0
}

#[cfg(not(unix))]
fn stdin_ready(_timeout: std::time::Duration) -> bool {
    true
}

fn handle_chunk(
    stdin_buffer: &mut StdinBuffer,
    renderer: &mut MainScreenController,
    term: &mut StdoutTerminal,
    demo: &mut DemoLines,
    chunk: &str,
) -> bool {
    for event in stdin_buffer.process(chunk) {
        if handle_event(renderer, term, demo, &event) {
            return true;
        }
    }
    false
}

fn handle_event(
    renderer: &mut MainScreenController,
    term: &mut StdoutTerminal,
    demo: &mut DemoLines,
    event: &StdinEvent,
) -> bool {
    let sequence = match event {
        StdinEvent::Data(s) => s.clone(),
        StdinEvent::Paste(_) => return false,
    };
    if matches!(parse_key(&sequence).as_deref(), Some("q") | Some("ctrl+c")) {
        return true;
    }
    match parse_key(&sequence).as_deref() {
        Some("space") | Some("j") => {
            demo.count += 1;
            redraw(renderer, term, demo);
        }
        Some("k") => {
            demo.count = demo.count.saturating_sub(1);
            redraw(renderer, term, demo);
        }
        _ => {}
    }
    false
}

// ---- termios raw mode (Unix) ----

#[cfg(unix)]
fn enable_raw_mode(fd: i32) -> libc::termios {
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) != 0 {
            return t;
        }
        let original = t;
        // cfmakeraw equivalent for input flags (mirror what the reference's
        // setRawMode(true) does through libuv).
        libc::cfmakeraw(&mut t);
        libc::tcsetattr(fd, libc::TCSANOW, &t);
        original
    }
}

#[cfg(unix)]
fn restore_raw_mode(fd: i32, original: libc::termios) {
    unsafe {
        libc::tcsetattr(fd, libc::TCSANOW, &original);
    }
}

#[cfg(not(unix))]
fn enable_raw_mode(_fd: i32) -> () {}

#[cfg(not(unix))]
unsafe fn restore_raw_mode(_fd: i32, _original: ()) {}
