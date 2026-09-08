#!/bin/sh
set -eu

SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPOSITORY_ROOT=$(dirname -- "$SCRIPT_DIRECTORY")
PROJECT="$REPOSITORY_ROOT/tests/projects/task-service"
ADAPTER_DIRECTORY="$REPOSITORY_ROOT/adapters/typescript/bin"
VIBEDOC="$REPOSITORY_ROOT/target/debug/vibedoc"
SCHEMA="$REPOSITORY_ROOT/schemas/vibedoc-report.schema.json"
PATH="$ADAPTER_DIRECTORY:$PATH"
export PATH

cargo build --workspace --manifest-path "$REPOSITORY_ROOT/Cargo.toml"
npm --prefix "$REPOSITORY_ROOT" run build

TEMP_DIRECTORY=$(mktemp -d)
trap 'rm -rf "$TEMP_DIRECTORY"' EXIT HUP INT TERM

(cd "$PROJECT" && "$VIBEDOC" doctor --format json) >"$TEMP_DIRECTORY/doctor.json"
(cd "$PROJECT" && "$VIBEDOC" inspect \
  --adapter typescript \
  --symbol TaskService.complete \
  --format json) >"$TEMP_DIRECTORY/inspect.json"
(cd "$PROJECT" && "$VIBEDOC" check --format json) >"$TEMP_DIRECTORY/valid.json"

node "$REPOSITORY_ROOT/scripts/assert-evaluation.mjs" doctor "$TEMP_DIRECTORY/doctor.json"
node "$REPOSITORY_ROOT/scripts/assert-evaluation.mjs" inspect "$TEMP_DIRECTORY/inspect.json"
node "$REPOSITORY_ROOT/scripts/assert-evaluation.mjs" valid "$TEMP_DIRECTORY/valid.json"
node "$REPOSITORY_ROOT/scripts/validate-report.mjs" "$SCHEMA" "$TEMP_DIRECTORY/valid.json"

set +e
(cd "$PROJECT" && "$VIBEDOC" check cases/invalid-reference.md \
  --profile reference \
  --experimental \
  --format json) >"$TEMP_DIRECTORY/invalid.json"
STATUS=$?
set -e

if [ "$STATUS" -ne 1 ]; then
  echo "Expected the invalid evaluation document to exit 1; got $STATUS." >&2
  exit 1
fi

node "$REPOSITORY_ROOT/scripts/assert-evaluation.mjs" invalid "$TEMP_DIRECTORY/invalid.json"
node "$REPOSITORY_ROOT/scripts/validate-report.mjs" "$SCHEMA" "$TEMP_DIRECTORY/invalid.json"

echo "Task-service evaluation passed."
