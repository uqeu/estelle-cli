# Estelle CLI acceptance walkthrough

Run commands from `cli-rs/` unless a step says otherwise. Use a terminal at least 120 columns wide for the
full production home and grounding side panel.

## 1. Prove the build

```sh
cargo fmt --check
cargo clippy -p estelle-client --all-targets -- -D warnings
cargo clippy -p estelle-tui --bin estelle -- -D warnings
cargo clippy -p estelle-acp -p estelle-mcp --all-targets -- -D warnings
cargo test -p estelle-client --lib
cargo test -p estelle-acp -p estelle-mcp
cargo test -p estelle-tui --bin estelle
cargo build --release -p estelle-tui --bin estelle
```

Expected denominators: 126 TUI tests, 21 passing client tests plus one ignored live-network test, two ACP
tests, and two MCP tests. The release binary is `target/release/estelle`.

## 2. Inspect the real renderer

Regenerate the fifteen frames rendered through the production `App` and `render_frame`:

```sh
ESTELLE_ACTUAL_GALLERY_DIR=docs/actual-gallery \
  cargo test -p estelle-tui --bin estelle actual_renderer_gallery_covers_the_product_surfaces
open tui/docs/actual-gallery/index.html
```

Review all fifteen frames: startup, honest empty state and dither, Orchestra active and completed,
production issue, proposed diff, slash palette, settings front door, ten-suite settings editor, model pool,
cream theme, autonomy, skills, and Todo expanded/collapsed. These are actual renderer evidence with typed
server payloads as test fixtures.

The separate composition studies are explicitly not product evidence:

```sh
open tui/docs/visual-gallery/index.html
```

## 3. Start as a real user

```sh
./target/release/estelle login --repo uqeu/estelle
./target/release/estelle --repo uqeu/estelle
```

Before entering a command, confirm that:

- the composer is immediately interactive;
- repository counts arrive asynchronously without moving the composer;
- the permanent home names App health, Agent health, What Estelle caught, What Estelle did, and GitHub;
- every absent section says why it is absent and what action is available;
- the cream/ghost dither stays behind content and the red wake appears only where earned.

## 4. Exercise the interaction layer

Perform these in order inside the TUI:

1. Type `/` and use Up/Down to move through the filtered slash palette. Press Tab to complete a row and
   Enter to run it.
2. Run `/settings`. Use Up/Down and Enter. Every row must either change a real client-owned setting or
   disclose its owner; it must not present an inert dial.
3. Run `/model`. Walk the server-provided BYOK pool. The screen is read-only until account-wide model
   mutation exists and must name the dashboard alternative instead of pretending to pin the session.
4. Run `/skills`. Confirm valid skill names survive filtering while credential-shaped text remains hidden.
5. Press `?` with an empty composer. Confirm the real keymap appears without submitting a request.
6. Press Shift-Tab twice. Plan mode must visibly toggle and must not invent a client-side routing policy.
7. Run `/context`, then press Alt+M. Both bindings must open and close the same persistent grounding panel.
8. Run `/todo`, then press Ctrl+T twice. The ledger must expand and collapse while completed results remain
   readable. With no server Todo payload, it must say that no snapshot was supplied.
9. Scroll with the mouse wheel. The transcript must move; composer history must not be recalled.
10. Press Esc during a live request to cancel it. The render loop must remain responsive.

## 5. Exercise grounded work

```text
/init
/memory
What defines the Estelle CLI production home? Cite file and line.
/verify
/work Add a small, reviewable documentation correction grounded in this repository.
/diff
/gate
```

Confirm that citations retain file and line, long requests show elapsed time, `/diff` is read-only, and
`/gate` never reports a clean result for something it did not measure. `/apply` is the only command that
writes the last proposed patch; do not run it during inspection unless that write is intentional.

## 6. Exercise production health

Run `/prod` or the top-level command below:

```sh
./target/release/estelle monitor --repo uqeu/estelle
```

Check that real issue rows preserve signal, bound symbol and file range, bind status, repair status, gate
verdict or `gate_absent_reason`, and PR state. Missing request denominators must say `error counts`, never
claim an error rate. An absent repair patch must remain absent rather than becoming an illustrative diff.

## 7. Exercise interoperability

ACP is a stdio protocol server, so use an ACP host and configure this command:

```sh
./target/release/estelle acp --repo uqeu/estelle
```

The accepted live ACP surface is protocol-v1 initialize plus `session/new`. Unsupported capabilities must
be advertised false. ACP does not resolve Estelle BYOK credentials.

To expose Estelle's production-advertised MCP tools to an MCP host (28 on Free; 37 on Ultra):

```sh
./target/release/estelle mcp-server --repo uqeu/estelle
```

To inspect another MCP stdio server from Estelle:

```sh
./target/release/estelle mcp --repo uqeu/estelle -- <server-command> <args...>
```

Every scoped MCP call must use the launch repository even when a foreign caller supplies another `repo`.

## 8. Confirm honest degradations

These client surfaces are implemented, but their complete live state cannot appear until the server adds
the contracts in `SERVER-CONTRACTS-NEEDED.md`:

- repair patch in the issue feed;
- revisioned Orchestra lifecycle and per-worker progress;
- Agent-health read path;
- account-wide PR feed;
- account-wide model mutation;
- ACP lifecycle beyond initialize and `session/new`;
- MCP capabilities beyond the catalogue exposed by the authenticated entitlement tier;
- live sandbox output streaming.

The acceptance condition is not that these look populated. It is that each one says exactly what is absent
and never synthesizes progress, success, a zero count, a model, a patch, or a stream.

## 9. Exit cleanly

Run `/exit` or press Ctrl-C. The alternate screen must restore and no background TUI process may remain.
