//! Canonical single-line Input component over the pure key/text primitives.

use std::borrow::Cow;

use pie_core::editor_model::EditorWordSegmenter;
use pie_core::fuzzy::js_is_whitespace;
use pie_core::keybindings::global::get_keybindings;
use pie_core::keys::decode_kitty_printable;
use pie_core::kill_ring::{KillRing, PushOptions};
use pie_core::screen::CURSOR_MARKER;
use pie_core::text::visible_width;
use pie_core::undo_stack::UndoStack;
use pie_core::word_navigation::{WordNavOptions, find_word_backward, find_word_forward};
use pie_core::wrap::slice_by_column;
use unicode_segmentation::UnicodeSegmentation;

use crate::Component;

pub type InputSubmitCallback = Box<dyn FnMut(String) + Send>;
pub type InputEscapeCallback = Box<dyn FnMut() + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputUndoSnapshot {
    value: String,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputLastAction {
    TypeWord,
    Kill,
    Yank,
}

/// Single-line input with UTF-16 cursor coordinates and horizontal scrolling.
pub struct Input {
    value: String,
    cursor: usize,
    pub focused: bool,
    paste_buffer: String,
    is_in_paste: bool,
    kill_ring: KillRing,
    last_action: Option<InputLastAction>,
    undo_stack: UndoStack<InputUndoSnapshot>,
    on_submit: Option<InputSubmitCallback>,
    on_escape: Option<InputEscapeCallback>,
    word_segmenter: Option<EditorWordSegmenter>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            focused: false,
            paste_buffer: String::new(),
            is_in_paste: false,
            kill_ring: KillRing::new(),
            last_action: None,
            undo_stack: UndoStack::new(),
            on_submit: None,
            on_escape: None,
            word_segmenter: None,
        }
    }
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.cursor.min(utf16_len(&self.value));
    }

    /// Observable JavaScript-compatible UTF-16 cursor offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_on_submit(&mut self, callback: Option<InputSubmitCallback>) {
        self.on_submit = callback;
    }

    pub fn set_on_escape(&mut self, callback: Option<InputEscapeCallback>) {
        self.on_escape = callback;
    }

    /// Inject host `Intl.Segmenter` partitions for exact word navigation.
    /// The default remains the documented bounded pure-Rust fallback.
    pub fn set_word_segmenter(&mut self, segmenter: Option<EditorWordSegmenter>) {
        self.word_segmenter = segmenter;
    }

    fn call_submit(&mut self) {
        let value = self.value.clone();
        if let Some(mut callback) = self.on_submit.take() {
            callback(value);
            self.on_submit = Some(callback);
        }
    }

    fn call_escape(&mut self) {
        if let Some(mut callback) = self.on_escape.take() {
            callback();
            self.on_escape = Some(callback);
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(&InputUndoSnapshot {
            value: self.value.clone(),
            cursor: self.cursor,
        });
    }

    fn insert_character(&mut self, text: &str) {
        if text.chars().any(js_is_whitespace) || self.last_action != Some(InputLastAction::TypeWord)
        {
            self.push_undo();
        }
        self.last_action = Some(InputLastAction::TypeWord);
        let byte = utf16_to_byte(&self.value, self.cursor);
        self.value.insert_str(byte, text);
        self.cursor += utf16_len(text);
    }

    fn backspace(&mut self) {
        self.last_action = None;
        if self.cursor == 0 {
            return;
        }
        self.push_undo();
        let before = utf16_prefix(&self.value, self.cursor);
        let grapheme = before.graphemes(true).next_back().unwrap_or("");
        let start = self.cursor.saturating_sub(utf16_len(grapheme).max(1));
        self.value = format!(
            "{}{}",
            utf16_prefix(&self.value, start),
            utf16_suffix(&self.value, self.cursor)
        );
        self.cursor = start;
    }

    fn delete_forward(&mut self) {
        self.last_action = None;
        if self.cursor >= utf16_len(&self.value) {
            return;
        }
        self.push_undo();
        let after = utf16_suffix(&self.value, self.cursor);
        let grapheme = after.graphemes(true).next().unwrap_or("");
        let end = self.cursor + utf16_len(grapheme).max(1);
        self.value = format!(
            "{}{}",
            utf16_prefix(&self.value, self.cursor),
            utf16_suffix(&self.value, end)
        );
    }

    fn delete_line_start(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.push_undo();
        let deleted = utf16_prefix(&self.value, self.cursor).to_owned();
        self.kill_ring.push(
            &deleted,
            PushOptions {
                prepend: true,
                accumulate: self.last_action == Some(InputLastAction::Kill),
            },
        );
        self.last_action = Some(InputLastAction::Kill);
        self.value = utf16_suffix(&self.value, self.cursor).to_owned();
        self.cursor = 0;
    }

    fn delete_line_end(&mut self) {
        if self.cursor >= utf16_len(&self.value) {
            return;
        }
        self.push_undo();
        let deleted = utf16_suffix(&self.value, self.cursor).to_owned();
        self.kill_ring.push(
            &deleted,
            PushOptions {
                prepend: false,
                accumulate: self.last_action == Some(InputLastAction::Kill),
            },
        );
        self.last_action = Some(InputLastAction::Kill);
        self.value = utf16_prefix(&self.value, self.cursor).to_owned();
    }

    fn move_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.last_action = None;
        let segment = |text: &str| {
            self.word_segmenter.as_ref().map_or_else(
                || pie_core::word_navigation::default_word_segments(text),
                |host| host.segment(text),
            )
        };
        self.cursor = find_word_backward(
            &self.value,
            self.cursor,
            &WordNavOptions {
                segment: Some(&segment),
                is_atomic_segment: None,
            },
        );
    }

    fn move_word_forward(&mut self) {
        if self.cursor >= utf16_len(&self.value) {
            return;
        }
        self.last_action = None;
        let segment = |text: &str| {
            self.word_segmenter.as_ref().map_or_else(
                || pie_core::word_navigation::default_word_segments(text),
                |host| host.segment(text),
            )
        };
        self.cursor = find_word_forward(
            &self.value,
            self.cursor,
            &WordNavOptions {
                segment: Some(&segment),
                is_atomic_segment: None,
            },
        );
    }

    fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let was_kill = self.last_action == Some(InputLastAction::Kill);
        self.push_undo();
        let old = self.cursor;
        self.move_word_backward();
        let start = self.cursor;
        let deleted = utf16_slice(&self.value, start, old).to_owned();
        self.kill_ring.push(
            &deleted,
            PushOptions {
                prepend: true,
                accumulate: was_kill,
            },
        );
        self.value = format!(
            "{}{}",
            utf16_prefix(&self.value, start),
            utf16_suffix(&self.value, old)
        );
        self.cursor = start;
        self.last_action = Some(InputLastAction::Kill);
    }

    fn delete_word_forward(&mut self) {
        if self.cursor >= utf16_len(&self.value) {
            return;
        }
        let was_kill = self.last_action == Some(InputLastAction::Kill);
        self.push_undo();
        let old = self.cursor;
        self.move_word_forward();
        let end = self.cursor;
        let deleted = utf16_slice(&self.value, old, end).to_owned();
        self.kill_ring.push(
            &deleted,
            PushOptions {
                prepend: false,
                accumulate: was_kill,
            },
        );
        self.value = format!(
            "{}{}",
            utf16_prefix(&self.value, old),
            utf16_suffix(&self.value, end)
        );
        self.cursor = old;
        self.last_action = Some(InputLastAction::Kill);
    }

    fn yank(&mut self) {
        let Some(text) = self.kill_ring.peek().map(str::to_owned) else {
            return;
        };
        self.push_undo();
        let byte = utf16_to_byte(&self.value, self.cursor);
        self.value.insert_str(byte, &text);
        self.cursor += utf16_len(&text);
        self.last_action = Some(InputLastAction::Yank);
    }

    fn yank_pop(&mut self) {
        if self.last_action != Some(InputLastAction::Yank) || self.kill_ring.len() <= 1 {
            return;
        }
        self.push_undo();
        let previous = self.kill_ring.peek().unwrap_or_default().to_owned();
        let start = self.cursor.saturating_sub(utf16_len(&previous));
        self.value = format!(
            "{}{}",
            utf16_prefix(&self.value, start),
            utf16_suffix(&self.value, self.cursor)
        );
        self.cursor = start;
        self.kill_ring.rotate();
        let next = self.kill_ring.peek().unwrap_or_default().to_owned();
        let byte = utf16_to_byte(&self.value, self.cursor);
        self.value.insert_str(byte, &next);
        self.cursor += utf16_len(&next);
        self.last_action = Some(InputLastAction::Yank);
    }

    fn undo(&mut self) {
        let Some(snapshot) = self.undo_stack.pop() else {
            return;
        };
        self.value = snapshot.value;
        self.cursor = snapshot.cursor;
        self.last_action = None;
    }

    fn handle_paste(&mut self, text: &str) {
        self.last_action = None;
        self.push_undo();
        let clean = text
            .replace("\r\n", "")
            .replace(['\r', '\n'], "")
            .replace('\t', "    ");
        let byte = utf16_to_byte(&self.value, self.cursor);
        self.value.insert_str(byte, &clean);
        self.cursor += utf16_len(&clean);
    }

    fn move_left(&mut self) {
        self.last_action = None;
        if self.cursor == 0 {
            return;
        }
        let before = utf16_prefix(&self.value, self.cursor);
        let grapheme = before.graphemes(true).next_back().unwrap_or("");
        self.cursor = self.cursor.saturating_sub(utf16_len(grapheme).max(1));
    }

    fn move_right(&mut self) {
        self.last_action = None;
        if self.cursor >= utf16_len(&self.value) {
            return;
        }
        let after = utf16_suffix(&self.value, self.cursor);
        let grapheme = after.graphemes(true).next().unwrap_or("");
        self.cursor += utf16_len(grapheme).max(1);
    }

    fn contains_control(text: &str) -> bool {
        text.chars().any(|character| {
            let code = character as u32;
            code < 32 || code == 0x7f || (0x80..=0x9f).contains(&code)
        })
    }

    pub fn handle_input(&mut self, data: &str) {
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
                self.handle_paste(&paste);
                self.is_in_paste = false;
                self.paste_buffer.clear();
                if !remaining.is_empty() {
                    self.handle_input(&remaining);
                }
            }
            return;
        }

        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            self.call_escape();
        } else if keybindings.matches(data, "tui.editor.undo") {
            self.undo();
        } else if keybindings.matches(data, "tui.input.submit") || data == "\n" {
            self.call_submit();
        } else if keybindings.matches(data, "tui.editor.deleteCharBackward") {
            self.backspace();
        } else if keybindings.matches(data, "tui.editor.deleteCharForward") {
            self.delete_forward();
        } else if keybindings.matches(data, "tui.editor.deleteWordBackward") {
            self.delete_word_backward();
        } else if keybindings.matches(data, "tui.editor.deleteWordForward") {
            self.delete_word_forward();
        } else if keybindings.matches(data, "tui.editor.deleteToLineStart") {
            self.delete_line_start();
        } else if keybindings.matches(data, "tui.editor.deleteToLineEnd") {
            self.delete_line_end();
        } else if keybindings.matches(data, "tui.editor.yank") {
            self.yank();
        } else if keybindings.matches(data, "tui.editor.yankPop") {
            self.yank_pop();
        } else if keybindings.matches(data, "tui.editor.cursorLeft") {
            self.move_left();
        } else if keybindings.matches(data, "tui.editor.cursorRight") {
            self.move_right();
        } else if keybindings.matches(data, "tui.editor.cursorLineStart") {
            self.last_action = None;
            self.cursor = 0;
        } else if keybindings.matches(data, "tui.editor.cursorLineEnd") {
            self.last_action = None;
            self.cursor = utf16_len(&self.value);
        } else if keybindings.matches(data, "tui.editor.cursorWordLeft") {
            self.move_word_backward();
        } else if keybindings.matches(data, "tui.editor.cursorWordRight") {
            self.move_word_forward();
        } else if let Some(printable) = decode_kitty_printable(data) {
            self.insert_character(&printable);
        } else if !Self::contains_control(data) {
            self.insert_character(data);
        }
    }

    pub fn render(&self, width: usize) -> Vec<String> {
        const PROMPT: &str = "> ";
        let available_width = width.saturating_sub(PROMPT.len());
        if width <= PROMPT.len() {
            return vec![PROMPT.to_owned()];
        }

        let total_width = visible_width(&self.value);
        let mut visible_text = self.value.clone();
        let mut cursor_display = self.cursor;
        if total_width >= available_width {
            let scroll_width = if self.cursor == utf16_len(&self.value) {
                available_width.saturating_sub(1)
            } else {
                available_width
            };
            let cursor_col = visible_width(utf16_prefix(&self.value, self.cursor));
            if scroll_width > 0 {
                let half = scroll_width / 2;
                let start_col = if cursor_col < half {
                    0
                } else if cursor_col > total_width.saturating_sub(half) {
                    total_width.saturating_sub(scroll_width)
                } else {
                    cursor_col.saturating_sub(half)
                };
                visible_text = slice_by_column(&self.value, start_col, scroll_width, true);
                let before = slice_by_column(
                    &self.value,
                    start_col,
                    cursor_col.saturating_sub(start_col),
                    true,
                );
                cursor_display = utf16_len(&before);
            } else {
                visible_text.clear();
                cursor_display = 0;
            }
        }

        let suffix = utf16_suffix(&visible_text, cursor_display);
        let at_cursor = suffix.graphemes(true).next().unwrap_or(" ");
        let before = utf16_prefix(&visible_text, cursor_display);
        let after = utf16_suffix(&visible_text, cursor_display + utf16_len(at_cursor));
        let marker = if self.focused { CURSOR_MARKER } else { "" };
        let display = format!("{before}{marker}\x1b[7m{at_cursor}\x1b[27m{after}");
        let padding = " ".repeat(available_width.saturating_sub(visible_width(&display)));
        vec![format!("{PROMPT}{display}{padding}")]
    }
}

impl Component for Input {
    fn render(&mut self, width: usize) -> Vec<String> {
        Input::render(self, width)
    }

    fn handle_input(&mut self, data: &str) {
        Input::handle_input(self, data);
    }

    fn focused(&self) -> Option<bool> {
        Some(self.focused)
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.focused = focused;
        true
    }
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
