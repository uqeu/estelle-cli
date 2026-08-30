# Estelle CLI

Rust port of the Estelle CLI, forked from OpenAI Codex at commit
`582569998181aad08a88bacc151a94b2048a5d1f`.

The current tree includes the Estelle server transport and TUI. Public releases are tag-triggered from the
separate `uqeu/estelle-cli` repository and contain four checksummed binaries: macOS/Linux on arm64/x86_64.
The release workflow requests GitHub-signed build provenance for every release artifact.
The component boundaries, trust model, release path, measurements, provenance inventory, and named limits
live in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/SCORECARD.md`](docs/SCORECARD.md), and
[`docs/WHAT-WE-BUILT.md`](docs/WHAT-WE-BUILT.md).

Install the latest Rust release with:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

The installer detects the platform, downloads from the Estelle-owned release repository by default, verifies
the archive against `SHA256SUMS`, rejects archives with unexpected members, and atomically installs `estelle`
to `~/.local/bin`. `ESTELLE_RELEASE_REPOSITORY` is an explicit operator/test override that changes that trust
root; normal installs must leave it unset. Release `v0.2.4` and this public install path were read back and
executed successfully on macOS arm64; the exact distribution evidence and unproven product paths are recorded
in [`docs/SCORECARD.md`](docs/SCORECARD.md).

To bypass credential storage immediately, set `ESTELLE_API_KEY` in the process environment; it takes
precedence over `~/.estelle/auth.json` and is never persisted by Estelle.

Before you trust an agent harness with your keys, see what it has already spilled — fully offline, no
account, no network:

```console
$ estelle leaked
estelle leaked: 213 files scanned under ~/.claude and ~/.codex — no exposed credentials found.
```

`estelle leaked` scans your own `~/.claude` and `~/.codex` trees with the shared secret engine
(the pinned gitleaks rule set plus Estelle's extensions, entropy gates and upstream allowlists on, and a
base64 sweep for encoded blobs). Findings print as `path:line — rule (fingerprint)`; the value is never
printed. Exits non-zero when anything is found, so it gates CI too.
