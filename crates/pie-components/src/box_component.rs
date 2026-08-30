//! Box — container applying horizontal/vertical padding and an optional
//! background to all children (reference `components/box.js`).

use pie_core::text::visible_width;
use pie_core::wrap::apply_background_to_line;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Component, StyleFn};

static NEXT_BOX_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoxChildId {
    owner: u64,
    child: u64,
}

struct BoxCache {
    lines: Vec<String>,
    child_lines: Vec<String>,
    width: usize,
    bg_sample: Option<String>,
}

/// Box component.
pub struct BoxComponent {
    children: Vec<Box<dyn Component>>,
    child_ids: Vec<BoxChildId>,
    owner: u64,
    next_child: u64,
    padding_x: usize,
    padding_y: usize,
    bg_fn: Option<StyleFn>,
    cache: Option<BoxCache>,
}

impl BoxComponent {
    pub fn new(padding_x: usize, padding_y: usize) -> Self {
        BoxComponent {
            children: Vec::new(),
            child_ids: Vec::new(),
            owner: NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed),
            next_child: 0,
            padding_x,
            padding_y,
            bg_fn: None,
            cache: None,
        }
    }

    pub fn with_bg_fn(padding_x: usize, padding_y: usize, bg_fn: StyleFn) -> Self {
        BoxComponent {
            children: Vec::new(),
            child_ids: Vec::new(),
            owner: NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed),
            next_child: 0,
            padding_x,
            padding_y,
            bg_fn: Some(bg_fn),
            cache: None,
        }
    }

    pub fn add_child(&mut self, component: Box<dyn Component>) -> BoxChildId {
        let id = BoxChildId {
            owner: self.owner,
            child: self.next_child,
        };
        self.next_child = self.next_child.wrapping_add(1);
        self.children.push(component);
        self.child_ids.push(id);
        self.cache = None;
        id
    }

    pub fn remove_child(&mut self, id: BoxChildId) {
        if id.owner != self.owner {
            return;
        }
        if let Some(index) = self.child_ids.iter().position(|candidate| *candidate == id) {
            self.children.remove(index);
            self.child_ids.remove(index);
            self.cache = None;
        }
    }

    pub fn clear(&mut self) {
        self.children.clear();
        self.child_ids.clear();
        self.cache = None;
    }

    pub fn set_bg_fn(&mut self, bg_fn: Option<StyleFn>) {
        self.bg_fn = bg_fn;
        // No invalidation here — bg changes are detected by sampling output.
    }

    fn apply_bg(&self, line: &str, width: usize) -> String {
        let vis_len = visible_width(line);
        let pad_needed = width.saturating_sub(vis_len);
        let padded = format!("{line}{}", " ".repeat(pad_needed));
        match &self.bg_fn {
            Some(bg) => apply_background_to_line(&padded, width, bg),
            None => padded,
        }
    }

    fn bg_sample(&self) -> Option<String> {
        self.bg_fn.as_ref().map(|bg| bg("test"))
    }

    fn match_cache(
        &self,
        width: usize,
        child_lines: &[String],
        bg_sample: &Option<String>,
    ) -> bool {
        match &self.cache {
            Some(cache) => {
                cache.width == width
                    && cache.bg_sample.is_same(bg_sample)
                    && cache.child_lines.len() == child_lines.len()
                    && cache
                        .child_lines
                        .iter()
                        .zip(child_lines.iter())
                        .all(|(a, b)| a == b)
            }
            None => false,
        }
    }
}

trait SameOpt {
    fn is_same(&self, other: &Self) -> bool;
}
impl SameOpt for Option<String> {
    fn is_same(&self, other: &Self) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

impl Component for BoxComponent {
    fn invalidate(&mut self) {
        self.cache = None;
        for child in &mut self.children {
            child.invalidate();
        }
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        if self.children.is_empty() {
            return Vec::new();
        }
        let content_width = (width.saturating_sub(self.padding_x * 2)).max(1);
        let left_pad = " ".repeat(self.padding_x);
        let mut child_lines: Vec<String> = Vec::new();
        for child in &mut self.children {
            for line in child.render(content_width) {
                child_lines.push(format!("{left_pad}{line}"));
            }
        }
        if child_lines.is_empty() {
            return Vec::new();
        }
        let bg_sample = self.bg_sample();
        if self.match_cache(width, &child_lines, &bg_sample) {
            return self.cache.as_ref().unwrap().lines.clone();
        }
        let mut result = Vec::new();
        for _ in 0..self.padding_y {
            result.push(self.apply_bg("", width));
        }
        for line in &child_lines {
            result.push(self.apply_bg(line, width));
        }
        for _ in 0..self.padding_y {
            result.push(self.apply_bg("", width));
        }
        self.cache = Some(BoxCache {
            lines: result.clone(),
            child_lines,
            width,
            bg_sample,
        });
        result
    }
}
