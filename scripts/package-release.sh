#!/bin/sh
# Package the four release binaries with normalized metadata and a checksum manifest.
set -eu

EXPECTED_TARGET_COUNT=4
ARCHIVE_MTIME=200001010000
TARGETS="aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu"

SOURCE_DIR=${1:?usage: package-release.sh SOURCE_DIR OUTPUT_DIR}
OUTPUT_DIR=${2:?usage: package-release.sh SOURCE_DIR OUTPUT_DIR}

[ -d "$SOURCE_DIR" ] || {
  echo "release package: source directory does not exist: $SOURCE_DIR" >&2
  exit 2
}
mkdir -p "$OUTPUT_DIR"
STAGE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/estelle-release-package.XXXXXX")
trap 'rm -rf "$STAGE_DIR"' EXIT HUP INT TERM

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

sha256_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{ print $1 }'
  else
    shasum -a 256 | awk '{ print $1 }'
  fi
}

write_archive() {
  stage=$1
  archive=$2
  tar_file="$stage/estelle.tar"
  case "$(tar --version 2>&1 | head -n 1)" in
    *bsdtar*)
      tar --format ustar --uid 0 --gid 0 --uname root --gname root \
        -C "$stage" -cf "$tar_file" estelle
      ;;
    *)
      tar --format=ustar --owner=0 --group=0 --numeric-owner \
        -C "$stage" -cf "$tar_file" estelle
      ;;
  esac
  gzip -n -c "$tar_file" > "$archive"
}

TARGET_COUNT=0
for TARGET in $TARGETS; do
  RAW_BINARY="$SOURCE_DIR/estelle-$TARGET"
  ARCHIVE="$OUTPUT_DIR/estelle-$TARGET.tar.gz"
  TARGET_STAGE="$STAGE_DIR/$TARGET"
  [ -f "$RAW_BINARY" ] && [ ! -L "$RAW_BINARY" ] || {
    echo "release package: missing regular binary: $RAW_BINARY" >&2
    exit 1
  }
  [ ! -e "$ARCHIVE" ] || {
    echo "release package: refusing to overwrite: $ARCHIVE" >&2
    exit 1
  }
  mkdir "$TARGET_STAGE"
  install -m 0755 "$RAW_BINARY" "$TARGET_STAGE/estelle"
  TZ=UTC0 touch -t "$ARCHIVE_MTIME" "$TARGET_STAGE/estelle"
  write_archive "$TARGET_STAGE" "$ARCHIVE"
  [ "$(tar -tzf "$ARCHIVE")" = estelle ]
  [ "$(sha256_file "$RAW_BINARY")" = "$(tar -xOzf "$ARCHIVE" estelle | sha256_stream)" ]
  TARGET_COUNT=$((TARGET_COUNT + 1))
done
[ "$TARGET_COUNT" -eq "$EXPECTED_TARGET_COUNT" ]

MANIFEST="$OUTPUT_DIR/SHA256SUMS"
[ ! -e "$MANIFEST" ] || {
  echo "release package: refusing to overwrite: $MANIFEST" >&2
  exit 1
}
for TARGET in $TARGETS; do
  ARCHIVE_NAME="estelle-$TARGET.tar.gz"
  HASH=$(sha256_file "$OUTPUT_DIR/$ARCHIVE_NAME")
  printf '%s  %s\n' "$HASH" "$ARCHIVE_NAME"
done | LC_ALL=C sort -k2 > "$MANIFEST"
[ "$(wc -l < "$MANIFEST" | tr -d ' ')" -eq "$EXPECTED_TARGET_COUNT" ]
