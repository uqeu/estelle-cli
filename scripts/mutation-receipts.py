#!/usr/bin/env python3
"""Mutation receipts for the CLI interaction pass (branch cli/ux-interaction-20260903).

Every guard written in that pass is paid for here: one source mutation, one named test, and
the run FAILS if the test still passes. A guard whose mutant survives is a guard that cannot
fire, and this repo has shipped four of those - see CLAUDE.md, "GREEN IS A CLAIM ABOUT WHAT
WAS MEASURED".

Usage:  python3 scripts/mutation-receipts.py [worktree] [--only <substring>]

The worktree defaults to the repository this file lives in. Nothing is left mutated: each
source file is restored in a `finally`, so an interrupted run does not leave a poisoned tree.
"""
import argparse
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent.parent

# (screen, description, file, needle, replacement, test that must go red)
MUTANTS = [
    # ── 40 · the walkable code graph ──────────────────────────────────────────
    (
        '40',
        'arrow keys stop moving the band',
        'tui/src/graph_walk.rs',
        '            KeyCode::Down => {\n                self.close_row();\n                self.cursor = (self.cursor + 1).min(self.matched.len().saturating_sub(1));\n            }',
        '            KeyCode::Down => {\n                self.close_row();\n            }',
        'graph_walk::tests::the_footer_advertises_exactly_the_keys_that_move_the_panel',
    ),
    (
        '40',
        'enter stops opening the row',
        'tui/src/graph_walk.rs',
        '            KeyCode::Enter => {\n                if self.opened {\n                    self.close_row();\n                } else {\n                    self.open();\n                }\n            }',
        '            KeyCode::Enter => {}',
        'graph_walk::tests::the_footer_advertises_exactly_the_keys_that_move_the_panel',
    ),
    (
        '40',
        'the filter line leaks its letters back into the walk keys',
        'tui/src/graph_walk.rs',
        '        if self.filtering {\n            return self.filter_key(key);\n        }',
        '        if false {\n            return self.filter_key(key);\n        }',
        'graph_walk::tests::the_filter_line_takes_letters_rather_than_firing_the_walk_keys',
    ),
    (
        '40',
        'a refusal is drawn as rows',
        'tui/src/graph_walk.rs',
        '        if let Some(reason) = &self.withheld {\n            return crate::graph_view::lines(',
        '        if let Some(reason) = &self.withheld.clone().filter(|_| false) {\n            return crate::graph_view::lines(',
        'graph_walk::tests::a_withheld_graph_draws_the_refusal_and_no_rows',
    ),
    (
        '40',
        'subsystem membership is exported as an edge',
        'tui/src/graph_walk.rs',
        '        for chain in &self.cycles {',
        '        for component in &self.subsystems {\n            for pair in component.windows(2) {\n                out.push_str(&format!(\n                    "  {} -> {} [source=\\"subsystems\\"];\\n",\n                    quote(&pair[0]),\n                    quote(&pair[1])\n                ));\n            }\n        }\n        for chain in &self.cycles {',
        'graph_walk::tests::the_dot_export_holds_only_measured_edges_and_labels_the_transitive_ones',
    ),
    (
        '40',
        'an unmeasured blast radius prints 0 instead of a dash',
        'tui/src/graph_view.rs',
        '        let moves = node\n            .moves\n            .map_or_else(|| "—".to_string(), |count| count.to_string());',
        '        let moves = node.moves.unwrap_or(0).to_string();',
        'graph_walk::tests::an_unasked_row_and_a_measured_empty_radius_draw_different_things',
    ),
    (
        '40',
        "the walk's keys fall through to the composer",
        'tui/src/main.rs',
        '            graph_walk::Action::Handled => return false,',
        '            graph_walk::Action::Handled => {}',
        'tests::the_walkable_graph_takes_its_keys_through_the_live_keymap_and_the_frame_moves',
    ),
    (
        '40',
        'a dead request opens a pane that reads as a server refusal',
        'tui/src/main.rs',
        '                    Err(error) => self.transcript.push(TranscriptEntry::System(format!(\n                        "The code graph could not be read: {error}"\n                    ))),',
        '                    Err(error) => {\n                        self.graph_walk = Some(graph_walk::Walk::new(graph_walk::Fetched {\n                            repo: "x".to_string(),\n                            withheld: Some(error),\n                            ..graph_walk::Fetched::default()\n                        }));\n                    }',
        'tests::a_withheld_graph_opens_the_pane_and_a_dead_request_does_not',
    ),
    (
        '40',
        "the footer advertises the walk's keys on the read-only production rail",
        'tui/src/production_hud.rs',
        '                hints: &[],',
        '                hints: crate::graph_walk::KEYS,',
        'production_hud::tests::the_production_rail_advertises_none_of_the_walks_keys',
    ),
    # ── 39 · tool calls ───────────────────────────────────────────────────────
    (
        '39',
        'expanding prints every line instead of the tail',
        'tui/src/public_widgets/history_transcript.rs',
        '                    let shown = lines.split_off(hidden);',
        '                    let shown = lines.split_off(0);',
        'public_widgets::history_transcript::tests::an_expanded_tool_call_shows_its_tail_and_counts_what_it_hid',
    ),
    (
        '39',
        'the hidden lines are dropped with no count',
        'tui/src/public_widgets/history_transcript.rs',
        '                    if hidden > 0 {',
        '                    if false {',
        'public_widgets::history_transcript::tests::an_expanded_tool_call_shows_its_tail_and_counts_what_it_hid',
    ),
    (
        '39',
        'a collapsed call draws its body',
        'tui/src/public_widgets/history_transcript.rs',
        '                if expanded {\n                    let total = lines.len();',
        '                if true {\n                    let total = lines.len();',
        'tool_output_stays_collapsed_until_its_exact_row_is_clicked',
    ),
    (
        '39',
        'ctrl+r opens every call rather than the newest',
        'tui/src/main.rs',
        "    if control_letter(&key, 'r') {\n        app.toggle_newest_tool();",
        "    if control_letter(&key, 'r') {\n        app.toggle_every_tool();",
        'tests::the_tool_call_chords_expand_one_expand_all_and_copy_all',
    ),
    (
        '39',
        'the copy hands over the visible tail rather than the whole output',
        'tui/src/main.rs',
        '        let body = lines\n            .iter()\n            .map(|line| mask_secret(line))',
        '        let body = lines\n            .iter()\n            .skip(18)\n            .map(|line| mask_secret(line))',
        'tests::the_tool_call_chords_expand_one_expand_all_and_copy_all',
    ),
    (
        '39',
        'the tool-call chord table takes a chord another surface already owns',
        'tui/src/transcript.rs',
        '    ("ctrl+e", "expands every call"),',
        '    ("ctrl+o", "expands every call"),',
        'tests::the_tool_call_screen_prints_the_keymaps_own_chords_and_no_others',
    ),
    (
        '39',
        'a tool chord with nothing to act on goes silent',
        'tui/src/main.rs',
        '            _ => self.transcript.push(TranscriptEntry::System(\n                "No tool call in this session yet. ctrl+r opens the newest one when there is."\n                    .to_string(),\n            )),',
        '            _ => {}',
        'tests::a_tool_chord_with_no_tool_call_answers_rather_than_no_opping',
    ),
    # ── 30 · memory remaining ─────────────────────────────────────────────────
    (
        '30',
        'the refusal does not lead with a verdict',
        'tui/src/main.rs',
        'if estimate.get("fits") == Some(&Value::Bool(false)) {\n                        lines.push(',
        'if false {\n                        lines.push(',
        'tests::memory_left_answers_a_refusal_with_the_numbers_and_the_advice',
    ),
    (
        '30',
        'the capacity screen is replaced by the serialised reply',
        'tui/src/main.rs',
        'lines.extend(crate::sweep_estimate::estimate_lines(&estimate));',
        'lines.push(estimate.to_string());',
        'tests::memory_left_answers_a_refusal_with_the_numbers_and_the_advice',
    ),
    (
        '30',
        'a failed capacity read stops saying nothing was ingested',
        'tui/src/main.rs',
        '"Nothing was ingested and nothing was billed.".to_string(),',
        '"Retry.".to_string(),',
        'tests::a_failed_capacity_read_names_the_failure_and_says_nothing_was_ingested',
    ),
    (
        '30',
        'the door swallows every /memory spelling',
        'tui/src/main.rs',
        '"memory" if argument.trim() == "left" => self.request_memory_left(tx),',
        '"memory" => self.request_memory_left(tx),',
        'tests::memory_left_is_a_local_door_and_plain_memory_is_still_the_remote_question',
    ),
    (
        '30',
        'the door grows a second capacity renderer',
        'tui/src/main.rs',
        '                    lines.extend(crate::sweep_estimate::estimate_lines(&estimate));',
        '                    lines.push(format!("held {:?}", estimate.get("held_tokens")));',
        'tests::the_capacity_door_prints_the_same_screen_the_sweep_refusal_does',
    ),
    # ── 14 · skills browse ────────────────────────────────────────────────────
    (
        '14',
        'the footer advertises typing on every picker',
        'tui/src/live_renderer.rs',
        '                if app.picker_takes_letters() {\n                    "type to filter',
        '                if true {\n                    "type to filter',
        'tests::only_the_picker_that_takes_letters_says_it_takes_letters',
    ),
    (
        '14',
        'the filterable picker stops saying it takes letters',
        'tui/src/live_renderer.rs',
        '                if app.picker_takes_letters() {\n                    "type to filter',
        '                if false {\n                    "type to filter',
        'tests::only_the_picker_that_takes_letters_says_it_takes_letters',
    ),
    (
        '14',
        'the picker stops narrowing on a typed letter',
        'tui/src/main.rs',
        "            KeyCode::Char(c)\n                if takes_letters && (c.is_ascii_alphabetic() || c == '-')",
        "            KeyCode::Char(c)\n                if false && (c.is_ascii_alphabetic() || c == '-')",
        'tests::only_the_picker_that_takes_letters_says_it_takes_letters',
    ),
    (
        '14',
        "screen 14's contract stops naming a field the wire lacks",
        'tui/src/design_book/mod.rs',
        'contract: "no enabled state, token cost or compose budget on the wire",',
        'contract: "no per-skill token cost on the wire",',
        'tests::the_skills_browse_contract_names_the_state_the_wire_does_not_carry',
    ),
    # ── 41 · held memory ──────────────────────────────────────────────────────
    (
        '41',
        'x retracts without a confirmation',
        'tui/src/memory_view.rs',
        "            KeyCode::Char('x') => {\n                self.confirm = self.selected_source().map(str::to_string);\n            }",
        "            KeyCode::Char('x') => {\n                if let Some(subject) = self.selected_source().map(str::to_string) {\n                    return Action::Retract(subject);\n                }\n            }",
        'memory_view::tests::a_retraction_needs_a_second_deliberate_key_and_any_other_key_cancels',
    ),
    (
        '41',
        'an unreported store is rendered as a success',
        'tui/src/memory_view.rs',
        'None => format!("The server did not report whether {subject}\'s claim was closed."),',
        'None => format!("{subject} is no longer answered as the current belief."),',
        'memory_view::tests::a_partial_retraction_leads_with_the_warning_and_never_reads_as_done',
    ),
    (
        '41',
        'a partial retraction stops leading with the warning',
        'tui/src/memory_view.rs',
        '    if flag("partial") == Some(true) {',
        '    if false {',
        'memory_view::tests::a_partial_retraction_leads_with_the_warning_and_never_reads_as_done',
    ),
    (
        '41',
        'the confirmation is drawn under the live list',
        'tui/src/memory_view.rs',
        '        if let Some(subject) = &self.confirm {\n            return confirm_lines(subject, &self.repo, palette, width);\n        }',
        '        if let Some(_subject) = &self.confirm {\n            // drawn under the list instead\n        }',
        'tests::the_held_memory_pane_takes_its_keys_and_retracts_only_after_a_confirmation',
    ),
    (
        '41',
        "the pane invents the book's trust vocabulary",
        'tui/src/memory_view.rs',
        '            let trust = item.trust.as_deref().unwrap_or(UNKNOWN);',
        '            let trust = if item.may_ground == Some(true) { "measured" } else { "asserted" };',
        'memory_view::tests::the_screen_draws_the_servers_vocabulary_and_omits_the_columns_it_has_no_field_for',
    ),
    (
        '41',
        'the retracted row is deleted from the listing',
        'tui/src/memory_view.rs',
        '        self.receipt = Some(receipt_lines(subject, reply));',
        '        self.receipt = Some(receipt_lines(subject, reply));\n        self.rows.retain(|row| row.source.as_deref() != Some(subject));\n        self.refilter();',
        'memory_view::tests::a_retracted_row_is_not_removed_from_the_listing',
    ),
    (
        '41',
        "the pane swallows the app's chords",
        'tui/src/memory_view.rs',
        '            return Action::Passthrough;\n        }\n        if let Some(subject) = self.confirm.clone() {',
        '            return Action::Handled;\n        }\n        if let Some(subject) = self.confirm.clone() {',
        'tests::the_held_memory_pane_owns_its_letters_and_passes_chords_through',
    ),
    # ── D7 · the orchestra glyphs ─────────────────────────────────────────────
    (
        'D7',
        'Queued goes back to its own glyph',
        'tui/src/orchestra_view.rs',
        '        FleetAgentStatus::Queued => mark(crate::marks::Mark::Queued),',
        '        FleetAgentStatus::Queued => ("·", palette.dim),',
        'orchestra_view::tests::the_states_that_mean_a_mark_read_the_mark_and_the_exemptions_are_named',
    ),
    (
        'D7',
        'Running goes back to its own glyph',
        'tui/src/orchestra_view.rs',
        '        FleetAgentStatus::Running => mark(crate::marks::Mark::InFlight),',
        '        FleetAgentStatus::Running => ("◐", palette.green),',
        'orchestra_view::tests::the_states_that_mean_a_mark_read_the_mark_and_the_exemptions_are_named',
    ),
    (
        'D7',
        'a terminal outcome silently takes a Mark glyph',
        'tui/src/orchestra_view.rs',
        '        FleetAgentStatus::Completed => ("✓", palette.green),',
        '        FleetAgentStatus::Completed => ("●", palette.green),',
        'orchestra_view::tests::the_states_that_mean_a_mark_read_the_mark_and_the_exemptions_are_named',
    ),
]


def run(root: pathlib.Path, test: str, target: "str | None") -> bool:
    """True when the named test PASSES. A surviving mutant is a failing guard."""
    env = dict(os.environ)
    if target:
        env["CARGO_TARGET_DIR"] = target
    done = subprocess.run(
        ["cargo", "test", "-p", "estelle-tui", "--lib", "--bin", "estelle", test],
        cwd=root, env=env, capture_output=True, text=True,
    )
    return done.returncode == 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worktree", nargs="?", default=str(HERE))
    parser.add_argument("--only", default="", help="run only mutants whose screen or name matches")
    parser.add_argument("--target-dir", default=os.environ.get("CARGO_TARGET_DIR"))
    args = parser.parse_args()
    root = pathlib.Path(args.worktree).resolve()

    # 🔴 **WARM THE BUILD BEFORE THE FIRST MUTATION, BECAUSE THE FIRST RUN OF THIS SUITE REPORTED
    # FIVE FALSE SURVIVORS AND EVERY ONE OF THEM WAS A REAL GUARD.** It was invoked straight after
    # a `cargo clippy`, whose artifacts do not satisfy `cargo test`; the first mutations went in
    # while cargo was still rebuilding from the clippy state, and the test that ran had not seen
    # them. Re-running the same five in isolation killed all five. That is `guard_mutants`'s own
    # `.pyc`-staleness hazard in Rust clothes: a kill INFERRED is not a kill ASSERTED, and so is a
    # SURVIVAL. One clean compile first, and every verdict after it is about the mutation.
    print("warming the build so the first verdict is about the mutation, not the cache ...")
    subprocess.run(
        ["cargo", "test", "-p", "estelle-tui", "--lib", "--bin", "estelle", "--no-run"],
        cwd=root,
        env={**os.environ, **({"CARGO_TARGET_DIR": args.target_dir} if args.target_dir else {})},
        capture_output=True,
        text=True,
        check=False,
    )

    survivors = []
    for screen, name, rel, old, new, test in MUTANTS:
        if args.only and args.only not in screen and args.only not in name:
            continue
        path = root / rel
        src = path.read_text()
        if old not in src:
            # 🔴 A NEEDLE THAT NO LONGER MATCHES IS A FAILURE, NOT A SKIP. Silently passing
            # over a mutation nobody could apply is how a mutation suite becomes decoration.
            survivors.append(f"{screen}: needle missing, mutant never applied - {name}")
            print(f"NOT APPLIED  {screen}  {name}")
            continue
        path.write_text(src.replace(old, new, 1))
        try:
            green = run(root, test, args.target_dir)
        finally:
            path.write_text(src)
        print(f'{"SURVIVED" if green else "killed":11}  {screen}  {name}  ->  {test}')
        if green:
            survivors.append(f"{screen}: {name}")

    print()
    if survivors:
        print("SURVIVING MUTANTS - these guards cannot fail:")
        for item in survivors:
            print(" -", item)
        return 1
    print("every mutant was killed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
