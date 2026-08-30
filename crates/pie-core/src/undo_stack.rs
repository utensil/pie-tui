//! Generic pure undo storage with clone-on-push ownership.

/// A last-in-first-out collection of detached state snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoStack<T: Clone> {
    stack: Vec<T>,
}

impl<T: Clone> Default for UndoStack<T> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<T: Clone> UndoStack<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store an owned clone so later changes to `state` cannot alter history.
    pub fn push(&mut self, state: &T) {
        self.stack.push(state.clone());
    }

    /// Return the newest already-detached snapshot.
    pub fn pop(&mut self) -> Option<T> {
        self.stack.pop()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}
