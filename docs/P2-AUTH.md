> ## LOOP-REV: 14
> 00efbbf3 6 minutes ago  test(E-045): the supersession invariant is a GATE now — the signal existed and nothing read it

# P2 auth and security record

## Result

P2 accepts §§5.1, 5.2 and 5.7. `estelle login` reads a key without echoing it, asks the remote
`/account` route to verify it, and distinguishes verified, explicitly rejected, and unavailable. A 200
stores a verified key; 401/403/404 store nothing; a transport or 5xx failure stores the key but says it
could not be verified. A failure to ask is not evidence that the key is bad.

Superseded 2026-08-18: the default store is `~/.estelle/auth.json`, created atomically inside a `0700`
directory with mode `0600`. Reads fail closed when group or world permission bits are present. The previous
encrypted `~/.estelle/secrets/estelle_auth.age` store still depended on an encryption key in macOS Keychain;
ad-hoc release signatures changed identity on every build, so Keychain treated each update as a different
application and repeated its access prompt. The runtime no longer reads that Keychain-backed store. This is
an intentional non-migration: reading it once to migrate would recreate the prompt being removed. Existing
users can set `ESTELLE_API_KEY` for the immediate non-persistent path or run `estelle login` once to write the
new private file. Stable Developer ID signing and notarization remain the correct later distribution fix.

Credential detection is one length-bounded regex shared by input and masking. It finds embedded
`estelle_live_`, `sk-`, `ghp_`, and `github_pat_` values without matching short lookalikes. The old
prefix predicate both matched short lookalikes and missed the exact shipped exposure: a credential
embedded in pasted prose. Masking is the fixed text `[credential hidden]`; it leaks no suffix.

The real composer is inspected after each typed key and paste, before the next frame. A detected value is
replaced in the composer, never enters the request queue or transcript as raw text, and submitting the
placeholder does not call the server. Startup credential resolution runs after the first frame in a
blocking worker, so keyring access cannot delay the interactive composer.

## Fail-before-green evidence

| invariant | red proof | restored result |
|---|---|---|
| secure Estelle namespace | the test did not compile: `EstelleAuth` and the namespace-aware keyring service did not exist | encrypted round trip, `estelle` service, `estelle_auth.age` mode `0600` |
| embedded secret predicate | the old `starts_with` check left an embedded Estelle sentinel visible | embedded keys match; short lookalikes do not |
| pre-frame composer boundary | rendering before Enter contained the complete bogus sentinel | the rendered buffer contains only `[credential hidden]` |
| split-paste seam | two safe-looking paste chunks assembled a complete credential that reached the frame | the composed draft is inspected after every paste |
| all rejection doors | the policy test stopped red at HTTP 403 under 401-only deletion | 401/403/404 discard; 502 retains |
| login outcome policy | the test did not compile before the remote validation outcomes existed | 200 verified, 401 rejected, 502 stored-unverified |
| auth failure rendering | a deliberate return to 401-only made the 403 case render as an ordinary request failure | 401/403/404 use one safe re-auth screen |
| snapshot sensitivity | changing `working` to `waiting` produced a one-line snapshot diff | mutation rejected in `cargo insta review`; original baseline green |

The terminal-writer lint remains in force. The TUI binary contains no `println!`/`eprintln!` path after
startup; Ratatui's render loop is the sole terminal writer.

## Snapshot review

All five new baselines were read before acceptance.

| frame | what was checked |
|---|---|
| empty composer | dim unresolved header values, idle status, real Codex composer |
| composer with text | ordinary text visible and stable in the composer |
| slash menu open | `/mo` actually opens the `/model` row |
| long-running query | submitted turn remains visible and elapsed status reads `working  93s` |
| every failure screen | all seven failure classes appear with what happened, whose side, and next action |

The first red output exposed two broken instruments. The slash fixture sent rapid key events, which the
composer correctly held as a paste burst, so no menu opened. It now uses the paste event and visibly
opens `/model`. The failure fixture used 24 rows, so normal transcript scrolling removed its earliest
failure cases. It now renders at 80×52 and visibly contains all seven. Only after those corrections were
the five `.snap.new` files reviewed and accepted. A later deliberate status-label mutation was reviewed
and rejected rather than adopted.

### Empty composer

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories


















propose  |  server ...

› Compose new task

  ? for shortcuts                                            100% context left
```

### Composer with text

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories


















propose  |  server ...

› trace the charge path

                                                             100% context left
```

### Slash menu open

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories


















propose  |  server ...

› /mo

  /model  choose what model and reasoning effort to use
```

### Long-running query

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories

you  Which repair changed charge.ts?
















working  93s  |  Esc cancels

› Compose new task

  ? for shortcuts                                            100% context left
```

### Every failure screen

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories

estelle  failed
Estelle rejected the stored credential.
The API reported that this credential is not authorized.
Authenticate again, then retry the question.

estelle  failed
Estelle returned HTTP 502: the server returned a non-Estelle error body
The failure is on the Estelle service path.
Retry once; if it repeats, narrow the question and report the status.

estelle  failed
Estelle returned HTTP 400: repo is required
The API refused this request as sent.
Correct the request or account state, then retry.

estelle  failed
The Estelle request exceeded 300 seconds.
The server did not complete the grounded answer in time.
Retry or ask a narrower question.

estelle  failed
The Estelle request could not reach a response.
The network path failed before the server returned a result.
Check connectivity and retry.

estelle  failed
The request was cancelled.
The client stopped waiting before the server answered.
Submit the question again when ready.

estelle  failed
The Estelle request failed: the response body was empty
The client could not accept the server result.
Retry; if it repeats, report this exact failure.











propose  |  server ...

› Compose new task

  ? for shortcuts                                            100% context left
```

### Preserved approval prompt

The inherited Codex approval snapshot tests passed without replacement: 2 passed, 3,339 filtered out.

```text
  Would you like to run the following command?

  Environment: remote

  Reason: this is a test reason such as one that would be produced by the
  model

  $ echo hello world

› 1. Yes, proceed (y)
  2. No, and tell Codex what to do differently (esc)

  Press enter to confirm or esc to cancel
```

## Measurements and the wrong forecast

- Cargo.lock packages: **1,307**, down 39 from ADR 0016's accepted ancestor count of 1,346.
- Workspace packages: **122**.
- Release artifact: **22,737,304 bytes**, Mach-O arm64.
- P0/P1 amputation exceptions after P2: **19**, unchanged.

The spec's forecast that the six direct TUI dependencies would become deletable after the HTTP transplant
is still wrong. `codex-app-server-client`, `codex-app-server-protocol`, `codex-cloud-config`,
`codex-core-plugins`, `codex-feedback`, and `codex-sandboxing` remain direct `estelle-tui` dependencies.
The preserved `codex_tui` library still compiles their consumers; the Estelle binary deliberately uses
that library's real `ComposerInput`, slash menu, and approval surface. Removing the dependencies would
make the maintained terminal layer uncompilable or require a new narrow extraction that stops being the
fork P0/P1 accepted. P2 does not replace those consumers with hollow stubs to manufacture a lower count.

The foreign-harness session also reported the exact failure `SessionStart hook (failed) — hook exited
with code 1` three times. It is recorded in the root `docs/THE-LOOP.md` as a portability defect and was
not chased during P2.

## Validation

- `estelle-client`: 13 passed, 1 credentialed production contract ignored.
- Estelle TUI binary: 13 passed.
- Estelle secrets namespace: 8 passed.
- Preserved approval prompt: 2 passed.
- strict Clippy: client all targets and Estelle TUI binary passed with warnings denied.
- `cargo fmt --check`: passed.
- optimized release build: passed.

`cargo-watch` and `cargo-insta` are installed. The exact save/relaunch and snapshot-review commands are in
`docs/DEVELOPMENT.md`.

P3, the production pane, ACP, grafts, Working memory, and client-side model routing remain untouched.
