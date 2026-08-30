#!/usr/bin/env bash
# Real-PTY smoke for the injected pinned-0.84.1 Main and Alt controllers.
set -euo pipefail

DEMO="${1:-target/debug/pie-screen-demo}"
SESSION_PREFIX="pie-main-alt-smoke-$$"
FAILURES=0

if ! command -v tmux >/dev/null 2>&1; then
  echo "SKIP: tmux not installed"
  exit 0
fi
if [[ ! -x $DEMO ]]; then
  echo "FAIL: screen demo binary not found at $DEMO"
  exit 1
fi

cleanup() {
  tmux kill-session -t "${SESSION_PREFIX}-main" 2>/dev/null || true
  tmux kill-session -t "${SESSION_PREFIX}-alt" 2>/dev/null || true
}
trap cleanup EXIT

capture() {
  tmux capture-pane -p -t "$1"
}

wait_until_contains() {
  local session="$1" needle="$2" attempt=0
  while (( attempt < 50 )); do
    if capture "$session" | grep -qF -- "$needle"; then
      return 0
    fi
    sleep 0.1
    attempt=$((attempt + 1))
  done
  return 1
}

assert_contains() {
  local session="$1" label="$2" needle="$3"
  if capture "$session" | grep -qF -- "$needle"; then
    echo "  ok: $label"
  else
    echo "  FAIL: $label (missing: $needle)"
    capture "$session" | sed 's/^/  | /'
    FAILURES=$((FAILURES + 1))
  fi
}

wait_for_exit() {
  local session="$1" attempt=0
  while tmux has-session -t "$session" 2>/dev/null && (( attempt < 50 )); do
    sleep 0.1
    attempt=$((attempt + 1))
  done
  if tmux has-session -t "$session" 2>/dev/null; then
    echo "  FAIL: $session did not exit"
    FAILURES=$((FAILURES + 1))
  else
    echo "  ok: clean exit"
  fi
}

DEMO_PATH="$PWD/$DEMO"

MAIN_SESSION="${SESSION_PREFIX}-main"
echo "[main] controller lifecycle, input, and resize"
tmux new-session -d -x 50 -y 8 -s "$MAIN_SESSION" "$DEMO_PATH main"
wait_until_contains "$MAIN_SESSION" "pie-screen-demo main controller" || true
assert_contains "$MAIN_SESSION" "main controller rendered" "pie-screen-demo main controller"
assert_contains "$MAIN_SESSION" "pinned reference visible" "reference: pi-tui 0.84.1"
assert_contains "$MAIN_SESSION" "main initial geometry" "viewport: 50x8"
tmux send-keys -t "$MAIN_SESSION" -l -- "j"
wait_until_contains "$MAIN_SESSION" "count: 1" || true
assert_contains "$MAIN_SESSION" "main differential input" "count: 1"
tmux resize-window -t "$MAIN_SESSION" -x 42 -y 7
wait_until_contains "$MAIN_SESSION" "viewport: 42x7" || true
assert_contains "$MAIN_SESSION" "main resize redraw" "viewport: 42x7"
tmux send-keys -t "$MAIN_SESSION" -l -- "q"
wait_for_exit "$MAIN_SESSION"

ALT_SESSION="${SESSION_PREFIX}-alt"
echo "[alt] controller lifecycle and alternate-buffer ownership"
tmux new-session -d -x 50 -y 8 -s "$ALT_SESSION" "$DEMO_PATH alt"
wait_until_contains "$ALT_SESSION" "pie-screen-demo alt controller" || true
assert_contains "$ALT_SESSION" "alt controller rendered" "pie-screen-demo alt controller"
assert_contains "$ALT_SESSION" "alt mode rendered" "mode: alt"
if [[ $(tmux display-message -p -t "$ALT_SESSION" '#{alternate_on}') == 1 ]]; then
  echo "  ok: alternate buffer active"
else
  echo "  FAIL: alternate buffer inactive"
  FAILURES=$((FAILURES + 1))
fi
tmux send-keys -t "$ALT_SESSION" -l -- "j"
wait_until_contains "$ALT_SESSION" "count: 1" || true
assert_contains "$ALT_SESSION" "alt differential input" "count: 1"
tmux send-keys -t "$ALT_SESSION" -l -- "q"
wait_for_exit "$ALT_SESSION"

if [[ $FAILURES -gt 0 ]]; then
  echo "main/alt tmux smoke: $FAILURES failure(s)"
  exit 1
fi
echo "main/alt tmux smoke: all assertions passed"
