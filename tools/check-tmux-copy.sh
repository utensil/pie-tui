#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
entry=${1:-"$repo_root/adapters/pie-napi/index.js"}
driver=${DRIVER_PATH:-"$repo_root/adapters/pie-napi/test/tmux-copy-driver.mjs"}
test_root=$(mktemp -d "${TMPDIR:-/tmp}/pie-copy-tmux.XXXXXX")
socket_name="pie-copy-$PPID-$$"
session_name=copy
receipt="$test_root/receipt.json"
raw="$test_root/raw.ansi"

cleanup() {
  tmux -L "$socket_name" kill-server 2>/dev/null || true
  if [[ ${KEEP_TMUX_ARTIFACTS:-0} == 1 ]]; then
    echo "tmux artifacts: $test_root" >&2
  else
    rm -rf -- "$test_root"
  fi
}
trap cleanup EXIT

command -v tmux >/dev/null || { echo 'tmux is required' >&2; exit 1; }
mkdir "$test_root/isolated-user"

wait_for_receipt() {
  local expression=$1
  local deadline=$((SECONDS + 20))
  while (( SECONDS < deadline )); do
    if [[ -f "$receipt" ]] && node -e "const r=require(process.argv[1]); if (!($expression)) process.exit(1)" "$receipt" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "timed out waiting for receipt: $expression" >&2
  [[ -f "$receipt" ]] && cat "$receipt" >&2
  return 1
}

printf -v pane_command 'exec env HOME=%q RECEIPT_PATH=%q node %q %q' \
  "$test_root/isolated-user" "$receipt" "$driver" "$entry"
tmux -L "$socket_name" new-session -d -x 30 -y 8 -s "$session_name" "$pane_command"
tmux -L "$socket_name" set-window-option -t "$session_name" remain-on-exit on >/dev/null
wait_for_receipt 'r.ready === true'

printf -v pipe_command 'exec cat > %q' "$raw"
tmux -L "$socket_name" pipe-pane -t "$session_name" "$pipe_command"

tmux -L "$socket_name" send-keys -t "$session_name" -H 1b 5b 3c 30 3b 31 3b 31 4d
sleep 0.1
tmux -L "$socket_name" send-keys -t "$session_name" -H 1b 5b 3c 33 32 3b 35 3b 31 4d
sleep 0.1
tmux -L "$socket_name" send-keys -t "$session_name" -H 1b 5b 3c 30 3b 35 3b 31 6d
wait_for_receipt 'r.selected === true'
sleep 0.2

node -e '
  const fs = require("node:fs");
  const raw = fs.readFileSync(process.argv[1], "latin1");
  if (raw.includes("\x1b]52;c;")) throw new Error("copyOnSelect:false emitted OSC 52 during drag");
' "$raw"

tmux -L "$socket_name" send-keys -t "$session_name" -l c
wait_for_receipt 'r.explicitCopy === true'
osc_deadline=$((SECONDS + 20))
while (( SECONDS < osc_deadline )); do
  if node -e '
    const fs = require("node:fs");
    const raw = fs.readFileSync(process.argv[1], "latin1");
    if (!raw.includes("\x1b]52;c;YWxwaGE=\x07")) process.exit(1);
  ' "$raw" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
(( SECONDS < osc_deadline )) || { echo 'explicit copy did not emit OSC 52 for alpha' >&2; exit 1; }

tmux -L "$socket_name" send-keys -t "$session_name" -H 1b 5b 3c 30 3b 31 3b 31 4d
sleep 0.1
tmux -L "$socket_name" send-keys -t "$session_name" -H 1b 5b 3c 30 3b 31 3b 31 6d
wait_for_receipt 'r.selectionCleared === true && r.selected === false'
tmux -L "$socket_name" send-keys -t "$session_name" -l q
wait_for_receipt 'r.terminalRestored === true && r.listenersRestored === true && r.exitCode === 0'

pane_deadline=$((SECONDS + 20))
pane_state=''
while (( SECONDS < pane_deadline )); do
  pane_state=$(tmux -L "$socket_name" display-message -p -t "$session_name" '#{pane_dead}:#{pane_dead_status}:#{alternate_on}' 2>/dev/null || true)
  [[ $pane_state == '1:0:0' ]] && break
  sleep 0.1
done
[[ $pane_state == '1:0:0' ]] || { echo "pane teardown failed: ${pane_state:-missing}" >&2; exit 1; }

node -e '
  const r = require(process.argv[1]);
  if (r.copyOnSelect !== false || r.selected !== false || r.selectionCleared !== true) {
    throw new Error("invalid copy mode or clear receipt");
  }
  process.stdout.write(JSON.stringify({ ...r, osc52Payload: "alpha", tmuxPaneDead: true, tmuxExitCode: 0, alternateScreen: false }) + "\n");
' "$receipt"
