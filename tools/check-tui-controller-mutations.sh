#!/bin/sh
# Prove that the focused TuiBase controller tests kill each lifecycle regression.
# The repository worktree is never edited: mutations run in a git-archive sandbox.
set -eu

REPOSITORY_ROOT=$(git rev-parse --show-toplevel)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-controller-mutations.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"
CONTROLLER_SOURCE="$MUTATION_ROOT/crates/pie-app/src/tui_controller.rs"
CONTAINER_SOURCE="$MUTATION_ROOT/crates/pie-components/src/container.rs"
TUI_SOURCE="$MUTATION_ROOT/crates/pie-components/src/tui.rs"
cp "$CONTROLLER_SOURCE" "$MUTATION_ROOT/tui_controller.rs.original"
cp "$CONTAINER_SOURCE" "$MUTATION_ROOT/container.rs.original"
cp "$TUI_SOURCE" "$MUTATION_ROOT/tui.rs.original"

RUST_TOOLCHAIN=${PIE_RUST_TOOLCHAIN:-1.98.0}
TARGET_DIRECTORY="$MUTATION_ROOT/target"
REFERENCE_DIST=${PI_TUI_DIST:?PI_TUI_DIST must point to the exact pi-tui 0.84.1 dist}
REFERENCE_PACKAGE=$(CDPATH= cd "$REFERENCE_DIST/.." && pwd)
REFERENCE_NODE_MODULES=$(CDPATH= cd "$REFERENCE_PACKAGE/../.." && pwd)

apply_mutation() {
    mutation_name=$1
    cp "$MUTATION_ROOT/tui_controller.rs.original" "$CONTROLLER_SOURCE"
    cp "$MUTATION_ROOT/container.rs.original" "$CONTAINER_SOURCE"
    cp "$MUTATION_ROOT/tui.rs.original" "$TUI_SOURCE"
    MUTATION_NAME="$mutation_name" MUTATION_ROOT="$MUTATION_ROOT" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const mutations = {
  "reverse-listener-order": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                state\n                    .input_listeners\n                    .iter()\n                    .filter(|entry| entry.order > last_order)\n                    .min_by_key(|entry| entry.order)\n                    .cloned()",
    after: "                state\n                    .input_listeners\n                    .iter()\n                    .filter(|entry| entry.order > last_order)\n                    .max_by_key(|entry| entry.order)\n                    .cloned()",
  },
  "ignore-listener-transform": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                    data = transformed;",
    after: "                    drop(transformed);",
  },
  "ignore-listener-consume": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                if result.consume {\n                    return;\n                }",
    after: "                if false && result.consume {\n                    return;\n                }",
  },
  "forward-key-release": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if is_key_release(data) && !focus.wants_key_release() {",
    after: "        if false && is_key_release(data) && !focus.wants_key_release() {",
  },
  "cell-response-before-listeners": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.consume_color_scheme_report(original) {\n            return;\n        }\n\n        let mut data = original.to_owned();",
    after: "        if self.consume_color_scheme_report(original) {\n            return;\n        }\n        if self.consume_cell_size_response(original) {\n            return;\n        }\n\n        let mut data = original.to_owned();",
  },
  "skip-cell-invalidation": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                        controller.invalidate();\n                        controller.request_render(false);",
    after: "                        controller.request_render(false);",
  },
  "drop-stale-osc-slot": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                        deferred_background = query.callback.take();",
    after: "                        deferred_background = query.callback.take();\n                        state.pending_background_replies = state.pending_background_replies.saturating_sub(1);",
  },
  "skip-scheme-listeners": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                Dispatch::Persistent(listener) => listener.invoke(scheme),",
    after: "                Dispatch::Persistent(_) => {},",
  },
  "scheme-timeout-is-light": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if let Some(callback) = deferred_scheme {\n            callback(None);\n        }",
    after: "        if let Some(callback) = deferred_scheme {\n            callback(Some(pie_core::terminal_colors::TerminalColorScheme::Light));\n        }",
  },
  "duplicate-notification-write": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            if state.color_scheme_notifications_enabled == enabled {\n                return;\n            }",
    after: "            if false && state.color_scheme_notifications_enabled == enabled {\n                return;\n            }",
  },
  "drift-render-interval": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "const MIN_RENDER_INTERVAL_MS: u64 = 16;",
    after: "const MIN_RENDER_INTERVAL_MS: u64 = 17;",
  },
  "disable-render-coalescing": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.render_requested {\n            return;\n        }\n        self.render_requested = true;",
    after: "        if false && self.render_requested {\n            return;\n        }\n        self.render_requested = true;",
  },
  "drop-force-reset": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            self.actions.push_back(ControllerAction::ResetRenderState);\n            self.request_immediate_render();\n            return;",
    after: "            self.request_immediate_render();\n            return;",
  },
  "throttle-accepted-input": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        self.inner.borrow_mut().request_immediate_render();",
    after: "        self.inner.borrow_mut().request_render(false);",
  },
  "focus-noncapturing-overlay": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let should_focus = !non_capturing && self.overlay_is_visible(id);",
    after: "        let should_focus = self.overlay_is_visible(id);",
  },
  "eager-visibility-snapshot": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "    pub fn has_overlay(&self) -> bool {\n        let length = self.inner.borrow().overlays.len();\n        for index in 0..length {\n            let entry = self.inner.borrow().overlays.get(index).cloned();",
    after: "    pub fn has_overlay(&self) -> bool {\n        let entries = self.inner.borrow().overlays.clone();\n        for entry in entries.into_iter().map(Some) {",
  },
  "evaluate-noncapturing-visibility": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            if entry.options.non_capturing || !self.overlay_entry_is_visible(&entry) {",
    after: "            if !self.overlay_entry_is_visible(&entry) || entry.options.non_capturing {",
  },
  "evaluate-show-visibility-before-capture": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let should_focus = !non_capturing && self.overlay_is_visible(id);",
    after: "        let should_focus = self.overlay_is_visible(id) && !non_capturing;",
  },
  "reverse-overlay-z-order": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        entries.sort_by_key(|entry| entry.focus_order);",
    after: "        entries.sort_by_key(|entry| std::cmp::Reverse(entry.focus_order));",
  },
  "default-overlay-width-79": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        .unwrap_or_else(|| i64::try_from(80.min(available_width)).unwrap_or(i64::MAX));",
    after: "        .unwrap_or_else(|| i64::try_from(79.min(available_width)).unwrap_or(i64::MAX));",
  },
  "ignore-overlay-left-margin": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "    let margin_left = usize::try_from(margin.left.max(0)).unwrap_or(usize::MAX);",
    after: "    let margin_left = 0;",
  },
  "ignore-overlay-max-height": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "    let max_height = options.max_height.map(|value| {\n        usize::try_from(resolved_size(value, term_height).max(1))\n            .unwrap_or(usize::MAX)\n            .min(available_height)\n    });",
    after: "    let max_height = None;",
  },
  "stop-retains-render-timer": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            let mut state = self.inner.borrow_mut();\n            state.stopped = true;\n            state.cancel_render_timer();\n            if state.color_scheme_notifications_enabled {",
    after: "            let mut state = self.inner.borrow_mut();\n            state.stopped = true;\n            if state.color_scheme_notifications_enabled {",
  },
  "drop-retains-owned-tasks": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "    fn plan_teardown(&mut self, notify_host: bool) -> Option<ComponentRef> {\n        self.cancel_all_tasks();\n        self.stopped = true;",
    after: "    fn plan_teardown(&mut self, notify_host: bool) -> Option<ComponentRef> {\n        self.stopped = true;",
  },
  "drop-leaves-component-focused": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let focused = self.focused_component.take();",
    after: "        self.focused_component = None;\n        let focused: Option<ComponentRef> = None;",
  },
  "retain-unsubscribed-input-listener": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            SubscriptionKind::Input(identity) => state\n                .input_listeners\n                .retain(|listener| listener.listener.identity() != identity),",
    after: "            SubscriptionKind::Input(_) => state\n                .input_listeners\n                .retain(|_| true),",
  },
  "retain-unsubscribed-scheme-listener": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            SubscriptionKind::Scheme(identity) => state.scheme_listeners.retain(|listener| {\n                !matches!(listener, SchemeListenerEntry::Persistent { listener: current, .. } if current.identity() == identity)\n            }),",
    after: "            SubscriptionKind::Scheme(_) => state.scheme_listeners.retain(|_| true),",
  },
  "reverse-scheme-query-insertion-order": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                    .scheme_listeners\n                    .iter()\n                    .enumerate()\n                    .filter(|(_, entry)| entry.order() > last_order)\n                    .min_by_key(|(_, entry)| entry.order())",
    after: "                    .scheme_listeners\n                    .iter()\n                    .enumerate()\n                    .filter(|(_, entry)| entry.order() > last_order)\n                    .max_by_key(|(_, entry)| entry.order())",
  },
  "restore-stale-debug-callback": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            if let Some(debug) = callback {\n                debug();\n            } else {",
    after: "            if let Some(debug) = callback {\n                debug();\n                self.inner.borrow_mut().debug_callback = Some(debug);\n            } else {",
  },
  "guard-recursive-input-listener": {
    file: "crates/pie-components/src/tui.rs",
    before: "    pub fn invoke(&self, data: &str) -> Option<TuiInputListenerResult> {\n        (self.callback)(data)\n    }",
    after: "    pub fn invoke(&self, data: &str) -> Option<TuiInputListenerResult> {\n        std::thread_local! {\n            static INPUT_INVOKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };\n        }\n        INPUT_INVOKING.with(|invoking| {\n            assert!(!invoking.replace(true), \"recursive input callback\");\n        });\n        let result = (self.callback)(data);\n        INPUT_INVOKING.with(|invoking| invoking.set(false));\n        result\n    }",
  },
  "guard-recursive-scheme-listener": {
    file: "crates/pie-components/src/tui.rs",
    before: "    pub fn invoke(&self, scheme: TerminalColorScheme) {\n        (self.callback)(scheme);\n    }",
    after: "    pub fn invoke(&self, scheme: TerminalColorScheme) {\n        std::thread_local! {\n            static SCHEME_INVOKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };\n        }\n        SCHEME_INVOKING.with(|invoking| {\n            assert!(!invoking.replace(true), \"recursive scheme callback\");\n        });\n        (self.callback)(scheme);\n        SCHEME_INVOKING.with(|invoking| invoking.set(false));\n    }",
  },
  "guard-recursive-debug-callback": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            if let Some(debug) = callback {\n                debug();\n            } else {",
    after: "            if let Some(debug) = callback {\n                std::thread_local! {\n                    static DEBUG_INVOKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };\n                }\n                DEBUG_INVOKING.with(|invoking| {\n                    assert!(!invoking.replace(true), \"recursive debug callback\");\n                });\n                debug();\n                DEBUG_INVOKING.with(|invoking| invoking.set(false));\n            } else {",
  },
  "repeat-identical-layout-root": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            if unchanged {\n                return;\n            }\n            state.layout_root = component;",
    after: "            if false && unchanged {\n                return;\n            }\n            state.layout_root = component;",
  },
  "retain-layout-cache-epoch": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            state.layout_cache_epoch = state.layout_cache_epoch.wrapping_add(1);",
    after: "            state.layout_cache_epoch = state.layout_cache_epoch;",
  },
  "invalidate-children-beside-layout-root": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        } else {\n            // A JavaScript Array iterator re-reads the element at its current",
    after: "        }\n        {\n            // A JavaScript Array iterator re-reads the element at its current",
  },
  "snapshot-root-invalidation": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            let root_components = Rc::clone(&self.inner.borrow().root_components);\n            let mut index = 0;\n            loop {\n                let component = root_components\n                    .borrow()\n                    .get(index)\n                    .map(|(_, component)| component.clone());",
    after: "            let root_components = Rc::clone(&self.inner.borrow().root_components);\n            let components = root_components.borrow().clone();\n            let mut index = 0;\n            loop {\n                let component = components\n                    .get(index)\n                    .map(|(_, component)| component.clone());",
  },
  "freeze-root-invalidation-length": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "            let root_components = Rc::clone(&self.inner.borrow().root_components);\n            let mut index = 0;\n            loop {\n                let component = root_components\n                    .borrow()\n                    .get(index)",
    after: "            let root_components = Rc::clone(&self.inner.borrow().root_components);\n            let length = root_components.borrow().len();\n            let mut index = 0;\n            loop {\n                if index == length {\n                    break;\n                }\n                let component = root_components\n                    .borrow()\n                    .get(index)",
  },
  "clear-root-array-in-place": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        state.roots.clear();\n        state.root_components = Rc::new(RefCell::new(Vec::new()));",
    after: "        state.roots.clear();\n        state.root_components.borrow_mut().clear();",
  },
  "snapshot-overlay-invalidation": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let mut index = 0;\n        loop {\n            let component = {\n                self.inner\n                    .borrow()\n                    .overlays\n                    .get(index)\n                    .map(|entry| entry.component.clone())\n            };",
    after: "        let overlays = self.inner.borrow().overlays.clone();\n        let mut index = 0;\n        loop {\n            let component = overlays\n                .get(index)\n                .map(|entry| entry.component.clone());",
  },
  "freeze-overlay-invalidation-length": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let mut index = 0;\n        loop {\n            let component = {\n                self.inner\n                    .borrow()\n                    .overlays\n                    .get(index)",
    after: "        let length = self.inner.borrow().overlays.len();\n        let mut index = 0;\n        loop {\n            if index == length {\n                break;\n            }\n            let component = {\n                self.inner\n                    .borrow()\n                    .overlays\n                    .get(index)",
  },
  "drop-reentrant-render-follow-up": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.render_in_progress {\n            self.deferred_schedule_render = true;\n            return;\n        }",
    after: "        if self.render_in_progress {\n            self.deferred_schedule_render = false;\n            return;\n        }",
  },
  "focus-callback-under-state-borrow": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if let Some(next) = &plan.next {\n            next.set_focused(true);\n        }",
    after: "        {\n            let _state = self.inner.borrow_mut();\n            if let Some(next) = &plan.next {\n                next.set_focused(true);\n            }\n        }",
  },
  "visibility-callback-under-state-borrow": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        let height = self.inner.terminal_rows();\n        predicate(width, height)",
    after: "        let height = self.inner.terminal_rows();\n        let _state = self.inner.borrow();\n        predicate(width, height)",
  },
  "invalidate-callback-under-state-borrow": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                component.invalidate();\n                index += 1;\n            }\n        }\n\n        let mut index = 0;",
    after: "                let _state = self.inner.borrow_mut();\n                component.invalidate();\n                index += 1;\n            }\n        }\n\n        let mut index = 0;",
  },
  "disable-terminal-event-wake": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "    fn request_terminal_event_drain(&mut self) {\n        if self.terminal_event_task.is_some() {",
    after: "    fn request_terminal_event_drain(&mut self) {\n        if true || self.terminal_event_task.is_some() {",
  },
  "skip-padding-empty-visible-overlay-set": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        entries.sort_by_key(|entry| entry.focus_order);\n        let mut rendered = Vec::new();",
    after: "        if entries.is_empty() {\n            return lines;\n        }\n        entries.sort_by_key(|entry| entry.focus_order);\n        let mut rendered = Vec::new();",
  },
  "gate-notification-cleanup-on-started": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.color_scheme_notifications_active {\n            self.actions.push_back(ControllerAction::TerminalWrite(",
    after: "        if self.started && self.color_scheme_notifications_active {\n            self.actions.push_back(ControllerAction::TerminalWrite(",
  },
  "gate-cursor-cleanup-on-started": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.cursor_hidden {\n            self.actions.push_back(ControllerAction::TerminalShowCursor);",
    after: "        if self.started && self.cursor_hidden {\n            self.actions.push_back(ControllerAction::TerminalShowCursor);",
  },
  "stop-terminal-before-start": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if self.started {\n            self.actions.push_back(ControllerAction::TerminalStop);\n        }",
    after: "        {\n            self.actions.push_back(ControllerAction::TerminalStop);\n        }",
  },
  "disable-host-action-reentry-guard": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "        if shared.driving_actions.replace(true) {\n            return;\n        }",
    after: "        shared.driving_actions.set(true);",
  },
  "skip-final-drop-reconciliation": {
    file: "crates/pie-app/src/tui_controller.rs",
    before: "                        shared\n                            .borrow_mut()\n                            .actions\n                            .push_back(ControllerAction::FinalizeTeardown);",
    after: "",
  },
  "erase-nested-component-ownership": {
    file: "crates/pie-components/src/container.rs",
    before: "        self.ptr_eq(component) || self.inner.contains_component(component.identity)",
    after: "        self.ptr_eq(component)",
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
    package=$2
    test_binary=$3
    test_name=$4
    apply_mutation "$mutation_name"
    if [ "${PIE_MUTATION_VALIDATE_ONLY:-0}" = 1 ]; then
        printf 'mutation marker valid: %s\n' "$mutation_name"
        return
    fi
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if CARGO_TARGET_DIR="$TARGET_DIRECTORY" rustup run "$RUST_TOOLCHAIN" cargo test \
        --manifest-path "$MUTATION_ROOT/Cargo.toml" \
        -p "$package" --test "$test_binary" "$test_name" -- --exact \
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
    if [ "${PIE_MUTATION_VALIDATE_ONLY:-0}" = 1 ]; then
        return
    fi
    case_root="$MUTATION_ROOT/oracle-$mutation_name"
    mkdir -p "$case_root/node_modules"
    cp -R "$REFERENCE_NODE_MODULES/." "$case_root/node_modules"
    cp "$case_root/node_modules/$copied_path" "$case_root/node_modules/$target_path"
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if PI_TUI_DIST="$case_root/node_modules/@earendil-works/pi-tui/dist" node \
        "$MUTATION_ROOT/tools/golden/gen-golden-tui-controller.mjs" --check \
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

expect_oracle_manifest_killed() {
    mutation_name=$1
    target_path=$2
    expected_digest=$3
    if [ "${PIE_MUTATION_VALIDATE_ONLY:-0}" = 1 ]; then
        return
    fi
    case_root="$MUTATION_ROOT/oracle-$mutation_name"
    mkdir -p "$case_root/node_modules"
    cp -R "$REFERENCE_NODE_MODULES/." "$case_root/node_modules"
    manifest="$case_root/node_modules/$target_path"
    MANIFEST="$manifest" node --input-type=module <<'NODE'
import { appendFileSync } from "node:fs";

appendFileSync(process.env.MANIFEST, "\n");
NODE
    log_path="$MUTATION_ROOT/$mutation_name.log"
    if PI_TUI_DIST="$case_root/node_modules/@earendil-works/pi-tui/dist" node \
        "$MUTATION_ROOT/tools/golden/gen-golden-tui-controller.mjs" --check \
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

expect_oracle_closure_omission_killed() {
    if [ "${PIE_MUTATION_VALIDATE_ONLY:-0}" = 1 ]; then
        return
    fi
    case_root="$MUTATION_ROOT/oracle-closure-omission"
    mkdir -p "$case_root/node_modules"
    cp -R "$REFERENCE_NODE_MODULES/." "$case_root/node_modules"
    generator="$MUTATION_ROOT/tools/golden/gen-golden-tui-controller.mjs"
    GENERATOR="$generator" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";

const path = process.env.GENERATOR;
const source = readFileSync(path, "utf8");
const marker = '  "components/stack.js",\n';
if (source.split(marker).length - 1 !== 1) {
  throw new Error("closure omission marker drifted");
}
writeFileSync(path, source.replace(marker, ""));
NODE
    log_path="$MUTATION_ROOT/closure-omission.log"
    if PI_TUI_DIST="$case_root/node_modules/@earendil-works/pi-tui/dist" node \
        "$generator" --check >"$log_path" 2>&1
    then
        printf 'oracle mutation survived: closure-omission\n' >&2
        cat "$log_path" >&2
        exit 1
    fi
    if ! grep -Fq 'runtime import closure mismatch' "$log_path"; then
        printf 'oracle mutation missed recursive closure gate: closure-omission\n' >&2
        cat "$log_path" >&2
        exit 1
    fi
    printf 'oracle mutation killed: closure-omission\n'
}

expect_oracle_copy_killed copied-tui \
    @earendil-works/pi-tui/dist/tui.js \
    @earendil-works/pi-tui/dist/terminal-image.js \
    tuiJs
expect_oracle_manifest_killed copied-package-json \
    @earendil-works/pi-tui/package.json \
    packageJson
expect_oracle_copy_killed copied-tui-dts \
    @earendil-works/pi-tui/dist/tui.d.ts \
    @earendil-works/pi-tui/dist/tui-alt-screen.d.ts \
    tuiDts
expect_oracle_copy_killed copied-tui-alt-screen \
    @earendil-works/pi-tui/dist/tui-alt-screen.js \
    @earendil-works/pi-tui/dist/tui.js \
    tuiAltScreenJs
expect_oracle_copy_killed copied-tui-alt-screen-dts \
    @earendil-works/pi-tui/dist/tui-alt-screen.d.ts \
    @earendil-works/pi-tui/dist/tui.d.ts \
    tuiAltScreenDts
expect_oracle_copy_killed copied-terminal-dts \
    @earendil-works/pi-tui/dist/terminal.d.ts \
    @earendil-works/pi-tui/dist/tui.d.ts \
    terminalDts
expect_oracle_copy_killed copied-alt-screen-flash \
    @earendil-works/pi-tui/dist/components/alt-screen-flash.js \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    altScreenFlashJs
expect_oracle_copy_killed copied-alt-screen-flash-dts \
    @earendil-works/pi-tui/dist/components/alt-screen-flash.d.ts \
    @earendil-works/pi-tui/dist/components/scroll-view.d.ts \
    altScreenFlashDts
expect_oracle_copy_killed copied-scroll-view \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    @earendil-works/pi-tui/dist/components/stack.js \
    scrollViewJs
expect_oracle_copy_killed copied-scroll-view-dts \
    @earendil-works/pi-tui/dist/components/scroll-view.d.ts \
    @earendil-works/pi-tui/dist/components/stack.d.ts \
    scrollViewDts
expect_oracle_copy_killed copied-stack \
    @earendil-works/pi-tui/dist/components/stack.js \
    @earendil-works/pi-tui/dist/components/scroll-view.js \
    stackJs
expect_oracle_copy_killed copied-stack-dts \
    @earendil-works/pi-tui/dist/components/stack.d.ts \
    @earendil-works/pi-tui/dist/components/scroll-view.d.ts \
    stackDts
expect_oracle_copy_killed copied-keybindings \
    @earendil-works/pi-tui/dist/keybindings.js \
    @earendil-works/pi-tui/dist/keys.js \
    keybindingsJs
expect_oracle_copy_killed copied-keybindings-dts \
    @earendil-works/pi-tui/dist/keybindings.d.ts \
    @earendil-works/pi-tui/dist/keys.d.ts \
    keybindingsDts
expect_oracle_copy_killed copied-keys \
    @earendil-works/pi-tui/dist/keys.js \
    @earendil-works/pi-tui/dist/terminal-colors.js \
    keysJs
expect_oracle_copy_killed copied-keys-dts \
    @earendil-works/pi-tui/dist/keys.d.ts \
    @earendil-works/pi-tui/dist/keybindings.d.ts \
    keysDts
expect_oracle_copy_killed copied-layout \
    @earendil-works/pi-tui/dist/layout.js \
    @earendil-works/pi-tui/dist/layout-node.js \
    layoutJs
expect_oracle_copy_killed copied-layout-dts \
    @earendil-works/pi-tui/dist/layout.d.ts \
    @earendil-works/pi-tui/dist/layout-node.d.ts \
    layoutDts
expect_oracle_copy_killed copied-layout-node \
    @earendil-works/pi-tui/dist/layout-node.js \
    @earendil-works/pi-tui/dist/layout.js \
    layoutNodeJs
expect_oracle_copy_killed copied-layout-node-dts \
    @earendil-works/pi-tui/dist/layout-node.d.ts \
    @earendil-works/pi-tui/dist/layout.d.ts \
    layoutNodeDts
expect_oracle_copy_killed copied-terminal-colors \
    @earendil-works/pi-tui/dist/terminal-colors.js \
    @earendil-works/pi-tui/dist/keys.js \
    terminalColorsJs
expect_oracle_copy_killed copied-terminal-colors-dts \
    @earendil-works/pi-tui/dist/terminal-colors.d.ts \
    @earendil-works/pi-tui/dist/keys.d.ts \
    terminalColorsDts
expect_oracle_copy_killed copied-terminal-image \
    @earendil-works/pi-tui/dist/terminal-image.js \
    @earendil-works/pi-tui/dist/terminal-colors.js \
    terminalImageJs
expect_oracle_copy_killed copied-terminal-image-dts \
    @earendil-works/pi-tui/dist/terminal-image.d.ts \
    @earendil-works/pi-tui/dist/terminal-colors.d.ts \
    terminalImageDts
expect_oracle_copy_killed copied-utils \
    @earendil-works/pi-tui/dist/utils.js \
    @earendil-works/pi-tui/dist/keys.js \
    utilsJs
expect_oracle_copy_killed copied-utils-dts \
    @earendil-works/pi-tui/dist/utils.d.ts \
    @earendil-works/pi-tui/dist/keys.d.ts \
    utilsDts
expect_oracle_copy_killed copied-lookup-data \
    get-east-asian-width/lookup-data.js \
    get-east-asian-width/lookup.js \
    eastAsianWidthLookupDataJs
expect_oracle_manifest_killed copied-width-package-json \
    get-east-asian-width/package.json \
    eastAsianWidthPackageJson
expect_oracle_copy_killed copied-width-index \
    get-east-asian-width/index.js \
    get-east-asian-width/lookup.js \
    eastAsianWidthIndexJs
expect_oracle_copy_killed copied-width-index-dts \
    get-east-asian-width/index.d.ts \
    @earendil-works/pi-tui/dist/keys.d.ts \
    eastAsianWidthIndexDts
expect_oracle_copy_killed copied-width-lookup \
    get-east-asian-width/lookup.js \
    get-east-asian-width/utilities.js \
    eastAsianWidthLookupJs
expect_oracle_copy_killed copied-width-utilities \
    get-east-asian-width/utilities.js \
    get-east-asian-width/lookup.js \
    eastAsianWidthUtilitiesJs
expect_oracle_closure_omission_killed

expect_killed reverse-listener-order pie-app tui_controller listener_transform_consume_release_and_debug_priority_match
expect_killed ignore-listener-transform pie-app tui_controller listener_transform_consume_release_and_debug_priority_match
expect_killed ignore-listener-consume pie-app tui_controller listener_transform_consume_release_and_debug_priority_match
expect_killed forward-key-release pie-app tui_controller listener_transform_consume_release_and_debug_priority_match
expect_killed cell-response-before-listeners pie-app tui_controller terminal_event_queue_cell_size_and_invalidation_are_ordered
expect_killed skip-cell-invalidation pie-app tui_controller terminal_event_queue_cell_size_and_invalidation_are_ordered
expect_killed drop-stale-osc-slot pie-app tui_controller osc11_timeout_keeps_the_stale_fifo_reply_slot
expect_killed skip-scheme-listeners pie-app tui_controller color_scheme_query_listener_order_and_notification_toggles_match
expect_killed scheme-timeout-is-light pie-app tui_controller color_scheme_query_listener_order_and_notification_toggles_match
expect_killed duplicate-notification-write pie-app tui_controller color_scheme_query_listener_order_and_notification_toggles_match
expect_killed drift-render-interval pie-app tui_controller start_render_coalescing_and_stop_follow_the_fake_clock
expect_killed disable-render-coalescing pie-app tui_controller start_render_coalescing_and_stop_follow_the_fake_clock
expect_killed drop-force-reset pie-app tui_controller start_render_coalescing_and_stop_follow_the_fake_clock
expect_killed throttle-accepted-input pie-app tui_controller listener_transform_consume_release_and_debug_priority_match
expect_killed focus-noncapturing-overlay pie-app tui_controller overlay_focus_stack_handles_own_visibility_and_restore
expect_killed eager-visibility-snapshot pie-app tui_controller visibility_iteration_observes_live_reentrant_overlay_removal
expect_killed evaluate-noncapturing-visibility pie-app tui_controller topmost_skips_noncapturing_before_evaluating_visibility
expect_killed evaluate-show-visibility-before-capture pie-app tui_controller showing_a_noncapturing_overlay_never_evaluates_visibility
expect_killed reverse-overlay-z-order pie-app tui_controller overlay_layout_and_composition_match_every_oracle_row
expect_killed default-overlay-width-79 pie-app tui_controller overlay_layout_and_composition_match_every_oracle_row
expect_killed ignore-overlay-left-margin pie-app tui_controller overlay_layout_and_composition_match_every_oracle_row
expect_killed ignore-overlay-max-height pie-app tui_controller overlay_layout_and_composition_match_every_oracle_row
expect_killed stop-retains-render-timer pie-app tui_controller start_render_coalescing_and_stop_follow_the_fake_clock
expect_killed drop-retains-owned-tasks pie-app tui_controller stop_repeats_like_the_oracle_but_drop_cancels_all_owned_work
expect_killed drop-leaves-component-focused pie-app tui_controller stop_repeats_like_the_oracle_but_drop_cancels_all_owned_work
expect_killed retain-unsubscribed-input-listener pie-app tui_controller input_listener_dispatch_uses_live_set_mutation_order
expect_killed retain-unsubscribed-scheme-listener pie-app tui_controller scheme_dispatch_is_live_and_queries_share_insertion_order
expect_killed reverse-scheme-query-insertion-order pie-app tui_controller scheme_dispatch_is_live_and_queries_share_insertion_order
expect_killed restore-stale-debug-callback pie-app tui_controller debug_callback_replacement_survives_current_dispatch
expect_killed guard-recursive-input-listener pie-app tui_controller input_listener_can_recursively_dispatch_the_same_listener
expect_killed guard-recursive-scheme-listener pie-app tui_controller scheme_listener_can_recursively_dispatch_the_same_listener
expect_killed guard-recursive-debug-callback pie-app tui_controller debug_callback_can_recursively_dispatch_itself
expect_killed repeat-identical-layout-root pie-app tui_controller layout_root_change_is_identity_aware_and_exclusive
expect_killed retain-layout-cache-epoch pie-app tui_controller layout_root_change_is_identity_aware_and_exclusive
expect_killed invalidate-children-beside-layout-root pie-app tui_controller layout_root_change_is_identity_aware_and_exclusive
expect_killed snapshot-root-invalidation pie-app tui_controller invalidation_root_iteration_observes_live_deletion
expect_killed freeze-root-invalidation-length pie-app tui_controller invalidation_root_iteration_observes_live_insertion
expect_killed clear-root-array-in-place pie-app tui_controller invalidation_root_clear_rebinds_the_active_array_identity
expect_killed snapshot-overlay-invalidation pie-app tui_controller invalidation_overlay_iteration_observes_live_deletion
expect_killed freeze-overlay-invalidation-length pie-app tui_controller invalidation_overlay_iteration_observes_live_insertion
expect_killed drop-reentrant-render-follow-up pie-app tui_controller host_render_can_request_a_follow_up_frame_reentrantly
expect_killed focus-callback-under-state-borrow pie-app tui_controller focus_setter_can_request_render_reentrantly
expect_killed visibility-callback-under-state-borrow pie-app tui_controller overlay_visibility_predicate_can_request_render_reentrantly
expect_killed invalidate-callback-under-state-borrow pie-app tui_controller component_invalidation_can_request_render_reentrantly
expect_killed disable-host-action-reentry-guard pie-app tui_controller every_host_callback_can_reenter_a_read_and_host_requiring_operation
expect_killed skip-final-drop-reconciliation pie-app tui_controller every_host_callback_can_reenter_a_read_and_host_requiring_operation
expect_killed disable-terminal-event-wake pie-app tui_controller terminal_callbacks_wake_and_drain_input_and_resize
expect_killed skip-padding-empty-visible-overlay-set pie-app tui_controller overlay_layout_and_composition_match_every_oracle_row
expect_killed gate-notification-cleanup-on-started pie-app tui_controller drop_restores_pre_start_terminal_side_effects_without_stopping
expect_killed gate-cursor-cleanup-on-started pie-app tui_controller drop_restores_pre_start_terminal_side_effects_without_stopping
expect_killed stop-terminal-before-start pie-app tui_controller drop_restores_pre_start_terminal_side_effects_without_stopping
expect_killed erase-nested-component-ownership pie-components tui_contracts erased_mount_is_nested_identity_safe
