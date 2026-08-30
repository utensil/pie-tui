//! M3 core differential suite against the pinned pi-tui 0.84.2 build.

use std::sync::OnceLock;

use pie_core::fuzzy::{fuzzy_filter, fuzzy_match};
use pie_core::keybindings::global::{get_keybindings, set_keybindings};
use pie_core::keybindings::{
    KeybindingShape, KeybindingsManager, ResolvedBinding, TUI_KEYBINDINGS,
};

static FIXTURE: OnceLock<serde_json::Value> = OnceLock::new();

fn fixture() -> &'static serde_json::Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/m3-core.json"))
            .expect("m3-core.json is valid JSON")
    })
}

#[test]
fn oracle_is_exactly_pinned() {
    let oracle = &fixture()["oracle"];
    assert_eq!(oracle["package"], "@earendil-works/pi-tui");
    assert_eq!(oracle["version"], "0.84.2");
    assert_eq!(
        oracle["files"]["fuzzy.js"],
        "4e8de99d7a73e192b1215d5e37c1cbd687be1c8917edfd0ceb7636f44352cbc8"
    );
    assert_eq!(
        oracle["files"]["keybindings.js"],
        "13d02095ac89383560b11f0a3bf5155c7911492af7e03bd41c9a80a1c2b3dc77"
    );
}

#[test]
fn fuzzy_match_vectors_are_bit_exact() {
    for case in fixture()["fuzzyMatch"].as_array().expect("cases") {
        let query = case["query"].as_str().expect("query");
        let text = case["text"].as_str().expect("text");
        let got = fuzzy_match(query, text);
        let want_matches = case["matches"].as_bool().expect("matches");
        assert_eq!(got.matches, want_matches, "fuzzyMatch({query:?}, {text:?})");
        let want_score = case["score"]
            .as_str()
            .expect("precision-preserving score")
            .parse::<f64>()
            .expect("score parses");
        assert_eq!(
            got.score.to_bits(),
            want_score.to_bits(),
            "fuzzyMatch({query:?}, {text:?}) score: got {}, want {}",
            got.score,
            want_score
        );
    }
}

#[test]
fn fuzzy_filter_vectors_preserve_order() {
    for case in fixture()["fuzzyFilter"].as_array().expect("cases") {
        let query = case["query"].as_str().expect("query");
        let items: Vec<String> = case["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|value| value.as_str().expect("item string").to_owned())
            .collect();
        let want: Vec<String> = case["result"]
            .as_array()
            .expect("result")
            .iter()
            .map(|value| value.as_str().expect("result string").to_owned())
            .collect();
        let got: Vec<String> = fuzzy_filter(&items, query, Clone::clone)
            .into_iter()
            .cloned()
            .collect();
        assert_eq!(got, want, "fuzzyFilter({query:?}) ordering");
    }
}

#[test]
fn keybindings_vectors_cover_shape_order_and_matching() {
    for case in fixture()["keybindings"].as_array().expect("cases") {
        let label = case["label"].as_str().expect("label");
        let user = string_pairs(&case["userBindings"]);
        let manager = KeybindingsManager::with_tui_defaults(user.clone());

        let got_definitions: Vec<(String, ResolvedBinding, String)> = TUI_KEYBINDINGS
            .iter()
            .map(|definition| {
                let keys = definition
                    .default_keys
                    .iter()
                    .map(|key| (*key).to_owned())
                    .collect::<Vec<_>>();
                (
                    definition.id.to_owned(),
                    match definition.default_shape {
                        KeybindingShape::Scalar => {
                            ResolvedBinding::Scalar(keys.into_iter().next().expect("scalar key"))
                        }
                        KeybindingShape::List => ResolvedBinding::List(keys),
                    },
                    definition.description.to_owned(),
                )
            })
            .collect();
        let want_definitions: Vec<(String, ResolvedBinding, String)> = case["definitions"]
            .as_array()
            .expect("definitions")
            .iter()
            .map(|definition| {
                (
                    definition["id"].as_str().expect("id").to_owned(),
                    resolved_binding(&definition["defaultKeys"]),
                    definition["description"]
                        .as_str()
                        .expect("description")
                        .to_owned(),
                )
            })
            .collect();
        assert_eq!(got_definitions, want_definitions, "{label}: definitions");

        for pair in case["keys"].as_array().expect("keys") {
            let pair = pair.as_array().expect("key pair");
            let id = pair[0].as_str().expect("id");
            assert_eq!(
                manager.get_keys(id),
                strings(&pair[1]),
                "{label}: keys {id}"
            );
            assert_eq!(
                manager.get_definition(id).map(|definition| definition.id),
                Some(id),
                "{label}: definition {id}"
            );
        }

        assert_eq!(
            manager.get_resolved_bindings(),
            resolved_pairs(&case["resolved"]),
            "{label}: resolved bindings"
        );
        assert_eq!(manager.get_user_bindings(), user, "{label}: user bindings");

        let got_conflicts: Vec<(String, Vec<String>)> = manager
            .get_conflicts()
            .into_iter()
            .map(|conflict| (conflict.key, conflict.keybindings))
            .collect();
        let want_conflicts: Vec<(String, Vec<String>)> = case["conflicts"]
            .as_array()
            .expect("conflicts")
            .iter()
            .map(|conflict| {
                (
                    conflict["key"].as_str().expect("key").to_owned(),
                    strings(&conflict["keybindings"]),
                )
            })
            .collect();
        assert_eq!(got_conflicts, want_conflicts, "{label}: conflicts");

        for check in case["checks"].as_array().expect("checks") {
            let data = check["data"].as_str().expect("data");
            let id = check["id"].as_str().expect("id");
            assert_eq!(
                manager.matches(data, id),
                check["matches"].as_bool().expect("matches"),
                "{label}: matches({data:?}, {id})"
            );
        }
    }
}

#[test]
fn global_manager_retains_identity_and_rebindings_are_live() {
    let oracle = &fixture()["globalKeybindings"];
    set_keybindings(KeybindingsManager::with_tui_defaults(Vec::new()));
    let retained = get_keybindings();
    assert_eq!(
        retained.matches("\x1b[A", "tui.select.up"),
        oracle["before"].as_bool().expect("before")
    );
    retained.set_user_bindings(vec![(
        "tui.select.up".to_owned(),
        vec!["ctrl+k".to_owned()],
    )]);
    let fetched_again = get_keybindings();
    assert_eq!(
        retained.ptr_eq(&fetched_again),
        oracle["sameIdentity"].as_bool().expect("same identity")
    );
    assert_eq!(
        retained.matches("\x1b[A", "tui.select.up"),
        oracle["afterOldKey"].as_bool().expect("old key")
    );
    assert_eq!(
        retained.matches("\x0b", "tui.select.up"),
        oracle["afterNewKey"].as_bool().expect("new key")
    );
    assert_eq!(
        ResolvedBinding::Scalar("ctrl+k".to_owned()),
        resolved_binding(&oracle["resolved"])
    );
}

#[test]
fn set_user_bindings_replaces_the_previous_map() {
    let case = fixture()["keybindings"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["label"] == "set-user-bindings-replaces")
        .expect("replacement case");
    let mut manager = KeybindingsManager::with_tui_defaults(vec![(
        "tui.select.cancel".to_owned(),
        vec!["escape".to_owned()],
    )]);
    manager.set_user_bindings(string_pairs(&case["observedUserBindings"]));
    assert_eq!(
        manager.get_user_bindings(),
        string_pairs(&case["observedUserBindings"])
    );
    assert!(!manager.matches("\x1b", "tui.select.cancel"));
    assert!(manager.matches("\x03", "tui.select.cancel"));
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect()
}

fn resolved_binding(value: &serde_json::Value) -> ResolvedBinding {
    match value {
        serde_json::Value::String(key) => ResolvedBinding::Scalar(key.clone()),
        serde_json::Value::Array(_) => ResolvedBinding::List(strings(value)),
        _ => panic!("resolved binding must be a string or array: {value}"),
    }
}

fn resolved_pairs(value: &serde_json::Value) -> Vec<(String, ResolvedBinding)> {
    value
        .as_array()
        .expect("resolved pairs")
        .iter()
        .map(|pair| {
            let pair = pair.as_array().expect("resolved pair");
            (
                pair[0].as_str().expect("resolved key").to_owned(),
                resolved_binding(&pair[1]),
            )
        })
        .collect()
}

fn string_pairs(value: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    value
        .as_array()
        .expect("pairs")
        .iter()
        .map(|pair| {
            let pair = pair.as_array().expect("pair");
            (
                pair[0].as_str().expect("pair key").to_owned(),
                strings(&pair[1]),
            )
        })
        .collect()
}
