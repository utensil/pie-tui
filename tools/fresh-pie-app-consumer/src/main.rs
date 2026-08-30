use std::cell::RefCell;
use std::rc::Rc;

use pie_app::{
    DetachedTuiScreenRuntime, TuiAltScreen, TuiAltScreenEnvironment, TuiAltScreenOptions,
    TuiMainScreen,
};
use pie_components::{Component, ComponentHandle, Tui, TuiStopOptions};
use pie_term::{InputHandler, ResizeHandler, Terminal};

#[derive(Clone)]
struct ConsumerTerminal {
    writes: Rc<RefCell<Vec<String>>>,
}

impl Terminal for ConsumerTerminal {
    fn start(&mut self, _on_input: InputHandler, _on_resize: ResizeHandler) {}
    fn stop(&mut self) {}
    fn write(&mut self, data: &str) {
        self.writes.borrow_mut().push(data.into());
    }
    fn columns(&self) -> usize {
        12
    }
    fn rows(&self) -> usize {
        4
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _lines: isize) {}
    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _title: &str) {}
    fn set_progress(&mut self, _active: bool) {}
}

struct ConsumerComponent;

impl Component for ConsumerComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        vec![format!("consumer-{width}")]
    }
}

fn main() {
    let main_writes = Rc::new(RefCell::new(Vec::new()));
    let main = TuiMainScreen::new(
        Box::new(ConsumerTerminal {
            writes: main_writes.clone(),
        }),
        Box::new(DetachedTuiScreenRuntime::default()),
        false,
    );
    let component = ComponentHandle::new(ConsumerComponent);
    main.add_child(component.as_component_ref());
    main.start();
    main.render_now(true);
    main.stop(TuiStopOptions {
        preserve_screen: true,
    });
    assert!(
        main_writes
            .borrow()
            .iter()
            .any(|write| write.contains("consumer-12"))
    );

    let alt_writes = Rc::new(RefCell::new(Vec::new()));
    let alt = TuiAltScreen::new(
        Box::new(ConsumerTerminal {
            writes: alt_writes.clone(),
        }),
        Box::new(DetachedTuiScreenRuntime::default()),
        false,
        TuiAltScreenOptions {
            wheel_scroll_lines: 1,
            mouse: false,
            open_url: None,
            on_right_click_paste: None,
            environment: TuiAltScreenEnvironment {
                multiplexer: false,
                is_windows: false,
            },
        },
    );
    alt.add_child(component.as_component_ref());
    alt.start();
    alt.render_now(true);
    alt.stop(TuiStopOptions {
        preserve_screen: true,
    });
    let writes = alt_writes.borrow();
    assert!(writes.iter().any(|write| write.contains("\x1b[?1049h")));
    assert!(writes.iter().any(|write| write.contains("consumer-12")));
    assert!(writes.iter().any(|write| write.contains("\x1b[?1049l")));

    println!("pie-app fresh Rust consumer: pinned Main/Alt controller surface");
}
