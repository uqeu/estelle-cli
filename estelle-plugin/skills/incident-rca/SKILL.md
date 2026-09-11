---
name: incident-rca
description: Triage a firing incident without guessing what to look at. Use when the task involves: incident, alert, pager, triage, sev, anomaly.
---

# incident-rca

Estelle's live-incident analyzer. This is not the retrospective (`postmortem`) and not local dev debugging
(`root-cause-loop` / `bug-hunt`) — it is the triage that runs *while the incident is burning*, auto-triggered by
an alert, to find the cause and ship the stop. It walks a typed investigation decision-tree (Meta's DrP,
arXiv 2512.04250: anomaly-detect → time-series correlate → dimension-isolate; grounded in the Google SRE
workbook's discipline), recalls similar past incidents from temporal `facts` and the Reflexion `ExperienceStore`,
isolates the fault to an exact symbol with the CodeGraph (`references`, `definition_sites`, `blast_radius`), and
emits a machine-readable root cause plus a mitigation PR that clears the deterministic merge gate. The retrospective
comes later; this produces the stop.

## When to run
The instant an alert fires — a SEV page, an SLO burn, an error-rate spike — while the timeline is still live and the
change window is still recoverable. Auto-triggered on the alert, not run by hand after the fact. When the incident is
mitigated, hand off to `postmortem` for the blameless retrospective; this skill's job is over once the stop is shipped.

## Procedure
1. **Ingest the alert and open a typed investigation.** Auto-trigger on the pager: capture the firing signal (metric,
   threshold, timestamp) as the root of a typed DrP-style decision tree, and record the incident as an OPEN temporal
   fact (`remember_fact` / POST /fact) so its state is queryable "as of now" while it burns — the incident is memory
   from minute one, not a Slack thread.
2. **Recall similar past incidents first.** Query temporal `facts` / `history` and `ExperienceStore.recall` for prior
   incidents on this signal or module — "this alert fired before; the cause was X" — so triage starts from the team's
   memory instead of a blank page. A repeat incident should be diagnosed in seconds.
3. **Anomaly-detect: bracket the change window.** Correlate the alert timestamp against `RunHistory.recent` — what
   shipped or which agent-run landed just before the signal broke. The prime suspect is the change whose timing
   brackets the anomaly; deploys are the single most common incident cause, so pin the window before hypothesizing.
4. **Time-series correlate across signals.** Walk the tree's correlate branch: which metrics moved *together* with the
   firing signal (latency + error rate + a saturated dependency), narrowing "something is wrong" to "these signals
   co-move." Confirm each branch with a log or a cheap probe before descending — an unconfirmed branch is a guess.
5. **Dimension-isolate to the code.** Map the failing signal to its module, then run the CodeGraph: `references` /
   `definition_sites` to land on the exact symbol, `blast_radius` to bound which downstream flows the fault reaches,
   and `central_files` / `betweenness_centrality` to check whether a chokepoint amplified it. This is where a metric
   dashboard stops and structural localization begins.
6. **Emit the machine-readable root cause.** Not prose — a typed record: `{signal, change_window, suspect_symbol,
   blast_radius, confirming_evidence}`. Each hypothesis and its confirm/refute result is written to `/facts` so the
   decision trail is auditable and supersedable, exactly as the DrP loop demands.
7. **Ship the gated mitigation PR.** The fastest safe stop — revert, flag-off, or a targeted patch — proposed as a
   gated change: `_gate_verdict` (grounding against the real symbol graph + `scan_diff` secret/SAST/CVE) must return
   `merge=true`, and `run_and_repair_suite` must leave the repo's real suite green. A mitigation that fails the gate
   is not shipped, even under pressure — the stop must not become the next incident.
8. **Close the incident into memory and hand off.** Record the resolution as a temporal fact (superseding the open one,
   not erasing it), distil the confirmed cause into a Reflexion lesson (`ExperienceStore.record` / `learn_from_gate`)
   keyed on the signal so the next firing recalls it, and hand the reconstructed timeline to `postmortem`. The live
   triage ends; the retrospective is a separate skill.

## Output
A live-incident record: the typed decision tree from alert → change window → correlated signals → isolated symbol, a
machine-readable root cause with confirming evidence, and a gate-clean, suite-green mitigation PR — the incident opened
and closed as temporal facts with a Reflexion lesson recorded. Distinct from `postmortem`: this runs while the incident
burns and produces the stop, not the retrospective document.
