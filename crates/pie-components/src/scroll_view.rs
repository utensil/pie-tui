//! Vertical ScrollView state and adapter-injected transient-scrollbar timer.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Component;
use crate::container::Container;
use crate::layout::{LayoutAllocation, LayoutBox, LayoutContext, ScrollViewId, ScrollbarPaint};

static NEXT_SCROLL_VIEW_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollViewAxis {
    #[default]
    Vertical,
    Horizontal,
}

impl ScrollViewAxis {
    fn as_reference_str(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollViewFollow {
    #[default]
    None,
    End,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollViewOverscroll {
    #[default]
    Chain,
    Contain,
}

/// Canonical reference union: `"hidden" | "auto" | "always"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollViewScrollbar {
    #[default]
    Hidden,
    Auto,
    Always,
}

#[derive(Clone)]
pub struct ScrollbarStyle(Rc<dyn Fn(&str) -> String>);

impl ScrollbarStyle {
    pub fn new(style: impl Fn(&str) -> String + 'static) -> Self {
        Self(Rc::new(style))
    }

    pub(crate) fn apply(&self, text: &str) -> String {
        (self.0)(text)
    }
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self::new(|text| format!("\x1b[100m{text}\x1b[49m"))
    }
}

impl Debug for ScrollbarStyle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ScrollbarStyle(..)")
    }
}

impl PartialEq for ScrollbarStyle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ScrollbarStyle {}

#[derive(Debug, Clone)]
pub struct ScrollViewOptions {
    pub axis: ScrollViewAxis,
    pub follow: ScrollViewFollow,
    pub primary: bool,
    pub overscroll: ScrollViewOverscroll,
    pub scrollbar: ScrollViewScrollbar,
    pub scrollbar_style: ScrollbarStyle,
    pub scrollbar_hide_delay_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollViewScrollToOptions {
    pub disable_follow: bool,
}

impl Default for ScrollViewOptions {
    fn default() -> Self {
        Self {
            axis: ScrollViewAxis::Vertical,
            follow: ScrollViewFollow::None,
            primary: false,
            overscroll: ScrollViewOverscroll::Chain,
            scrollbar: ScrollViewScrollbar::Hidden,
            scrollbar_style: ScrollbarStyle::default(),
            scrollbar_hide_delay_ms: 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollViewError {
    UnsupportedAxis(ScrollViewAxis),
    ExtraChild,
    RemoveChild,
    ClearChild,
}

impl std::fmt::Display for ScrollViewError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedAxis(axis) => {
                write!(
                    formatter,
                    "Unsupported ScrollView axis: {}",
                    axis.as_reference_str()
                )
            }
            Self::ExtraChild => formatter.write_str("ScrollView has exactly one child"),
            Self::RemoveChild => formatter.write_str("ScrollView child cannot be removed"),
            Self::ClearChild => formatter.write_str("ScrollView child cannot be cleared"),
        }
    }
}

impl Error for ScrollViewError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScrollViewTimerId(u64);

impl ScrollViewTimerId {
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub fn into_raw(self) -> u64 {
        self.0
    }
}

/// Runtime-owned timer seam. Implementations must enqueue callbacks rather
/// than invoking them synchronously from `set_timeout`.
pub trait ScrollViewTimerHost {
    fn set_timeout(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> ScrollViewTimerId;
    fn clear_timeout(&self, timer: ScrollViewTimerId);
    fn unref_timeout(&self, _timer: ScrollViewTimerId) {}
}

struct PendingTimer {
    _delay_ms: u64,
    _callback: Box<dyn FnOnce()>,
}

#[derive(Default)]
struct PassiveTimerHost {
    next_id: RefCell<u64>,
    pending: RefCell<BTreeMap<ScrollViewTimerId, PendingTimer>>,
}

impl ScrollViewTimerHost for PassiveTimerHost {
    fn set_timeout(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> ScrollViewTimerId {
        let mut next_id = self.next_id.borrow_mut();
        *next_id = next_id.wrapping_add(1);
        let id = ScrollViewTimerId(*next_id);
        self.pending.borrow_mut().insert(
            id,
            PendingTimer {
                _delay_ms: delay_ms,
                _callback: callback,
            },
        );
        id
    }

    fn clear_timeout(&self, timer: ScrollViewTimerId) {
        self.pending.borrow_mut().remove(&timer);
    }
}

#[derive(Default)]
struct TransientScrollbar {
    visible: bool,
    active: bool,
    generation: u64,
    timer: Option<ScrollbarTimerToken>,
    request_render: Option<Rc<dyn Fn()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollbarTimerToken {
    id: ScrollViewTimerId,
    generation: u64,
}

fn advance_timer_generation(transient: &mut TransientScrollbar) -> u64 {
    transient.generation = transient
        .generation
        .checked_add(1)
        .expect("ScrollView timer generation exhausted");
    transient.generation
}

pub struct ScrollView {
    id: ScrollViewId,
    child: Container,
    follow_end: bool,
    primary: bool,
    overscroll: ScrollViewOverscroll,
    scrollbar_style: ScrollbarStyle,
    scrollbar: ScrollViewScrollbar,
    scrollbar_hide_delay_ms: u64,
    scroll_top: usize,
    content_height: usize,
    viewport_height: usize,
    following_end: bool,
    follow_suppressed_at_end: bool,
    transient: Rc<RefCell<TransientScrollbar>>,
    timer_host: Rc<dyn ScrollViewTimerHost>,
}

impl ScrollView {
    pub fn new(component: Box<dyn Component>, options: ScrollViewOptions) -> Self {
        Self::try_with_timer_host(component, options, Rc::new(PassiveTimerHost::default()))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_new(
        component: Box<dyn Component>,
        options: ScrollViewOptions,
    ) -> Result<Self, ScrollViewError> {
        Self::try_with_timer_host(component, options, Rc::new(PassiveTimerHost::default()))
    }

    pub fn with_timer_host(
        component: Box<dyn Component>,
        options: ScrollViewOptions,
        timer_host: Rc<dyn ScrollViewTimerHost>,
    ) -> Self {
        Self::try_with_timer_host(component, options, timer_host)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_with_timer_host(
        component: Box<dyn Component>,
        options: ScrollViewOptions,
        timer_host: Rc<dyn ScrollViewTimerHost>,
    ) -> Result<Self, ScrollViewError> {
        if options.axis != ScrollViewAxis::Vertical {
            return Err(ScrollViewError::UnsupportedAxis(options.axis));
        }
        let mut child = Container::new();
        child.add_child(component);
        let follow_end = options.follow == ScrollViewFollow::End;
        Ok(Self {
            id: ScrollViewId(NEXT_SCROLL_VIEW_ID.fetch_add(1, Ordering::Relaxed)),
            child,
            follow_end,
            primary: options.primary,
            overscroll: options.overscroll,
            scrollbar_style: options.scrollbar_style,
            scrollbar: options.scrollbar,
            scrollbar_hide_delay_ms: options.scrollbar_hide_delay_ms,
            scroll_top: 0,
            content_height: 0,
            viewport_height: 0,
            following_end: follow_end,
            follow_suppressed_at_end: false,
            transient: Rc::new(RefCell::new(TransientScrollbar::default())),
            timer_host,
        })
    }

    pub fn id(&self) -> ScrollViewId {
        self.id
    }

    pub fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    pub fn is_following_end(&self) -> bool {
        self.following_end
    }

    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub fn scrollbar(&self) -> ScrollViewScrollbar {
        self.scrollbar
    }

    pub fn primary(&self) -> bool {
        self.primary
    }

    pub fn overscroll(&self) -> ScrollViewOverscroll {
        self.overscroll
    }

    pub fn is_scrollbar_visible(&self) -> bool {
        match self.scrollbar {
            ScrollViewScrollbar::Always => self.viewport_height > 0,
            ScrollViewScrollbar::Auto => {
                self.content_height > self.viewport_height && self.transient.borrow().visible
            }
            ScrollViewScrollbar::Hidden => false,
        }
    }

    pub fn set_scrollbar(&mut self, scrollbar: ScrollViewScrollbar) {
        if scrollbar == self.scrollbar {
            return;
        }
        self.scrollbar = scrollbar;
        if scrollbar != ScrollViewScrollbar::Auto {
            self.hide_transient_scrollbar();
        } else if self.transient.borrow().active {
            self.mark_scrollbar_activity();
        }
        self.request_render();
    }

    pub fn get_content_width(&self, width: usize) -> usize {
        if self.scrollbar == ScrollViewScrollbar::Always && width > 1 {
            width - 1
        } else {
            width
        }
    }

    fn cancel_timer(&self, timer: Option<ScrollbarTimerToken>) {
        if let Some(timer) = timer {
            self.timer_host.clear_timeout(timer.id);
        }
    }

    fn mark_scrollbar_activity(&self) {
        if self.scrollbar != ScrollViewScrollbar::Auto
            || self.content_height <= self.viewport_height
        {
            return;
        }
        let (old_timer, active, generation) = {
            let mut transient = self.transient.borrow_mut();
            transient.visible = true;
            let old_timer = transient.timer.take();
            let active = transient.active;
            let generation = advance_timer_generation(&mut transient);
            (old_timer, active, generation)
        };
        self.cancel_timer(old_timer);
        if active {
            return;
        }
        let transient = self.transient.clone();
        let timer = self.timer_host.set_timeout(
            self.scrollbar_hide_delay_ms,
            Box::new(move || {
                let request_render = {
                    let mut transient = transient.borrow_mut();
                    if transient.generation != generation
                        || transient
                            .timer
                            .is_none_or(|timer| timer.generation != generation)
                    {
                        return;
                    }
                    transient.timer = None;
                    transient.visible = false;
                    transient.request_render.clone()
                };
                if let Some(request_render) = request_render {
                    request_render();
                }
            }),
        );
        self.transient.borrow_mut().timer = Some(ScrollbarTimerToken {
            id: timer,
            generation,
        });
        self.timer_host.unref_timeout(timer);
    }

    fn hide_transient_scrollbar(&self) {
        let timer = {
            let mut transient = self.transient.borrow_mut();
            transient.visible = false;
            if transient.timer.is_some() {
                advance_timer_generation(&mut transient);
            }
            transient.timer.take()
        };
        self.cancel_timer(timer);
    }

    pub fn set_scrollbar_active(&mut self, active: bool) {
        if self.transient.borrow().active == active {
            return;
        }
        self.transient.borrow_mut().active = active;
        self.mark_scrollbar_activity();
    }

    pub fn scroll_to(&mut self, scroll_top: i64) {
        self.scroll_to_with_options(scroll_top, ScrollViewScrollToOptions::default());
    }

    pub fn scroll_to_with_options(&mut self, scroll_top: i64, options: ScrollViewScrollToOptions) {
        let maximum = self.maximum_scroll_top();
        let next = clamp_signed(scroll_top, maximum);
        let next_follow_suppressed_at_end = options.disable_follow && next == maximum;
        let next_following_end =
            !next_follow_suppressed_at_end && self.follow_end && next == maximum;
        if next == self.scroll_top
            && next_following_end == self.following_end
            && next_follow_suppressed_at_end == self.follow_suppressed_at_end
        {
            return;
        }
        let moved = next != self.scroll_top;
        self.scroll_top = next;
        self.following_end = next_following_end;
        self.follow_suppressed_at_end = next_follow_suppressed_at_end;
        if moved {
            self.mark_scrollbar_activity();
        }
        self.request_render();
    }

    pub fn scroll_by(&mut self, lines: i64) -> i64 {
        if lines == 0 {
            return 0;
        }
        let maximum = self.maximum_scroll_top();
        let start = if self.following_end {
            maximum
        } else {
            self.scroll_top
        };
        let requested = saturating_add_signed(start, lines);
        let next = clamp_signed(requested, maximum);
        let moved = signed_difference(next, start);
        self.scroll_top = next;
        let was_following_end = self.following_end;
        self.following_end = self.follow_end && next == maximum;
        self.follow_suppressed_at_end = false;
        if moved != 0 {
            self.mark_scrollbar_activity();
        }
        if moved != 0 || self.following_end != was_following_end {
            self.request_render();
        }
        lines.saturating_sub(moved)
    }

    pub fn scroll_to_start(&mut self) {
        let following = self.follow_end && self.content_height <= self.viewport_height;
        let changed = self.scroll_top != 0 || self.following_end != following;
        self.scroll_top = 0;
        self.following_end = following;
        self.follow_suppressed_at_end = false;
        if changed {
            self.mark_scrollbar_activity();
            self.request_render();
        }
    }

    pub fn scroll_to_end(&mut self) {
        let next = self.maximum_scroll_top();
        let changed = self.scroll_top != next || self.following_end != self.follow_end;
        self.scroll_top = next;
        self.following_end = self.follow_end;
        self.follow_suppressed_at_end = false;
        if changed {
            self.mark_scrollbar_activity();
            self.request_render();
        }
    }

    pub fn update_layout(
        &mut self,
        content_height: usize,
        viewport_height: usize,
        request_render: Rc<dyn Fn()>,
    ) {
        self.content_height = content_height;
        self.viewport_height = viewport_height;
        self.transient.borrow_mut().request_render = Some(request_render);
        let maximum = self.maximum_scroll_top();
        if self.following_end {
            self.scroll_top = maximum;
        } else {
            self.scroll_top = self.scroll_top.min(maximum);
        }
        if self.scroll_top < maximum {
            self.follow_suppressed_at_end = false;
        }
        if self.follow_end && self.scroll_top == maximum && !self.follow_suppressed_at_end {
            self.following_end = true;
        }
        if self.content_height <= self.viewport_height {
            self.hide_transient_scrollbar();
        }
    }

    pub fn add_child(&mut self, _component: Box<dyn Component>) -> Result<(), ScrollViewError> {
        Err(ScrollViewError::ExtraChild)
    }

    pub fn remove_child(&mut self, _component: &dyn Component) -> Result<(), ScrollViewError> {
        Err(ScrollViewError::RemoveChild)
    }

    pub fn clear(&mut self) -> Result<(), ScrollViewError> {
        Err(ScrollViewError::ClearChild)
    }

    fn maximum_scroll_top(&self) -> usize {
        self.content_height.saturating_sub(self.viewport_height)
    }

    fn request_render(&self) {
        let callback = self.transient.borrow().request_render.clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    pub(crate) fn layout_child(
        &mut self,
        context: &mut LayoutContext,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        self.child.layout_child(0, context, allocation)
    }

    pub(crate) fn render_child_cached(
        &mut self,
        context: &mut LayoutContext,
        width: usize,
    ) -> Vec<String> {
        self.child.render_child_cached(0, context, width)
    }

    pub(crate) fn paint_state(&self) -> ScrollbarPaint {
        ScrollbarPaint {
            visible: self.is_scrollbar_visible(),
            style: self.scrollbar_style.clone(),
            scroll_top: self.scroll_top,
            content_height: self.content_height,
        }
    }
}

impl Component for ScrollView {
    fn invalidate(&mut self) {
        self.child.invalidate();
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        let content_width = self.get_content_width(width);
        let lines = self.child.render_child(0, content_width);
        if content_width == width {
            lines
        } else {
            lines.into_iter().map(|line| format!("{line} ")).collect()
        }
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        context.layout_scroll_view(self, allocation)
    }
}

impl Drop for ScrollView {
    fn drop(&mut self) {
        let timer = {
            let mut transient = self.transient.borrow_mut();
            advance_timer_generation(&mut transient);
            transient.request_render = None;
            transient.timer.take()
        };
        self.cancel_timer(timer);
    }
}

fn clamp_signed(value: i64, maximum: usize) -> usize {
    if value <= 0 {
        0
    } else {
        usize::try_from(value).unwrap_or(usize::MAX).min(maximum)
    }
}

fn saturating_add_signed(value: usize, delta: i64) -> i64 {
    let value = i64::try_from(value).unwrap_or(i64::MAX);
    value.saturating_add(delta)
}

fn signed_difference(next: usize, previous: usize) -> i64 {
    if next >= previous {
        i64::try_from(next - previous).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(previous - next).unwrap_or(i64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{
        ScrollView, ScrollViewOptions, ScrollViewScrollbar, ScrollViewTimerHost, ScrollViewTimerId,
    };
    use crate::Component;

    struct Rows;

    impl Component for Rows {
        fn render(&mut self, _width: usize) -> Vec<String> {
            vec!["0".to_string(), "1".to_string(), "2".to_string()]
        }
    }

    #[derive(Default)]
    struct RecordingTimerHost {
        clears: Cell<usize>,
    }

    impl ScrollViewTimerHost for RecordingTimerHost {
        fn set_timeout(&self, _delay_ms: u64, _callback: Box<dyn FnOnce()>) -> ScrollViewTimerId {
            ScrollViewTimerId::from_raw(1)
        }

        fn clear_timeout(&self, _timer: ScrollViewTimerId) {
            self.clears.set(self.clears.get() + 1);
        }
    }

    #[test]
    fn drop_invalidates_generation_clears_callback_and_cancels_current_token() {
        let host = Rc::new(RecordingTimerHost::default());
        let mut view = ScrollView::with_timer_host(
            Box::new(Rows),
            ScrollViewOptions {
                scrollbar: ScrollViewScrollbar::Auto,
                ..ScrollViewOptions::default()
            },
            host.clone(),
        );
        view.update_layout(3, 1, Rc::new(|| {}));
        view.scroll_by(1);
        let transient = view.transient.clone();
        let generation = transient.borrow().generation;
        assert!(transient.borrow().timer.is_some());

        drop(view);

        let transient = transient.borrow();
        assert_eq!(transient.generation, generation + 1);
        assert!(transient.timer.is_none());
        assert!(transient.request_render.is_none());
        assert_eq!(host.clears.get(), 1);
    }
}
