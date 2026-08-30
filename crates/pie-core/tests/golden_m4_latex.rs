//! Black-box differential vectors for the pure M4 LaTeX renderer.

use pie_core::latex::{RenderLatexOptions, render_latex};

#[test]
fn render_latex_matches_pinned_reference() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/m4-latex.json"))
        .expect("m4-latex.json is valid JSON");
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(
        fixture["reference"]["indexDtsSha256"],
        "f86836256fea4329d5618a87ae503c89f73efa74523a11c0a84294b17b12bea3"
    );
    assert_eq!(
        fixture["reference"]["latexDtsSha256"],
        "76a3bda961e678e859bf8749d68b40a4ce20a08a701329e92758dedda79812f8"
    );
    assert_eq!(
        fixture["reference"]["latexJsSha256"],
        "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716"
    );

    for case in fixture["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let source = case["source"].as_str().expect("case source");
        let display = case
            .get("options")
            .and_then(|options| options.get("display"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let expected = case["output"].as_str().map(str::to_string);
        let actual = render_latex(source, RenderLatexOptions { display });
        assert_eq!(actual, expected, "{name}: renderLatex differential");
    }
}

#[test]
fn render_latex_adversarial_matrix_matches_pinned_reference() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/m4-latex-adversarial.json"))
            .expect("m4-latex-adversarial.json is valid JSON");
    assert_eq!(fixture["reference"]["version"], "0.84.1");
    assert_eq!(
        fixture["reference"]["latexJsSha256"],
        "d8778b4166001faf09fa555d550c06a8d63b84b86244e04b45fa1b3fc68b1716"
    );

    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let source = case["source"].as_str().expect("case source");
        let display = case
            .get("options")
            .and_then(|options| options.get("display"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let expected = case["output"].as_str().map(str::to_string);
        let actual = render_latex(source, RenderLatexOptions { display });
        if actual != expected {
            failures.push(format!("{name}: expected {expected:?}, actual {actual:?}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} adversarial renderLatex mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
