#!/usr/bin/env bash
# tmux-smoke.sh — real-terminal functional verification (verification ladder 4).
# Boots pie-demo in a tmux session, drives keys, and asserts capture-pane output.
# Usage: tools/tmux-smoke.sh [path-to-pie-demo]   (default: target/debug/pie-demo)
set -euo pipefail

DEMO="${1:-target/debug/pie-demo}"
SESSION="pie-tui-smoke-$$"
FAILURES=0

if ! command -v tmux >/dev/null 2>&1; then
  echo "SKIP: tmux not installed"
  exit 0
fi
if [[ ! -x $DEMO ]]; then
  echo "FAIL: demo binary not found at $DEMO (build with cargo build -p pie-app)"
  exit 1
fi

cleanup() {
  tmux kill-session -t "$SESSION" 2>/dev/null || true
}
trap cleanup EXIT

tmux new-session -d -x 80 -y 24 -s "$SESSION" "$PWD/$DEMO"

capture() {
  tmux capture-pane -p -t "$SESSION"
}

wait_until_contains() {
  local needle="$1" attempt=0
  while (( attempt < 50 )); do
    if capture | grep -qF -- "$needle"; then
      return 0
    fi
    sleep 0.1
    attempt=$((attempt + 1))
  done
  return 1
}

assert_contains() {
  local label="$1" needle="$2"
  if capture | grep -qF -- "$needle"; then
    echo "  ok: $label"
  else
    echo "  FAIL: $label (missing: $needle)"
    echo "  --- capture-pane ---"
    capture | sed 's/^/  | /'
    FAILURES=$((FAILURES + 1))
  fi
}

assert_not_contains() {
  local label="$1" needle="$2"
  if capture | grep -qF -- "$needle"; then
    echo "  FAIL: $label (unexpected: $needle)"
    FAILURES=$((FAILURES + 1))
  else
    echo "  ok: $label"
  fi
}

assert_equals() {
  local label="$1" expected="$2" actual="$3"
  if [[ $actual == "$expected" ]]; then
    echo "  ok: $label"
  else
    echo "  FAIL: $label (expected: $expected, actual: $actual)"
    FAILURES=$((FAILURES + 1))
  fi
}

assert_nonblank_lines() {
  local label="$1" expected="$2" actual
  actual="$(capture | awk 'NF { count++ } END { print count + 0 }')"
  assert_equals "$label" "$expected" "$actual"
}

echo "[1] initial frame"
wait_until_contains "viewport: 80x24" || true
assert_contains "title rendered" "pie-demo — pie-tui Rust port smoke"
assert_contains "counter at zero" "count: 0"
assert_contains "initial geometry rendered" "viewport: 80x24"
assert_contains "hint line" "space/j: +1   k: -1   q: quit"
assert_nonblank_lines "probe fits one line at width 80" "5"

echo "[2] increment via space and j"
tmux send-keys -t "$SESSION" " "
tmux send-keys -t "$SESSION" "j"
wait_until_contains "count: 2" || true
assert_contains "counter at two" "count: 2"

echo "[3] decrement via k"
tmux send-keys -t "$SESSION" "k"
wait_until_contains "count: 1" || true
assert_contains "counter at one" "count: 1"

echo "[4] resize redraw"
tmux resize-window -t "$SESSION" -x 60 -y 20
wait_until_contains "viewport: 60x20" || true
assert_equals "tmux changed geometry" "60x20" "$(tmux display-message -p -t "$SESSION" '#{window_width}x#{window_height}')"
assert_contains "application observed geometry" "viewport: 60x20"
assert_not_contains "stale geometry cleared" "viewport: 80x24"
assert_contains "title survives resize" "pie-demo — pie-tui Rust port smoke"
assert_contains "counter survives resize" "count: 1"
assert_nonblank_lines "probe rewraps to two lines at width 60" "6"

echo "[5] literal input chunk"
tmux send-keys -t "$SESSION" -l -- "xxxx"
assert_contains "counter unchanged by letters" "count: 1"

echo "[6] lone Escape timeout"
tmux send-keys -t "$SESSION" Escape
sleep 0.25
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "  ok: session remains alive after lone Escape"
else
  echo "  FAIL: session exited after lone Escape"
  FAILURES=$((FAILURES + 1))
fi

echo "[7] quit via one q after Escape timeout"
tmux send-keys -t "$SESSION" -l -- "q"
attempt=0
while tmux has-session -t "$SESSION" 2>/dev/null && (( attempt < 50 )); do
  sleep 0.1
  attempt=$((attempt + 1))
done
if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "  FAIL: session still alive after one q"
  FAILURES=$((FAILURES + 1))
else
  echo "  ok: clean exit on one q"
fi

if [[ $FAILURES -gt 0 ]]; then
  echo "tmux smoke: $FAILURES failure(s)"
  exit 1
fi
echo "tmux smoke: all assertions passed"
