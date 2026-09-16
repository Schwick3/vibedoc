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
node "$SCRIPT_DIRECTORY/release-version.mjs" "v$VERSION" >/dev/null
STAGING_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/vibedoc-adapter-python.XXXXXX")
trap 'rm -rf "$STAGING_DIRECTORY"' EXIT HUP INT TERM
mkdir -p "$STAGING_DIRECTORY/bin" "$OUTPUT_DIRECTORY"
cp "$REPOSITORY_ROOT/adapters/python/adapter.py" "$STAGING_DIRECTORY/adapter.py"
cp "$REPOSITORY_ROOT/adapters/python/bin/vibedoc-adapter-python" "$STAGING_DIRECTORY/bin/"
cp "$REPOSITORY_ROOT/LICENSE" "$STAGING_DIRECTORY/LICENSE"
chmod 755 "$STAGING_DIRECTORY/bin/vibedoc-adapter-python"
ARCHIVE="$OUTPUT_DIRECTORY/vibedoc-adapter-python-v$VERSION.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$ARCHIVE" -C "$STAGING_DIRECTORY" .
echo "$ARCHIVE"
