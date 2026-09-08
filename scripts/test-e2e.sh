#!/bin/sh
set -eu

SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPOSITORY_ROOT=$(dirname -- "$SCRIPT_DIRECTORY")
FIXTURE="$REPOSITORY_ROOT/tests/fixtures/typescript-project"
ADAPTER="$REPOSITORY_ROOT/adapters/typescript/bin/vibedoc-adapter-typescript"
VIBEDOC="$REPOSITORY_ROOT/target/debug/vibedoc"
PATH="$REPOSITORY_ROOT/adapters/typescript/bin:$PATH"
export PATH

cargo build --workspace --manifest-path "$REPOSITORY_ROOT/Cargo.toml"
npm --prefix "$REPOSITORY_ROOT" run build

(cd "$FIXTURE" && "$VIBEDOC" check --adapter-command "typescript=$ADAPTER")
(cd "$FIXTURE" && "$VIBEDOC" doctor --format json)
(cd "$FIXTURE" && "$VIBEDOC" inspect --adapter typescript --symbol AuthenticationService.login --format json)
(cd "$FIXTURE" && "$VIBEDOC" explain VDOC-G006 --format json)

TEMP_DIRECTORY=$(mktemp -d)
trap 'rm -rf "$TEMP_DIRECTORY"' EXIT HUP INT TERM

set +e
(cd "$FIXTURE" && "$VIBEDOC" check cases/invalid.md \
  --profile reference \
  --project typescript=tsconfig.json \
  --adapter-command "typescript=$ADAPTER" \
  --experimental \
  --format json) >"$TEMP_DIRECTORY/invalid.json"
STATUS=$?
set -e

if [ "$STATUS" -ne 1 ]; then
  echo "Expected seeded diagnostic fixture to exit 1; got $STATUS." >&2
  exit 1
fi

for RULE in VDOC-L001 VDOC-L002 VDOC-L003 VDOC-T001 VDOC-D001 VDOC-D002 VDOC-G003 VDOC-G004 VDOC-G005 VDOC-G006 VDOC-G007 VDOC-X001 VDOC-X002; do
  grep -q "\"ruleId\": \"$RULE\"" "$TEMP_DIRECTORY/invalid.json"
done

node "$REPOSITORY_ROOT/scripts/validate-report.mjs" \
  "$REPOSITORY_ROOT/schemas/vibedoc-report.schema.json" \
  "$TEMP_DIRECTORY/invalid.json"

echo "End-to-end checks passed."
