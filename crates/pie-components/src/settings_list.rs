//! SettingsList — searchable settings navigation, value cycling, and submenus.
//!
//! Port of the pinned `components/settings-list.js` implementation from
//! `@earendil-works/pi-tui@0.84.1`. The embedded search editor is deliberately
//! private: M3 exposes SettingsList while the full public Input port belongs to
//! the later editor milestone.

use std::sync::{Arc, Mutex};

use pie_core::fuzzy::fuzzy_filter;
use pie_core::keybindings::global::get_keybindings;
use pie_core::keys::decode_printable_key;
use pie_core::text::visible_width;
use pie_core::wrap::{
    is_punctuation_char, is_whitespace_char, slice_by_column, truncate_to_width,
    wrap_text_with_ansi,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Component, StyleFn};

pub type SelectedStyleFn = Box<dyn Fn(&str, bool) -> String + Send>;
pub type SubmenuDone = Box<dyn FnMut(Option<String>) + Send>;
pub type SubmenuFactory = Box<dyn FnMut(&str, SubmenuDone) -> Box<dyn Component + Send> + Send>;

/// One setting row (reference `SettingItem`).
pub struct SettingItem {
    pub id: String,
    pub label: String,
    pub current_value: String,
    pub values: Option<Vec<String>>,
    pub description: Option<String>,
    pub submenu: Option<SubmenuFactory>,
}

impl SettingItem {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        current_value: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            current_value: current_value.into(),
            values: None,
            description: None,
            submenu: None,
        }
    }

    pub fn with_values(mut self, values: Vec<String>) -> Self {
        self.values = Some(values);
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_submenu(mut self, submenu: SubmenuFactory) -> Self {
        self.submenu = Some(submenu);
        self
    }
}

/// Styling callbacks used by [`SettingsList`].
pub struct SettingsListTheme {
    pub label: SelectedStyleFn,
    pub value: SelectedStyleFn,
    pub description: StyleFn,
    pub cursor: String,
    pub hint: StyleFn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsListOptions {
    pub enable_search: bool,
}

type ChangeCallback = Box<dyn FnMut(&str, &str) + Send>;

/// Scrollable settings component with optional fuzzy search.
pub struct SettingsList {
    items: Vec<SettingItem>,
    filtered_indices: Vec<usize>,
    theme: SettingsListTheme,
    selected_index: usize,
    max_visible: usize,
    on_change: ChangeCallback,
    on_cancel: Box<dyn FnMut() + Send>,
    search_input: Option<SearchInput>,
    search_enabled: bool,
    submenu_component: Option<Box<dyn Component + Send>>,
    submenu_item_index: Option<usize>,
    submenu_done: Option<Arc<Mutex<Option<Option<String>>>>>,
}

impl SettingsList {
    pub fn new(
        items: Vec<SettingItem>,
        max_visible: usize,
        theme: SettingsListTheme,
        on_change: ChangeCallback,
        on_cancel: Box<dyn FnMut() + Send>,
    ) -> Self {
        Self::with_options(
            items,
            max_visible,
            theme,
            on_change,
            on_cancel,
            SettingsListOptions::default(),
        )
    }

    pub fn with_options(
        items: Vec<SettingItem>,
        max_visible: usize,
        theme: SettingsListTheme,
        on_change: ChangeCallback,
        on_cancel: Box<dyn FnMut() + Send>,
        options: SettingsListOptions,
    ) -> Self {
        let filtered_indices = (0..items.len()).collect();
        Self {
            items,
            filtered_indices,
            theme,
            selected_index: 0,
            max_visible,
            on_change,
            on_cancel,
            search_input: options.enable_search.then(SearchInput::default),
            search_enabled: options.enable_search,
            submenu_component: None,
            submenu_item_index: None,
            submenu_done: None,
        }
    }

    pub fn update_value(&mut self, id: &str, new_value: impl Into<String>) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.current_value = new_value.into();
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        let index = if self.search_enabled {
            *self.filtered_indices.get(self.selected_index)?
        } else {
            self.selected_index
        };
        self.items.get(index)
    }

    fn display_indices(&self) -> Vec<usize> {
        if self.search_enabled {
            self.filtered_indices.clone()
        } else {
            (0..self.items.len()).collect()
        }
    }

    fn add_hint_line(&self, lines: &mut Vec<String>, width: usize) {
        lines.push(String::new());
        let text = if self.search_enabled {
            "  Type to search · Enter/Space to change · Esc to cancel"
        } else {
            "  Enter/Space to change · Esc to cancel"
        };
        lines.push(truncate_to_width(
            &(self.theme.hint)(text),
            width,
            "...",
            false,
        ));
    }

    fn render_main_list(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(search) = self.search_input.as_mut() {
            lines.extend(search.render(width));
            lines.push(String::new());
        }
        if self.items.is_empty() {
            lines.push((self.theme.hint)("  No settings available"));
            if self.search_enabled {
                self.add_hint_line(&mut lines, width);
            }
            return lines;
        }
        let display_indices = self.display_indices();
        if display_indices.is_empty() {
            lines.push(truncate_to_width(
                &(self.theme.hint)("  No matching settings"),
                width,
                "...",
                false,
            ));
            self.add_hint_line(&mut lines, width);
            return lines;
        }
        let max_start = display_indices.len().saturating_sub(self.max_visible);
        let start = self
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(max_start);
        let end = (start + self.max_visible).min(display_indices.len());
        let max_label_width = self
            .items
            .iter()
            .map(|item| visible_width(&item.label))
            .max()
            .unwrap_or(0)
            .min(30);
        for (position, item_index) in display_indices.iter().enumerate().take(end).skip(start) {
            let item = &self.items[*item_index];
            let is_selected = position == self.selected_index;
            let prefix = if is_selected {
                self.theme.cursor.as_str()
            } else {
                "  "
            };
            let prefix_width = visible_width(prefix);
            let label_padding = max_label_width.saturating_sub(visible_width(&item.label));
            let label = format!("{}{}", item.label, " ".repeat(label_padding));
            let label = (self.theme.label)(&label, is_selected);
            let used_width = prefix_width + max_label_width + 2;
            let value_max_width = width.saturating_sub(used_width + 2);
            let value = truncate_to_width(&item.current_value, value_max_width, "", false);
            let value = (self.theme.value)(&value, is_selected);
            lines.push(truncate_to_width(
                &format!("{prefix}{label}  {value}"),
                width,
                "...",
                false,
            ));
        }
        if start > 0 || end < display_indices.len() {
            let scroll = format!("  ({}/{})", self.selected_index + 1, display_indices.len());
            lines.push((self.theme.hint)(&truncate_to_width(
                &scroll,
                width.saturating_sub(2),
                "",
                false,
            )));
        }
        if let Some(description) = display_indices
            .get(self.selected_index)
            .and_then(|index| self.items.get(*index))
            .and_then(|item| item.description.as_deref())
        {
            lines.push(String::new());
            for line in wrap_text_with_ansi(description, width.saturating_sub(4)) {
                lines.push((self.theme.description)(&format!("  {line}")));
            }
        }
        self.add_hint_line(&mut lines, width);
        lines
    }

    fn apply_filter(&mut self, query: &str) {
        let filtered = fuzzy_filter(&self.items, query, |item| item.label.clone());
        self.filtered_indices = filtered
            .into_iter()
            .filter_map(|matched| {
                self.items
                    .iter()
                    .position(|item| std::ptr::eq(item, matched))
            })
            .collect();
        self.selected_index = 0;
    }

    fn activate_item(&mut self) {
        let display_index = self.selected_index;
        let item_index = if self.search_enabled {
            match self.filtered_indices.get(display_index) {
                Some(index) => *index,
                None => return,
            }
        } else if display_index < self.items.len() {
            display_index
        } else {
            return;
        };
        if self.items[item_index].submenu.is_some() {
            let current_value = self.items[item_index].current_value.clone();
            let mut factory = self.items[item_index]
                .submenu
                .take()
                .expect("submenu checked above");
            let result = Arc::new(Mutex::new(None));
            let result_for_callback = result.clone();
            let done: SubmenuDone = Box::new(move |selected| {
                *result_for_callback.lock().expect("submenu result lock") = Some(selected);
            });
            let component = factory(&current_value, done);
            self.items[item_index].submenu = Some(factory);
            self.submenu_item_index = Some(display_index);
            self.submenu_done = Some(result);
            self.submenu_component = Some(component);
        } else if let Some(values) = &self.items[item_index].values
            && !values.is_empty()
        {
            let current = values
                .iter()
                .position(|value| value == &self.items[item_index].current_value)
                .map(|index| index as isize)
                .unwrap_or(-1);
            let next = ((current + 1) as usize) % values.len();
            let value = values[next].clone();
            self.items[item_index].current_value.clone_from(&value);
            let id = self.items[item_index].id.clone();
            (self.on_change)(&id, &value);
        }
    }

    fn finish_submenu_if_done(&mut self) {
        let completed = self
            .submenu_done
            .as_ref()
            .and_then(|slot| slot.lock().expect("submenu result lock").take());
        let Some(selected_value) = completed else {
            return;
        };
        if let Some(value) = selected_value
            && let Some(display_index) = self.submenu_item_index
        {
            let item_index = if self.search_enabled {
                self.filtered_indices.get(display_index).copied()
            } else {
                Some(display_index)
            };
            if let Some(item_index) = item_index {
                self.items[item_index].current_value.clone_from(&value);
                let id = self.items[item_index].id.clone();
                (self.on_change)(&id, &value);
            }
        }
        self.submenu_component = None;
        self.submenu_done = None;
        if let Some(index) = self.submenu_item_index.take() {
            self.selected_index = index;
        }
    }
}

impl Component for SettingsList {
    fn invalidate(&mut self) {
        self.finish_submenu_if_done();
        if let Some(submenu) = self.submenu_component.as_mut() {
            submenu.invalidate();
        }
        self.finish_submenu_if_done();
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.finish_submenu_if_done();
        if let Some(submenu) = self.submenu_component.as_mut() {
            let lines = submenu.render(width);
            self.finish_submenu_if_done();
            lines
        } else {
            self.render_main_list(width)
        }
    }

    fn handle_input(&mut self, data: &str) {
        if let Some(submenu) = self.submenu_component.as_mut() {
            submenu.handle_input(data);
            self.finish_submenu_if_done();
            return;
        }
        let display_len = if self.search_enabled {
            self.filtered_indices.len()
        } else {
            self.items.len()
        };
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            if display_len == 0 {
                return;
            }
            self.selected_index = if self.selected_index == 0 {
                display_len - 1
            } else {
                self.selected_index - 1
            };
        } else if keybindings.matches(data, "tui.select.down") {
            if display_len == 0 {
                return;
            }
            self.selected_index = if self.selected_index == display_len - 1 {
                0
            } else {
                self.selected_index + 1
            };
        } else if keybindings.matches(data, "tui.select.confirm")
            || (data == " "
                && (!self.search_enabled
                    || self
                        .search_input
                        .as_ref()
                        .is_none_or(|input| input.value.is_empty())))
        {
            self.activate_item();
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else if let Some(input) = self.search_input.as_mut() {
            input.handle_input(data);
            let query = input.value.clone();
            self.apply_filter(&query);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchAction {
    TypeWord,
    Kill,
    Yank,
}

#[derive(Debug, Clone)]
struct SearchSnapshot {
    value: String,
    cursor: usize,
}

#[derive(Default)]
struct SearchInput {
    value: String,
    cursor: usize,
    paste_buffer: String,
    is_in_paste: bool,
    kill_ring: Vec<String>,
    last_action: Option<SearchAction>,
    undo_stack: Vec<SearchSnapshot>,
}

impl SearchInput {
    fn handle_input(&mut self, data: &str) {
        const PASTE_START: &str = "\x1b[200~";
        const PASTE_END: &str = "\x1b[201~";

        let paste_chunk = data.contains(PASTE_START).then(|| {
            self.is_in_paste = true;
            self.paste_buffer.clear();
            data.replacen(PASTE_START, "", 1)
        });
        let data = paste_chunk.as_deref().unwrap_or(data);
        if self.is_in_paste {
            self.paste_buffer.push_str(data);
            if let Some(end) = self.paste_buffer.find(PASTE_END) {
                let pasted = self.paste_buffer[..end].to_string();
                let remaining = self.paste_buffer[end + PASTE_END.len()..].to_string();
                self.handle_paste(&pasted);
                self.is_in_paste = false;
                self.paste_buffer.clear();
                if !remaining.is_empty() {
                    self.handle_input(&remaining);
                }
            }
            return;
        }

        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.editor.undo") {
            self.undo();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteCharBackward") {
            self.handle_backspace();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteCharForward") {
            self.handle_forward_delete();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteWordBackward") {
            self.delete_word_backward();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteWordForward") {
            self.delete_word_forward();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteToLineStart") {
            self.delete_to_line_start();
            return;
        }
        if keybindings.matches(data, "tui.editor.deleteToLineEnd") {
            self.delete_to_line_end();
            return;
        }
        if keybindings.matches(data, "tui.editor.yank") {
            self.yank();
            return;
        }
        if keybindings.matches(data, "tui.editor.yankPop") {
            self.yank_pop();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLeft") {
            self.last_action = None;
            self.cursor = previous_grapheme_start(&self.value, self.cursor);
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorRight") {
            self.last_action = None;
            self.cursor = next_grapheme_end(&self.value, self.cursor);
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLineStart") {
            self.last_action = None;
            self.cursor = 0;
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorLineEnd") {
            self.last_action = None;
            self.cursor = self.value.len();
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorWordLeft") {
            self.last_action = None;
            self.cursor = find_word_backward(&self.value, self.cursor);
            return;
        }
        if keybindings.matches(data, "tui.editor.cursorWordRight") {
            self.last_action = None;
            self.cursor = find_word_forward(&self.value, self.cursor);
            return;
        }

        let printable = decode_printable_key(data).unwrap_or_else(|| data.to_string());
        if printable.chars().any(is_input_control) {
            return;
        }
        self.insert_text(&printable);
    }

    fn insert_text(&mut self, text: &str) {
        if text.chars().any(is_whitespace_char) || self.last_action != Some(SearchAction::TypeWord)
        {
            self.push_undo();
        }
        self.last_action = Some(SearchAction::TypeWord);
        self.value.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    fn handle_backspace(&mut self) {
        self.last_action = None;
        if self.cursor == 0 {
            return;
        }
        self.push_undo();
        let previous = previous_grapheme_start(&self.value, self.cursor);
        self.value.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    fn handle_forward_delete(&mut self) {
        self.last_action = None;
        if self.cursor >= self.value.len() {
            return;
        }
        self.push_undo();
        let next = next_grapheme_end(&self.value, self.cursor);
        self.value.replace_range(self.cursor..next, "");
    }

    fn delete_to_line_start(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.push_undo();
        let deleted = self.value[..self.cursor].to_string();
        self.push_kill(deleted, true, self.last_action == Some(SearchAction::Kill));
        self.value.replace_range(..self.cursor, "");
        self.cursor = 0;
        self.last_action = Some(SearchAction::Kill);
    }

    fn delete_to_line_end(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.push_undo();
        let deleted = self.value[self.cursor..].to_string();
        self.push_kill(deleted, false, self.last_action == Some(SearchAction::Kill));
        self.value.truncate(self.cursor);
        self.last_action = Some(SearchAction::Kill);
    }

    fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let accumulate = self.last_action == Some(SearchAction::Kill);
        self.push_undo();
        let from = find_word_backward(&self.value, self.cursor);
        let deleted = self.value[from..self.cursor].to_string();
        self.push_kill(deleted, true, accumulate);
        self.value.replace_range(from..self.cursor, "");
        self.cursor = from;
        self.last_action = Some(SearchAction::Kill);
    }

    fn delete_word_forward(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let accumulate = self.last_action == Some(SearchAction::Kill);
        self.push_undo();
        let to = find_word_forward(&self.value, self.cursor);
        let deleted = self.value[self.cursor..to].to_string();
        self.push_kill(deleted, false, accumulate);
        self.value.replace_range(self.cursor..to, "");
        self.last_action = Some(SearchAction::Kill);
    }

    fn push_kill(&mut self, text: String, prepend: bool, accumulate: bool) {
        if text.is_empty() {
            return;
        }
        if accumulate && let Some(previous) = self.kill_ring.last_mut() {
            if prepend {
                previous.insert_str(0, &text);
            } else {
                previous.push_str(&text);
            }
        } else {
            self.kill_ring.push(text);
        }
    }

    fn yank(&mut self) {
        let Some(text) = self.kill_ring.last().cloned() else {
            return;
        };
        self.push_undo();
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.last_action = Some(SearchAction::Yank);
    }

    fn yank_pop(&mut self) {
        if self.last_action != Some(SearchAction::Yank) || self.kill_ring.len() <= 1 {
            return;
        }
        self.push_undo();
        let previous = self.kill_ring.last().cloned().unwrap_or_default();
        let from = self.cursor.saturating_sub(previous.len());
        self.value.replace_range(from..self.cursor, "");
        self.cursor = from;
        let latest = self.kill_ring.pop().expect("kill ring length checked");
        self.kill_ring.insert(0, latest);
        let text = self.kill_ring.last().cloned().unwrap_or_default();
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.last_action = Some(SearchAction::Yank);
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(SearchSnapshot {
            value: self.value.clone(),
            cursor: self.cursor,
        });
    }

    fn undo(&mut self) {
        let Some(snapshot) = self.undo_stack.pop() else {
            return;
        };
        self.value = snapshot.value;
        self.cursor = snapshot.cursor;
        self.last_action = None;
    }

    fn handle_paste(&mut self, pasted: &str) {
        self.last_action = None;
        self.push_undo();
        let clean = pasted
            .replace("\r\n", "")
            .replace(['\r', '\n'], "")
            .replace('\t', "    ");
        self.value.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
    }

    fn render(&self, width: usize) -> Vec<String> {
        let prompt = "> ";
        let available = width.saturating_sub(prompt.len());
        if available == 0 {
            return vec![prompt.to_string()];
        }

        let total_width = visible_width(&self.value);
        let (visible, cursor_display) = if total_width < available {
            (self.value.clone(), self.cursor)
        } else {
            let scroll_width = if self.cursor == self.value.len() {
                available.saturating_sub(1)
            } else {
                available
            };
            if scroll_width == 0 {
                (String::new(), 0)
            } else {
                let cursor_column = visible_width(&self.value[..self.cursor]);
                let half = scroll_width / 2;
                let start_column = if cursor_column < half {
                    0
                } else if cursor_column > total_width.saturating_sub(half) {
                    total_width.saturating_sub(scroll_width)
                } else {
                    cursor_column.saturating_sub(half)
                };
                let visible = slice_by_column(&self.value, start_column, scroll_width, true);
                let before_cursor = slice_by_column(
                    &self.value,
                    start_column,
                    cursor_column.saturating_sub(start_column),
                    true,
                );
                let cursor_display = before_cursor.len().min(visible.len());
                (visible, cursor_display)
            }
        };
        let before = &visible[..cursor_display];
        let after = &visible[cursor_display..];
        let at_cursor = after.graphemes(true).next().unwrap_or(" ");
        let after_cursor = &after[at_cursor.len().min(after.len())..];
        let text = format!("{before}\x1b[7m{at_cursor}\x1b[27m{after_cursor}");
        let padding = " ".repeat(available.saturating_sub(visible_width(&text)));
        vec![format!("{prompt}{text}{padding}")]
    }
}

fn is_input_control(ch: char) -> bool {
    let code = u32::from(ch);
    code < 32 || code == 0x7f || (0x80..=0x9f).contains(&code)
}

fn previous_grapheme_start(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .grapheme_indices(true)
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_grapheme_end(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .graphemes(true)
        .next()
        .map(|grapheme| cursor + grapheme.len())
        .unwrap_or(value.len())
}

fn is_word_segment(segment: &str) -> bool {
    segment.unicode_words().next().is_some()
}

fn is_whitespace_segment(segment: &str) -> bool {
    segment.chars().any(is_whitespace_char)
}

// The embedded M3 editor follows UAX #29 word boundaries plus the reference's
// explicit ASCII punctuation stops. `Intl.Segmenter` dictionary boundaries for
// some CJK phrases remain host-ICU dependent and belong to the full Input port.
fn find_word_backward(value: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut segments = value[..cursor]
        .split_word_bound_indices()
        .collect::<Vec<_>>();
    let mut position = cursor;
    while segments
        .last()
        .is_some_and(|(_, segment)| is_whitespace_segment(segment))
    {
        position -= segments.pop().expect("segment checked").1.len();
    }
    let Some((_, last)) = segments.last().copied() else {
        return position;
    };
    if is_word_segment(last) {
        if let Some((index, punctuation)) = last
            .char_indices()
            .rfind(|(_, ch)| is_punctuation_char(*ch))
        {
            position -= last.len() - index - punctuation.len_utf8();
        } else {
            position -= last.len();
        }
    } else {
        while segments.last().is_some_and(|(_, segment)| {
            !is_word_segment(segment) && !is_whitespace_segment(segment)
        }) {
            position -= segments.pop().expect("segment checked").1.len();
        }
    }
    position
}

fn find_word_forward(value: &str, cursor: usize) -> usize {
    if cursor >= value.len() {
        return value.len();
    }
    let mut segments = value[cursor..].split_word_bounds().peekable();
    let mut position = cursor;
    while segments
        .peek()
        .is_some_and(|segment| is_whitespace_segment(segment))
    {
        position += segments.next().expect("segment checked").len();
    }
    let Some(first) = segments.next() else {
        return position;
    };
    if is_word_segment(first) {
        position += first
            .char_indices()
            .find(|(_, ch)| is_punctuation_char(*ch))
            .map_or(first.len(), |(index, _)| index);
    } else {
        position += first.len();
        while segments
            .peek()
            .is_some_and(|segment| !is_word_segment(segment) && !is_whitespace_segment(segment))
        {
            position += segments.next().expect("segment checked").len();
        }
    }
    position
}
