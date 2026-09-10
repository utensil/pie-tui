#!/usr/bin/env bash
set -euo pipefail

consumer_root=${CONSUMER_ROOT:?CONSUMER_ROOT is required}
consumer_head=${CONSUMER_HEAD:?CONSUMER_HEAD is required}
repo_root=$(git rev-parse --show-toplevel)
driver=${DRIVER_PATH:-"$repo_root/adapters/pie-napi/test/current-dsh-interactive-tmux.mjs"}
expected_tui_package=${EXPECTED_TUI_PACKAGE:-pie-tui-native}
columns=${TMUX_COLUMNS:-90}
rows=${TMUX_ROWS:-28}
test_root=$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-dsh-tmux.XXXXXX")
socket_name="pie-tui-dsh-$PPID-$$"
session_name=consumer

cleanup() {
  tmux -L "$socket_name" kill-server 2>/dev/null || true
  rm -rf -- "$test_root"
}
trap cleanup EXIT

command -v tmux >/dev/null || {
  echo "tmux is required for the current dsh consumer gate" >&2
  exit 1
}
[[ $columns =~ ^[0-9]+$ && $rows =~ ^[0-9]+$ ]] || {
  echo "TMUX_COLUMNS and TMUX_ROWS must be positive integers" >&2
  exit 1
}
(( columns > 0 && rows > 0 )) || {
  echo "TMUX_COLUMNS and TMUX_ROWS must be positive integers" >&2
  exit 1
}
[[ $(git -C "$consumer_root" rev-parse HEAD) == "$consumer_head" ]] || {
  echo "current-consumer checkout did not resolve the pinned head" >&2
  exit 1
}

receipt="$test_root/receipt.json"
stream_ack="$test_root/stream-ack"
capture="$test_root/pane.txt"
capture_flat="$test_root/pane-flat.txt"
mkdir -p "$test_root/isolated-user"
wait_for_file_field() {
  local pattern=$1
  local deadline=$((SECONDS + 20))
  while (( SECONDS < deadline )); do
    if [[ -f "$receipt" ]] && grep -Eq "$pattern" "$receipt"; then
      return 0
    fi
    sleep 0.1
  done
  echo "timed out waiting for receipt field: $pattern" >&2
  [[ -f "$receipt" ]] && sed -n '1,160p' "$receipt" >&2
  return 1
}
wait_for_pane() {
  local pattern=$1
  local deadline=$((SECONDS + 20))
  while (( SECONDS < deadline )); do
    tmux -L "$socket_name" capture-pane -p -t "$session_name" >"$capture"
    tr '\n' ' ' <"$capture" >"$capture_flat"
    if grep -Eq "$pattern" "$capture_flat"; then
      return 0
    fi
    sleep 0.1
  done
  echo "timed out waiting for pane text: $pattern" >&2
  sed -n '1,200p' "$capture" >&2
  return 1
}

printf -v pane_command \
  'exec env HOME=%q XDG_CONFIG_HOME=%q PI_OFFLINE=1 CONSUMER_ROOT=%q CONSUMER_HEAD=%q RECEIPT_PATH=%q STREAM_ACK_PATH=%q EXPECTED_TUI_PACKAGE=%q node %q' \
  "$test_root/isolated-user" "$test_root/isolated-user/config" "$consumer_root" "$consumer_head" "$receipt" "$stream_ack" "$expected_tui_package" "$driver"
tmux -L "$socket_name" new-session -d -x "$columns" -y "$rows" -s "$session_name" "$pane_command"
tmux -L "$socket_name" set-window-option -t "$session_name" remain-on-exit on >/dev/null
wait_for_file_field '"ready": true'

tmux -L "$socket_name" send-keys -t "$session_name" -l '/set'
wait_for_pane 'Open settings menu|→ settings'
cp "$capture" "$test_root/autocomplete.txt"
tmux -L "$socket_name" send-keys -t "$session_name" Tab Enter
wait_for_pane 'Auto-compact|Autocomplete max items'
cp "$capture" "$test_root/settings.txt"
[[ $columns == 90 ]] || { echo "settings resize gate starts at 90 columns" >&2; exit 1; }
tmux -L "$socket_name" resize-window -t "$session_name" -x 24 -y "$rows"
wait_for_file_field '"lastResizeWidth": 24'
[[ $(tmux -L "$socket_name" display-message -p -t "$session_name" '#{pane_width}') == 24 ]]
wait_for_pane 'Auto-compact'
cp "$capture" "$test_root/settings-narrow.txt"
tmux -L "$socket_name" send-keys -t "$session_name" Space
wait_for_file_field '"columns": 24'
tmux -L "$socket_name" resize-window -t "$session_name" -x 90 -y "$rows"
wait_for_file_field '"lastResizeWidth": 90'
[[ $(tmux -L "$socket_name" display-message -p -t "$session_name" '#{pane_width}') == 90 ]]
wait_for_pane 'Auto-compact +false'
cp "$capture" "$test_root/settings-restored.txt"
tmux -L "$socket_name" send-keys -t "$session_name" Escape
sleep 0.3

tmux -L "$socket_name" send-keys -t "$session_name" -l 'consumer harness prompt'
tmux -L "$socket_name" send-keys -t "$session_name" Enter
wait_for_file_field '"submittedText": "consumer harness prompt"'
wait_for_pane 'stream-only-marker'
: >"$stream_ack"
wait_for_file_field '"streamAcknowledged": true'
wait_for_pane 'deterministic final answer'
wait_for_pane 'fixture-tool-output'
wait_for_pane 'fixture backend complete'
cp "$capture" "$test_root/turn.txt"

tmux -L "$socket_name" send-keys -t "$session_name" C-d
wait_for_file_field '"terminalRestored": true'
wait_for_file_field '"listenersRestored": true'
wait_for_file_field '"exitCode": 0'
pane_deadline=$((SECONDS + 20))
while (( SECONDS < pane_deadline )); do
  pane_state=$(tmux -L "$socket_name" display-message -p -t "$session_name" '#{pane_dead}:#{alternate_on}' 2>/dev/null || true)
  [[ $pane_state == "1:0" ]] && break
  sleep 0.1
done
[[ ${pane_state:-} == "1:0" ]] || {
  echo "tmux pane did not exit outside the alternate screen: ${pane_state:-missing}" >&2
  exit 1
}

node -e '
  const fs = require("node:fs");
  const receipt = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  if (receipt.backend !== "deterministic-fake") throw new Error("unexpected backend");
  if (receipt.submittedText !== "consumer harness prompt") throw new Error("ordinary prompt did not reach host");
  if (JSON.stringify(receipt.resizeWidths) !== "[24,90]") throw new Error("expected live resize from 90 to 24 to 90");
  if (JSON.stringify(receipt.settingsChanges) !== JSON.stringify([{ enabled: false, columns: 24 }])) throw new Error("settings did not change at narrow width");
  if (!receipt.streamAcknowledged) throw new Error("stream-only output was not observed before finalization");
  if (!receipt.realTty || !receipt.terminalRestored || !receipt.listenersRestored) throw new Error("terminal lifecycle failed");
  process.stdout.write(JSON.stringify({ ...receipt, tmuxPaneDead: true, alternateScreen: false }) + "\n");
' "$receipt"
if [[ -n ${RECEIPT_DIR:-} ]]; then
  mkdir -p "$RECEIPT_DIR"
  cp "$receipt" "$test_root"/*.txt "$RECEIPT_DIR/"
fi
echo "current dsh consumer tmux OK: actual InteractiveMode, command dispatch, fake-backend stream/tool turn, clean exit at $consumer_head"
