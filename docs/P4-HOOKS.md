# P4 - canonical grounding hooks

P4 makes the Rust binary the canonical `PreToolUse` and `PostToolUse` hook. Codex's maintained `hooks`
crate remains the dispatcher; `estelle install-hooks` writes the same command declarations to Claude Code's
`~/.claude/settings.json` and Codex's `$CODEX_HOME/hooks.json`.

Only the P4 surface landed. The production pane, ACP and grafts remain unstarted.

## Register #66 - the local file reader exists

This is **CLI register #66**, named in the root `.gitignore`; it is unrelated to server register #66 about
ingest metering.

The P3 Rust CLI already read local files:

- `tui/src/top_level.rs:888` `sweep()` calls `collect_files()` and posts the contents to `/sync` at `:924`.
- `tui/src/top_level.rs:1049` `reindex()` computes changed paths, reads them at `:1088`, and posts them to
  `/reindex` at `:1105`.
- `estelle-client` is transport only. The reader is in `tui/src/top_level.rs:690`.

The old implementation had the four intended controls but one unsafe transition. It ran the exact Git
inventory, used an extension allowlist outside Git, rejected symlinks and canonical paths outside the repo,
and scanned content before upload. However, **any failed Git inventory fell back to `read_dir` even inside a
Git worktree**. An ignored allowlisted tree such as `testbed/*.js` could therefore enter the payload when
Git failed.

The fixed contract is:

1. A Git worktree must successfully run `git ls-files --cached --others --exclude-standard`; failure aborts
   collection rather than falling back.
2. Only a directory proven not to be a Git worktree uses the extension-allowlisted walker.
3. Explicitly named paths, including PostToolUse edits, are intersected with the same Git inventory.
4. Canonical containment and `symlink_metadata().is_file()` reject symlink escapes.
5. The shared credential scanner covers Estelle/OpenAI/GitHub, Stripe, AWS and private-key shapes.
6. Every ingest-related Git command uses `-z`; NUL splitting preserves legal filenames containing newlines.

### Red-before-green proof

The regression fixture initializes a Git repo containing `main.rs`, ignored `.env`, and ignored
`testbed/vendor.js`. `.env` proves the direct secret case; the allowlisted JavaScript file proves the
instrument is not vacuous.

With collection deliberately replaced by `walk_paths(root)`, the test failed:

```text
left:  ["main.rs", "testbed/vendor.js"]
right: ["main.rs"]
```

After restoring the exact Git inventory, both ignored paths are absent. The same test also names those files
explicitly and proves explicit `/reindex` and hook paths cannot bypass Git ignore rules.

A second filename fixture creates `odd\nname.rs`. With parsing deliberately changed back to line splitting,
the test failed at **0 files instead of 1**. NUL-delimited parsing restores the filename as one path.

## Hook contract

`estelle hook ground` reads the maintained hook stdin shape, posts scoped `/verify` with `{answer, repo}` and
renders the four states in fail-closed order: `unreachable`, `unverified`, `flagged`, `clean`. An abstention
is visible and is never rendered as a pass. A flagged result remains advisory because the server does not
yet attest index freshness; the CLI does not claim that a local "posted" stamp proves current server state.

`estelle hook sync` reads the completed file through the same repository collector and posts scoped
`/reindex`, never `/sync`. Ignored files, files outside the repo, symlinks, unsupported extensions and
credential-shaped content do not travel.

Rust parity tests execute `scripts/hooks/estelle_hook.py` for every fixture and compare its decision with the
Rust result. The covered shapes include empty error containers, refusal envelopes, abstention-before-finding
ordering, scalar findings, file-extension decisions and credential shapes.

The first sync-parity run failed red on `sk_live_...`: Python refused it while Rust returned clean. The shared
Rust scanner was expanded, after which Stripe, AWS and private-key fixtures all matched Python.

## Configuration safety

Installation parses before writing, refuses non-object or malformed JSON, preserves unrelated settings and
hooks, backs up an existing file, and writes atomically at mode `0600`. Uninstall removes only commands whose
mode is one of Estelle's known hook modes. The generated `$CODEX_HOME/hooks.json` is deserialized by
`codex_config::HooksFile` in the test, rather than trusted as compatible prose.

The preserving test contains customer model, permissions, env, PreToolUse, PostToolUse and Stop settings.
With the merge deliberately mutated to clear an event array, it failed because PreToolUse had **1 group
instead of 2**. The retaining merge restores the required 2 and uninstall reproduces the original JSON.

## Live customer-visible proof

These are the literal JSON frames emitted by the real Rust binary against production.

Fabricated symbol in swept `uqeu/estelle`:

```json
{"systemMessage":"Estelle FLAGGED p4_probe.py: not defined in this repo: GhostScopeP4","hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Estelle's grounding gate FLAGGED this edit to p4_probe.py: not defined in this repo: GhostScopeP4. The finding is advisory because the server does not yet attest index freshness."}}
```

Real symbol in the same repo:

```json
{"systemMessage":"Estelle PASSED p4_probe.py: grounded against uqeu/estelle."}
```

The same real symbol scoped to fresh, unswept `uqeu/estelle-p4-unswept-cKKEB6`:

```json
{"systemMessage":"Estelle ABSTAINED on p4_probe.py: this repo has not been swept — nothing to ground against","hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Estelle's grounding gate ABSTAINED on this edit to p4_probe.py: this repo has not been swept — nothing to ground against. This is not a pass; no symbol in this edit was certified."}}
```

Each process exited `0`: P4 is advisory by default, but the three states are not visually or structurally
interchangeable.

## Measurements and corrections

- `Cargo.lock` packages: **1,307**, unchanged.
- Workspace packages: **122**, unchanged.
- Direct TUI dependency exceptions from P0: **19**, unchanged. Hook work made no hollow crate deletable.
- The hypothesis that there was no Rust local reader was wrong: both sweep and reindex already read files.
- The existing reader's Git failure fallback was also wrong: it crossed from Git consent to a directory walk.
- The existing Rust credential scanner was weaker than the Python hook on three named shapes; parity caught it.

The server-lane prompt-injection finding remains register #33 and was not sanitized client-side. The server
meter defect already numbered #66 also remains server-lane work; this report does not relabel or close it.

## Validation

- `cargo test -p estelle-tui --bin estelle`: **50 passed**.
- `cargo test -p estelle-client`: **16 passed**, one explicit live test ignored.
- Python/Rust hook parity: passed for ground verdict and sync refusal fixtures.
- Generated hook declaration parsed by the maintained Codex hook schema.
- strict Clippy for `estelle-client --all-targets` and the Estelle TUI binary: passed with warnings denied.
- `cargo fmt --check -p estelle-client -p estelle-tui`: passed.
- `cargo build --release -p estelle-tui --bin estelle`: passed; optimized artifact is **23,860,200 bytes**,
  Mach-O arm64. The preserved `codex-app-server` emits one existing `unused_mut` warning outside the two
  strict packages.
- The exact optimized artifact invoked production and rendered the real-symbol `PASSED` frame.
