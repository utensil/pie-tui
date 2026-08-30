#!/bin/sh
# Prove that focused Main/Alt tests kill one causal regression per product family.
# The repository worktree is never edited: mutations run in a git-archive sandbox.
set -eu

REPOSITORY_ROOT=$(git rev-parse --show-toplevel)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-main-alt-product.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"
RUST_TOOLCHAIN=${PIE_RUST_TOOLCHAIN:-1.98.0}
TARGET_DIRECTORY="$MUTATION_ROOT/target"

for source in \
    crates/pie-term/src/renderer.rs \
    crates/pie-app/src/screen_runtime.rs \
    crates/pie-app/src/tui_controller.rs \
    crates/pie-app/src/tui_main_screen.rs \
    crates/pie-app/src/tui_alt_screen.rs
do
    cp "$MUTATION_ROOT/$source" "$MUTATION_ROOT/$source.original"
done

apply_mutation() {
    mutation_name=$1
    for source in \
        crates/pie-term/src/renderer.rs \
        crates/pie-app/src/screen_runtime.rs \
        crates/pie-app/src/tui_controller.rs \
        crates/pie-app/src/tui_main_screen.rs \
        crates/pie-app/src/tui_alt_screen.rs
    do
        cp "$MUTATION_ROOT/$source.original" "$MUTATION_ROOT/$source"
    done
    MUTATION_NAME="$mutation_name" MUTATION_ROOT="$MUTATION_ROOT" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const mutations = {
  "flatten-cursor-operation": [{
    file: "crates/pie-term/src/renderer.rs",
    before: "                PlannedTerminalAction::ShowCursor => term.show_cursor(),",
    after: "                PlannedTerminalAction::ShowCursor => term.write(\"\\x1b[?25h\"),",
  }],
  "hide-main-hardware-cursor": [{
    file: "crates/pie-app/src/tui_main_screen.rs",
    before: "        renderer.set_show_hardware_cursor(base.show_hardware_cursor());",
    after: "        renderer.set_show_hardware_cursor(false);",
  }],
  "enter-wrong-alt-buffer": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "const ENTER_ALT_SCREEN: &str = \"\\x1b[?1049h\";",
    after: "const ENTER_ALT_SCREEN: &str = \"\\x1b[?1047h\";",
  }],
  "ignore-layout-root": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        if let Some(mut root) = self.base().and_then(|base| base.layout_root()) {",
    after: "        if false && let Some(mut root) = self.base().and_then(|base| base.layout_root()) {",
  }],
  "reverse-wheel-direction": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "            self.scroll_by(i64::from(wheel.direction) * self.wheel_scroll_lines as i64);",
    after: "            self.scroll_by(-i64::from(wheel.direction) * self.wheel_scroll_lines as i64);",
  }],
  "disable-multiclick-window": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "const DOUBLE_CLICK_INTERVAL_MS: u64 = 500;",
    after: "const DOUBLE_CLICK_INTERVAL_MS: u64 = 0;",
  }],
  "redraw-kitty-on-text-only-change": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        let images_need_redraw = screen.iter().enumerate().any(|(row, line)| {",
    after: "        let images_need_redraw = had_uploaded || screen.iter().enumerate().any(|(row, line)| {",
  }],
  "retain-iterm-capability-during-alt": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "                    images: None,",
    after: "                    images: Some(ImageProtocol::ITerm2),",
  }],
  "enable-all-motion-under-multiplexer": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "            if self.environment.multiplexer {\n                ENABLE_BUTTON_MOTION_MOUSE\n            } else {\n                ENABLE_ALL_MOTION_MOUSE\n            }",
    after: "            if self.environment.multiplexer {\n                ENABLE_ALL_MOTION_MOUSE\n            } else {\n                ENABLE_ALL_MOTION_MOUSE\n            }",
  }],
  "duplicate-terminal-drain-task": [{
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.terminal_event_task.is_some() {\n            return;\n        }",
    after: "        if false && self.terminal_event_task.is_some() {\n            return;\n        }",
  }],
  "drop-reentrant-render-follow-up": [{
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.render_in_progress {\n            self.deferred_schedule_render = true;\n            return;\n        }",
    after: "        if self.render_in_progress {\n            self.deferred_schedule_render = false;\n            return;\n        }",
  }],
  "route-flash-by-payload-only": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        let task = TuiHostTask::AltFlashTimeout { flash_id };\n        let Some(token) = base.claim_screen_task(id, task) else {\n            return false;\n        };",
    after: "        let token = self\n            .flashes\n            .borrow()\n            .entries\n            .iter()\n            .find(|entry| entry.id == flash_id)\n            .map_or(0, |entry| entry.task);",
  }],
  "flush-flash-before-recording": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        self.flashes.borrow_mut().entries.push(FlashEntry {\n            id,\n            message: message.into(),\n            task,\n        });\n        base.flush_screen_actions();",
    after: "        base.flush_screen_actions();\n        self.flashes.borrow_mut().entries.push(FlashEntry {\n            id,\n            message: message.into(),\n            task,\n        });",
  }],
  "raise-kitty-offscreen-count-cap": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "const MAX_CACHED_OFFSCREEN_KITTY_IMAGES: usize = 16;",
    after: "const MAX_CACHED_OFFSCREEN_KITTY_IMAGES: usize = 17;",
  }],
  "trust-unregistered-kitty-lines": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        for line in screen {\n            let Some(image) = render.kitty_image_registry.placement_for_line(line) else {",
    after: "        for line in screen {\n            if let Some(image_id) = line\n                .split(\"\\x1b_G\")\n                .nth(1)\n                .and_then(|tail| tail.split(';').next())\n                .and_then(|controls| {\n                    controls.split(',').find_map(|control| {\n                        let (key, value) = control.split_once('=')?;\n                        (key == \"i\").then(|| value.parse::<u32>().ok()).flatten()\n                    })\n                })\n            {\n                render.kitty_image_registry.register(KittyImageMetadata {\n                    image_id,\n                    columns: 1,\n                    rows: 1,\n                    width_px: 9,\n                    height_px: 18,\n                });\n            }\n            let Some(image) = render.kitty_image_registry.placement_for_line(line) else {",
  }],
  "read-live-terminal-columns-during-write": [{
    file: "crates/pie-app/src/screen_runtime.rs",
    before: "    fn columns_snapshot(&self) -> usize {\n        if let Ok(terminal) = self.0.terminal.try_borrow() {\n            self.0.columns.set(terminal.columns());\n        }\n        self.0.columns.get()\n    }",
    after: "    fn columns_snapshot(&self) -> usize {\n        self.0.terminal.borrow().columns()\n    }",
  }],
  "read-live-terminal-rows-during-write": [{
    file: "crates/pie-app/src/screen_runtime.rs",
    before: "    fn rows_snapshot(&self) -> usize {\n        if let Ok(terminal) = self.0.terminal.try_borrow() {\n            self.0.rows.set(terminal.rows());\n        }\n        self.0.rows.get()\n    }",
    after: "    fn rows_snapshot(&self) -> usize {\n        self.0.terminal.borrow().rows()\n    }",
  }],
  "read-live-main-renderer-during-write": [{
    file: "crates/pie-app/src/tui_main_screen.rs",
    before: "        self.state.render_snapshot.borrow().clone()",
    after: "        self.state.renderer.borrow().capture_render_state()",
  }],
  "strong-screen-backlink-cycle": [
    {
      file: "crates/pie-app/src/tui_controller.rs",
      before: "    inner: Weak<TuiShared>,",
      after: "    inner: Rc<TuiShared>,",
    },
    {
      file: "crates/pie-app/src/tui_controller.rs",
      before: "        self.inner\n            .upgrade()\n            .map(|inner| TuiBaseController { inner })",
      after: "        Some(TuiBaseController {\n            inner: Rc::clone(&self.inner),\n        })",
    },
    {
      file: "crates/pie-app/src/tui_controller.rs",
      before: "            inner: Rc::downgrade(&self.inner),",
      after: "            inner: Rc::clone(&self.inner),",
    },
  ],
  "skip-active-alt-drop-stop": [{
    file: "crates/pie-app/src/tui_alt_screen.rs",
    before: "        if self.state.render.borrow().alt_screen_active {",
    after: "        if false && self.state.render.borrow().alt_screen_active {",
  }],
};

const mutation = process.env.MUTATION_NAME;
const edits = mutations[mutation];
if (!edits) throw new Error(`unknown mutation: ${mutation}`);
for (const { file, before, after } of edits) {
  const sourcePath = join(process.env.MUTATION_ROOT, file);
  const source = readFileSync(sourcePath, "utf8");
  const occurrences = source.split(before).length - 1;
  if (occurrences !== 1) {
    throw new Error(`${mutation}: expected one marker in ${file}, found ${occurrences}`);
  }
  writeFileSync(sourcePath, source.replace(before, after));
}
NODE
}

expect_killed() {
    mutation_name=$1
    package=$2
    test_target=$3
    test_name=$4
    apply_mutation "$mutation_name"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if CARGO_TARGET_DIR="$TARGET_DIRECTORY" rustup run "$RUST_TOOLCHAIN" cargo test \
        --manifest-path "$MUTATION_ROOT/Cargo.toml" \
        -p "$package" --test "$test_target" "$test_name" -- --exact \
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

expect_killed flatten-cursor-operation pie-term golden_render \
    renderer_replays_cursor_visibility_as_a_terminal_operation
expect_killed hide-main-hardware-cursor pie-app main_alt_controller \
    main_screen_products_equal_all_three_oracle_cases
expect_killed enter-wrong-alt-buffer pie-app main_alt_controller \
    alt_screen_lifecycle_product_equals_oracle
expect_killed ignore-layout-root pie-app main_alt_controller \
    alt_layout_focus_overlay_and_main_screen_restore_equal_oracle
expect_killed reverse-wheel-direction pie-app main_alt_controller \
    alt_scroll_mouse_release_and_live_keybindings_equal_oracle
expect_killed disable-multiclick-window pie-app main_alt_controller \
    alt_selection_clipboard_granularity_and_focus_out_equal_oracle
expect_killed redraw-kitty-on-text-only-change pie-app main_alt_controller \
    alt_kitty_transmission_placement_and_teardown_equal_oracle
expect_killed retain-iterm-capability-during-alt pie-app main_alt_controller \
    alt_iterm_capability_suspension_and_unpreserved_stop_equal_oracle
expect_killed enable-all-motion-under-multiplexer pie-app main_alt_controller \
    alt_multiplexer_button_motion_lifecycle_equals_oracle
expect_killed duplicate-terminal-drain-task pie-app main_alt_controller \
    terminal_callback_drain_routes_each_input_once
expect_killed drop-reentrant-render-follow-up pie-app main_alt_controller \
    reentrant_component_render_defers_without_losing_follow_up
expect_killed route-flash-by-payload-only pie-app main_alt_controller \
    alt_flash_tasks_require_matching_route_identity
expect_killed flush-flash-before-recording pie-app main_alt_controller \
    second_flash_reentrant_stop_is_borrow_free_and_tears_down_exactly
expect_killed raise-kitty-offscreen-count-cap pie-app main_alt_controller \
    alt_kitty_offscreen_cache_eviction_and_revisit_equal_oracle
expect_killed trust-unregistered-kitty-lines pie-app main_alt_controller \
    alt_raw_unregistered_kitty_lines_are_not_owned_equal_oracle
expect_killed read-live-terminal-columns-during-write pie-app main_alt_controller \
    terminal_write_reentrant_alt_geometry_reads_use_last_safe_snapshot
expect_killed read-live-terminal-rows-during-write pie-app main_alt_controller \
    terminal_write_reentrant_alt_geometry_reads_use_last_safe_snapshot
expect_killed read-live-main-renderer-during-write pie-app main_alt_controller \
    terminal_write_reentrant_main_capture_reads_last_committed_snapshot
expect_killed strong-screen-backlink-cycle pie-app main_alt_controller \
    dropping_active_screens_breaks_weak_cycles_and_cancels_tasks
expect_killed skip-active-alt-drop-stop pie-app main_alt_controller \
    dropping_active_screens_breaks_weak_cycles_and_cancels_tasks

printf 'Main/Alt product mutations: OK (20/20 killed)\n'
