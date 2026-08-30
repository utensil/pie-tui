#!/bin/sh
# Prove the bounded TuiBase ledger/documentation receipt with one mutation at a time.
# The source worktree is never edited: mutations run in a git-archive sandbox
# overlaid with the current receipt files so the gate is usable before commit.

set -eu

REPOSITORY_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-controller-receipt.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"

RECEIPT_FILES='README.md
docs/roadmap.md
docs/parity.md
tools/build-coverage-ledger.mjs
tools/surface-coverage.json
tools/check-coverage.mjs
tools/check-tui-controller-receipt-mutations.sh
crates/pie-app/tests/fixtures/tui-controller.json
crates/pie-components/src/tui.rs
tools/check-tui-controller-mutations.sh'
RECEIPT_FILES="$RECEIPT_FILES
crates/pie-app/tests/fixtures/main-alt-controller.json
tools/check-main-alt-product-mutations.sh
tools/check-main-alt-oracle-mutations.sh
adapters/pie-napi/runtime.cjs
adapters/pie-napi/index.js
adapters/pie-napi/index.d.ts
adapters/pie-napi/README.md
adapters/pie-napi/package.json
adapters/pie-napi/test/oracle-contract.json
adapters/pie-napi/test/check-type-surface.mjs
adapters/pie-napi/test/check-upstream-drift.mjs
adapters/pie-napi/test/m5-runtime.test.mjs
adapters/pie-napi/test/differential.mjs
adapters/pie-napi/test/pack-consumer.mjs
adapters/pie-napi/test/check-mutations.mjs
adapters/pie-napi/test/upstream-drift.json
tools/tmux-napi-smoke.sh
tools/check-current-dsh-consumer.sh"

restore_receipt() {
    printf '%s\n' "$RECEIPT_FILES" | while IFS= read -r path; do
        mkdir -p "$(dirname "$MUTATION_ROOT/$path")"
        cp "$REPOSITORY_ROOT/$path" "$MUTATION_ROOT/$path"
    done
}

apply_mutation() {
    mutation_name=$1
    MUTATION_NAME="$mutation_name" MUTATION_ROOT="$MUTATION_ROOT" node --input-type=module <<'NODE'
import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.env.MUTATION_ROOT;
const pathFor = (path) => join(root, path);
const replaceOnce = (path, before, after) => {
  const absolute = pathFor(path);
  const source = readFileSync(absolute, "utf8");
  const count = source.split(before).length - 1;
  if (count !== 1) throw new Error(`${path}: expected one mutation marker, found ${count}`);
  writeFileSync(absolute, source.replace(before, after));
};
const mutateLedger = (name, callback) => {
  const path = pathFor("tools/surface-coverage.json");
  const ledger = JSON.parse(readFileSync(path, "utf8"));
  const row = ledger.symbols.find((entry) => entry.statementId === "S27-tui" && entry.name === name);
  if (!row) throw new Error(`missing S27 row: ${name}`);
  callback(row, ledger);
  writeFileSync(path, `${JSON.stringify(ledger, null, 2)}\n`);
};
const mutateS28Ledger = (name, callback) => {
  const path = pathFor("tools/surface-coverage.json");
  const ledger = JSON.parse(readFileSync(path, "utf8"));
  const row = ledger.symbols.find(
    (entry) => entry.statementId === "S28-tui-alt-screen" && entry.name === name,
  );
  if (!row) throw new Error(`missing S28 row: ${name}`);
  callback(row, ledger);
  writeFileSync(path, `${JSON.stringify(ledger, null, 2)}\n`);
};
const duplicateParityRow = () => {
  const path = pathFor("docs/parity.md");
  const source = readFileSync(path, "utf8");
  const row = source.split("\n").find((line) => line.startsWith("| Shared TuiBase controller lifecycle |"));
  if (!row) throw new Error("shared TuiBase parity row missing");
  replaceOnce("docs/parity.md", row, `${row}\n${row}`);
};
const duplicateMainAltParityRow = () => {
  const path = pathFor("docs/parity.md");
  const source = readFileSync(path, "utf8");
  const row = source.split("\n").find((line) => line.startsWith("| Main/Alt TUI lifecycle |"));
  if (!row) throw new Error("injected Main/Alt parity row missing");
  replaceOnce("docs/parity.md", row, `${row}\n${row}`);
};

const mutations = {
  "generator-removes-overlay-anchor": () => replaceOnce(
    "tools/build-coverage-ledger.mjs",
    '"setCellDimensions", "OverlayAnchor", "OverlayHandle"',
    '"setCellDimensions", "OverlayHandle"',
  ),
  "ledger-ports-overlay-anchor": () => mutateLedger("OverlayAnchor", (row) => {
    row.status = "ported";
  }),
  "ledger-fakes-overlay-rust-evidence": () => mutateLedger("OverlayHandle", (row) => {
    row.rustEvidence = { export: "pie_napi::pi_tui::OverlayHandle" };
  }),
  "ledger-removes-controller-evidence": () => mutateLedger("TuiMode", (row) => {
    row.behaviorEvidence = [];
  }),
  "ledger-weakens-tui-gap": () => mutateLedger("TUI", (row) => {
    row.gaps = ["Shared lifecycle exists."];
  }),
  "ledger-regresses-component-gap": () => mutateLedger("Component", (row) => {
    row.gaps = ["Rust Component does not expose the optional wantsKeyRelease property."];
  }),
  "ledger-moves-is-focusable": () => mutateLedger("isFocusable", (row) => {
    row.status = "partial";
  }),
  "fixture-drops-oracle-case": () => {
    const path = pathFor("crates/pie-app/tests/fixtures/tui-controller.json");
    const fixture = JSON.parse(readFileSync(path, "utf8"));
    fixture.cases.pop();
    writeFileSync(path, `${JSON.stringify(fixture, null, 2)}\n`);
  },
  "evidence-removes-rank-two-source": () => {
    unlinkSync(pathFor("crates/pie-components/src/tui.rs"));
  },
  "mutation-cardinality-drops-product-case": () => replaceOnce(
    "tools/check-tui-controller-mutations.sh",
    "expect_killed reverse-listener-order pie-app tui_controller listener_transform_consume_release_and_debug_priority_match\n",
    "",
  ),
  "generator-removes-alt-partials": () => replaceOnce(
    "tools/build-coverage-ledger.mjs",
    '"TuiStopOptions", "ViewportTUI", "TuiAltScreen", "TuiAltScreenOptions",',
    '"TuiStopOptions", "ViewportTUI",',
  ),
  "ledger-defers-alt-screen": () => mutateS28Ledger("TuiAltScreen", (row) => {
    row.status = "deferred";
  }),
  "ledger-removes-alt-evidence": () => mutateS28Ledger("TuiAltScreenOptions", (row) => {
    row.behaviorEvidence = [];
  }),
  "main-alt-fixture-drops-oracle-case": () => {
    const path = pathFor("crates/pie-app/tests/fixtures/main-alt-controller.json");
    const fixture = JSON.parse(readFileSync(path, "utf8"));
    fixture.cases.pop();
    writeFileSync(path, `${JSON.stringify(fixture, null, 2)}\n`);
  },
  "main-alt-product-cardinality-drops-case": () => replaceOnce(
    "tools/check-main-alt-product-mutations.sh",
    "expect_killed skip-active-alt-drop-stop pie-app main_alt_controller",
    "expect_not_killed skip-active-alt-drop-stop pie-app main_alt_controller",
  ),
  "main-alt-oracle-cardinality-drops-copy": () => replaceOnce(
    "tools/check-main-alt-oracle-mutations.sh",
    "expect_copied_source_killed copied-width-lookup-data",
    "expect_not_copied_source_killed copied-width-lookup-data",
  ),
  "napi-runtime-swaps-first-two-exports": () => replaceOnce(
    "adapters/pie-napi/runtime.cjs",
    "  Box,\n  CURSOR_MARKER,",
    "  CURSOR_MARKER,\n  Box,",
  ),
  "napi-package-readme-regresses-count": () => replaceOnce(
    "adapters/pie-napi/README.md",
    "complete 69-export runtime namespace shared by pi-tui 0.84.1 and",
    "complete 68-export runtime namespace shared by pi-tui 0.84.1 and",
  ),
  "readme-status-drops-deterministic-host": () => replaceOnce(
    "README.md",
    "declaration namespace; production ProcessTerminal",
    "declaration namespace; production terminal",
  ),
  "readme-m5-drops-main-alt-boundary": () => replaceOnce(
    "README.md",
    "production ProcessTerminal and canonical Main/Alt facades",
    "production ProcessTerminal and canonical facades",
  ),
  "readme-limit-drops-production-host": () => replaceOnce(
    "README.md",
    "adapter now supplies production host/event-loop wiring",
    "adapter now supplies host/event-loop wiring",
  ),
  "readme-status-regresses-napi-count": () => replaceOnce(
    "README.md",
    "private package exposes all 69 canonical runtime",
    "private package exposes all 68 canonical runtime",
  ),
  "readme-limit-regresses-napi-residual": () => replaceOnce(
    "README.md",
    "exposes exactly 69/69 canonical runtime",
    "exposes exactly 68/69 canonical runtime",
  ),
  "readme-oracle-command-suffix": () => replaceOnce(
    "README.md",
    "node tools/golden/gen-golden-tui-controller.mjs --check\n",
    "node tools/golden/gen-golden-tui-controller.mjs --check-mutated\n",
  ),
  "readme-mutation-command-suffix": () => replaceOnce(
    "README.md",
    "tools/check-tui-controller-mutations.sh\n",
    "tools/check-tui-controller-mutations.sh-mutated\n",
  ),
  "readme-main-alt-oracle-command-suffix": () => replaceOnce(
    "README.md",
    "node tools/golden/gen-golden-main-alt-controller.mjs --check\n",
    "node tools/golden/gen-golden-main-alt-controller.mjs --check-mutated\n",
  ),
  "readme-main-alt-product-command-suffix": () => replaceOnce(
    "README.md",
    "tools/check-main-alt-product-mutations.sh\n",
    "tools/check-main-alt-product-mutations.sh-mutated\n",
  ),
  "readme-main-alt-tmux-command-suffix": () => replaceOnce(
    "README.md",
    "tools/tmux-main-alt-smoke.sh\n",
    "tools/tmux-main-alt-smoke.sh-mutated\n",
  ),
  "roadmap-drops-shared-controller": () => replaceOnce(
    "docs/roadmap.md",
    "production ProcessTerminal and canonical Main/Alt facades",
    "production terminal and canonical Main/Alt facades",
  ),
  "roadmap-drops-production-host": () => replaceOnce(
    "docs/roadmap.md",
    "current `dsh-pi-tui-mono` front-door execution",
    "pinned dsh front-door execution",
  ),
  "roadmap-regresses-napi-residual": () => replaceOnce(
    "docs/roadmap.md",
    "explicit 0.84.4 drift",
    "explicit 0.84.5 drift",
  ),
  "parity-duplicates-controller-row": duplicateParityRow,
  "parity-removes-structural-evidence": () => replaceOnce(
    "docs/parity.md",
    "crates/pie-components/tests/tui_contracts.rs",
    "crates/pie-components/tests/tui_contract.rs",
  ),
  "parity-drops-main-alt-boundary": () => replaceOnce(
    "docs/parity.md",
    "reviewed injected Rust Main/Alt controllers build on it",
    "screen controllers build on it",
  ),
  "parity-duplicates-main-alt-row": duplicateMainAltParityRow,
  "parity-removes-main-alt-test-evidence": () => replaceOnce(
    "docs/parity.md",
    "`tools/tmux-napi-smoke.sh` adds production ProcessTerminal",
    "`tools/tmux-napi-smok.sh` adds production ProcessTerminal",
  ),
  "parity-regresses-napi-residual": () => replaceOnce(
    "docs/parity.md",
    "unchanged 0.84.2 runtime surface",
    "changed 0.84.2 runtime surface",
  ),
};

const mutation = mutations[process.env.MUTATION_NAME];
if (!mutation) throw new Error(`unknown mutation: ${process.env.MUTATION_NAME}`);
mutation();
NODE
}

run_check() {
    CARGO_TARGET_DIR="$MUTATION_ROOT/target" node "$MUTATION_ROOT/tools/check-coverage.mjs"
}

restore_receipt
baseline_log="$MUTATION_ROOT/baseline.log"
if ! run_check >"$baseline_log" 2>&1; then
    printf 'baseline TuiBase receipt failed\n' >&2
    cat "$baseline_log" >&2
    exit 1
fi

expect_killed() {
    mutation_name=$1
    expected=$2
    restore_receipt
    apply_mutation "$mutation_name"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if run_check >"$log_path" 2>&1; then
        printf 'receipt mutation survived: %s\n' "$mutation_name" >&2
        exit 1
    fi
    if ! grep -Fq "$expected" "$log_path"; then
        printf 'receipt mutation missed expected gate: %s (%s)\n' "$mutation_name" "$expected" >&2
        cat "$log_path" >&2
        exit 1
    fi
    printf 'receipt mutation killed: %s\n' "$mutation_name"
}

expect_killed generator-removes-overlay-anchor "L21 generated coverage ledger is stale"
expect_killed ledger-ports-overlay-anchor "T0 exact S27 status/compiled-evidence disposition drift"
expect_killed ledger-fakes-overlay-rust-evidence "T0 exact S27 status/compiled-evidence disposition drift"
expect_killed ledger-removes-controller-evidence "T1 TuiMode partial/evidence/gap receipt drift"
expect_killed ledger-weakens-tui-gap "T1 TUI partial/evidence/gap receipt drift"
expect_killed ledger-regresses-component-gap "T3 Component wantsKeyRelease/focus evidence-gap receipt drift"
expect_killed ledger-moves-is-focusable "T2 isFocusable must remain an unmapped S27 deferral"
expect_killed fixture-drops-oracle-case "T6 pinned 0.84.1 TuiBase 29-case oracle receipt drift"
expect_killed evidence-removes-rank-two-source "T5 TuiBase checkpoint evidence missing"
expect_killed mutation-cardinality-drops-product-case "T7 TuiBase mutation cardinality drift"
expect_killed generator-removes-alt-partials "L21 generated coverage ledger is stale"
expect_killed ledger-defers-alt-screen "A0 exact S28 status/compiled-evidence disposition drift"
expect_killed ledger-removes-alt-evidence "A1 TuiAltScreenOptions partial/evidence/gap receipt drift"
expect_killed main-alt-fixture-drops-oracle-case "A3 pinned 0.84.1 Main/Alt 12-case oracle receipt drift"
expect_killed main-alt-product-cardinality-drops-case "A4 Main/Alt regression cardinality drift"
expect_killed main-alt-oracle-cardinality-drops-copy "A4 Main/Alt regression cardinality drift"
expect_killed napi-runtime-swaps-first-two-exports "N7 private Node-API selected surface drift"
expect_killed napi-package-readme-regresses-count "N11 package README must retain"
expect_killed readme-status-drops-deterministic-host "T10 README status callout omits"
expect_killed readme-m5-drops-main-alt-boundary "T11 README M5 row omits"
expect_killed readme-limit-drops-production-host "T13 README TuiBase current-limit receipt drift"
expect_killed readme-status-regresses-napi-count "N8 README status callout omits"
expect_killed readme-limit-regresses-napi-residual "N14 README private-package current-limit receipt drift"
expect_killed readme-oracle-command-suffix "T14 expected one exact TuiBase oracle command"
expect_killed readme-mutation-command-suffix "T15 expected one exact TuiBase mutation command"
expect_killed readme-main-alt-oracle-command-suffix "A6 expected one exact Main/Alt oracle command"
expect_killed readme-main-alt-product-command-suffix "A8 expected one exact Main/Alt product-regression command"
expect_killed readme-main-alt-tmux-command-suffix "A9 expected one exact Main/Alt tmux command"
expect_killed roadmap-drops-shared-controller "T10 roadmap M5 row omits"
expect_killed roadmap-drops-production-host "T11 roadmap M5 row omits"
expect_killed roadmap-regresses-napi-residual "N15 roadmap M5 row omits"
expect_killed parity-duplicates-controller-row "T16 expected exactly one shared TuiBase parity row"
expect_killed parity-removes-structural-evidence "T18 shared TuiBase parity row omits"
expect_killed parity-drops-main-alt-boundary "T18 shared TuiBase parity row omits"
expect_killed parity-duplicates-main-alt-row "A10 expected exactly one injected Main/Alt parity row"
expect_killed parity-removes-main-alt-test-evidence "A12 injected Main/Alt parity row omits"
expect_killed parity-regresses-napi-residual "N16 private Node-API parity row omits"

printf 'M5 Tui/NAPI receipt mutations: OK (37/37 killed)\n'
