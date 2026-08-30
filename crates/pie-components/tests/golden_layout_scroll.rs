//! Differential layout/ScrollView harness harvested from the pinned
//! `@earendil-works/pi-tui@0.84.1` distribution.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use pie_components::layout::{
    LayoutBox, LayoutFrame, ScrollViewId, ScrollbarGeometry, get_scroll_view_box,
    get_scroll_views_at, get_scrollbar_geometry, render_layout_frame,
};
use pie_components::{
    Align, Component, ComponentHandle, HStack, ScrollView, ScrollViewAxis, ScrollViewFollow,
    ScrollViewOptions, ScrollViewOverscroll, ScrollViewScrollbar, ScrollViewTimerHost,
    ScrollViewTimerId, ScrollbarStyle, SizeValue, StackEntry, VStack, allocate_stack_sizes,
};
use pie_core::screen::CURSOR_MARKER;
use serde_json::{Value, json};

static FIXTURE: OnceLock<Value> = OnceLock::new();

type TimerCallback = Box<dyn FnOnce()>;
type TimerEntries = BTreeMap<ScrollViewTimerId, (u64, TimerCallback)>;
type RequestLog = (Rc<RefCell<Vec<String>>>, Rc<dyn Fn()>);

fn fixture() -> &'static Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/layout-scroll-golden.json"))
            .expect("layout-scroll fixture")
    })
}

fn box_snapshot_named(
    layout_box: &LayoutBox,
    names: &HashMap<ScrollViewId, &'static str>,
) -> Value {
    json!({
        "rect": {
            "x": layout_box.rect.x,
            "y": layout_box.rect.y,
            "width": layout_box.rect.width,
            "height": layout_box.rect.height,
        },
        "clip": {
            "x": layout_box.clip.x,
            "y": layout_box.clip.y,
            "width": layout_box.clip.width,
            "height": layout_box.clip.height,
        },
        "lineOffset": layout_box.line_offset,
        "scrollView": layout_box.scroll_view.and_then(|id| names.get(&id).copied()),
        "layer": layout_box.layer,
        "children": layout_box.children.iter()
            .map(|child| box_snapshot_named(child, names))
            .collect::<Vec<_>>(),
    })
}

fn frame_snapshot_named(frame: &LayoutFrame, names: &HashMap<ScrollViewId, &'static str>) -> Value {
    json!({
        "width": frame.width,
        "height": frame.height,
        "lines": frame.lines,
        "primaryScrollView": frame.primary_scroll_view.and_then(|id| names.get(&id).copied()),
        "root": box_snapshot_named(&frame.root, names),
    })
}

fn frame_snapshot(frame: &LayoutFrame) -> Value {
    frame_snapshot_named(frame, &HashMap::new())
}

fn geometry_snapshot(geometry: Option<ScrollbarGeometry>) -> Value {
    geometry.map_or(Value::Null, |geometry| {
        json!({
            "column": geometry.column,
            "trackTop": geometry.track_top,
            "trackHeight": geometry.track_height,
            "thumbTop": geometry.thumb_top,
            "thumbHeight": geometry.thumb_height,
            "maxScrollTop": geometry.max_scroll_top,
        })
    })
}

struct Probe {
    name: &'static str,
    rows: Vec<String>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl Probe {
    fn new(name: &'static str, rows: &[&str], calls: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name,
            rows: rows.iter().map(|row| (*row).to_string()).collect(),
            calls: calls.clone(),
        }
    }
}

impl Component for Probe {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("{}:{width}", self.name));
        self.rows.clone()
    }
}

struct DynamicProbe {
    name: &'static str,
    rows: Rc<RefCell<Vec<String>>>,
    calls: Rc<RefCell<Vec<String>>>,
}

static OWNED_ALPHA_CALLS: AtomicUsize = AtomicUsize::new(0);
static OWNED_BETA_CALLS: AtomicUsize = AtomicUsize::new(0);

struct OwnedAlpha;

impl Component for OwnedAlpha {
    fn render(&mut self, _width: usize) -> Vec<String> {
        OWNED_ALPHA_CALLS.fetch_add(1, Ordering::Relaxed);
        vec!["alpha".to_string()]
    }
}

struct OwnedBeta;

impl Component for OwnedBeta {
    fn render(&mut self, _width: usize) -> Vec<String> {
        OWNED_BETA_CALLS.fetch_add(1, Ordering::Relaxed);
        vec!["beta".to_string()]
    }
}

impl Component for DynamicProbe {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.calls.borrow_mut().push(format!(
            "{}:{width}:{}",
            self.name,
            self.rows.borrow().len()
        ));
        self.rows.borrow().clone()
    }
}

#[derive(Default)]
struct FakeTimerHost {
    now: Cell<u64>,
    next_id: Cell<u64>,
    pending: RefCell<TimerEntries>,
    events: RefCell<Vec<String>>,
}

impl FakeTimerHost {
    fn advance(&self, milliseconds: u64) {
        self.now.set(self.now.get().saturating_add(milliseconds));
        let due = self
            .pending
            .borrow()
            .iter()
            .filter_map(|(id, (deadline, _))| (*deadline <= self.now.get()).then_some(*id))
            .collect::<Vec<_>>();
        for id in due {
            let Some((_, callback)) = self.pending.borrow_mut().remove(&id) else {
                continue;
            };
            self.events
                .borrow_mut()
                .push(format!("fire:{}", id.into_raw()));
            callback();
        }
    }

    fn pending_count(&self) -> usize {
        self.pending.borrow().len()
    }

    fn dequeue_first(&self) -> TimerCallback {
        let id = *self.pending.borrow().keys().next().expect("pending timer");
        let (_, callback) = self.pending.borrow_mut().remove(&id).expect("timer entry");
        self.events
            .borrow_mut()
            .push(format!("dequeue:{}", id.into_raw()));
        callback
    }
}

impl ScrollViewTimerHost for FakeTimerHost {
    fn set_timeout(&self, delay_ms: u64, callback: Box<dyn FnOnce()>) -> ScrollViewTimerId {
        let raw = self.next_id.get().wrapping_add(1);
        self.next_id.set(raw);
        let id = ScrollViewTimerId::from_raw(raw);
        self.pending
            .borrow_mut()
            .insert(id, (self.now.get().saturating_add(delay_ms), callback));
        self.events
            .borrow_mut()
            .push(format!("set:{raw}:{delay_ms}"));
        id
    }

    fn clear_timeout(&self, timer: ScrollViewTimerId) {
        let existed = self.pending.borrow_mut().remove(&timer).is_some();
        self.events
            .borrow_mut()
            .push(format!("clear:{}:{existed}", timer.into_raw()));
    }

    fn unref_timeout(&self, timer: ScrollViewTimerId) {
        self.events
            .borrow_mut()
            .push(format!("unref:{}", timer.into_raw()));
    }
}

fn no_render_request() -> Rc<dyn Fn()> {
    Rc::new(|| {})
}

fn hash_names(entries: &[(ScrollViewId, &'static str)]) -> HashMap<ScrollViewId, &'static str> {
    entries.iter().copied().collect()
}

fn request_log() -> RequestLog {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let callback_requests = requests.clone();
    let callback: Rc<dyn Fn()> = Rc::new(move || {
        callback_requests.borrow_mut().push("request".to_string());
    });
    (requests, callback)
}

#[test]
fn oracle_provenance_pins_every_layout_source_and_declaration() {
    assert_eq!(fixture()["oracle"]["package"], "@earendil-works/pi-tui");
    assert_eq!(fixture()["oracle"]["version"], "0.84.1");
    let expected = [
        (
            "index.d.ts",
            "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3",
        ),
        (
            "index.js",
            "80fdafd86a5649384fde4be965ccf4b2375b168fc375c074cfd9b5e9e9d59f89",
        ),
        (
            "tui.d.ts",
            "0b34c1688da2789a4d73d66a748287c8731f906bee480c5ef79633b66c9ab2f9",
        ),
        (
            "tui.js",
            "b425ed8e8535cf76deaeeea7de91edfda3d07606ee5ef9b2f02028583600a05a",
        ),
        (
            "layout.d.ts",
            "cfa0950012579f3912d7f6887a2b24f8618fa5e7eb0df15447ec992cf806a40a",
        ),
        (
            "layout.js",
            "fdc6c58b4245e735a0daabdc93201017e77cbbb01d7d440eda6427270556b2af",
        ),
        (
            "layout-node.d.ts",
            "8a19dd70f320755c3793cbec86902ad037afedd4151f1d174c0175cc281cf77d",
        ),
        (
            "layout-node.js",
            "73c3942b68d52ed29072f1f78184c99d405f9259bb4e24a1b6b0e3688381f7f5",
        ),
        (
            "utils.d.ts",
            "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
        ),
        (
            "utils.js",
            "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
        ),
        (
            "terminal-image.d.ts",
            "ba498675c6f16339fe04c329dcd95757743f0f6d22a18879b2fda6e9e8b4d8ec",
        ),
        (
            "terminal-image.js",
            "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2",
        ),
        (
            "components/stack.d.ts",
            "4a263287262dd550b75213d4234e84e9060ad4536d78d42bcb052e8012a7c212",
        ),
        (
            "components/stack.js",
            "02b4dafebc728f1c0e8d01b5cc330f82eb760c58bc71c5cc9bff6d98bf34dbf3",
        ),
        (
            "components/v-stack.d.ts",
            "7b695980cc75b8cafe67ee61108769550fa4effc851a83054776f32ba1bc42cb",
        ),
        (
            "components/v-stack.js",
            "7b6e6f6e1fbb037f33a8238ab02ec9e6e5f65fe49029bb430acd59c40148bfa4",
        ),
        (
            "components/h-stack.d.ts",
            "ad3796f7ee53e96f5e46d62f2409b6232da3f53364d4eaa8f741e5463ecf027d",
        ),
        (
            "components/h-stack.js",
            "b4cf1819879bcbbf20f2cfd3a5d5960b92c7f69308310d2ad44f0dbae4ed9c0f",
        ),
        (
            "components/scroll-view.d.ts",
            "7ee8cf2eced7d5a3cc68f0f054c9813f93b43019bdd72745b56f0c603b13f097",
        ),
        (
            "components/scroll-view.js",
            "796fdaa30bfb850df9d3e9647cd7b08c1bf3f3775335ce90a158c177c282f53f",
        ),
    ];
    for (file, digest) in expected {
        assert_eq!(fixture()["oracle"]["files"][file], digest, "{file}");
    }
}

#[test]
fn canonical_size_value_preserves_absolute_and_floors_percentage() {
    assert_eq!(
        fixture()["sizeValue"]["declaration"],
        "number | `${number}%`"
    );
    assert_eq!("37.5%".parse::<SizeValue>().unwrap().resolve(10), 3.0);
    assert_eq!(SizeValue::Absolute(2.75).resolve(10), 2.75);
}

#[test]
fn vertical_5x3_layout_clamps_and_scrolls_cursor_line_into_allocation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut stack = VStack::new(1, Align::Start);
    stack.add_child_with_entry(
        Box::new(Probe::new("top", &["top"], &calls)),
        StackEntry {
            basis: Some(1),
            shrink: 0,
            ..StackEntry::default()
        },
    );
    stack.add_child_with_entry(
        Box::new(Probe::new(
            "cursor",
            &["b0", "b1", &format!("b2{CURSOR_MARKER}"), "b3"],
            &calls,
        )),
        StackEntry {
            basis: Some(4),
            shrink: 1,
            min_size: 1,
            ..StackEntry::default()
        },
    );
    let frame = render_layout_frame(&mut stack, 5, 3, no_render_request());
    assert_eq!(
        frame_snapshot(&frame),
        fixture()["layout"]["vertical5x3"]["frame"]
    );
    assert_eq!(
        json!(calls.lock().expect("calls").clone()),
        fixture()["layout"]["vertical5x3"]["calls"]
    );
}

#[test]
fn horizontal_8x3_layout_matches_growth_end_alignment_and_cache_calls() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut stack = HStack::new(1, Align::End);
    stack.add_child_with_entry(
        Box::new(Probe::new("left", &["LL", "L2", "L3"], &calls)),
        StackEntry {
            grow: 1,
            max_size: 4,
            ..StackEntry::default()
        },
    );
    stack.add_child_with_entry(
        Box::new(Probe::new("right", &["R"], &calls)),
        StackEntry {
            grow: 2,
            ..StackEntry::default()
        },
    );
    let frame = render_layout_frame(&mut stack, 8, 3, no_render_request());
    assert_eq!(
        frame_snapshot(&frame),
        fixture()["layout"]["horizontal8x3"]["frame"]
    );
    assert_eq!(
        json!(calls.lock().expect("calls").clone()),
        fixture()["layout"]["horizontal8x3"]["calls"]
    );
}

#[test]
fn layout_clamps_zero_viewport_to_one_by_one() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut leaf = Probe::new("safe", &["x"], &calls);
    let frame = render_layout_frame(&mut leaf, 0, 0, no_render_request());
    assert_eq!(
        frame_snapshot(&frame),
        fixture()["layout"]["minimumViewport"]["frame"]
    );
    assert_eq!(
        json!(calls.lock().expect("calls").clone()),
        fixture()["layout"]["minimumViewport"]["calls"]
    );
}

#[test]
fn stack_allocation_matches_max_safe_and_contradictory_limit_oracles() {
    const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;
    let max_safe_shrink = allocate_stack_sizes(
        &[StackEntry {
            basis: Some(MAX_SAFE_INTEGER),
            shrink: MAX_SAFE_INTEGER,
            ..StackEntry::default()
        }],
        &[0],
        Some(0),
        0,
    );
    let contradictory_grow = allocate_stack_sizes(
        &[StackEntry {
            basis: Some(0),
            grow: 1,
            min_size: 5,
            max_size: 2,
            ..StackEntry::default()
        }],
        &[0],
        Some(10),
        0,
    );
    let contradictory_shrink = allocate_stack_sizes(
        &[StackEntry {
            basis: Some(10),
            min_size: 5,
            max_size: 2,
            ..StackEntry::default()
        }],
        &[0],
        Some(0),
        0,
    );
    let contradictory_auto = allocate_stack_sizes(
        &[StackEntry {
            grow: 1,
            min_size: 7,
            max_size: 3,
            ..StackEntry::default()
        }],
        &[2],
        Some(20),
        0,
    );
    assert_eq!(
        json!({
            "maxSafeShrink": max_safe_shrink,
            "contradictoryGrow": contradictory_grow,
            "contradictoryShrink": contradictory_shrink,
            "contradictoryAuto": contradictory_auto,
        }),
        fixture()["layout"]["stackEdgeCases"]
    );
}

#[test]
fn layout_strips_only_repeated_leading_osc133_shell_zone_prefixes() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut zones = Probe::new(
        "zones",
        &[
            "\x1b]133;A\x07alpha",
            "\x1b]133;B\x1b\\beta",
            "\x1b]133;C\x07\x1b]133;A\x1b\\\x1b]133;B\x07gamma",
            "prefix\x1b]133;A\x07inside",
            "\x1b]133;D\x07unsupported",
            "\x1b]133;Aunterminated",
        ],
        &calls,
    );
    let frame = render_layout_frame(&mut zones, 24, 6, no_render_request());
    assert_eq!(
        json!({
            "frame": frame_snapshot(&frame),
            "calls": calls.lock().expect("calls").clone(),
        }),
        fixture()["layout"]["shellZones"]
    );
}

#[test]
fn distinct_owned_zst_mounts_have_distinct_frame_cache_identities() {
    OWNED_ALPHA_CALLS.store(0, Ordering::Relaxed);
    OWNED_BETA_CALLS.store(0, Ordering::Relaxed);
    let alpha: Box<dyn Component> = Box::new(OwnedAlpha);
    let beta: Box<dyn Component> = Box::new(OwnedBeta);
    assert_eq!(alpha.render_identity(), beta.render_identity());

    let mut stack = VStack::new(0, Align::Stretch);
    stack.add_child_with_entry(
        alpha,
        StackEntry {
            basis: Some(1),
            ..StackEntry::default()
        },
    );
    stack.add_child_with_entry(
        beta,
        StackEntry {
            basis: Some(1),
            ..StackEntry::default()
        },
    );
    let frame = render_layout_frame(&mut stack, 6, 2, no_render_request());
    assert!(frame.lines[0].contains("alpha"));
    assert!(frame.lines[1].contains("beta"));
    assert_eq!(OWNED_ALPHA_CALLS.load(Ordering::Relaxed), 1);
    assert_eq!(OWNED_BETA_CALLS.load(Ordering::Relaxed), 1);
}

#[test]
fn stack_uses_container_identity_for_duplicate_first_removal_and_frame_cache() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let shared = ComponentHandle::new(Probe::new("shared", &["old"], &calls));
    let mut stack = VStack::new(0, Align::Stretch);
    stack.add_shared_child_with_entry(
        shared.clone(),
        StackEntry {
            basis: Some(1),
            ..StackEntry::default()
        },
    );
    stack.add_shared_child_with_entry(
        shared.clone(),
        StackEntry {
            basis: Some(2),
            ..StackEntry::default()
        },
    );
    let first = render_layout_frame(&mut stack, 6, 2, no_render_request());
    assert_eq!(first.lines.len(), 2);
    assert_eq!(calls.lock().expect("calls").as_slice(), ["shared:6"]);

    stack.remove_component(&shared);
    assert_eq!(stack.data.entries[0].basis, Some(2));
    shared.borrow_mut().rows = vec!["new".to_string()];
    calls.lock().expect("calls").clear();
    let second = render_layout_frame(&mut stack, 6, 1, no_render_request());
    assert!(second.lines[0].contains("new"));
    assert_eq!(calls.lock().expect("calls").as_slice(), ["shared:6"]);
}

#[test]
fn always_scrollbar_5x3_reserves_column_rounds_thumb_and_scrolls() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let child = Probe::new(
        "always-child",
        &["0000", "1111", "2222", "3333", "4444", "5555"],
        &calls,
    );
    let mut view = ScrollView::new(
        Box::new(child),
        ScrollViewOptions {
            scrollbar: ScrollViewScrollbar::Always,
            scrollbar_style: ScrollbarStyle::new(|_| "#".to_string()),
            ..ScrollViewOptions::default()
        },
    );
    let names = hash_names(&[(view.id(), "always")]);
    let (requests, callback) = request_log();
    let first = render_layout_frame(&mut view, 5, 3, callback.clone());
    let first_geometry = get_scrollbar_geometry(
        get_scroll_view_box(&first, view.id()).expect("always ScrollView box"),
    );
    let remainder = view.scroll_by(2);
    let second = render_layout_frame(&mut view, 5, 3, callback);
    let second_geometry = get_scrollbar_geometry(
        get_scroll_view_box(&second, view.id()).expect("always ScrollView box"),
    );
    let actual = json!({
        "first": frame_snapshot_named(&first, &names),
        "firstGeometry": geometry_snapshot(first_geometry),
        "remainder": remainder,
        "second": frame_snapshot_named(&second, &names),
        "secondGeometry": geometry_snapshot(second_geometry),
        "state": {
            "scrollTop": view.scroll_top(),
            "following": view.is_following_end(),
            "viewportHeight": view.viewport_height(),
        },
        "calls": calls.lock().expect("calls").clone(),
        "requests": requests.borrow().clone(),
    });
    assert_eq!(actual, fixture()["scroll"]["always5x3"]);
}

#[test]
fn auto_scrollbar_8x3_overlays_without_reserving_content_column() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let child = Probe::new(
        "auto-child",
        &[
            "row-0000", "row-1111", "row-2222", "row-3333", "row-4444", "row-5555",
        ],
        &calls,
    );
    let mut view = ScrollView::new(
        Box::new(child),
        ScrollViewOptions {
            scrollbar: ScrollViewScrollbar::Auto,
            scrollbar_style: ScrollbarStyle::new(|_| "#".to_string()),
            ..ScrollViewOptions::default()
        },
    );
    let names = hash_names(&[(view.id(), "auto")]);
    let (requests, callback) = request_log();
    let first = render_layout_frame(&mut view, 8, 3, callback.clone());
    let first_visible = view.is_scrollbar_visible();
    assert!(
        get_scrollbar_geometry(
            get_scroll_view_box(&first, view.id()).expect("initial auto ScrollView box")
        )
        .is_none()
    );
    let remainder = view.scroll_by(1);
    let second = render_layout_frame(&mut view, 8, 3, callback);
    let geometry = get_scrollbar_geometry(
        get_scroll_view_box(&second, view.id()).expect("auto ScrollView box"),
    );
    let actual = json!({
        "first": frame_snapshot_named(&first, &names),
        "firstVisible": first_visible,
        "remainder": remainder,
        "second": frame_snapshot_named(&second, &names),
        "secondVisible": view.is_scrollbar_visible(),
        "geometry": geometry_snapshot(geometry),
        "calls": calls.lock().expect("calls").clone(),
        "requests": requests.borrow().clone(),
    });
    assert_eq!(actual, fixture()["scroll"]["auto8x3"]);
}

#[test]
fn follow_end_detaches_and_reattaches_across_content_growth() {
    let rows = Rc::new(RefCell::new(
        ["0", "1", "2", "3", "4", "5"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    ));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let child = DynamicProbe {
        name: "follow",
        rows: rows.clone(),
        calls: calls.clone(),
    };
    let mut view = ScrollView::new(
        Box::new(child),
        ScrollViewOptions {
            follow: ScrollViewFollow::End,
            ..ScrollViewOptions::default()
        },
    );
    let (requests, callback) = request_log();
    let mut states = Vec::new();
    render_layout_frame(&mut view, 5, 3, callback.clone());
    states.push(json!({ "name": "attached", "scrollTop": view.scroll_top(), "following": view.is_following_end(), "viewportHeight": view.viewport_height() }));
    states.push(json!({ "name": "detach-remainder", "remainder": view.scroll_by(-1) }));
    states.push(json!({ "name": "detached", "scrollTop": view.scroll_top(), "following": view.is_following_end(), "viewportHeight": view.viewport_height() }));
    rows.borrow_mut().extend(["6".to_string(), "7".to_string()]);
    render_layout_frame(&mut view, 5, 3, callback.clone());
    states.push(json!({ "name": "growth-detached", "scrollTop": view.scroll_top(), "following": view.is_following_end(), "viewportHeight": view.viewport_height() }));
    view.scroll_to_end();
    states.push(json!({ "name": "reattached", "scrollTop": view.scroll_top(), "following": view.is_following_end(), "viewportHeight": view.viewport_height() }));
    rows.borrow_mut().extend(["8".to_string(), "9".to_string()]);
    render_layout_frame(&mut view, 5, 3, callback);
    states.push(json!({ "name": "growth-attached", "scrollTop": view.scroll_top(), "following": view.is_following_end(), "viewportHeight": view.viewport_height() }));
    assert_eq!(
        json!({
            "states": states,
            "calls": calls.borrow().clone(),
            "requests": requests.borrow().clone(),
        }),
        fixture()["scroll"]["followEnd"]
    );
}

#[test]
fn overscroll_remainder_preserves_direction_at_both_edges() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let options = [
        ScrollViewOptions::default(),
        ScrollViewOptions {
            overscroll: ScrollViewOverscroll::Contain,
            ..ScrollViewOptions::default()
        },
    ];
    let mut snapshots = Vec::new();
    for (index, options) in options.into_iter().enumerate() {
        let child = Probe::new(
            if index == 0 {
                "remainder-chain"
            } else {
                "remainder-contain"
            },
            &["0", "1", "2", "3", "4", "5"],
            &calls,
        );
        let mut view = ScrollView::new(Box::new(child), options);
        render_layout_frame(&mut view, 5, 3, no_render_request());
        let mut sequence = Vec::new();
        for delta in [-2, 5, -5] {
            sequence.push(json!({
                "delta": delta,
                "remainder": view.scroll_by(delta),
                "scrollTop": view.scroll_top(),
            }));
        }
        let overscroll = match view.overscroll() {
            ScrollViewOverscroll::Chain => "chain",
            ScrollViewOverscroll::Contain => "contain",
        };
        snapshots.push(json!({ "overscroll": overscroll, "sequence": sequence }));
    }
    assert_eq!(json!(snapshots), fixture()["scroll"]["remainders"]);
}

#[test]
fn nested_hit_testing_is_deepest_first_and_primary_is_explicit_inner() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let inner_child = Probe::new("inner-child", &["i0", "i1", "i2", "i3", "i4"], &calls);
    let inner = ScrollView::new(
        Box::new(inner_child),
        ScrollViewOptions {
            primary: true,
            ..ScrollViewOptions::default()
        },
    );
    let inner_id = inner.id();
    let top = Probe::new("nest-top", &["top"], &calls);
    let mut content = VStack::new(0, Align::Stretch);
    content.add_child(Box::new(top));
    content.add_child_with_entry(
        Box::new(inner),
        StackEntry {
            basis: Some(3),
            ..StackEntry::default()
        },
    );
    let mut outer = ScrollView::new(Box::new(content), ScrollViewOptions::default());
    let outer_id = outer.id();
    let names = hash_names(&[(outer_id, "outer"), (inner_id, "inner")]);
    let frame = render_layout_frame(&mut outer, 8, 4, no_render_request());
    let names_at = |x, y| {
        get_scroll_views_at(&frame, x, y)
            .into_iter()
            .map(|id| names[&id])
            .collect::<Vec<_>>()
    };
    assert_eq!(
        json!({
            "frame": frame_snapshot_named(&frame, &names),
            "at2x2": names_at(2, 2),
            "at2x0": names_at(2, 0),
        }),
        fixture()["scroll"]["nestedHit"]
    );
}

#[test]
fn explicit_primary_overrides_the_naturally_selected_first_sibling() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let natural = ScrollView::new(
        Box::new(Probe::new("natural", &["n0", "n1"], &calls)),
        ScrollViewOptions::default(),
    );
    let natural_id = natural.id();
    let explicit = ScrollView::new(
        Box::new(Probe::new("explicit", &["e0", "e1"], &calls)),
        ScrollViewOptions {
            primary: true,
            ..ScrollViewOptions::default()
        },
    );
    let explicit_id = explicit.id();
    let names = hash_names(&[(natural_id, "natural"), (explicit_id, "explicit")]);
    let mut stack = VStack::new(0, Align::Stretch);
    stack.add_child_with_entry(
        Box::new(natural),
        StackEntry {
            basis: Some(2),
            ..StackEntry::default()
        },
    );
    stack.add_child_with_entry(
        Box::new(explicit),
        StackEntry {
            basis: Some(2),
            ..StackEntry::default()
        },
    );
    let frame = render_layout_frame(&mut stack, 8, 4, no_render_request());
    assert_eq!(frame.primary_scroll_view, Some(explicit_id));
    assert_eq!(
        json!({
            "frame": frame_snapshot_named(&frame, &names),
            "calls": calls.lock().expect("calls").clone(),
        }),
        fixture()["scroll"]["explicitPrimary"]
    );
}

#[test]
fn fake_clock_hides_auto_scrollbar_requests_render_and_cancels_timer() {
    let host = Rc::new(FakeTimerHost::default());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let child = Probe::new("timer", &["0", "1", "2", "3", "4"], &calls);
    let mut view = ScrollView::with_timer_host(
        Box::new(child),
        ScrollViewOptions {
            scrollbar: ScrollViewScrollbar::Auto,
            scrollbar_hide_delay_ms: 10,
            ..ScrollViewOptions::default()
        },
        host.clone(),
    );
    let (requests, callback) = request_log();
    render_layout_frame(&mut view, 5, 2, callback);
    view.set_scrollbar_active(true);
    let active_visible = view.is_scrollbar_visible();
    view.set_scrollbar_active(false);
    let waiting_visible = view.is_scrollbar_visible();
    host.advance(9);
    let before_deadline_visible = view.is_scrollbar_visible();
    host.advance(1);
    let hidden_visible = view.is_scrollbar_visible();
    view.scroll_by(1);
    let scrolled_visible = view.is_scrollbar_visible();
    view.set_scrollbar(ScrollViewScrollbar::Hidden);
    assert_eq!(
        json!({
            "activeVisible": active_visible,
            "waitingVisible": waiting_visible,
            "beforeDeadlineVisible": before_deadline_visible,
            "hiddenVisible": hidden_visible,
            "scrolledVisible": scrolled_visible,
            "requests": requests.borrow().clone(),
            "events": host.events.borrow().clone(),
            "pendingTimers": host.pending_count(),
        }),
        fixture()["scroll"]["fakeClock"]
    );
}

#[test]
fn stale_dequeued_timer_cannot_hide_or_clear_the_restarted_current_timer() {
    let host = Rc::new(FakeTimerHost::default());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let child = Probe::new("restart", &["0", "1", "2", "3", "4"], &calls);
    let mut view = ScrollView::with_timer_host(
        Box::new(child),
        ScrollViewOptions {
            scrollbar: ScrollViewScrollbar::Auto,
            scrollbar_hide_delay_ms: 5,
            ..ScrollViewOptions::default()
        },
        host.clone(),
    );
    let (requests, callback) = request_log();
    render_layout_frame(&mut view, 5, 2, callback);

    view.scroll_by(1);
    let stale = host.dequeue_first();
    view.scroll_by(1);
    assert!(view.is_scrollbar_visible());
    assert_eq!(host.pending_count(), 1);
    assert_eq!(requests.borrow().len(), 2);

    stale();
    assert!(view.is_scrollbar_visible());
    assert_eq!(host.pending_count(), 1);
    assert_eq!(requests.borrow().len(), 2);

    let current = host.dequeue_first();
    current();
    assert!(!view.is_scrollbar_visible());
    assert_eq!(host.pending_count(), 0);
    assert_eq!(requests.borrow().len(), 3);
    drop(view);
    assert_eq!(
        host.events.borrow().as_slice(),
        [
            "set:1:5",
            "unref:1",
            "dequeue:1",
            "clear:1:false",
            "set:2:5",
            "unref:2",
            "dequeue:2",
        ]
    );
}

#[test]
fn drop_cancels_pending_and_dequeued_current_timers_before_callbacks_run() {
    let host = Rc::new(FakeTimerHost::default());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (requests, callback) = request_log();
    {
        let child = Probe::new("drop-pending", &["0", "1", "2", "3"], &calls);
        let mut view = ScrollView::with_timer_host(
            Box::new(child),
            ScrollViewOptions {
                scrollbar: ScrollViewScrollbar::Auto,
                scrollbar_hide_delay_ms: 5,
                ..ScrollViewOptions::default()
            },
            host.clone(),
        );
        render_layout_frame(&mut view, 5, 2, callback);
        view.scroll_by(1);
        assert_eq!(host.pending_count(), 1);
    }
    assert_eq!(host.pending_count(), 0);
    host.advance(5);
    assert_eq!(requests.borrow().as_slice(), ["request"]);
    assert_eq!(
        host.events.borrow().as_slice(),
        ["set:1:5", "unref:1", "clear:1:true"]
    );

    let dequeued = {
        let child = Probe::new("drop-dequeued", &["0", "1", "2", "3"], &calls);
        let mut view = ScrollView::with_timer_host(
            Box::new(child),
            ScrollViewOptions {
                scrollbar: ScrollViewScrollbar::Auto,
                scrollbar_hide_delay_ms: 5,
                ..ScrollViewOptions::default()
            },
            host.clone(),
        );
        render_layout_frame(&mut view, 5, 2, no_render_request());
        view.scroll_by(1);
        let dequeued = host.dequeue_first();
        drop(view);
        dequeued
    };
    dequeued();
    assert_eq!(requests.borrow().as_slice(), ["request"]);
    assert_eq!(
        host.events.borrow().as_slice(),
        [
            "set:1:5",
            "unref:1",
            "clear:1:true",
            "set:2:5",
            "unref:2",
            "dequeue:2",
            "clear:2:false",
        ]
    );
}

#[test]
fn scroll_view_rejects_axis_and_all_mutators_with_exact_messages() {
    struct Empty;
    impl Component for Empty {
        fn render(&mut self, _width: usize) -> Vec<String> {
            vec!["child".to_string()]
        }
    }

    let mut view = ScrollView::new(Box::new(Empty), ScrollViewOptions::default());
    let other = Empty;
    let errors = vec![
        (
            "addChild",
            view.add_child(Box::new(Empty)).unwrap_err().to_string(),
        ),
        (
            "removeChild",
            view.remove_child(&other).unwrap_err().to_string(),
        ),
        ("clear", view.clear().unwrap_err().to_string()),
        (
            "axis",
            ScrollView::try_new(
                Box::new(Empty),
                ScrollViewOptions {
                    axis: ScrollViewAxis::Horizontal,
                    ..ScrollViewOptions::default()
                },
            )
            .err()
            .expect("horizontal axis rejected")
            .to_string(),
        ),
    ];
    assert_eq!(
        json!(
            errors
                .into_iter()
                .map(|(name, message)| json!({ "name": name, "message": message }))
                .collect::<Vec<_>>()
        ),
        fixture()["scroll"]["errors"]
    );
    assert_eq!(view.render(5), ["child"]);
}
