#!/usr/bin/env bash
# Clean current-consumer gate for the pi-dsh front door.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
package_root="$repo_root/adapters/pie-napi"
consumer_head="c59fd5de1251438ff3d8cee3fdb22eeedee01626"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/pie-tui-dsh-consumer.XXXXXX")"

cleanup() {
  rm -rf -- "$test_root"
}
trap cleanup EXIT

mkdir "$test_root/pack"
(
  cd "$package_root"
  npm pack --json --pack-destination "$test_root/pack" >/dev/null
)
tarball="$test_root/pack/pie-tui-native-0.1.0.tgz"

git clone --quiet https://github.com/utensil/dsh-pi-tui-mono.git "$test_root/consumer"
git -C "$test_root/consumer" checkout --quiet --detach "$consumer_head"
if [[ $(git -C "$test_root/consumer" rev-parse HEAD) != "$consumer_head" ]]; then
  echo "current-consumer checkout did not resolve the pinned head" >&2
  exit 1
fi

npm --prefix "$test_root/consumer" pkg set \
  "pnpm.overrides.@earendil-works/pi-tui=file:$tarball"
npx --yes pnpm@10.4.1 --dir "$test_root/consumer" install --no-frozen-lockfile
npx --yes pnpm@10.4.1 --dir "$test_root/consumer" \
  --filter @dsh-pi/tui test

CONSUMER_ROOT="$test_root/consumer" CONSUMER_HEAD="$consumer_head" \
  node "$package_root/test/current-dsh-interactive-lifecycle.mjs"

echo "current dsh consumer OK: 40 front-door tests and InteractiveMode full lifecycle at $consumer_head"
