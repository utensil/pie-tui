#!/bin/sh
# Prove that the focused Image differential tests kill each parity regression.
# The repository worktree is never edited: every mutation runs in a git-archive
# sandbox of the current HEAD.
set -eu

REPOSITORY_ROOT=$(git rev-parse --show-toplevel)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-image-mutations.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"
IMAGE_SOURCE="$MUTATION_ROOT/crates/pie-components/src/image.rs"
ORIGINAL_SOURCE="$MUTATION_ROOT/image.rs.original"
FIXTURE_SOURCE="$MUTATION_ROOT/crates/pie-components/tests/fixtures/image-component.json"
ORIGINAL_FIXTURE="$MUTATION_ROOT/image-component.json.original"
cp "$IMAGE_SOURCE" "$ORIGINAL_SOURCE"
cp "$FIXTURE_SOURCE" "$ORIGINAL_FIXTURE"

RUST_TOOLCHAIN=${PIE_RUST_TOOLCHAIN:-1.98.0}
TARGET_DIRECTORY="$MUTATION_ROOT/target"

apply_mutation() {
    mutation_name=$1
    cp "$ORIGINAL_SOURCE" "$IMAGE_SOURCE"
    cp "$ORIGINAL_FIXTURE" "$FIXTURE_SOURCE"
    MUTATION_NAME="$mutation_name" IMAGE_SOURCE="$IMAGE_SOURCE" FIXTURE_SOURCE="$FIXTURE_SOURCE" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";

const mutations = {
  "allocate-id-per-width": {
    file: "image",
    before: "if protocol == ImageProtocol::Kitty && self.image_id.is_none() {",
    after: "if protocol == ImageProtocol::Kitty {",
  },
  "cache-capability-key": {
    file: "image",
    before: "            && cache.width == width\n",
    after: "            && cache.width == width\n            && self.environment.capabilities().images == Some(ImageProtocol::Kitty)\n",
  },
  "iterm-image-first": {
    file: "image",
    before: "            lines.push(format!(\"{move_up}{sequence}\"));",
    after: "            lines.insert(0, format!(\"{move_up}{sequence}\"));",
  },
  "reserve-one-column": {
    file: "image",
    before: "let max_width = ((width as f64) - 2.0)",
    after: "let max_width = ((width as f64) - 1.0)",
  },
  "ignore-explicit-max-height": {
    file: "image",
    before: "let max_height = self.options.max_height_cells.unwrap_or(default_max_height);",
    after: "let max_height = default_max_height;",
  },
  "fallback-unstyled": {
    file: "image",
    before: "                &self.theme.fallback(&fallback),",
    after: "                &fallback,",
  },
  "fallback-untruncated": {
    file: "image",
    before: "&self.theme.fallback(&fallback),\n                width,\n                \"...\",",
    after: "&self.theme.fallback(&fallback),\n                usize::MAX,\n                \"...\",",
  },
  "kitty-moves-cursor": {
    file: "image",
    before: "                    move_cursor: Some(false),",
    after: "                    move_cursor: Some(true),",
  },
  "empty-filename-is-present": {
    file: "image",
    before: ".as_deref()\n                    .filter(|filename| !filename.is_empty()),",
    after: ".as_deref(),",
  },
  "zero-id-owns-deletion": {
    file: "image",
    before: "let deletion_owner = image_id\n            .filter(|image_id| *image_id != 0)\n            .map(|_| KittyImageDeletionOwner::Caller);",
    after: "let deletion_owner = image_id.map(|_| KittyImageDeletionOwner::Caller);",
  },
  "default-width-eighty": {
    file: "image",
    before: "self.options.max_width_cells.unwrap_or(60.0)",
    after: "self.options.max_width_cells.unwrap_or(80.0)",
  },
  "default-height-constant": {
    file: "image",
    before: "let default_max_height = ((max_width * cell_dimensions.width_px)\n            / cell_dimensions.height_px)\n            .ceil()\n            .max(1.0);",
    after: "let default_max_height = 999.0;",
  },
  "copied-utils-dts-provenance": {
    file: "fixture",
    before: "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2",
    after: "45cfb14d766704c70017d7ec3a2d382f148fbf56b7f76c4c3155cc80bb5ff6cb",
  },
  "copied-utils-js-provenance": {
    file: "fixture",
    before: "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052",
    after: "dd6791e17fbeb0a48c2b73d521d31356edf11795e44e0fae05b5f8c322c470e1",
  },
  "array-identity-not-recorded": {
    file: "fixture",
    before: "\"sameWidthSameReference\": true,\n    \"widthMissNewReference\": true,",
    after: "\"sameWidthSameReference\": false,\n    \"widthMissNewReference\": true,",
  },
};

const mutation = process.env.MUTATION_NAME;
const entry = mutations[mutation];
if (!entry) throw new Error(`unknown mutation: ${mutation}`);
const sourcePath = entry.file === "fixture" ? process.env.FIXTURE_SOURCE : process.env.IMAGE_SOURCE;
const { before, after } = entry;
const source = readFileSync(sourcePath, "utf8");
const occurrences = source.split(before).length - 1;
if (occurrences !== 1) {
  throw new Error(`${mutation}: expected one source marker, found ${occurrences}`);
}
writeFileSync(sourcePath, source.replace(before, after));
NODE
}

expect_killed() {
    mutation_name=$1
    test_name=$2
    apply_mutation "$mutation_name"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if CARGO_TARGET_DIR="$TARGET_DIRECTORY" rustup run "$RUST_TOOLCHAIN" cargo test \
        --manifest-path "$MUTATION_ROOT/Cargo.toml" \
        -p pie-components --test golden_image_component "$test_name" -- --exact \
        >"$log_path" 2>&1
    then
        printf 'mutation survived: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    if ! grep -Eq 'test .* FAILED|test result: FAILED' "$log_path"; then
        printf 'mutation did not reach its test: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    printf 'mutation killed: %s\n' "$mutation_name"
}

expect_killed allocate-id-per-width kitty_layout_id_cache_and_ownership_match
expect_killed cache-capability-key exact_width_is_the_only_cache_key_and_invalidate_refreshes_facts
expect_killed iterm-image-first iterm_order_and_all_numeric_boundaries_match
expect_killed reserve-one-column kitty_layout_id_cache_and_ownership_match
expect_killed ignore-explicit-max-height iterm_order_and_all_numeric_boundaries_match
expect_killed fallback-unstyled fallback_is_themed_then_terminal_width_truncated
expect_killed fallback-untruncated fallback_is_themed_then_terminal_width_truncated
expect_killed kitty-moves-cursor kitty_layout_id_cache_and_ownership_match
expect_killed empty-filename-is-present fallback_is_themed_then_terminal_width_truncated
expect_killed zero-id-owns-deletion caller_zero_id_is_preserved_without_allocation_transmission_or_ownership
expect_killed default-width-eighty default_width_and_cell_aspect_height_limits_match
expect_killed default-height-constant default_width_and_cell_aspect_height_limits_match
expect_killed copied-utils-dts-provenance oracle_is_exactly_pinned_and_non_vacuous
expect_killed copied-utils-js-provenance oracle_is_exactly_pinned_and_non_vacuous
expect_killed array-identity-not-recorded oracle_is_exactly_pinned_and_non_vacuous
