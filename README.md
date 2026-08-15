# Estelle CLI

Rust port of the Estelle CLI, forked from OpenAI Codex at commit
`582569998181aad08a88bacc151a94b2048a5d1f`.

The current tree includes the Estelle server transport and TUI. Public releases are tag-triggered from the
separate `uqeu/estelle-cli` repository and contain four checksummed binaries: macOS/Linux on arm64/x86_64.
The release workflow requests GitHub-signed build provenance for every release artifact.

Once the first Rust release is published, the clean-machine install command is:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

The installer detects the platform, downloads from the Estelle-owned release repository by default, verifies
the archive against `SHA256SUMS`, rejects archives with unexpected members, and atomically installs `estelle`
to `~/.local/bin`. `ESTELLE_RELEASE_REPOSITORY` is an explicit operator/test override that changes that trust
root; normal installs must leave it unset. Until a release containing `install.sh` exists, this is a release
contract—not a claim that the command is live.
