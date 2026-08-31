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

These are the literal JSON frames emitted by the real Rust binary against production. The four grounding
lines and the two transport lines below were **re-measured on 2026-08-31** after the wording was rewritten
(see "The line the founder reads" below); the earlier `Estelle FLAGGED` / `Estelle PASSED` /
`Estelle ABSTAINED` / `Estelle UNREACHABLE` spellings are history, not current output.

Fabricated symbol in swept `uqeu/estelle`:

```json
{"systemMessage":"Estelle flagged p4_probe.py: not defined in this repo: GhostScopeP4. Edit not blocked.","hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Estelle's grounding gate flagged this edit to p4_probe.py: not defined in this repo: GhostScopeP4. NOT BLOCKED, and the reason is freshness rather than doubt about the finding: the server does not yet attest that the index is current for this file, so a flagged symbol may be one it has not seen yet. Treat it as unverified, not as absent."}}
```

Real symbol in the same repo:

```json
{"systemMessage":"Estelle checked p4_probe.py: grounded against uqeu/estelle."}
```

Real symbol called with the wrong arity — the same run, so the gate is doing arity as well as existence:

```json
{"systemMessage":"Estelle flagged p4_probe.py: signature mismatch: resolve_grounding_scope() missing required positional argument(s): expected at least 3, got 0. Edit not blocked."}
```

An abstention (a repo with nothing to ground against) reads:

```json
{"systemMessage":"Estelle could not verify p4_probe.py: this repo has not been swept — nothing to ground against. Edit not blocked.","hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Estelle's grounding gate ABSTAINED on this edit to p4_probe.py: this repo has not been swept — nothing to ground against. This is NOT a pass - no symbol in this edit was checked, and the edit was ALLOWED to proceed anyway. Do not treat any API used here as confirmed to exist."}}
```

Each process exited `0`: P4 is advisory by default, but the states are not visually or structurally
interchangeable.

## The line the founder reads

🔴 **"UNREACHABLE" WAS ONE WORD FOR FOUR OPPOSITE FACTS, AND IT NAMED THE WRONG ONE.** A deadline the
client chose, a refused connection, a name that does not resolve, a missing credential and a server that
ANSWERED with a status all printed `Estelle UNREACHABLE - {file} was NOT grounded: {error}`. Measured
2026-08-31: production answered `/health` 200 in 0.303s / 0.305s / 0.299s while the hook called it
unreachable, and the founder read it as an outage and lost the afternoon. Three of those five facts are
not an outage at all, and they send a reader to different systems.

The two transport frames, both taken live from this binary against `api.fatelabs.ca` on 2026-08-31:

```json
{"systemMessage":"Estelle did not check billing.py: answered and declined (http 401) — the server is reachable. Edit not blocked."}
{"systemMessage":"Estelle did not check billing.py: has no usable credential on this machine (run estelle login, or estelle doctor to see why). Edit not blocked."}
```

What 0.2.31 printed for those same two runs, same host, same second:

```json
{"systemMessage":"Estelle UNREACHABLE - billing.py was NOT grounded: Estelle returned HTTP 401 Unauthorized: unknown or missing api key — the credential was rejected on verify and no stored credential was removed; a single rejection can be route scope, not a bad key. If you passed --key, check that key; otherwise run estelle login only if you revoked it."}
{"systemMessage":"Estelle UNREACHABLE - billing.py was NOT grounded: no Estelle credential is configured"}
```

Three defects in one line, all fixed: the **subject** was wrong (a 401 is a server that answered), the
**subject was stated three times** ("UNREACHABLE", "was NOT grounded", and again inside the error), and
the **raw error text was interpolated** — `reqwest`'s own `Display` is `error sending request for url
(https://…)`, so a transport failure put the endpoint and anything in its query string into the
customer's terminal and their on-disk transcript.

`classify_transport_failure` names timeout / refused / dns / http NNN / bad-response / cancelled, and
returns **none of the error's own text**. The DNS and refused branches are separated by a measured fact
rather than by matching prose: a refused connection ends its source chain in an `io::Error` of kind
`ConnectionRefused` carrying `errno 61`, while `getaddrinfo` reports through `gai_strerror` and sets no
`errno` at all. A resolver that did set one would fall through to "could not be reached", which
understates the failure rather than misnaming it.

⚠️ **LIMIT.** The `timeout` branch is very hard to reach from the plugin: the client deadline is
`estelle_client::DEFAULT_TIMEOUT` (300s) while `estelle-plugin/hooks/hooks.json` gives `hook ground` a
**15s** host budget, and the measured `/gate` latency on 2026-08-31 was **16.6s**. The host kills the hook
first, and no message of ours is printed at all. Raising that host budget is a separate change and is not
made here.

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
