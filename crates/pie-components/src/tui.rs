//! Rank-2 structural contracts shared by TUI components and controllers.
//!
//! Runtime scheduling and terminal ownership live in `pie-app`; these types
//! contain only component identities, callbacks, and canonical controller
//! facts, so lower components never import an application or adapter.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use pie_core::terminal_colors::{RgbColor, TerminalColorScheme};

use crate::{ComponentRef, ContainerChildId, SizeValue};

static NEXT_LISTENER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Regular,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TuiStopOptions {
    pub preserve_screen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiInputListenerResult {
    pub consume: bool,
    pub data: Option<String>,
}

impl TuiInputListenerResult {
    pub fn pass() -> Self {
        Self {
            consume: false,
            data: None,
        }
    }

    pub fn consume() -> Self {
        Self {
            consume: true,
            data: None,
        }
    }

    pub fn transform(data: impl Into<String>) -> Self {
        Self {
            consume: false,
            data: Some(data.into()),
        }
    }
}

type InputCallback = dyn Fn(&str) -> Option<TuiInputListenerResult>;

/// Retained callback identity matching JavaScript `Set` listener semantics.
#[derive(Clone)]
pub struct TuiInputListener {
    identity: u64,
    callback: Rc<InputCallback>,
}

impl TuiInputListener {
    pub fn new(callback: impl Fn(&str) -> Option<TuiInputListenerResult> + 'static) -> Self {
        Self {
            identity: NEXT_LISTENER_ID.fetch_add(1, Ordering::Relaxed),
            callback: Rc::new(callback),
        }
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    #[doc(hidden)]
    pub fn invoke(&self, data: &str) -> Option<TuiInputListenerResult> {
        (self.callback)(data)
    }
}

type SchemeCallback = dyn Fn(TerminalColorScheme);

/// Retained terminal-color listener identity with insertion-order semantics.
#[derive(Clone)]
pub struct TerminalColorSchemeListener {
    identity: u64,
    callback: Rc<SchemeCallback>,
}

impl TerminalColorSchemeListener {
    pub fn new(callback: impl Fn(TerminalColorScheme) + 'static) -> Self {
        Self {
            identity: NEXT_LISTENER_ID.fetch_add(1, Ordering::Relaxed),
            callback: Rc::new(callback),
        }
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    #[doc(hidden)]
    pub fn invoke(&self, scheme: TerminalColorScheme) {
        (self.callback)(scheme);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OverlayAnchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    LeftCenter,
    RightCenter,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayMargin {
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
    pub left: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMargins {
    All(i64),
    Sides(OverlayMargin),
}

pub type OverlayVisibility = Rc<dyn Fn(usize, usize) -> bool>;

#[derive(Clone, Default)]
pub struct OverlayOptions {
    pub width: Option<SizeValue>,
    pub min_width: Option<usize>,
    pub max_height: Option<SizeValue>,
    pub anchor: OverlayAnchor,
    pub offset_x: i64,
    pub offset_y: i64,
    pub row: Option<SizeValue>,
    pub col: Option<SizeValue>,
    pub margin: Option<OverlayMargins>,
    pub visible: Option<OverlayVisibility>,
    pub non_capturing: bool,
}

#[derive(Clone)]
pub enum OverlayUnfocus {
    Restore,
    Target(Option<ComponentRef>),
}

pub trait OverlayControl {
    fn hide(&self);
    fn set_hidden(&self, hidden: bool);
    fn is_hidden(&self) -> bool;
    fn focus(&self);
    fn unfocus(&self, target: OverlayUnfocus);
    fn is_focused(&self) -> bool;
}

pub trait SubscriptionControl {
    fn unsubscribe(&self);
    fn is_active(&self) -> bool;
}

pub type BackgroundColorQueryCallback = Box<dyn FnOnce(Option<RgbColor>)>;
pub type ColorSchemeQueryCallback = Box<dyn FnOnce(Option<TerminalColorScheme>)>;
pub type DebugCallback = Box<dyn Fn()>;

/// Object-safe Rust surface corresponding to the canonical `TUI` structure.
pub trait Tui {
    fn mode(&self) -> TuiMode;
    fn terminal_columns(&self) -> usize;
    fn terminal_rows(&self) -> usize;
    fn full_redraws(&self) -> usize;
    fn add_child(&self, component: ComponentRef) -> ContainerChildId;
    fn remove_child(&self, child: ContainerChildId) -> bool;
    fn clear(&self);
    fn show_hardware_cursor(&self) -> bool;
    fn set_show_hardware_cursor(&self, enabled: bool);
    fn clear_on_shrink(&self) -> bool;
    fn set_clear_on_shrink(&self, enabled: bool);
    fn focused_component(&self) -> Option<ComponentRef>;
    fn set_focus(&self, component: Option<ComponentRef>);
    fn show_overlay(
        &self,
        component: ComponentRef,
        options: OverlayOptions,
    ) -> Box<dyn OverlayControl>;
    fn hide_overlay(&self);
    fn has_overlay(&self) -> bool;
    fn start(&self);
    fn stop(&self, options: TuiStopOptions);
    fn render_now(&self, force: bool);
    fn request_render(&self, force: bool);
    fn add_input_listener(&self, listener: TuiInputListener) -> Box<dyn SubscriptionControl>;
    fn remove_input_listener(&self, listener: &TuiInputListener);
    fn on_terminal_color_scheme_change(
        &self,
        listener: TerminalColorSchemeListener,
    ) -> Box<dyn SubscriptionControl>;
    fn set_terminal_color_scheme_notifications(&self, enabled: bool);
    fn query_terminal_background_color(
        &self,
        timeout_ms: u64,
        callback: BackgroundColorQueryCallback,
    );
    fn query_terminal_color_scheme(&self, timeout_ms: u64, callback: ColorSchemeQueryCallback);
    fn set_debug_callback(&self, callback: Option<DebugCallback>);
}

/// Structural marker for controllers that own a layout viewport root.
pub trait ViewportTui: Tui {
    fn set_layout_root(&self, component: Option<ComponentRef>);
}
