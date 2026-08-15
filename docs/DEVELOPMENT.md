# Estelle Rust CLI development

Run commands from `cli-rs/`.

## Live rebuild loop

```bash
cargo watch -x 'run --bin estelle'
```

`cargo-watch` rebuilds and relaunches the real Ratatui binary after each saved Rust change.

## Render snapshots

```bash
cargo test -p estelle-tui --bin estelle snapshot_
cargo insta review
```

Read every rendered frame before accepting it. Reject a baseline when the fixture did not actually
produce the intended state.

## Focused validation

```bash
cargo test -p estelle-client
cargo test -p codex-secrets
cargo test -p estelle-tui --bin estelle
cargo clippy -p estelle-client --all-targets -- -D warnings
cargo clippy -p estelle-tui --bin estelle -- -D warnings
cargo build --release -p estelle-tui --bin estelle
```
