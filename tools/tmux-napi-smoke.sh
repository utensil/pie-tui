#!/usr/bin/env bash
# Real-PTY smoke for the canonical Node-API ProcessTerminal and screen facades.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
demo="$repo_root/adapters/pie-napi/test/fixtures/napi-screen-demo.mjs"
session_prefix="pie-napi-smoke-$$"
failures=0

if ! command -v tmux >/dev/null 2>&1; then
  echo "SKIP: tmux not installed"
  exit 0
fi
if [[ ! -f $demo ]]; then
  echo "FAIL: NAPI screen demo not found at $demo"
  exit 1
fi

cleanup() {
  tmux kill-session -t "${session_prefix}-main" 2>/dev/null || true
  tmux kill-session -t "${session_prefix}-alt" 2>/dev/null || true
}
trap cleanup EXIT

capture() { tmux capture-pane -p -t "$1"; }

wait_until_contains() {
  local session="$1" needle="$2" attempt=0
  while (( attempt < 50 )); do
    if capture "$session" | grep -qF -- "$needle"; then return 0; fi
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
    failures=$((failures + 1))
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
    failures=$((failures + 1))
  else
    echo "  ok: clean host teardown"
  fi
}

for mode in main alt; do
  session="${session_prefix}-${mode}"
  echo "[$mode] Node-API screen, ProcessTerminal input/resize, and teardown"
  tmux new-session -d -x 50 -y 8 -s "$session" "node '$demo' '$mode'"
  wait_until_contains "$session" "pie-tui-native $mode screen" || true
  assert_contains "$session" "screen rendered" "pie-tui-native $mode screen"
  assert_contains "$session" "current reference contract" "reference contract: pi-tui 0.84.2"
  assert_contains "$session" "initial geometry" "viewport: 50x8"
  if [[ $mode == alt ]]; then
    if [[ $(tmux display-message -p -t "$session" '#{alternate_on}') == 1 ]]; then
      echo "  ok: alternate buffer active"
    else
      echo "  FAIL: alternate buffer inactive"
      failures=$((failures + 1))
    fi
  fi
  tmux send-keys -t "$session" -l -- j
  wait_until_contains "$session" "count: 1" || true
  assert_contains "$session" "input reached focused component" "count: 1"
  tmux resize-window -t "$session" -x 42 -y 7
  wait_until_contains "$session" "viewport: 42x7" || true
  assert_contains "$session" "resize reached screen" "viewport: 42x7"
  if [[ $mode == alt ]]; then
    tmux send-keys -t "$session" -l -- $'\033[<64;1;1M'
    sleep 0.05
    assert_contains "$session" "SGR mouse input retained the Alt frame" "pie-tui-native alt screen"
    if [[ $(tmux display-message -p -t "$session" '#{alternate_on}') == 1 ]]; then
      echo "  ok: SGR mouse input kept alternate buffer active"
    else
      echo "  FAIL: alternate buffer inactive after SGR mouse input"
      failures=$((failures + 1))
    fi
  fi
  tmux send-keys -t "$session" Escape
  sleep 0.05
  if tmux has-session -t "$session" 2>/dev/null; then
    echo "  ok: lone Escape flushed without terminating"
  else
    echo "  FAIL: session exited after lone Escape"
    failures=$((failures + 1))
  fi
  tmux send-keys -t "$session" -l -- q
  wait_for_exit "$session"
done

if (( failures > 0 )); then
  echo "NAPI tmux smoke: $failures failure(s)"
  exit 1
fi
echo "NAPI tmux smoke: all assertions passed"
