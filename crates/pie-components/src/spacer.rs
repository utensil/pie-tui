//! Spacer — renders N empty lines (reference `components/spacer.js`).

use crate::Component;

#[derive(Default)]
pub struct Spacer {
    lines: usize,
}

impl Spacer {
    pub fn new(lines: usize) -> Self {
        Spacer { lines }
    }

    pub fn set_lines(&mut self, lines: usize) {
        self.lines = lines;
    }
}

impl Component for Spacer {
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec![String::new(); self.lines]
    }
}
