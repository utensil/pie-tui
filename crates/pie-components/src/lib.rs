//! pie-components — the component library of the pie TUI.
//!
//! Behaviorally identical ports of the pinned pi-tui components. Render output
//! is validated against reference goldens (tests/). API shapes adapt to Rust
//! ownership: `render(&mut self, width)` instead of JS mutable-this, and
//! timers (Loader animation) become explicit `advance_frame()` calls owned by
//! the runtime adapter.

pub mod autocomplete;
pub mod box_component;
pub mod cancellable_loader;
pub mod cancellation;
pub mod container;
pub mod editor;
pub mod editor_component;
pub mod image;
pub mod input;
#[doc(hidden)]
pub mod layout;
pub mod loader;
pub mod markdown;
pub mod scroll_view;
pub mod select_list;
pub mod settings_list;
pub mod size_value;
pub mod spacer;
pub mod stack;
pub mod text;
pub mod truncated_text;
pub mod tui;
pub mod vstack_hstack;

pub use autocomplete::{
    ArgumentCompletionFn, ArgumentCompletionFuture, ArgumentCompletionResult, AutocompleteCommand,
    AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
    AutocompleteSuggestionsFuture, CombinedAutocompleteProvider, CompletionResult, SlashCommand,
};
pub use box_component::{BoxChildId, BoxComponent};
pub use cancellable_loader::CancellableLoader;
pub use cancellation::{CancellationController, CancellationSignal};
pub use container::{ComponentHandle, ComponentRef, Container, ContainerChildId};
pub use editor::{
    DetachedEditorHost, Editor, EditorAutocompleteFuture, EditorHost, EditorHostTask,
    EditorOptions, EditorSelectListTheme, EditorTaskId, EditorTextCallback, EditorTheme,
    SharedStyleFn,
};
pub use editor_component::EditorComponent;
pub use image::{
    Image, ImageCacheStats, ImageEnvironment, ImageOptions, ImageTheme, KittyImageDeletionOwner,
    KittyImageOwnership,
};
pub use input::{Input, InputEscapeCallback, InputSubmitCallback};
pub use loader::{Loader, LoaderIndicatorOptions, SpinnerIndicator};
pub use markdown::{
    DefaultTextStyle, HighlightCodeFn, Markdown, MarkdownOptions, MarkdownTheme,
    MarkdownTransformFn,
};
pub use scroll_view::{
    ScrollView, ScrollViewAxis, ScrollViewError, ScrollViewFollow, ScrollViewOptions,
    ScrollViewOverscroll, ScrollViewScrollToOptions, ScrollViewScrollbar, ScrollViewTimerHost,
    ScrollViewTimerId, ScrollbarStyle,
};
pub use select_list::{
    SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme, SelectListTruncateContext,
    TruncatePrimaryFn,
};
pub type SelectListTruncatePrimaryContext = SelectListTruncateContext;
pub use settings_list::{
    SelectedStyleFn, SettingItem, SettingsList, SettingsListOptions, SettingsListTheme,
    SubmenuDone, SubmenuFactory,
};
pub use size_value::{ParseSizeValueError, SizeValue};
pub use spacer::Spacer;
pub use stack::{StackEntry, StackViewport, StackVisibilityFn, allocate_stack_sizes};
pub use text::Text;
pub use truncated_text::TruncatedText;
pub use tui::{
    BackgroundColorQueryCallback, ColorSchemeQueryCallback, DebugCallback, OverlayAnchor,
    OverlayControl, OverlayMargin, OverlayMargins, OverlayOptions, OverlayUnfocus,
    SubscriptionControl, TerminalColorSchemeListener, Tui, TuiInputListener,
    TuiInputListenerResult, TuiMode, TuiStopOptions, ViewportTui,
};
pub use vstack_hstack::{Align, HStack, VStack};

/// A styling closure over a text span (reference `ColorFn`-style callbacks).
pub type StyleFn = Box<dyn Fn(&str) -> String + Send>;

/// A renderable node in the component tree (reference `Component` shape:
/// `render(width) -> lines` plus optional `invalidate`).
pub trait Component {
    /// Render to logical lines at the given width (unpadded contract varies
    /// per component, mirroring the reference exactly).
    fn render(&mut self, width: usize) -> Vec<String>;
    /// Drop cached state.
    fn invalidate(&mut self) {}
    /// Consume raw terminal input when the component is interactive.
    fn handle_input(&mut self, _data: &str) {}

    /// Whether this component opts in to Kitty key-release events.
    fn wants_key_release(&self) -> bool {
        false
    }

    /// Return the canonical focus flag when this component is focusable.
    fn focused(&self) -> Option<bool> {
        None
    }

    /// Set the canonical focus flag. Returns whether the component is
    /// focusable, which is Rust's structural counterpart to `isFocusable`.
    fn set_focused(&mut self, _focused: bool) -> bool {
        false
    }

    /// Internal retained-identity lookup used by nested containers.
    #[doc(hidden)]
    fn contains_component(&self, _identity: u64) -> bool {
        false
    }

    /// Stable-for-the-frame render-cache identity. Shared component mounts
    /// delegate this to their retained object, matching JavaScript object-key
    /// cache semantics.
    #[doc(hidden)]
    fn render_identity(&self) -> usize {
        let pointer: *const Self = self;
        pointer.cast::<()>() as usize
    }

    /// Internal layout dispatch. Ordinary components are leaves; layout-aware
    /// components override this without widening the canonical public barrel.
    #[doc(hidden)]
    fn layout(
        &mut self,
        context: &mut layout::LayoutContext,
        allocation: layout::LayoutAllocation,
    ) -> layout::LayoutBox {
        context.layout_leaf(self, allocation)
    }
}
