//! pie-core — pure TUI logic: text measurement, input events, layout, diff frames, themes.
//! Runtime dependency policy: no I/O of any kind (enforced by the boundary gate S1/S2 rules
//! plus review); golden fixtures under tests/fixtures/ are data-only.

pub mod placeholder {
    /// Removed once real modules land (M1+). Exists so the workspace compiles green from M0.
    pub fn scaffold() -> &'static str {
        "pie-core"
    }
}

pub mod editor_model;
pub mod frame;
pub mod fuzzy;
pub mod keybindings;
pub mod keys;
pub mod keys_tables;
pub mod kill_ring;
pub mod latex;
pub mod screen;
pub mod stdin_buffer;
pub mod terminal_colors;
pub mod terminal_image;
pub mod text;
pub mod undo_stack;
pub mod word_navigation;
pub mod wrap;
