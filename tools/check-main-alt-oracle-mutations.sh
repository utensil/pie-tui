#!/bin/sh
# Prove Main/Alt oracle provenance is checked before importing copied sources.
# The repository and pinned reference are never edited.
set -eu

REPOSITORY_ROOT=$(git rev-parse --show-toplevel)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-main-alt-oracle.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

REFERENCE_DIST=${PI_TUI_DIST:?PI_TUI_DIST must point to the exact pi-tui 0.84.1 dist}
REFERENCE_PACKAGE=$(CDPATH= cd "$REFERENCE_DIST/.." && pwd)
REFERENCE_NODE_MODULES=$(CDPATH= cd "$REFERENCE_PACKAGE/../.." && pwd)
MUTATED_NODE_MODULES="$MUTATION_ROOT/node_modules"
mkdir -p "$MUTATED_NODE_MODULES"
cp -R "$REFERENCE_NODE_MODULES/." "$MUTATED_NODE_MODULES"

expect_copied_source_killed() {
    mutation_name=$1
    target_path=$2
    copied_path=$3
    expected_digest=$4
    target="$MUTATED_NODE_MODULES/$target_path"
    original="$REFERENCE_NODE_MODULES/$target_path"
    copied="$REFERENCE_NODE_MODULES/$copied_path"
    log_path="$MUTATION_ROOT/$mutation_name.log"

    cp "$original" "$target"
    cp "$copied" "$target"
    if PI_TUI_DIST="$MUTATED_NODE_MODULES/@earendil-works/pi-tui/dist" node \
        "$REPOSITORY_ROOT/tools/golden/gen-golden-main-alt-controller.mjs" --check \
        >"$log_path" 2>&1
    then
        printf 'oracle mutation survived: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    if ! grep -Fq "$expected_digest digest mismatch" "$log_path"; then
        printf 'oracle mutation missed pre-import digest gate: %s\n' "$mutation_name" >&2
        cat "$log_path" >&2
        exit 1
    fi
    cp "$original" "$target"
    printf 'oracle mutation killed: %s\n' "$mutation_name"
}

expect_copied_source_killed copied-main-js \
    @earendil-works/pi-tui/dist/tui-main-screen.js \
    @earendil-works/pi-tui/dist/tui-alt-screen.js \
    tuiMainScreenJs
expect_copied_source_killed copied-main-dts \
    @earendil-works/pi-tui/dist/tui-main-screen.d.ts \
    @earendil-works/pi-tui/dist/tui-alt-screen.d.ts \
    tuiMainScreenDts
expect_copied_source_killed copied-alt-js \
    @earendil-works/pi-tui/dist/tui-alt-screen.js \
    @earendil-works/pi-tui/dist/tui-main-screen.js \
    tuiAltScreenJs
expect_copied_source_killed copied-alt-dts \
    @earendil-works/pi-tui/dist/tui-alt-screen.d.ts \
    @earendil-works/pi-tui/dist/tui-main-screen.d.ts \
    tuiAltScreenDts
expect_copied_source_killed copied-tui \
    @earendil-works/pi-tui/dist/tui.js \
    @earendil-works/pi-tui/dist/terminal-image.js \
    tuiJs
expect_copied_source_killed copied-terminal-dts \
    @earendil-works/pi-tui/dist/terminal.d.ts \
    @earendil-works/pi-tui/dist/tui.d.ts \
    terminalDts
expect_copied_source_killed copied-flash \
    @earendil-works/pi-tui/dist/components/alt-screen-flash.js \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    altScreenFlashJs
expect_copied_source_killed copied-scroll-view \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    @earendil-works/pi-tui/dist/components/stack.js \
    scrollViewJs
expect_copied_source_killed copied-stack \
    @earendil-works/pi-tui/dist/components/stack.js \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    stackJs
expect_copied_source_killed copied-keybindings \
    @earendil-works/pi-tui/dist/keybindings.js \
    @earendil-works/pi-tui/dist/keys.js \
    keybindingsJs
expect_copied_source_killed copied-keys \
    @earendil-works/pi-tui/dist/keys.js \
    @earendil-works/pi-tui/dist/keybindings.js \
    keysJs
expect_copied_source_killed copied-layout \
    @earendil-works/pi-tui/dist/layout.js \
    @earendil-works/pi-tui/dist/layout-node.js \
    layoutJs
expect_copied_source_killed copied-layout-node \
    @earendil-works/pi-tui/dist/layout-node.js \
    @earendil-works/pi-tui/dist/layout.js \
    layoutNodeJs
expect_copied_source_killed copied-terminal-colors \
    @earendil-works/pi-tui/dist/terminal-colors.js \
    @earendil-works/pi-tui/dist/keys.js \
    terminalColorsJs
expect_copied_source_killed copied-terminal-image \
    @earendil-works/pi-tui/dist/terminal-image.js \
    @earendil-works/pi-tui/dist/terminal-colors.js \
    terminalImageJs
expect_copied_source_killed copied-utils \
    @earendil-works/pi-tui/dist/utils.js \
    @earendil-works/pi-tui/dist/keys.js \
    utilsJs
expect_copied_source_killed copied-width-lookup-data \
    get-east-asian-width/lookup-data.js \
    get-east-asian-width/utilities.js \
    widthLookupDataJs
