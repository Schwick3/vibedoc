#!/bin/sh
set -eu

SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPOSITORY_ROOT=$(dirname -- "$SCRIPT_DIRECTORY")

case "$(uname -s)" in
  Darwin|Linux) ;;
  *)
    echo "Vibedoc v1 supports macOS and Linux." >&2
    exit 2
    ;;
esac

command -v cargo >/dev/null 2>&1 || {
  echo "Cargo is required." >&2
  exit 2
}
command -v node >/dev/null 2>&1 || {
  echo "Node.js 22 or later is required." >&2
  exit 2
}
command -v npm >/dev/null 2>&1 || {
  echo "npm is required." >&2
  exit 2
}

NODE_MAJOR=$(node -p "Number(process.versions.node.split('.')[0])")
if [ "$NODE_MAJOR" -lt 22 ]; then
  echo "Node.js 22 or later is required; found $(node --version)." >&2
  exit 2
fi

cargo install --path "$REPOSITORY_ROOT/crates/vibedoc-cli" --force
npm --prefix "$REPOSITORY_ROOT" ci
npm --prefix "$REPOSITORY_ROOT" run build
(cd "$REPOSITORY_ROOT/adapters/typescript" && npm link)

echo "Installed vibedoc and linked vibedoc-adapter-typescript."
