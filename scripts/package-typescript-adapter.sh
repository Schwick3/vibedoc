#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 VERSION OUTPUT_DIRECTORY" >&2
  exit 2
fi

VERSION=$1
OUTPUT_DIRECTORY=$2

SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPOSITORY_ROOT=$(dirname -- "$SCRIPT_DIRECTORY")
ADAPTER_ROOT="$REPOSITORY_ROOT/adapters/typescript"

node "$REPOSITORY_ROOT/scripts/release-version.mjs" "v$VERSION" >/dev/null
npm --prefix "$REPOSITORY_ROOT" run build

TYPESCRIPT_ROOT="$ADAPTER_ROOT/node_modules/typescript"
if [ ! -d "$TYPESCRIPT_ROOT" ]; then
  TYPESCRIPT_ROOT="$REPOSITORY_ROOT/node_modules/typescript"
fi
if [ ! -d "$TYPESCRIPT_ROOT" ]; then
  echo "The pinned TypeScript production dependency is not installed" >&2
  exit 2
fi

STAGING_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/vibedoc-adapter-typescript.XXXXXX")
trap 'rm -rf "$STAGING_DIRECTORY"' EXIT HUP INT TERM

mkdir -p "$STAGING_DIRECTORY/node_modules" "$OUTPUT_DIRECTORY"
cp -R "$ADAPTER_ROOT/dist" "$STAGING_DIRECTORY/dist"
cp -R "$TYPESCRIPT_ROOT" "$STAGING_DIRECTORY/node_modules/typescript"
cp "$ADAPTER_ROOT/package.json" "$STAGING_DIRECTORY/package.json"
cp "$REPOSITORY_ROOT/LICENSE" "$STAGING_DIRECTORY/LICENSE"

ARCHIVE="$OUTPUT_DIRECTORY/vibedoc-adapter-typescript-v$VERSION.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$ARCHIVE" -C "$STAGING_DIRECTORY" .
echo "$ARCHIVE"
