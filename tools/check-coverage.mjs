#!/usr/bin/env node
// Exhaustive canonical API, compiled Rust mapping, and staged import-contract gate.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const loadRaw = (path) => readFileSync(join(root, path), "utf8");
const load = (path) => JSON.parse(loadRaw(path));
const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const same = (left, right) => JSON.stringify(left) === JSON.stringify(right);
const errors = [];

const args = process.argv.slice(2);
const milestoneMode = args.length === 2 && args[0] === "--milestone" && args[1] === "M3";
if (args.length !== 0 && !milestoneMode) {
  console.error("usage: node tools/check-coverage.mjs [--milestone M3]");
  process.exit(64);
}

const apiRaw = loadRaw("tools/api-surface.json");
const api = JSON.parse(apiRaw);
const ledger = load("tools/surface-coverage.json");
const imports = load("tools/pi2dsh-import-contract.json");
const napiPackage = load("adapters/pie-napi/package.json");
const napiOracle = load("adapters/pie-napi/test/oracle-contract.json");
const napiReadme = loadRaw("adapters/pie-napi/README.md");
const tuiFixture = load("crates/pie-app/tests/fixtures/tui-controller.json");
const tuiMutationScript = loadRaw("tools/check-tui-controller-mutations.sh");
const mainAltFixture = load("crates/pie-app/tests/fixtures/main-alt-controller.json");
const mainAltProductScript = loadRaw("tools/check-main-alt-product-mutations.sh");
const mainAltOracleScript = loadRaw("tools/check-main-alt-oracle-mutations.sh");
const ciWorkflow = loadRaw(".github/workflows/ci.yml");
const readme = loadRaw("README.md");
const roadmap = loadRaw("docs/roadmap.md");
const parity = loadRaw("docs/parity.md");

const CANONICAL = Object.freeze({
  package: "@earendil-works/pi-tui",
  version: "0.84.2",
  indexSha256: "2eb2c4d88617a344f5c8f51cc1e42e4770d0867dea67e6466263d4346c980d5c",
  fileSha256: "095c6b786d473808f2636a850de9c7bf458f6eae23e34d61751a9e735b72bceb",
  statementCount: 30,
  symbolCount: 133,
});
const PROVED_FLOOR = 88;
const COMPILED_FLOOR = 106;
const VERIFIED_FLOOR = 40;
const M3_TARGET = 80;
const RUST_CONTROLLER_ORACLE_VERSION = "0.84.1";
const EXPECTED_NAPI_PACKAGE = "pie-tui-native";
const EXPECTED_NAPI_RUNTIME_EXPORTS = Object.freeze(Object.keys(napiOracle.runtimeTypes ?? {}));
const EXPECTED_NAPI_EVIDENCE = Object.freeze([
  "adapters/pie-napi/test/check-reference.mjs",
  "adapters/pie-napi/test/check-surface.mjs",
  "adapters/pie-napi/test/check-type-surface.mjs",
  "adapters/pie-napi/test/check-upstream-drift.mjs",
  "adapters/pie-napi/test/runtime.test.mjs",
  "adapters/pie-napi/test/m5-runtime.test.mjs",
  "adapters/pie-napi/test/m6-runtime.test.mjs",
  "adapters/pie-napi/test/m6-semantic-oracle.mjs",
  "adapters/pie-napi/test/differential.mjs",
  "adapters/pie-napi/test/pack-consumer.mjs",
  "adapters/pie-napi/test/artifact-repro.mjs",
  "adapters/pie-napi/test/check-mutations.mjs",
  "tools/tmux-napi-smoke.sh",
  "tools/check-current-dsh-consumer.sh",
]);
const CALCULATE_IMAGE_ROWS_GAP = "Five oracle rows match, including the canonical omitted third-argument literal { widthPx: 9, heightPx: 18 } default; the remaining gap is that Rust's typed numeric boundary does not yet cover every JavaScript number coercion.";
const CALCULATE_IMAGE_ROWS_README = "`calculateImageRows` defaults omitted cell dimensions to the canonical literal\n  9x18 values. Mutable global cell-dimension access is still needed by\n  `renderImage`;";
const CALCULATE_IMAGE_ROWS_PARITY = "`calculateImageRows` matches the canonical omitted-argument literal 9x18 default but retains a JavaScript numeric/coercion gap; `imageFallback` still receives home/capability facts explicitly, and `renderImage` still receives cached capabilities and mutable global cell dimensions explicitly.";
const NAPI_README_STATUS = "The private package exposes all 69 canonical runtime\n> exports and the 133-name declaration namespace";
const TUI_CONTROLLER_TEST = "crates/pie-app/tests/tui_controller.rs";
const TUI_CONTRACT_TEST = "crates/pie-components/tests/tui_contracts.rs";
const MAIN_ALT_FIXTURE = "crates/pie-app/tests/fixtures/main-alt-controller.json";
const MAIN_ALT_TEST = "crates/pie-app/tests/main_alt_controller.rs";
const MAIN_ALT_PRODUCT_SCRIPT = "tools/check-main-alt-product-mutations.sh";
const MAIN_ALT_ORACLE_SCRIPT = "tools/check-main-alt-oracle-mutations.sh";
const MAIN_ALT_TMUX_SCRIPT = "tools/tmux-main-alt-smoke.sh";
const TUI_CHECKPOINT_EVIDENCE = Object.freeze([
  "crates/pie-components/src/tui.rs",
  TUI_CONTRACT_TEST,
  "crates/pie-app/src/tui_controller.rs",
  "crates/pie-app/tests/fixtures/tui-controller.json",
  TUI_CONTROLLER_TEST,
  "tools/golden/gen-golden-tui-controller.mjs",
  "tools/check-tui-controller-mutations.sh",
  "tools/check-tui-controller-receipt-mutations.sh",
]);
const EXPECTED_S27_DISPOSITIONS = Object.freeze([
  ["Component", "ported", true],
  ["Container", "partial", true],
  ["CURSOR_MARKER", "verified", true],
  ["compositeTuiLine", "ported", true],
  ["Focusable", "deferred", false],
  ["isFocusable", "deferred", false],
  ["isViewportTUI", "deferred", false],
  ["OverlayAnchor", "partial", false],
  ["OverlayHandle", "partial", false],
  ["OverlayMargin", "partial", false],
  ["OverlayOptions", "partial", false],
  ["OverlayUnfocusOptions", "partial", false],
  ["SizeValue", "ported", true],
  ["TUI", "partial", false],
  ["TuiInputListener", "partial", false],
  ["TuiInputListenerResult", "partial", false],
  ["TuiMode", "partial", false],
  ["TuiStopOptions", "partial", false],
  ["ViewportTUI", "partial", false],
]);
const EXPECTED_TUI_PARTIAL_EVIDENCE = Object.freeze({
  OverlayAnchor: [TUI_CONTRACT_TEST, TUI_CONTROLLER_TEST],
  OverlayHandle: [TUI_CONTROLLER_TEST],
  OverlayMargin: [TUI_CONTROLLER_TEST],
  OverlayOptions: [TUI_CONTRACT_TEST, TUI_CONTROLLER_TEST],
  OverlayUnfocusOptions: [TUI_CONTROLLER_TEST],
  TUI: [TUI_CONTRACT_TEST, TUI_CONTROLLER_TEST, MAIN_ALT_TEST],
  TuiInputListener: [TUI_CONTROLLER_TEST],
  TuiInputListenerResult: [TUI_CONTROLLER_TEST],
  TuiMode: [TUI_CONTROLLER_TEST, MAIN_ALT_TEST],
  TuiStopOptions: [TUI_CONTROLLER_TEST, MAIN_ALT_TEST],
  ViewportTUI: [TUI_CONTRACT_TEST, TUI_CONTROLLER_TEST, MAIN_ALT_TEST],
});
const EXPECTED_TUI_GAPS = Object.freeze({
  OverlayAnchor: "Rust exposes all nine semantic anchors as public OverlayAnchor variants; the oracle exercises center, top-left, and bottom-right layout, but the canonical JavaScript string union and NAPI boundary remain absent.",
  OverlayHandle: "The public rank-3 OverlayHandle covers hide, temporary visibility, focus, unfocus, and focus-state behavior, but Rust's OverlayUnfocus argument and retained handle identity are not the canonical optional JavaScript options/object boundary, and no NAPI export exists.",
  OverlayMargin: "Rust models four required i64 fields defaulting to zero plus OverlayMargins::All for the numeric union; canonical fields are optional JavaScript numbers with different coercion and object semantics, and no NAPI mapping exists.",
  OverlayOptions: "Layout, default, visibility, and non-capturing behavior is reviewed, but Rust uses Option, usize, i64, SizeValue, and Rc callback fields rather than the exact optional JavaScript object, number coercions, callback identity, and NAPI shape.",
  OverlayUnfocusOptions: "Rust OverlayUnfocus::Restore and OverlayUnfocus::Target(Option<ComponentRef>) model omitted options versus an explicit target, but they are not the canonical OverlayUnfocusOptions { target: Component | null } JavaScript/NAPI object.",
  TUI: "The object-safe rank-2 Tui trait and rank-3 TuiBaseController cover shared lifecycle, focus, overlays, listener dispatch, queries, and scheduling, and reviewed injected Rust Main/Alt controllers build on that foundation. Tui is not a Component supertrait, and direct Rust lacks canonical mutable children, terminal, onDebug, and Promise-query property shapes; the M5 adapter supplies the production host/event loop and JavaScript facade.",
  TuiInputListener: "Rust retains callback identity and live Set-style ordering and reentrancy, but uses a valid-UTF-8 borrowed Rust callback rather than exact JavaScript function/string identity or NAPI.",
  TuiInputListenerResult: "Outer Option plus TuiInputListenerResult models undefined/pass, consume, and transform, but consume is a required bool and data is Option<String>, not the canonical optional JavaScript object/string/coercion identity, and no NAPI mapping exists.",
  TuiMode: "Rust exposes Regular and Fullscreen variants and both injected Main/Alt controller modes are oracle-exercised; the exact JavaScript string union and NAPI boundary remain absent.",
  TuiStopOptions: "Rust uses a required preserve_screen bool with a false default rather than the canonical optional preserveScreen JavaScript property object, and no NAPI mapping exists.",
  ViewportTUI: "The ViewportTui trait, TuiBaseController layout-root behavior, and injected Rust Alt controller are reviewed. Direct Rust lacks the canonical VIEWPORT_TUI symbol brand and JavaScript interface; the M5 adapter supplies isViewportTUI, the public type facade, and production host event loop.",
});
const EXPECTED_S28_DISPOSITIONS = Object.freeze([
  ["TuiAltScreen", "partial", false],
  ["TuiAltScreenOptions", "partial", false],
]);
const EXPECTED_MAIN_ALT_EVIDENCE = Object.freeze({
  TuiAltScreen: [MAIN_ALT_FIXTURE, MAIN_ALT_TEST, MAIN_ALT_TMUX_SCRIPT],
  TuiAltScreenOptions: [MAIN_ALT_FIXTURE, MAIN_ALT_TEST],
});
const EXPECTED_MAIN_ALT_GAPS = Object.freeze({
  TuiAltScreen: "The injected Rust TuiAltScreen covers the pinned 0.84.1 lifecycle, alternate-buffer, layout-root, focus/overlay, scroll/mouse/selection, image ownership, and teardown corpus. It still uses injected runtime and terminal seams rather than the canonical JavaScript class; the M5 adapter supplies ProcessTerminal/task pumping, JavaScript identity, package exposure, and current-consumer execution.",
  TuiAltScreenOptions: "Wheel, mouse, URL-open, and right-click callback behavior is exercised through typed Rust options, but Rust callback ownership and numeric/value coercion are not the canonical optional JavaScript object or NAPI boundary.",
});
const EXPECTED_COMPONENT_GAP = "Rust Component now provides default-false wants_key_release() behavior and structural focus hooks, but the canonical optional mutable wantsKeyRelease property, the separate Focusable.focused property shape, and JavaScript/NAPI property identity remain absent.";
const EXPECTED_CONTAINER_GAP = "ComponentHandle and ComponentRef cover retained and nested identity, duplicate-first removal, cross-owner/stale no-op, and re-add behavior, but the canonical mutable public children array and exact addChild/removeChild Component-object signature are not exposed.";
const TUI_ORACLE_COMMAND = "PI_TUI_DIST=/path/to/pinned-0.84.1/dist node tools/golden/gen-golden-tui-controller.mjs --check";
const TUI_MUTATION_COMMAND = "PI_TUI_DIST=/path/to/pinned-0.84.1/dist tools/check-tui-controller-mutations.sh";
const MAIN_ALT_ORACLE_COMMAND = "PI_TUI_DIST=/path/to/pinned-0.84.1/dist node tools/golden/gen-golden-main-alt-controller.mjs --check";
const MAIN_ALT_ORACLE_MUTATION_COMMAND = "PI_TUI_DIST=/path/to/pinned-0.84.1/dist tools/check-main-alt-oracle-mutations.sh";
const MAIN_ALT_PRODUCT_MUTATION_COMMAND = "tools/check-main-alt-product-mutations.sh";
const MAIN_ALT_TMUX_COMMAND = "tools/tmux-main-alt-smoke.sh";
const FRESH_PIE_APP_CONSUMER_COMMAND = "bash tools/check-fresh-pie-app-consumer.sh";
const DOCUMENTED_MILESTONES = Object.freeze([
  { name: "M3", readmeState: "Complete", roadmapState: "✅" },
  { name: "M4.0", readmeState: "Complete", roadmapState: "✅" },
  { name: "M4", readmeState: "Complete", roadmapState: "✅" },
  { name: "M5", readmeState: "Complete", roadmapState: "✅" },
]);

const freshPieAppConsumerCiCount = ciWorkflow
  .split("\n")
  .filter((line) => line.trim() === `run: ${FRESH_PIE_APP_CONSUMER_COMMAND}`)
  .length;
if (freshPieAppConsumerCiCount !== 1) {
  errors.push(
    `G0 fresh pie-app consumer CI command count: expected 1, got ${freshPieAppConsumerCiCount}`,
  );
}

if (sha256(apiRaw) !== CANONICAL.fileSha256) {
  errors.push(`C0 canonical manifest digest drift: ${sha256(apiRaw)}`);
}
for (const key of ["package", "version", "indexSha256", "statementCount", "symbolCount"]) {
  if (api.reference?.[key] !== CANONICAL[key]) {
    errors.push(`C1 reference ${key}: expected ${CANONICAL[key]}, got ${api.reference?.[key]}`);
  }
}

function defaultMetadata(signature) {
  const optionalMarkerCount = [...signature.matchAll(/\b[A-Za-z_$][\w$]*\?\s*(?=[:(])/g)].length;
  const initializerCount = [...signature.matchAll(/\b[A-Za-z_$][\w$]*\s*=\s*[^,;)}]+/g)].length;
  const documented = [...signature.matchAll(/.{0,60}\bdefault\b.{0,80}/gi)]
    .map((match) => match[0].replace(/\s+/g, " ").trim())
    .filter((value, index, values) => values.indexOf(value) === index);
  return { optionalMarkerCount, initializerCount, documented };
}

const canonicalSymbols = [];
const statementIds = new Set();
const canonicalNames = new Set();
for (const [index, statement] of (api.statements ?? []).entries()) {
  const expectedPrefix = `S${String(index + 1).padStart(2, "0")}-`;
  if (!statement.id?.startsWith(expectedPrefix)) errors.push(`C2 unstable statement id at ${index}: ${statement.id}`);
  if (statementIds.has(statement.id)) errors.push(`C2 duplicate statement id: ${statement.id}`);
  statementIds.add(statement.id);
  if (sha256(statement.barrelStatement) !== statement.barrelStatementSha256) {
    errors.push(`C3 ${statement.id}: barrel statement hash drift`);
  }
  for (const symbol of statement.symbols ?? []) {
    if (canonicalNames.has(symbol.name)) errors.push(`C4 duplicate canonical symbol: ${symbol.name}`);
    canonicalNames.add(symbol.name);
    if (!["runtime", "type"].includes(symbol.exportKind)) {
      errors.push(`C5 ${symbol.name}: invalid canonical kind ${symbol.exportKind}`);
    }
    if (sha256(symbol.signature) !== symbol.signatureSha256) {
      errors.push(`C6 ${symbol.name}: canonical signature hash drift`);
    }
    const defaults = defaultMetadata(symbol.signature);
    if (!same(defaults, symbol.defaultMetadata)) {
      errors.push(`C7 ${symbol.name}: canonical default metadata drift`);
    }
    canonicalSymbols.push({ statement, symbol });
  }
}
if (statementIds.size !== CANONICAL.statementCount || canonicalSymbols.length !== CANONICAL.symbolCount) {
  errors.push(`C8 canonical cardinality: ${statementIds.size}/${CANONICAL.statementCount} statements, ${canonicalSymbols.length}/${CANONICAL.symbolCount} symbols`);
}

const canonicalRuntimeNames = canonicalSymbols
  .filter(({ symbol }) => symbol.exportKind === "runtime")
  .map(({ symbol }) => symbol.name);
const selectedNapiRuntimeExports = Object.keys(napiOracle.runtimeTypes ?? {});
if (canonicalRuntimeNames.length !== 69) {
  errors.push(`N0 canonical runtime cardinality: expected 69, got ${canonicalRuntimeNames.length}`);
}
if (napiPackage.name !== EXPECTED_NAPI_PACKAGE || napiPackage.private !== true) {
  errors.push(
    `N1 private package identity: expected ${EXPECTED_NAPI_PACKAGE}/true, `
    + `got ${napiPackage.name}/${String(napiPackage.private)}`,
  );
}
if (napiOracle.reference?.package !== CANONICAL.package
  || napiOracle.reference?.version !== CANONICAL.version) {
  errors.push("N2 private package oracle is not pinned to the canonical package/version");
}
if (!same(selectedNapiRuntimeExports, EXPECTED_NAPI_RUNTIME_EXPORTS)) {
  errors.push(`N3 private package ordered ${EXPECTED_NAPI_RUNTIME_EXPORTS.length}-export runtime surface drift`);
}
if (new Set(selectedNapiRuntimeExports).size !== selectedNapiRuntimeExports.length) {
  errors.push("N4 private package runtime subset contains duplicate exports");
}
for (const name of selectedNapiRuntimeExports) {
  if (!canonicalRuntimeNames.includes(name)) {
    errors.push(`N5 private package selected export is not canonical runtime: ${name}`);
  }
}

if (!same(ledger.reference, api.reference)) errors.push("L0 ledger reference does not equal canonical reference");
const napiBinding = ledger.bindings?.napi;
if (!napiBinding
  || napiBinding.package !== EXPECTED_NAPI_PACKAGE
  || napiBinding.private !== true
  || napiBinding.dropIn !== true
  || napiBinding.selectedRuntimeExportCount !== EXPECTED_NAPI_RUNTIME_EXPORTS.length
  || napiBinding.canonicalRuntimeExportCount !== canonicalRuntimeNames.length
  || !same(napiBinding.selectedRuntimeExports, EXPECTED_NAPI_RUNTIME_EXPORTS)
  || !same(napiBinding.evidence, EXPECTED_NAPI_EVIDENCE)) {
  errors.push("N6 generated private Node-API binding receipt drift");
}
for (const path of EXPECTED_NAPI_EVIDENCE) {
  if (!existsSync(join(root, path))) errors.push(`N7 private Node-API evidence path missing: ${path}`);
}
const napiSurface = spawnSync(process.execPath, ["adapters/pie-napi/test/check-surface.mjs"], {
  cwd: root,
  encoding: "utf8",
});
if (napiSurface.status !== 0) {
  const output = `${napiSurface.stdout ?? ""}\n${napiSurface.stderr ?? ""}`.trim();
  errors.push(`N7 private Node-API selected surface drift (exit ${napiSurface.status}):\n${output}`);
}
const allowedStatuses = new Set(["external", "deferred", "partial", "ported", "verified"]);
const ledgerRows = ledger.symbols ?? [];
const ledgerById = new Map();
for (const row of ledgerRows) {
  if (ledgerById.has(row.id)) errors.push(`L1 duplicate ledger id: ${row.id}`);
  ledgerById.set(row.id, row);
}
if (ledgerRows.length !== CANONICAL.symbolCount) errors.push(`L2 expected ${CANONICAL.symbolCount} ledger rows, got ${ledgerRows.length}`);

for (const { statement, symbol } of canonicalSymbols) {
  const id = `${statement.id}:${symbol.name}`;
  const row = ledgerById.get(id);
  if (!row) {
    errors.push(`L3 missing ledger row: ${id}`);
    continue;
  }
  if (row.name !== symbol.name || row.statementId !== statement.id) {
    errors.push(`L4 renamed/misassigned row ${id}: ${row.statementId}:${row.name}`);
  }
  if (row.kind !== symbol.exportKind) errors.push(`L5 ${id}: kind drift ${row.kind} != ${symbol.exportKind}`);
  if (row.signatureSha256 !== symbol.signatureSha256) errors.push(`L6 ${id}: signature drift`);
  if (!same(row.defaultMetadata, symbol.defaultMetadata)) errors.push(`L7 ${id}: default metadata drift`);
  if (!allowedStatuses.has(row.status)) errors.push(`L8 ${id}: invalid status ${row.status}`);
  if (row.status === "partial" && !(row.gaps?.length > 0)) errors.push(`L9 ${id}: partial without an explicit gap`);
  if (["ported", "verified"].includes(row.status) && row.rustEvidence === null) {
    errors.push(`L10 ${id}: ${row.status} without compiled Rust evidence`);
  }
  if (row.status === "verified" && !(row.behaviorEvidence?.length > 0)) {
    errors.push(`L11 ${id}: verified without behavioral evidence`);
  }
  const paths = [
    ...(row.rustEvidence?.productPaths ?? []),
    ...(row.rustEvidence?.contractTest ? [row.rustEvidence.contractTest] : []),
    ...(row.behaviorEvidence ?? []),
  ];
  for (const path of paths) {
    if (!existsSync(join(root, path))) errors.push(`L12 ${id}: evidence path missing: ${path}`);
  }
}
for (const id of ledgerById.keys()) {
  if (!canonicalSymbols.some(({ statement, symbol }) => id === `${statement.id}:${symbol.name}`)) {
    errors.push(`L13 unknown ledger row: ${id}`);
  }
}
const calculateImageRowsRow = ledgerById.get("S26-terminal-image:calculateImageRows");
if (!calculateImageRowsRow || !same(calculateImageRowsRow.gaps, [CALCULATE_IMAGE_ROWS_GAP])) {
  errors.push("L20 calculateImageRows literal-default/numeric-gap receipt drift");
}

const s27Rows = ledgerRows.filter((row) => row.statementId === "S27-tui");
const s27Dispositions = s27Rows.map((row) => [
  row.name,
  row.status,
  row.rustEvidence !== null,
]);
if (!same(s27Dispositions, EXPECTED_S27_DISPOSITIONS)) {
  errors.push("T0 exact S27 status/compiled-evidence disposition drift");
}
for (const [name, expectedEvidence] of Object.entries(EXPECTED_TUI_PARTIAL_EVIDENCE)) {
  const row = ledgerById.get(`S27-tui:${name}`);
  if (!row
    || row.status !== "partial"
    || row.rustEvidence !== null
    || !same(row.behaviorEvidence, expectedEvidence)
    || !same(row.gaps, [EXPECTED_TUI_GAPS[name]])) {
    errors.push(`T1 ${name} partial/evidence/gap receipt drift`);
  }
}
for (const name of ["Focusable", "isFocusable", "isViewportTUI"]) {
  const row = ledgerById.get(`S27-tui:${name}`);
  if (!row
    || row.status !== "deferred"
    || row.rustEvidence !== null
    || !same(row.behaviorEvidence, [])
    || !same(row.gaps, [])) {
    errors.push(`T2 ${name} must remain an unmapped S27 deferral`);
  }
}
const componentRow = ledgerById.get("S27-tui:Component");
if (!componentRow
  || !same(componentRow.behaviorEvidence, [TUI_CONTRACT_TEST, TUI_CONTROLLER_TEST])
  || !same(componentRow.gaps, [EXPECTED_COMPONENT_GAP])) {
  errors.push("T3 Component wantsKeyRelease/focus evidence-gap receipt drift");
}
const containerRow = ledgerById.get("S27-tui:Container");
if (!containerRow
  || !same(containerRow.behaviorEvidence, [
    "crates/pie-components/tests/golden_m3_components.rs",
    TUI_CONTRACT_TEST,
    TUI_CONTROLLER_TEST,
  ])
  || !same(containerRow.gaps, [EXPECTED_CONTAINER_GAP])) {
  errors.push("T4 Container retained/nested identity evidence-gap receipt drift");
}
for (const path of TUI_CHECKPOINT_EVIDENCE) {
  if (!existsSync(join(root, path))) errors.push(`T5 TuiBase checkpoint evidence missing: ${path}`);
}
if (tuiFixture.generator !== "tools/golden/gen-golden-tui-controller.mjs"
  || tuiFixture.reference?.package !== CANONICAL.package
  || tuiFixture.reference?.version !== RUST_CONTROLLER_ORACLE_VERSION
  || tuiFixture.reference?.node !== "24.4.1"
  || tuiFixture.reference?.icu !== "77.1"
  || tuiFixture.reference?.unicode !== "16.0"
  || tuiFixture.reference?.platform !== "darwin"
  || tuiFixture.reference?.arch !== "arm64"
  || tuiFixture.cases?.length !== 29
  || new Set(tuiFixture.cases?.map((entry) => entry.name)).size !== 29) {
  errors.push("T6 pinned 0.84.1 TuiBase 29-case oracle receipt drift");
}
const tuiMutationCounts = {
  product: (tuiMutationScript.match(/^expect_killed\s+/gm) ?? []).length,
  oracleCopy: (tuiMutationScript.match(/^expect_oracle_copy_killed\s+/gm) ?? []).length,
  oracleManifest: (tuiMutationScript.match(/^expect_oracle_manifest_killed\s+/gm) ?? []).length,
  oracleClosure: (tuiMutationScript.match(/^expect_oracle_closure_omission_killed$/gm) ?? []).length,
};
if (!same(tuiMutationCounts, {
  product: 52,
  oracleCopy: 30,
  oracleManifest: 2,
  oracleClosure: 1,
})) {
  errors.push(`T7 TuiBase mutation cardinality drift: ${JSON.stringify(tuiMutationCounts)}`);
}

const s28Rows = ledgerRows.filter((row) => row.statementId === "S28-tui-alt-screen");
const s28Dispositions = s28Rows.map((row) => [
  row.name,
  row.status,
  row.rustEvidence !== null,
]);
if (!same(s28Dispositions, EXPECTED_S28_DISPOSITIONS)) {
  errors.push("A0 exact S28 status/compiled-evidence disposition drift");
}
for (const [name, expectedEvidence] of Object.entries(EXPECTED_MAIN_ALT_EVIDENCE)) {
  const row = ledgerById.get(`S28-tui-alt-screen:${name}`);
  if (!row
    || row.status !== "partial"
    || row.rustEvidence !== null
    || !same(row.behaviorEvidence, expectedEvidence)
    || !same(row.gaps, [EXPECTED_MAIN_ALT_GAPS[name]])) {
    errors.push(`A1 ${name} partial/evidence/gap receipt drift`);
  }
}
for (const path of [
  MAIN_ALT_FIXTURE,
  MAIN_ALT_TEST,
  "tools/golden/gen-golden-main-alt-controller.mjs",
  MAIN_ALT_PRODUCT_SCRIPT,
  MAIN_ALT_ORACLE_SCRIPT,
  MAIN_ALT_TMUX_SCRIPT,
  "tools/check-fresh-pie-app-consumer.sh",
]) {
  if (!existsSync(join(root, path))) errors.push(`A2 Main/Alt checkpoint evidence missing: ${path}`);
}
if (mainAltFixture.generator !== "tools/golden/gen-golden-main-alt-controller.mjs"
  || mainAltFixture.reference?.package !== CANONICAL.package
  || mainAltFixture.reference?.version !== RUST_CONTROLLER_ORACLE_VERSION
  || mainAltFixture.reference?.node !== "24.4.1"
  || mainAltFixture.reference?.icu !== "77.1"
  || mainAltFixture.reference?.unicode !== "16.0"
  || mainAltFixture.reference?.platform !== "darwin"
  || mainAltFixture.reference?.arch !== "arm64"
  || mainAltFixture.cases?.length !== 12
  || new Set(mainAltFixture.cases?.map((entry) => entry.name)).size !== 12) {
  errors.push("A3 pinned 0.84.1 Main/Alt 12-case oracle receipt drift");
}
const mainAltMutationCounts = {
  product: (mainAltProductScript.match(/^expect_killed\s+/gm) ?? []).length,
  oracleCopy: (mainAltOracleScript.match(/^expect_copied_source_killed\s+/gm) ?? []).length,
};
if (!same(mainAltMutationCounts, { product: 20, oracleCopy: 17 })) {
  errors.push(`A4 Main/Alt regression cardinality drift: ${JSON.stringify(mainAltMutationCounts)}`);
}

function statementStatus(rows) {
  const ported = rows.filter((row) => ["ported", "verified"].includes(row.status)).length;
  const verified = rows.filter((row) => row.status === "verified").length;
  if (rows.every((row) => row.status === "external")) return "external";
  if (rows.every((row) => row.status === "deferred")) return "deferred";
  if (ported === rows.length) return verified === rows.length ? "verified" : "ported";
  return "partial";
}

const derivedStatements = api.statements.map((statement) => {
  const rows = ledgerRows.filter((row) => row.statementId === statement.id);
  return {
    id: statement.id,
    status: statementStatus(rows),
    portedSymbols: rows.filter((row) => ["ported", "verified"].includes(row.status)).length,
    verifiedSymbols: rows.filter((row) => row.status === "verified").length,
    totalSymbols: rows.length,
  };
});
if (!same(derivedStatements, ledger.statements)) errors.push("L14 statement summaries drift from symbol dispositions");

const derivedMetrics = {
  symbols: {
    total: ledgerRows.length,
    compiledMappings: ledgerRows.filter((row) => row.rustEvidence !== null).length,
    portedOrVerified: ledgerRows.filter((row) => ["ported", "verified"].includes(row.status)).length,
    verified: ledgerRows.filter((row) => row.status === "verified").length,
    m3Target: M3_TARGET,
  },
  statements: {
    total: derivedStatements.length,
    complete: derivedStatements.filter((row) => ["ported", "verified"].includes(row.status)).length,
    verified: derivedStatements.filter((row) => row.status === "verified").length,
  },
};
if (!same(derivedMetrics, ledger.metrics)) errors.push("L15 metrics drift from exhaustive dispositions");
const expectedTuiCheckpointMetrics = {
  symbols: {
    total: 133,
    compiledMappings: 106,
    portedOrVerified: 88,
    verified: 40,
    m3Target: 80,
  },
  statements: { total: 30, complete: 12, verified: 5 },
};
if (!same(derivedMetrics, expectedTuiCheckpointMetrics)) {
  errors.push(`T8 TuiBase checkpoint must preserve exact canonical metrics: ${JSON.stringify(derivedMetrics)}`);
}
const s27Summary = derivedStatements.find((row) => row.id === "S27-tui");
if (!same(s27Summary, {
  id: "S27-tui",
  status: "partial",
  portedSymbols: 4,
  verifiedSymbols: 1,
  totalSymbols: 19,
})) {
  errors.push(`T9 S27 statement summary drift: ${JSON.stringify(s27Summary)}`);
}
const s28Summary = derivedStatements.find((row) => row.id === "S28-tui-alt-screen");
if (!same(s28Summary, {
  id: "S28-tui-alt-screen",
  status: "partial",
  portedSymbols: 0,
  verifiedSymbols: 0,
  totalSymbols: 2,
})) {
  errors.push(`A5 S28 statement summary drift: ${JSON.stringify(s28Summary)}`);
}
if (ledger.policy?.ordinaryFloor !== PROVED_FLOOR
  || ledger.policy?.compiledFloor !== COMPILED_FLOOR
  || ledger.policy?.verifiedFloor !== VERIFIED_FLOOR
  || ledger.policy?.m3Target !== M3_TARGET) {
  errors.push(
    `L16 policy drift: expected ported ${PROVED_FLOOR}, compiled ${COMPILED_FLOOR}, `
    + `verified ${VERIFIED_FLOOR}, M3 target ${M3_TARGET}`,
  );
}
if (derivedMetrics.symbols.portedOrVerified < PROVED_FLOOR) {
  errors.push(`L17 proved floor regressed: ${derivedMetrics.symbols.portedOrVerified} < ${PROVED_FLOOR}`);
}
if (derivedMetrics.symbols.compiledMappings < COMPILED_FLOOR) {
  errors.push(`L18 compiled floor regressed: ${derivedMetrics.symbols.compiledMappings} < ${COMPILED_FLOOR}`);
}
if (derivedMetrics.symbols.verified < VERIFIED_FLOOR) {
  errors.push(`L19 verified floor regressed: ${derivedMetrics.symbols.verified} < ${VERIFIED_FLOOR}`);
}

const readmeMetricLines = readme.match(/^Canonical API ledger:.*$/gm) ?? [];
if (readmeMetricLines.length !== 1) {
  errors.push(`D0 expected exactly one canonical README metrics sentence, got ${readmeMetricLines.length}`);
} else {
  const readmeMetricLine = readmeMetricLines[0];
  const expectedReadmeMetrics = [
    {
      label: "ported or verified",
      pattern: /(\d+)\/(\d+)\s+ported or verified\b/,
      expected: [derivedMetrics.symbols.portedOrVerified, derivedMetrics.symbols.total],
    },
    {
      label: "behavior-verified",
      pattern: /(\d+)\/(\d+)\s+behavior-verified\b/,
      expected: [derivedMetrics.symbols.verified, derivedMetrics.symbols.total],
    },
    {
      label: "compiled mappings",
      pattern: /(\d+)\s+compiled mappings\b/,
      expected: [derivedMetrics.symbols.compiledMappings],
    },
    {
      label: "M3 target",
      pattern: /\bM3 target\s+(\d+)\b/,
      expected: [M3_TARGET],
    },
    {
      label: "M3 gap",
      pattern: /\bgap\s+(\d+)\b/,
      expected: [Math.max(0, M3_TARGET - derivedMetrics.symbols.portedOrVerified)],
    },
  ];
  for (const { label, pattern, expected } of expectedReadmeMetrics) {
    const match = readmeMetricLine.match(pattern);
    const actual = match?.slice(1).map(Number);
    if (!actual || !same(actual, expected)) {
      errors.push(`D1 stale README ${label}: expected ${expected.join("/")}, got ${actual?.join("/") ?? "missing"}`);
    }
  }
}

function tableRows(markdown) {
  return markdown.split("\n")
    .filter((line) => /^\|[^-].*\|$/.test(line))
    .map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()));
}

const readmeRows = tableRows(readme);
const roadmapRows = tableRows(roadmap);
const readmeMilestones = new Map(readmeRows.map((row) => [row[0], row[1]]));
const roadmapMilestones = new Map(roadmapRows.map((row) => [row[0], row[2]]));

function checkNapiDocReceipt(label, text) {
  const normalized = text.replace(/\s+/g, " ");
  if (!new RegExp(`(?:${EXPECTED_NAPI_RUNTIME_EXPORTS.length}/${canonicalRuntimeNames.length}|${EXPECTED_NAPI_RUNTIME_EXPORTS.length}-export|all ${EXPECTED_NAPI_RUNTIME_EXPORTS.length} canonical runtime)`, "i").test(normalized)) {
    errors.push(`N8 ${label} omits the complete runtime-surface receipt`);
  }
  if (!/133-name/i.test(normalized)) {
    errors.push(`N9 ${label} omits the exact declaration-namespace receipt`);
  }
}

function compactDoc(text) {
  return (text ?? "")
    .replace(/^>\s?/gm, "")
    .replace(/`/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function checkTuiDocBoundary(label, text) {
  const normalized = compactDoc(text);
  if (!/production ProcessTerminal/i.test(normalized)) {
    errors.push(`T10 ${label} omits the production ProcessTerminal receipt`);
  }
  if (!/Main\/Alt/i.test(normalized)
    || !/(?:dsh-pi-tui-mono|current pi-dsh|current consumer)/i.test(normalized)) {
    errors.push(`T11 ${label} omits the Main/Alt or current-consumer receipt`);
  }
}

const readmeM5Rows = readmeRows.filter((row) => row[0] === "M5");
if (readmeM5Rows.length !== 1) {
  errors.push(`N8 expected exactly one README M5 row, got ${readmeM5Rows.length}`);
} else {
  checkNapiDocReceipt("README M5 row", readmeM5Rows[0].slice(1).join(" "));
}
if (!readme.includes(NAPI_README_STATUS)) {
  errors.push(`N8 README status callout omits the complete runtime/type receipt`);
}
const readmeStatus = readme.match(/^> \*\*Status:\*\*[\s\S]*?(?=\n\n)/m)?.[0];
if (!readmeStatus) {
  errors.push("T10 README status callout missing");
} else {
  checkTuiDocBoundary("README status callout", readmeStatus);
}
if (readmeM5Rows.length === 1) {
  checkTuiDocBoundary("README M5 row", readmeM5Rows[0].slice(1).join(" "));
}
const pieAppBlock = readme.match(/^- `pie-app`[\s\S]*?(?=\n- `pie-napi`)/m)?.[0];
if (!pieAppBlock
  || !compactDoc(pieAppBlock).includes(
    "shared deterministic TuiBaseController lifecycle with an injected TuiControllerHost",
  )
  || !compactDoc(pieAppBlock).includes("TuiBaseController remains screen-agnostic")
  || !compactDoc(pieAppBlock).includes("reviewed injected Rust TuiMainScreen and TuiAltScreen controllers")
  || !compactDoc(pieAppBlock).includes("Node adapter supplies the process-stream/event-loop ownership and canonical JavaScript screen classes")) {
  errors.push("T12 README pie-app TuiBase availability/boundary receipt drift");
}
const tuiLimitBlock = readme.match(/^- The integrated `TuiBaseController`[\s\S]*?(?=\n- |\n\n)/m)?.[0];
if (compactDoc(tuiLimitBlock) !== "- The integrated TuiBaseController remains a deterministic injected-host Rust foundation. The adapter now supplies production host/event-loop wiring, canonical screen classes, and current-consumer execution.") {
  errors.push("T13 README TuiBase current-limit receipt drift");
}
const napiLimitBlock = readme.match(/^- The private `pie-tui-native` package[\s\S]*?(?=\n- |\n\n)/m)?.[0];
if (compactDoc(napiLimitBlock) !== `- The private pie-tui-native package exposes exactly ${EXPECTED_NAPI_RUNTIME_EXPORTS.length}/${canonicalRuntimeNames.length} canonical runtime exports and the exact 133-name type namespace. Its drop-in claim is bounded to the authenticated 0.84.2 contract and recorded consumer/behavior corpus.`) {
  errors.push("N14 README private-package current-limit receipt drift");
}
const napiVerifyCommands = readme.match(/^npm --prefix adapters\/pie-napi run verify$/gm) ?? [];
if (napiVerifyCommands.length !== 1) {
  errors.push(`N10 expected one exact private Node-API verification command, got ${napiVerifyCommands.length}`);
}
const tuiOracleCommands = readme.split("\n")
  .filter((line) => line.includes("tools/golden/gen-golden-tui-controller.mjs"));
if (!same(tuiOracleCommands, [TUI_ORACLE_COMMAND])) {
  errors.push(`T14 expected one exact TuiBase oracle command, got ${JSON.stringify(tuiOracleCommands)}`);
}
const tuiMutationCommands = readme.split("\n")
  .filter((line) => line.includes("tools/check-tui-controller-mutations.sh"));
if (!same(tuiMutationCommands, [TUI_MUTATION_COMMAND])) {
  errors.push(`T15 expected one exact TuiBase mutation command, got ${JSON.stringify(tuiMutationCommands)}`);
}
const mainAltOracleCommands = readme.split("\n")
  .filter((line) => line.includes("tools/golden/gen-golden-main-alt-controller.mjs"));
if (!same(mainAltOracleCommands, [MAIN_ALT_ORACLE_COMMAND])) {
  errors.push(`A6 expected one exact Main/Alt oracle command, got ${JSON.stringify(mainAltOracleCommands)}`);
}
const mainAltOracleMutationCommands = readme.split("\n")
  .filter((line) => line.includes(MAIN_ALT_ORACLE_SCRIPT));
if (!same(mainAltOracleMutationCommands, [MAIN_ALT_ORACLE_MUTATION_COMMAND])) {
  errors.push(`A7 expected one exact Main/Alt oracle-regression command, got ${JSON.stringify(mainAltOracleMutationCommands)}`);
}
const mainAltProductMutationCommands = readme.split("\n")
  .filter((line) => line.includes(MAIN_ALT_PRODUCT_SCRIPT));
if (!same(mainAltProductMutationCommands, [MAIN_ALT_PRODUCT_MUTATION_COMMAND])) {
  errors.push(`A8 expected one exact Main/Alt product-regression command, got ${JSON.stringify(mainAltProductMutationCommands)}`);
}
const mainAltTmuxCommands = readme.split("\n")
  .filter((line) => line.includes(MAIN_ALT_TMUX_SCRIPT));
if (!same(mainAltTmuxCommands, [MAIN_ALT_TMUX_COMMAND])) {
  errors.push(`A9 expected one exact Main/Alt tmux command, got ${JSON.stringify(mainAltTmuxCommands)}`);
}
const normalizedNapiReadme = napiReadme.replace(/\s+/g, " ");
if (!normalizedNapiReadme.includes(`complete ${EXPECTED_NAPI_RUNTIME_EXPORTS.length}-export runtime namespace`)
  || !normalizedNapiReadme.includes("authenticated pi-tui 0.84.2")
  || !normalizedNapiReadme.includes("133-name")
  || !normalizedNapiReadme.includes("not the upstream package")
  || !normalizedNapiReadme.includes("documented parity ledger")) {
  errors.push("N11 package README must retain its complete-surface and bounded-envelope receipt");
}
for (const { name, readmeState, roadmapState } of DOCUMENTED_MILESTONES) {
  if (readmeMilestones.get(name) !== readmeState) {
    errors.push(`D2 README ${name} state: expected ${readmeState}, got ${readmeMilestones.get(name) ?? "missing"}`);
  }
  if (!roadmapMilestones.get(name)?.startsWith(roadmapState)) {
    errors.push(`D3 roadmap ${name} state: expected ${roadmapState}, got ${roadmapMilestones.get(name) ?? "missing"}`);
  }
}
if (!readme.includes(CALCULATE_IMAGE_ROWS_README)) {
  errors.push("D10 README must distinguish calculateImageRows literal 9x18 default from renderImage globals");
}
if (!parity.includes(CALCULATE_IMAGE_ROWS_PARITY)) {
  errors.push("D11 parity must record calculateImageRows literal 9x18 default and numeric gap");
}

const EXPECTED_BASELINE = [
  ["Box", "runtime", "m3-enforced"], ["Component", "type", "m3-enforced"],
  ["Container", "runtime", "m3-enforced"], ["SettingsListTheme", "type", "m3-enforced"],
  ["SelectListTheme", "type", "m3-enforced"], ["Spacer", "runtime", "m3-enforced"],
  ["Text", "runtime", "m3-enforced"], ["getCapabilities", "runtime", "m5-enforced"],
  ["getImageDimensions", "runtime", "m5-enforced"], ["hyperlink", "runtime", "m5-enforced"],
  ["imageFallback", "runtime", "m5-enforced"], ["truncateToWidth", "runtime", "m3-enforced"],
];
const EXPECTED_ADDITIONS = [
  ["getKeybindings", "runtime", "m3-enforced"],
  ["stripTerminalSequences", "runtime", "m3-enforced"],
];
const tuples = (rows) => rows.map((row) => [row.name, row.kind, row.classification]);
if (!same(tuples(imports.baselineImports ?? []), EXPECTED_BASELINE)) {
  errors.push("P0 pinned v0.12.4 import inventory/classification drift");
}
if (!same(tuples(imports.currentAdditions ?? []), EXPECTED_ADDITIONS)) {
  errors.push("P1 current-main addition inventory/classification drift");
}
const expectedProvenance = {
  baseline: {
    tag: "v0.12.4", commit: "1de5a4a1c577e9ce87fd6082a6a6e143948ea2ea",
    archiveSha256: "1149ba139151fb14ded63338f24b0aaf2135d046c5dab4c9374d60731e4ea",
    shimSha256: "25842c37b6c32e4e00fd5571869e8b1f5eb89738cd554a7a6d5c0bde86e0007d",
    abiSha256: "c2f84dcba6a03a45f63d1dbca5991b149a41d5fd54e781d0cd1c975e8ce91c6d",
  },
  current: {
    version: "0.23.0", commit: "2f3ced97d0d19d9ed1e8f8c5cba6d160b7aa7339",
    archiveSha256: "e2e272eb87737014001285870bee530dc2d00a9eea5d547d50603ce54002a333",
    shimSha256: "25842c37b6c32e4e00fd5571869e8b1f5eb89738cd554a7a6d5c0bde86e0007d",
    abiSha256: "c2f84dcba6a03a45f63d1dbca5991b149a41d5fd54e781d0cd1c975e8ce91c6d",
  },
};
for (const [key, value] of Object.entries(expectedProvenance.baseline)) {
  if (imports.baseline?.[key] !== value) errors.push(`P2 baseline provenance ${key} drift`);
}
for (const [key, value] of Object.entries(expectedProvenance.current)) {
  if (imports.currentReceipt?.[key] !== value) errors.push(`P3 current provenance ${key} drift`);
}
if (imports.currentReceipt?.piTuiShimByteIdentical !== true
  || imports.currentReceipt?.abiContractByteIdentical !== true) {
  errors.push("P4 no-drift receipt must assert byte-identical shim and ABI contract");
}

const importRows = [...(imports.baselineImports ?? []), ...(imports.currentAdditions ?? [])];
const importNames = new Set();
for (const imported of importRows) {
  if (importNames.has(imported.name)) errors.push(`P5 import classified more than once: ${imported.name}`);
  importNames.add(imported.name);
  const canonical = canonicalSymbols.find(({ symbol }) => symbol.name === imported.name)?.symbol;
  if (!canonical) {
    errors.push(`P6 imported symbol is unknown to canonical barrel: ${imported.name}`);
    continue;
  }
  if (canonical.exportKind !== imported.kind) errors.push(`P7 ${imported.name}: import kind drift`);
  const row = ledgerRows.find((candidate) => candidate.name === imported.name);
  if (imported.classification.endsWith("-enforced") && row?.rustEvidence === null) {
    errors.push(`P8 ${imported.name}: active import lacks compiled mapping`);
  }
  if (imported.classification === "m5-deferred"
    && !(row?.milestone === "M5" && row?.status === "deferred" && row?.rustEvidence === null)) {
    errors.push(`P9 ${imported.name}: M5 deferral is not exact`);
  }
}
const derivedDeferred = importRows
  .filter((row) => row.classification === "m5-deferred")
  .map((row) => row.name);
if (!same(derivedDeferred, imports.m5DeferredAllowlist)) errors.push("P10 M5 deferred allowlist drift");

const activeImports = importRows.filter((row) => row.classification.endsWith("-enforced")).length;
const parityRows = tableRows(parity);
const canonicalParityRows = parityRows.filter((row) => row[0] === "canonical public API contract");
const importParityRows = parityRows.filter((row) => row[0] === "pi2dsh direct-import contract");

function checkParityNumbers(label, text, pattern, expected) {
  const actual = text.match(pattern)?.slice(1).map(Number);
  if (!actual || !same(actual, expected)) {
    errors.push(`D5 parity ${label}: expected ${expected.join("/")}, got ${actual?.join("/") ?? "missing"}`);
  }
}

if (canonicalParityRows.length !== 1) {
  errors.push(`D4 expected exactly one canonical public API parity row, got ${canonicalParityRows.length}`);
} else {
  const [, status, evidence] = canonicalParityRows[0];
  if (status !== "✅ M6 authenticated surface") {
    errors.push(`D5 parity canonical status: expected M6 authenticated surface, got ${status}`);
  }
  checkParityNumbers(
    "canonical symbol count", evidence, /(\d+)\s+exact symbols/, [derivedMetrics.symbols.total],
  );
  checkParityNumbers(
    "canonical statement count", evidence, /across\s+(\d+)\s+statements/,
    [derivedMetrics.statements.total],
  );
  checkParityNumbers(
    "compiled mappings", evidence, /(\d+)\s+compiled mappings/,
    [derivedMetrics.symbols.compiledMappings],
  );
  checkParityNumbers(
    "ported symbols", evidence, /(\d+)\s+ported\/verified/,
    [derivedMetrics.symbols.portedOrVerified],
  );
  checkParityNumbers(
    "verified symbols", evidence, /(\d+)\s+behavior-verified/, [derivedMetrics.symbols.verified],
  );
  checkParityNumbers(
    "complete statements", evidence, /(\d+)\/\d+\s+complete statements/,
    [derivedMetrics.statements.complete],
  );
  checkParityNumbers(
    "statement denominator", evidence, /\d+\/(\d+)\s+complete statements/,
    [derivedMetrics.statements.total],
  );
  checkParityNumbers(
    "verified statements", evidence, /\((\d+)\s+fully verified\)/,
    [derivedMetrics.statements.verified],
  );
  const expectedM3State = derivedMetrics.symbols.portedOrVerified >= M3_TARGET ? "green" : "red";
  const m3Match = evidence.match(/independent\s+(\d+)-symbol M3 receipt is\s+(green|red)/);
  if (!m3Match || Number(m3Match[1]) !== M3_TARGET || m3Match[2] !== expectedM3State) {
    errors.push(
      `D5 parity M3 receipt: expected ${M3_TARGET}/${expectedM3State}, `
      + `got ${m3Match ? `${m3Match[1]}/${m3Match[2]}` : "missing"}`,
    );
  }
  checkParityNumbers(
    "canonical remaining",
    evidence,
    /remaining\s+(\d+)\s+symbols/,
    [derivedMetrics.symbols.total - derivedMetrics.symbols.portedOrVerified],
  );
}

if (importParityRows.length !== 1) {
  errors.push(`D4 expected exactly one pi2dsh direct-import parity row, got ${importParityRows.length}`);
} else {
  const [, status, evidence] = importParityRows[0];
  const expectedStatus = derivedDeferred.length === 0 ? "✅ zero deferrals" : "🟨 deferrals remain";
  if (status !== expectedStatus) {
    errors.push(`D6 parity import status: expected ${expectedStatus}, got ${status}`);
  }
  checkParityNumbers(
    "active imports",
    evidence,
    /all\s+(\d+)\s+imported names now require compiled mappings/,
    [activeImports],
  );
  const deferralMatch = evidence.match(/M5 deferral allowlist is\s+(empty|non-empty)/);
  const expectedDeferral = derivedDeferred.length === 0 ? "empty" : "non-empty";
  if (!deferralMatch || deferralMatch[1] !== expectedDeferral) {
    errors.push(`D6 parity deferral state: expected ${expectedDeferral}, got ${deferralMatch?.[1] ?? "missing"}`);
  }
}

function checkRoadmapNumbers(label, text, pattern, expected) {
  const actual = text.match(pattern)?.slice(1).map(Number);
  if (!actual || !same(actual, expected)) {
    errors.push(`D8 roadmap ${label}: expected ${expected.join("/")}, got ${actual?.join("/") ?? "missing"}`);
  }
}

const roadmapM3Rows = roadmapRows.filter((row) => row[0] === "M3");
const roadmapM5Rows = roadmapRows.filter((row) => row[0] === "M5");
if (roadmapM3Rows.length !== 1) {
  errors.push(`D7 expected exactly one roadmap M3 row, got ${roadmapM3Rows.length}`);
} else {
  const [, , status, evidence] = roadmapM3Rows[0];
  if (status !== "✅ target and receipts complete") {
    errors.push(`D8 roadmap M3 status: expected target and receipts complete, got ${status}`);
  }
  checkRoadmapNumbers(
    "M3 ported symbols", evidence, /(\d+)\/(\d+)\s+ported or verified/,
    [derivedMetrics.symbols.portedOrVerified, derivedMetrics.symbols.total],
  );
  checkRoadmapNumbers(
    "M3 verified symbols", evidence, /(\d+)\/(\d+)\s+behavior-verified/,
    [derivedMetrics.symbols.verified, derivedMetrics.symbols.total],
  );
  checkRoadmapNumbers(
    "M3 compiled mappings", evidence, /(\d+)\s+compiled mappings/,
    [derivedMetrics.symbols.compiledMappings],
  );
  checkRoadmapNumbers(
    "M3 complete statements", evidence, /(\d+)\/(\d+)\s+complete statements/,
    [derivedMetrics.statements.complete, derivedMetrics.statements.total],
  );
  const m3Receipt = evidence.match(/at\s+(\d+)\/(\d+)\s+are\s+(green|red)/);
  const expectedM3State = derivedMetrics.symbols.portedOrVerified >= M3_TARGET ? "green" : "red";
  if (!m3Receipt
    || Number(m3Receipt[1]) !== derivedMetrics.symbols.portedOrVerified
    || Number(m3Receipt[2]) !== M3_TARGET
    || m3Receipt[3] !== expectedM3State) {
    errors.push(
      `D8 roadmap M3 receipt: expected ${derivedMetrics.symbols.portedOrVerified}/${M3_TARGET}/${expectedM3State}, `
      + `got ${m3Receipt ? `${m3Receipt[1]}/${m3Receipt[2]}/${m3Receipt[3]}` : "missing"}`,
    );
  }
}

if (roadmapM5Rows.length !== 1) {
  errors.push(`D7 expected exactly one roadmap M5 row, got ${roadmapM5Rows.length}`);
} else {
  const [, , status, evidence] = roadmapM5Rows[0];
  if (status !== "✅ acceptance complete") {
    errors.push(`D9 roadmap M5 status: expected acceptance complete, got ${status}`);
  }
  const importReceipt = evidence.match(
    /all\s+(\d+)\s+downstream imports are enforced with\s+(zero|\d+)\s+deferrals/,
  );
  const expectedDeferred = derivedDeferred.length === 0 ? "zero" : String(derivedDeferred.length);
  if (!importReceipt
    || Number(importReceipt[1]) !== activeImports
    || importReceipt[2] !== expectedDeferred) {
    errors.push(
      `D9 roadmap M5 imports: expected ${activeImports}/${expectedDeferred}, `
      + `got ${importReceipt ? `${importReceipt[1]}/${importReceipt[2]}` : "missing"}`,
    );
  }
  checkNapiDocReceipt("roadmap M5 row", evidence);
  checkTuiDocBoundary("roadmap M5 row", evidence);
  for (const receipt of ["0.84.2", "0.84.4", "real-tmux"]) {
    if (!compactDoc(evidence).includes(receipt)) {
      errors.push(`N15 roadmap M5 row omits: ${receipt}`);
    }
  }
}

const napiParityRows = parityRows.filter((row) => row[0] === "Private Node-API package");
if (napiParityRows.length !== 1) {
  errors.push(`N12 expected exactly one private Node-API parity row, got ${napiParityRows.length}`);
} else {
  const [, status, evidence] = napiParityRows[0];
  if (status !== `✅ reviewed ${EXPECTED_NAPI_RUNTIME_EXPORTS.length}/${canonicalRuntimeNames.length} runtime + 133-name types`) {
    errors.push(`N13 private Node-API parity status drift: ${status}`);
  }
  checkNapiDocReceipt("private Node-API parity row", `${status} ${evidence}`);
  for (const receipt of ["authenticated 0.84.1 Rust-oracle provenance", "authenticated 0.84.2", "0.84.4", "clean alias install"]) {
    if (!compactDoc(evidence).includes(receipt)) {
      errors.push(`N16 private Node-API parity row omits: ${receipt}`);
    }
  }
}

const tuiParityRows = parityRows.filter((row) => row[0] === "Shared TuiBase controller lifecycle");
if (tuiParityRows.length !== 1) {
  errors.push(`T16 expected exactly one shared TuiBase parity row, got ${tuiParityRows.length}`);
} else {
  const [, status, evidence] = tuiParityRows[0];
  if (status !== "✅ reviewed shared controller") {
    errors.push(`T17 shared TuiBase parity status drift: ${status}`);
  }
  const normalized = compactDoc(evidence);
  const required = [
    "crates/pie-app/tests/fixtures/tui-controller.json",
    TUI_CONTROLLER_TEST,
    TUI_CONTRACT_TEST,
    "tools/check-tui-controller-mutations.sh",
    "exact 0.84.1 29-case oracle",
    "52 product mutations plus 33 oracle-provenance mutations",
    "screen-agnostic injected-host TuiBase foundation",
    "reviewed injected Rust Main/Alt controllers build on it",
    "production host/event loop",
    "NAPI surface",
    "drop-in package",
  ];
  for (const receipt of required) {
    if (!normalized.includes(receipt)) {
      errors.push(`T18 shared TuiBase parity row omits: ${receipt}`);
    }
  }
}

const mainAltParityRows = parityRows.filter((row) => row[0] === "Main/Alt TUI lifecycle");
if (mainAltParityRows.length !== 1) {
  errors.push(`A10 expected exactly one injected Main/Alt parity row, got ${mainAltParityRows.length}`);
} else {
  const [, status, evidence] = mainAltParityRows[0];
  if (status !== "✅ injected Rust + production facade") {
    errors.push(`A11 injected Main/Alt parity status drift: ${status}`);
  }
  const normalized = compactDoc(evidence);
  const required = [
    "pinned 0.84.1 12-case Rust oracle",
    "20 product/17 provenance mutations",
    "tools/tmux-napi-smoke.sh",
    "production ProcessTerminal",
    "canonical JavaScript classes",
    "tools/check-current-dsh-consumer.sh",
    "0.84.2 front-door consumer",
  ];
  for (const receipt of required) {
    if (!normalized.includes(receipt)) {
      errors.push(`A12 injected Main/Alt parity row omits: ${receipt}`);
    }
  }
}

if (errors.length === 0) {
  const generatedLedger = spawnSync(process.execPath, ["tools/build-coverage-ledger.mjs", "--check"], {
    cwd: root,
    encoding: "utf8",
  });
  if (generatedLedger.status !== 0) {
    const output = `${generatedLedger.stdout ?? ""}\n${generatedLedger.stderr ?? ""}`.trim();
    errors.push(`L21 generated coverage ledger is stale (exit ${generatedLedger.status}):\n${output}`);
  }
}

if (errors.length === 0) {
  const result = spawnSync("cargo", ["test", "-p", "pie-napi", "--test", "api_contract", "--quiet"], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`.trim().split("\n").slice(-12).join("\n");
    errors.push(`R0 compiled Rust contract failed (exit ${result.status}):\n${output}`);
  }
}

if (milestoneMode && derivedMetrics.symbols.portedOrVerified < M3_TARGET) {
  errors.push(
    `M3 target unmet: ${derivedMetrics.symbols.portedOrVerified}/${CANONICAL.symbolCount} < ${M3_TARGET} `
    + `(gap ${M3_TARGET - derivedMetrics.symbols.portedOrVerified})`,
  );
}

const summary = `symbols ${derivedMetrics.symbols.portedOrVerified}/${CANONICAL.symbolCount} ported, ${derivedMetrics.symbols.verified}/${CANONICAL.symbolCount} verified, ${derivedMetrics.symbols.compiledMappings} compiled mappings; statements ${derivedMetrics.statements.complete}/${CANONICAL.statementCount} complete, ${derivedMetrics.statements.verified}/${CANONICAL.statementCount} verified; imports ${activeImports} active, ${derivedDeferred.length} M5-deferred; private NAPI ${selectedNapiRuntimeExports.length}/${canonicalRuntimeNames.length} runtime exports`;
if (errors.length > 0) {
  console.error(errors.join("\n"));
  console.error(`api-conformance receipt: ${summary}`);
  process.exit(1);
}
console.log(`api-conformance OK: ${summary}`);
