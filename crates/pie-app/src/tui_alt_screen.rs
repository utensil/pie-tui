//! Concrete alternate-screen controller over the shared TUI lifecycle.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use pie_components::layout::{
    LayoutFrame, ScrollViewId, get_scroll_view_box, get_scroll_views_at, render_layout_frame,
};
use pie_components::{
    Component, ScrollView, ScrollViewFollow, ScrollViewOptions, TuiInputListener,
    TuiInputListenerResult, TuiMode, TuiStopOptions, ViewportTui,
};
use pie_core::keys::is_key_release;
use pie_core::screen::{
    CURSOR_MARKER, apply_line_resets, composite_tui_line, extract_cursor_position, is_image_line,
};
use pie_core::terminal_image::{
    ImageProtocol, KittyImageMetadata, KittyImageRegistry, delete_all_kitty_images,
    delete_all_kitty_placements, delete_kitty_image,
};
use pie_core::text::{extract_ansi_code_len, strip_terminal_sequences, visible_width};
use pie_core::word_navigation::default_word_segments;
use pie_core::wrap::{
    get_grapheme_cell_range, get_osc8_link_at_column, slice_by_column, truncate_to_width,
};
use pie_term::Terminal;
use pie_term::capabilities::{TerminalCapabilities, get_capabilities, set_capabilities};

use crate::screen_runtime::{
    ScreenControllerHost, ScreenLifecycle, SharedTerminal, TuiScreenRuntime, delegate_tui,
    shared_runtime,
};
use crate::tui_controller::{TuiBaseController, TuiHostTask, TuiTaskId, WeakTuiBaseController};

const ENTER_ALT_SCREEN: &str = "\x1b[?1049h";
const EXIT_ALT_SCREEN: &str = "\x1b[?1049l";
const DISABLE_AUTOWRAP: &str = "\x1b[?7l";
const ENABLE_AUTOWRAP: &str = "\x1b[?7h";
const ENABLE_BUTTON_MOTION_MOUSE: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1004h\x1b[?1006h";
const ENABLE_ALL_MOTION_MOUSE: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1004h\x1b[?1006h";
const DISABLE_MOUSE: &str = "\x1b[?1006l\x1b[?1004l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
const FOCUS_IN: &str = "\x1b[I";
const FOCUS_OUT: &str = "\x1b[O";
const BEGIN_SYNCHRONIZED_OUTPUT: &str = "\x1b[?2026h";
const END_SYNCHRONIZED_OUTPUT: &str = "\x1b[?2026l";
const PAGE_SCROLL_OVERLAP: usize = 4;
const MAX_CACHED_OFFSCREEN_KITTY_IMAGES: usize = 16;
const MAX_CACHED_OFFSCREEN_KITTY_TRANSMISSION_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHED_OFFSCREEN_KITTY_DECODED_BYTES: u128 = 64 * 1024 * 1024;
const DOUBLE_CLICK_INTERVAL_MS: u64 = 500;
const DEFAULT_FLASH_DURATION_MS: u64 = 1_000;

/// Stable environment facts which affect alternate-screen terminal setup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TuiAltScreenEnvironment {
    pub multiplexer: bool,
    pub is_windows: bool,
}

impl TuiAltScreenEnvironment {
    pub fn current() -> Self {
        let term = std::env::var("TERM")
            .unwrap_or_default()
            .to_ascii_lowercase();
        Self {
            multiplexer: std::env::var_os("TMUX").is_some()
                || std::env::var_os("ZELLIJ").is_some()
                || std::env::var_os("STY").is_some()
                || term.starts_with("tmux")
                || term.starts_with("screen"),
            is_windows: cfg!(windows),
        }
    }
}

pub type OpenUrlCallback = Rc<dyn Fn(&str)>;
pub type RightClickPasteCallback = Rc<dyn Fn()>;

/// Alternate-screen behavior options corresponding to the pinned 0.84.1
/// controller surface, with process facts made injectable.
pub struct TuiAltScreenOptions {
    pub wheel_scroll_lines: usize,
    pub mouse: bool,
    pub open_url: Option<OpenUrlCallback>,
    pub on_right_click_paste: Option<RightClickPasteCallback>,
    pub environment: TuiAltScreenEnvironment,
}

impl Default for TuiAltScreenOptions {
    fn default() -> Self {
        Self {
            wheel_scroll_lines: 1,
            mouse: true,
            open_url: None,
            on_right_click_paste: None,
            environment: TuiAltScreenEnvironment::current(),
        }
    }
}

struct BaseDocument {
    base: WeakTuiBaseController,
}

impl Component for BaseDocument {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.base
            .upgrade()
            .map_or_else(Vec::new, |base| base.render_document(width))
    }

    fn invalidate(&mut self) {
        if let Some(base) = self.base.upgrade() {
            base.invalidate();
        }
    }
}

#[derive(Clone)]
struct CachedKittyImage {
    image_id: u32,
    transmission_generation: u64,
    transmission_bytes: usize,
    estimated_decoded_bytes: u128,
}

struct AltRenderState {
    previous_screen: Vec<String>,
    last_document: Vec<String>,
    previous_width: usize,
    previous_height: usize,
    current_layout: Option<LayoutFrame>,
    implicit_scroll_view: Option<ScrollView>,
    alt_screen_active: bool,
    image_protocol: Option<ImageProtocol>,
    saved_capabilities: Option<Arc<TerminalCapabilities>>,
    kitty_image_registry: KittyImageRegistry,
    uploaded_kitty_images: VecDeque<CachedKittyImage>,
}

impl AltRenderState {
    fn reset(&mut self) {
        self.previous_screen.clear();
        self.previous_width = 0;
        self.previous_height = 0;
        self.current_layout = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionGranularity {
    Character,
    Word,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionPoint {
    row: usize,
    col: usize,
    boundary: bool,
    scroll_view: Option<ScrollViewId>,
}

impl SelectionPoint {
    fn new(row: usize, col: usize, scroll_view: Option<ScrollViewId>) -> Self {
        Self {
            row,
            col,
            boundary: false,
            scroll_view,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SelectionRange {
    start: SelectionPoint,
    end: SelectionPoint,
}

#[derive(Debug, Clone, Copy)]
struct LastClick {
    timestamp: u64,
    count: u8,
    row: usize,
    scroll_view: Option<ScrollViewId>,
    word_start: usize,
    word_end: usize,
}

#[derive(Default)]
struct SelectionState {
    anchor: Option<SelectionPoint>,
    focus: Option<SelectionPoint>,
    granularity: Option<SelectionGranularity>,
    initial_range: Option<SelectionRange>,
    last_click: Option<LastClick>,
    press_active: bool,
    dragged: bool,
    pressed_url: Option<String>,
}

impl SelectionState {
    fn reset(&mut self) {
        *self = Self {
            granularity: Some(SelectionGranularity::Character),
            ..Self::default()
        };
    }
}

struct FlashEntry {
    id: u64,
    message: String,
    task: u64,
}

#[derive(Default)]
struct FlashState {
    next_id: u64,
    entries: Vec<FlashEntry>,
}

struct AltScreenState {
    base: RefCell<Option<WeakTuiBaseController>>,
    terminal: SharedTerminal,
    render: RefCell<AltRenderState>,
    selection: RefCell<SelectionState>,
    flashes: RefCell<FlashState>,
    wheel_scroll_lines: usize,
    mouse_enabled: bool,
    open_url: Option<OpenUrlCallback>,
    on_right_click_paste: Option<RightClickPasteCallback>,
    environment: TuiAltScreenEnvironment,
}

impl AltScreenState {
    fn base(&self) -> Option<TuiBaseController> {
        self.base.borrow().as_ref()?.upgrade()
    }

    fn set_base(&self, base: WeakTuiBaseController) {
        *self.base.borrow_mut() = Some(base);
    }

    fn request_render(&self) {
        if let Some(base) = self.base() {
            base.request_render(false);
        }
    }

    fn render_layout(&self, width: usize, height: usize) -> LayoutFrame {
        let request_base = self.base.borrow().clone();
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(base) = request_base
                .as_ref()
                .and_then(WeakTuiBaseController::upgrade)
            {
                base.request_render(false);
            }
        });
        if let Some(mut root) = self.base().and_then(|base| base.layout_root()) {
            return render_layout_frame(&mut root, width, height, request_render);
        }

        let mut scroll_view = self
            .render
            .borrow_mut()
            .implicit_scroll_view
            .take()
            .expect("implicit alternate-screen ScrollView is present");
        let frame = render_layout_frame(&mut scroll_view, width, height, request_render);
        self.render.borrow_mut().implicit_scroll_view = Some(scroll_view);
        frame
    }

    fn render_screen(&self) {
        let Some(base) = self.base() else {
            return;
        };
        if !self.render.borrow().alt_screen_active {
            return;
        }
        let width = base.terminal_columns().max(1);
        let height = base.terminal_rows().max(1);
        let next_layout = self.render_layout(width, height);
        let mut screen = next_layout
            .lines
            .iter()
            .map(|line| strip_osc133_zone_prefix(line))
            .collect::<Vec<_>>();
        screen = base.composite_overlays(screen, width, height);
        if screen.len() > height {
            screen = screen.split_off(screen.len() - height);
        }
        screen = self.apply_selection(screen, &next_layout);
        screen = self.composite_flashes(screen, width, height);
        let cursor = extract_cursor_position(&mut screen, height);
        screen = apply_line_resets(screen)
            .into_iter()
            .map(|line| {
                if is_image_line(&line) || visible_width(&line) <= width {
                    line
                } else {
                    slice_by_column(&line, 0, width, true)
                }
            })
            .collect();

        let (previous_screen, previous_width, previous_height, image_protocol, had_uploaded) = {
            let render = self.render.borrow();
            (
                render.previous_screen.clone(),
                render.previous_width,
                render.previous_height,
                render.image_protocol,
                !render.uploaded_kitty_images.is_empty(),
            )
        };
        let full_redraw =
            previous_screen.is_empty() || previous_width != width || previous_height != height;
        let images_need_redraw = screen.iter().enumerate().any(|(row, line)| {
            line != previous_screen.get(row).map_or("", String::as_str)
                && (is_image_line(line)
                    || previous_screen
                        .get(row)
                        .is_some_and(|line| is_image_line(line)))
        });
        let redraw_images = full_redraw || images_need_redraw;
        let (prepared_screen, evicted_deletion) =
            if redraw_images && image_protocol == Some(ImageProtocol::Kitty) {
                self.prepare_kitty_screen(&screen)
            } else {
                (screen.clone(), String::new())
            };

        let mut buffer = String::from(BEGIN_SYNCHRONIZED_OUTPUT);
        if full_redraw {
            base.set_full_redraws(base.full_redraws().saturating_add(1));
            let clear_images = if image_protocol == Some(ImageProtocol::Kitty) && had_uploaded {
                delete_all_kitty_placements()
            } else if image_protocol == Some(ImageProtocol::Kitty) {
                delete_all_kitty_images()
            } else {
                ""
            };
            buffer.push_str(clear_images);
            buffer.push_str("\x1b[2J");
        } else if images_need_redraw {
            match image_protocol {
                Some(ImageProtocol::ITerm2) => buffer.push_str("\x1b[2J"),
                Some(ImageProtocol::Kitty) => buffer.push_str(delete_all_kitty_placements()),
                None => {}
            }
        }
        buffer.push_str(&evicted_deletion);
        for row in 0..height {
            if !full_redraw
                && !images_need_redraw
                && screen.get(row).map(String::as_str)
                    == previous_screen.get(row).map(String::as_str)
            {
                continue;
            }
            buffer.push_str(&format!("\x1b[{};1H\x1b[2K", row + 1));
            if let Some(line) = prepared_screen.get(row) {
                buffer.push_str(line);
            }
        }
        if let Some(cursor) = cursor {
            buffer.push_str(&format!(
                "\x1b[{};{}H",
                cursor.row + 1,
                cursor.col.min(width) + 1
            ));
            buffer.push_str(if base.show_hardware_cursor() {
                "\x1b[?25h"
            } else {
                "\x1b[?25l"
            });
        } else {
            buffer.push_str("\x1b[?25l");
        }
        buffer.push_str(END_SYNCHRONIZED_OUTPUT);
        // See MainScreenState::render: the TuiBase action driver is the
        // reentrancy boundary around every renderer-owned terminal call.
        self.terminal.borrow_mut().write(&buffer);

        let mut render = self.render.borrow_mut();
        render.previous_screen = screen;
        render.previous_width = width;
        render.previous_height = height;
        render.current_layout = Some(next_layout);
    }

    fn prepare_kitty_screen(&self, screen: &[String]) -> (Vec<String>, String) {
        let mut render = self.render.borrow_mut();
        let mut visible_image_ids = BTreeSet::new();
        let mut lines = Vec::with_capacity(screen.len());
        for line in screen {
            let Some(image) = render.kitty_image_registry.placement_for_line(line) else {
                lines.push(line.clone());
                continue;
            };
            visible_image_ids.insert(image.image_id);
            let cached = render
                .uploaded_kitty_images
                .iter()
                .position(|cached| cached.image_id == image.image_id)
                .and_then(|index| render.uploaded_kitty_images.remove(index));
            let unchanged = cached.as_ref().is_some_and(|cached| {
                cached.transmission_generation == image.transmission_generation
            });
            render.uploaded_kitty_images.push_back(CachedKittyImage {
                image_id: image.image_id,
                transmission_generation: image.transmission_generation,
                transmission_bytes: image.transmission_bytes,
                estimated_decoded_bytes: image.estimated_decoded_bytes,
            });
            lines.push(if unchanged {
                image.replacement_line
            } else {
                line.clone()
            });
        }

        let mut offscreen_count = 0usize;
        let mut offscreen_transmission_bytes = 0usize;
        let mut offscreen_decoded_bytes = 0u128;
        for cached in &render.uploaded_kitty_images {
            if visible_image_ids.contains(&cached.image_id) {
                continue;
            }
            offscreen_count = offscreen_count.saturating_add(1);
            offscreen_transmission_bytes =
                offscreen_transmission_bytes.saturating_add(cached.transmission_bytes);
            offscreen_decoded_bytes =
                offscreen_decoded_bytes.saturating_add(cached.estimated_decoded_bytes);
        }

        let mut evicted_deletion = String::new();
        while offscreen_count > MAX_CACHED_OFFSCREEN_KITTY_IMAGES
            || offscreen_transmission_bytes > MAX_CACHED_OFFSCREEN_KITTY_TRANSMISSION_BYTES
            || offscreen_decoded_bytes > MAX_CACHED_OFFSCREEN_KITTY_DECODED_BYTES
        {
            let Some(index) = render
                .uploaded_kitty_images
                .iter()
                .position(|cached| !visible_image_ids.contains(&cached.image_id))
            else {
                break;
            };
            let cached = render
                .uploaded_kitty_images
                .remove(index)
                .expect("located Kitty cache entry");
            evicted_deletion.push_str(&delete_kitty_image(cached.image_id));
            offscreen_count = offscreen_count.saturating_sub(1);
            offscreen_transmission_bytes =
                offscreen_transmission_bytes.saturating_sub(cached.transmission_bytes);
            offscreen_decoded_bytes =
                offscreen_decoded_bytes.saturating_sub(cached.estimated_decoded_bytes);
        }
        (lines, evicted_deletion)
    }

    fn render_document_for_main_screen(&self, width: usize) -> Vec<String> {
        let Some(base) = self.base() else {
            return Vec::new();
        };
        if let Some(layout_root) = base.layout_root() {
            layout_root.render(width)
        } else {
            base.render_document(width)
        }
    }

    fn dispose_flashes(&self) {
        let tasks = {
            let mut flashes = self.flashes.borrow_mut();
            flashes
                .entries
                .drain(..)
                .map(|entry| entry.task)
                .collect::<Vec<_>>()
        };
        if let Some(base) = self.base() {
            base.cancel_screen_tasks(&tasks);
        }
    }

    fn flash(&self, message: impl Into<String>, duration_ms: Option<u64>) {
        let Some(base) = self.base() else {
            return;
        };
        let id = {
            let mut flashes = self.flashes.borrow_mut();
            let id = flashes.next_id;
            flashes.next_id = flashes.next_id.wrapping_add(1);
            id
        };
        let task = base.plan_screen_task(
            duration_ms.unwrap_or(DEFAULT_FLASH_DURATION_MS),
            TuiHostTask::AltFlashTimeout { flash_id: id },
        );
        self.flashes.borrow_mut().entries.push(FlashEntry {
            id,
            message: message.into(),
            task,
        });
        base.flush_screen_actions();
        self.request_render();
    }

    fn run_flash_task(&self, id: TuiTaskId, flash_id: u64) -> bool {
        let Some(base) = self.base() else {
            return false;
        };
        let task = TuiHostTask::AltFlashTimeout { flash_id };
        let Some(token) = base.claim_screen_task(id, task) else {
            return false;
        };
        let removed = {
            let mut flashes = self.flashes.borrow_mut();
            let Some(index) = flashes
                .entries
                .iter()
                .position(|entry| entry.id == flash_id && entry.task == token)
            else {
                return false;
            };
            flashes.entries.remove(index);
            true
        };
        if removed {
            self.request_render();
        }
        removed
    }

    fn composite_flashes(
        &self,
        mut screen: Vec<String>,
        width: usize,
        height: usize,
    ) -> Vec<String> {
        let flashes = self
            .flashes
            .borrow()
            .entries
            .iter()
            .map(|entry| {
                let message = truncate_to_width(&format!(" {} ", entry.message), width, "", false);
                format!("\x1b[7m{message}\x1b[27m")
            })
            .collect::<Vec<_>>();
        let first = flashes.len().saturating_sub(height);
        let flashes = &flashes[first..];
        if flashes.is_empty() {
            return screen;
        }
        screen.resize(height, String::new());
        for (row, line) in flashes.iter().enumerate() {
            let flash_width = visible_width(line);
            if flash_width == 0 {
                continue;
            }
            screen[row] = composite_tui_line(
                screen.get(row).map_or("", String::as_str),
                line,
                width.saturating_sub(flash_width),
                flash_width,
                width,
            );
        }
        screen
    }

    fn viewport_top(&self) -> usize {
        self.render
            .borrow()
            .implicit_scroll_view
            .as_ref()
            .map_or(0, ScrollView::scroll_top)
    }

    fn is_following_output(&self) -> bool {
        self.render
            .borrow()
            .implicit_scroll_view
            .as_ref()
            .is_some_and(ScrollView::is_following_end)
    }

    fn scroll_by(&self, lines: i64) {
        if let Some(scroll_view) = self.render.borrow_mut().implicit_scroll_view.as_mut() {
            scroll_view.scroll_by(lines);
        }
        self.request_render();
    }

    fn scroll_to_top(&self) {
        if let Some(scroll_view) = self.render.borrow_mut().implicit_scroll_view.as_mut() {
            scroll_view.scroll_to_start();
        }
        self.request_render();
    }

    fn scroll_to_bottom(&self) {
        if let Some(scroll_view) = self.render.borrow_mut().implicit_scroll_view.as_mut() {
            scroll_view.scroll_to_end();
        }
        self.request_render();
    }

    fn page_delta(&self, half: bool) -> usize {
        let viewport_height = self
            .render
            .borrow()
            .implicit_scroll_view
            .as_ref()
            .map_or(0, ScrollView::viewport_height);
        if half {
            (viewport_height / 2).max(1)
        } else {
            viewport_height.saturating_sub(PAGE_SCROLL_OVERLAP).max(1)
        }
    }

    fn handle_input(&self, data: &str) -> Option<TuiInputListenerResult> {
        if data == FOCUS_OUT {
            let mut selection = self.selection.borrow_mut();
            let had_active = selection.press_active;
            selection.press_active = false;
            selection.dragged = false;
            selection.pressed_url = None;
            if had_active {
                selection.anchor = None;
                selection.focus = None;
                selection.granularity = Some(SelectionGranularity::Character);
                selection.initial_range = None;
            }
            selection.last_click = None;
            drop(selection);
            self.request_render();
            return Some(TuiInputListenerResult::consume());
        }
        if data == FOCUS_IN {
            return Some(TuiInputListenerResult::consume());
        }
        if let Some(wheel) = parse_wheel_event(data) {
            self.scroll_by(i64::from(wheel.direction) * self.wheel_scroll_lines as i64);
            return Some(TuiInputListenerResult::consume());
        }
        if let Some(mouse) = parse_sgr_mouse_event(data) {
            if self.handle_right_click_paste(mouse) {
                return Some(TuiInputListenerResult::consume());
            }
            self.handle_selection_mouse_event(mouse);
            return Some(TuiInputListenerResult::consume());
        }
        if is_mouse_sequence(data) {
            return Some(TuiInputListenerResult::consume());
        }

        let keybindings = pie_core::keybindings::global::get_keybindings();
        let release = is_key_release(data);
        if keybindings.matches(data, "tui.altScreen.pageUp") {
            if !release {
                self.scroll_by(-(self.page_delta(false) as i64));
            }
            return Some(TuiInputListenerResult::consume());
        }
        if keybindings.matches(data, "tui.altScreen.pageDown") {
            if !release {
                self.scroll_by(self.page_delta(false) as i64);
            }
            return Some(TuiInputListenerResult::consume());
        }
        if keybindings.matches(data, "tui.altScreen.halfPageUp") {
            if !release {
                self.scroll_by(-(self.page_delta(true) as i64));
            }
            return Some(TuiInputListenerResult::consume());
        }
        if keybindings.matches(data, "tui.altScreen.halfPageDown") {
            if !release {
                self.scroll_by(self.page_delta(true) as i64);
            }
            return Some(TuiInputListenerResult::consume());
        }
        if keybindings.matches(data, "tui.altScreen.top") {
            if !release {
                self.scroll_to_top();
            }
            return Some(TuiInputListenerResult::consume());
        }
        if keybindings.matches(data, "tui.altScreen.bottom") {
            if !release {
                self.scroll_to_bottom();
            }
            return Some(TuiInputListenerResult::consume());
        }
        None
    }

    fn handle_right_click_paste(&self, event: MouseEvent) -> bool {
        let Some(callback) = self.on_right_click_paste.as_ref() else {
            return false;
        };
        if !self.environment.is_windows || event.release || event.button != 2 {
            return false;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()));
        true
    }

    fn handle_selection_mouse_event(&self, event: MouseEvent) {
        if event.button & 3 != 0 {
            return;
        }
        if event.release {
            let active = self.selection.borrow().press_active;
            if !active {
                return;
            }
            self.selection.borrow_mut().press_active = false;
            let anchor_scroll = self
                .selection
                .borrow()
                .anchor
                .and_then(|point| point.scroll_view);
            let point = self.selection_point(event, anchor_scroll);
            self.update_selection_focus(point);
            let (clicked_url, dragged, same_point) = {
                let selection = self.selection.borrow();
                let same_point = selection.anchor.is_some_and(|anchor| {
                    anchor.scroll_view == point.scroll_view
                        && anchor.row == point.row
                        && anchor.col == point.col
                });
                (selection.pressed_url.clone(), selection.dragged, same_point)
            };
            self.selection.borrow_mut().pressed_url = None;
            if !dragged
                && same_point
                && let (Some(url), Some(open_url)) = (clicked_url, self.open_url.as_ref())
            {
                let mut selection = self.selection.borrow_mut();
                selection.anchor = None;
                selection.focus = None;
                drop(selection);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| open_url(&url)));
                self.request_render();
                return;
            }
            self.copy_selection_to_clipboard();
            self.request_render();
            return;
        }
        if event.button & 32 != 0 {
            if !self.selection.borrow().press_active || self.selection.borrow().anchor.is_none() {
                return;
            }
            {
                let mut selection = self.selection.borrow_mut();
                selection.dragged = true;
                selection.last_click = None;
                selection.pressed_url = None;
            }
            let anchor_scroll = self
                .selection
                .borrow()
                .anchor
                .and_then(|point| point.scroll_view);
            let point = self.selection_point(event, anchor_scroll);
            self.update_selection_focus(point);
            self.request_render();
            return;
        }

        let scroll_view = if self.base().is_some_and(|base| !base.has_overlay()) {
            self.render
                .borrow()
                .current_layout
                .as_ref()
                .and_then(|layout| {
                    get_scroll_views_at(layout, event.x as i64, event.y as i64)
                        .into_iter()
                        .next()
                })
        } else {
            None
        };
        let anchor = self.selection_point(event, scroll_view);
        let word = self.word_selection(anchor);
        let click_count = self.click_count(anchor, word);
        let range = match click_count {
            2 => word,
            3 => Some(self.line_selection(anchor)),
            _ => None,
        };
        let mut selection = self.selection.borrow_mut();
        selection.press_active = true;
        selection.granularity = Some(if click_count == 2 {
            SelectionGranularity::Word
        } else if click_count == 3 {
            SelectionGranularity::Line
        } else {
            SelectionGranularity::Character
        });
        selection.initial_range = range;
        selection.anchor = Some(range.map_or(anchor, |range| range.start));
        selection.focus = Some(range.map_or(anchor, |range| range.end));
        selection.dragged = false;
        let (rows, columns) = self.base().map_or((1, 1), |base| {
            (base.terminal_rows().max(1), base.terminal_columns().max(1))
        });
        selection.pressed_url = if range.is_some() {
            None
        } else {
            let render = self.render.borrow();
            render
                .previous_screen
                .get(event.y.min(rows - 1))
                .and_then(|line| get_osc8_link_at_column(line, event.x.min(columns - 1)))
        };
        drop(selection);
        self.request_render();
    }

    fn selection_point(
        &self,
        event: MouseEvent,
        scroll_view: Option<ScrollViewId>,
    ) -> SelectionPoint {
        if let Some(scroll_view) = scroll_view
            && let Some(point) = self.scroll_selection_point(scroll_view, event.x, event.y)
        {
            return point;
        }
        let (rows, columns) = self.base().map_or((1, 1), |base| {
            (base.terminal_rows().max(1), base.terminal_columns().max(1))
        });
        SelectionPoint::new(event.y.min(rows - 1), event.x.min(columns - 1), None)
    }

    fn scroll_selection_point(
        &self,
        scroll_view: ScrollViewId,
        x: usize,
        y: usize,
    ) -> Option<SelectionPoint> {
        let render = self.render.borrow();
        let layout = render.current_layout.as_ref()?;
        let layout_box = get_scroll_view_box(layout, scroll_view)?;
        if layout_box.rect.height == 0 || layout_box.clip.height == 0 {
            return None;
        }
        let terminal_rows = self.base()?.terminal_rows().max(1) as i64;
        let visible_top = layout_box.rect.y.max(layout_box.clip.y).max(0);
        let visible_bottom = (layout_box.rect.y + layout_box.rect.height as i64 - 1)
            .min(layout_box.clip.y + layout_box.clip.height as i64 - 1)
            .min(terminal_rows - 1);
        if visible_bottom < visible_top {
            return None;
        }
        let pointer_row = (y as i64).clamp(visible_top, visible_bottom);
        let max_content_row = layout_box
            .scroll_content_lines
            .as_ref()
            .map_or(0, |lines| lines.len().saturating_sub(1));
        let scroll_top = render
            .implicit_scroll_view
            .as_ref()
            .filter(|view| view.id() == scroll_view)
            .map_or(0, ScrollView::scroll_top);
        let row = scroll_top
            .saturating_add(usize::try_from(pointer_row - layout_box.rect.y).unwrap_or(0))
            .min(max_content_row);
        let col = usize::try_from((x as i64 - layout_box.rect.x).max(0))
            .unwrap_or(usize::MAX)
            .min(layout_box.rect.width.saturating_sub(1));
        Some(SelectionPoint::new(row, col, Some(scroll_view)))
    }

    fn selection_source_line(&self, point: SelectionPoint) -> String {
        let render = self.render.borrow();
        if let (Some(scroll_view), Some(layout)) = (point.scroll_view, &render.current_layout)
            && let Some(lines) = get_scroll_view_box(layout, scroll_view)
                .and_then(|layout_box| layout_box.scroll_content_lines.as_ref())
        {
            return lines.get(point.row).cloned().unwrap_or_default();
        }
        render
            .previous_screen
            .get(point.row)
            .cloned()
            .unwrap_or_default()
    }

    fn word_selection(&self, point: SelectionPoint) -> Option<SelectionRange> {
        let line = strip_terminal_sequences(&self.selection_source_line(point));
        let mut start = 0;
        for segment in default_word_segments(&line) {
            let end = start + visible_width(&segment.text);
            if point.col >= start && point.col < end {
                let mut range_end = SelectionPoint::new(point.row, end, point.scroll_view);
                range_end.boundary = true;
                return Some(SelectionRange {
                    start: SelectionPoint::new(point.row, start, point.scroll_view),
                    end: range_end,
                });
            }
            start = end;
        }
        None
    }

    fn line_selection(&self, point: SelectionPoint) -> SelectionRange {
        let mut end = SelectionPoint::new(
            point.row,
            visible_width(&self.selection_source_line(point)),
            point.scroll_view,
        );
        end.boundary = true;
        SelectionRange {
            start: SelectionPoint::new(point.row, 0, point.scroll_view),
            end,
        }
    }

    fn click_count(&self, point: SelectionPoint, word: Option<SelectionRange>) -> u8 {
        let now = self.base().map_or(0, |base| base.runtime_now_ms());
        let previous = self.selection.borrow().last_click;
        let count = if let (Some(word), Some(previous)) = (word, previous) {
            if now.saturating_sub(previous.timestamp) <= DOUBLE_CLICK_INTERVAL_MS
                && previous.row == point.row
                && previous.scroll_view == point.scroll_view
                && previous.word_start == word.start.col
                && previous.word_end == word.end.col
            {
                previous.count % 3 + 1
            } else {
                1
            }
        } else {
            1
        };
        self.selection.borrow_mut().last_click = word.map(|word| LastClick {
            timestamp: now,
            count,
            row: point.row,
            scroll_view: point.scroll_view,
            word_start: word.start.col,
            word_end: word.end.col,
        });
        count
    }

    fn update_selection_focus(&self, point: SelectionPoint) {
        let (granularity, initial) = {
            let selection = self.selection.borrow();
            (selection.granularity, selection.initial_range)
        };
        if granularity == Some(SelectionGranularity::Character) || initial.is_none() {
            self.selection.borrow_mut().focus = Some(point);
            return;
        }
        let range = if granularity == Some(SelectionGranularity::Word) {
            self.word_selection(point)
        } else {
            Some(self.line_selection(point))
        };
        let (Some(range), Some(initial)) = (range, initial) else {
            return;
        };
        let before = range.start.row < initial.start.row
            || (range.start.row == initial.start.row && range.start.col < initial.start.col);
        let mut selection = self.selection.borrow_mut();
        if before {
            selection.anchor = Some(initial.end);
            selection.focus = Some(range.start);
        } else {
            selection.anchor = Some(initial.start);
            selection.focus = Some(range.end);
        }
    }

    fn selection_bounds(&self) -> Option<SelectionRange> {
        let selection = self.selection.borrow();
        let anchor = selection.anchor?;
        let focus = selection.focus?;
        if anchor.scroll_view != focus.scroll_view
            || (anchor.row == focus.row && anchor.col == focus.col)
        {
            return None;
        }
        let anchor_before =
            anchor.row < focus.row || (anchor.row == focus.row && anchor.col < focus.col);
        Some(if anchor_before {
            SelectionRange {
                start: anchor,
                end: focus,
            }
        } else {
            SelectionRange {
                start: focus,
                end: anchor,
            }
        })
    }

    fn selection_columns(
        line: &str,
        row: usize,
        selection: SelectionRange,
        min_column: usize,
        max_column: usize,
    ) -> (usize, usize) {
        let line_width = visible_width(line);
        let mut start = min_column;
        let mut end = line_width.min(max_column);
        if row == selection.start.row {
            start = get_grapheme_cell_range(line, selection.start.col)
                .map_or(selection.start.col.min(line_width), |range| range.start);
        }
        if row == selection.end.row {
            end = if selection.end.boundary {
                selection.end.col.min(line_width)
            } else {
                get_grapheme_cell_range(line, selection.end.col).map_or(
                    selection.end.col.saturating_add(1).min(line_width),
                    |range| range.end,
                )
            };
        }
        (start.max(min_column), end.min(max_column))
    }

    fn copy_selection_to_clipboard(&self) {
        let Some(selection) = self.selection_bounds() else {
            return;
        };
        let source_lines = if let Some(scroll_view) = selection.start.scroll_view {
            let render = self.render.borrow();
            let Some(lines) = render
                .current_layout
                .as_ref()
                .and_then(|layout| get_scroll_view_box(layout, scroll_view))
                .and_then(|layout_box| layout_box.scroll_content_lines.clone())
            else {
                return;
            };
            lines
        } else {
            self.render.borrow().previous_screen.clone()
        };
        let mut copied = Vec::new();
        for row in selection.start.row..=selection.end.row {
            let line = source_lines.get(row).map_or("", String::as_str);
            let (start, end) =
                Self::selection_columns(line, row, selection, 0, visible_width(line));
            let sliced = slice_by_column(line, start, end.saturating_sub(start), true);
            copied.push(
                strip_terminal_sequences(&sliced)
                    .trim_end_matches(char::is_whitespace)
                    .to_owned(),
            );
        }
        let text = copied.join("\n");
        if text.is_empty() {
            return;
        }
        if let Some(base) = self.base() {
            base.write_terminal(format!("\x1b]52;c;{}\x07", base64_encode(text.as_bytes())));
        }
        self.flash("Copied!", None);
    }

    fn apply_selection(&self, mut screen: Vec<String>, layout: &LayoutFrame) -> Vec<String> {
        let Some(selection) = self.selection_bounds() else {
            return screen;
        };
        let mut screen_selection = selection;
        let mut min_row = 0;
        let mut max_row = screen.len().saturating_sub(1);
        let mut min_column = 0;
        let mut max_column = self.base().map_or(0, |base| base.terminal_columns());
        if let Some(scroll_view) = selection.start.scroll_view {
            let Some(layout_box) = get_scroll_view_box(layout, scroll_view) else {
                return screen;
            };
            min_row = usize::try_from(layout_box.rect.y.max(layout_box.clip.y).max(0))
                .unwrap_or(usize::MAX);
            max_row = usize::try_from(
                (layout_box.rect.y + layout_box.rect.height as i64 - 1)
                    .min(layout_box.clip.y + layout_box.clip.height as i64 - 1),
            )
            .unwrap_or(0)
            .min(screen.len().saturating_sub(1));
            min_column = usize::try_from(layout_box.rect.x.max(layout_box.clip.x).max(0))
                .unwrap_or(usize::MAX);
            max_column = usize::try_from(
                (layout_box.rect.x + layout_box.rect.width as i64)
                    .min(layout_box.clip.x + layout_box.clip.width as i64),
            )
            .unwrap_or(0)
            .min(max_column);
            let scroll_top = self
                .render
                .borrow()
                .implicit_scroll_view
                .as_ref()
                .filter(|view| view.id() == scroll_view)
                .map_or(0, ScrollView::scroll_top);
            let translate = |point: SelectionPoint| SelectionPoint {
                row: usize::try_from(layout_box.rect.y.max(0))
                    .unwrap_or(usize::MAX)
                    .saturating_add(point.row.saturating_sub(scroll_top)),
                col: usize::try_from(layout_box.rect.x.max(0))
                    .unwrap_or(usize::MAX)
                    .saturating_add(point.col),
                ..point
            };
            screen_selection = SelectionRange {
                start: translate(selection.start),
                end: translate(selection.end),
            };
        }
        for (row, line) in screen.iter_mut().enumerate() {
            if row < min_row
                || row > max_row
                || row < screen_selection.start.row
                || row > screen_selection.end.row
                || is_image_line(line)
            {
                continue;
            }
            let line_width = visible_width(line);
            let (start, end) =
                Self::selection_columns(line, row, screen_selection, min_column, max_column);
            if end <= start {
                continue;
            }
            let before = slice_by_column(line, 0, start, true);
            let selected = slice_by_column(line, start, end - start, true);
            let after = slice_by_column(line, end, line_width.saturating_sub(end), true);
            *line = format!("{before}{}{after}", apply_selection_highlight(&selected));
        }
        screen
    }
}

impl ScreenLifecycle for AltScreenState {
    fn render(&self) {
        self.render_screen();
    }

    fn reset_render_state(&self) {
        self.render.borrow_mut().reset();
    }

    fn images_supported(&self) -> bool {
        self.render.borrow().saved_capabilities.is_none()
    }

    fn before_terminal_start(&self) {
        self.dispose_flashes();
        self.selection.borrow_mut().reset();
        let capabilities = get_capabilities();
        {
            let mut render = self.render.borrow_mut();
            render.alt_screen_active = true;
            render.image_protocol = capabilities.images;
            render.uploaded_kitty_images.clear();
            if capabilities.images == Some(ImageProtocol::ITerm2) {
                render.saved_capabilities = Some(capabilities.clone());
                set_capabilities(Arc::new(TerminalCapabilities {
                    images: None,
                    true_color: capabilities.true_color,
                    hyperlinks: capabilities.hyperlinks,
                }));
            }
            render.last_document.clear();
            render.reset();
        }
        if capabilities.images == Some(ImageProtocol::ITerm2)
            && let Some(base) = self.base()
        {
            base.invalidate();
        }
        let mouse = if self.mouse_enabled {
            if self.environment.multiplexer {
                ENABLE_BUTTON_MOTION_MOUSE
            } else {
                ENABLE_ALL_MOTION_MOUSE
            }
        } else {
            ""
        };
        // before_terminal_start is action-driven, so reentrant controller work
        // remains queued until this terminal callback returns.
        self.terminal.borrow_mut().write(&format!(
            "{ENTER_ALT_SCREEN}{DISABLE_AUTOWRAP}{mouse}\x1b[2J\x1b[H\x1b[?25l"
        ));
    }

    fn before_terminal_stop(&self, _options: TuiStopOptions) {
        self.dispose_flashes();
        self.selection.borrow_mut().press_active = false;
        let (active, image_protocol) = {
            let render = self.render.borrow();
            (render.alt_screen_active, render.image_protocol)
        };
        if !active {
            return;
        }
        let images = if image_protocol == Some(ImageProtocol::Kitty) {
            delete_all_kitty_images()
        } else {
            ""
        };
        let mouse = if self.mouse_enabled {
            DISABLE_MOUSE
        } else {
            ""
        };
        if let Some(base) = self.base() {
            base.write_terminal(format!(
                "{BEGIN_SYNCHRONIZED_OUTPUT}{images}{mouse}{ENABLE_AUTOWRAP}{END_SYNCHRONIZED_OUTPUT}"
            ));
        }
        self.render.borrow_mut().uploaded_kitty_images.clear();
    }

    fn after_terminal_stop(&self, options: TuiStopOptions) {
        let active = self.render.borrow().alt_screen_active;
        if !active {
            return;
        }
        self.render.borrow_mut().alt_screen_active = false;
        // after_terminal_stop is also action-driven; capability restoration
        // stays after the final terminal write while callbacks remain queued.
        if options.preserve_screen {
            self.terminal.borrow_mut().write(&format!(
                "{BEGIN_SYNCHRONIZED_OUTPUT}{EXIT_ALT_SCREEN}\x1b[?25h{END_SYNCHRONIZED_OUTPUT}"
            ));
        } else {
            let width = self.terminal.borrow_mut().columns().max(1);
            let mut document = self
                .render_document_for_main_screen(width)
                .into_iter()
                .map(|line| strip_osc133_zone_prefix(&line).replace(CURSOR_MARKER, ""))
                .collect::<Vec<_>>();
            document = apply_line_resets(document)
                .into_iter()
                .map(|line| {
                    if is_image_line(&line) || visible_width(&line) <= width {
                        line
                    } else {
                        slice_by_column(&line, 0, width, true)
                    }
                })
                .collect();
            let mut buffer =
                format!("{BEGIN_SYNCHRONIZED_OUTPUT}{EXIT_ALT_SCREEN}{DISABLE_AUTOWRAP}");
            for (row, line) in document.iter().enumerate() {
                if row > 0 {
                    buffer.push_str("\r\n");
                }
                buffer.push_str("\r\x1b[2K");
                buffer.push_str(line);
            }
            buffer.push_str(&format!(
                "\x1b[0m{ENABLE_AUTOWRAP}\r\n\x1b[?25h{END_SYNCHRONIZED_OUTPUT}"
            ));
            self.terminal.borrow_mut().write(&buffer);
            self.render.borrow_mut().last_document = document;
        }
        if let Some(capabilities) = self.render.borrow_mut().saved_capabilities.take() {
            set_capabilities(capabilities);
        }
    }

    fn controller_dropped(&self) {
        self.dispose_flashes();
        self.base.borrow_mut().take();
    }
}

/// Alternate-screen TUI with an application-owned scrollable viewport.
pub struct TuiAltScreen {
    // See `TuiMainScreen`: lifecycle teardown must run before the explicit
    // state owner is released.
    base: TuiBaseController,
    state: Rc<AltScreenState>,
}

impl TuiAltScreen {
    pub fn new(
        terminal: Box<dyn Terminal>,
        runtime: Box<dyn TuiScreenRuntime>,
        show_hardware_cursor: bool,
        mut options: TuiAltScreenOptions,
    ) -> Self {
        options.wheel_scroll_lines = options.wheel_scroll_lines.max(1);
        let terminal = SharedTerminal::new(terminal);
        let runtime = shared_runtime(runtime);

        // The BaseDocument needs a weak base link, but the base itself needs
        // the screen host. Construct state with a temporary detached document
        // and replace its implicit viewport immediately after base creation.
        let placeholder_base = RefCell::new(None);
        let state = Rc::new(AltScreenState {
            base: placeholder_base,
            terminal: terminal.clone(),
            render: RefCell::new(AltRenderState {
                previous_screen: Vec::new(),
                last_document: Vec::new(),
                previous_width: 0,
                previous_height: 0,
                current_layout: None,
                implicit_scroll_view: None,
                alt_screen_active: false,
                image_protocol: None,
                saved_capabilities: None,
                kitty_image_registry: KittyImageRegistry::default(),
                uploaded_kitty_images: VecDeque::new(),
            }),
            selection: RefCell::new(SelectionState {
                granularity: Some(SelectionGranularity::Character),
                ..SelectionState::default()
            }),
            flashes: RefCell::new(FlashState::default()),
            wheel_scroll_lines: options.wheel_scroll_lines,
            mouse_enabled: options.mouse,
            open_url: options.open_url,
            on_right_click_paste: options.on_right_click_paste,
            environment: options.environment,
        });
        let lifecycle: Rc<dyn ScreenLifecycle> = state.clone();
        let host = ScreenControllerHost::new(runtime, lifecycle);
        let base = TuiBaseController::new(
            Box::new(terminal),
            Box::new(host),
            TuiMode::Fullscreen,
            show_hardware_cursor,
        );
        let weak_base = base.downgrade();
        state.set_base(weak_base.clone());
        state.render.borrow_mut().implicit_scroll_view = Some(ScrollView::new(
            Box::new(BaseDocument { base: weak_base }),
            ScrollViewOptions {
                follow: ScrollViewFollow::End,
                primary: true,
                ..ScrollViewOptions::default()
            },
        ));
        let weak_state = Rc::downgrade(&state);
        base.add_input_listener(TuiInputListener::new(move |data| {
            weak_state
                .upgrade()
                .and_then(|state| state.handle_input(data))
        }));
        Self { base, state }
    }

    pub(crate) fn base(&self) -> &TuiBaseController {
        &self.base
    }

    pub fn viewport_top(&self) -> usize {
        self.state.viewport_top()
    }

    pub fn is_following_output(&self) -> bool {
        self.state.is_following_output()
    }

    pub fn scroll_by(&self, lines: i64) {
        self.state.scroll_by(lines);
    }

    pub fn scroll_to_top(&self) {
        self.state.scroll_to_top();
    }

    pub fn scroll_to_bottom(&self) {
        self.state.scroll_to_bottom();
    }

    pub fn flash(&self, message: impl Into<String>, duration_ms: Option<u64>) {
        self.state.flash(message, duration_ms);
    }

    /// Register metadata produced by an image-rendering ownership path.
    ///
    /// The alternate-screen cache deliberately does not infer ownership from
    /// arbitrary Kitty `i=` control strings. Hosts which render a
    /// [`pie_components::Image`] can pass its ownership metadata here before
    /// the corresponding line is rendered.
    pub fn register_kitty_image_metadata(&self, metadata: KittyImageMetadata) {
        self.state
            .render
            .borrow_mut()
            .kitty_image_registry
            .register(metadata);
    }

    pub fn run_task(&self, id: TuiTaskId, task: TuiHostTask) {
        if let TuiHostTask::AltFlashTimeout { flash_id } = task {
            self.state.run_flash_task(id, flash_id);
            return;
        }
        self.base.run_task(id, task);
    }

    pub fn set_layout_root(&self, component: Option<pie_components::ComponentRef>) {
        self.base.set_layout_root(component);
    }
}

impl Deref for TuiAltScreen {
    type Target = TuiBaseController;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl Drop for TuiAltScreen {
    fn drop(&mut self) {
        if self.state.render.borrow().alt_screen_active {
            self.base.stop(TuiStopOptions {
                preserve_screen: true,
            });
        }
    }
}

delegate_tui!(TuiAltScreen);

impl ViewportTui for TuiAltScreen {
    fn set_layout_root(&self, component: Option<pie_components::ComponentRef>) {
        Self::set_layout_root(self, component);
    }
}

#[derive(Debug, Clone, Copy)]
struct WheelEvent {
    direction: i32,
}

#[derive(Debug, Clone, Copy)]
struct MouseEvent {
    button: u32,
    x: usize,
    y: usize,
    release: bool,
}

fn parse_wheel_event(data: &str) -> Option<WheelEvent> {
    if let Some(event) = parse_sgr_mouse_event(data) {
        if event.button & 64 == 0 {
            return None;
        }
        return match event.button & 3 {
            0 => Some(WheelEvent { direction: -1 }),
            1 => Some(WheelEvent { direction: 1 }),
            _ => None,
        };
    }
    let bytes = data.as_bytes();
    if bytes.len() == 6 && bytes.starts_with(b"\x1b[M") {
        let button = u32::from(bytes[3].saturating_sub(32));
        if button & 64 == 0 {
            return None;
        }
        return match button & 3 {
            0 => Some(WheelEvent { direction: -1 }),
            1 => Some(WheelEvent { direction: 1 }),
            _ => None,
        };
    }
    None
}

fn parse_sgr_mouse_event(data: &str) -> Option<MouseEvent> {
    let body = data.strip_prefix("\x1b[<")?;
    let release = body.ends_with('m');
    if !release && !body.ends_with('M') {
        return None;
    }
    let body = &body[..body.len() - 1];
    let mut parts = body.split(';');
    let button = parts.next()?.parse::<u32>().ok()?;
    let x = parts.next()?.parse::<usize>().ok()?.saturating_sub(1);
    let y = parts.next()?.parse::<usize>().ok()?.saturating_sub(1);
    if parts.next().is_some() {
        return None;
    }
    Some(MouseEvent {
        button,
        x,
        y,
        release,
    })
}

fn is_mouse_sequence(data: &str) -> bool {
    parse_sgr_mouse_event(data).is_some()
        || (data.len() == 6 && data.as_bytes().starts_with(b"\x1b[M"))
}

fn strip_osc133_zone_prefix(line: &str) -> String {
    let mut rest = line;
    while let Some(body) = rest.strip_prefix("\x1b]133;") {
        let Some(kind) = body.as_bytes().first() else {
            break;
        };
        if !matches!(kind, b'A' | b'B' | b'C') {
            break;
        }
        let after_kind = &body[1..];
        if let Some(after) = after_kind.strip_prefix('\x07') {
            rest = after;
        } else if let Some(after) = after_kind.strip_prefix("\x1b\\") {
            rest = after;
        } else {
            break;
        }
    }
    rest.to_owned()
}

fn apply_selection_highlight(text: &str) -> String {
    let mut result = String::from("\x1b[7m");
    let mut index = 0;
    while index < text.len() {
        if let Some(length) = extract_ansi_code_len(text, index) {
            let code = &text[index..index + length];
            result.push_str(code);
            if code.ends_with('m') {
                result.push_str("\x1b[7m");
            }
            index += length;
        } else {
            let ch = text[index..].chars().next().expect("valid UTF-8 boundary");
            result.push(ch);
            index += ch.len_utf8();
        }
    }
    result.push_str("\x1b[27m");
    result
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}
