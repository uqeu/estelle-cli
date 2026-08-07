# P1 transplant record

## Result

The preserved Codex composer now submits typed questions to Estelle's production `/deep-search`
endpoint and renders the typed response in the Ratatui loop. Header requests begin only after the
first frame, run independently, and replace dim placeholders as they complete. Requests run outside
the render loop, show elapsed time, support cancellation with Esc, and have a 300-second default
timeout (with a hard minimum of 120 seconds).

The production proof used repository `uqeu/estelle` and the question:

> Which file defines the Rust CLI port specification? Answer with the path only.

The UI remained responsive and displayed elapsed seconds. Production returned a grounded response
after about 33 seconds, with sources, but its answer was incorrect:
`docs/adr/0015-cli-stays-javascript.md` rather than `docs/CLI-RUST-PORT-SPEC.md`. This proves the P1
transport and render path, but it also exposes a server-side grounding/index freshness defect. A
non-empty HTTP 200 response is not recorded as a correct answer.

## Client boundary

`estelle-client` owns HTTP, authentication, cancellation, repository scope, and the typed response
models consumed by the TUI. It has no terminal dependency or output path.

- Production base URL: `https://api.fatelabs.ca/`
- API inventory: 47 registered HTTP paths represented by typed endpoint metadata
- Repository resolution: `--repo` first, then git origin, then working-directory name
- Scoped calls: repository scope is inserted by one client boundary, never by callers
- Credentials: environment first, then `~/.estelle/auth.json`; stored files are mode `0600`
- Rejected stored credentials: deleted on HTTP 401; environment credentials are never deleted
- Foreign proxy bodies: replaced with a stable client-owned message

## Measurements that changed the design

- `/account`: 2.6 seconds in the brief's earlier production measurement
- `/search`: 5.6 seconds warm, 10.6 seconds cold in that measurement
- `/deep-search`: 11.8 seconds in that measurement; 14.12 seconds in the direct P1 contract proof;
  about 33 seconds in the complete TUI proof
- `/v1/chat/completions`: timed out after 182.9 seconds with a 180-second client timeout; a second
  request returned HTTP 200 after 219.98 seconds but its first answer was empty

The production evidence invalidates 150 seconds as a generally safe tail. P1 uses 300 seconds while
retaining the specified 120-second lower bound.

## Fail-before-green evidence

The client invariant mutation simultaneously broke timeout validation, secret redaction, callback
routing, repository override precedence, chat repository headers, scoped JSON bodies, cancellation,
and proxy-error sanitization. Eight of eight affected tests failed. Separate credential mutations
made files mode `0644`, disabled masking, and disabled stored-key deletion; both security tests failed.

The TUI mutations removed the three-part failure shape, repository matching, and stale-request ID
guard. Their focused tests failed before each implementation was restored. The terminal-writer lint
was also proven red in P0 and remains enforced.

## Spec corrections

1. The endpoint section says 50 endpoints, but lists 49 distinct paths. `/help` and `/c` are local
   aliases and are not server routes, leaving 47 represented HTTP paths.
2. The server registers `/github/app/callback`; the spec lists `/github/callback`.
3. The measured `/v1/chat/completions` behavior does not satisfy the assumption that a non-empty,
   bounded response is available for the first end-to-end question. The shipped JS session path uses
   `/deep-search`, which is the working P1 transport.
4. The six direct dependencies do not become deletable merely by swapping the binary's transport.
   The preserved `codex_tui` library still compiles modules that consume all six, and the actual
   composer directly imports app-server protocol skill types.

## Post-transplant amputation count

The exception count remains **19**, unchanged from P0. Cargo metadata still contains every proposed
direct deletion:

| dependency | Rust source files that reference it |
|---|---:|
| `app-server-client` | 15 |
| `app-server-protocol` | 120 |
| `cloud-config` | 3 |
| `core-plugins` | 2 |
| `feedback` | 13 |
| `sandboxing` | 1 |

Deleting these crates now either makes the preserved TUI library uncompilable or requires extracting
the composer into a new, narrower crate. The latter is a separate surgery with a larger verification
surface; P1 does not disguise it with stubs or replace the real composer with a hand-written input.

The production pane, ACP, and grafts remain untouched as required.
