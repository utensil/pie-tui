//! Internal viewport layout tree and frame compositor.
//!
//! This is the Rust seam corresponding to the reference's deep
//! `layout-node.js`/`layout.js` modules. It is intentionally not re-exported as
//! a canonical top-level symbol.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::rc::Rc;

use pie_core::screen::{CURSOR_MARKER, composite_tui_line, is_image_line};
use pie_core::text::{extract_ansi_code_len, visible_width};
use pie_core::wrap::{get_grapheme_cell_range, slice_by_column};

use crate::Component;
use crate::scroll_view::{ScrollView, ScrollbarStyle};
use crate::stack::{StackViewport, allocate_stack_sizes};
use crate::vstack_hstack::{Align, StackData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRect {
    pub x: i64,
    pub y: i64,
    pub width: usize,
    pub height: usize,
}

impl LayoutRect {
    fn right(self) -> i64 {
        self.x.saturating_add(self.width as i64)
    }

    fn bottom(self) -> i64 {
        self.y.saturating_add(self.height as i64)
    }

    pub fn contains(self, x: i64, y: i64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self {
            x,
            y,
            width: usize::try_from(right.saturating_sub(x).max(0)).unwrap_or(usize::MAX),
            height: usize::try_from(bottom.saturating_sub(y).max(0)).unwrap_or(usize::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScrollViewId(pub(crate) u64);

#[derive(Debug, Clone, Copy)]
pub struct LayoutAllocation {
    pub x: i64,
    pub y: i64,
    pub width: usize,
    pub height: Option<usize>,
    pub clip: LayoutRect,
}

impl LayoutAllocation {
    pub fn root(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height: Some(height),
            clip: LayoutRect {
                x: 0,
                y: 0,
                width,
                height,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutBox {
    pub component_id: usize,
    pub rect: LayoutRect,
    pub clip: LayoutRect,
    pub children: Vec<LayoutBox>,
    pub lines: Option<Vec<String>>,
    pub line_offset: Option<usize>,
    pub scroll_view: Option<ScrollViewId>,
    pub scroll_content_lines: Option<Vec<String>>,
    pub layer: usize,
    pub(crate) scrollbar: Option<ScrollbarPaint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutFrame {
    pub root: LayoutBox,
    pub width: usize,
    pub height: usize,
    pub lines: Vec<String>,
    pub primary_scroll_view: Option<ScrollViewId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScrollbarPaint {
    pub visible: bool,
    pub style: ScrollbarStyle,
    pub scroll_top: usize,
    pub content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarGeometry {
    pub column: i64,
    pub track_top: i64,
    pub track_height: usize,
    pub thumb_top: i64,
    pub thumb_height: usize,
    pub max_scroll_top: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RenderCacheIdentity {
    Direct(usize),
    Mount(u64),
}

pub struct LayoutContext {
    viewport: StackViewport,
    render_cache: HashMap<(RenderCacheIdentity, usize), Vec<String>>,
    mounted_identities: Vec<(usize, u64)>,
    request_render: Rc<dyn Fn()>,
    primary_scroll_view: Option<ScrollViewId>,
}

impl LayoutContext {
    fn new(width: usize, height: usize, request_render: Rc<dyn Fn()>) -> Self {
        Self {
            viewport: StackViewport { width, height },
            render_cache: HashMap::new(),
            mounted_identities: Vec::new(),
            request_render,
            primary_scroll_view: None,
        }
    }

    pub fn viewport(&self) -> StackViewport {
        self.viewport
    }

    pub fn request_render_callback(&self) -> Rc<dyn Fn()> {
        self.request_render.clone()
    }

    pub fn select_primary_scroll_view(&mut self, id: ScrollViewId, explicit: bool) {
        if explicit || self.primary_scroll_view.is_none() {
            self.primary_scroll_view = Some(id);
        }
    }

    pub fn render_cached<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
    ) -> Vec<String> {
        let object_identity = component.render_identity();
        let identity = self
            .mounted_identities
            .iter()
            .rev()
            .find_map(|(candidate, mount)| {
                (*candidate == object_identity).then_some(RenderCacheIdentity::Mount(*mount))
            })
            .unwrap_or(RenderCacheIdentity::Direct(object_identity));
        self.render_cached_with_identity(component, width, identity)
    }

    fn render_cached_with_identity<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
        identity: RenderCacheIdentity,
    ) -> Vec<String> {
        let safe_width = width.max(1);
        let key = (identity, safe_width);
        if let Some(lines) = self.render_cache.get(&key) {
            return lines.clone();
        }
        let lines = component.render(safe_width);
        self.render_cache.insert(key, lines.clone());
        lines
    }

    pub(crate) fn render_cached_mounted<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
        mount: u64,
    ) -> Vec<String> {
        self.render_cached_with_identity(component, width, RenderCacheIdentity::Mount(mount))
    }

    pub(crate) fn measure_width_mounted<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
        mount: u64,
    ) -> usize {
        self.render_cached_mounted(component, width, mount)
            .iter()
            .map(|line| visible_width(line))
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn layout_mounted<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        mount: u64,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        let object_identity = component.render_identity();
        self.mounted_identities.push((object_identity, mount));
        let layout_box = component.layout(self, allocation);
        let popped = self.mounted_identities.pop();
        debug_assert_eq!(popped, Some((object_identity, mount)));
        layout_box
    }

    pub fn measure_height<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
    ) -> usize {
        self.render_cached(component, width).len()
    }

    pub fn measure_width<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        width: usize,
    ) -> usize {
        self.render_cached(component, width)
            .iter()
            .map(|line| visible_width(line))
            .max()
            .unwrap_or(0)
    }

    pub fn layout_component(
        &mut self,
        component: &mut dyn Component,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        component.layout(self, allocation)
    }

    pub fn layout_leaf<C: Component + ?Sized>(
        &mut self,
        component: &mut C,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        let safe_width = allocation.width.max(1);
        let lines = self.render_cached(component, safe_width);
        let allocated_height = allocation.height.unwrap_or(lines.len());
        let mut line_offset = 0;
        if lines.len() > allocated_height
            && allocated_height > 0
            && let Some(cursor_line) = lines.iter().position(|line| line.contains(CURSOR_MARKER))
            && cursor_line >= allocated_height
        {
            line_offset = cursor_line - allocated_height + 1;
        }
        let rect = LayoutRect {
            x: allocation.x,
            y: allocation.y,
            width: safe_width,
            height: allocated_height,
        };
        LayoutBox {
            component_id: component.render_identity(),
            rect,
            clip: allocation.clip.intersection(rect),
            children: Vec::new(),
            lines: Some(lines),
            line_offset: Some(line_offset),
            scroll_view: None,
            scroll_content_lines: None,
            layer: 0,
            scrollbar: None,
        }
    }

    pub fn layout_vstack(
        &mut self,
        data: &mut StackData,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        let safe_width = allocation.width.max(1);
        let visible = data.visible_indices(self.viewport);
        let intrinsic = visible
            .iter()
            .map(|index| {
                data.entries[*index]
                    .basis
                    .unwrap_or_else(|| data.measure_child_height(*index, self, safe_width))
            })
            .collect::<Vec<_>>();
        let entries = visible
            .iter()
            .map(|index| data.entries[*index].clone())
            .collect::<Vec<_>>();
        let sizes = allocate_stack_sizes(&entries, &intrinsic, allocation.height, data.gap);
        let gap_total = visible.len().saturating_sub(1).saturating_mul(data.gap);
        let natural_height = sizes
            .iter()
            .copied()
            .sum::<usize>()
            .saturating_add(gap_total);
        let allocated_height = allocation.height.unwrap_or(natural_height);
        let rect = LayoutRect {
            x: allocation.x,
            y: allocation.y,
            width: safe_width,
            height: allocated_height,
        };
        let clip = allocation.clip.intersection(rect);
        let mut children = Vec::with_capacity(visible.len());
        let mut child_y = allocation.y;
        for (visible_index, child_index) in visible.iter().enumerate() {
            children.push(data.layout_child(
                *child_index,
                self,
                LayoutAllocation {
                    x: allocation.x,
                    y: child_y,
                    width: safe_width,
                    height: Some(sizes[visible_index]),
                    clip,
                },
            ));
            child_y = child_y.saturating_add(
                i64::try_from(sizes[visible_index].saturating_add(data.gap)).unwrap_or(i64::MAX),
            );
        }
        LayoutBox {
            component_id: data.render_identity(),
            rect,
            clip,
            children,
            lines: None,
            line_offset: None,
            scroll_view: None,
            scroll_content_lines: None,
            layer: 0,
            scrollbar: None,
        }
    }

    pub fn layout_hstack(
        &mut self,
        data: &mut StackData,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        let safe_width = allocation.width.max(1);
        let visible = data.visible_indices(self.viewport);
        let intrinsic_widths = visible
            .iter()
            .map(|index| {
                data.entries[*index]
                    .basis
                    .unwrap_or_else(|| data.measure_child_width(*index, self, safe_width))
            })
            .collect::<Vec<_>>();
        let entries = visible
            .iter()
            .map(|index| data.entries[*index].clone())
            .collect::<Vec<_>>();
        let widths = allocate_stack_sizes(&entries, &intrinsic_widths, Some(safe_width), data.gap);
        let intrinsic_heights = visible
            .iter()
            .enumerate()
            .map(|(index, child)| data.measure_child_height(*child, self, widths[index].max(1)))
            .collect::<Vec<_>>();
        let allocated_height = allocation
            .height
            .unwrap_or_else(|| intrinsic_heights.iter().copied().max().unwrap_or(0));
        let rect = LayoutRect {
            x: allocation.x,
            y: allocation.y,
            width: safe_width,
            height: allocated_height,
        };
        let clip = allocation.clip.intersection(rect);
        let mut children = Vec::with_capacity(visible.len());
        let mut child_x = allocation.x;
        for (visible_index, child_index) in visible.iter().enumerate() {
            let natural_height = intrinsic_heights[visible_index];
            let child_height = if data.align == Align::Stretch {
                allocated_height
            } else {
                allocated_height.min(natural_height)
            };
            let offset = match data.align {
                Align::Center => allocated_height.saturating_sub(child_height) / 2,
                Align::End => allocated_height.saturating_sub(child_height),
                Align::Stretch | Align::Start => 0,
            };
            let child_y = allocation
                .y
                .saturating_add(i64::try_from(offset).unwrap_or(i64::MAX));
            let child_width = widths[visible_index];
            if child_width == 0 {
                children.push(LayoutBox {
                    component_id: data.child_identity(*child_index),
                    rect: LayoutRect {
                        x: child_x,
                        y: child_y,
                        width: 0,
                        height: child_height,
                    },
                    clip: LayoutRect {
                        x: child_x,
                        y: child_y,
                        width: 0,
                        height: 0,
                    },
                    children: Vec::new(),
                    lines: None,
                    line_offset: None,
                    scroll_view: None,
                    scroll_content_lines: None,
                    layer: 0,
                    scrollbar: None,
                });
            } else {
                children.push(data.layout_child(
                    *child_index,
                    self,
                    LayoutAllocation {
                        x: child_x,
                        y: child_y,
                        width: child_width,
                        height: Some(child_height),
                        clip,
                    },
                ));
            }
            child_x = child_x.saturating_add(
                i64::try_from(child_width.saturating_add(data.gap)).unwrap_or(i64::MAX),
            );
        }
        LayoutBox {
            component_id: data.render_identity(),
            rect,
            clip,
            children,
            lines: None,
            line_offset: None,
            scroll_view: None,
            scroll_content_lines: None,
            layer: 0,
            scrollbar: None,
        }
    }

    pub fn layout_scroll_view(
        &mut self,
        scroll_view: &mut ScrollView,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        let safe_width = allocation.width.max(1);
        let previous_scroll_top = scroll_view.scroll_top();
        let content_width = scroll_view.get_content_width(safe_width);
        let previous = i64::try_from(previous_scroll_top).unwrap_or(i64::MAX);
        let mut child = scroll_view.layout_child(
            self,
            LayoutAllocation {
                x: allocation.x,
                y: allocation.y.saturating_sub(previous),
                width: content_width,
                height: None,
                clip: allocation.clip,
            },
        );
        let content_height = child.rect.height;
        let viewport_height = allocation.height.unwrap_or(content_height);
        scroll_view.update_layout(
            content_height,
            viewport_height,
            self.request_render_callback(),
        );
        let current = i64::try_from(scroll_view.scroll_top()).unwrap_or(i64::MAX);
        translate_box(&mut child, previous.saturating_sub(current));
        let id = scroll_view.id();
        self.select_primary_scroll_view(id, scroll_view.primary());
        let rect = LayoutRect {
            x: allocation.x,
            y: allocation.y,
            width: safe_width,
            height: viewport_height,
        };
        let clip = allocation.clip.intersection(rect);
        update_clips(&mut child, clip);
        let scroll_content_lines = scroll_view.render_child_cached(self, content_width);
        LayoutBox {
            component_id: scroll_view.render_identity(),
            rect,
            clip,
            children: vec![child],
            lines: None,
            line_offset: None,
            scroll_view: Some(id),
            scroll_content_lines: Some(scroll_content_lines),
            layer: 0,
            scrollbar: Some(scroll_view.paint_state()),
        }
    }
}

pub fn translate_box(layout_box: &mut LayoutBox, delta_y: i64) {
    layout_box.rect.y = layout_box.rect.y.saturating_add(delta_y);
    for child in &mut layout_box.children {
        translate_box(child, delta_y);
    }
}

pub fn update_clips(layout_box: &mut LayoutBox, parent_clip: LayoutRect) {
    layout_box.clip = parent_clip.intersection(layout_box.rect);
    let clip = layout_box.clip;
    for child in &mut layout_box.children {
        update_clips(child, clip);
    }
}

fn rounded_ratio(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return usize::MAX;
    }
    numerator
        .saturating_add(denominator / 2)
        .checked_div(denominator)
        .unwrap_or(usize::MAX)
}

pub fn get_scrollbar_geometry(layout_box: &LayoutBox) -> Option<ScrollbarGeometry> {
    let scrollbar = layout_box.scrollbar.as_ref()?;
    if !scrollbar.visible || layout_box.rect.width == 0 || layout_box.rect.height == 0 {
        return None;
    }
    let content_height = layout_box
        .children
        .first()
        .map(|child| child.rect.height)
        .or_else(|| layout_box.scroll_content_lines.as_ref().map(Vec::len))
        .unwrap_or(scrollbar.content_height);
    let track_height = layout_box.rect.height;
    let minimum_thumb_height = 2.min(track_height);
    let ratio = rounded_ratio(track_height.saturating_mul(track_height), content_height);
    let thumb_height = minimum_thumb_height.max(track_height.min(ratio));
    let max_scroll_top = content_height.saturating_sub(track_height);
    let max_thumb_top = track_height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll_top == 0 {
        0
    } else {
        rounded_ratio(
            scrollbar.scroll_top.saturating_mul(max_thumb_top),
            max_scroll_top,
        )
    };
    let column = layout_box
        .rect
        .x
        .saturating_add(i64::try_from(layout_box.rect.width - 1).unwrap_or(i64::MAX));
    if column < layout_box.clip.x || column >= layout_box.clip.right() {
        return None;
    }
    Some(ScrollbarGeometry {
        column,
        track_top: layout_box.rect.y,
        track_height,
        thumb_top: layout_box
            .rect
            .y
            .saturating_add(i64::try_from(thumb_offset).unwrap_or(i64::MAX)),
        thumb_height,
        max_scroll_top,
    })
}

fn style_scrollbar_cell(
    line: &str,
    column: usize,
    total_width: usize,
    style: &ScrollbarStyle,
) -> String {
    if is_image_line(line) {
        return line.to_string();
    }
    let range = get_grapheme_cell_range(line, column);
    let start = range.map_or(column, |range| range.start);
    let end = range.map_or(column.saturating_add(1), |range| range.end);
    let before = slice_by_column(line, 0, start, true);
    let target = slice_by_column(line, start, end.saturating_sub(start), true);
    let after = slice_by_column(line, end, total_width.saturating_sub(end), true);
    let mut target_index = 0;
    while target_index < target.len() {
        let Some(length) = extract_ansi_code_len(&target, target_index) else {
            break;
        };
        target_index += length;
    }
    let target_prefix = &target[..target_index];
    let target_text = if target_index == target.len() {
        " ".repeat(end.saturating_sub(start))
    } else {
        target[target_index..].to_string()
    };
    let before_padding = " ".repeat(start.saturating_sub(visible_width(&before)));
    format!(
        "{before}{before_padding}{target_prefix}{}{after}",
        style.apply(&target_text)
    )
}

fn paint_scrollbar(layout_box: &LayoutBox, screen: &mut [String], total_width: usize) {
    let Some(geometry) = get_scrollbar_geometry(layout_box) else {
        return;
    };
    let Some(scrollbar) = &layout_box.scrollbar else {
        return;
    };
    for offset in 0..geometry.thumb_height {
        let row = geometry
            .thumb_top
            .saturating_add(i64::try_from(offset).unwrap_or(i64::MAX));
        if row < layout_box.clip.y
            || row >= layout_box.clip.bottom()
            || row < 0
            || row >= screen.len() as i64
        {
            continue;
        }
        let row = usize::try_from(row).expect("scrollbar row is non-negative");
        let column = usize::try_from(geometry.column.max(0)).unwrap_or(usize::MAX);
        screen[row] = style_scrollbar_cell(&screen[row], column, total_width, &scrollbar.style);
    }
}

fn strip_leading_osc133_zone_prefixes(mut line: &str) -> &str {
    loop {
        let Some(zone) = line.strip_prefix("\x1b]133;") else {
            return line;
        };
        let Some(marker) = zone.as_bytes().first() else {
            return line;
        };
        if !matches!(marker, b'A' | b'B' | b'C') {
            return line;
        }
        let tail = &zone[1..];
        if let Some(rest) = tail.strip_prefix('\x07') {
            line = rest;
        } else if let Some(rest) = tail.strip_prefix("\x1b\\") {
            line = rest;
        } else {
            return line;
        }
    }
}

fn paint_box(layout_box: &LayoutBox, screen: &mut [String], total_width: usize) {
    if let Some(lines) = &layout_box.lines {
        let offset = layout_box.line_offset.unwrap_or(0);
        let first_row = layout_box.rect.y.max(layout_box.clip.y).max(0);
        let last_row = layout_box
            .rect
            .bottom()
            .min(layout_box.clip.bottom())
            .min(screen.len() as i64);
        for row in first_row..last_row {
            let source_index = offset.saturating_add(
                usize::try_from(row.saturating_sub(layout_box.rect.y)).unwrap_or(usize::MAX),
            );
            let Some(line) = lines.get(source_index) else {
                continue;
            };
            let line = strip_leading_osc133_zone_prefixes(line);
            let target = usize::try_from(row).expect("paint row is non-negative");
            if is_image_line(line) && layout_box.rect.x == 0 && layout_box.rect.width >= total_width
            {
                screen[target] = line.to_string();
            } else {
                let x = usize::try_from(layout_box.rect.x.max(0)).unwrap_or(usize::MAX);
                screen[target] = composite_tui_line(
                    &screen[target],
                    line,
                    x,
                    layout_box.rect.width,
                    total_width,
                );
            }
        }
    }
    for child in &layout_box.children {
        paint_box(child, screen, total_width);
    }
    paint_scrollbar(layout_box, screen, total_width);
}

pub fn render_layout_frame(
    root: &mut dyn Component,
    width: usize,
    height: usize,
    request_render: Rc<dyn Fn()>,
) -> LayoutFrame {
    let safe_width = width.max(1);
    let safe_height = height.max(1);
    let mut context = LayoutContext::new(safe_width, safe_height, request_render);
    let root_box = context.layout_component(root, LayoutAllocation::root(safe_width, safe_height));
    let mut lines = vec![String::new(); safe_height];
    paint_box(&root_box, &mut lines, safe_width);
    LayoutFrame {
        root: root_box,
        width: safe_width,
        height: safe_height,
        lines,
        primary_scroll_view: context.primary_scroll_view,
    }
}

pub fn get_scroll_view_box(frame: &LayoutFrame, scroll_view: ScrollViewId) -> Option<&LayoutBox> {
    fn visit(layout_box: &LayoutBox, scroll_view: ScrollViewId) -> Option<&LayoutBox> {
        if layout_box.scroll_view == Some(scroll_view) {
            return Some(layout_box);
        }
        layout_box
            .children
            .iter()
            .find_map(|child| visit(child, scroll_view))
    }
    visit(&frame.root, scroll_view)
}

pub fn get_scroll_views_at(frame: &LayoutFrame, x: i64, y: i64) -> Vec<ScrollViewId> {
    fn visit(
        layout_box: &LayoutBox,
        x: i64,
        y: i64,
        depth: usize,
        matches: &mut Vec<(ScrollViewId, usize)>,
    ) {
        if !layout_box.clip.contains(x, y) {
            return;
        }
        if let Some(scroll_view) = layout_box.scroll_view
            && layout_box.rect.contains(x, y)
        {
            matches.push((scroll_view, depth));
        }
        for child in &layout_box.children {
            visit(child, x, y, depth + 1, matches);
        }
    }
    let mut matches = Vec::new();
    visit(&frame.root, x, y, 0, &mut matches);
    matches.sort_by_key(|entry| Reverse(entry.1));
    matches
        .into_iter()
        .map(|(scroll_view, _)| scroll_view)
        .collect()
}
