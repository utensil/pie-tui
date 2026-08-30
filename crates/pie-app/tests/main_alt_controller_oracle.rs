//! Contract guard for the pinned Main/Alt controller oracle.
//!
//! Regenerate with:
//! `PI_TUI_DIST=<pi-tui-dist> node tools/golden/gen-golden-main-alt-controller.mjs`.

use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/main-alt-controller.json")).unwrap()
}

fn case<'a>(root: &'a Value, name: &str) -> &'a Value {
    root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing oracle case {name}"))
}

#[test]
fn oracle_pins_the_complete_reachable_runtime_and_type_closure() {
    let root = fixture();
    assert_eq!(root["reference"]["package"], "@earendil-works/pi-tui");
    assert_eq!(root["reference"]["version"], "0.84.1");
    assert_eq!(
        root["reference"]["widthDependency"],
        serde_json::json!({
            "package": "get-east-asian-width",
            "version": "1.6.0",
        })
    );
    assert_eq!(
        root["reference"]["runtimeClosure"],
        serde_json::json!([
            "components/alt-screen-flash.js",
            "components/scroll-view.js",
            "components/stack.js",
            "keybindings.js",
            "keys.js",
            "layout-node.js",
            "layout.js",
            "terminal-colors.js",
            "terminal-image.js",
            "tui-alt-screen.js",
            "tui-main-screen.js",
            "tui.js",
            "utils.js",
        ])
    );
    assert_eq!(
        root["reference"]["sourceDigests"]
            .as_object()
            .unwrap()
            .len(),
        34
    );
}

#[test]
fn oracle_covers_every_bounded_main_alt_family() {
    let root = fixture();
    let names: Vec<&str> = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "main-lifecycle-diff-resize-cursor-stop",
            "main-long-document-viewport-and-preserved-stop",
            "alt-raw-unregistered-kitty-lines-are-not-owned",
            "main-kitty-ownership-and-render-state-restore",
            "alt-kitty-offscreen-cache-eviction-and-revisit",
            "alt-lifecycle-diff-resize-preserved-stop",
            "alt-layout-root-focus-overlay-and-main-screen-restore",
            "alt-scroll-mouse-release-filter-and-live-keybindings",
            "alt-selection-clipboard-granularity-and-focus-out",
            "alt-kitty-transmission-placement-and-teardown-ownership",
            "alt-iterm-capability-suspension-and-unpreserved-stop",
            "alt-multiplexer-button-motion-lifecycle",
        ]
    );
}

#[test]
fn main_oracle_preserves_scrollback_cursor_and_image_ownership_facts() {
    let root = fixture();
    let lifecycle = case(&root, "main-lifecycle-diff-resize-cursor-stop");
    assert_eq!(lifecycle["mode"], "regular");
    assert_eq!(lifecycle["fullRedraws"], 2);
    assert_eq!(lifecycle["initial"]["terminal"][0][0], "start");
    assert_eq!(lifecycle["initial"]["terminal"][4][0], "show-cursor");
    assert_eq!(lifecycle["stop"]["terminal"][3][0], "show-cursor");
    assert_eq!(lifecycle["stop"]["terminal"][4][0], "stop");

    let long = case(&root, "main-long-document-viewport-and-preserved-stop");
    assert_eq!(long["initial"]["state"]["previousViewportTop"], 2);
    assert_eq!(long["append"]["state"]["previousViewportTop"], 4);
    assert_eq!(long["append"]["state"]["cursorRow"], 6);

    let image = case(&root, "main-kitty-ownership-and-render-state-restore");
    assert!(
        image["captured"]["previousLines"][0]
            .as_str()
            .unwrap()
            .contains("i=7")
    );
    assert_eq!(image["restored"]["previousLines"][0], "");
    assert!(
        image["removal"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("a=d,d=I,i=7")
    );
}

#[test]
fn alt_oracle_pins_scroll_layout_selection_images_and_teardown_order() {
    let root = fixture();
    let raw_kitty = case(&root, "alt-raw-unregistered-kitty-lines-are-not-owned");
    let raw_writes = raw_kitty["renderWrites"].as_array().unwrap();
    assert_eq!(raw_writes.len(), 17);
    assert!(
        raw_writes
            .iter()
            .all(|write| !write.as_str().unwrap().contains("\u{1b}_Ga=d,d=I"))
    );

    let lifecycle = case(&root, "alt-lifecycle-diff-resize-preserved-stop");
    assert_eq!(lifecycle["mode"], "fullscreen");
    assert_eq!(lifecycle["fullRedraws"], 2);
    assert_eq!(lifecycle["stop"]["terminal"][1][0], "show-cursor");
    assert_eq!(lifecycle["stop"]["terminal"][2][0], "stop");
    assert!(
        lifecycle["stop"]["terminal"][3][1]
            .as_str()
            .unwrap()
            .contains("\u{1b}[?1049l")
    );

    let layout = case(
        &root,
        "alt-layout-root-focus-overlay-and-main-screen-restore",
    );
    assert_eq!(layout["identityNoop"]["trace"], serde_json::json!([]));
    assert_eq!(
        layout["invalidate"]["trace"],
        serde_json::json!([["layout", "invalidate"], ["overlay", "invalidate"]])
    );
    assert_eq!(layout["restoredChildren"]["childFocused"], true);

    let scroll = case(
        &root,
        "alt-scroll-mouse-release-filter-and-live-keybindings",
    );
    let states = scroll["states"].as_array().unwrap();
    let tops: Vec<u64> = states
        .iter()
        .map(|state| state["viewportTop"].as_u64().unwrap())
        .collect();
    assert_eq!(tops, [2, 2, 1, 1, 0, 4, 0]);
    assert_eq!(states[3]["phase"]["trace"][1][1], "input");

    let selection = case(&root, "alt-selection-clipboard-granularity-and-focus-out");
    assert!(
        selection["clicks"][1]["phase"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("YWxwaGE=")
    );
    assert!(
        selection["clicks"][2]["phase"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("YWxwaGEgYmV0YQ==")
    );

    let kitty = case(
        &root,
        "alt-kitty-transmission-placement-and-teardown-ownership",
    );
    assert!(
        kitty["retransmit"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("a=d,d=a")
    );
    assert!(
        kitty["stop"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("a=d,d=A")
    );

    let eviction = case(&root, "alt-kitty-offscreen-cache-eviction-and-revisit");
    assert!(
        !eviction["atBound"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("a=d,d=I")
    );
    assert!(
        eviction["eviction"]["terminal"][0][1]
            .as_str()
            .unwrap()
            .contains("\u{1b}_Ga=d,d=I,i=1,q=2\u{1b}\\")
    );
    let revisit = eviction["revisit"]["terminal"][0][1].as_str().unwrap();
    assert!(revisit.contains("\u{1b}_Ga=d,d=I,i=2,q=2\u{1b}\\"));
    assert!(revisit.contains("\u{1b}_Ga=T,f=100,q=2,c=1,r=1,i=1;QUFBQQ==\u{1b}\\"));
    assert!(!revisit.contains("\u{1b}_Ga=p,q=2,i=1"));

    let iterm = case(
        &root,
        "alt-iterm-capability-suspension-and-unpreserved-stop",
    );
    assert!(iterm["during"]["capabilities"]["images"].is_null());
    assert!(
        iterm["during"]["terminal"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event
                .get(1)
                .and_then(Value::as_str)
                .is_some_and(|write| write.contains("\u{1b}]1337;File=")))
    );
    assert_eq!(iterm["restoredCapabilities"]["images"], "iterm2");
    assert_eq!(iterm["stop"]["terminal"][1][0], "show-cursor");
    assert_eq!(iterm["stop"]["terminal"][2][0], "stop");
}
