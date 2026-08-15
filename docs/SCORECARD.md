# Estelle CLI scorecard

**Status:** partial measurement. Rows record measurements, not feature inventory. A green local row does not
imply a public artifact, live-terminal behavior, or production reachability.

| measured question | method / corpus | n | control | measured result | does not cover |
|---|---|---:|---|---|---|
| Does the current Estelle-owned TUI command surface satisfy its local contracts? | `RUST_MIN_STACK=8388608 cargo test --locked -p estelle-tui --bin estelle`; 2026-08-15 local source checkout | 223 binary tests | Typed fixtures, temp Git repositories, wiremock seams, renderer snapshots, and named negative states | 223 passed, 0 failed | Live terminal, production, public binary, or the much larger inherited TUI-library population |
| Does the typed client satisfy its local wire/storage contracts? | `RUST_MIN_STACK=8388608 cargo test --locked -p estelle-client`; 2026-08-15 local source checkout | 32 tests declared | Real secure-store/temp-file behavior, typed envelopes, cancellation, redaction, and request-shape assertions | 31 passed; 1 live-production test explicitly ignored | Production auth, entitlement, endpoint availability, or customer reachability |
| Does the wider locked TUI package pass with the required stack bound? | `RUST_MIN_STACK=8388608 cargo test --locked -p estelle-tui`; 2026-08-15 local source checkout | 3,542 executed passing tests across library, binary, and integration targets | The same physical-chord test overflows the default 2 MiB libtest stack; the named 8 MiB bound is the workflow contract | 3,542 passed; 5 tests requiring legacy Codex/tmux conditions ignored | A clean public checkout until the standalone parity repair is exercised there; any ignored PTY behavior |
| Does the native release build identify itself as the tagged product? | `cargo build --locked --release --package estelle-tui --bin estelle` then `./target/release/estelle --version`; macOS arm64, 2026-08-15 | 1 native artifact | Exact expected output `estelle 0.2.4`; warning-denied target builds remain CI-owned | Build passed; exact version matched | The other three targets, signature/provenance, public download, or clean-machine install |
| Does the shell installer enforce the four-target checksum contract? | `scripts/test-installer.sh` and `scripts/test-release-package.sh`; 2026-08-15 | 4 target selections · 4 refusal mutants · 4 reproducible archives | Valid fixture installs; malformed repository, malformed version, corrupt checksum, and extra-member archive must install nothing | All target selections installed; all four mutants refused; four normalized archives reproduced byte-for-byte | TLS endpoint ownership, GitHub availability, actual release binaries, signatures, or customer machine state |
| Does the npm retirement shim install only a verified native artifact? | `npm test --prefix npm-shim`; packed tarball opened and enumerated, 2026-08-15 | 5 tests · 1 packed npm artifact | Four target mappings, HTTPS/GitHub redirect fence, exact checksum row, atomic install, checksum mutant; tarball must omit legacy modules | 5/5 passed; packed artifact contained only launcher, installer, package metadata, and README | npm trusted-publisher configuration, registry publication, GitHub release availability, or all archive-member mutants |
| Is fork provenance and the source-level egress register internally consistent? | `python3 scripts/check-fork-audit.py`; exact upstream object fetched in CI, 2026-08-15 | 1 upstream tree · 1 import tree · 3 reviewed high-risk blobs · 19 sink rows | Tree hashes, audited ancestry, exact risky path set/blob hashes, source-symbol presence, and primitive occurrence counts | PASS; 14 released and 5 latent sinks matched the declared denominator | Runtime process-tree traffic, DNS destinations, provider behavior, or customer consent quality |
| Can the public repository pass its release validation from a clean checkout? | GitHub Actions runs `31911232894` and `31912554854`, tag `v0.2.4`, Ubuntu clean runners, 2026-08-15 | 2 completed clean runs | Tag/version, installer, archive reproducibility, and fork provenance passed before locked Rust tests; release/build/npm jobs remained fail-closed | REFUSED: first run found the returning-brief dependency; second reached the binary suite, where 217 passed and 6 parity tests exposed the same parent-source assumption | The complete standalone-oracle repair recorded with this scorecard, platform builds, release creation, npm publication, or artifact readback |

## Publication gate

No row may claim a shipped Rust CLI until all of these are read back from public systems:

1. the remote tag resolves to the intended source commit;
2. four release archives and `SHA256SUMS` exist at customer URLs;
3. the downloaded archive hashes to its manifest row and contains exactly one regular binary;
4. that binary prints the tag version;
5. GitHub verifies its provenance attestation;
6. `npm view --prefer-online @fatelabs/estelle` reports the native compatibility shim, and `npm pack`
   shows no abandoned JavaScript implementation inside it.

The founder's clean-machine install is a separate proof after these six machine-readable facts hold.
