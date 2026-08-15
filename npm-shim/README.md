# @fatelabs/estelle

This package is a compatibility launcher for the native Estelle CLI. The previous JavaScript CLI is
retired. Installation downloads the matching macOS or Linux release from `uqeu/estelle-cli`, verifies it
against the release's SHA-256 manifest, rejects unexpected archive members, and only then installs it.

The primary clean-machine install command is:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

`npx @fatelabs/estelle` remains available only so existing installations move onto the same native binary.
