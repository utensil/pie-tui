//! Pure ring buffer for Emacs-style kill and yank operations.

/// Options controlling how a killed span enters the ring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PushOptions {
    /// Put newly accumulated text before the latest entry (backward kill).
    pub prepend: bool,
    /// Merge with the latest entry instead of adding another ring entry.
    pub accumulate: bool,
}

/// Text killed by editor commands, newest entry last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KillRing {
    ring: Vec<String>,
}

impl KillRing {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add killed text. Empty strings are ignored, as in the reference.
    pub fn push(&mut self, text: &str, options: PushOptions) {
        if text.is_empty() {
            return;
        }
        if options.accumulate && !self.ring.is_empty() {
            let latest = self.ring.pop().expect("non-empty ring has a latest entry");
            self.ring.push(if options.prepend {
                format!("{text}{latest}")
            } else {
                format!("{latest}{text}")
            });
        } else {
            self.ring.push(text.to_owned());
        }
    }

    /// Inspect the newest entry without rotating the ring.
    pub fn peek(&self) -> Option<&str> {
        self.ring.last().map(String::as_str)
    }

    /// Move the newest entry to the oldest position for yank-pop cycling.
    pub fn rotate(&mut self) {
        if self.ring.len() > 1 {
            let latest = self.ring.pop().expect("ring has at least two entries");
            self.ring.insert(0, latest);
        }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}
