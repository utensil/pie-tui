//! SelectList — filterable, scrollable command selection.
//!
//! Port of the pinned `components/select-list.js` implementation from
//! `@earendil-works/pi-tui@0.84.1`.

use pie_core::keybindings::global::get_keybindings;
use pie_core::text::visible_width;
use pie_core::wrap::truncate_to_width;

use crate::{Component, StyleFn};

const DEFAULT_PRIMARY_COLUMN_WIDTH: usize = 32;
const PRIMARY_COLUMN_GAP: usize = 2;
const MIN_DESCRIPTION_WIDTH: usize = 10;

/// One selectable row (reference `SelectItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
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

/// Styling callbacks used by [`SelectList`].
pub struct SelectListTheme {
    pub selected_prefix: StyleFn,
    pub selected_text: StyleFn,
    pub description: StyleFn,
    pub scroll_info: StyleFn,
    pub no_match: StyleFn,
}

/// Owned Rust adaptation of the reference `SelectListTruncateContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectListTruncateContext {
    pub text: String,
    pub max_width: usize,
    pub column_width: usize,
    pub item: SelectItem,
    pub is_selected: bool,
}

pub type TruncatePrimaryFn = Box<dyn FnMut(&SelectListTruncateContext) -> String + Send>;

/// Optional two-column layout controls.
#[derive(Default)]
pub struct SelectListLayoutOptions {
    pub min_primary_column_width: Option<usize>,
    pub max_primary_column_width: Option<usize>,
    pub truncate_primary: Option<TruncatePrimaryFn>,
}

type ItemCallback = Box<dyn FnMut(&SelectItem) + Send>;

/// Filterable, scrollable list with wrap-around keyboard navigation.
pub struct SelectList {
    items: Vec<SelectItem>,
    filtered_indices: Vec<usize>,
    selected_index: isize,
    max_visible: usize,
    theme: SelectListTheme,
    layout: SelectListLayoutOptions,
    on_select: Option<ItemCallback>,
    on_cancel: Option<Box<dyn FnMut() + Send>>,
    on_selection_change: Option<ItemCallback>,
}

impl SelectList {
    pub fn new(items: Vec<SelectItem>, max_visible: usize, theme: SelectListTheme) -> Self {
        Self::with_layout(
            items,
            max_visible,
            theme,
            SelectListLayoutOptions::default(),
        )
    }

    pub fn with_layout(
        items: Vec<SelectItem>,
        max_visible: usize,
        theme: SelectListTheme,
        layout: SelectListLayoutOptions,
    ) -> Self {
        let filtered_indices = (0..items.len()).collect();
        Self {
            items,
            filtered_indices,
            selected_index: 0,
            max_visible,
            theme,
            layout,
            on_select: None,
            on_cancel: None,
            on_selection_change: None,
        }
    }

    pub fn set_on_select(&mut self, callback: Option<ItemCallback>) {
        self.on_select = callback;
    }

    pub fn set_on_cancel(&mut self, callback: Option<Box<dyn FnMut() + Send>>) {
        self.on_cancel = callback;
    }

    pub fn set_on_selection_change(&mut self, callback: Option<ItemCallback>) {
        self.on_selection_change = callback;
    }

    /// Prefix filter over `item.value`, case-insensitive, resetting selection.
    pub fn set_filter(&mut self, filter: &str) {
        let lower = filter.to_lowercase();
        self.filtered_indices = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.value
                    .to_lowercase()
                    .starts_with(&lower)
                    .then_some(index)
            })
            .collect();
        self.selected_index = 0;
    }

    /// Clamp as the reference does, including its observable empty-list `-1`.
    pub fn set_selected_index(&mut self, index: isize) {
        let max = self.filtered_indices.len() as isize - 1;
        self.selected_index = 0.max(index.min(max));
    }

    pub fn selected_item(&self) -> Option<&SelectItem> {
        let selected = usize::try_from(self.selected_index).ok()?;
        let item_index = *self.filtered_indices.get(selected)?;
        self.items.get(item_index)
    }

    fn display_value(item: &SelectItem) -> &str {
        if item.label.is_empty() {
            &item.value
        } else {
            &item.label
        }
    }

    fn primary_column_bounds(&self) -> (usize, usize) {
        let raw_min = self
            .layout
            .min_primary_column_width
            .or(self.layout.max_primary_column_width)
            .unwrap_or(DEFAULT_PRIMARY_COLUMN_WIDTH);
        let raw_max = self
            .layout
            .max_primary_column_width
            .or(self.layout.min_primary_column_width)
            .unwrap_or(DEFAULT_PRIMARY_COLUMN_WIDTH);
        (raw_min.min(raw_max).max(1), raw_min.max(raw_max).max(1))
    }

    fn primary_column_width(&self) -> usize {
        let (min, max) = self.primary_column_bounds();
        let widest = self
            .filtered_indices
            .iter()
            .map(|index| {
                visible_width(Self::display_value(&self.items[*index])) + PRIMARY_COLUMN_GAP
            })
            .max()
            .unwrap_or(0);
        widest.clamp(min, max)
    }

    fn truncate_primary(
        &mut self,
        item: &SelectItem,
        is_selected: bool,
        max_width: usize,
        column_width: usize,
    ) -> String {
        let text = Self::display_value(item).to_string();
        let candidate = match &mut self.layout.truncate_primary {
            Some(callback) => callback(&SelectListTruncateContext {
                text,
                max_width,
                column_width,
                item: item.clone(),
                is_selected,
            }),
            None => text,
        };
        truncate_to_width(&candidate, max_width, "", false)
    }

    fn render_item(
        &mut self,
        item: &SelectItem,
        is_selected: bool,
        width: usize,
        description: Option<&str>,
        primary_column_width: usize,
    ) -> String {
        let prefix = if is_selected { "→ " } else { "  " };
        let prefix_width = visible_width(prefix);
        if let Some(description) = description.filter(|_| width > 40) {
            let effective_column = primary_column_width
                .min(width.saturating_sub(prefix_width + 4))
                .max(1);
            let max_primary = effective_column.saturating_sub(PRIMARY_COLUMN_GAP).max(1);
            let value = self.truncate_primary(item, is_selected, max_primary, effective_column);
            let value_width = visible_width(&value);
            let spacing = " ".repeat(effective_column.saturating_sub(value_width).max(1));
            let description_start = prefix_width + value_width + spacing.len();
            let remaining = width.saturating_sub(description_start + 2);
            if remaining > MIN_DESCRIPTION_WIDTH {
                let description = truncate_to_width(description, remaining, "", false);
                if is_selected {
                    return (self.theme.selected_text)(&format!(
                        "{prefix}{value}{spacing}{description}"
                    ));
                }
                let styled_description =
                    (self.theme.description)(&format!("{spacing}{description}"));
                return format!("{prefix}{value}{styled_description}");
            }
        }
        let max_width = width.saturating_sub(prefix_width + 2);
        let value = self.truncate_primary(item, is_selected, max_width, max_width);
        if is_selected {
            (self.theme.selected_text)(&format!("{prefix}{value}"))
        } else {
            format!("{prefix}{value}")
        }
    }

    fn notify_selection_change(&mut self) {
        let item = self.selected_item().cloned();
        if let (Some(item), Some(callback)) = (item.as_ref(), self.on_selection_change.as_mut()) {
            callback(item);
        }
    }
}

impl Component for SelectList {
    fn render(&mut self, width: usize) -> Vec<String> {
        if self.filtered_indices.is_empty() {
            return vec![(self.theme.no_match)("  No matching commands")];
        }
        let primary_column_width = self.primary_column_width();
        let half = self.max_visible / 2;
        let selected = usize::try_from(self.selected_index).unwrap_or(0);
        let max_start = self.filtered_indices.len().saturating_sub(self.max_visible);
        let start = selected.saturating_sub(half).min(max_start);
        let end = (start + self.max_visible).min(self.filtered_indices.len());
        let rows: Vec<(SelectItem, bool, Option<String>)> = (start..end)
            .filter_map(|position| {
                let item = self
                    .items
                    .get(*self.filtered_indices.get(position)?)?
                    .clone();
                let description = item
                    .description
                    .as_deref()
                    .map(normalize_to_single_line)
                    .filter(|description| !description.is_empty());
                Some((item, position == selected, description))
            })
            .collect();
        let mut lines = rows
            .iter()
            .map(|(item, is_selected, description)| {
                self.render_item(
                    item,
                    *is_selected,
                    width,
                    description.as_deref(),
                    primary_column_width,
                )
            })
            .collect::<Vec<_>>();
        if start > 0 || end < self.filtered_indices.len() {
            let scroll = format!("  ({}/{})", selected + 1, self.filtered_indices.len());
            let truncated = truncate_to_width(&scroll, width.saturating_sub(2), "", false);
            lines.push((self.theme.scroll_info)(&truncated));
        }
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            self.selected_index = if self.selected_index == 0 {
                self.filtered_indices.len() as isize - 1
            } else {
                self.selected_index - 1
            };
            self.notify_selection_change();
        } else if keybindings.matches(data, "tui.select.down") {
            self.selected_index = if self.selected_index == self.filtered_indices.len() as isize - 1
            {
                0
            } else {
                self.selected_index + 1
            };
            self.notify_selection_change();
        } else if keybindings.matches(data, "tui.select.confirm") {
            let item = self.selected_item().cloned();
            if let (Some(item), Some(callback)) = (item.as_ref(), self.on_select.as_mut()) {
                callback(item);
            }
        } else if keybindings.matches(data, "tui.select.cancel")
            && let Some(callback) = self.on_cancel.as_mut()
        {
            callback();
        }
    }
}

fn normalize_to_single_line(text: &str) -> String {
    text.split(['\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}
