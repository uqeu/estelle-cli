# Fork-audit triage — 92 undeclared high-risk diffs, 2026-09-03

`scripts/check-fork-audit.py` (run by the `validate` job of `.github/workflows/release.yml`) refused on
pristine `origin/main` at `3b16169ef`:

```
fork audit failed: high-risk delta is not exactly reviewed: 92 undeclared, 0 declared-but-unchanged
```

**These are 92 undeclared DIFFS, not 92 vulnerabilities.** The manifest tracks every file under
`high_risk_paths` that differs from `policy.audited_through_commit`
(`3ea4936a74e9345e3f1f8331bdb012a4688088bc`, "fix(acp): fail closed at session boundaries"). 325 commits
have landed since. A file appears here because *nobody wrote a row for it*, which is the audit doing its
job — the same refusal blocked `v0.2.32` twice before (see the `login/src/auth/manager.rs` and
`estelle-client/src/secret_engine.rs` rows). Every tag is blocked until each of the 92 carries a row.

## Method

Per file: `git diff <audited> -- <path>`, read in full, then a mechanical axis count over the **added
lines only**, reproducible by anyone:

```
python3 scripts/fork-audit-axes.py -v <path>...
```

Axes: new URL/host literals · network calls · process/exec · filesystem writes · environment reads ·
credential reads. The scan is **lexical and deliberately over-inclusive** — a `token` in a doc comment
and a `.get("data")` JSON lookup both fire. A **zero is a result**; a **non-zero is a reading
assignment**, and every non-zero below was read line by line before it was written into a row.

## Groups

| # | Shape | Files | Cost |
|---|-------|-------|------|
| A | **Copy pass** — human-readable string literals only: `Codex`→`Estelle` brand pass, second-person removal (`your browser` → `the browser`). No control flow. | 27 | cheap |
| B | **Glyph pass** — the tree prefix `"  └ "` → `"  │ "` in render code and its comments/fixtures. | 8 | cheap |
| C | **New render/format modules, all axes zero** — `design_book/*`, `demo_session*`, `graph_view`, `orchestra_view`, `session_view`, `marks`, `affinity_cli/*`, and their test files. Pure `Vec<Line>` producers. | 34 | medium |
| D | **Modified logic, all axes zero** — test fixtures, wrapping, usage accounting, onboarding copy+layout. | 12 | medium |
| E | **Non-zero on at least one axis** — read line by line, findings reported at the top of the audit report. | 11 | expensive |

Group E, with the raw lexical counts that put them there:

| File | Counts (lexical, pre-reading) |
|------|-------------------------------|
| `tui/src/ground_block.rs` | fs_write=16, env_read=2, cred_read=1 |
| `tui/src/gate_refusal.rs` | network=3 |
| `tui/src/mcp_tool.rs` | network=1 |
| `tui/src/design_book/script.rs` | network=1, cred_read=12, url_host=2 |
| `tui/src/demo_session.rs` | process=1 |
| `tui/src/design_book/mod.rs` | env_read=9, cred_read=6 |
| `tui/src/sweep_estimate.rs` | cred_read=10 |
| `tui/src/hook_guard.rs` | cred_read=6, url_host=1 |
| `tui/src/box_glyphs.rs` | url_host=1, env_read=1, cred_read=4 |
| `tui/src/test_gallery.rs` | fs_write=1 |
| `tui/src/style.rs`, `plain_english.rs`, `release_version.rs`, `test_stack_budget.rs`, `run_spend.rs`, `boot_scene.rs`, `design_book/{account,rail,skills,stats,surfaces,loops,script_*}.rs`, `bottom_pane/{approval_overlay,feedback_view}.rs`, `chatwidget/windows_sandbox_prompts.rs`, `onboarding/auth.rs`, `tooltips.rs`, `wrapping.rs` | 1–7 hits on one axis each |

## What this triage does NOT claim

It is a **routing decision**, not a verdict. A file in group A is cheap *to read*, not proven safe by its
group; each still gets its own row from its own diff. The axis scan cannot see semantics — it would score
a deliberate exfiltration written with an already-imported client as zero on `network` if the call reused
an existing helper name. Zeros are evidence, not proof.

## 🔴 The refusal message understates the blocker: it is 126 files, not 92

`verify_provenance` refuses on the **set comparison** (`risky != sorted(reviewed)`) *before* it
reaches the per-row `git hash-object` loop. So the 92-undeclared message is the FIRST refusal, not
the only one. Checking every declared row's blob directly, ahead of that gate:

```
python3 - <<'PY'   # (in the worktree)
import json, subprocess, os
m = json.load(open('fork-manifest.yaml'))
for r in m['reviewed_changes_after_audited_commit']:
    p = r['path']
    if r['blob'] == 'deleted' or not os.path.exists(p): continue
    a = subprocess.run(['git','hash-object',p], capture_output=True, text=True).stdout.strip()
    if a != r['blob']: print(p, a, r['blob'])
PY
```

**34 of the 87 existing rows have drifted blobs**, totalling **14,254 added / 2,348 removed lines**
against the hash each row attests. Added to the 92 undeclared files (~19,000 added lines), the tag
is blocked on **126 files**, not 92. The drifted set includes the four largest files in the fork:

| File | delta vs. its ATTESTED blob |
|------|------------------------------|
| `tui/src/main.rs` | +6,218 / −785 |
| `tui/src/top_level.rs` | +1,726 / −216 |
| `tui/src/live_renderer.rs` | +1,609 / −834 |
| `tui/src/screens.rs` | +1,283 / −144 |
| `tui/src/commands.rs` | +620 / −47 |
| `tui/src/theme.rs` | +510 / −12 |
| `tui/src/production_hud.rs` | +507 / −109 |
| (27 more, each under 300 added) | |

This is the audit working exactly as designed — a blob is the mechanism by which a row stops being
true when its file moves — but it means **no honest count of this work is 92**. Anyone planning
around the error message's number is planning around a third of the job.
