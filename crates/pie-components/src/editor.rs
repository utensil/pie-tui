//! Canonical multi-line Editor component over [`pie_core::editor_model`].
//!
//! Runtime work is inverted through [`EditorHost`]. The component schedules
//! deterministic task facts and hands provider futures to the host; app and
//! adapter layers decide how those facts enter their event loop.

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pie_core::editor_model::{
    EditorAction, EditorCursor, EditorEffect, EditorModel, EditorModelSnapshot, EditorWordSegmenter,
};
use pie_core::fuzzy::js_is_whitespace;
use pie_core::keybindings::global::{SharedKeybindings, get_keybindings};
use pie_core::keys::{decode_printable_key, matches_key};
use pie_core::screen::CURSOR_MARKER;
use pie_core::text::visible_width;
use pie_core::wrap::slice_by_column;
use unicode_segmentation::UnicodeSegmentation;

use crate::Component;
use crate::autocomplete::{
    AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
};
use crate::select_list::{SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme};

const AUTOCOMPLETE_DEBOUNCE_MS: u64 = 20;
const DEFAULT_TRIGGER_CHARACTERS: &[&str] = &["@", "#"];

pub type SharedStyleFn = Arc<dyn Fn(&str) -> String + Send + Sync>;
pub type EditorAutocompleteFuture =
    Pin<Box<dyn Future<Output = Option<AutocompleteSuggestions>> + Send + 'static>>;
pub type EditorTextCallback = Box<dyn FnMut(String) + Send>;

#[derive(Clone)]
pub struct EditorSelectListTheme {
    pub selected_prefix: SharedStyleFn,
    pub selected_text: SharedStyleFn,
    pub description: SharedStyleFn,
    pub scroll_info: SharedStyleFn,
    pub no_match: SharedStyleFn,
}

impl EditorSelectListTheme {
    pub fn plain() -> Self {
        let plain: SharedStyleFn = Arc::new(str::to_owned);
        Self {
            selected_prefix: Arc::clone(&plain),
            selected_text: Arc::clone(&plain),
            description: Arc::clone(&plain),
            scroll_info: Arc::clone(&plain),
            no_match: plain,
        }
    }

    fn instantiate(&self) -> SelectListTheme {
        let selected_prefix = Arc::clone(&self.selected_prefix);
        let selected_text = Arc::clone(&self.selected_text);
        let description = Arc::clone(&self.description);
        let scroll_info = Arc::clone(&self.scroll_info);
        let no_match = Arc::clone(&self.no_match);
        SelectListTheme {
            selected_prefix: Box::new(move |text| selected_prefix(text)),
            selected_text: Box::new(move |text| selected_text(text)),
            description: Box::new(move |text| description(text)),
            scroll_info: Box::new(move |text| scroll_info(text)),
            no_match: Box::new(move |text| no_match(text)),
        }
    }
}

/// Styling callbacks used by [`Editor`].
#[derive(Clone)]
pub struct EditorTheme {
    pub border_color: SharedStyleFn,
    pub select_list: EditorSelectListTheme,
}

impl EditorTheme {
    pub fn plain() -> Self {
        Self {
            border_color: Arc::new(str::to_owned),
            select_list: EditorSelectListTheme::plain(),
        }
    }
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self::plain()
    }
}

#[derive(Default)]
pub struct EditorOptions {
    pub padding_x: usize,
    pub autocomplete_max_visible: Option<usize>,
    /// Host `Intl.Segmenter` adapter seam. `None` retains the documented
    /// bounded Rust fallback rather than claiming the residual Thai domain.
    pub word_segmenter: Option<EditorWordSegmenter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorTaskId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorHostTask {
    StartAutocomplete {
        token: u64,
        force: bool,
        explicit_tab: bool,
    },
}

/// Lower, object-safe runtime seam for terminal facts and deterministic work.
pub trait EditorHost: Send {
    fn terminal_rows(&self) -> usize;
    fn request_render(&mut self, force: bool);
    fn schedule_task(&mut self, delay_ms: u64, task: EditorHostTask) -> EditorTaskId;
    fn cancel_task(&mut self, task: EditorTaskId);
    fn spawn_autocomplete(&mut self, request_id: u64, future: EditorAutocompleteFuture);
    fn discard_autocomplete(&mut self, request_id: u64);
}

/// Inert host useful for pure rendering and integrations that do not enable
/// autocomplete. Runtime adapters replace it with their event-loop host.
pub struct DetachedEditorHost {
    rows: usize,
    next_task: u64,
}

impl DetachedEditorHost {
    pub fn new(rows: usize) -> Self {
        Self { rows, next_task: 0 }
    }
}

impl Default for DetachedEditorHost {
    fn default() -> Self {
        Self::new(24)
    }
}

impl EditorHost for DetachedEditorHost {
    fn terminal_rows(&self) -> usize {
        self.rows
    }

    fn request_render(&mut self, _force: bool) {}

    fn schedule_task(&mut self, _delay_ms: u64, _task: EditorHostTask) -> EditorTaskId {
        self.next_task += 1;
        EditorTaskId(self.next_task)
    }

    fn cancel_task(&mut self, _task: EditorTaskId) {}

    fn spawn_autocomplete(&mut self, _request_id: u64, _future: EditorAutocompleteFuture) {}

    fn discard_autocomplete(&mut self, _request_id: u64) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutocompleteKind {
    Regular,
    Force,
}

#[derive(Clone)]
struct AutocompleteMenu {
    items: Vec<AutocompleteItem>,
    prefix: String,
    selected: usize,
    kind: AutocompleteKind,
}

struct ActiveAutocomplete {
    request_id: u64,
    controller: crate::CancellationController,
    text: String,
    cursor: EditorCursor,
    force: bool,
    explicit_tab: bool,
    provider_generation: u64,
}

#[derive(Debug, Clone, Copy)]
struct QueuedAutocomplete {
    token: u64,
    force: bool,
    explicit_tab: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JumpMode {
    Forward,
    Backward,
}

/// Multi-line canonical editor component.
pub struct Editor {
    model: EditorModel,
    host: Box<dyn EditorHost>,
    theme: EditorTheme,
    padding_x: usize,
    autocomplete_max_visible: usize,
    pub focused: bool,
    pub disable_submit: bool,
    last_width: usize,
    scroll_offset: usize,
    paste_buffer: String,
    is_in_paste: bool,
    jump_mode: Option<JumpMode>,
    on_submit: Option<EditorTextCallback>,
    on_change: Option<EditorTextCallback>,
    autocomplete_provider: Option<Arc<dyn AutocompleteProvider>>,
    autocomplete_trigger_characters: Vec<String>,
    autocomplete_menu: Option<AutocompleteMenu>,
    scheduled_start: Option<(EditorTaskId, u64)>,
    active_autocomplete: Option<ActiveAutocomplete>,
    queued_autocomplete: Option<QueuedAutocomplete>,
    autocomplete_start_token: u64,
    autocomplete_request_id: u64,
    provider_generation: u64,
}

impl Editor {
    pub fn new(host: Box<dyn EditorHost>, theme: EditorTheme, options: EditorOptions) -> Self {
        let mut model = EditorModel::new();
        model.set_word_segmenter(options.word_segmenter);
        Self {
            model,
            host,
            theme,
            padding_x: options.padding_x,
            autocomplete_max_visible: options.autocomplete_max_visible.unwrap_or(5).clamp(3, 20),
            focused: false,
            disable_submit: false,
            last_width: 80,
            scroll_offset: 0,
            paste_buffer: String::new(),
            is_in_paste: false,
            jump_mode: None,
            on_submit: None,
            on_change: None,
            autocomplete_provider: None,
            autocomplete_trigger_characters: DEFAULT_TRIGGER_CHARACTERS
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            autocomplete_menu: None,
            scheduled_start: None,
            active_autocomplete: None,
            queued_autocomplete: None,
            autocomplete_start_token: 0,
            autocomplete_request_id: 0,
            provider_generation: 0,
        }
    }

    pub fn detached(theme: EditorTheme, options: EditorOptions) -> Self {
        Self::new(Box::new(DetachedEditorHost::default()), theme, options)
    }

    pub fn get_text(&self) -> String {
        self.model.text()
    }

    pub fn get_expanded_text(&self) -> String {
        self.model.expanded_text()
    }

    pub fn get_lines(&self) -> Vec<String> {
        self.model.snapshot().lines
    }

    pub fn get_cursor(&self) -> EditorCursor {
        self.model.cursor()
    }

    pub fn get_padding_x(&self) -> usize {
        self.padding_x
    }

    pub fn set_padding_x(&mut self, padding: usize) {
        if self.padding_x != padding {
            self.padding_x = padding;
            self.host.request_render(false);
        }
    }

    pub fn set_border_color(&mut self, border_color: SharedStyleFn) {
        self.theme.border_color = border_color;
        self.host.request_render(false);
    }

    pub fn get_autocomplete_max_visible(&self) -> usize {
        self.autocomplete_max_visible
    }

    pub fn set_autocomplete_max_visible(&mut self, max_visible: usize) {
        let max_visible = max_visible.clamp(3, 20);
        if self.autocomplete_max_visible != max_visible {
            self.autocomplete_max_visible = max_visible;
            self.host.request_render(false);
        }
    }

    pub fn set_on_submit(&mut self, callback: Option<EditorTextCallback>) {
        self.on_submit = callback;
    }

    pub fn set_on_change(&mut self, callback: Option<EditorTextCallback>) {
        self.on_change = callback;
    }

    pub fn set_word_segmenter(&mut self, segmenter: Option<EditorWordSegmenter>) {
        self.model.set_word_segmenter(segmenter);
    }

    pub fn set_autocomplete_provider(&mut self, provider: Arc<dyn AutocompleteProvider>) {
        self.cancel_autocomplete();
        self.provider_generation += 1;
        self.autocomplete_trigger_characters = DEFAULT_TRIGGER_CHARACTERS
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        if let Some(characters) = provider.trigger_characters() {
            for character in characters {
                if character.encode_utf16().count() == 1
                    && character != "/"
                    && !character.chars().any(js_is_whitespace)
                    && !self.autocomplete_trigger_characters.contains(character)
                {
                    self.autocomplete_trigger_characters.push(character.clone());
                }
            }
        }
        self.autocomplete_provider = Some(provider);
    }

    pub fn clear_autocomplete_provider(&mut self) {
        self.cancel_autocomplete();
        self.provider_generation += 1;
        self.autocomplete_provider = None;
    }

    pub fn is_showing_autocomplete(&self) -> bool {
        self.autocomplete_menu.is_some()
    }

    pub fn add_to_history(&mut self, text: impl Into<String>) {
        self.model.apply(EditorAction::AddHistory(text.into()));
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.cancel_autocomplete();
        let effects = self.model.apply(EditorAction::SetText(text.into()));
        self.scroll_offset = 0;
        self.emit_effects(effects);
    }

    pub fn insert_text_at_cursor(&mut self, text: impl Into<String>) {
        self.cancel_autocomplete();
        let effects = self.model.apply(EditorAction::InsertText(text.into()));
        self.emit_effects(effects);
    }

    fn emit_effects(&mut self, effects: Vec<EditorEffect>) {
        for effect in effects {
            match effect {
                EditorEffect::Change(text) => {
                    if let Some(mut callback) = self.on_change.take() {
                        callback(text);
                        self.on_change = Some(callback);
                    }
                }
                EditorEffect::Submit(text) => {
                    if let Some(mut callback) = self.on_submit.take() {
                        callback(text);
                        self.on_submit = Some(callback);
                    }
                }
            }
        }
    }

    fn apply_action(&mut self, action: EditorAction) {
        let effects = self.model.apply(action);
        self.emit_effects(effects);
    }

    fn submit(&mut self) {
        self.cancel_autocomplete();
        self.scroll_offset = 0;
        self.apply_action(EditorAction::Submit);
    }

    fn model_snapshot_for_view(&mut self, layout_width: usize) -> EditorModelSnapshot {
        let rows = self.host.terminal_rows();
        self.model.apply(EditorAction::SetView {
            width: layout_width,
            rows,
        });
        self.model.snapshot()
    }

    pub fn render(&mut self, width: usize) -> Vec<String> {
        let max_padding = width.saturating_sub(1) / 2;
        let padding_x = self.padding_x.min(max_padding);
        let content_width = width.saturating_sub(padding_x * 2).max(1);
        let layout_width = if padding_x == 0 {
            content_width.saturating_sub(1).max(1)
        } else {
            content_width
        };
        self.last_width = layout_width;
        let snapshot = self.model_snapshot_for_view(layout_width);
        let mut layout = Vec::with_capacity(snapshot.visual_lines.len());
        for (index, visual) in snapshot.visual_lines.iter().enumerate() {
            let line = snapshot
                .lines
                .get(visual.logical_line)
                .map(String::as_str)
                .unwrap_or("");
            let text = utf16_slice(line, visual.start_col, visual.start_col + visual.length);
            let last_for_logical = snapshot
                .visual_lines
                .get(index + 1)
                .is_none_or(|next| next.logical_line != visual.logical_line);
            let has_cursor = snapshot.cursor.line == visual.logical_line
                && snapshot.cursor.col >= visual.start_col
                && (snapshot.cursor.col < visual.start_col + visual.length
                    || (last_for_logical
                        && snapshot.cursor.col == visual.start_col + visual.length));
            layout.push((
                text.to_owned(),
                has_cursor.then(|| snapshot.cursor.col - visual.start_col),
            ));
        }
        if layout.is_empty() {
            layout.push((String::new(), Some(0)));
        }

        let rows = self.host.terminal_rows();
        let max_visible = 5.max(rows.saturating_mul(3) / 10);
        let cursor_line = layout
            .iter()
            .position(|(_, cursor)| cursor.is_some())
            .unwrap_or(0);
        if cursor_line < self.scroll_offset {
            self.scroll_offset = cursor_line;
        } else if cursor_line >= self.scroll_offset + max_visible {
            self.scroll_offset = cursor_line - max_visible + 1;
        }
        self.scroll_offset = self
            .scroll_offset
            .min(layout.len().saturating_sub(max_visible));
        let end = (self.scroll_offset + max_visible).min(layout.len());
        let visible = &layout[self.scroll_offset..end];

        let horizontal = (self.theme.border_color)("─");
        let mut result = Vec::new();
        if self.scroll_offset > 0 {
            result.push((self.theme.border_color)(&create_scroll_border(
                "↑",
                self.scroll_offset,
                width,
            )));
        } else {
            result.push(horizontal.repeat(width));
        }
        let left_padding = " ".repeat(padding_x);
        for (text, cursor) in visible {
            let mut display = text.clone();
            let mut line_width = visible_width(text);
            let mut cursor_in_padding = false;
            if let Some(cursor) = cursor {
                let before = utf16_prefix(&display, *cursor).to_owned();
                let after = utf16_suffix(&display, *cursor).to_owned();
                let marker = if self.focused { CURSOR_MARKER } else { "" };
                if after.is_empty() {
                    display = format!("{before}{marker}\x1b[7m \x1b[0m");
                    line_width += 1;
                    cursor_in_padding = line_width > content_width && padding_x > 0;
                } else {
                    let grapheme = first_editor_segment(&after, &snapshot);
                    let rest = &after[grapheme.len()..];
                    display = format!("{before}{marker}\x1b[7m{grapheme}\x1b[0m{rest}");
                }
            }
            let padding = " ".repeat(content_width.saturating_sub(line_width));
            let right_padding =
                " ".repeat(padding_x.saturating_sub(usize::from(cursor_in_padding)));
            result.push(format!("{left_padding}{display}{padding}{right_padding}"));
        }
        let below = layout.len().saturating_sub(end);
        if below > 0 {
            result.push((self.theme.border_color)(&create_scroll_border(
                "↓", below, width,
            )));
        } else {
            result.push(horizontal.repeat(width));
        }

        if let Some(menu) = self.autocomplete_menu.clone() {
            let items = menu
                .items
                .iter()
                .map(|item| SelectItem {
                    value: item.value.clone(),
                    label: item.label.clone(),
                    description: item.description.clone(),
                })
                .collect();
            let mut list = if menu.prefix.starts_with('/') {
                SelectList::with_layout(
                    items,
                    self.autocomplete_max_visible,
                    self.theme.select_list.instantiate(),
                    SelectListLayoutOptions {
                        min_primary_column_width: Some(12),
                        max_primary_column_width: Some(32),
                        truncate_primary: None,
                    },
                )
            } else {
                SelectList::new(
                    items,
                    self.autocomplete_max_visible,
                    self.theme.select_list.instantiate(),
                )
            };
            list.set_selected_index(menu.selected as isize);
            for line in Component::render(&mut list, content_width) {
                let padding = " ".repeat(content_width.saturating_sub(visible_width(&line)));
                result.push(format!(
                    "{left_padding}{line}{padding}{}",
                    " ".repeat(padding_x)
                ));
            }
        }
        result
    }

    fn is_first_visual_line(&mut self) -> bool {
        let snapshot = self.model_snapshot_for_view(self.last_width);
        current_visual_line(&snapshot) == 0
    }

    fn is_last_visual_line(&mut self) -> bool {
        let snapshot = self.model_snapshot_for_view(self.last_width);
        current_visual_line(&snapshot) + 1 == snapshot.visual_lines.len()
    }

    fn handle_paste(&mut self, text: &str) {
        self.cancel_autocomplete();
        self.apply_action(EditorAction::Paste(text.to_owned()));
    }

    fn handle_menu_input(&mut self, data: &str) -> MenuInputResult {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            self.cancel_autocomplete();
            return MenuInputResult::Consumed;
        }
        if keybindings.matches(data, "tui.select.up") {
            if let Some(menu) = self.autocomplete_menu.as_mut()
                && !menu.items.is_empty()
            {
                menu.selected = if menu.selected == 0 {
                    menu.items.len() - 1
                } else {
                    menu.selected - 1
                };
            }
            return MenuInputResult::Consumed;
        }
        if keybindings.matches(data, "tui.select.down") {
            if let Some(menu) = self.autocomplete_menu.as_mut()
                && !menu.items.is_empty()
            {
                menu.selected = (menu.selected + 1) % menu.items.len();
            }
            return MenuInputResult::Consumed;
        }
        let tab = keybindings.matches(data, "tui.input.tab");
        let confirm = keybindings.matches(data, "tui.select.confirm");
        if tab || confirm {
            let slash = self.apply_selected_completion(confirm && !tab);
            if tab || !slash {
                return MenuInputResult::Consumed;
            }
            return MenuInputResult::SubmitAfterApply;
        }
        MenuInputResult::Continue
    }

    pub fn handle_input(&mut self, data: &str) {
        if let Some(mode) = self.jump_mode.take() {
            let keys = get_keybindings();
            if keys.matches(data, "tui.editor.jumpForward")
                || keys.matches(data, "tui.editor.jumpBackward")
            {
                return;
            }
            if let Some(printable) = decode_printable_key(data).or_else(|| {
                data.chars()
                    .next()
                    .filter(|character| (*character as u32) >= 32)
                    .map(|_| data.to_owned())
            }) {
                self.apply_action(match mode {
                    JumpMode::Forward => EditorAction::JumpForward(printable),
                    JumpMode::Backward => EditorAction::JumpBackward(printable),
                });
                return;
            }
        }

        let input = if data.contains("\x1b[200~") {
            self.is_in_paste = true;
            self.paste_buffer.clear();
            Cow::Owned(data.replacen("\x1b[200~", "", 1))
        } else {
            Cow::Borrowed(data)
        };
        let data = input.as_ref();
        if self.is_in_paste {
            self.paste_buffer.push_str(data);
            if let Some(end) = self.paste_buffer.find("\x1b[201~") {
                let paste = self.paste_buffer[..end].to_owned();
                let remaining = self.paste_buffer[end + 6..].to_owned();
                if !paste.is_empty() {
                    self.handle_paste(&paste);
                }
                self.is_in_paste = false;
                self.paste_buffer.clear();
                if !remaining.is_empty() {
                    self.handle_input(&remaining);
                }
            }
            return;
        }

        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.input.copy") {
            return;
        }
        if keybindings.matches(data, "tui.editor.undo") {
            self.apply_action(EditorAction::Undo);
            return;
        }
        if self.autocomplete_menu.is_some() {
            match self.handle_menu_input(data) {
                MenuInputResult::Consumed => return,
                MenuInputResult::SubmitAfterApply => {
                    if !self.disable_submit {
                        self.submit();
                    }
                    return;
                }
                MenuInputResult::Continue => {}
            }
        }
        if keybindings.matches(data, "tui.input.tab") {
            self.handle_tab_completion();
            return;
        }

        let action = if keybindings.matches(data, "tui.editor.deleteToLineEnd") {
            Some(EditorAction::DeleteLineEnd)
        } else if keybindings.matches(data, "tui.editor.deleteToLineStart") {
            Some(EditorAction::DeleteLineStart)
        } else if keybindings.matches(data, "tui.editor.deleteWordBackward") {
            Some(EditorAction::DeleteWordBackward)
        } else if keybindings.matches(data, "tui.editor.deleteWordForward") {
            Some(EditorAction::DeleteWordForward)
        } else if keybindings.matches(data, "tui.editor.deleteCharBackward")
            || matches_key(data, "shift+backspace")
        {
            Some(EditorAction::Backspace)
        } else if keybindings.matches(data, "tui.editor.deleteCharForward")
            || matches_key(data, "shift+delete")
        {
            Some(EditorAction::DeleteForward)
        } else if keybindings.matches(data, "tui.editor.yank") {
            Some(EditorAction::Yank)
        } else if keybindings.matches(data, "tui.editor.yankPop") {
            Some(EditorAction::YankPop)
        } else if keybindings.matches(data, "tui.editor.historyPrevious") {
            Some(EditorAction::HistoryPrevious)
        } else if keybindings.matches(data, "tui.editor.historyNext") {
            Some(EditorAction::HistoryNext)
        } else if keybindings.matches(data, "tui.editor.cursorLineStart") {
            Some(EditorAction::LineStart)
        } else if keybindings.matches(data, "tui.editor.cursorLineEnd") {
            Some(EditorAction::LineEnd)
        } else if keybindings.matches(data, "tui.editor.cursorWordLeft") {
            Some(EditorAction::MoveWordBackward)
        } else if keybindings.matches(data, "tui.editor.cursorWordRight") {
            Some(EditorAction::MoveWordForward)
        } else {
            None
        };
        if let Some(action) = action {
            let history = matches!(
                &action,
                EditorAction::HistoryPrevious | EditorAction::HistoryNext
            );
            let deletion = matches!(
                &action,
                EditorAction::Backspace | EditorAction::DeleteForward
            );
            if history {
                self.cancel_autocomplete();
            }
            self.apply_action(action);
            if deletion {
                self.refresh_autocomplete_after_deletion();
            }
            return;
        }

        let is_new_line = keybindings.matches(data, "tui.input.newLine")
            || (data.starts_with('\n') && data.len() > 1)
            || data == "\x1b\r"
            || data == "\x1b[13;2~"
            || (data.len() > 1 && data.contains('\x1b') && data.contains('\r'))
            || data == "\n";
        if is_new_line {
            if self.should_submit_on_backslash_enter(data, &keybindings) {
                self.apply_action(EditorAction::Backspace);
                self.refresh_autocomplete_after_deletion();
                self.submit();
                return;
            }
            self.cancel_autocomplete();
            self.apply_action(EditorAction::NewLine);
            return;
        }
        if keybindings.matches(data, "tui.input.submit") {
            if self.disable_submit {
                return;
            }
            let snapshot = self.model.snapshot();
            let current = snapshot
                .lines
                .get(snapshot.cursor.line)
                .map(String::as_str)
                .unwrap_or("");
            if utf16_prefix(current, snapshot.cursor.col).ends_with('\\') {
                self.apply_action(EditorAction::Backspace);
                self.refresh_autocomplete_after_deletion();
                self.cancel_autocomplete();
                self.apply_action(EditorAction::NewLine);
            } else {
                self.submit();
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorUp") {
            let snapshot = self.model.snapshot();
            if self.is_first_visual_line()
                && (snapshot.text.is_empty()
                    || snapshot.history_index > -1
                    || snapshot.cursor.col == 0)
            {
                self.apply_action(EditorAction::HistoryPrevious);
            } else if self.is_first_visual_line() {
                self.apply_action(EditorAction::LineStart);
            } else {
                self.apply_action(EditorAction::MoveUp);
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorDown") {
            let history = self.model.snapshot().history_index;
            if history > -1 && self.is_last_visual_line() {
                self.apply_action(EditorAction::HistoryNext);
            } else if self.is_last_visual_line() {
                self.apply_action(EditorAction::LineEnd);
            } else {
                self.apply_action(EditorAction::MoveDown);
            }
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorRight") {
            self.apply_action(EditorAction::MoveRight);
            self.refresh_open_autocomplete();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLeft") {
            self.apply_action(EditorAction::MoveLeft);
            self.refresh_open_autocomplete();
            return;
        }
        if keybindings.matches(data, "tui.editor.pageUp") {
            self.apply_action(EditorAction::PageUp);
            return;
        }
        if keybindings.matches(data, "tui.editor.pageDown") {
            self.apply_action(EditorAction::PageDown);
            return;
        }
        if keybindings.matches(data, "tui.editor.jumpForward") {
            self.jump_mode = Some(JumpMode::Forward);
            return;
        }
        if keybindings.matches(data, "tui.editor.jumpBackward") {
            self.jump_mode = Some(JumpMode::Backward);
            return;
        }
        if matches_key(data, "shift+space") {
            self.type_text(" ");
            return;
        }
        if let Some(printable) = decode_printable_key(data) {
            self.type_text(&printable);
        } else if data
            .chars()
            .next()
            .is_some_and(|character| (character as u32) >= 32)
        {
            self.type_text(data);
        }
    }

    fn type_text(&mut self, text: &str) {
        self.apply_action(EditorAction::Type(text.to_owned()));
        self.trigger_autocomplete_after_type(text);
    }

    fn trigger_autocomplete_after_type(&mut self, text: &str) {
        if self.autocomplete_provider.is_none() {
            return;
        }
        if self.autocomplete_menu.is_some() {
            self.request_autocomplete(
                self.autocomplete_menu
                    .as_ref()
                    .is_some_and(|menu| menu.kind == AutocompleteKind::Force),
                false,
            );
            return;
        }
        let snapshot = self.model.snapshot();
        let current = snapshot
            .lines
            .get(snapshot.cursor.line)
            .map(String::as_str)
            .unwrap_or("");
        let before = utf16_prefix(current, snapshot.cursor.col);
        if text == "/" && snapshot.cursor.line == 0 && before.trim_matches(js_is_whitespace) == "/"
        {
            self.request_autocomplete(false, false);
            return;
        }
        if self
            .autocomplete_trigger_characters
            .iter()
            .any(|item| item == text)
        {
            let prefix_units = utf16_len(before).saturating_sub(utf16_len(text));
            let preceding = utf16_prefix(before, prefix_units).chars().next_back();
            if preceding.is_none_or(|character| character == ' ' || character == '\t') {
                self.request_autocomplete(false, false);
            }
            return;
        }
        if text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
            && (self.in_slash_context(before) || self.in_trigger_context(before))
        {
            self.request_autocomplete(false, false);
        }
    }

    fn refresh_open_autocomplete(&mut self) {
        if let Some(menu) = self.autocomplete_menu.as_ref() {
            self.request_autocomplete(menu.kind == AutocompleteKind::Force, false);
        }
    }

    fn refresh_autocomplete_after_deletion(&mut self) {
        if self.autocomplete_menu.is_some() {
            self.refresh_open_autocomplete();
            return;
        }
        let snapshot = self.model.snapshot();
        let current = snapshot
            .lines
            .get(snapshot.cursor.line)
            .map(String::as_str)
            .unwrap_or("");
        let before = utf16_prefix(current, snapshot.cursor.col);
        if self.in_slash_context(before) || self.in_trigger_context(before) {
            self.request_autocomplete(false, false);
        }
    }

    fn in_slash_context(&self, before: &str) -> bool {
        self.model.cursor().line == 0
            && before.trim_start_matches(js_is_whitespace).starts_with('/')
    }

    fn in_trigger_context(&self, before: &str) -> bool {
        let token = before.rsplit(js_is_whitespace).next().unwrap_or(before);
        self.autocomplete_trigger_characters
            .iter()
            .any(|trigger| token.starts_with(trigger))
    }

    fn should_submit_on_backslash_enter(
        &self,
        data: &str,
        keybindings: &SharedKeybindings,
    ) -> bool {
        if self.disable_submit || !matches_key(data, "enter") {
            return false;
        }
        let submit_keys = keybindings.get_keys("tui.input.submit");
        if !submit_keys
            .iter()
            .any(|key| key == "shift+enter" || key == "shift+return")
        {
            return false;
        }
        let snapshot = self.model.snapshot();
        let current = snapshot
            .lines
            .get(snapshot.cursor.line)
            .map(String::as_str)
            .unwrap_or("");
        snapshot.cursor.col > 0 && utf16_prefix(current, snapshot.cursor.col).ends_with('\\')
    }

    fn handle_tab_completion(&mut self) {
        if self.autocomplete_provider.is_none() {
            return;
        }
        let snapshot = self.model.snapshot();
        let current = snapshot
            .lines
            .get(snapshot.cursor.line)
            .map(String::as_str)
            .unwrap_or("");
        let before = utf16_prefix(current, snapshot.cursor.col);
        if self.in_slash_context(before)
            && !before.trim_start_matches(js_is_whitespace).contains(' ')
        {
            self.request_autocomplete(false, true);
        } else {
            self.request_autocomplete(true, true);
        }
    }

    fn request_autocomplete(&mut self, force: bool, explicit_tab: bool) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        if force {
            let snapshot = self.model.snapshot();
            if !provider.should_trigger_file_completion(
                &snapshot.lines,
                snapshot.cursor.line,
                snapshot.cursor.col,
            ) {
                return;
            }
        }
        self.cancel_autocomplete_request();
        self.autocomplete_start_token += 1;
        let token = self.autocomplete_start_token;
        let delay = if explicit_tab || force {
            0
        } else if self.should_debounce() {
            AUTOCOMPLETE_DEBOUNCE_MS
        } else {
            0
        };
        let task = EditorHostTask::StartAutocomplete {
            token,
            force,
            explicit_tab,
        };
        let task_id = self.host.schedule_task(delay, task);
        self.scheduled_start = Some((task_id, token));
    }

    fn should_debounce(&self) -> bool {
        let snapshot = self.model.snapshot();
        let current = snapshot
            .lines
            .get(snapshot.cursor.line)
            .map(String::as_str)
            .unwrap_or("");
        let before = utf16_prefix(current, snapshot.cursor.col);
        let token = before
            .rsplit_once([' ', '\t'])
            .map_or(before, |(_, token)| token);
        if token.starts_with('@') {
            return true;
        }
        self.autocomplete_trigger_characters
            .iter()
            .filter(|trigger| trigger.as_str() != "@")
            .any(|trigger| token.starts_with(trigger))
    }

    /// Consume a task previously emitted through [`EditorHost::schedule_task`].
    pub fn handle_host_task(&mut self, task: EditorHostTask) {
        let EditorHostTask::StartAutocomplete {
            token,
            force,
            explicit_tab,
        } = task;
        if token != self.autocomplete_start_token {
            return;
        }
        if self
            .scheduled_start
            .is_some_and(|(_, scheduled_token)| scheduled_token == token)
        {
            self.scheduled_start = None;
        }
        let queued = QueuedAutocomplete {
            token,
            force,
            explicit_tab,
        };
        if self.active_autocomplete.is_some() {
            self.queued_autocomplete = Some(queued);
        } else {
            self.launch_autocomplete(queued);
        }
    }

    fn launch_autocomplete(&mut self, start: QueuedAutocomplete) {
        if start.token != self.autocomplete_start_token {
            return;
        }
        let Some(provider) = self.autocomplete_provider.as_ref().map(Arc::clone) else {
            return;
        };
        let snapshot = self.model.snapshot();
        let controller = crate::CancellationController::new();
        let options = AutocompleteOptions {
            force: start.force,
            signal: controller.signal(),
        };
        let lines = snapshot.lines.clone();
        let cursor = snapshot.cursor;
        self.autocomplete_request_id += 1;
        let request_id = self.autocomplete_request_id;
        let future: EditorAutocompleteFuture = Box::pin(async move {
            provider
                .get_suggestions(&lines, cursor.line, cursor.col, options)
                .await
        });
        self.host.spawn_autocomplete(request_id, future);
        self.active_autocomplete = Some(ActiveAutocomplete {
            request_id,
            controller,
            text: snapshot.text,
            cursor,
            force: start.force,
            explicit_tab: start.explicit_tab,
            provider_generation: self.provider_generation,
        });
    }

    /// Deliver a provider future result from the host event loop.
    pub fn complete_autocomplete(
        &mut self,
        request_id: u64,
        suggestions: Option<AutocompleteSuggestions>,
    ) {
        let Some(active) = self.active_autocomplete.take() else {
            return;
        };
        if active.request_id != request_id {
            self.active_autocomplete = Some(active);
            return;
        }
        let snapshot = self.model.snapshot();
        let current = !active.controller.signal().aborted()
            && active.provider_generation == self.provider_generation
            && snapshot.text == active.text
            && snapshot.cursor == active.cursor;
        if current {
            match suggestions.filter(|value| !value.items.is_empty()) {
                None => {
                    self.clear_autocomplete_ui();
                    self.host.request_render(false);
                }
                Some(suggestions)
                    if active.force && active.explicit_tab && suggestions.items.len() == 1 =>
                {
                    self.apply_completion(&suggestions.items[0], &suggestions.prefix, true);
                    self.host.request_render(false);
                }
                Some(suggestions) => {
                    let selected = best_autocomplete_match(&suggestions.items, &suggestions.prefix)
                        .unwrap_or(0);
                    self.autocomplete_menu = Some(AutocompleteMenu {
                        items: suggestions.items,
                        prefix: suggestions.prefix,
                        selected,
                        kind: if active.force {
                            AutocompleteKind::Force
                        } else {
                            AutocompleteKind::Regular
                        },
                    });
                    self.host.request_render(false);
                }
            }
        }
        if let Some(queued) = self.queued_autocomplete.take()
            && queued.token == self.autocomplete_start_token
        {
            self.launch_autocomplete(queued);
        }
    }

    fn apply_selected_completion(&mut self, suppress_slash_change: bool) -> bool {
        let Some(menu) = self.autocomplete_menu.clone() else {
            return false;
        };
        let Some(item) = menu.items.get(menu.selected) else {
            return false;
        };
        let slash = menu.prefix.starts_with('/');
        self.apply_completion(item, &menu.prefix, !(slash && suppress_slash_change));
        self.cancel_autocomplete();
        slash
    }

    fn apply_completion(&mut self, item: &AutocompleteItem, prefix: &str, emit_change: bool) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        let snapshot = self.model.snapshot();
        let result = provider.apply_completion(
            &snapshot.lines,
            snapshot.cursor.line,
            snapshot.cursor.col,
            item,
            prefix,
        );
        let effects = self.model.apply(EditorAction::ApplyCompletion {
            lines: result.lines,
            cursor: EditorCursor {
                line: result.cursor_line,
                col: result.cursor_col,
            },
        });
        if emit_change {
            self.emit_effects(effects);
        }
    }

    fn cancel_autocomplete_request(&mut self) {
        self.autocomplete_start_token += 1;
        if let Some((task, _)) = self.scheduled_start.take() {
            self.host.cancel_task(task);
        }
        if let Some(active) = self.active_autocomplete.as_ref() {
            active.controller.cancel();
        }
        self.queued_autocomplete = None;
    }

    fn clear_autocomplete_ui(&mut self) {
        self.autocomplete_menu = None;
    }

    fn cancel_autocomplete(&mut self) {
        self.cancel_autocomplete_request();
        self.clear_autocomplete_ui();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Some((task, _)) = self.scheduled_start.take() {
            self.host.cancel_task(task);
        }
        if let Some(active) = self.active_autocomplete.take() {
            active.controller.cancel();
            self.host.discard_autocomplete(active.request_id);
        }
        self.queued_autocomplete = None;
    }
}

impl Component for Editor {
    fn render(&mut self, width: usize) -> Vec<String> {
        Editor::render(self, width)
    }

    fn handle_input(&mut self, data: &str) {
        Editor::handle_input(self, data);
    }

    fn focused(&self) -> Option<bool> {
        Some(self.focused)
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.focused = focused;
        true
    }
}

enum MenuInputResult {
    Consumed,
    SubmitAfterApply,
    Continue,
}

fn best_autocomplete_match(items: &[AutocompleteItem], prefix: &str) -> Option<usize> {
    if prefix.is_empty() {
        return None;
    }
    let mut first_prefix = None;
    for (index, item) in items.iter().enumerate() {
        if item.value == prefix {
            return Some(index);
        }
        if first_prefix.is_none() && item.value.starts_with(prefix) {
            first_prefix = Some(index);
        }
    }
    first_prefix
}

fn current_visual_line(snapshot: &EditorModelSnapshot) -> usize {
    snapshot
        .visual_lines
        .iter()
        .enumerate()
        .find_map(|(index, line)| {
            let last = snapshot
                .visual_lines
                .get(index + 1)
                .is_none_or(|next| next.logical_line != line.logical_line);
            (snapshot.cursor.line == line.logical_line
                && snapshot.cursor.col >= line.start_col
                && (snapshot.cursor.col < line.start_col + line.length
                    || (last && snapshot.cursor.col == line.start_col + line.length)))
                .then_some(index)
        })
        .unwrap_or_else(|| snapshot.visual_lines.len().saturating_sub(1))
}

fn create_scroll_border(direction: &str, hidden: usize, width: usize) -> String {
    let indicator = format!("─── {direction} {hidden} more ");
    let remaining = width as isize - visible_width(&indicator) as isize;
    if remaining >= 0 {
        return format!("{indicator}{}", "─".repeat(remaining as usize));
    }
    let ellipsis = &"..."[..width.min(3)];
    let indicator_width = width.saturating_sub(visible_width(ellipsis));
    format!(
        "{}{}",
        slice_by_column(&indicator, 0, indicator_width, true),
        ellipsis
    )
}

fn first_editor_segment<'a>(suffix: &'a str, snapshot: &EditorModelSnapshot) -> &'a str {
    if suffix.starts_with("[paste #")
        && let Some(end) = suffix.find(']')
    {
        let marker = &suffix[..=end];
        if snapshot
            .pastes
            .iter()
            .any(|(id, _)| marker.starts_with(&format!("[paste #{id}")))
        {
            return marker;
        }
    }
    suffix.graphemes(true).next().unwrap_or("")
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut consumed = 0;
    for (byte, character) in text.char_indices() {
        if consumed >= units || consumed + character.len_utf16() > units {
            return byte;
        }
        consumed += character.len_utf16();
    }
    text.len()
}

fn utf16_prefix(text: &str, end: usize) -> &str {
    &text[..utf16_to_byte(text, end)]
}

fn utf16_suffix(text: &str, start: usize) -> &str {
    &text[utf16_to_byte(text, start)..]
}

fn utf16_slice(text: &str, start: usize, end: usize) -> &str {
    &text[utf16_to_byte(text, start)..utf16_to_byte(text, end)]
}
