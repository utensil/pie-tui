use std::cell::Cell;
use std::rc::Rc;

use pie_components::layout::{LayoutAllocation, LayoutBox, LayoutContext, render_layout_frame};
use pie_components::{
    Component, ComponentHandle, Container, OverlayAnchor, OverlayOptions, Tui, ViewportTui,
};

#[derive(Default)]
struct FocusProbe {
    focused: bool,
    inputs: Vec<String>,
    renders: usize,
}

struct LayoutProbe {
    layouts: Rc<Cell<usize>>,
}

impl Component for LayoutProbe {
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec!["layout".to_owned()]
    }

    fn layout(&mut self, context: &mut LayoutContext, allocation: LayoutAllocation) -> LayoutBox {
        self.layouts.set(self.layouts.get() + 1);
        context.layout_leaf(self, allocation)
    }
}

impl Component for FocusProbe {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.renders += 1;
        vec![format!("{width}")]
    }

    fn handle_input(&mut self, data: &str) {
        self.inputs.push(data.to_owned());
    }

    fn focused(&self) -> Option<bool> {
        Some(self.focused)
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.focused = focused;
        true
    }
}

#[test]
fn retained_component_ref_preserves_identity_focus_and_mutation() {
    let handle = ComponentHandle::new(FocusProbe::default());
    let first = handle.as_component_ref();
    let second = handle.as_component_ref();
    assert!(first.ptr_eq(&second));
    assert_eq!(first.identity(), second.identity());

    assert_eq!(first.focused(), Some(false));
    assert!(first.set_focused(true));
    first.handle_input("x");
    assert_eq!(second.render(7), vec!["7"]);

    let probe = handle.borrow();
    assert!(probe.focused);
    assert_eq!(probe.inputs, ["x"]);
    assert_eq!(probe.renders, 1);
}

#[test]
fn erased_mount_is_nested_identity_safe() {
    let leaf = ComponentHandle::new(FocusProbe::default());
    let leaf_ref = leaf.as_component_ref();
    let mut nested = Container::new();
    nested.add_component_ref(leaf_ref.clone());
    assert!(nested.contains_component_ref(&leaf_ref));

    let nested = ComponentHandle::new(nested);
    let nested_ref = nested.as_component_ref();
    assert!(nested_ref.contains_component_ref(&leaf_ref));
    let mut root = Container::new();
    let mount = root.add_component_ref(nested_ref.clone());
    assert!(root.contains_component_ref(&nested_ref));
    assert!(root.contains_component_ref(&leaf_ref));
    assert_eq!(root.render(4), vec!["4"]);
    assert!(root.remove_child(mount));
    assert!(!root.contains_component_ref(&leaf_ref));
}

#[test]
fn retained_component_ref_dispatches_custom_layout_without_changing_identity() {
    let layouts = Rc::new(Cell::new(0));
    let handle = ComponentHandle::new(LayoutProbe {
        layouts: Rc::clone(&layouts),
    });
    let mut root = handle.as_component_ref();
    let identity = root.identity();
    let frame = render_layout_frame(&mut root, 8, 3, Rc::new(|| {}));

    assert_eq!(layouts.get(), 1);
    assert_eq!(root.identity(), identity);
    assert!(frame.lines[0].contains("layout"));
}

#[test]
fn canonical_structural_traits_are_object_safe() {
    fn accept_tui(_: &dyn Tui) {}
    fn accept_viewport(_: &dyn ViewportTui) {}
    let _ = accept_tui as fn(&dyn Tui);
    let _ = accept_viewport as fn(&dyn ViewportTui);

    let options = OverlayOptions {
        anchor: OverlayAnchor::BottomRight,
        non_capturing: true,
        ..OverlayOptions::default()
    };
    assert_eq!(options.anchor, OverlayAnchor::BottomRight);
    assert!(options.non_capturing);
}
