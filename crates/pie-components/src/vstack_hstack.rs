//! VStack / HStack — stacked layout containers with gap and alignment
//! (reference `components/v-stack.js` / `h-stack.js`).

use pie_core::screen::composite_tui_line;
use pie_core::text::visible_width;

use crate::Component;
use crate::container::{ComponentHandle, Container, ContainerChildId};
use crate::layout::{LayoutAllocation, LayoutBox, LayoutContext};
use crate::stack::{StackEntry, StackViewport, allocate_stack_sizes};

/// Alignment of children within the cross axis.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Align {
    #[default]
    Stretch,
    Start,
    Center,
    End,
}

/// Shared stack state (children + per-child entries + gap/align).
pub struct StackData {
    container: Container,
    pub entries: Vec<StackEntry>,
    child_ids: Vec<ContainerChildId>,
    pub gap: usize,
    pub align: Align,
}

impl StackData {
    pub fn new(gap: usize, align: Align) -> Self {
        StackData {
            container: Container::new(),
            entries: Vec::new(),
            child_ids: Vec::new(),
            gap,
            align,
        }
    }

    pub fn add_child(&mut self, component: Box<dyn Component>) -> ContainerChildId {
        self.add_child_with_entry(component, StackEntry::auto())
    }

    pub fn add_child_with_entry(
        &mut self,
        component: Box<dyn Component>,
        entry: StackEntry,
    ) -> ContainerChildId {
        let id = self.container.add_child(component);
        self.entries.push(entry);
        self.child_ids.push(id);
        id
    }

    pub fn add_shared_child<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
    ) -> ContainerChildId {
        self.add_shared_child_with_entry(component, StackEntry::auto())
    }

    pub fn add_shared_child_with_entry<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
        entry: StackEntry,
    ) -> ContainerChildId {
        let id = self.container.add_shared_child(component);
        self.entries.push(entry);
        self.child_ids.push(id);
        id
    }

    pub fn remove_child(&mut self, id: ContainerChildId) {
        if let Some(index) = self.child_ids.iter().position(|candidate| *candidate == id) {
            self.container.remove_child(id);
            self.entries.remove(index);
            self.child_ids.remove(index);
        }
    }

    pub fn remove_component<T: Component + 'static>(&mut self, component: &ComponentHandle<T>) {
        if let Some(id) = self.container.remove_component(component)
            && let Some(index) = self.child_ids.iter().position(|candidate| *candidate == id)
        {
            self.entries.remove(index);
            self.child_ids.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.container.clear();
        self.entries.clear();
        self.child_ids.clear();
    }

    pub(crate) fn visible_indices(&self, viewport: StackViewport) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.is_visible(viewport).then_some(index))
            .collect()
    }

    pub(crate) fn render_identity(&self) -> usize {
        self as *const Self as usize
    }

    pub(crate) fn child_identity(&self, index: usize) -> usize {
        self.container.child_identity(index)
    }

    pub(crate) fn render_child(&mut self, index: usize, width: usize) -> Vec<String> {
        self.container.render_child(index, width)
    }

    pub(crate) fn measure_child_height(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        width: usize,
    ) -> usize {
        self.container
            .render_child_cached(index, context, width)
            .len()
    }

    pub(crate) fn measure_child_width(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        width: usize,
    ) -> usize {
        self.container.measure_child_width(index, context, width)
    }

    pub(crate) fn layout_child(
        &mut self,
        index: usize,
        context: &mut LayoutContext,
        allocation: LayoutAllocation,
    ) -> LayoutBox {
        self.container.layout_child(index, context, allocation)
    }
}

/// Vertical stack: children stacked top-to-bottom with `gap` empty lines
/// between, each clamped/grown/shrunk to its allocated row count.
pub struct VStack {
    pub data: StackData,
}

impl Default for VStack {
    fn default() -> Self {
        VStack {
            data: StackData::new(0, Align::Stretch),
        }
    }
}

impl VStack {
    pub fn new(gap: usize, align: Align) -> Self {
        VStack {
            data: StackData::new(gap, align),
        }
    }

    pub fn add_child(&mut self, component: Box<dyn Component>) -> ContainerChildId {
        self.data.add_child(component)
    }

    pub fn add_child_with_entry(
        &mut self,
        component: Box<dyn Component>,
        entry: StackEntry,
    ) -> ContainerChildId {
        self.data.add_child_with_entry(component, entry)
    }

    pub fn add_shared_child<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
    ) -> ContainerChildId {
        self.data.add_shared_child(component)
    }

    pub fn add_shared_child_with_entry<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
        entry: StackEntry,
    ) -> ContainerChildId {
        self.data.add_shared_child_with_entry(component, entry)
    }

    pub fn remove_child(&mut self, id: ContainerChildId) {
        self.data.remove_child(id);
    }

    pub fn remove_component<T: Component + 'static>(&mut self, component: &ComponentHandle<T>) {
        self.data.remove_component(component);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl Component for VStack {
    fn render(&mut self, width: usize) -> Vec<String> {
        let viewport_width = width.max(1);
        let viewport = StackViewport {
            width: viewport_width,
            height: usize::MAX,
        };
        let visible_indices = self.data.visible_indices(viewport);
        let rendered: Vec<Vec<String>> = visible_indices
            .iter()
            .map(|index| self.data.render_child(*index, viewport_width))
            .collect();
        let visible_entries = visible_indices
            .iter()
            .map(|index| self.data.entries[*index].clone())
            .collect::<Vec<_>>();
        let sizes = allocate_stack_sizes(
            &visible_entries,
            &rendered.iter().map(Vec::len).collect::<Vec<_>>(),
            None,
            self.data.gap,
        );
        let mut lines = Vec::new();
        for index in 0..rendered.len() {
            if index > 0 {
                for _ in 0..self.data.gap {
                    lines.push(String::new());
                }
            }
            let child_lines = &rendered[index][..sizes[index].min(rendered[index].len())];
            lines.extend(child_lines.iter().cloned());
            for _ in child_lines.len()..sizes[index] {
                lines.push(String::new());
            }
        }
        lines
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        context.layout_vstack(&mut self.data, allocation)
    }
}

/// Horizontal stack: children laid left-to-right, column widths allocated by
/// the stack algorithm, cross-axis alignment per `align`.
pub struct HStack {
    pub data: StackData,
}

impl Default for HStack {
    fn default() -> Self {
        HStack {
            data: StackData::new(0, Align::Stretch),
        }
    }
}

impl HStack {
    pub fn new(gap: usize, align: Align) -> Self {
        HStack {
            data: StackData::new(gap, align),
        }
    }

    pub fn add_child(&mut self, component: Box<dyn Component>) -> ContainerChildId {
        self.data.add_child(component)
    }

    pub fn add_child_with_entry(
        &mut self,
        component: Box<dyn Component>,
        entry: StackEntry,
    ) -> ContainerChildId {
        self.data.add_child_with_entry(component, entry)
    }

    pub fn add_shared_child<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
    ) -> ContainerChildId {
        self.data.add_shared_child(component)
    }

    pub fn add_shared_child_with_entry<T: Component + 'static>(
        &mut self,
        component: ComponentHandle<T>,
        entry: StackEntry,
    ) -> ContainerChildId {
        self.data.add_shared_child_with_entry(component, entry)
    }

    pub fn remove_child(&mut self, id: ContainerChildId) {
        self.data.remove_child(id);
    }

    pub fn remove_component<T: Component + 'static>(&mut self, component: &ComponentHandle<T>) {
        self.data.remove_component(component);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl Component for HStack {
    fn render(&mut self, width: usize) -> Vec<String> {
        let safe_width = width.max(1);
        let viewport = StackViewport {
            width: safe_width,
            height: usize::MAX,
        };
        let visible_indices = self.data.visible_indices(viewport);
        if visible_indices.is_empty() {
            return Vec::new();
        }
        // Intrinsic widths measured at the full width.
        let intrinsic_widths: Vec<usize> = visible_indices
            .iter()
            .map(|index| {
                self.data
                    .render_child(*index, safe_width)
                    .iter()
                    .map(|line| visible_width(line))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let visible_entries = visible_indices
            .iter()
            .map(|index| self.data.entries[*index].clone())
            .collect::<Vec<_>>();
        let widths = allocate_stack_sizes(
            &visible_entries,
            &intrinsic_widths,
            Some(safe_width),
            self.data.gap,
        );
        let rendered: Vec<Vec<String>> = visible_indices
            .iter()
            .enumerate()
            .map(|(rendered_index, child_index)| {
                if widths[rendered_index] == 0 {
                    Vec::new()
                } else {
                    self.data.render_child(*child_index, widths[rendered_index])
                }
            })
            .collect();
        let height = rendered.iter().map(Vec::len).max().unwrap_or(0);
        let mut result = vec![String::new(); height];
        let mut x = 0usize;
        for (index, lines) in rendered.iter().enumerate() {
            let child_width = widths[index];
            let offset = match self.data.align {
                Align::Center => (height.saturating_sub(lines.len())) / 2,
                Align::End => height.saturating_sub(lines.len()),
                Align::Stretch | Align::Start => 0,
            };
            for (row, line) in lines.iter().enumerate() {
                let target = row + offset;
                if target >= result.len() {
                    continue;
                }
                result[target] =
                    composite_tui_line(&result[target], line, x, child_width, safe_width);
            }
            x += child_width + self.data.gap;
        }
        result
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        context.layout_hstack(&mut self.data, allocation)
    }
}
