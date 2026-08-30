//! Keybindings — port of reference `keybindings.js`.
//!
//! Central table of action ids with default keys, user re-binding support,
//! conflict detection, and a process-global manager (reference
//! `getKeybindings()` singleton).

use crate::keys::matches_key;

/// One keybinding definition (reference entry in `TUI_KEYBINDINGS`).
#[derive(Debug, Clone)]
pub struct KeybindingDef {
    pub id: &'static str,
    pub default_keys: &'static [&'static str],
    pub default_shape: KeybindingShape,
    pub description: &'static str,
}

/// JavaScript preserves whether a binding value was a scalar key id or an
/// array. The distinction is observable through `getResolvedBindings()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindingShape {
    Scalar,
    List,
}

/// A resolved binding with the exact scalar-versus-list shape exposed by the
/// reference manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBinding {
    Scalar(String),
    List(Vec<String>),
}

macro_rules! def {
    ($id:expr, scalar $key:expr, $desc:expr) => {
        KeybindingDef {
            id: $id,
            default_keys: &[$key],
            default_shape: KeybindingShape::Scalar,
            description: $desc,
        }
    };
    ($id:expr, $keys:expr, $desc:expr) => {
        KeybindingDef {
            id: $id,
            default_keys: $keys,
            default_shape: KeybindingShape::List,
            description: $desc,
        }
    };
}

/// The reference `TUI_KEYBINDINGS` table, in the exact original order.
pub static TUI_KEYBINDINGS: &[KeybindingDef] = &[
    def!("tui.editor.cursorUp", scalar "up", "Move cursor up"),
    def!("tui.editor.cursorDown", scalar "down", "Move cursor down"),
    def!(
        "tui.editor.historyPrevious",
        &[],
        "Select previous prompt history entry"
    ),
    def!(
        "tui.editor.historyNext",
        &[],
        "Select next prompt history entry"
    ),
    def!(
        "tui.editor.cursorLeft",
        &["left", "ctrl+b"],
        "Move cursor left"
    ),
    def!(
        "tui.editor.cursorRight",
        &["right", "ctrl+f"],
        "Move cursor right"
    ),
    def!(
        "tui.editor.cursorWordLeft",
        &["alt+left", "ctrl+left", "alt+b"],
        "Move cursor word left"
    ),
    def!(
        "tui.editor.cursorWordRight",
        &["alt+right", "ctrl+right", "alt+f"],
        "Move cursor word right"
    ),
    def!(
        "tui.editor.cursorLineStart",
        &["home", "ctrl+home", "ctrl+a"],
        "Move to line start"
    ),
    def!(
        "tui.editor.cursorLineEnd",
        &["end", "ctrl+end", "ctrl+e"],
        "Move to line end"
    ),
    def!(
        "tui.editor.jumpForward",
        scalar "ctrl+]",
        "Jump forward to character"
    ),
    def!(
        "tui.editor.jumpBackward",
        scalar "ctrl+alt+]",
        "Jump backward to character"
    ),
    def!("tui.editor.pageUp", &["pageUp", "ctrl+pageUp"], "Page up"),
    def!(
        "tui.editor.pageDown",
        &["pageDown", "ctrl+pageDown"],
        "Page down"
    ),
    def!(
        "tui.editor.deleteCharBackward",
        scalar "backspace",
        "Delete character backward"
    ),
    def!(
        "tui.editor.deleteCharForward",
        &["delete", "ctrl+d"],
        "Delete character forward"
    ),
    def!(
        "tui.editor.deleteWordBackward",
        &["ctrl+w", "alt+backspace"],
        "Delete word backward"
    ),
    def!(
        "tui.editor.deleteWordForward",
        &["alt+d", "alt+delete"],
        "Delete word forward"
    ),
    def!(
        "tui.editor.deleteToLineStart",
        scalar "ctrl+u",
        "Delete to line start"
    ),
    def!(
        "tui.editor.deleteToLineEnd",
        scalar "ctrl+k",
        "Delete to line end"
    ),
    def!("tui.editor.yank", scalar "ctrl+y", "Yank"),
    def!("tui.editor.yankPop", scalar "alt+y", "Yank pop"),
    def!("tui.editor.undo", scalar "ctrl+-", "Undo"),
    def!(
        "tui.input.newLine",
        &["shift+enter", "ctrl+j"],
        "Insert newline"
    ),
    def!("tui.input.submit", scalar "enter", "Submit input"),
    def!("tui.input.tab", scalar "tab", "Tab / autocomplete"),
    def!("tui.input.copy", scalar "ctrl+c", "Copy selection"),
    def!("tui.select.up", scalar "up", "Move selection up"),
    def!("tui.select.down", scalar "down", "Move selection down"),
    def!(
        "tui.select.pageUp",
        scalar "pageUp",
        "Selection page up"
    ),
    def!(
        "tui.select.pageDown",
        scalar "pageDown",
        "Selection page down"
    ),
    def!(
        "tui.select.confirm",
        scalar "enter",
        "Confirm selection"
    ),
    def!(
        "tui.select.cancel",
        &["escape", "ctrl+c"],
        "Cancel selection"
    ),
    // These intentionally shadow the unmodified editor bindings in fullscreen mode.
    def!(
        "tui.altScreen.pageUp",
        scalar "pageUp",
        "Scroll viewport up one page"
    ),
    def!(
        "tui.altScreen.pageDown",
        scalar "pageDown",
        "Scroll viewport down one page"
    ),
    def!(
        "tui.altScreen.halfPageUp",
        &[],
        "Scroll viewport up half a page"
    ),
    def!(
        "tui.altScreen.halfPageDown",
        &[],
        "Scroll viewport down half a page"
    ),
    def!("tui.altScreen.lineUp", &[], "Scroll viewport up one line"),
    def!(
        "tui.altScreen.lineDown",
        &[],
        "Scroll viewport down one line"
    ),
    def!(
        "tui.altScreen.previousPrompt",
        scalar "ctrl+shift+up",
        "Jump to previous semantic prompt"
    ),
    def!(
        "tui.altScreen.nextPrompt",
        scalar "ctrl+shift+down",
        "Jump to next semantic prompt"
    ),
    def!(
        "tui.altScreen.search",
        scalar "ctrl+shift+f",
        "Search the primary scroll view"
    ),
    def!(
        "tui.altScreen.searchNext",
        &["enter", "ctrl+g"],
        "Select the next search match"
    ),
    def!(
        "tui.altScreen.searchPrevious",
        &["shift+enter", "ctrl+shift+g"],
        "Select the previous search match"
    ),
    def!(
        "tui.altScreen.searchClose",
        scalar "escape",
        "Close transcript search"
    ),
    def!(
        "tui.altScreen.top",
        scalar "home",
        "Scroll viewport to top"
    ),
    def!(
        "tui.altScreen.bottom",
        scalar "end",
        "Scroll viewport to bottom"
    ),
];

/// A key conflict after user re-binding (reference `conflicts` entries).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyConflict {
    pub key: String,
    pub keybindings: Vec<String>,
}

/// Manager holding resolved key lists per action id (reference
/// `KeybindingsManager`). User bindings replace defaults entirely; a key
/// claimed by more than one user-rebound action is reported as a conflict.
pub struct KeybindingsManager {
    definitions: Vec<KeybindingDef>,
    user_bindings: Vec<(String, Vec<String>)>,
    keys_by_id: Vec<(String, Vec<String>)>,
    conflicts: Vec<KeyConflict>,
}

/// Reference `normalizeKeys`: undefined -> [], single -> [x], array ->
/// deduplicated copy (first occurrence wins).
fn normalize_keys(keys: Option<&Vec<String>>) -> Vec<String> {
    let Some(keys) = keys else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for key in keys {
        if seen.insert(key.clone()) {
            result.push(key.clone());
        }
    }
    result
}

impl KeybindingsManager {
    pub fn new(definitions: Vec<KeybindingDef>, user_bindings: Vec<(String, Vec<String>)>) -> Self {
        let mut mgr = KeybindingsManager {
            definitions,
            user_bindings,
            keys_by_id: Vec::new(),
            conflicts: Vec::new(),
        };
        mgr.rebuild();
        mgr
    }

    /// Manager over the reference `TUI_KEYBINDINGS` table.
    pub fn with_tui_defaults(user_bindings: Vec<(String, Vec<String>)>) -> Self {
        Self::new(TUI_KEYBINDINGS.to_vec(), user_bindings)
    }

    pub fn rebuild(&mut self) {
        self.keys_by_id.clear();
        self.conflicts.clear();
        // Ordered user claims: key -> claimant ids in insertion order.
        let mut user_claims: Vec<(String, Vec<String>)> = Vec::new();
        for (keybinding, keys) in &self.user_bindings {
            if !self.definitions.iter().any(|d| d.id == keybinding) {
                continue;
            }
            for key in normalize_keys(Some(keys)) {
                match user_claims.iter_mut().find(|(k, _)| k == &key) {
                    Some((_, claimants)) => {
                        if !claimants.contains(keybinding) {
                            claimants.push(keybinding.clone());
                        }
                    }
                    None => user_claims.push((key, vec![keybinding.clone()])),
                }
            }
        }
        for (key, claimants) in &user_claims {
            if claimants.len() > 1 {
                self.conflicts.push(KeyConflict {
                    key: key.clone(),
                    keybindings: claimants.clone(),
                });
            }
        }
        for def in &self.definitions {
            let user_keys = self
                .user_bindings
                .iter()
                .find(|(id, _)| id == def.id)
                .map(|(_, keys)| keys);
            let keys = match user_keys {
                None => def
                    .default_keys
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                Some(keys) => normalize_keys(Some(keys)),
            };
            self.keys_by_id.push((def.id.to_string(), keys));
        }
    }

    /// Does raw terminal `data` match any current key bound to `keybinding`?
    pub fn matches(&self, data: &str, keybinding: &str) -> bool {
        let keys = self.keys_for(keybinding);
        keys.iter().any(|key| matches_key(data, key))
    }

    fn keys_for(&self, keybinding: &str) -> &[String] {
        self.keys_by_id
            .iter()
            .find(|(id, _)| id == keybinding)
            .map(|(_, keys)| keys.as_slice())
            .unwrap_or(&[])
    }

    /// Copy of the currently bound keys for an action.
    pub fn get_keys(&self, keybinding: &str) -> Vec<String> {
        self.keys_for(keybinding).to_vec()
    }

    pub fn get_definition(&self, keybinding: &str) -> Option<&KeybindingDef> {
        self.definitions.iter().find(|d| d.id == keybinding)
    }

    pub fn get_conflicts(&self) -> Vec<KeyConflict> {
        self.conflicts.clone()
    }

    pub fn set_user_bindings(&mut self, user_bindings: Vec<(String, Vec<String>)>) {
        self.user_bindings = user_bindings;
        self.rebuild();
    }

    pub fn get_user_bindings(&self) -> Vec<(String, Vec<String>)> {
        self.user_bindings.clone()
    }

    /// All resolved bindings keyed by definition order. The reference exposes
    /// exactly one key as a scalar and zero or multiple keys as an array,
    /// regardless of the input value's original shape.
    pub fn get_resolved_bindings(&self) -> Vec<(String, ResolvedBinding)> {
        self.definitions
            .iter()
            .map(|d| {
                let keys = self.keys_for(d.id).to_vec();
                let resolved = match keys.as_slice() {
                    [key] => ResolvedBinding::Scalar(key.clone()),
                    _ => ResolvedBinding::List(keys),
                };
                (d.id.to_string(), resolved)
            })
            .collect()
    }
}

/// Process-global manager slot (reference `globalKeybindings`).
pub mod global {
    use super::KeybindingsManager;
    use std::sync::{Arc, RwLock};

    /// Clonable identity handle for the process-global manager. Every clone
    /// observes later `set_user_bindings` calls on that same manager.
    #[derive(Clone)]
    pub struct SharedKeybindings(Arc<RwLock<KeybindingsManager>>);

    impl SharedKeybindings {
        pub fn new(manager: KeybindingsManager) -> Self {
            Self(Arc::new(RwLock::new(manager)))
        }

        pub fn matches(&self, data: &str, keybinding: &str) -> bool {
            self.0
                .read()
                .expect("keybindings manager lock")
                .matches(data, keybinding)
        }

        pub fn get_keys(&self, keybinding: &str) -> Vec<String> {
            self.0
                .read()
                .expect("keybindings manager lock")
                .get_keys(keybinding)
        }

        pub fn set_user_bindings(&self, user_bindings: Vec<(String, Vec<String>)>) {
            self.0
                .write()
                .expect("keybindings manager lock")
                .set_user_bindings(user_bindings);
        }

        pub fn get_user_bindings(&self) -> Vec<(String, Vec<String>)> {
            self.0
                .read()
                .expect("keybindings manager lock")
                .get_user_bindings()
        }

        pub fn ptr_eq(&self, other: &Self) -> bool {
            Arc::ptr_eq(&self.0, &other.0)
        }
    }

    static SLOT: RwLock<Option<SharedKeybindings>> = RwLock::new(None);

    /// Reference `setKeybindings`.
    pub fn set_keybindings(manager: KeybindingsManager) {
        *SLOT.write().expect("keybindings lock") = Some(SharedKeybindings::new(manager));
    }

    /// Reference `getKeybindings` — lazily default-initialized over
    /// `TUI_KEYBINDINGS`.
    pub fn get_keybindings() -> SharedKeybindings {
        {
            let guard = SLOT.read().expect("keybindings lock");
            if let Some(mgr) = guard.as_ref() {
                return mgr.clone();
            }
        }
        let mut guard = SLOT.write().expect("keybindings lock");
        guard
            .get_or_insert_with(|| {
                SharedKeybindings::new(KeybindingsManager::with_tui_defaults(Vec::new()))
            })
            .clone()
    }
}
