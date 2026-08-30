//! Container — ordered, identity-preserving component composition.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Component;
use crate::layout::{LayoutAllocation, LayoutBox, LayoutContext};

static NEXT_CONTAINER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_COMPONENT_ID: AtomicU64 = AtomicU64::new(1);

/// Owner-scoped mount token. Passing a token to a different container, or
/// reusing it after removal, is deliberately a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContainerChildId {
    owner: u64,
    mount: u64,
}

/// Typed shared identity for a component. The caller and every container mount
/// retain the same object, matching JavaScript reference semantics without
/// leaking ownership or relying on raw pointers.
pub struct ComponentHandle<T: Component + 'static> {
    identity: u64,
    inner: Rc<RefCell<T>>,
}

impl<T: Component + 'static> Clone for ComponentHandle<T> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity,
            inner: self.inner.clone(),
        }
    }
}

impl<T: Component + 'static> ComponentHandle<T> {
    pub fn new(component: T) -> Self {
        Self {
            identity: NEXT_COMPONENT_ID.fetch_add(1, Ordering::Relaxed),
            inner: Rc::new(RefCell::new(component)),
        }
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        self.inner.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    /// Erase the concrete type while retaining the same shared component.
    pub fn as_component_ref(&self) -> ComponentRef {
        ComponentRef {
            identity: self.identity,
            inner: Rc::new(RetainedHandle {
                inner: self.inner.clone(),
            }),
        }
    }
}

trait RetainedComponent {
    fn render(&self, width: usize) -> Vec<String>;
    fn invalidate(&self);
    fn handle_input(&self, data: &str);
    fn wants_key_release(&self) -> bool;
    fn focused(&self) -> Option<bool>;
    fn set_focused(&self, focused: bool) -> bool;
    fn render_cached(&self, context: &mut LayoutContext, width: usize, mount: u64) -> Vec<String>;
    fn measure_width(&self, context: &mut LayoutContext, width: usize, mount: u64) -> usize;
    fn layout(
        &self,
        context: &mut LayoutContext,
        allocation: LayoutAllocation,
        mount: u64,
    ) -> LayoutBox;
    fn render_identity(&self) -> usize;
    fn contains_component(&self, identity: u64) -> bool;
}

struct RetainedHandle<T: Component + 'static> {
    inner: Rc<RefCell<T>>,
}

impl<T: Component + 'static> RetainedComponent for RetainedHandle<T> {
    fn render(&self, width: usize) -> Vec<String> {
        self.inner.borrow_mut().render(width)
    }

    fn invalidate(&self) {
        self.inner.borrow_mut().invalidate();
    }

    fn handle_input(&self, data: &str) {
        self.inner.borrow_mut().handle_input(data);
    }

    fn wants_key_release(&self) -> bool {
        self.inner.borrow().wants_key_release()
    }

    fn focused(&self) -> Option<bool> {
        self.inner.borrow().focused()
    }

    fn set_focused(&self, focused: bool) -> bool {
        self.inner.borrow_mut().set_focused(focused)
    }

    fn render_cached(&self, context: &mut LayoutContext, width: usize, mount: u64) -> Vec<String> {
        context.render_cached_mounted(&mut *self.inner.borrow_mut(), width, mount)
    }

    fn measure_width(&self, context: &mut LayoutContext, width: usize, mount: u64) -> usize {
        context.measure_width_mounted(&mut *self.inner.borrow_mut(), width, mount)
    }

    fn layout(
        &self,
        context: &mut LayoutContext,
        allocation: LayoutAllocation,
        mount: u64,
    ) -> LayoutBox {
        context.layout_mounted(&mut *self.inner.borrow_mut(), mount, allocation)
    }

    fn render_identity(&self) -> usize {
        self.inner.borrow().render_identity()
    }

    fn contains_component(&self, identity: u64) -> bool {
        let component = self.inner.borrow();
        component.contains_component(identity)
    }
}

/// Type-erased retained component identity used by focus, overlays, and TUI
/// roots. Clones refer to the same object and mutations remain visible through
/// the original typed [`ComponentHandle`].
#[derive(Clone)]
pub struct ComponentRef {
    identity: u64,
    inner: Rc<dyn RetainedComponent>,
}

impl ComponentRef {
    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }

    pub fn render(&self, width: usize) -> Vec<String> {
        self.inner.render(width)
    }

    pub fn invalidate(&self) {
        self.inner.invalidate();
    }

    pub fn handle_input(&self, data: &str) {
        self.inner.handle_input(data);
    }

    pub fn wants_key_release(&self) -> bool {
        self.inner.wants_key_release()
    }

    pub fn focused(&self) -> Option<bool> {
        self.inner.focused()
    }

    pub fn set_focused(&self, focused: bool) -> bool {
        self.inner.set_focused(focused)
    }

    pub fn contains_component_ref(&self, component: &Self) -> bool {
        self.ptr_eq(component) || self.inner.contains_component(component.identity)
    }
}

/// Preserve the retained component's custom layout dispatch after type
/// erasure. This is the private rank-2 bridge used by application-owned
/// viewport controllers; it does not add a second component identity.
impl Component for ComponentRef {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.inner.render(width)
    }

    fn invalidate(&mut self) {
        self.inner.invalidate();
    }

    fn handle_input(&mut self, data: &str) {
        self.inner.handle_input(data);
    }

    fn wants_key_release(&self) -> bool {
        self.inner.wants_key_release()
    }

    fn focused(&self) -> Option<bool> {
        self.inner.focused()
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.inner.set_focused(focused)
    }

    fn contains_component(&self, identity: u64) -> bool {
        self.identity == identity || self.inner.contains_component(identity)
    }

    fn render_identity(&self) -> usize {
        self.inner.render_identity()
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        self.inner.layout(context, allocation, self.identity)
    }
}

trait MountedComponent {
    fn identity(&self) -> u64;
    fn render(&mut self, width: usize) -> Vec<String>;
    fn invalidate(&mut self);
    fn render_cached(&mut self, context: &mut LayoutContext, width: usize) -> Vec<String>;
    fn measure_width(&mut self, context: &mut LayoutContext, width: usize) -> usize;
    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox;
    fn render_identity(&self) -> usize;
    fn contains_component(&self, identity: u64) -> bool;
}

struct SharedMount<T: Component + 'static> {
    handle: ComponentHandle<T>,
}

impl<T: Component + 'static> MountedComponent for SharedMount<T> {
    fn identity(&self) -> u64 {
        self.handle.identity
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.handle.borrow_mut().render(width)
    }

    fn invalidate(&mut self) {
        self.handle.borrow_mut().invalidate();
    }

    fn render_cached(&mut self, context: &mut LayoutContext, width: usize) -> Vec<String> {
        context.render_cached_mounted(&mut *self.handle.borrow_mut(), width, self.handle.identity)
    }

    fn measure_width(&mut self, context: &mut LayoutContext, width: usize) -> usize {
        context.measure_width_mounted(&mut *self.handle.borrow_mut(), width, self.handle.identity)
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        context.layout_mounted(
            &mut *self.handle.borrow_mut(),
            self.handle.identity,
            allocation,
        )
    }

    fn render_identity(&self) -> usize {
        self.handle.borrow().render_identity()
    }

    fn contains_component(&self, identity: u64) -> bool {
        self.handle.identity == identity || self.handle.borrow().contains_component(identity)
    }
}

struct ErasedSharedMount {
    handle: ComponentRef,
}

impl MountedComponent for ErasedSharedMount {
    fn identity(&self) -> u64 {
        self.handle.identity
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.handle.inner.render(width)
    }

    fn invalidate(&mut self) {
        self.handle.inner.invalidate();
    }

    fn render_cached(&mut self, context: &mut LayoutContext, width: usize) -> Vec<String> {
        self.handle
            .inner
            .render_cached(context, width, self.handle.identity)
    }

    fn measure_width(&mut self, context: &mut LayoutContext, width: usize) -> usize {
        self.handle
            .inner
            .measure_width(context, width, self.handle.identity)
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        self.handle
            .inner
            .layout(context, allocation, self.handle.identity)
    }

    fn render_identity(&self) -> usize {
        self.handle.inner.render_identity()
    }

    fn contains_component(&self, identity: u64) -> bool {
        self.handle.identity == identity || self.handle.inner.contains_component(identity)
    }
}

struct OwnedMount {
    identity: u64,
    component: Box<dyn Component>,
}

impl MountedComponent for OwnedMount {
    fn identity(&self) -> u64 {
        self.identity
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.component.render(width)
    }

    fn invalidate(&mut self) {
        self.component.invalidate();
    }

    fn render_cached(&mut self, context: &mut LayoutContext, width: usize) -> Vec<String> {
        context.render_cached_mounted(&mut *self.component, width, self.identity)
    }

    fn measure_width(&mut self, context: &mut LayoutContext, width: usize) -> usize {
        context.measure_width_mounted(&mut *self.component, width, self.identity)
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        context.layout_mounted(&mut *self.component, self.identity, allocation)
    }

    fn render_identity(&self) -> usize {
        self.component.render_identity()
    }

    fn contains_component(&self, identity: u64) -> bool {
        self.identity == identity || self.component.contains_component(identity)
    }
}

struct ChildMount {
    id: ContainerChildId,
    component: Box<dyn MountedComponent>,
}

pub struct Container {
    owner: u64,
    children: Vec<ChildMount>,
    next_mount: u64,
}

impl Container {
    pub fn new() -> Self {
        Self {
            owner: NEXT_CONTAINER_ID.fetch_add(1, Ordering::Relaxed),
            children: Vec::new(),
            next_mount: 0,
        }
    }

    /// Compatibility entry point for uniquely owned children.
    pub fn add_child(&mut self, component: Box<dyn Component>) -> ContainerChildId {
        let identity = NEXT_COMPONENT_ID.fetch_add(1, Ordering::Relaxed);
        self.push_mount(Box::new(OwnedMount {
            identity,
            component,
        }))
    }

    /// Mount a retained shared component. The handle remains usable by the
    /// caller and may be mounted more than once.
    pub fn add_shared_child<T: Component + 'static>(
        &mut self,
        handle: ComponentHandle<T>,
    ) -> ContainerChildId {
        self.push_mount(Box::new(SharedMount { handle }))
    }

    /// Mount an already type-erased retained component.
    pub fn add_component_ref(&mut self, handle: ComponentRef) -> ContainerChildId {
        self.push_mount(Box::new(ErasedSharedMount { handle }))
    }

    fn push_mount(&mut self, component: Box<dyn MountedComponent>) -> ContainerChildId {
        let id = ContainerChildId {
            owner: self.owner,
            mount: self.next_mount,
        };
        self.next_mount = self.next_mount.wrapping_add(1);
        self.children.push(ChildMount { id, component });
        id
    }

    pub fn remove_child(&mut self, id: ContainerChildId) -> bool {
        if id.owner != self.owner {
            return false;
        }
        if let Some(index) = self.children.iter().position(|child| child.id == id) {
            self.children.remove(index);
            return true;
        }
        false
    }

    /// Reference-style `removeChild(component)`: remove the first mount with
    /// this exact shared identity.
    pub fn remove_component<T: Component + 'static>(
        &mut self,
        handle: &ComponentHandle<T>,
    ) -> Option<ContainerChildId> {
        if let Some(index) = self
            .children
            .iter()
            .position(|child| child.component.identity() == handle.identity)
        {
            return Some(self.children.remove(index).id);
        }
        None
    }

    pub fn clear(&mut self) {
        self.children.clear();
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    pub(crate) fn child_identity(&self, index: usize) -> usize {
        self.children[index].component.render_identity()
    }

    pub fn contains_component_ref(&self, component: &ComponentRef) -> bool {
        self.children.iter().any(|child| {
            child.component.identity() == component.identity
                || child.component.contains_component(component.identity)
        })
    }

    pub(crate) fn render_child(&mut self, index: usize, width: usize) -> Vec<String> {
        self.children[index].component.render(width)
    }

    pub(crate) fn render_child_cached(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        width: usize,
    ) -> Vec<String> {
        self.children[index].component.render_cached(context, width)
    }

    pub(crate) fn measure_child_width(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        width: usize,
    ) -> usize {
        self.children[index].component.measure_width(context, width)
    }

    pub(crate) fn layout_child(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        self.children[index].component.layout(context, allocation)
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Container {
    fn invalidate(&mut self) {
        for child in &mut self.children {
            child.component.invalidate();
        }
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for child in &mut self.children {
            lines.extend(child.component.render(width));
        }
        lines
    }

    fn contains_component(&self, identity: u64) -> bool {
        self.children.iter().any(|child| {
            child.component.identity() == identity || child.component.contains_component(identity)
        })
    }
}
