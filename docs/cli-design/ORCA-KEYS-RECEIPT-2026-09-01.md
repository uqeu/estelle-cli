# Orca terminal-input and scrollback port — receipt, 2026-09-01

Branch `coach/orca-keys-20260901`, based on `origin/coach/r11-cli-integration` @
`f6fd5cc1bde9cc9fadee0608770dd64ec7ab0a15` — read back with `git ls-remote origin
coach/r11-cli-integration`, not taken from the brief.

Source of truth for the port is `vendor-reference/orca` (MIT), read directly. Where this receipt
and Orca's source disagree, Orca's source is right and this receipt is the defect.

---

## 1. What I own

Five new files, plus one declaration line each in `lib.rs`. **No lane-owned file was edited**
(`live_renderer.rs`, `main.rs`, `session_view.rs`, `bottom_pane/**` are all untouched — see §7).

| File | Lines | What it is |
|---|---|---|
| `tui/src/terminal_shortcut_policy.rs` | 701 | keystroke arbitration: policy toggle, KKP gate, control-byte table |
| `tui/src/terminal_scrollback_snapshots.rs` | 496 | bounded, UTF-8-safe, atomic, traversal-proof snapshot store |
| `tui/src/agent_prompt_injection.rs` | 365 | size-aware submit delay, bracketed-paste framing, ESC sanitising, chunking |
| `tui/src/terminal_write_gate.rs` | 347 | write admission, and the fence re-asserted before the delayed Enter |
| `tui/src/terminal_scrollback_limits.rs` | 119 | store/replay bounds; backlog cap derived from the scrollback setting |

Registration — `tui/src/lib.rs`, +5 lines, 0 deletions:

```
lib.rs:91   mod agent_prompt_injection;
lib.rs:178  mod terminal_scrollback_limits;
lib.rs:179  mod terminal_scrollback_snapshots;
lib.rs:180  mod terminal_shortcut_policy;
lib.rs:183  mod terminal_write_gate;
```

`lib.rs` is not on the lane-owned list and was unmodified on the base branch, so five added `mod`
lines cannot collide with in-flight work in the files that lane holds.

---

## 2. The five states, answered one at a time

Per module, because one verdict spanning five modules is the partial-guard-reporting-complete
shape.

| | built | wired | tested | shipped-in-preview | probed |
|---|---|---|---|---|---|
| `terminal_shortcut_policy` | yes | **NO** | yes | no | no |
| `agent_prompt_injection` | yes | **NO** | yes | no | no |
| `terminal_write_gate` | yes | **NO** | yes | no | no |
| `terminal_scrollback_limits` | yes | **NO** | yes | no | no |
| `terminal_scrollback_snapshots` | yes | **NO** | yes | no | no |

- **built** — compiles in `estelle-tui` at the SHA above; both crate-wide source guards pass (§6).
- **wired — NO, for all five, and it is the headline limitation.** Nothing calls any of it. Every
  natural call site is lane-owned, and more fundamentally the surface these mechanisms arbitrate
  does not exist in this CLI yet (§7.1). **No user-visible behaviour has changed.**
- **tested** — 56 unit tests, all in-process. No PTY, no terminal, no clock.
- **shipped-in-preview — no.** Not released, not published, not in a binary anyone has run.
- **probed — no, and it cannot be otherwise while `wired` is NO.** There is no running process in
  which to observe any of this. No live-probe claim appears anywhere in this receipt.

---

## 3. Red first

Every test was run against a deliberately wrong implementation before the real one. Five mutants,
each the naive implementation a developer writes if they port the *shape* of Orca's code without
the lessons in it.

| # | Mutant | File |
|---|---|---|
| M1 | the KKP gate does not exist: `emits_native_sequences` always false | `terminal_shortcut_policy.rs` |
| M2 | the submit delay is capped at 2 s | `agent_prompt_injection.rs` |
| M3 | truncation is a byte slice: `&text[text.len() - max_bytes..]` | `terminal_scrollback_snapshots.rs` |
| M4 | the ref is validated when minted, so `snapshot_path` does not revalidate | `terminal_scrollback_snapshots.rs` |
| M5 | the submit trusts the admission taken before the pause | `terminal_write_gate.rs` |

**Result: 16 of 56 red.** Per module: `terminal_shortcut_policy` 12 passed / 5 failed ·
`agent_prompt_injection` 9 / 3 · `terminal_scrollback_*` 13 / 4 · `terminal_write_gate` 6 / 4.

The failures, quoted from the run:

```
---- terminal_shortcut_policy::tests::alt_arrow_defers_under_kkp_and_injects_word_nav_without_it
panicked at tui/src/terminal_shortcut_policy.rs:389:9:
assertion `left == right` failed
  left: Act(SendInput("\u{1b}b"))
```

That left-hand value **is the bug the KKP gate exists to prevent**, reproduced: with the gate
removed, alt+Left under a negotiated protocol injects `\eb`, which arrives at the child as
`alt+b`.

```
---- terminal_scrollback_snapshots::tests::truncation_keeps_the_tail_and_never_splits_a_character
panicked at tui/src/terminal_scrollback_snapshots.rs:119:10:
start byte index 2900 is not a char boundary; it is inside '中' (bytes 2898..2901)

---- terminal_scrollback_snapshots::tests::a_hostile_ref_that_somehow_reached_a_path_call_is_still_refused
assertion `left == right` failed
  left: Some("/snapshots/v1-../../../etc/passwd.bin")

---- terminal_write_gate::tests::a_rebind_during_the_pause_refuses_the_submit
assertion `left == right` failed
  left: Ok("\r")
```

The traversal mutant produced a literal `/etc/passwd` escape; the fence mutant let the Enter go out
on a stale admission.

**Honest accounting of one failure.** M2 killed **two** tests, not three.
`the_windows_host_is_slower_than_the_posix_one_at_every_size` also failed, but for its own reason:
it called `paste_ingest_delay` directly, which M2 does not touch, and it asserted a claim that is
simply false — at 1 byte both hosts round up to the same whole millisecond. **That was my bug, not
the mutant's kill**, it survived into the first green run, and it is fixed by asserting the true
invariant (ConPTY is never given the *shorter* wait, and is *strictly* longer above one Posix
millisecond, so a swap of the two rates still fails the test). Counting it as a mutant kill would
have overstated the mutants by one.

**Negative controls held.** 40 of 56 tests stayed green under the mutants — the ones that should
not move did not, so the 16 reds locate the defect rather than merely reporting breakage.

---

## 4. Green, with real before/after counts

Full suite, `cargo test -p estelle-tui --lib`:

| | tests | passing | failing | ignored |
|---|---|---|---|---|
| base (my 5 `mod` lines commented out) | 3,232 | 3,225 | **6** | 1 |
| with this branch | 3,288 | 3,281 | **6** | 1 |
| delta | **+56** | +56 | **0** | 0 |

Per module, all green: `terminal_shortcut_policy` 17/17 · `terminal_scrollback_*` 17/17 ·
`agent_prompt_injection` 12/12 · `terminal_write_gate` 10/10.

**The 6 failures are pre-existing and are not mine.** That is measured, not assumed: I commented
out all five `mod` declarations so none of my code was compiled, and re-ran exactly those tests.
They fail identically:

```
chatwidget::tests::plan_mode::collab_mode_applies_default_preset
chatwidget::tests::plan_mode::collab_mode_is_sent_after_enabling
chatwidget::tests::plan_mode::enter_submits_when_plan_stream_is_not_active
chatwidget::tests::plan_mode::user_turn_includes_personality_from_config
bottom_pane::command_popup::tests::default_command_popup_items_snapshot
chatwidget::tests::popups_and_settings::personality_selection_popup_snapshot
```

Four are `plan_mode` expectation drift (a `UserTurn` carrying `personality: None`); two are insta
snapshot drift in `bottom_pane` and `chatwidget`. All six are in files the other lane owns or is
actively editing, and both snapshot names correspond to `.snap.new` files already present in that
lane's working tree. **I did not touch them and I did not update anyone's snapshots.**

---

## 5. The KKP gate, both states

The gate is asserted in **both** states at every site, because a single-state test passes just as
green against an implementation that ignores the gate entirely.

| chord | KKP negotiated | not negotiated | different? |
|---|---|---|---|
| alt+Left | `DeferToNativeEncoding` | `SendInput("\x1bb")` | yes |
| alt+Right | `DeferToNativeEncoding` | `SendInput("\x1bf")` | yes |
| alt+Backspace | `DeferToNativeEncoding` | `SendInput("\x1b\x7f")` | yes |
| shift+Enter | `SendInput("\x1b[13;2u")` | `SendInput("\x1b\r")` | yes |
| ctrl+Enter, local ConPTY | `SendInput("\x1b[13;5u")` | `SendInput("\r")` | yes |
| ctrl+Enter, elsewhere | `SendInput("\x1b[13;5u")` | `SendInput("\x1b[13;5u")` | **no — deliberate** |

The last row is stated rather than hidden. Away from a local ConPTY, Orca does *not* branch on KKP
for ctrl+Enter, because a query-only TUI binds CSI-u without ever negotiating. A test asserting a
difference there would be asserting a bug, so
`ctrl_enter_off_conpty_is_csi_u_in_both_kkp_states_and_that_is_deliberate` asserts the *sameness*
and says why.

### Two divergences from Orca, both deliberate

**KKP state is three-valued, not a number that defaults to zero.** Orca's policy reads
`getKittyKeyboardFlagsActivePane?.() ?? 0`, collapsing "proven inactive" and "never heard" into the
same `0` — while its own boundary parser (`terminal-kitty-keyboard-flags.ts:21`) goes out of its
way to keep them apart, because "laundering it into known zero would make Preview commit raw text
against a bit-3 TUI". Our `KittyKeyboard` is `Unknown | NotNegotiated | Negotiated(flags)`. This is
not a rewrite for its own sake: `tui/src/terminal_probe.rs:51` already produces
`keyboard_enhancement_supported: Option<bool>`, so all three states survive to this decision
anyway. `Unknown` resolves to "no native encoding" — the same effective answer as Orca — but it
resolves **once, in a named method, with the cost of the choice written down**, instead of at five
call sites via a `??`.

**Declining and not-recognising are different answers.** Orca returns `null` for both "no rule
matched" and "a rule matched and is standing down so the native encoding wins". Our
`TerminalKeyDisposition` has `Unclaimed` and `DeferToNativeEncoding` as separate variants. This is
what makes the KKP rows above assertable at all: the test names the decline, rather than inferring
it from an absence that an unimplemented gate would produce identically.

---

## 6. Guards

Both crate-wide source guards pass with the five new files in the tree:

- `box_glyphs::source_guard::nothing_this_crate_ships_puts_a_box_corner_in_a_string` — **pass**.
  Zero corner glyphs in the new files; verified independently by grep before the run. No exemption
  was added, no exemption list exists, and the guard's `MIN_FILES_SCANNED = 200` vacuity floor is
  unaffected (the walk gained 5 files).
- The brand-palette guard — **pass**. The new modules contain no `Color::` at all, so their budget
  is 0 and stays 0.

---

## 7. Limits — stated in the body, because a hostile reader will find them anyway

### 7.1 The surface these mechanisms arbitrate does not exist in this CLI

This is the most important thing in the receipt, and it survived the brief unexamined.

`terminal_shortcut_policy` decides **who wins a keystroke: the app, or the child process on a
PTY**. `terminal_write_gate` decides **whether bytes may enter that PTY**. Both presuppose an
embedded terminal pane hosting a child process.

**The Estelle TUI has no such surface.** Measured: `codex-utils-pty` is a dependency of `core`,
`tools`, `exec-server`, `app-server`, `sandboxing` and `rmcp-client` — and **not of `tui`**
(`grep -n pty tui/Cargo.toml` returns nothing). The one PTY in `tui/` is
`tui/tests/suite/focus_palette.rs:79`, which `openpty`s to run *the TUI itself as the child* in a
test harness. `tui/src/exec_command.rs` is command-string formatting, not a PTY host.

So: this is **correct, tested, unconsumed infrastructure for a feature that has not been built**.
It is not one patch away from working. If the terminal-pane surface is never built, three of these
five modules are dead code, and when it *is* built they probably belong in a non-`tui` crate next
to `codex-utils-pty` rather than in the TUI. I put them in `tui` because that is where the brief
scoped the lane; that placement is a guess and should be revisited by whoever builds the consumer.

The two scrollback modules are different — they have an immediately applicable consumer (§7.2) —
but they are equally unwired today.

### 7.2 `MAX_TRANSCRIPT_ENTRIES`: the answer is "not from a user setting, and the unit is wrong"

The brief asked whether our flat `MAX_TRANSCRIPT_ENTRIES = 300` should be derived the way Orca
derives its backlog cap. Two separate answers.

**Should it be derived from a user scrollback setting? No.** Orca's cap bounds *memory* while a
starved display catches up, so scaling it with the user's own retention setting is coherent: the
user is choosing how much of their own RAM to spend on their own history. Ours bounds a *render
cost* against a frame deadline (`main.rs:1088-1105`: ~2.9 µs per line, linear; ~47 ms at 20,000
lines in release against a 50 ms budget). A user preference must not be allowed to raise a bound
whose binding constraint is a deadline the user never consented to missing. Deriving it from a
setting would let someone configure their own UI into dropped frames.

**But the cap is measured in the wrong unit, and that is a live defect.** Its own doc comment
states the cost model per **line**. The constant counts **entries**. Lines-per-entry is unbounded:

- `TranscriptEntry::Tool { lines: Vec<String> }` and `Command { lines }` (`transcript.rs:32-52`)
- rendered as `lines.join("\n")` (`transcript.rs:357-363`) — one entry becomes N lines
- and `Tool` is built **directly from local shell output** at `main.rs:4585`, from `Ok(lines)`

So one `cargo build`, `git log`, or `find /` through the shell tool is **one entry** that can carry
tens of thousands of lines. `trim_transcript` (`main.rs:2853`) sees 1 of 300 and does nothing,
while the frame budget is already blown. **The guard reads green over exactly the case it exists to
prevent.**

Its guarding test cannot see this, and that is the second half of the defect:
`transcript_of_size(lines)` (`main.rs:9998-10012`) builds entries at a **fixed ~3 lines per
entry**, so under that fixture 300 entries really is ~900 lines. The fixture models a shape
production does not have. This is the "a double friendlier than production certifies code
production rejects" family, and Orca's derived cap is what surfaced it: Orca bounds **chars**, a
proxy for the real cost, and we bound a container count that is not.

**Recommended, and NOT applied** (`MAX_TRANSCRIPT_ENTRIES` and `trim_transcript` are both in
`main.rs`, which this lane does not own). Keep the entry cap, add a line budget beside it, and
evict on whichever binds first:

```rust
// main.rs, beside MAX_TRANSCRIPT_ENTRIES at :1105
/// A LINE budget, because the measured cost is per line and an entry is not a line.
///
/// `Tool`/`Command` entries carry `Vec<String>` straight from shell output (`main.rs:4585`), so
/// one entry can be 20,000 lines and the entry cap will never see it. ~47ms at 20,000 lines in
/// release against a 50ms budget; 6,000 leaves headroom on a loaded machine.
const MAX_TRANSCRIPT_LINES: usize = 6_000;

// main.rs, in trim_transcript at :2853 — drop from the front until BOTH bounds hold, keeping the
// existing "N earlier entries were dropped" notice, which must stay: eviction still announces
// itself.
```

and a fixture whose lines-per-entry ratio is **not** fixed, so the guard can see a single fat
entry. I did not write that patch into `main.rs`; I am reporting it, with the line numbers, for the
lane that owns the file.

### 7.3 Not ported, and named rather than silently dropped

`terminal-option-shortcut-policy.ts` (189 lines) — the macOS Option-as-Alt modes, dead-key
tracking, layout-character composition, and CSI-u encoding of option chords. It depends on
`layoutCharacterForCode`, a keyboard-layout lookup crossterm does not expose (crossterm resolves
the character for us before we see it). Porting it would require inventing that lookup, so it is
absent, deliberately, and the port is incomplete by exactly that much.

### 7.4 Numbers I did not measure

The ConPTY ingest table in `agent_prompt_injection.rs` (2,000 B → 14/25 ms … 320,000 B → 3342/2969
ms, and the 64 B/ms and 4,096 B/ms rates derived from it) is **Orca's measurement, carried over
with its hosts named — not mine.** I have not measured Estelle's transport. If our PTY layer has a
different slope, the constant is wrong and nothing in this branch would catch it. The tests assert
the delay *outlasts Orca's slowest observed host*, which is a check on internal consistency, not a
measurement of ours.

Likewise the macOS `cmd+*` rows only fire when the terminal reports `KeyModifiers::SUPER`, which
legacy terminals do not do for Cmd. **I did not measure which terminals report it**, so how much of
that table is reachable in practice is unknown.

### 7.5 Smaller ones

- **No test drives a real PTY, terminal, or clock.** The delay is asserted as a `Duration` value;
  nothing ever sleeps. That the caller actually waits it is unproven, and unprovable until wired.
- `read_snapshot` uses `from_utf8_lossy`. A snapshot holding invalid UTF-8 replays with
  replacement characters instead of failing — right for a display path, but the store is not
  byte-transparent, and a test asserts the seek does not *create* such a character rather than that
  none can exist.
- `write_snapshot` re-applies mode `0o700` to the snapshot directory on every write, which would
  clobber a mode a user deliberately set.
- `SnapshotRef::for_pane` truncates SHA-256 to 128 bits. That is a lookup key, not a security
  token, and 128 bits is not collidable in practice — recorded because it is a truncation and the
  full digest was available.
- The `TerminalFirst` scope split (which chords yield to the shell) is **my mapping**, not Orca's.
  Orca drives it off a 2,400-line keybinding registry with per-action `scope` and `allowInTerminal`
  fields; I reduced it to a five-variant enum with a hand-assigned `ChordScope`. The *mechanism* is
  faithful; the *membership* is a judgement call that a real keybinding registry should replace.

---

## 8. Constraints honoured

- No deploy, no publish, no push to `main` or `coach/r11-cli-integration`. Only
  `coach/orca-keys-20260901`.
- No lane-owned file edited: `git diff --name-only` lists only my five new files and `lib.rs`.
- No `git checkout -- <path>` anywhere. The mutant revert used `cp` from a backup taken before the
  mutation, verified by md5 against the pristine copies.
- No box corners, no bordered panel, no exemption added or weakened.
- No credential file read, grepped, printed or echoed. No `railway` command of any kind.
- One build at a time throughout; no `--release` build.
