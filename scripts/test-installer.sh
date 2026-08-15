#!/bin/sh
# Behavioural installer proof without network: fake uname selects every supported target and a fake curl
# serves checksummed fixture files by basename. Release builders can also pass their real native binary.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/estelle-installer-test.XXXXXX")
trap 'rm -rf "$TEST_DIR"' EXIT HUP INT TERM
mkdir -p "$TEST_DIR/fixture" "$TEST_DIR/bin" "$TEST_DIR/home"
EXPECTED_TARGET_COUNT=4
TARGETS="aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

create_archive() {
  target=$1
  archive="estelle-$target.tar.gz"
  tar -czf "$TEST_DIR/fixture/$archive" -C "$TEST_DIR/fixture" estelle
  hash=$(sha256_file "$TEST_DIR/fixture/$archive")
  printf '%s  %s\n' "$hash" "$archive" >> "$TEST_DIR/fixture/SHA256SUMS"
}

if [ -n "${ESTELLE_TEST_BINARY:-}" ]; then
  SELECTED_TARGET=${ESTELLE_TEST_TARGET:?set the native target for the real test binary}
  case " $TARGETS " in
    *" $SELECTED_TARGET "*) ;;
    *) echo "installer test received unsupported target: $SELECTED_TARGET" >&2; exit 2 ;;
  esac
  cp "$ESTELLE_TEST_BINARY" "$TEST_DIR/fixture/estelle"
  EXPECTED_OUTPUT=${ESTELLE_TEST_EXPECTED_OUTPUT:?set expected output for the real test binary}
  EXPECTED_PROVED_TARGET_COUNT=1
else
  printf '#!/bin/sh\nprintf "fixture-estelle\\n"\n' > "$TEST_DIR/fixture/estelle"
  chmod 0755 "$TEST_DIR/fixture/estelle"
  EXPECTED_OUTPUT=fixture-estelle
  SELECTED_TARGET=x86_64-unknown-linux-gnu
  EXPECTED_PROVED_TARGET_COUNT=$EXPECTED_TARGET_COUNT
fi

: > "$TEST_DIR/fixture/SHA256SUMS"
if [ -n "${ESTELLE_TEST_BINARY:-}" ]; then
  create_archive "$SELECTED_TARGET"
else
  ARCHIVE_TARGET_COUNT=0
  for TARGET in $TARGETS; do
    create_archive "$TARGET"
    ARCHIVE_TARGET_COUNT=$((ARCHIVE_TARGET_COUNT + 1))
  done
  [ "$ARCHIVE_TARGET_COUNT" -eq "$EXPECTED_TARGET_COUNT" ]
fi

cat > "$TEST_DIR/bin/curl" <<'EOF'
#!/bin/sh
set -eu
test "$8" = --output
cp "$ESTELLE_FIXTURE_DIR/${10##*/}" "$9"
EOF
chmod 0755 "$TEST_DIR/bin/curl"

cat > "$TEST_DIR/bin/uname" <<'EOF'
#!/bin/sh
set -eu
case "${1:-}" in
  -s) printf '%s\n' "$ESTELLE_TEST_OS" ;;
  -m) printf '%s\n' "$ESTELLE_TEST_ARCH" ;;
  *) echo "installer test: unexpected uname arguments" >&2; exit 2 ;;
esac
EOF
chmod 0755 "$TEST_DIR/bin/uname"

prove_platform() {
  os=$1
  arch=$2
  target=$3
  install_dir="$TEST_DIR/install-$target"
  PATH="$TEST_DIR/bin:$PATH" HOME="$TEST_DIR/home" ESTELLE_FIXTURE_DIR="$TEST_DIR/fixture" \
    ESTELLE_TEST_OS="$os" ESTELLE_TEST_ARCH="$arch" ESTELLE_INSTALL_DIR="$install_dir" \
    sh "$ROOT/install.sh" >/dev/null
  [ "$("$install_dir/estelle" --version)" = "$EXPECTED_OUTPUT" ]
}

PROVED_TARGET_COUNT=0
if [ -n "${ESTELLE_TEST_BINARY:-}" ]; then
  case "$SELECTED_TARGET" in
    aarch64-apple-darwin) prove_platform Darwin arm64 "$SELECTED_TARGET" ;;
    x86_64-apple-darwin) prove_platform Darwin x86_64 "$SELECTED_TARGET" ;;
    x86_64-unknown-linux-gnu) prove_platform Linux x86_64 "$SELECTED_TARGET" ;;
    aarch64-unknown-linux-gnu) prove_platform Linux aarch64 "$SELECTED_TARGET" ;;
  esac
  PROVED_TARGET_COUNT=1
else
  prove_platform Darwin arm64 aarch64-apple-darwin
  prove_platform Darwin x86_64 x86_64-apple-darwin
  prove_platform Linux x86_64 x86_64-unknown-linux-gnu
  prove_platform Linux aarch64 aarch64-unknown-linux-gnu
  PROVED_TARGET_COUNT=4
fi
[ "$PROVED_TARGET_COUNT" -eq "$EXPECTED_PROVED_TARGET_COUNT" ]

case "$SELECTED_TARGET" in
  aarch64-apple-darwin) MUTANT_OS=Darwin; MUTANT_ARCH=arm64 ;;
  x86_64-apple-darwin) MUTANT_OS=Darwin; MUTANT_ARCH=x86_64 ;;
  x86_64-unknown-linux-gnu) MUTANT_OS=Linux; MUTANT_ARCH=x86_64 ;;
  aarch64-unknown-linux-gnu) MUTANT_OS=Linux; MUTANT_ARCH=aarch64 ;;
esac
ARCHIVE="estelle-$SELECTED_TARGET.tar.gz"
MUTANT_INSTALL="$TEST_DIR/install-mutant"

if PATH="$TEST_DIR/bin:$PATH" HOME="$TEST_DIR/home" ESTELLE_FIXTURE_DIR="$TEST_DIR/fixture" \
    ESTELLE_TEST_OS="$MUTANT_OS" ESTELLE_TEST_ARCH="$MUTANT_ARCH" \
    ESTELLE_INSTALL_DIR="$MUTANT_INSTALL" ESTELLE_VERSION='../attacker' \
    sh "$ROOT/install.sh" >/dev/null 2>&1; then
  echo "installer accepted a malformed release version" >&2
  exit 1
fi
[ ! -e "$MUTANT_INSTALL/estelle" ]

if PATH="$TEST_DIR/bin:$PATH" HOME="$TEST_DIR/home" ESTELLE_FIXTURE_DIR="$TEST_DIR/fixture" \
    ESTELLE_TEST_OS="$MUTANT_OS" ESTELLE_TEST_ARCH="$MUTANT_ARCH" \
    ESTELLE_INSTALL_DIR="$MUTANT_INSTALL" ESTELLE_RELEASE_REPOSITORY='../attacker' \
    sh "$ROOT/install.sh" >/dev/null 2>&1; then
  echo "installer accepted a malformed release repository" >&2
  exit 1
fi
[ ! -e "$MUTANT_INSTALL/estelle" ]

printf 'corruption' >> "$TEST_DIR/fixture/$ARCHIVE"
if PATH="$TEST_DIR/bin:$PATH" HOME="$TEST_DIR/home" ESTELLE_FIXTURE_DIR="$TEST_DIR/fixture" \
    ESTELLE_TEST_OS="$MUTANT_OS" ESTELLE_TEST_ARCH="$MUTANT_ARCH" \
    ESTELLE_INSTALL_DIR="$MUTANT_INSTALL" sh "$ROOT/install.sh" >/dev/null 2>&1; then
  echo "installer accepted an archive whose checksum did not match" >&2
  exit 1
fi
[ ! -e "$MUTANT_INSTALL/estelle" ]

printf '#!/bin/sh\nprintf "fixture-estelle\\n"\n' > "$TEST_DIR/fixture/estelle"
printf 'unexpected payload\n' > "$TEST_DIR/fixture/extra"
tar -czf "$TEST_DIR/fixture/$ARCHIVE" -C "$TEST_DIR/fixture" estelle extra
HASH=$(sha256_file "$TEST_DIR/fixture/$ARCHIVE")
printf '%s  %s\n' "$HASH" "$ARCHIVE" > "$TEST_DIR/fixture/SHA256SUMS"
if PATH="$TEST_DIR/bin:$PATH" HOME="$TEST_DIR/home" ESTELLE_FIXTURE_DIR="$TEST_DIR/fixture" \
    ESTELLE_TEST_OS="$MUTANT_OS" ESTELLE_TEST_ARCH="$MUTANT_ARCH" \
    ESTELLE_INSTALL_DIR="$MUTANT_INSTALL" sh "$ROOT/install.sh" >/dev/null 2>&1; then
  echo "installer accepted an archive with an unexpected extra member" >&2
  exit 1
fi
[ ! -e "$MUTANT_INSTALL/estelle" ]
echo "installer proof: target selection installed; repository, version, checksum, and member mutants refused"
