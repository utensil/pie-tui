#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
consumer_source="$repo_root/tools/fresh-pie-core-consumer"
consumer_root="$(mktemp -d "${TMPDIR:-/tmp}/pie-core-consumer.XXXXXX")"

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

actual_tree="$({
  rustup run 1.98.0 cargo tree \
    --manifest-path "$manifest" \
    --edges normal \
    --prefix none \
    --format '{p}'
} | awk '/^(icu_|unicode-segmentation )/ { print $1 " " $2 }' | LC_ALL=C sort -u)"

expected_tree='icu_collections v2.0.0
icu_locale v2.0.0
icu_locale_core v2.0.0
icu_locale_data v2.0.0
icu_provider v2.0.0
icu_segmenter v2.0.0
icu_segmenter_data v2.0.0
unicode-segmentation v1.12.0'

if [[ "$actual_tree" != "$expected_tree" ]]; then
  echo "fresh consumer resolved an unexpected ICU/Unicode tree:" >&2
  echo "$actual_tree" >&2
  exit 1
fi

echo "fresh consumer tree: exact ICU 2.0.0 family / unicode-segmentation 1.12.0"
