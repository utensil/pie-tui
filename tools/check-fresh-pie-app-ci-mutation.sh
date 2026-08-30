#!/bin/sh
# Prove the CI-owned fresh pie-app consumer cannot be silently orphaned.
set -eu

REPOSITORY_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MUTATION_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pie-app-ci-mutation.XXXXXX")
trap 'rm -rf -- "$MUTATION_ROOT"' EXIT HUP INT TERM

git -C "$REPOSITORY_ROOT" archive HEAD | tar -xf - -C "$MUTATION_ROOT"
cp "$REPOSITORY_ROOT/.github/workflows/ci.yml" "$MUTATION_ROOT/.github/workflows/ci.yml"
cp "$REPOSITORY_ROOT/tools/check-coverage.mjs" "$MUTATION_ROOT/tools/check-coverage.mjs"

baseline_log="$MUTATION_ROOT/baseline.log"
if ! CARGO_TARGET_DIR="$MUTATION_ROOT/target" \
    node "$MUTATION_ROOT/tools/check-coverage.mjs" >"$baseline_log" 2>&1
then
    printf 'baseline fresh pie-app CI receipt failed\n' >&2
    cat "$baseline_log" >&2
    exit 1
fi

MUTATION_ROOT="$MUTATION_ROOT" node --input-type=module <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const path = join(process.env.MUTATION_ROOT, ".github/workflows/ci.yml");
const source = readFileSync(path, "utf8");
const marker = "        run: bash tools/check-fresh-pie-app-consumer.sh\n";
const occurrences = source.split(marker).length - 1;
if (occurrences !== 1) {
  throw new Error(`expected one fresh pie-app CI marker, found ${occurrences}`);
}
writeFileSync(path, source.replace(marker, ""));
NODE

mutation_log="$MUTATION_ROOT/omission.log"
if CARGO_TARGET_DIR="$MUTATION_ROOT/target" \
    node "$MUTATION_ROOT/tools/check-coverage.mjs" >"$mutation_log" 2>&1
then
    printf 'fresh pie-app CI omission mutation survived\n' >&2
    exit 1
fi
if ! grep -Fq 'G0 fresh pie-app consumer CI command count: expected 1, got 0' "$mutation_log"
then
    printf 'fresh pie-app CI omission missed its ownership gate\n' >&2
    cat "$mutation_log" >&2
    exit 1
fi

printf 'fresh pie-app CI omission mutation killed\n'
