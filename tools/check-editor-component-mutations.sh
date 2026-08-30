#!/bin/sh
# Prove that the focused Editor/Input tests kill each required parity regression.
# The repository worktree is never edited: mutations run in a git-archive sandbox.
set -eu

REPOSITORY_ROOT=$(git rev-parse --show-toplevel)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-editor-mutations.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"
EDITOR_SOURCE="$MUTATION_ROOT/crates/pie-components/src/editor.rs"
INPUT_SOURCE="$MUTATION_ROOT/crates/pie-components/src/input.rs"
MODEL_SOURCE="$MUTATION_ROOT/crates/pie-core/src/editor_model.rs"
cp "$EDITOR_SOURCE" "$MUTATION_ROOT/editor.rs.original"
cp "$INPUT_SOURCE" "$MUTATION_ROOT/input.rs.original"
cp "$MODEL_SOURCE" "$MUTATION_ROOT/editor_model.rs.original"

RUST_TOOLCHAIN=${PIE_RUST_TOOLCHAIN:-1.98.0}
TARGET_DIRECTORY="$MUTATION_ROOT/target"
REFERENCE_DIST=${PI_TUI_DIST:?PI_TUI_DIST must point to the exact pi-tui 0.84.1 dist}
REFERENCE_PACKAGE=$(CDPATH= cd "$REFERENCE_DIST/.." && pwd)
MUTATED_REFERENCE="$MUTATION_ROOT/reference-package"
mkdir -p "$MUTATED_REFERENCE"
cp -R "$REFERENCE_PACKAGE/." "$MUTATED_REFERENCE"
cp "$MUTATED_REFERENCE/dist/keybindings.js" "$MUTATION_ROOT/keybindings.js.original"
cp "$MUTATED_REFERENCE/dist/keys.js" "$MUTATION_ROOT/keys.js.original"
cp "$MUTATED_REFERENCE/node_modules/get-east-asian-width/lookup-data.js" "$MUTATION_ROOT/lookup-data.js.original"

apply_mutation() {
    mutation_name=$1
    cp "$MUTATION_ROOT/editor.rs.original" "$EDITOR_SOURCE"
    cp "$MUTATION_ROOT/input.rs.original" "$INPUT_SOURCE"
    cp "$MUTATION_ROOT/editor_model.rs.original" "$MODEL_SOURCE"
    MUTATION_NAME="$mutation_name" MUTATION_ROOT="$MUTATION_ROOT" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const mutations = {
  "capture-input-keybindings": {
    file: "crates/pie-components/src/input.rs",
    before: "        let keybindings = get_keybindings();",
    after: "        let keybindings = pie_core::keybindings::KeybindingsManager::with_tui_defaults(Vec::new());",
  },
  "leak-utf8-input-cursor": {
    file: "crates/pie-components/src/input.rs",
    before: "    text.encode_utf16().count()",
    after: "    text.len()",
  },
  "reverse-submit-callback-order": {
    file: "crates/pie-core/src/editor_model.rs",
    before: "        vec![\n            EditorEffect::Change(String::new()),\n            EditorEffect::Submit(result),\n        ]",
    after: "        vec![\n            EditorEffect::Submit(result),\n            EditorEffect::Change(String::new()),\n        ]",
  },
  "lower-paste-marker-threshold": {
    file: "crates/pie-core/src/editor_model.rs",
    before: "        if line_count > 10 || total_units > 1000 {",
    after: "        if line_count >= 10 || total_units > 1000 {",
  },
  "trust-literal-paste-marker": {
    file: "crates/pie-components/src/editor.rs",
    before: "            .any(|(id, _)| marker.starts_with(&format!(\"[paste #{id}\")))",
    after: "            .all(|_| true)",
  },
  "drift-autocomplete-debounce": {
    file: "crates/pie-components/src/editor.rs",
    before: "const AUTOCOMPLETE_DEBOUNCE_MS: u64 = 20;",
    after: "const AUTOCOMPLETE_DEBOUNCE_MS: u64 = 21;",
  },
  "launch-autocomplete-concurrently": {
    file: "crates/pie-components/src/editor.rs",
    before: "        if self.active_autocomplete.is_some() {",
    after: "        if self.active_autocomplete.is_none() {",
  },
  "drop-forced-autocomplete-flag": {
    file: "crates/pie-components/src/editor.rs",
    before: "        let options = AutocompleteOptions {\n            force: start.force,\n            signal: controller.signal(),\n        };",
    after: "        let options = AutocompleteOptions {\n            force: false,\n            signal: controller.signal(),\n        };",
  },
  "retain-autocomplete-future-after-drop": {
    file: "crates/pie-components/src/editor.rs",
    before: "            self.host.discard_autocomplete(active.request_id);",
    after: "            let _ = active.request_id;",
  },
  "preserve-input-paste-newlines": {
    file: "crates/pie-components/src/input.rs",
    before: "            .replace(['\\r', '\\n'], \"\")",
    after: "            .replace(['\\r', '\\n'], \" \")",
  },
  "drop-input-viewport-centering": {
    file: "crates/pie-components/src/input.rs",
    before: "                let half = scroll_width / 2;",
    after: "                let half = 0;",
  },
  "eager-noncursor-visual-subtraction": {
    file: "crates/pie-components/src/editor.rs",
    before: "                has_cursor.then(|| snapshot.cursor.col - visual.start_col),",
    after: "                has_cursor.then_some(snapshot.cursor.col - visual.start_col),",
  },
  "truncate-jump-chunk": {
    file: "crates/pie-components/src/editor.rs",
    before: "                    .map(|_| data.to_owned())",
    after: "                    .map(|character| character.to_string())",
  },
  "retain-history-autocomplete": {
    file: "crates/pie-components/src/editor.rs",
    before: "            if history {\n                self.cancel_autocomplete();\n            }",
    after: "            if false && history {\n                self.cancel_autocomplete();\n            }",
  },
  "refresh-nonrefresh-actions": {
    file: "crates/pie-components/src/editor.rs",
    before: "            self.apply_action(action);\n            if deletion {",
    after: "            self.apply_action(action);\n            if !history && !deletion {\n                self.refresh_open_autocomplete();\n            }\n            if deletion {",
  },
  "skip-horizontal-requery": {
    file: "crates/pie-components/src/editor.rs",
    before: "        if keybindings.matches(data, \"tui.editor.cursorLeft\") {\n            self.apply_action(EditorAction::MoveLeft);\n            self.refresh_open_autocomplete();\n            return;\n        }",
    after: "        if keybindings.matches(data, \"tui.editor.cursorLeft\") {\n            self.apply_action(EditorAction::MoveLeft);\n            return;\n        }",
  },
  "skip-deletion-retrigger": {
    file: "crates/pie-components/src/editor.rs",
    before: "        if self.in_slash_context(before) || self.in_trigger_context(before) {\n            self.request_autocomplete(false, false);\n        }",
    after: "        if false && (self.in_slash_context(before) || self.in_trigger_context(before)) {\n            self.request_autocomplete(false, false);\n        }",
  },
  "emit-slash-intermediate-change": {
    file: "crates/pie-components/src/editor.rs",
    before: "        self.apply_completion(item, &menu.prefix, !(slash && suppress_slash_change));",
    after: "        self.apply_completion(item, &menu.prefix, true);",
  },
  "use-ascii-trigger-whitespace": {
    file: "crates/pie-components/src/editor.rs",
    before: "        let token = before.rsplit(js_is_whitespace).next().unwrap_or(before);",
    after: "        let token = before.rsplit([' ', '\\t']).next().unwrap_or(before);",
  },
  "ignore-live-shift-enter-binding": {
    file: "crates/pie-components/src/editor.rs",
    before: "        let submit_keys = keybindings.get_keys(\"tui.input.submit\");",
    after: "        let submit_keys = Vec::<String>::new();",
  },
};

const mutation = process.env.MUTATION_NAME;
const entry = mutations[mutation];
if (!entry) throw new Error(`unknown mutation: ${mutation}`);
const sourcePath = join(process.env.MUTATION_ROOT, entry.file);
const source = readFileSync(sourcePath, "utf8");
const occurrences = source.split(entry.before).length - 1;
if (occurrences !== 1) {
  throw new Error(`${mutation}: expected one source marker, found ${occurrences}`);
}
writeFileSync(sourcePath, source.replace(entry.before, entry.after));
NODE
}

expect_killed() {
    mutation_name=$1
    test_binary=$2
    test_name=$3
    apply_mutation "$mutation_name"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if CARGO_TARGET_DIR="$TARGET_DIRECTORY" rustup run "$RUST_TOOLCHAIN" cargo test \
        --manifest-path "$MUTATION_ROOT/Cargo.toml" \
        -p pie-components --test "$test_binary" "$test_name" -- --exact \
        >"$log_path" 2>&1
    then
        printf 'mutation survived: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    if ! grep -Eq 'assertion .* failed|test .* FAILED|test result: FAILED' "$log_path"; then
        printf 'mutation did not reach its test: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    printf 'mutation killed: %s\n' "$mutation_name"
}

expect_oracle_copy_killed() {
    mutation_name=$1
    target_path=$2
    copied_path=$3
    expected_digest=$4
    cp "$MUTATION_ROOT/keybindings.js.original" "$MUTATED_REFERENCE/dist/keybindings.js"
    cp "$MUTATION_ROOT/keys.js.original" "$MUTATED_REFERENCE/dist/keys.js"
    cp "$MUTATION_ROOT/lookup-data.js.original" "$MUTATED_REFERENCE/node_modules/get-east-asian-width/lookup-data.js"
    cp "$MUTATED_REFERENCE/$copied_path" "$MUTATED_REFERENCE/$target_path"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if PI_TUI_DIST="$MUTATED_REFERENCE/dist" node \
        "$MUTATION_ROOT/tools/golden/gen-golden-editor-components.mjs" --check \
        >"$log_path" 2>&1
    then
        printf 'oracle mutation survived: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    if ! grep -Fq "$expected_digest digest mismatch" "$log_path"; then
        printf 'oracle mutation did not reach its pre-import digest gate: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    printf 'oracle mutation killed: %s\n' "$mutation_name"
}

expect_oracle_copy_killed copied-keybindings dist/keybindings.js dist/keys.js keybindingsJs
expect_oracle_copy_killed copied-keys dist/keys.js dist/keybindings.js keysJs
expect_oracle_copy_killed copied-lookup-data \
    node_modules/get-east-asian-width/lookup-data.js \
    node_modules/get-east-asian-width/lookup.js \
    eastAsianWidthLookupDataJs

expect_killed capture-input-keybindings input_golden input_kill_yank_undo_live_keys_and_host_segmenter_are_surfaced
expect_killed leak-utf8-input-cursor input_golden input_defaults_callbacks_paste_unicode_and_viewport_match_oracle
expect_killed reverse-submit-callback-order editor_input_golden editor_render_input_paste_history_and_effects_match_oracle
expect_killed lower-paste-marker-threshold editor_input_golden editor_render_input_paste_history_and_effects_match_oracle
expect_killed trust-literal-paste-marker editor_input_golden editor_render_input_paste_history_and_effects_match_oracle
expect_killed drift-autocomplete-debounce editor_autocomplete custom_trigger_requires_a_token_boundary_and_resets_at_exactly_20ms
expect_killed launch-autocomplete-concurrently editor_autocomplete autocomplete_serializes_supersession_and_stale_results_cannot_win
expect_killed drop-forced-autocomplete-flag editor_autocomplete debounce_escape_force_drop_and_provider_replacement_are_causal
expect_killed retain-autocomplete-future-after-drop editor_autocomplete debounce_escape_force_drop_and_provider_replacement_are_causal
expect_killed preserve-input-paste-newlines input_golden input_defaults_callbacks_paste_unicode_and_viewport_match_oracle
expect_killed drop-input-viewport-centering input_golden input_defaults_callbacks_paste_unicode_and_viewport_match_oracle
expect_killed eager-noncursor-visual-subtraction editor_model_surface public_editor_surface_replays_pinned_model_atoms
expect_killed truncate-jump-chunk editor_input_golden editor_jump_chunk_matches_oracle
expect_killed retain-history-autocomplete editor_autocomplete pending_and_history_autocomplete_lifecycle_match_oracle
expect_killed refresh-nonrefresh-actions editor_autocomplete non_refresh_actions_match_oracle
expect_killed skip-horizontal-requery editor_autocomplete open_menu_horizontal_requery_matches_oracle
expect_killed skip-deletion-retrigger editor_autocomplete deletion_retrigger_matches_oracle
expect_killed emit-slash-intermediate-change editor_autocomplete autocomplete_confirmation_callbacks_match_oracle
expect_killed use-ascii-trigger-whitespace editor_autocomplete continued_trigger_js_whitespace_matches_oracle
expect_killed ignore-live-shift-enter-binding editor_input_golden editor_live_newline_binding_matches_oracle
