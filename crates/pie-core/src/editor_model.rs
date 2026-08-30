//! Pure editor state and ordered effects.
//!
//! This module owns no terminal, callback, timer, future, or renderer. Its
//! cursor columns are JavaScript-compatible UTF-16 code-unit offsets.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

use regex::{Captures, Regex};
use unicode_segmentation::UnicodeSegmentation;

use crate::fuzzy::js_is_whitespace;
use crate::kill_ring::{KillRing, PushOptions};
use crate::text::visible_width;
use crate::undo_stack::UndoStack;
use crate::word_navigation::{
    WordNavOptions, WordSegment, default_word_segments, find_word_backward, find_word_forward,
};
use crate::wrap::is_cjk_break_segment;

static PASTE_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[paste #([0-9]+)( (\+[0-9]+ lines|[0-9]+ chars))?\]").expect("paste marker regex")
});
static PASTE_MARKER_FULL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[paste #([0-9]+)( (\+[0-9]+ lines|[0-9]+ chars))?\]$")
        .expect("full paste marker regex")
});
static TMUX_CTRL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[([0-9]+);5u").expect("tmux control regex"));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorCursor {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEffect {
    Change(String),
    Submit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    SetText(String),
    InsertText(String),
    Type(String),
    Paste(String),
    NewLine,
    Submit,
    Backspace,
    DeleteForward,
    LineStart,
    LineEnd,
    DeleteLineStart,
    DeleteLineEnd,
    DeleteWordBackward,
    DeleteWordForward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordBackward,
    MoveWordForward,
    PageUp,
    PageDown,
    JumpBackward(String),
    JumpForward(String),
    AddHistory(String),
    HistoryPrevious,
    HistoryNext,
    Yank,
    YankPop,
    Undo,
    ApplyCompletion {
        lines: Vec<String>,
        cursor: EditorCursor,
    },
    SetView {
        width: usize,
        rows: usize,
    },
}

/// Owned host word-segmentation seam. The pure default remains bounded to the
/// pinned UAX/dictionary fallback; an adapter can inject host `Intl.Segmenter`
/// partitions without introducing an upward dependency into `pie-core`.
type OwnedWordSegmenter = dyn Fn(&str) -> Vec<WordSegment> + Send + Sync;

#[derive(Clone)]
pub struct EditorWordSegmenter(Arc<OwnedWordSegmenter>);

impl EditorWordSegmenter {
    pub fn new(segment: impl Fn(&str) -> Vec<WordSegment> + Send + Sync + 'static) -> Self {
        Self(Arc::new(segment))
    }

    pub fn segment(&self, text: &str) -> Vec<WordSegment> {
        (self.0)(text)
    }
}

impl fmt::Debug for EditorWordSegmenter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EditorWordSegmenter(..)")
    }
}

/// Detached observable state used by hosts and differential tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorModelSnapshot {
    pub text: String,
    pub expanded_text: String,
    pub lines: Vec<String>,
    pub cursor: EditorCursor,
    pub pastes: Vec<(u32, String)>,
    pub paste_counter: u32,
    pub history: Vec<String>,
    pub history_index: isize,
    pub kill_length: usize,
    pub kill_peek: Option<String>,
    pub undo_length: usize,
    pub last_action: Option<String>,
    pub preferred_visual_col: Option<usize>,
    pub snapped_from_cursor_col: Option<usize>,
    pub visual_lines: Vec<EditorVisualLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorVisualLine {
    pub logical_line: usize,
    pub start_col: usize,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorState {
    lines: Vec<String>,
    cursor: EditorCursor,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: EditorCursor::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UndoSnapshot {
    state: EditorState,
    pastes: BTreeMap<u32, String>,
    paste_counter: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastAction {
    TypeWord,
    Kill,
    Yank,
}

impl LastAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::TypeWord => "type-word",
            Self::Kill => "kill",
            Self::Yank => "yank",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TextSpan {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
pub struct EditorModel {
    state: EditorState,
    pastes: BTreeMap<u32, String>,
    paste_counter: u32,
    history: Vec<String>,
    history_index: isize,
    history_draft: Option<EditorState>,
    kill_ring: KillRing,
    last_action: Option<LastAction>,
    preferred_visual_col: Option<usize>,
    snapped_from_cursor_col: Option<usize>,
    undo_stack: UndoStack<UndoSnapshot>,
    view_width: usize,
    terminal_rows: usize,
    word_segmenter: Option<EditorWordSegmenter>,
}

impl Default for EditorModel {
    fn default() -> Self {
        Self {
            state: EditorState::default(),
            pastes: BTreeMap::new(),
            paste_counter: 0,
            history: Vec::new(),
            history_index: -1,
            history_draft: None,
            kill_ring: KillRing::new(),
            last_action: None,
            preferred_visual_col: None,
            snapped_from_cursor_col: None,
            undo_stack: UndoStack::new(),
            view_width: 80,
            terminal_rows: 24,
            word_segmenter: None,
        }
    }
}

impl EditorModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.state.lines.join("\n")
    }

    pub fn expanded_text(&self) -> String {
        self.expand_paste_markers(&self.text())
    }

    pub fn cursor(&self) -> EditorCursor {
        self.state.cursor
    }

    pub fn set_word_segmenter(&mut self, segmenter: Option<EditorWordSegmenter>) {
        self.word_segmenter = segmenter;
    }

    pub fn snapshot(&self) -> EditorModelSnapshot {
        EditorModelSnapshot {
            text: self.text(),
            expanded_text: self.expanded_text(),
            lines: self.state.lines.clone(),
            cursor: self.state.cursor,
            pastes: self
                .pastes
                .iter()
                .map(|(id, text)| (*id, text.clone()))
                .collect(),
            paste_counter: self.paste_counter,
            history: self.history.clone(),
            history_index: self.history_index,
            kill_length: self.kill_ring.len(),
            kill_peek: self.kill_ring.peek().map(str::to_owned),
            undo_length: self.undo_stack.len(),
            last_action: self.last_action.map(LastAction::as_str).map(str::to_owned),
            preferred_visual_col: self.preferred_visual_col,
            snapped_from_cursor_col: self.snapped_from_cursor_col,
            visual_lines: self.build_visual_lines(self.view_width),
        }
    }

    /// Apply one pure action and return effects in the order a host must emit
    /// them. No callbacks are retained or executed by the model.
    pub fn apply(&mut self, action: EditorAction) -> Vec<EditorEffect> {
        match action {
            EditorAction::SetText(text) => self.set_text(&text),
            EditorAction::InsertText(text) => self.insert_text(&text),
            EditorAction::Type(text) => self.type_text(&text),
            EditorAction::Paste(text) => self.paste(&text),
            EditorAction::NewLine => self.new_line(),
            EditorAction::Submit => self.submit(),
            EditorAction::Backspace => self.backspace(),
            EditorAction::DeleteForward => self.delete_forward(),
            EditorAction::LineStart => {
                self.last_action = None;
                self.set_cursor_col(0);
                Vec::new()
            }
            EditorAction::LineEnd => {
                self.last_action = None;
                self.set_cursor_col(self.current_line_units());
                Vec::new()
            }
            EditorAction::DeleteLineStart => self.delete_line_start(),
            EditorAction::DeleteLineEnd => self.delete_line_end(),
            EditorAction::DeleteWordBackward => self.delete_word_backward(),
            EditorAction::DeleteWordForward => self.delete_word_forward(),
            EditorAction::MoveLeft => {
                self.move_horizontal(-1);
                Vec::new()
            }
            EditorAction::MoveRight => {
                self.move_horizontal(1);
                Vec::new()
            }
            EditorAction::MoveUp => {
                self.move_vertical(-1);
                Vec::new()
            }
            EditorAction::MoveDown => {
                self.move_vertical(1);
                Vec::new()
            }
            EditorAction::MoveWordBackward => {
                self.move_word_backward();
                Vec::new()
            }
            EditorAction::MoveWordForward => {
                self.move_word_forward();
                Vec::new()
            }
            EditorAction::PageUp => {
                self.page(-1);
                Vec::new()
            }
            EditorAction::PageDown => {
                self.page(1);
                Vec::new()
            }
            EditorAction::JumpBackward(text) => {
                self.jump_to(&text, false);
                Vec::new()
            }
            EditorAction::JumpForward(text) => {
                self.jump_to(&text, true);
                Vec::new()
            }
            EditorAction::AddHistory(text) => {
                self.add_history(&text);
                Vec::new()
            }
            EditorAction::HistoryPrevious => self.navigate_history(-1),
            EditorAction::HistoryNext => self.navigate_history(1),
            EditorAction::Yank => self.yank(),
            EditorAction::YankPop => self.yank_pop(),
            EditorAction::Undo => self.undo(),
            EditorAction::ApplyCompletion { lines, cursor } => self.apply_completion(lines, cursor),
            EditorAction::SetView { width, rows } => {
                self.view_width = width;
                self.terminal_rows = rows;
                Vec::new()
            }
        }
    }

    fn change_effect(&self) -> Vec<EditorEffect> {
        vec![EditorEffect::Change(self.text())]
    }

    fn apply_completion(
        &mut self,
        mut lines: Vec<String>,
        mut cursor: EditorCursor,
    ) -> Vec<EditorEffect> {
        self.push_undo();
        self.exit_history();
        self.last_action = None;
        if lines.is_empty() {
            lines.push(String::new());
        }
        cursor.line = cursor.line.min(lines.len() - 1);
        cursor.col = cursor.col.min(utf16_len(&lines[cursor.line]));
        cursor.col = byte_to_utf16(
            &lines[cursor.line],
            utf16_to_byte(&lines[cursor.line], cursor.col),
        );
        self.state = EditorState { lines, cursor };
        self.preferred_visual_col = None;
        self.snapped_from_cursor_col = None;
        self.change_effect()
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(&UndoSnapshot {
            state: self.state.clone(),
            pastes: self.pastes.clone(),
            paste_counter: self.paste_counter,
        });
    }

    fn exit_history(&mut self) {
        self.history_index = -1;
        self.history_draft = None;
    }

    fn set_cursor_col(&mut self, col: usize) {
        self.state.cursor.col = col;
        self.preferred_visual_col = None;
        self.snapped_from_cursor_col = None;
    }

    fn current_line(&self) -> &str {
        self.state
            .lines
            .get(self.state.cursor.line)
            .map_or("", String::as_str)
    }

    fn current_line_units(&self) -> usize {
        utf16_len(self.current_line())
    }

    fn set_text(&mut self, text: &str) -> Vec<EditorEffect> {
        self.last_action = None;
        self.exit_history();
        let normalized = normalize_text(text);
        if self.text() != normalized {
            self.push_undo();
        }
        self.pastes.clear();
        self.paste_counter = 0;
        self.set_text_internal(&normalized, false);
        self.change_effect()
    }

    fn set_text_internal(&mut self, text: &str, at_start: bool) {
        self.state.lines = text.split('\n').map(str::to_owned).collect();
        if self.state.lines.is_empty() {
            self.state.lines.push(String::new());
        }
        self.state.cursor.line = if at_start {
            0
        } else {
            self.state.lines.len() - 1
        };
        let col = if at_start {
            0
        } else {
            utf16_len(&self.state.lines[self.state.cursor.line])
        };
        self.set_cursor_col(col);
    }

    fn insert_text(&mut self, text: &str) -> Vec<EditorEffect> {
        if text.is_empty() {
            return Vec::new();
        }
        self.push_undo();
        self.last_action = None;
        self.exit_history();
        self.insert_text_internal(text);
        self.change_effect()
    }

    fn insert_text_internal(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let normalized = normalize_text(text);
        let inserted: Vec<&str> = normalized.split('\n').collect();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        let cursor_byte = utf16_to_byte(&current, self.state.cursor.col);
        let before = &current[..cursor_byte];
        let after = &current[cursor_byte..];
        if inserted.len() == 1 {
            self.state.lines[line_index] = format!("{before}{normalized}{after}");
            self.set_cursor_col(self.state.cursor.col + utf16_len(&normalized));
            return;
        }

        let mut replacement = Vec::with_capacity(inserted.len());
        replacement.push(format!("{before}{}", inserted[0]));
        replacement.extend(
            inserted[1..inserted.len() - 1]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        replacement.push(format!("{}{after}", inserted[inserted.len() - 1]));
        self.state
            .lines
            .splice(line_index..=line_index, replacement);
        self.state.cursor.line = line_index + inserted.len() - 1;
        self.set_cursor_col(utf16_len(inserted[inserted.len() - 1]));
    }

    fn type_text(&mut self, text: &str) -> Vec<EditorEffect> {
        self.exit_history();
        if text.chars().any(js_is_whitespace) || self.last_action != Some(LastAction::TypeWord) {
            self.push_undo();
        }
        self.last_action = Some(LastAction::TypeWord);
        self.insert_raw_at_cursor(text);
        self.change_effect()
    }

    fn insert_raw_at_cursor(&mut self, text: &str) {
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        let cursor_byte = utf16_to_byte(&current, self.state.cursor.col);
        self.state.lines[line_index] = format!(
            "{}{text}{}",
            &current[..cursor_byte],
            &current[cursor_byte..]
        );
        self.set_cursor_col(self.state.cursor.col + utf16_len(text));
    }

    fn paste(&mut self, text: &str) -> Vec<EditorEffect> {
        self.exit_history();
        self.last_action = None;
        self.push_undo();
        let decoded = TMUX_CTRL_RE.replace_all(text, |captures: &Captures<'_>| {
            let code = captures[1].parse::<u32>().unwrap_or_default();
            match code {
                97..=122 => char::from_u32(code - 96).unwrap_or_default().to_string(),
                65..=90 => char::from_u32(code - 64).unwrap_or_default().to_string(),
                _ => captures[0].to_owned(),
            }
        });
        let clean = normalize_text(&decoded);
        let mut filtered: String = clean
            .chars()
            .filter(|character| *character == '\n' || u32::from(*character) >= 32)
            .collect();
        if filtered.starts_with(['/', '~', '.']) {
            let prefix = utf16_prefix(self.current_line(), self.state.cursor.col);
            if prefix
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                filtered.insert(0, ' ');
            }
        }
        let line_count = filtered.split('\n').count();
        let total_units = utf16_len(&filtered);
        if line_count > 10 || total_units > 1000 {
            self.paste_counter += 1;
            let id = self.paste_counter;
            self.pastes.insert(id, filtered);
            let marker = if line_count > 10 {
                format!("[paste #{id} +{line_count} lines]")
            } else {
                format!("[paste #{id} {total_units} chars]")
            };
            self.insert_text_internal(&marker);
        } else {
            self.insert_text_internal(&filtered);
        }
        if filtered_is_empty_after_paste(text, &decoded, &clean) {
            Vec::new()
        } else {
            self.change_effect()
        }
    }

    fn new_line(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        self.last_action = None;
        self.push_undo();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        let cursor_byte = utf16_to_byte(&current, self.state.cursor.col);
        self.state.lines[line_index] = current[..cursor_byte].to_owned();
        self.state
            .lines
            .insert(line_index + 1, current[cursor_byte..].to_owned());
        self.state.cursor.line += 1;
        self.set_cursor_col(0);
        self.change_effect()
    }

    fn submit(&mut self) -> Vec<EditorEffect> {
        let expanded = self.expanded_text();
        let result = expanded.trim_matches(js_is_whitespace).to_owned();
        self.state = EditorState::default();
        self.pastes.clear();
        self.paste_counter = 0;
        self.exit_history();
        self.undo_stack.clear();
        self.last_action = None;
        self.preferred_visual_col = None;
        self.snapped_from_cursor_col = None;
        vec![
            EditorEffect::Change(String::new()),
            EditorEffect::Submit(result),
        ]
    }

    fn backspace(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        self.last_action = None;
        if self.state.cursor.col > 0 {
            self.push_undo();
            let line_index = self.state.cursor.line;
            let current = self.current_line().to_owned();
            let span = self
                .grapheme_spans(&current)
                .into_iter()
                .rev()
                .find(|span| span.end <= self.state.cursor.col)
                .unwrap_or(TextSpan {
                    start: self.state.cursor.col.saturating_sub(1),
                    end: self.state.cursor.col,
                });
            let deleted = utf16_slice(&current, span.start, span.end);
            if let Some(id) = paste_marker_id(deleted) {
                self.remove_paste_id(id);
            }
            let updated = self.state.lines[line_index].clone();
            let before = utf16_prefix(&updated, span.start);
            let after = utf16_suffix(&updated, span.end);
            self.state.lines[line_index] = format!("{before}{after}");
            self.set_cursor_col(span.start);
        } else if self.state.cursor.line > 0 {
            self.push_undo();
            let line_index = self.state.cursor.line;
            let current = self.state.lines[line_index].clone();
            let previous = self.state.lines[line_index - 1].clone();
            let previous_units = utf16_len(&previous);
            self.state.lines[line_index - 1] = format!("{previous}{current}");
            self.state.lines.remove(line_index);
            self.state.cursor.line -= 1;
            self.set_cursor_col(previous_units);
        }
        self.change_effect()
    }

    fn delete_forward(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        self.last_action = None;
        let current_units = self.current_line_units();
        if self.state.cursor.col < current_units {
            self.push_undo();
            let line_index = self.state.cursor.line;
            let current = self.current_line().to_owned();
            let span = self
                .grapheme_spans(&current)
                .into_iter()
                .find(|span| span.start >= self.state.cursor.col)
                .unwrap_or(TextSpan {
                    start: self.state.cursor.col,
                    end: self.state.cursor.col + 1,
                });
            self.state.lines[line_index] = format!(
                "{}{}",
                utf16_prefix(&current, span.start),
                utf16_suffix(&current, span.end)
            );
        } else if self.state.cursor.line + 1 < self.state.lines.len() {
            self.push_undo();
            let line_index = self.state.cursor.line;
            let next = self.state.lines.remove(line_index + 1);
            self.state.lines[line_index].push_str(&next);
        }
        self.change_effect()
    }

    fn delete_line_start(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        if self.state.cursor.col > 0 {
            self.push_undo();
            let deleted = utf16_prefix(&current, self.state.cursor.col);
            self.kill_ring.push(
                deleted,
                PushOptions {
                    prepend: true,
                    accumulate: self.last_action == Some(LastAction::Kill),
                },
            );
            self.last_action = Some(LastAction::Kill);
            self.state.lines[line_index] = utf16_suffix(&current, self.state.cursor.col).to_owned();
            self.set_cursor_col(0);
        } else if line_index > 0 {
            self.push_undo();
            self.kill_ring.push(
                "\n",
                PushOptions {
                    prepend: true,
                    accumulate: self.last_action == Some(LastAction::Kill),
                },
            );
            self.last_action = Some(LastAction::Kill);
            let previous = self.state.lines[line_index - 1].clone();
            let previous_units = utf16_len(&previous);
            self.state.lines[line_index - 1] = format!("{previous}{current}");
            self.state.lines.remove(line_index);
            self.state.cursor.line -= 1;
            self.set_cursor_col(previous_units);
        }
        self.change_effect()
    }

    fn delete_line_end(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        if self.state.cursor.col < utf16_len(&current) {
            self.push_undo();
            let deleted = utf16_suffix(&current, self.state.cursor.col);
            self.kill_ring.push(
                deleted,
                PushOptions {
                    prepend: false,
                    accumulate: self.last_action == Some(LastAction::Kill),
                },
            );
            self.last_action = Some(LastAction::Kill);
            self.state.lines[line_index] = utf16_prefix(&current, self.state.cursor.col).to_owned();
        } else if line_index + 1 < self.state.lines.len() {
            self.push_undo();
            self.kill_ring.push(
                "\n",
                PushOptions {
                    prepend: false,
                    accumulate: self.last_action == Some(LastAction::Kill),
                },
            );
            self.last_action = Some(LastAction::Kill);
            let next = self.state.lines.remove(line_index + 1);
            self.state.lines[line_index].push_str(&next);
        }
        self.change_effect()
    }

    fn delete_word_backward(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        if self.state.cursor.col == 0 {
            if line_index > 0 {
                self.push_undo();
                self.kill_ring.push(
                    "\n",
                    PushOptions {
                        prepend: true,
                        accumulate: self.last_action == Some(LastAction::Kill),
                    },
                );
                self.last_action = Some(LastAction::Kill);
                let previous = self.state.lines[line_index - 1].clone();
                let previous_units = utf16_len(&previous);
                self.state.lines[line_index - 1] = format!("{previous}{current}");
                self.state.lines.remove(line_index);
                self.state.cursor.line -= 1;
                self.set_cursor_col(previous_units);
            }
        } else {
            self.push_undo();
            let was_kill = self.last_action == Some(LastAction::Kill);
            let old_col = self.state.cursor.col;
            self.move_word_backward();
            let delete_from = self.state.cursor.col;
            self.set_cursor_col(old_col);
            let deleted = utf16_slice(&current, delete_from, old_col);
            self.kill_ring.push(
                deleted,
                PushOptions {
                    prepend: true,
                    accumulate: was_kill,
                },
            );
            self.last_action = Some(LastAction::Kill);
            self.state.lines[line_index] = format!(
                "{}{}",
                utf16_prefix(&current, delete_from),
                utf16_suffix(&current, old_col)
            );
            self.set_cursor_col(delete_from);
        }
        self.change_effect()
    }

    fn delete_word_forward(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        if self.state.cursor.col >= utf16_len(&current) {
            if line_index + 1 < self.state.lines.len() {
                self.push_undo();
                self.kill_ring.push(
                    "\n",
                    PushOptions {
                        prepend: false,
                        accumulate: self.last_action == Some(LastAction::Kill),
                    },
                );
                self.last_action = Some(LastAction::Kill);
                let next = self.state.lines.remove(line_index + 1);
                self.state.lines[line_index].push_str(&next);
            }
        } else {
            self.push_undo();
            let was_kill = self.last_action == Some(LastAction::Kill);
            let old_col = self.state.cursor.col;
            self.move_word_forward();
            let delete_to = self.state.cursor.col;
            self.set_cursor_col(old_col);
            let deleted = utf16_slice(&current, old_col, delete_to);
            self.kill_ring.push(
                deleted,
                PushOptions {
                    prepend: false,
                    accumulate: was_kill,
                },
            );
            self.last_action = Some(LastAction::Kill);
            self.state.lines[line_index] = format!(
                "{}{}",
                utf16_prefix(&current, old_col),
                utf16_suffix(&current, delete_to)
            );
        }
        self.change_effect()
    }

    fn move_horizontal(&mut self, direction: isize) {
        self.last_action = None;
        let visual_lines = self.build_visual_lines(self.view_width);
        let current_visual_line = self.find_current_visual_line(&visual_lines);
        let current = self.current_line().to_owned();
        if direction > 0 {
            if self.state.cursor.col < utf16_len(&current) {
                if let Some(span) = self
                    .grapheme_spans(&current)
                    .into_iter()
                    .find(|span| span.start >= self.state.cursor.col)
                {
                    self.set_cursor_col(span.end);
                }
            } else if self.state.cursor.line + 1 < self.state.lines.len() {
                self.state.cursor.line += 1;
                self.set_cursor_col(0);
            } else if let Some(line) = visual_lines.get(current_visual_line) {
                self.preferred_visual_col = Some(self.state.cursor.col - line.start_col);
            }
        } else if self.state.cursor.col > 0 {
            if let Some(span) = self
                .grapheme_spans(&current)
                .into_iter()
                .rev()
                .find(|span| span.end <= self.state.cursor.col)
            {
                self.set_cursor_col(span.start);
            }
        } else if self.state.cursor.line > 0 {
            self.state.cursor.line -= 1;
            self.set_cursor_col(self.current_line_units());
        }
    }

    fn move_vertical(&mut self, direction: isize) {
        self.last_action = None;
        let visual_lines = self.build_visual_lines(self.view_width);
        let current = self.find_current_visual_line(&visual_lines);
        let target = current as isize + direction;
        if target >= 0 && (target as usize) < visual_lines.len() {
            self.move_to_visual_line(&visual_lines, current, target as usize);
        }
    }

    fn page(&mut self, direction: isize) {
        self.last_action = None;
        let page_size = 5.max(self.terminal_rows.saturating_mul(3) / 10);
        let visual_lines = self.build_visual_lines(self.view_width);
        let current = self.find_current_visual_line(&visual_lines);
        let target = (current as isize + direction * page_size as isize)
            .clamp(0, visual_lines.len().saturating_sub(1) as isize) as usize;
        self.move_to_visual_line(&visual_lines, current, target);
    }

    fn move_word_backward(&mut self) {
        self.last_action = None;
        if self.state.cursor.col == 0 {
            if self.state.cursor.line > 0 {
                self.state.cursor.line -= 1;
                self.set_cursor_col(self.current_line_units());
            }
            return;
        }
        let line = self.current_line().to_owned();
        let segment = |text: &str| {
            word_segments_with_pastes(text, &self.pastes, self.word_segmenter.as_ref())
        };
        let is_atomic =
            |text: &str| paste_marker_id(text).is_some_and(|id| self.pastes.contains_key(&id));
        let col = find_word_backward(
            &line,
            self.state.cursor.col,
            &WordNavOptions {
                segment: Some(&segment),
                is_atomic_segment: Some(&is_atomic),
            },
        );
        self.set_cursor_col(col);
    }

    fn move_word_forward(&mut self) {
        self.last_action = None;
        if self.state.cursor.col >= self.current_line_units() {
            if self.state.cursor.line + 1 < self.state.lines.len() {
                self.state.cursor.line += 1;
                self.set_cursor_col(0);
            }
            return;
        }
        let line = self.current_line().to_owned();
        let segment = |text: &str| {
            word_segments_with_pastes(text, &self.pastes, self.word_segmenter.as_ref())
        };
        let is_atomic =
            |text: &str| paste_marker_id(text).is_some_and(|id| self.pastes.contains_key(&id));
        let col = find_word_forward(
            &line,
            self.state.cursor.col,
            &WordNavOptions {
                segment: Some(&segment),
                is_atomic_segment: Some(&is_atomic),
            },
        );
        self.set_cursor_col(col);
    }

    fn yank(&mut self) -> Vec<EditorEffect> {
        let Some(text) = self.kill_ring.peek().map(str::to_owned) else {
            return Vec::new();
        };
        self.push_undo();
        self.insert_yanked_text(&text);
        self.last_action = Some(LastAction::Yank);
        self.change_effect()
    }

    fn yank_pop(&mut self) -> Vec<EditorEffect> {
        if self.last_action != Some(LastAction::Yank) || self.kill_ring.len() <= 1 {
            return Vec::new();
        }
        self.push_undo();
        self.delete_yanked_text();
        let mut effects = self.change_effect();
        self.kill_ring.rotate();
        let text = self.kill_ring.peek().unwrap_or_default().to_owned();
        self.insert_yanked_text(&text);
        effects.extend(self.change_effect());
        self.last_action = Some(LastAction::Yank);
        effects
    }

    fn insert_yanked_text(&mut self, text: &str) {
        self.exit_history();
        let inserted: Vec<&str> = text.split('\n').collect();
        let line_index = self.state.cursor.line;
        let current = self.current_line().to_owned();
        let cursor_byte = utf16_to_byte(&current, self.state.cursor.col);
        let before = &current[..cursor_byte];
        let after = &current[cursor_byte..];
        if inserted.len() == 1 {
            self.state.lines[line_index] = format!("{before}{text}{after}");
            self.set_cursor_col(self.state.cursor.col + utf16_len(text));
            return;
        }
        let mut replacement = Vec::with_capacity(inserted.len());
        replacement.push(format!("{before}{}", inserted[0]));
        replacement.extend(
            inserted[1..inserted.len() - 1]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        replacement.push(format!("{}{after}", inserted[inserted.len() - 1]));
        self.state
            .lines
            .splice(line_index..=line_index, replacement);
        self.state.cursor.line = line_index + inserted.len() - 1;
        self.set_cursor_col(utf16_len(inserted[inserted.len() - 1]));
    }

    fn delete_yanked_text(&mut self) {
        let Some(text) = self.kill_ring.peek().map(str::to_owned) else {
            return;
        };
        let lines: Vec<&str> = text.split('\n').collect();
        if lines.len() == 1 {
            let line_index = self.state.cursor.line;
            let current = self.current_line().to_owned();
            let start = self.state.cursor.col.saturating_sub(utf16_len(&text));
            self.state.lines[line_index] = format!(
                "{}{}",
                utf16_prefix(&current, start),
                utf16_suffix(&current, self.state.cursor.col)
            );
            self.set_cursor_col(start);
            return;
        }
        let start_line = self.state.cursor.line - (lines.len() - 1);
        let start_col = utf16_len(&self.state.lines[start_line]) - utf16_len(lines[0]);
        let before = utf16_prefix(&self.state.lines[start_line], start_col).to_owned();
        let after = utf16_suffix(
            &self.state.lines[self.state.cursor.line],
            self.state.cursor.col,
        )
        .to_owned();
        self.state.lines.splice(
            start_line..=self.state.cursor.line,
            [format!("{before}{after}")],
        );
        self.state.cursor.line = start_line;
        self.set_cursor_col(start_col);
    }

    fn undo(&mut self) -> Vec<EditorEffect> {
        self.exit_history();
        let Some(snapshot) = self.undo_stack.pop() else {
            return Vec::new();
        };
        self.state = snapshot.state;
        self.pastes = snapshot.pastes;
        self.paste_counter = snapshot.paste_counter;
        self.last_action = None;
        self.preferred_visual_col = None;
        self.change_effect()
    }

    fn add_history(&mut self, text: &str) {
        let trimmed = text.trim_matches(js_is_whitespace);
        if trimmed.is_empty() || self.history.first().is_some_and(|entry| entry == trimmed) {
            return;
        }
        self.history.insert(0, trimmed.to_owned());
        self.history.truncate(100);
    }

    fn navigate_history(&mut self, direction: isize) -> Vec<EditorEffect> {
        self.last_action = None;
        if self.history.is_empty() {
            return Vec::new();
        }
        let new_index = self.history_index - direction;
        if new_index < -1 || new_index >= self.history.len() as isize {
            return Vec::new();
        }
        if self.history_index == -1 && new_index >= 0 {
            self.push_undo();
            self.history_draft = Some(self.state.clone());
        }
        self.history_index = new_index;
        if new_index == -1 {
            let draft = self.history_draft.take();
            if let Some(draft) = draft {
                self.state = draft;
                self.preferred_visual_col = None;
                self.snapped_from_cursor_col = None;
            } else {
                self.set_text_internal("", false);
            }
        } else {
            let entry = self.history[new_index as usize].clone();
            self.set_text_internal(&entry, direction == -1);
        }
        self.change_effect()
    }

    fn jump_to(&mut self, needle: &str, forward: bool) {
        self.last_action = None;
        if needle.is_empty() {
            return;
        }
        if forward {
            for line_index in self.state.cursor.line..self.state.lines.len() {
                let line = &self.state.lines[line_index];
                let minimum = if line_index == self.state.cursor.line {
                    self.state.cursor.col + 1
                } else {
                    0
                };
                let found = match_columns(line, needle)
                    .into_iter()
                    .find(|col| *col >= minimum);
                if let Some(col) = found {
                    self.state.cursor.line = line_index;
                    self.set_cursor_col(col);
                    return;
                }
            }
        } else {
            for line_index in (0..=self.state.cursor.line).rev() {
                let line = &self.state.lines[line_index];
                let maximum = if line_index == self.state.cursor.line {
                    self.state.cursor.col.saturating_sub(1)
                } else {
                    usize::MAX
                };
                let found = match_columns(line, needle)
                    .into_iter()
                    .rfind(|col| *col <= maximum);
                if let Some(col) = found {
                    self.state.cursor.line = line_index;
                    self.set_cursor_col(col);
                    return;
                }
            }
        }
    }

    fn expand_paste_markers(&self, text: &str) -> String {
        let mut result = text.to_owned();
        for (paste_id, paste_content) in &self.pastes {
            let marker = Regex::new(&format!(
                r"\[paste #{paste_id}( (\+[0-9]+ lines|[0-9]+ chars))?\]"
            ))
            .expect("paste ID produces a valid marker regex");
            result = marker
                .replace_all(&result, regex::NoExpand(paste_content))
                .into_owned();
        }
        result
    }

    fn remove_paste_id(&mut self, target: u32) {
        self.pastes.remove(&target);
        self.paste_counter = self.paste_counter.saturating_sub(1);
        let mut shifted = BTreeMap::new();
        for (id, text) in std::mem::take(&mut self.pastes) {
            shifted.insert(if id > target { id - 1 } else { id }, text);
        }
        self.pastes = shifted;
        for line in &mut self.state.lines {
            *line = PASTE_MARKER_RE
                .replace_all(line, |captures: &Captures<'_>| {
                    let id = captures[1].parse::<u32>().unwrap_or_default();
                    if id <= target {
                        captures[0].to_owned()
                    } else {
                        let suffix = captures.get(2).map_or("", |capture| capture.as_str());
                        format!("[paste #{}{suffix}]", id - 1)
                    }
                })
                .into_owned();
        }
    }

    fn marker_byte_spans(&self, line: &str) -> Vec<(usize, usize)> {
        PASTE_MARKER_RE
            .captures_iter(line)
            .filter_map(|captures| {
                let id = captures[1].parse::<u32>().ok()?;
                let matched = captures.get(0)?;
                self.pastes
                    .contains_key(&id)
                    .then_some((matched.start(), matched.end()))
            })
            .collect()
    }

    fn grapheme_spans(&self, line: &str) -> Vec<TextSpan> {
        let markers = self.marker_byte_spans(line);
        let mut marker_index = 0;
        let mut result = Vec::new();
        for (byte, grapheme) in line.grapheme_indices(true) {
            while marker_index < markers.len() && markers[marker_index].1 <= byte {
                marker_index += 1;
            }
            if let Some(&(start, end)) = markers.get(marker_index)
                && byte >= start
                && byte < end
            {
                if byte == start {
                    result.push(TextSpan {
                        start: byte_to_utf16(line, start),
                        end: byte_to_utf16(line, end),
                    });
                }
                continue;
            }
            let end = byte + grapheme.len();
            result.push(TextSpan {
                start: byte_to_utf16(line, byte),
                end: byte_to_utf16(line, end),
            });
        }
        result
    }

    fn plain_grapheme_spans(line: &str) -> Vec<TextSpan> {
        line.grapheme_indices(true)
            .map(|(byte, grapheme)| TextSpan {
                start: byte_to_utf16(line, byte),
                end: byte_to_utf16(line, byte + grapheme.len()),
            })
            .collect()
    }

    fn wrap_line_ranges(&self, line: &str, width: usize) -> Vec<(usize, usize)> {
        let width = width.max(1);
        let line_units = utf16_len(line);
        if line.is_empty() || visible_width(line) <= width {
            return vec![(0, line_units)];
        }
        let spans = self.grapheme_spans(line);
        self.wrap_line_ranges_from_spans(line, width, &spans)
    }

    fn wrap_line_ranges_from_spans(
        &self,
        line: &str,
        width: usize,
        spans: &[TextSpan],
    ) -> Vec<(usize, usize)> {
        let line_units = utf16_len(line);
        let mut chunks = Vec::new();
        let mut current_width = 0;
        let mut chunk_start = 0;
        let mut wrap_opportunity: Option<(usize, usize)> = None;
        for (index, span) in spans.iter().enumerate() {
            let text = utf16_slice(line, span.start, span.end);
            let grapheme_width = visible_width(text);
            if current_width + grapheme_width > width {
                if let Some((wrap_index, wrap_width)) = wrap_opportunity
                    && current_width - wrap_width + grapheme_width <= width
                {
                    chunks.push((chunk_start, wrap_index));
                    chunk_start = wrap_index;
                    current_width -= wrap_width;
                } else if chunk_start < span.start {
                    chunks.push((chunk_start, span.start));
                    chunk_start = span.start;
                    current_width = 0;
                }
                wrap_opportunity = None;
            }
            let is_marker = paste_marker_id(text).is_some_and(|id| self.pastes.contains_key(&id));
            if is_marker && grapheme_width > width {
                let marker_spans = Self::plain_grapheme_spans(text);
                let subchunks = self.wrap_line_ranges_from_spans(text, width, &marker_spans);
                let (last, completed) = subchunks.split_last().expect("marker wrap is non-empty");
                for &(start, end) in completed {
                    chunks.push((span.start + start, span.start + end));
                }
                let &(last_start, last_end) = last;
                chunk_start = span.start + last_start;
                current_width = visible_width(utf16_slice(text, last_start, last_end));
                wrap_opportunity = None;
                continue;
            }
            current_width += grapheme_width;
            let next = spans.get(index + 1);
            let is_whitespace = !is_marker && text.chars().any(js_is_whitespace);
            if let Some(next) = next {
                let next_text = utf16_slice(line, next.start, next.end);
                let next_marker =
                    paste_marker_id(next_text).is_some_and(|id| self.pastes.contains_key(&id));
                let next_whitespace = !next_marker && next_text.chars().any(js_is_whitespace);
                let whitespace_boundary = is_whitespace && !next_whitespace;
                let cjk_boundary = !is_whitespace
                    && !next_whitespace
                    && (!is_marker && is_cjk_break_segment(text)
                        || !next_marker && is_cjk_break_segment(next_text));
                if whitespace_boundary || cjk_boundary {
                    wrap_opportunity = Some((next.start, current_width));
                }
            }
        }
        chunks.push((chunk_start, line_units));
        chunks
    }

    fn build_visual_lines(&self, width: usize) -> Vec<EditorVisualLine> {
        let mut result = Vec::new();
        for (logical_line, line) in self.state.lines.iter().enumerate() {
            for (start, end) in self.wrap_line_ranges(line, width) {
                result.push(EditorVisualLine {
                    logical_line,
                    start_col: start,
                    length: end - start,
                });
            }
        }
        result
    }

    fn find_visual_line_at(
        &self,
        lines: &[EditorVisualLine],
        logical_line: usize,
        col: usize,
    ) -> usize {
        for (index, line) in lines.iter().enumerate() {
            if line.logical_line != logical_line {
                continue;
            }
            let offset = col.saturating_sub(line.start_col);
            let last_for_logical = lines
                .get(index + 1)
                .is_none_or(|next| next.logical_line != line.logical_line);
            if col >= line.start_col
                && (offset < line.length || (last_for_logical && offset == line.length))
            {
                return index;
            }
        }
        lines.len().saturating_sub(1)
    }

    fn find_current_visual_line(&self, lines: &[EditorVisualLine]) -> usize {
        self.find_visual_line_at(lines, self.state.cursor.line, self.state.cursor.col)
    }

    fn move_to_visual_line(&mut self, lines: &[EditorVisualLine], current: usize, target: usize) {
        let (Some(current_line), Some(target_line)) = (lines.get(current), lines.get(target))
        else {
            return;
        };
        let current_visual_col = if let Some(snapped) = self.snapped_from_cursor_col {
            let index = self.find_visual_line_at(lines, current_line.logical_line, snapped);
            snapped - lines[index].start_col
        } else {
            self.state.cursor.col - current_line.start_col
        };
        let current_is_last = lines
            .get(current + 1)
            .is_none_or(|next| next.logical_line != current_line.logical_line);
        let source_max = if current_is_last {
            current_line.length
        } else {
            current_line.length.saturating_sub(1)
        };
        let target_is_last = lines
            .get(target + 1)
            .is_none_or(|next| next.logical_line != target_line.logical_line);
        let target_max = if target_is_last {
            target_line.length
        } else {
            target_line.length.saturating_sub(1)
        };
        let visual_col = self.compute_vertical_column(current_visual_col, source_max, target_max);
        self.state.cursor.line = target_line.logical_line;
        let line_units = self.current_line_units();
        self.state.cursor.col = (target_line.start_col + visual_col).min(line_units);
        let current_text = self.current_line().to_owned();
        if let Some(span) = self.grapheme_spans(&current_text).into_iter().find(|span| {
            span.end - span.start > 1
                && span.start <= self.state.cursor.col
                && self.state.cursor.col < span.end
        }) {
            let is_continuation = span.start < target_line.start_col;
            let is_moving_down = target > current;
            if is_continuation && is_moving_down {
                let mut next = target + 1;
                while next < lines.len()
                    && lines[next].logical_line == target_line.logical_line
                    && lines[next].start_col < span.end
                {
                    next += 1;
                }
                if next < lines.len() {
                    self.move_to_visual_line(lines, current, next);
                    return;
                }
            }
            self.snapped_from_cursor_col = Some(self.state.cursor.col);
            self.state.cursor.col = span.start;
        } else {
            self.snapped_from_cursor_col = None;
        }
    }

    fn compute_vertical_column(
        &mut self,
        current_col: usize,
        source_max: usize,
        target_max: usize,
    ) -> usize {
        let cursor_in_middle = current_col < source_max;
        let target_too_short = target_max < current_col;
        if self.preferred_visual_col.is_none() || cursor_in_middle {
            if target_too_short {
                self.preferred_visual_col = Some(current_col);
                return target_max;
            }
            self.preferred_visual_col = None;
            return current_col;
        }
        let preferred = self.preferred_visual_col.expect("checked above");
        if target_too_short || target_max < preferred {
            return target_max;
        }
        self.preferred_visual_col = None;
        preferred
    }
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
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

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
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

fn paste_marker_id(text: &str) -> Option<u32> {
    PASTE_MARKER_FULL_RE
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

fn word_segments_with_pastes(
    text: &str,
    pastes: &BTreeMap<u32, String>,
    segmenter: Option<&EditorWordSegmenter>,
) -> Vec<WordSegment> {
    let segment = |value: &str| {
        segmenter.map_or_else(|| default_word_segments(value), |host| host.segment(value))
    };
    let mut result = Vec::new();
    let mut cursor = 0;
    for captures in PASTE_MARKER_RE.captures_iter(text) {
        let Some(id) = captures[1].parse::<u32>().ok() else {
            continue;
        };
        let Some(marker) = captures.get(0) else {
            continue;
        };
        if !pastes.contains_key(&id) {
            continue;
        }
        if marker.start() > cursor {
            result.extend(segment(&text[cursor..marker.start()]));
        }
        result.push(WordSegment {
            text: marker.as_str().to_owned(),
            is_word_like: false,
        });
        cursor = marker.end();
    }
    if cursor < text.len() {
        result.extend(segment(&text[cursor..]));
    }
    result
}

fn match_columns(line: &str, needle: &str) -> Vec<usize> {
    line.match_indices(needle)
        .map(|(byte, _)| byte_to_utf16(line, byte))
        .collect()
}

fn filtered_is_empty_after_paste(_input: &str, _decoded: &str, clean: &str) -> bool {
    clean
        .chars()
        .all(|character| character != '\n' && u32::from(character) < 32)
}
