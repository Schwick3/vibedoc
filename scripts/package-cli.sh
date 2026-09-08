#!/bin/sh
set -eu

if [ "$#" -ne 4 ]; then
  echo "usage: $0 VERSION TARGET BINARY OUTPUT_DIRECTORY" >&2
  exit 2
fi

VERSION=$1
TARGET=$2
BINARY=$3
OUTPUT_DIRECTORY=$4

SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPOSITORY_ROOT=$(dirname -- "$SCRIPT_DIRECTORY")

node "$REPOSITORY_ROOT/scripts/release-version.mjs" "v$VERSION" >/dev/null

if [ ! -f "$BINARY" ]; then
  echo "Vibedoc binary does not exist: $BINARY" >&2
  exit 2
fi

case "$TARGET" in
  aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;;
  *)
    echo "Unsupported release target: $TARGET" >&2
    exit 2
    ;;
esac

REPORTED_VERSION=$("$BINARY" --version)
if [ "$REPORTED_VERSION" != "vibedoc $VERSION" ]; then
  echo "Binary version does not match $VERSION: $REPORTED_VERSION" >&2
  exit 2
fi

STAGING_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/vibedoc-cli.XXXXXX")
trap 'rm -rf "$STAGING_DIRECTORY"' EXIT HUP INT TERM

mkdir -p "$OUTPUT_DIRECTORY"
cp "$BINARY" "$STAGING_DIRECTORY/vibedoc"
chmod 755 "$STAGING_DIRECTORY/vibedoc"
cp "$REPOSITORY_ROOT/LICENSE" "$STAGING_DIRECTORY/LICENSE"
cp "$REPOSITORY_ROOT/README.md" "$STAGING_DIRECTORY/README.md"

ARCHIVE="$OUTPUT_DIRECTORY/vibedoc-v$VERSION-$TARGET.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$ARCHIVE" -C "$STAGING_DIRECTORY" .
echo "$ARCHIVE"
