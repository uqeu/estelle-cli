#!/bin/sh
# Prove a repack produces identical bytes and preserves exactly one executable payload.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/estelle-release-package-test.XXXXXX")
trap 'rm -rf "$TEST_DIR"' EXIT HUP INT TERM
SOURCE_DIR="$TEST_DIR/source"
FIRST_DIR="$TEST_DIR/first"
SECOND_DIR="$TEST_DIR/second"
EXPECTED_TARGET_COUNT=5
TARGETS="aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-msvc"
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
  # The member name is part of the contract the npm launcher checks: `estelle.exe` on Windows only.
  case "$TARGET" in
    *-pc-windows-msvc) MEMBER=estelle.exe ;;
    *) MEMBER=estelle ;;
  esac
  cmp "$FIRST_DIR/$ARCHIVE" "$SECOND_DIR/$ARCHIVE"
  [ "$(tar -tzf "$FIRST_DIR/$ARCHIVE")" = "$MEMBER" ]
  mkdir "$TEST_DIR/unpack-$TARGET"
  tar -xzf "$FIRST_DIR/$ARCHIVE" -C "$TEST_DIR/unpack-$TARGET"
  [ -x "$TEST_DIR/unpack-$TARGET/$MEMBER" ]
  [ "$("$TEST_DIR/unpack-$TARGET/$MEMBER")" = "fixture-$TARGET" ]
  TARGET_COUNT=$((TARGET_COUNT + 1))
done
[ "$TARGET_COUNT" -eq "$EXPECTED_TARGET_COUNT" ]
[ "$(wc -l < "$FIRST_DIR/SHA256SUMS" | tr -d ' ')" -eq "$EXPECTED_TARGET_COUNT" ]
grep -q '  estelle-x86_64-pc-windows-msvc.tar.gz$' "$FIRST_DIR/SHA256SUMS"
cmp "$FIRST_DIR/SHA256SUMS" "$SECOND_DIR/SHA256SUMS"
echo "release package proof: five normalized archives reproduce byte for byte"
