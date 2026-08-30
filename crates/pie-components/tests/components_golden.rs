//! Components golden differential harness — replays the exact scripted render
//! sequences recorded by `tools/golden/gen-golden-components.mjs` against the
//! pinned pi-tui build (0.84.1) and requires byte-identical output from the
//! Rust ports.
//!
//! Fixture: `tests/fixtures/components-golden.json`. Regenerate with
//! `PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-components.mjs`.

use std::sync::OnceLock;

use pie_components::{
    BoxComponent, Component, HStack, Loader, Spacer, StackEntry, Text, TruncatedText, VStack,
};

static FIXTURE: OnceLock<serde_json::Value> = OnceLock::new();

fn fixture() -> &'static serde_json::Value {
    FIXTURE.get_or_init(|| {
        let raw = include_str!("fixtures/components-golden.json");
        serde_json::from_str(raw).expect("components-golden.json is valid JSON")
    })
}

fn reference_version() -> &'static str {
    fixture()["reference"].as_str().expect("reference version")
}

fn golden(case: &str) -> Vec<Vec<String>> {
    let cases = fixture()["cases"].as_array().expect("cases array");
    let found = cases
        .iter()
        .find(|c| c["name"] == *case)
        .unwrap_or_else(|| panic!("case {case} missing from fixture"));
    serde_json::from_value(found["outputs"].clone()).expect("case outputs shape")
}

/// Assert `outputs` equals the golden for `case`, with a byte-level report.
fn check(case: &str, outputs: &[Vec<String>]) {
    let expect = golden(case);
    let ver = reference_version();
    assert_eq!(
        outputs.len(),
        expect.len(),
        "{case}: render-call count differs (reference v{ver})"
    );
    for (call, (actual, expected)) in outputs.iter().zip(expect.iter()).enumerate() {
        if actual != expected {
            panic!(
                "{case}: render #{call} differs (reference v{ver})\nexpected:\n{expected:#?}\nactual:\n{actual:#?}"
            );
        }
    }
}

fn bg_green() -> Box<dyn Fn(&str) -> String + Send> {
    Box::new(|s: &str| format!("\x1b[42m{s}\x1b[0m"))
}

fn cyan() -> Box<dyn Fn(&str) -> String + Send> {
    Box::new(|s: &str| format!("\x1b[36m{s}\x1b[0m"))
}

fn dim() -> Box<dyn Fn(&str) -> String + Send> {
    Box::new(|s: &str| format!("\x1b[2m{s}\x1b[0m"))
}

#[test]
fn text_default_plain() {
    let mut c = Text::new("hello world\nsecond line");
    let outputs = vec![c.render(20), c.render(40)];
    check("text-default-plain", &outputs);
}

#[test]
fn text_ansi_cjk() {
    let mut c = Text::new("mix \x1b[4munder\u{6f22}\u{5b57}\x1b[24m tail\n\ttabbed");
    let outputs = vec![c.render(12), c.render(30)];
    check("text-ansi-cjk", &outputs);
}

#[test]
fn text_bgfn() {
    let mut c = Text::with_bg_fn("bg text here", 1, 1, bg_green());
    let outputs = vec![c.render(16)];
    check("text-bgfn", &outputs);
}

#[test]
fn text_settext_cache() {
    let mut c = Text::with_padding("before", 0, 0);
    let first = c.render(20);
    c.set_text("after with longer content wrapping");
    let outputs = vec![first, c.render(20)];
    check("text-settext-cache", &outputs);
}

#[test]
fn text_empty_whitespace() {
    let mut c = Text::new("  \n\t ");
    let outputs = vec![c.render(10)];
    check("text-empty-whitespace", &outputs);
}

#[test]
fn truncated_default() {
    let mut c = TruncatedText::new("a very long line that will definitely need truncation");
    let outputs = vec![c.render(20), c.render(80)];
    check("truncated-default", &outputs);
}

#[test]
fn truncated_ansi_newline_cjk() {
    let mut c = TruncatedText::with_padding(
        "\x1b[31mred\u{6f22}\u{5b57} styled\x1b[0m tail\nhidden second",
        2,
        1,
    );
    let outputs = vec![c.render(18)];
    check("truncated-ansi-newline-cjk", &outputs);
}

#[test]
fn spacer_1_3() {
    let mut c = Spacer::new(1);
    let first = c.render(10);
    c.set_lines(3);
    let outputs = vec![first, c.render(10)];
    check("spacer-1-3", &outputs);
}

#[test]
fn box_children() {
    let mut b = BoxComponent::new(1, 1);
    b.add_child(Box::new(Text::with_padding("one", 0, 0)));
    b.add_child(Box::new(Text::with_padding("two", 0, 0)));
    let mut c = b;
    let outputs = vec![c.render(14)];
    check("box-children", &outputs);
}

#[test]
fn box_bgfn() {
    let mut b = BoxComponent::with_bg_fn(2, 1, bg_green());
    b.add_child(Box::new(Text::with_padding("boxed", 0, 0)));
    let mut c = b;
    let outputs = vec![c.render(16)];
    check("box-bgfn", &outputs);
}

#[test]
fn box_empty() {
    let mut c = BoxComponent::new(1, 1);
    let outputs = vec![c.render(10)];
    check("box-empty", &outputs);
}

#[test]
fn vstack_gap0() {
    let mut v = VStack::new(0, pie_components::Align::Stretch);
    v.add_child(Box::new(Text::with_padding("top", 0, 0)));
    v.add_child(Box::new(Spacer::new(1)));
    v.add_child(Box::new(Text::with_padding("bottom", 0, 0)));
    let mut c = v;
    let outputs = vec![c.render(12)];
    check("vstack-gap0", &outputs);
}

#[test]
fn vstack_gap2_grow() {
    let mut v = VStack::new(2, pie_components::Align::Stretch);
    v.add_child_with_entry(
        Box::new(Text::with_padding("fixed", 0, 0)),
        StackEntry::auto(),
    );
    v.add_child_with_entry(
        Box::new(Text::with_padding("grows", 0, 0)),
        StackEntry {
            grow: 1,
            ..StackEntry::auto()
        },
    );
    let mut c = v;
    let outputs = vec![c.render(10)];
    check("vstack-gap2-grow", &outputs);
}

#[test]
fn hstack_align_default() {
    let mut h = HStack::new(2, pie_components::Align::Stretch);
    h.add_child(Box::new(Text::with_padding("L1\nL2\nL3", 0, 0)));
    h.add_child(Box::new(Text::with_padding("R1\nR2", 0, 0)));
    let mut c = h;
    let outputs = vec![c.render(20)];
    check("hstack-align-default", &outputs);
}

#[test]
fn hstack_align_center_end() {
    let mut h = HStack::new(1, pie_components::Align::Center);
    h.add_child(Box::new(Text::with_padding("a\nbb\nccc", 0, 0)));
    h.add_child(Box::new(Text::with_padding("X\nY", 0, 0)));
    let mut c = h;
    let outputs = vec![c.render(14)];
    check("hstack-align-center-end", &outputs);
}

#[test]
fn loader_frames() {
    let mut c = Loader::with_colors("Loading stuff...", Some(cyan()), Some(dim()), None);
    let first = c.render(24);
    // Reference script sets currentFrame = 1 directly, then = 5; equivalent
    // states reached through the public tick (frames.len() > 1).
    c.advance_frame();
    let second = c.render(24);
    for _ in 0..4 {
        c.advance_frame();
    }
    let third = c.render(24);
    let outputs = vec![first, second, third];
    check("loader-frames", &outputs);
}
