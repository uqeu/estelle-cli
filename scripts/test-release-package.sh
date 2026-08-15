#!/bin/sh
# Prove a repack produces identical bytes and preserves exactly one executable payload.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/estelle-release-package-test.XXXXXX")
trap 'rm -rf "$TEST_DIR"' EXIT HUP INT TERM
SOURCE_DIR="$TEST_DIR/source"
FIRST_DIR="$TEST_DIR/first"
SECOND_DIR="$TEST_DIR/second"
EXPECTED_TARGET_COUNT=4
TARGETS="aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu"
mkdir "$SOURCE_DIR"

WORKFLOW="$ROOT/.github/workflows/release.yml"
[ "$(grep -Ec '^[[:space:]]+target: ' "$WORKFLOW")" -eq "$EXPECTED_TARGET_COUNT" ]
TARGET_COUNT=0
for TARGET in $TARGETS; do
  [ "$(grep -Ec "^[[:space:]]+target: $TARGET$" "$WORKFLOW")" -eq 1 ]
  printf '#!/bin/sh\nprintf "fixture-%s\\n"\n' "$TARGET" > "$SOURCE_DIR/estelle-$TARGET"
  chmod 0755 "$SOURCE_DIR/estelle-$TARGET"
  TARGET_COUNT=$((TARGET_COUNT + 1))
done
[ "$TARGET_COUNT" -eq "$EXPECTED_TARGET_COUNT" ]

"$ROOT/scripts/package-release.sh" "$SOURCE_DIR" "$FIRST_DIR"
TZ=UTC0 touch -t 202608150101 "$SOURCE_DIR"/estelle-*
"$ROOT/scripts/package-release.sh" "$SOURCE_DIR" "$SECOND_DIR"

TARGET_COUNT=0
for TARGET in $TARGETS; do
  ARCHIVE="estelle-$TARGET.tar.gz"
  cmp "$FIRST_DIR/$ARCHIVE" "$SECOND_DIR/$ARCHIVE"
  [ "$(tar -tzf "$FIRST_DIR/$ARCHIVE")" = estelle ]
  mkdir "$TEST_DIR/unpack-$TARGET"
  tar -xzf "$FIRST_DIR/$ARCHIVE" -C "$TEST_DIR/unpack-$TARGET"
  [ -x "$TEST_DIR/unpack-$TARGET/estelle" ]
  [ "$("$TEST_DIR/unpack-$TARGET/estelle")" = "fixture-$TARGET" ]
  TARGET_COUNT=$((TARGET_COUNT + 1))
done
[ "$TARGET_COUNT" -eq "$EXPECTED_TARGET_COUNT" ]
cmp "$FIRST_DIR/SHA256SUMS" "$SECOND_DIR/SHA256SUMS"
echo "release package proof: four normalized archives reproduce byte for byte"
