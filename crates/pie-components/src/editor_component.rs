//! Object-safe contract for canonical and extension-provided editors.

use std::sync::Arc;

use crate::{AutocompleteProvider, Component, Editor, EditorTextCallback, SharedStyleFn};

/// Idiomatic Rust analogue of the reference `EditorComponent` interface.
/// Optional extension hooks default to no-ops or the primary text value.
pub trait EditorComponent: Component {
    fn get_text(&self) -> String;
    fn set_text(&mut self, text: String);
    fn set_on_submit(&mut self, callback: Option<EditorTextCallback>);
    fn set_on_change(&mut self, callback: Option<EditorTextCallback>);

    fn add_to_history(&mut self, _text: String) {}
    fn insert_text_at_cursor(&mut self, _text: String) {}
    fn get_expanded_text(&self) -> String {
        self.get_text()
    }
    fn set_autocomplete_provider(&mut self, _provider: Arc<dyn AutocompleteProvider>) {}
    fn set_border_color(&mut self, _border_color: SharedStyleFn) {}
    fn set_padding_x(&mut self, _padding: usize) {}
    fn set_autocomplete_max_visible(&mut self, _max_visible: usize) {}
}

impl EditorComponent for Editor {
    fn get_text(&self) -> String {
        Editor::get_text(self)
    }

    fn set_text(&mut self, text: String) {
        Editor::set_text(self, text);
    }

    fn set_on_submit(&mut self, callback: Option<EditorTextCallback>) {
        Editor::set_on_submit(self, callback);
    }

    fn set_on_change(&mut self, callback: Option<EditorTextCallback>) {
        Editor::set_on_change(self, callback);
    }

    fn add_to_history(&mut self, text: String) {
        Editor::add_to_history(self, text);
    }

    fn insert_text_at_cursor(&mut self, text: String) {
        Editor::insert_text_at_cursor(self, text);
    }

    fn get_expanded_text(&self) -> String {
        Editor::get_expanded_text(self)
    }

    fn set_autocomplete_provider(&mut self, provider: Arc<dyn AutocompleteProvider>) {
        Editor::set_autocomplete_provider(self, provider);
    }

    fn set_border_color(&mut self, border_color: SharedStyleFn) {
        Editor::set_border_color(self, border_color);
    }

    fn set_padding_x(&mut self, padding: usize) {
        Editor::set_padding_x(self, padding);
    }

    fn set_autocomplete_max_visible(&mut self, max_visible: usize) {
        Editor::set_autocomplete_max_visible(self, max_visible);
    }
}
