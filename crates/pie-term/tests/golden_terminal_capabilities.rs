//! M5 terminal capability detection/cache differential.

use std::sync::Arc;

use pie_core::terminal_image::ImageProtocol;
use pie_term::capabilities::{CapabilitiesCache, TerminalCapabilities, TerminalEnvironment};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/terminal-capabilities.json")).unwrap()
}

#[test]
fn oracle_is_exactly_pinned() {
    let root = fixture();
    assert_eq!(root["oracle"]["package"], "@earendil-works/pi-tui");
    assert_eq!(root["oracle"]["version"], "0.84.1");
    assert_eq!(
        root["oracle"]["files"]["terminal-image.js"],
        "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2"
    );
}

#[test]
fn capability_priority_matrix_matches() {
    for case in fixture()["cases"].as_array().unwrap() {
        let environment = TerminalEnvironment::from_pairs(
            false,
            case["env"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str().unwrap())),
        );
        let mut calls = 0_u32;
        let got = pie_term::capabilities::detect_capabilities(&environment, || {
            calls += 1;
            case["probeResult"].as_bool().unwrap()
        });
        assert_eq!(
            capabilities_json(&got),
            case["result"],
            "{}",
            case["label"].as_str().unwrap()
        );
        assert_eq!(calls, case["probeCalls"].as_u64().unwrap() as u32);
    }
}

#[test]
fn cache_is_lazy_identity_stable_resettable_and_overridable() {
    let expected = &fixture()["cache"];
    let cache = CapabilitiesCache::default();
    let kitty = TerminalEnvironment::from_pairs(false, [("TERM_PROGRAM", "kitty")]);
    let iterm = TerminalEnvironment::from_pairs(false, [("TERM_PROGRAM", "iTerm.app")]);

    let first = cache.get_or_detect(&kitty, || panic!("kitty never probes tmux"));
    let second = cache.get_or_detect(&kitty, || panic!("cached lookup never probes"));
    let stale = cache.get_or_detect(&iterm, || panic!("cached lookup never probes"));
    assert_eq!(capabilities_json(&first), expected["first"]);
    assert_eq!(Arc::ptr_eq(&first, &second), expected["firstSecondSame"]);
    assert_eq!(capabilities_json(&stale), expected["stale"]);
    assert_eq!(Arc::ptr_eq(&first, &stale), expected["firstStaleSame"]);

    cache.reset();
    let refreshed = cache.get_or_detect(&iterm, || panic!("iTerm never probes tmux"));
    assert_eq!(capabilities_json(&refreshed), expected["refreshed"]);
    assert_eq!(
        Arc::ptr_eq(&first, &refreshed),
        expected["refreshedSameAsFirst"]
    );

    let explicit = Arc::new(TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: true,
    });
    cache.set(Arc::clone(&explicit));
    let overridden = cache.get_or_detect(&kitty, || panic!("override is cached"));
    assert_eq!(capabilities_json(&overridden), expected["overridden"]);
    assert_eq!(
        Arc::ptr_eq(&explicit, &overridden),
        expected["overrideSameIdentity"]
    );
}

#[test]
fn windows_unknown_fallback_is_explicitly_truecolor_without_links() {
    let environment = TerminalEnvironment::from_pairs(true, std::iter::empty::<(&str, &str)>());
    assert_eq!(
        pie_term::capabilities::detect_capabilities(&environment, || false),
        TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: false,
        }
    );
}

fn capabilities_json(value: &TerminalCapabilities) -> serde_json::Value {
    serde_json::json!({
        "images": match value.images {
            Some(ImageProtocol::Kitty) => serde_json::Value::String("kitty".to_owned()),
            Some(ImageProtocol::ITerm2) => serde_json::Value::String("iterm2".to_owned()),
            None => serde_json::Value::Null,
        },
        "trueColor": value.true_color,
        "hyperlinks": value.hyperlinks,
    })
}
