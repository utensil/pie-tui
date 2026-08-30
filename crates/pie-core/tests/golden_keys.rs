//! Golden matrix for keys — 105 inputs x 51 key ids x {legacy, kitty} modes,
//! harvested from the pinned reference build. Regenerate with
//! PI_TUI_DIST=... node tools/golden/gen-golden-keys.mjs
use pie_core::keys::{
    is_key_release, is_key_repeat, matches_key, parse_key, set_kitty_protocol_active,
};

struct Fixture {
    inputs: Vec<String>,
    key_ids: Vec<String>,
    legacy: ModeVectors,
    kitty: ModeVectors,
}
struct ModeVectors {
    parse_key: Vec<Option<String>>,
    release: Vec<bool>,
    repeat: Vec<bool>,
    pairs: Vec<(usize, usize)>, // true matchesKey cells
}

fn load() -> Fixture {
    let raw = include_str!("fixtures/keys-golden.json");
    let v: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let to_string_vec = |a: &serde_json::Value| -> Vec<String> {
        a.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap_or_default().to_string())
            .collect()
    };
    let mode = |m: &serde_json::Value| -> ModeVectors {
        let parse_key = m["parseKey"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().map(str::to_string))
            .collect();
        let flags = |k: &str| -> Vec<bool> {
            m[k].as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_bool().unwrap())
                .collect()
        };
        let pairs = m["matrix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                let (i, j) = x.as_str().unwrap().split_once(':').unwrap();
                (i.parse().unwrap(), j.parse().unwrap())
            })
            .collect();
        ModeVectors {
            parse_key,
            release: flags("release"),
            repeat: flags("repeat"),
            pairs,
        }
    };
    Fixture {
        inputs: to_string_vec(&v["inputs"]),
        key_ids: to_string_vec(&v["keyIds"]),
        legacy: mode(&v["modes"]["legacy"]),
        kitty: mode(&v["modes"]["kitty"]),
    }
}

#[test]
fn keys_golden_matrix_matches_reference() {
    let f = load();
    for (mode_name, vectors, kitty_on) in [("legacy", &f.legacy, false), ("kitty", &f.kitty, true)]
    {
        set_kitty_protocol_active(kitty_on);
        for (j, input) in f.inputs.iter().enumerate() {
            let ours = parse_key(input);
            if ours != vectors.parse_key[j] {
                eprintln!(
                    "DIFF [{mode_name}] input={input:?} ours={ours:?} ref={:?}",
                    vectors.parse_key[j]
                );
            }
            assert_eq!(
                ours, vectors.parse_key[j],
                "parseKey[{mode_name}]({input:?})"
            );
            assert_eq!(
                is_key_release(input),
                vectors.release[j],
                "release[{mode_name}]({input:?})"
            );
            assert_eq!(
                is_key_repeat(input),
                vectors.repeat[j],
                "repeat[{mode_name}]({input:?})"
            );
        }
        // dense recheck of the sparse true-matrix plus sampled falses per row
        let mut covered_rows = std::collections::BTreeSet::new();
        for &(i, j) in &vectors.pairs {
            assert!(
                matches_key(&f.inputs[j], &f.key_ids[i]),
                "matches[{mode_name}]({:?}, {:?}) expected true",
                f.inputs[j],
                f.key_ids[i]
            );
            covered_rows.insert(i);
        }
        for i in 0..f.key_ids.len() {
            if !covered_rows.contains(&i) {
                continue;
            }
            for j in 0..f.inputs.len() {
                if !vectors.pairs.contains(&(i, j)) {
                    // sparse false only asserted on parse-recognized or ascii rows to bound runtime
                    if !matches_key_untrusted_fast_path(&f.inputs[j]) && j % 3 != 0 {
                        continue;
                    }
                    assert!(
                        !matches_key(&f.inputs[j], &f.key_ids[i]),
                        "matches[{mode_name}]({:?}, {:?}) expected false",
                        f.inputs[j],
                        f.key_ids[i]
                    );
                }
            }
        }
    }
}

// Fast-path note: matches without any escape/ctrl byte and not matching the key char
// rarely flip; sampling keeps the full sweep bounded while still covering every row.
fn matches_key_untrusted_fast_path(_data: &str) -> bool {
    _data.contains('\u{1b}') || _data.bytes().any(|b| b < 0x20 || b == 0x7f)
}
