//! Combined slash-command and filesystem autocomplete provider.
//!
//! Rust adaptation of the pinned `autocomplete.js` contract from
//! `@earendil-works/pi-tui@0.84.1`. Cursor columns use JavaScript UTF-16 code
//! units, argument providers are awaitable, and cancellation remains live for
//! the full request.

use std::cmp::{Ordering, Reverse};
use std::fs;
use std::future::Future;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

use pie_core::fuzzy::fuzzy_filter;

use crate::cancellation::CancellationSignal;

const PATH_DELIMITERS: &[char] = &[' ', '\t', '"', '\'', '='];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl AutocompleteItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    pub fn with_description(
        value: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: Some(description.into()),
        }
    }
}

pub type ArgumentCompletionFuture =
    Pin<Box<dyn Future<Output = Option<Vec<AutocompleteItem>>> + Send + 'static>>;

/// Rust representation of the reference `Awaitable<T>` callback result.
pub enum ArgumentCompletionResult {
    Ready(Option<Vec<AutocompleteItem>>),
    Future(ArgumentCompletionFuture),
}

impl ArgumentCompletionResult {
    pub fn ready(items: Option<Vec<AutocompleteItem>>) -> Self {
        Self::Ready(items)
    }

    pub fn future(
        future: impl Future<Output = Option<Vec<AutocompleteItem>>> + Send + 'static,
    ) -> Self {
        Self::Future(Box::pin(future))
    }

    async fn resolve(self) -> Option<Vec<AutocompleteItem>> {
        match self {
            Self::Ready(items) => items,
            Self::Future(future) => future.await,
        }
    }
}

impl From<Option<Vec<AutocompleteItem>>> for ArgumentCompletionResult {
    fn from(items: Option<Vec<AutocompleteItem>>) -> Self {
        Self::Ready(items)
    }
}

pub type ArgumentCompletionFn = Box<dyn Fn(String) -> ArgumentCompletionResult + Send + Sync>;

pub struct SlashCommand {
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub get_argument_completions: Option<ArgumentCompletionFn>,
}

impl SlashCommand {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            argument_hint: None,
            get_argument_completions: None,
        }
    }
}

pub enum AutocompleteCommand {
    Item(AutocompleteItem),
    Slash(SlashCommand),
}

impl From<AutocompleteItem> for AutocompleteCommand {
    fn from(value: AutocompleteItem) -> Self {
        Self::Item(value)
    }
}

impl From<SlashCommand> for AutocompleteCommand {
    fn from(value: SlashCommand) -> Self {
        Self::Slash(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteSuggestions {
    pub items: Vec<AutocompleteItem>,
    pub prefix: String,
}

#[derive(Clone, Default)]
pub struct AutocompleteOptions {
    pub force: bool,
    pub signal: CancellationSignal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResult {
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

pub type AutocompleteSuggestionsFuture<'a> =
    Pin<Box<dyn Future<Output = Option<AutocompleteSuggestions>> + Send + 'a>>;

/// Rust adaptation of the reference async provider interface.
pub trait AutocompleteProvider: Send + Sync {
    fn trigger_characters(&self) -> Option<&[String]> {
        None
    }

    fn get_suggestions<'a>(
        &'a self,
        lines: &'a [String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteSuggestionsFuture<'a>;

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult;

    fn should_trigger_file_completion(
        &self,
        _lines: &[String],
        _cursor_line: usize,
        _cursor_col: usize,
    ) -> bool {
        true
    }
}

pub struct CombinedAutocompleteProvider {
    commands: Vec<AutocompleteCommand>,
    base_path: PathBuf,
    fd_path: Option<PathBuf>,
}

impl AutocompleteProvider for CombinedAutocompleteProvider {
    fn get_suggestions<'a>(
        &'a self,
        lines: &'a [String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteSuggestionsFuture<'a> {
        CombinedAutocompleteProvider::get_suggestions(self, lines, cursor_line, cursor_col, options)
    }

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        CombinedAutocompleteProvider::apply_completion(
            self,
            lines,
            cursor_line,
            cursor_col,
            item,
            prefix,
        )
    }

    fn should_trigger_file_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) -> bool {
        CombinedAutocompleteProvider::should_trigger_file_completion(
            self,
            lines,
            cursor_line,
            cursor_col,
        )
    }
}

impl CombinedAutocompleteProvider {
    pub fn new(
        commands: Vec<AutocompleteCommand>,
        base_path: impl Into<PathBuf>,
        fd_path: Option<PathBuf>,
    ) -> Self {
        Self {
            commands,
            base_path: base_path.into(),
            fd_path,
        }
    }

    pub fn get_suggestions<'a>(
        &'a self,
        lines: &'a [String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteSuggestionsFuture<'a> {
        Box::pin(async move {
            self.get_suggestions_inner(lines, cursor_line, cursor_col, options)
                .await
        })
    }

    async fn get_suggestions_inner(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> Option<AutocompleteSuggestions> {
        let current_line = lines.get(cursor_line).map(String::as_str).unwrap_or("");
        let text_before_cursor = prefix_at_utf16(current_line, cursor_col);
        if let Some(at_prefix) = extract_at_prefix(text_before_cursor) {
            let parsed = parse_path_prefix(&at_prefix);
            let suggestions = self
                .get_fuzzy_file_suggestions(
                    &parsed.raw_prefix,
                    parsed.is_quoted_prefix,
                    options.signal.clone(),
                )
                .await;
            if suggestions.is_empty() {
                return None;
            }
            return Some(AutocompleteSuggestions {
                items: suggestions,
                prefix: at_prefix,
            });
        }
        if !options.force && text_before_cursor.starts_with('/') {
            if let Some(space_index) = text_before_cursor.find(' ') {
                let command_name = &text_before_cursor[1..space_index];
                let argument_text = &text_before_cursor[space_index + 1..];
                let command = self.commands.iter().find_map(|command| match command {
                    AutocompleteCommand::Slash(command) if command.name == command_name => {
                        Some(command)
                    }
                    _ => None,
                })?;
                let suggestions = command
                    .get_argument_completions
                    .as_ref()
                    .map(|callback| callback(argument_text.to_string()))?
                    .resolve()
                    .await?;
                if suggestions.is_empty() {
                    return None;
                }
                return Some(AutocompleteSuggestions {
                    items: suggestions,
                    prefix: argument_text.to_string(),
                });
            }
            let prefix = &text_before_cursor[1..];
            let command_items = self
                .commands
                .iter()
                .map(|command| {
                    let (name, description, hint) = match command {
                        AutocompleteCommand::Item(item) => {
                            (&item.value, item.description.as_deref(), None)
                        }
                        AutocompleteCommand::Slash(command) => (
                            &command.name,
                            command.description.as_deref(),
                            command.argument_hint.as_deref(),
                        ),
                    };
                    let description = match (hint, description) {
                        (Some(hint), Some(description)) if !description.is_empty() => {
                            Some(format!("{hint} — {description}"))
                        }
                        (Some(hint), _) => Some(hint.to_string()),
                        (None, Some(description)) if !description.is_empty() => {
                            Some(description.to_string())
                        }
                        _ => None,
                    };
                    AutocompleteItem {
                        value: name.to_string(),
                        label: name.to_string(),
                        description,
                    }
                })
                .collect::<Vec<_>>();
            let items = fuzzy_filter(&command_items, prefix, |item| item.value.clone())
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            if items.is_empty() {
                return None;
            }
            return Some(AutocompleteSuggestions {
                items,
                prefix: text_before_cursor.to_string(),
            });
        }
        let path_prefix = extract_path_prefix(text_before_cursor, options.force)?;
        let suggestions = self.get_file_suggestions(&path_prefix);
        if suggestions.is_empty() {
            None
        } else {
            Some(AutocompleteSuggestions {
                items: suggestions,
                prefix: path_prefix,
            })
        }
    }

    pub fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        let current_line = lines.get(cursor_line).map(String::as_str).unwrap_or("");
        let cursor_col = cursor_col.min(utf16_len(current_line));
        let cursor_byte = utf16_to_byte_index(current_line, cursor_col);
        let prefix_start = cursor_col.saturating_sub(utf16_len(prefix));
        let prefix_start_byte = utf16_to_byte_index(current_line, prefix_start);
        let before_prefix = &current_line[..prefix_start_byte];
        let after_cursor = &current_line[cursor_byte..];
        let is_quoted_prefix = prefix.starts_with('"') || prefix.starts_with("@\"");
        let adjusted_after =
            if is_quoted_prefix && item.value.ends_with('"') && after_cursor.starts_with('"') {
                &after_cursor[1..]
            } else {
                after_cursor
            };
        let is_slash_command = prefix.starts_with('/')
            && before_prefix.trim().is_empty()
            && !prefix[1..].contains('/');
        let (new_line, new_col) = if is_slash_command {
            (
                format!("{before_prefix}/{} {adjusted_after}", item.value),
                utf16_len(before_prefix) + utf16_len(&item.value) + 2,
            )
        } else if prefix.starts_with('@') {
            let is_directory = item.label.ends_with('/');
            let suffix = if is_directory { "" } else { " " };
            let cursor_offset = completion_cursor_offset(item, is_directory);
            (
                format!("{before_prefix}{}{suffix}{adjusted_after}", item.value),
                utf16_len(before_prefix) + cursor_offset + utf16_len(suffix),
            )
        } else {
            let is_directory = item.label.ends_with('/');
            let cursor_offset = completion_cursor_offset(item, is_directory);
            (
                format!("{before_prefix}{}{adjusted_after}", item.value),
                utf16_len(before_prefix) + cursor_offset,
            )
        };
        let mut new_lines = lines.to_vec();
        if cursor_line < new_lines.len() {
            new_lines[cursor_line] = new_line;
        } else {
            while new_lines.len() < cursor_line {
                new_lines.push(String::new());
            }
            new_lines.push(new_line);
        }
        CompletionResult {
            lines: new_lines,
            cursor_line,
            cursor_col: new_col,
        }
    }

    pub fn should_trigger_file_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) -> bool {
        let current_line = lines.get(cursor_line).map(String::as_str).unwrap_or("");
        let before = prefix_at_utf16(current_line, cursor_col).trim();
        !before.starts_with('/') || before.contains(' ')
    }

    fn get_file_suggestions(&self, prefix: &str) -> Vec<AutocompleteItem> {
        let parsed = parse_path_prefix(prefix);
        let raw = parsed.raw_prefix.as_str();
        let expanded = expand_home_path(raw);
        let is_root = raw.is_empty()
            || matches!(raw, "./" | "../" | "~" | "~/" | "/")
            || (parsed.is_at_prefix && raw.is_empty());
        let (search_dir, search_prefix) = if is_root || raw.ends_with('/') {
            let dir = if raw.starts_with('~') || expanded.starts_with('/') {
                PathBuf::from(&expanded)
            } else {
                self.base_path.join(&expanded)
            };
            (dir, String::new())
        } else {
            let expanded_path = Path::new(&expanded);
            let dir = expanded_path.parent().unwrap_or_else(|| Path::new(""));
            let file = expanded_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            let search_dir = if raw.starts_with('~') || expanded.starts_with('/') {
                dir.to_path_buf()
            } else {
                self.base_path.join(dir)
            };
            (search_dir, file)
        };
        let Ok(entries) = fs::read_dir(search_dir) else {
            return Vec::new();
        };
        let mut suggestions = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name
                .to_lowercase()
                .starts_with(&search_prefix.to_lowercase())
            {
                continue;
            }
            let is_directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                || entry
                    .metadata()
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false);
            let mut relative = if raw.ends_with('/') {
                format!("{raw}{name}")
            } else if raw.contains('/') || raw.contains('\\') {
                if let Some(home_relative) = raw.strip_prefix("~/") {
                    match Path::new(home_relative).parent().and_then(Path::to_str) {
                        Some("") | Some(".") | None => format!("~/{name}"),
                        Some(parent) => format!("~/{parent}/{name}"),
                    }
                } else if raw.starts_with('/') {
                    let parent = Path::new(raw)
                        .parent()
                        .and_then(Path::to_str)
                        .unwrap_or("/");
                    if parent == "/" {
                        format!("/{name}")
                    } else {
                        format!("{parent}/{name}")
                    }
                } else {
                    let parent = Path::new(raw).parent().and_then(Path::to_str).unwrap_or("");
                    let joined = if parent.is_empty() || parent == "." {
                        name.clone()
                    } else {
                        format!("{parent}/{name}")
                    };
                    if raw.starts_with("./") && !joined.starts_with("./") {
                        format!("./{joined}")
                    } else {
                        joined
                    }
                }
            } else if raw.starts_with('~') {
                format!("~/{name}")
            } else {
                name.clone()
            };
            relative = to_display_path(&relative);
            if is_directory {
                relative.push('/');
            }
            suggestions.push(AutocompleteItem {
                value: build_completion_value(
                    &relative,
                    parsed.is_at_prefix,
                    parsed.is_quoted_prefix,
                ),
                label: format!("{name}{}", if is_directory { "/" } else { "" }),
                description: None,
            });
        }
        suggestions.sort_by(|a, b| {
            let a_dir = a.value.ends_with('/');
            let b_dir = b.value.ends_with('/');
            b_dir
                .cmp(&a_dir)
                .then_with(|| reference_locale_cmp(&a.label, &b.label))
        });
        suggestions
    }

    async fn get_fuzzy_file_suggestions(
        &self,
        query: &str,
        is_quoted_prefix: bool,
        signal: CancellationSignal,
    ) -> Vec<AutocompleteItem> {
        let Some(fd_path) = self.fd_path.as_ref() else {
            return Vec::new();
        };
        if signal.aborted() {
            return Vec::new();
        }
        let (base_dir, fd_query, display_base) = self
            .resolve_scoped_fuzzy_query(query)
            .unwrap_or_else(|| (self.base_path.clone(), query.to_string(), None));
        let worker_fd_path = fd_path.clone();
        let worker_base_dir = base_dir.clone();
        let worker_query = fd_query.clone();
        let worker_signal = signal.clone();
        let entries = spawn_blocking(move || {
            walk_directory_with_fd(
                &worker_fd_path,
                &worker_base_dir,
                &worker_query,
                &worker_signal,
            )
        })
        .await;
        if signal.aborted() {
            return Vec::new();
        }
        let mut entries = entries
            .into_iter()
            .filter_map(|(path, is_directory)| {
                let score = if fd_query.is_empty() {
                    1
                } else {
                    score_entry(&path, &fd_query, is_directory)
                };
                (score > 0).then_some((path, is_directory, score))
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| Reverse(entry.2));
        entries
            .into_iter()
            .take(20)
            .map(|(path, is_directory, _)| {
                let path_without_slash = path.trim_end_matches('/');
                let display = display_base
                    .as_deref()
                    .map(|base| scoped_path_for_display(base, path_without_slash))
                    .unwrap_or_else(|| path_without_slash.to_string());
                let name = display.rsplit('/').next().unwrap_or(&display).to_string();
                let completion_path = format!("{display}{}", if is_directory { "/" } else { "" });
                AutocompleteItem {
                    value: build_completion_value(&completion_path, true, is_quoted_prefix),
                    label: format!("{name}{}", if is_directory { "/" } else { "" }),
                    description: Some(display),
                }
            })
            .collect()
    }

    fn resolve_scoped_fuzzy_query(
        &self,
        raw_query: &str,
    ) -> Option<(PathBuf, String, Option<String>)> {
        let normalized = to_display_path(raw_query);
        let slash = normalized.rfind('/')?;
        let display_base = normalized[..=slash].to_string();
        let query = normalized[slash + 1..].to_string();
        let base_dir = if display_base.starts_with("~/") {
            PathBuf::from(expand_home_path(&display_base))
        } else if display_base.starts_with('/') {
            PathBuf::from(&display_base)
        } else {
            self.base_path.join(&display_base)
        };
        base_dir
            .is_dir()
            .then_some((base_dir, query, Some(display_base)))
    }
}

struct BlockingState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

struct BlockingFuture<T> {
    state: Arc<Mutex<BlockingState<T>>>,
}

impl<T> Future for BlockingFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().expect("blocking autocomplete state");
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

fn spawn_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> BlockingFuture<T> {
    let state = Arc::new(Mutex::new(BlockingState {
        result: None,
        waker: None,
    }));
    let worker_state = state.clone();
    thread::spawn(move || {
        let result = work();
        let waker = {
            let mut state = worker_state.lock().expect("blocking autocomplete state");
            state.result = Some(result);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    });
    BlockingFuture { state }
}

fn walk_directory_with_fd(
    fd_path: &Path,
    base_dir: &Path,
    query: &str,
    signal: &CancellationSignal,
) -> Vec<(String, bool)> {
    if signal.aborted() {
        return Vec::new();
    }

    let mut command = Command::new(fd_path);
    command
        .args([
            "--base-directory",
            base_dir.to_str().unwrap_or(""),
            "--max-results",
            "100",
            "--type",
            "f",
            "--type",
            "d",
            "--follow",
            "--hidden",
            "--exclude",
            ".git",
            "--exclude",
            ".git/*",
            "--exclude",
            ".git/**",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if to_display_path(query).contains('/') {
        command.arg("--full-path");
    }
    if !query.is_empty() {
        command.arg(build_fd_path_query(query));
    }
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let stdout_reader = child.stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
    });
    let stderr_reader = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
    });

    let status = loop {
        if signal.aborted() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(reader) = stdout_reader {
                let _ = reader.join();
            }
            if let Some(reader) = stderr_reader {
                let _ = reader.join();
            }
            return Vec::new();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(reader) = stdout_reader {
                    let _ = reader.join();
                }
                if let Some(reader) = stderr_reader {
                    let _ = reader.join();
                }
                return Vec::new();
            }
        }
    };

    let stdout = stdout_reader
        .and_then(|reader| reader.join().ok())
        .and_then(Result::ok)
        .unwrap_or_default();
    if let Some(reader) = stderr_reader {
        let _ = reader.join();
    }
    if signal.aborted() || !status.success() || stdout.is_empty() {
        return Vec::new();
    }

    String::from_utf8_lossy(&stdout)
        .trim()
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let path = to_display_path(line);
            let is_directory = path.ends_with('/');
            let normalized = path.trim_end_matches('/');
            if normalized == ".git"
                || normalized.starts_with(".git/")
                || normalized.contains("/.git/")
            {
                None
            } else {
                Some((path, is_directory))
            }
        })
        .collect()
}

fn build_fd_path_query(query: &str) -> String {
    let normalized = to_display_path(query);
    if !normalized.contains('/') {
        return normalized;
    }
    let has_trailing_separator = normalized.ends_with('/');
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return normalized;
    }
    let separator_pattern = "[\\\\/]";
    let segments = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(escape_fd_regex)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return normalized;
    }
    let mut pattern = segments.join(separator_pattern);
    if has_trailing_separator {
        pattern.push_str(separator_pattern);
    }
    pattern
}

fn escape_fd_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(
            ch,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn utf16_to_byte_index(text: &str, utf16_index: usize) -> usize {
    let mut consumed = 0;
    for (byte, ch) in text.char_indices() {
        let next = consumed + ch.len_utf16();
        if next > utf16_index {
            return byte;
        }
        consumed = next;
    }
    text.len()
}

fn prefix_at_utf16(text: &str, utf16_index: usize) -> &str {
    &text[..utf16_to_byte_index(text, utf16_index)]
}

fn completion_cursor_offset(item: &AutocompleteItem, is_directory: bool) -> usize {
    if is_directory && item.value.ends_with('"') {
        utf16_len(&item.value).saturating_sub(1)
    } else {
        utf16_len(&item.value)
    }
}

/// The JS oracle delegates filename ordering to host `Intl.Collator` through
/// `localeCompare`. Rust has no locale collation in `std`, so this stable key
/// pins the observed en-US primary/accent/case policy used by the 0.84.1
/// fixture instead of inheriting a machine locale. Non-Latin characters fall
/// back to Unicode scalar order after case folding.
fn reference_locale_cmp(left: &str, right: &str) -> Ordering {
    locale_key(left)
        .cmp(&locale_key(right))
        .then_with(|| left.cmp(right))
}

fn locale_key(value: &str) -> (Vec<u32>, Vec<u8>, Vec<u8>) {
    let mut primary = Vec::new();
    let mut accents = Vec::new();
    let mut case = Vec::new();
    for ch in value.chars() {
        let (base, accent) = latin_collation_element(ch);
        let folded = base.to_lowercase().next().unwrap_or(base);
        let weight = match folded {
            ' ' | '\t' | '\n' | '\r' => 1,
            '_' => 2,
            '-' => 3,
            '.' => 4,
            '@' => 5,
            '\u{1f000}'..='\u{1faff}' => 0x10,
            '0'..='9' => 0x20 + u32::from(folded) - u32::from('0'),
            'a'..='z' => 0x100 + u32::from(folded) - u32::from('a'),
            _ => 0x1_0000 + u32::from(folded),
        };
        primary.push(weight);
        accents.push(accent);
        case.push(u8::from(ch.is_uppercase()));
    }
    (primary, accents, case)
}

fn latin_collation_element(ch: char) -> (char, u8) {
    match ch {
        'á' | 'Á' | 'é' | 'É' | 'í' | 'Í' | 'ó' | 'Ó' | 'ú' | 'Ú' | 'ý' | 'Ý' => {
            (strip_latin_accent(ch), 1)
        }
        'à' | 'À' | 'è' | 'È' | 'ì' | 'Ì' | 'ò' | 'Ò' | 'ù' | 'Ù' => {
            (strip_latin_accent(ch), 2)
        }
        'â' | 'Â' | 'ê' | 'Ê' | 'î' | 'Î' | 'ô' | 'Ô' | 'û' | 'Û' => {
            (strip_latin_accent(ch), 3)
        }
        'ã' | 'Ã' | 'ñ' | 'Ñ' | 'õ' | 'Õ' => (strip_latin_accent(ch), 4),
        'ä' | 'Ä' | 'ë' | 'Ë' | 'ï' | 'Ï' | 'ö' | 'Ö' | 'ü' | 'Ü' | 'ÿ' | 'Ÿ' => {
            (strip_latin_accent(ch), 5)
        }
        'å' | 'Å' => (strip_latin_accent(ch), 6),
        'ç' | 'Ç' => (strip_latin_accent(ch), 7),
        _ => (ch, 0),
    }
}

fn strip_latin_accent(ch: char) -> char {
    match ch {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'ç' => 'c',
        'Ç' => 'C',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ý' | 'ÿ' => 'y',
        'Ý' | 'Ÿ' => 'Y',
        _ => ch,
    }
}

fn to_display_path(value: &str) -> String {
    value.replace('\\', "/")
}

fn find_last_delimiter(text: &str) -> Option<usize> {
    text.char_indices()
        .rev()
        .find_map(|(index, ch)| PATH_DELIMITERS.contains(&ch).then_some(index))
}

fn find_unclosed_quote_start(text: &str) -> Option<usize> {
    let mut open = None;
    for (index, ch) in text.char_indices() {
        if ch == '"' {
            open = if open.is_some() { None } else { Some(index) };
        }
    }
    open
}

fn is_token_start(text: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }
    text[..index]
        .chars()
        .next_back()
        .is_some_and(|ch| PATH_DELIMITERS.contains(&ch))
}

fn extract_quoted_prefix(text: &str) -> Option<String> {
    let quote = find_unclosed_quote_start(text)?;
    if quote > 0 && text.as_bytes().get(quote - 1) == Some(&b'@') {
        if !is_token_start(text, quote - 1) {
            return None;
        }
        return Some(text[quote - 1..].to_string());
    }
    is_token_start(text, quote).then(|| text[quote..].to_string())
}

fn extract_at_prefix(text: &str) -> Option<String> {
    if let Some(quoted) = extract_quoted_prefix(text)
        && quoted.starts_with("@\"")
    {
        return Some(quoted);
    }
    let start = find_last_delimiter(text).map_or(0, |index| index + 1);
    (text.as_bytes().get(start) == Some(&b'@')).then(|| text[start..].to_string())
}

fn extract_path_prefix(text: &str, force: bool) -> Option<String> {
    if let Some(quoted) = extract_quoted_prefix(text) {
        return Some(quoted);
    }
    let start = find_last_delimiter(text).map_or(0, |index| index + 1);
    let prefix = &text[start..];
    if force
        || prefix.contains('/')
        || prefix.starts_with('.')
        || prefix.starts_with("~/")
        || (prefix.is_empty() && text.ends_with(' '))
    {
        Some(prefix.to_string())
    } else {
        None
    }
}

struct ParsedPathPrefix {
    raw_prefix: String,
    is_at_prefix: bool,
    is_quoted_prefix: bool,
}

fn parse_path_prefix(prefix: &str) -> ParsedPathPrefix {
    if let Some(raw) = prefix.strip_prefix("@\"") {
        ParsedPathPrefix {
            raw_prefix: raw.to_string(),
            is_at_prefix: true,
            is_quoted_prefix: true,
        }
    } else if let Some(raw) = prefix.strip_prefix('"') {
        ParsedPathPrefix {
            raw_prefix: raw.to_string(),
            is_at_prefix: false,
            is_quoted_prefix: true,
        }
    } else if let Some(raw) = prefix.strip_prefix('@') {
        ParsedPathPrefix {
            raw_prefix: raw.to_string(),
            is_at_prefix: true,
            is_quoted_prefix: false,
        }
    } else {
        ParsedPathPrefix {
            raw_prefix: prefix.to_string(),
            is_at_prefix: false,
            is_quoted_prefix: false,
        }
    }
}

fn build_completion_value(path: &str, is_at_prefix: bool, is_quoted_prefix: bool) -> String {
    let prefix = if is_at_prefix { "@" } else { "" };
    if is_quoted_prefix || path.contains(' ') {
        format!("{prefix}\"{path}\"")
    } else {
        format!("{prefix}{path}")
    }
}

fn expand_home_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if path == "~" {
        home
    } else if let Some(rest) = path.strip_prefix("~/") {
        let mut expanded = PathBuf::from(home)
            .join(rest)
            .to_string_lossy()
            .into_owned();
        if path.ends_with('/') && !expanded.ends_with('/') {
            expanded.push('/');
        }
        expanded
    } else {
        path.to_string()
    }
}

fn score_entry(file_path: &str, query: &str, is_directory: bool) -> i32 {
    let file_name = file_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(file_path)
        .to_lowercase();
    let query = query.to_lowercase();
    let path = file_path.to_lowercase();
    let mut score = if file_name == query {
        100
    } else if file_name.starts_with(&query) {
        80
    } else if file_name.contains(&query) {
        50
    } else if path.contains(&query) {
        30
    } else {
        0
    };
    if is_directory && score > 0 {
        score += 10;
    }
    score
}

fn scoped_path_for_display(display_base: &str, relative: &str) -> String {
    if display_base == "/" {
        format!("/{relative}")
    } else {
        format!("{display_base}{relative}")
    }
}
