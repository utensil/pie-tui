//! Tier-0 shell driver for the pinned 0.84.1 Main/Alt controllers.
//!
//! This intentionally supplies only a real terminal and explicit redraw loop;
//! it is a controller smoke target, not the canonical JavaScript host facade.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use pie_app::{DetachedTuiScreenRuntime, TuiAltScreen, TuiAltScreenOptions, TuiMainScreen};
use pie_components::{Component, ComponentHandle, Tui, TuiStopOptions};
use pie_term::{InputHandler, ResizeHandler, Terminal};

const STDIN_FD: i32 = 0;

#[derive(Clone, Copy)]
enum ScreenKind {
    Main,
    Alt,
}

impl ScreenKind {
    fn parse() -> Option<Self> {
        match std::env::args().nth(1).as_deref() {
            Some("main") => Some(Self::Main),
            Some("alt") => Some(Self::Alt),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Alt => "alt",
        }
    }
}

struct DemoState {
    kind: ScreenKind,
    count: u32,
}

struct DemoComponent {
    state: Rc<RefCell<DemoState>>,
}

impl Component for DemoComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let state = self.state.borrow();
        let (_, height) = terminal_size();
        vec![
            format!("pie-screen-demo {} controller", state.kind.name()),
            "reference: pi-tui 0.84.1".into(),
            format!("mode: {}", state.kind.name()),
            format!("count: {}", state.count),
            format!("viewport: {width}x{height}"),
            "j: +1   k: -1   q: quit".into(),
        ]
    }
}

struct StdoutTerminal;

impl Terminal for StdoutTerminal {
    fn start(&mut self, _on_input: InputHandler, _on_resize: ResizeHandler) {}
    fn stop(&mut self) {}

    fn write(&mut self, data: &str) {
        let mut output = std::io::stdout().lock();
        let _ = output.write_all(data.as_bytes());
        let _ = output.flush();
    }

    fn columns(&self) -> usize {
        terminal_size().0
    }

    fn rows(&self) -> usize {
        terminal_size().1
    }

    fn kitty_protocol_active(&self) -> bool {
        false
    }

    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.write(&format!("\x1b[{lines}B"));
        } else if lines < 0 {
            self.write(&format!("\x1b[{}A", lines.saturating_abs()));
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

fn controller(kind: ScreenKind) -> Box<dyn Tui> {
    match kind {
        ScreenKind::Main => Box::new(TuiMainScreen::new(
            Box::new(StdoutTerminal),
            Box::new(DetachedTuiScreenRuntime::default()),
            false,
        )),
        ScreenKind::Alt => Box::new(TuiAltScreen::new(
            Box::new(StdoutTerminal),
            Box::new(DetachedTuiScreenRuntime::default()),
            false,
            TuiAltScreenOptions {
                mouse: false,
                ..TuiAltScreenOptions::default()
            },
        )),
    }
}

fn main() -> ExitCode {
    let Some(kind) = ScreenKind::parse() else {
        eprintln!("usage: pie-screen-demo <main|alt>");
        return ExitCode::from(64);
    };
    let original_termios = enable_raw_mode(STDIN_FD);
    let state = Rc::new(RefCell::new(DemoState { kind, count: 0 }));
    let component = ComponentHandle::new(DemoComponent {
        state: state.clone(),
    });
    let screen = controller(kind);
    screen.add_child(component.as_component_ref());
    screen.start();
    screen.render_now(true);

    let exit = run_input_loop(&*screen, &state);
    screen.stop(TuiStopOptions {
        preserve_screen: true,
    });
    restore_raw_mode(STDIN_FD, original_termios);
    exit
}

fn run_input_loop(screen: &dyn Tui, state: &Rc<RefCell<DemoState>>) -> ExitCode {
    let mut input = std::io::stdin().lock();
    let mut previous_size = terminal_size();
    loop {
        if terminal_size() != previous_size {
            previous_size = terminal_size();
            screen.render_now(true);
        }
        if !stdin_ready(Duration::from_millis(50)) {
            continue;
        }
        let mut byte = [0u8; 1];
        match input.read(&mut byte) {
            Ok(0) | Err(_) => return ExitCode::FAILURE,
            Ok(_) => match byte[0] {
                b'q' | 3 => return ExitCode::SUCCESS,
                b'j' => {
                    let mut state = state.borrow_mut();
                    state.count = state.count.saturating_add(1);
                    drop(state);
                    screen.render_now(false);
                }
                b'k' => {
                    let mut state = state.borrow_mut();
                    state.count = state.count.saturating_sub(1);
                    drop(state);
                    screen.render_now(false);
                }
                _ => {}
            },
        }
    }
}

fn terminal_size() -> (usize, usize) {
    #[cfg(unix)]
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0
            && size.ws_col > 0
            && size.ws_row > 0
        {
            return (usize::from(size.ws_col), usize::from(size.ws_row));
        }
    }
    (80, 24)
}

#[cfg(unix)]
fn stdin_ready(timeout: Duration) -> bool {
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
fn stdin_ready(_timeout: Duration) -> bool {
    true
}

#[cfg(unix)]
fn enable_raw_mode(fd: i32) -> libc::termios {
    unsafe {
        let mut terminal: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut terminal) != 0 {
            return terminal;
        }
        let original = terminal;
        libc::cfmakeraw(&mut terminal);
        libc::tcsetattr(fd, libc::TCSANOW, &terminal);
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
fn enable_raw_mode(_fd: i32) {}

#[cfg(not(unix))]
fn restore_raw_mode(_fd: i32, _original: ()) {}
