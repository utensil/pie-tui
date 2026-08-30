#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
consumer_source="$repo_root/tools/fresh-pie-app-consumer"
consumer_root="$(mktemp -d "${TMPDIR:-/tmp}/pie-app-consumer.XXXXXX")"

cleanup() {
  rm -rf -- "$consumer_root"
}
trap cleanup EXIT

cp -R "$consumer_source" "$consumer_root/consumer"
ln -s "$repo_root" "$consumer_root/pie-tui"

manifest="$consumer_root/consumer/Cargo.toml"
if [[ -e "$consumer_root/consumer/Cargo.lock" ]]; then
  echo "fresh consumer unexpectedly started with a lockfile" >&2
  exit 1
fi

rustup run 1.98.0 cargo run --manifest-path "$manifest" --quiet

resolved="$(rustup run 1.98.0 cargo tree \
  --manifest-path "$manifest" \
  --edges normal \
  --prefix none \
  --format '{p}' | awk '/^pie-(app|components|core|term) / { print $1 }' | LC_ALL=C sort -u)"
expected='pie-app
pie-components
pie-core
pie-term'
if [[ $resolved != "$expected" ]]; then
  echo "fresh consumer resolved an unexpected pie crate set:" >&2
  echo "$resolved" >&2
  exit 1
fi

echo "fresh pie-app consumer tree: exact four-crate Rust controller closure"
