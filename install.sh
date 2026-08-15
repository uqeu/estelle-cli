#!/bin/sh
# Install the published Estelle Rust CLI after verifying its release checksum.
set -eu

REPOSITORY=${ESTELLE_RELEASE_REPOSITORY:-uqeu/estelle-cli}
VERSION=${ESTELLE_VERSION:-latest}
INSTALL_DIR=${ESTELLE_INSTALL_DIR:-"${HOME}/.local/bin"}

REPOSITORY_OWNER=${REPOSITORY%%/*}
REPOSITORY_NAME=${REPOSITORY#*/}
if ! printf '%s\n' "$REPOSITORY" | grep -Eq '^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$' \
    || [ "$REPOSITORY_OWNER" = . ] || [ "$REPOSITORY_OWNER" = .. ] \
    || [ "$REPOSITORY_NAME" = . ] || [ "$REPOSITORY_NAME" = .. ]; then
  echo "estelle installer: invalid release repository" >&2
  exit 2
fi
if [ "$VERSION" != latest ] && ! printf '%s\n' "$VERSION" \
    | grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$'; then
  echo "estelle installer: version must be latest or an exact vX.Y.Z tag" >&2
  exit 2
fi

case "$(uname -s)" in
  Darwin) OS=apple-darwin ;;
  Linux) OS=unknown-linux-gnu ;;
  *) echo "estelle installer: unsupported operating system: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64) ARCH=x86_64 ;;
  *) echo "estelle installer: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
ARCHIVE="estelle-${TARGET}.tar.gz"
if [ "$VERSION" = latest ]; then
  RELEASE_PATH=latest/download
else
  RELEASE_PATH="download/${VERSION}"
fi
BASE_URL="https://github.com/${REPOSITORY}/releases/${RELEASE_PATH}"

command -v curl >/dev/null 2>&1 || {
  echo "estelle installer: curl is required" >&2
  exit 1
}
TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/estelle-install.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT HUP INT TERM

curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  --output "$TMP_DIR/SHA256SUMS" "$BASE_URL/SHA256SUMS"
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  --output "$TMP_DIR/$ARCHIVE" "$BASE_URL/$ARCHIVE"

EXPECTED=$(awk -v archive="$ARCHIVE" '$2 == archive { print $1 }' "$TMP_DIR/SHA256SUMS")
printf '%s\n' "$EXPECTED" | grep -Eq '^[0-9a-f]{64}$' || {
  echo "estelle installer: release manifest has no valid checksum for $ARCHIVE" >&2
  exit 1
}
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum "$TMP_DIR/$ARCHIVE" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 "$TMP_DIR/$ARCHIVE" | awk '{ print $1 }')
else
  echo "estelle installer: sha256sum or shasum is required" >&2
  exit 1
fi
[ "$ACTUAL" = "$EXPECTED" ] || {
  echo "estelle installer: checksum mismatch for $ARCHIVE; nothing was installed" >&2
  exit 1
}

[ "$(tar -tzf "$TMP_DIR/$ARCHIVE")" = "estelle" ] || {
  echo "estelle installer: archive must contain exactly one estelle binary" >&2
  exit 1
}
mkdir "$TMP_DIR/unpack"
tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR/unpack"
[ -f "$TMP_DIR/unpack/estelle" ] && [ ! -L "$TMP_DIR/unpack/estelle" ] || {
  echo "estelle installer: archive did not contain a regular estelle binary" >&2
  exit 1
}

mkdir -p "$INSTALL_DIR"
DEST_TMP=$(mktemp "$INSTALL_DIR/.estelle.XXXXXX")
trap 'rm -rf "$TMP_DIR"; rm -f "$DEST_TMP"' EXIT HUP INT TERM
install -m 0755 "$TMP_DIR/unpack/estelle" "$DEST_TMP"
mv -f "$DEST_TMP" "$INSTALL_DIR/estelle"
trap 'rm -rf "$TMP_DIR"' EXIT HUP INT TERM

echo "Installed Estelle to $INSTALL_DIR/estelle"
case ":${PATH}:" in
  *:"$INSTALL_DIR":*) ;;
  *) echo "Add $INSTALL_DIR to PATH to run: estelle" ;;
esac
