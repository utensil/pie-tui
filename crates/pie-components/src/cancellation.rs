//! Small, dependency-free analogue of the DOM `AbortSignal` contract.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type AbortCallback = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct CancellationState {
    aborted: AtomicBool,
    callbacks: Mutex<Vec<AbortCallback>>,
}

/// A clonable live cancellation signal. Cancellation is one-shot: every
/// retained clone observes the transition and listeners run at most once.
#[derive(Clone, Default)]
pub struct CancellationSignal {
    state: Arc<CancellationState>,
}

impl CancellationSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn aborted(&self) -> bool {
        self.state.aborted.load(Ordering::Acquire)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Register a one-shot listener. As with `AbortSignal`, listeners added
    /// after cancellation observe it immediately.
    pub fn on_cancel(&self, callback: impl FnOnce() + Send + 'static) {
        if self.aborted() {
            callback();
            return;
        }
        let mut callbacks = self.state.callbacks.lock().expect("cancellation callbacks");
        if self.aborted() {
            drop(callbacks);
            callback();
        } else {
            callbacks.push(Box::new(callback));
        }
    }

    /// Transition to cancelled. Returns true only for the first transition.
    pub(crate) fn cancel(&self) -> bool {
        if self.state.aborted.swap(true, Ordering::AcqRel) {
            return false;
        }
        let callbacks = {
            let mut callbacks = self.state.callbacks.lock().expect("cancellation callbacks");
            std::mem::take(&mut *callbacks)
        };
        for callback in callbacks {
            callback();
        }
        true
    }
}

/// Owner for a [`CancellationSignal`], mirroring the DOM `AbortController`
/// split between the mutating controller and clonable read-only signal.
#[derive(Clone, Default)]
pub struct CancellationController {
    signal: CancellationSignal,
}

impl CancellationController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn signal(&self) -> CancellationSignal {
        self.signal.clone()
    }

    /// Cancel every retained clone. Returns true only for the first call.
    pub fn cancel(&self) -> bool {
        self.signal.cancel()
    }
}
