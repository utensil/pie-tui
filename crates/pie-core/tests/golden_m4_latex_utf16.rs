//! Raw UTF-16 black-box oracle provenance for the exact M4 LaTeX boundary.

use pie_core::latex::{RenderLatexOptions, render_latex, render_latex_utf16};

fn units(value: &serde_json::Value, field: &str) -> Vec<u16> {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("{field} is a UTF-16 unit array"))
        .iter()
        .map(|unit| u16::try_from(unit.as_u64().expect("unit is unsigned")).expect("unit fits u16"))
        .collect()
}

fn assert_cases(cases: &serde_json::Value) {
    let mut failures = Vec::new();
    for case in cases.as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let source = units(case, "sourceUnits");
        let display = case
            .get("options")
            .and_then(|options| options.get("display"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let expected = case
            .get("outputUnits")
            .filter(|value| !value.is_null())
            .map(|_| units(case, "outputUnits"));
        let actual = render_latex_utf16(&source, RenderLatexOptions { display });
        if actual != expected {
            failures.push(format!("{name}: expected {expected:?}, actual {actual:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} raw UTF-16 LaTeX mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn utf16_oracle_is_pinned_and_records_the_preimplementation_red_witnesses() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-utf16.json"))
            .expect("m4-latex-utf16.json is valid JSON");
    let reference = &fixture["reference"];
    assert_eq!(reference["package"], "@earendil-works/pi-tui");
    assert_eq!(reference["version"], "0.84.1");
    assert_eq!(
        reference["latexJsSha256"],
        "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716"
    );
    assert_eq!(
        reference["utilsJsSha256"],
        "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052"
    );
    assert_eq!(reference["node"], "24.4.1");
    assert_eq!(reference["icu"], "77.1");
    assert_eq!(reference["unicode"], "16.0");
    assert_eq!(fixture["grammar"]["caseCount"], 108);

    let red_witnesses = fixture["preImplementationRedWitnesses"]
        .as_array()
        .expect("red witnesses array");
    assert_eq!(red_witnesses.len(), 4);
    assert!(
        red_witnesses
            .iter()
            .any(|name| name == "edge-lone-high-plain")
    );
    assert!(
        red_witnesses
            .iter()
            .any(|name| name == "edge-unbraced-astral-fraction")
    );
}

#[test]
fn detached_review_oracle_records_the_pre_repair_red_checkpoint() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-utf16.json"))
            .expect("m4-latex-utf16.json is valid JSON");
    let review = &fixture["detachedReview"];
    assert_eq!(
        review["baselineHead"],
        "d0291a6e9ca67cd08f3af26c68867ac05aa96078"
    );
    assert_eq!(review["caseCount"], 861);
    assert_eq!(review["preRepairMismatchCount"], 71);

    let families = review["preRepairMismatchFamilies"]
        .as_object()
        .expect("mismatch family object");
    for (name, count) in [
        ("accent-parse-argument", 33),
        ("display-fraction-normalization", 12),
        ("raw-unit-visible-width", 12),
        ("inline-grouping-predicates", 6),
        ("root-grouping-predicate", 4),
        ("operator-duplicate-scripts", 4),
    ] {
        assert_eq!(
            families[name]
                .as_array()
                .unwrap_or_else(|| panic!("{name} rows"))
                .len(),
            count,
            "{name} pre-repair mismatch count"
        );
    }

    let cases = fixture["reviewProduct"]
        .as_array()
        .expect("review product array");
    assert_eq!(cases.len(), 861);
    let case = |name: &str| {
        cases
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("missing review row {name}"))
    };
    assert_eq!(
        case("bar-unbraced-astral-d0")["outputUnits"],
        serde_json::json!([0xd83d, 0x0305, 0xde00])
    );
    assert_eq!(
        case("fraction-empty-over-empty-d1")["outputUnits"],
        serde_json::json!([10, 0x2500])
    );
    assert_eq!(
        case("fraction-ab-over-high-d1")["outputUnits"],
        serde_json::json!([97, 98, 10, 0x2500, 0x2500, 10, 32, 0xd83d])
    );
    assert!(case("operator-duplicate-sub-d0")["outputUnits"].is_null());
    assert_eq!(
        fixture["repairEdges"]
            .as_array()
            .expect("repair edge array")
            .len(),
        8
    );
}

#[test]
fn raw_utf16_core_matches_the_finite_grammar_product_and_exact_edges() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-utf16.json"))
            .expect("m4-latex-utf16.json is valid JSON");
    assert_cases(&fixture["grammarProduct"]);
    assert_cases(&fixture["exactEdges"]);
}

#[test]
fn raw_utf16_core_matches_detached_review_product_and_repair_edges() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-utf16.json"))
            .expect("m4-latex-utf16.json is valid JSON");
    assert_cases(&fixture["reviewProduct"]);
    assert_cases(&fixture["repairEdges"]);
}

#[test]
fn plain_text_roundtrips_every_oracle_unit_sequence_without_collision() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-utf16.json"))
            .expect("m4-latex-utf16.json is valid JSON");
    assert_cases(&fixture["roundtrip"]);
    for case in fixture["roundtrip"].as_array().expect("roundtrip array") {
        assert_eq!(
            units(case, "sourceUnits"),
            units(case, "outputUnits"),
            "{} is a collision-free unit roundtrip",
            case["name"].as_str().expect("case name")
        );
    }
}

#[test]
fn plain_text_roundtrip_property_covers_arbitrary_unit_products() {
    let alphabet = [
        b'a' as u16,
        0x754c,
        0x0301,
        0xd83d,
        0xde00,
        0xe000,
        0xdbc0,
        0xdc00,
    ];
    let mut checked = 0usize;
    for length in 1usize..=3 {
        let case_count = alphabet.len().pow(length as u32);
        for mut ordinal in 0..case_count {
            let mut source = Vec::with_capacity(length);
            for _ in 0..length {
                source.push(alphabet[ordinal % alphabet.len()]);
                ordinal /= alphabet.len();
            }
            assert_eq!(
                render_latex_utf16(&source, RenderLatexOptions::default()),
                Some(source.clone()),
                "plain UTF-16 unit product {source:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 584);
}

#[test]
fn rust_string_wrapper_is_strict_about_unpaired_output() {
    let astral = "\u{1f600}";
    assert_eq!(
        render_latex(astral, RenderLatexOptions::default()),
        Some(astral.to_string())
    );
    assert_eq!(
        render_latex(&format!("\\sqrt {astral}"), RenderLatexOptions::default()),
        None,
        "the exact core separates the surrogate pair around parentheses"
    );
    assert_eq!(
        render_latex_utf16(
            &[b'A' as u16, 0xd83d, b'B' as u16],
            RenderLatexOptions::default()
        ),
        Some(vec![b'A' as u16, 0xd83d, b'B' as u16]),
        "the exact core preserves a lone surrogate without replacement"
    );
}

#[test]
fn required_utf16_mutations_have_named_non_vacuous_kills() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/m4-latex-utf16-mutation-receipt.json"
    ))
    .expect("UTF-16 mutation receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "pie-tui-m4-latex-utf16-mutation-receipt-v1"
    );
    let mutations = receipt["mutations"].as_array().expect("mutations array");
    assert_eq!(mutations.len(), 6);
    let expected_ids = [
        "revert-parser-to-scalar-chars",
        "use-terminal-width-for-fraction-grouping",
        "erase-nested-group-script-wrapper",
        "lossy-conversion-inside-exact-core",
        "reuse-bmp-private-use-padding-marker",
        "collapse-empty-root",
    ];
    for id in expected_ids {
        let mutation = mutations
            .iter()
            .find(|mutation| mutation["id"] == id)
            .unwrap_or_else(|| panic!("missing mutation {id}"));
        assert_eq!(mutation["exitCode"], 101, "{id} must fail its gate");
        assert!(
            !mutation["killedRows"]
                .as_array()
                .expect("killed rows array")
                .is_empty(),
            "{id} must name a killed row or property"
        );
    }
}

#[test]
fn detached_review_repair_mutations_have_named_non_vacuous_kills() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/m4-latex-utf16-repair-mutation-receipt.json"
    ))
    .expect("repair mutation receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "pie-tui-m4-latex-utf16-repair-mutation-receipt-v1"
    );
    assert_eq!(
        receipt["isolation"],
        "each mutation was applied alone, gated, and restored before the next mutation"
    );
    let mutations = receipt["mutations"].as_array().expect("mutations array");
    let expected = [
        ("require-braced-accent-arguments", 33),
        ("remove-display-fraction-width-floor-and-global-trim", 18),
        ("count-lone-surrogates-as-replacement-width", 12),
        ("restore-coarse-inline-fraction-grouping", 6),
        ("restore-coarse-root-grouping", 4),
        ("allow-operator-duplicate-direction-scripts", 4),
    ];
    assert_eq!(mutations.len(), expected.len());
    for (id, mismatch_count) in expected {
        let mutation = mutations
            .iter()
            .find(|mutation| mutation["id"] == id)
            .unwrap_or_else(|| panic!("missing mutation {id}"));
        assert_eq!(mutation["exitCode"], 101, "{id} must fail its gate");
        assert_eq!(
            mutation["mismatchCount"], mismatch_count,
            "{id} mismatch count"
        );
        assert!(
            !mutation["killedRows"]
                .as_array()
                .expect("killed rows array")
                .is_empty(),
            "{id} must name killed rows"
        );
    }
}
