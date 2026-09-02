#!/usr/bin/env python3
"""Rebuild the CLI design book's frames from what the REAL Rust renderer wrote.

🔴 WHY THIS SCRIPT EXISTS
-------------------------
The founder read `CLI-DESIGN-BOOK.html` screen by screen and asked one thing of the next pass:
*"Is this rendered in Rust or JavaScript? I want you to render all of this now in Rust, so that
it's easier for you to port these over."*

The SHIPPED frames in the book were already read back off the production renderer's SVG. The SPEC
and PROPOSED ones were hand-drawn HTML — their columns were spaces somebody counted, which is a
layout claim nothing can falsify. Now every screen in the book comes out of
`cargo test -p estelle-tui --bin estelle actual_renderer_gallery`, so the book is a COPY of the
product rather than a translation of it.

⚠️ THE COLOURS ARE READ BACK, NOT RETYPED. Each cell's foreground is matched against the exact
`theme::Palette` value for its theme. A colour that matches no token is emitted as `.bad`/`.badc`
with a dotted outline and COUNTED in the report — which is how the four untokened-colour defects in
this book were found in the first place. A generator that quietly mapped an unknown colour to the
nearest role would have hidden every one of them.

⚠️ THE PROSE IS NOT REGENERATED. Every `<p class="purpose">`, every `.rule`/`.ask`/`.ok` note and
every section header in the book is the founder's review material or an argument somebody made. This
script replaces FRAMES and rebuilds the contents list; it never rewrites a sentence. Losing the
prose to regenerate the pictures would be a bad trade.
"""

from __future__ import annotations

import argparse
import html
import re
import sys
from collections import defaultdict
from pathlib import Path

CELL_W = 9
CELL_H = 18
ORIGIN_X = 16
BASELINE = 24

# ── The two palettes, as the book's CSS declares them ──────────────────────────────────────
# Kept in the same order as `theme::Palette`'s fields so a diff against that file is a straight
# read. `sel`, `add` and `del` are BACKGROUND roles and are handled separately below.
DARK = {
    "#16130F": None,  # ground — the canvas, never a span
    "#6F6A5E": "d",
    "#948E81": "m",
    "#E9E6DC": "b",
    "#C52416": "r",
    "#5F9E6E": "g",
    "#C9A227": "w",
    "#7FB3C8": "c",
    "#9FC4E0": "pl",
    "#D48FB0": "sk",
}
CREAM = {
    "#DDDAD1": None,  # ground
    "#8B8578": "d",
    "#575043": "m",
    "#1F1C17": "b",
    "#B0210F": "r",
    "#3D7550": "g",
    "#96751A": "w",
    "#38708C": "c",
    "#356A8C": "pl",
    "#B06A8C": "sk",
}
DARK_BG = {"#241F19": "sel", "#1B2E1D": "add", "#361A18": "del"}
CREAM_BG = {"#D1CCBE": "sel", "#D2DFCC": "add", "#EBD3CF": "del"}

# The two raw-ANSI values `test_gallery::color_hex` emits for a colour that is in no palette. They
# are rendered with a dotted outline so a defect is visible in the book rather than merely present.
DEFECTS = {"#65A8FF": "bad", "#70C6CC": "badc"}

CREAM_FRAMES = {"13-cream-ink"}

# ── The screens the book DESCRIBED and could not show, now rendered ────────────────────────
#
# 🔴 These are the twelve SPEC and four PROPOSED screens, plus seven SHIPPED renderer states the
# gallery had never captured. Every one of them was hand-drawn HTML in the previous book — or, for
# the seven, a sentence saying "no gallery frame exists". They come out of the production renderer
# now, through `cols`, in the product's own palette, under the same no-box guard as everything else.
#
# `book` is the screen number in the founder's own numbering, so a note about "screen 14" still
# lands on the screen he meant. Frames with no number in his book (`33b`) are additions and say so.
NEW_SCREENS: list[tuple[str, str, str, str, str]] = [
    ("02-login-two-stage", "2", "Login, two stage: who you are, then who pays", "b-spec",
     "Stage one is identity. Stage two is <b>who pays for model tokens</b>, which is a different "
     "question with a different answer — an Estelle plan buys grounding and never buys inference. "
     "The five options are the ones the shipped credential picker already offers. "
     "<b>It fills the frame now</b>: the founder's note was that it was cut off halfway."),
    ("06-no-repository-here", "6", "No repository here", "b-ship",
     "Estelle started outside a git repo. Memory and the code graph are per-repo, so there is "
     "nothing to ground against — and the screen says what to do rather than stopping. "
     "<b>A shipped renderer state that had never been looked at</b> (<code>live_renderer.rs:997</code>)."),
    ("09-gate-refused", "9", "Gate refused: a package that does not exist", "b-ship",
     "The deterministic gate refusing an import of a package that is not on PyPI. No model was "
     "asked. 🔴 <b>The refusal is a step in a loop, not a stop</b> — the mark pulses, the reason is "
     "given in full, and the turn visibly continues into round 2. Rendered by "
     "<code>gate_refusal.rs</code>, the same function the live modal calls."),
    ("10-navigation-stale", "10", "Navigation refuses: the index is stale", "b-prop",
     "The repo moved since the last sweep, so a new symbol would be answered with a plausible "
     "wrong citation. Refusing is the only answer that cannot be confidently wrong. The server "
     "verdict is real; this is the screen it had nowhere to appear on."),
    ("11-compaction-refused", "11", "Compaction refuses: one line instead of a context bar", "b-spec",
     "One message is larger than the window, so there is nothing to compact against — it can only "
     "be split. <b>Deliberately no percentage bar</b>: the founder replaced the bar with the "
     "sentence, and the parts add up to the number in it."),
    ("12-skills-typed", "12", "Skills, typed", "b-spec",
     "<code>/</code> then tab. 🔴 <b>No box.</b> The selected row is a band, not a frame, and "
     "<code>enter</code> preloads the skill so you never finish typing a slash command."),
    ("13-skills-offered", "13", "Skills, offered — before the message is sent", "b-spec",
     "🔴 <b>The offer fires on send, not after the answer.</b> Your draft sits highlighted, the "
     "match is named with its provenance, and <code>tab</code> sends it with the skill. "
     "The principle is printed on the frame: it offers, it never auto-runs."),
    ("14-skills-browse", "14", "Skills, browse and toggle", "b-spec",
     "The density the shipped screen lacks: a total and an on-count, a per-skill token cost, and a "
     "compose budget. The on/off toggle is a <b>permission</b>, not a filter — a skill that is off "
     "may not be recommended or auto-used."),
    ("18-every-command", "18", "Every command, and which ones actually work", "b-prop",
     "🔴 <b>The audit the founder asked for, made visible.</b> 63 commands are advertised. Ten show "
     "at a time with a real gutter between the name and its description. Rows are coloured by what "
     "they actually do: live, inert, duplicate, or advertised-and-refused."),
    ("19-shell-mode", "19", "Shell mode, and why it must not look like Estelle", "b-spec",
     "<code>!cargo test</code> running on your machine. Visually distinct from Estelle's own output "
     "without a box: a different gutter, a different weight, an exit code and a visible timeout."),
    ("25-panels-one-terminal", "25", "Panels, one terminal", "b-spec",
     "Several agents as tabs, production over the orchestra fleet on the right. 🔴 <b>The fleet "
     "columns are labelled in words a person reads</b> — model, task, state, tokens, price, "
     "last seen — and the footnote says which of them the wire does not yet carry."),
    ("30-provider-keys", "30", "Provider API key picker", "b-ship",
     "Ten providers, each naming <b>how it authenticates</b> and what the key costs to use. "
     "🔴 <b>The corners are gone.</b> The founder's note on this screen was the third time he said "
     "no boxes; the selected row is a band now."),
    ("32-memory-remaining", "32", "How much memory do I have left", "b-prop",
     "🔴 <b>The whole <code>POST /sweep/estimate</code> answer.</b> Fourteen fields are returned on "
     "every sweep and <code>top_level.rs:2314</code> reads one of them. Held, cap, remaining, net "
     "new, blocked, billable, the suggested plan, the largest paths and the server's own sentence — "
     "all of it was already on the wire."),
    ("33-usage-spend", "33", "Spend — what ctrl+s opens", "b-prop",
     "🔴 <b><code>ctrl+s</code> shows the spend instead of naming a shortcut.</b> "
     "\"This session you spent $5.46\", in words, with the per-model breakdown that backs it and "
     "the turn broken into input, output and cache."),
    ("33b-model-cost", "—", "The costing panel", "b-prop",
     "🔴 <b>The panel the founder said he misses most</b>, and the four things he asked for on one "
     "screen: per model what it costs, what the run is spending, what is left in the plan, and how "
     "much memory is used. The model lock is <b>per role</b> — planning and solving can be "
     "different — and an unlocked role reads <code>affinity</code>, never a blank."),
    ("34-answer-table-diagram", "34", "The answer itself: a table, and a diagram", "b-spec",
     "🔴 <b>Estelle draws in mermaid.</b> A rendered markdown table with citations still clickable "
     "inside it, and a drawn flowchart. The footnote names which diagram types are drawn and which "
     "fall back to their fenced source — a wrong picture is worse than the source."),
    ("35-session-tabs", "35", "Session tabs", "b-ship",
     "Several sessions in one terminal, each carrying its repo, its state and its spend. "
     "A shipped renderer state (<code>live_renderer.rs:114</code>) the gallery had never captured."),
    ("36-doctor-failing", "36", "Doctor, failing", "b-spec",
     "A failing check whose last line states <b>what the failure is not</b> — the clause that stops "
     "a reader debugging the wrong layer. Passing rows are shown too, so the failing one has "
     "contrast rather than a bare red row on an empty screen."),
    ("37-resume-session", "37", "Resume a session", "b-spec",
     "Frecency-ranked, zoxide-style, and every row carries <b>how that session ended</b>: answered, "
     "refused, closed, or still running on the server."),
    ("38-sweep-running", "38", "Sweep, running", "b-ship",
     "The five real states with their real percentages. 🔴 <b>The capacity check shows its answer</b> "
     "— the founder's rule that cost and budget are always visible, applied to the one step that is "
     "literally a call to <code>/sweep/estimate</code>."),
    ("39-tool-calls", "39", "Tool calls, collapsed and open", "b-spec",
     "One row per call, expandable. 🔴 <b>What is hidden is counted, first, before the tail</b> — "
     "never a silent truncation, because a capped read means \"cannot answer\", not \"that is all "
     "there is\"."),
    ("40-code-graph", "40", "The code graph", "b-spec",
     "Walkable and filterable, with fan-in and fan-out off the swept graph rather than inferred. "
     "🔴 <b>A chokepoint is not a label</b>: the row says how many files touching it moves."),
    ("41-memory-correct", "41", "Memory: what Estelle knows, and correcting it", "b-prop",
     "Held memory with trust tiers — measured, observed, asserted — plus the half that does not "
     "exist yet: saying \"that is wrong\" without leaving the terminal. 🔴 <b>An edit supersedes; "
     "it does not overwrite</b>, and the superseded claim stays on the screen, dated."),
]



RECT = re.compile(
    r'<rect x="(\d+)" y="(\d+)" width="(\d+)" height="\d+" fill="(#[0-9A-Fa-f]{6})"/>'
)
TEXT = re.compile(
    r'<text x="(\d+)" y="(\d+)" fill="(#[0-9A-Fa-f]{6})" font-weight="(\d+)">(.*?)</text>'
)


# 🔴 AN UNTOKENED COLOUR IS RENDERED AS ITSELF, OUTLINED, AND COUNTED.
#
# ⚠️ The first version of this script RAISED on a colour that matched no token. That was the wrong
# shape twice over: it stopped the book from building over a defect the book exists to SHOW, and it
# made the census a thing you only learn by reading a traceback. Snapping an unknown to the nearest
# role would have been worse still — quietly tidying away exactly what a colour read-back is for.
#
# So an unknown colour keeps its own hex inline, gets a dotted outline, and lands in the report with
# a cell count. That is how the four untokened colours in this book were found: 1,019 boot cells,
# 51 on the slash palette, 17 on the waiting screen, and a "Claude-like semantic blue" nobody owned.
UNTOKENED_CLASS = "untok"


def parse_svg(path: Path) -> tuple[list[list[tuple[str, str | None, str | None, bool]]], int, int]:
    """`(rows, width, height)` where each cell is `(char, fg_hex, bg_hex, bold)`."""
    source = path.read_text(encoding="utf-8")
    header = re.search(r'<svg[^>]*width="(\d+)" height="(\d+)"', source)
    if header is None:
        raise ValueError(f"{path} has no svg header")
    width = (int(header.group(1)) - 32) // CELL_W
    height = (int(header.group(2)) - 32) // CELL_H

    backgrounds: dict[tuple[int, int], str] = {}
    for x, y, run, fill in RECT.findall(source):
        row = (int(y) + 14 - BASELINE) // CELL_H
        start = (int(x) - ORIGIN_X) // CELL_W
        for column in range(start, start + int(run) // CELL_W):
            backgrounds[(row, column)] = fill.upper()

    glyphs: dict[tuple[int, int], tuple[str, str, bool]] = {}
    for x, y, fill, weight, glyph in TEXT.findall(source):
        row = (int(y) - BASELINE) // CELL_H
        column = (int(x) - ORIGIN_X) // CELL_W
        glyphs[(row, column)] = (html.unescape(glyph), fill.upper(), weight == "700")

    rows = []
    for row in range(height):
        cells = []
        for column in range(width):
            glyph, fg, bold = glyphs.get((row, column), (" ", None, False))
            cells.append((glyph, fg, backgrounds.get((row, column)), bold))
        rows.append(cells)
    return rows, width, height


def classes_for(fg: str | None, bg: str | None, cream: bool, defects: dict) -> str:
    """The book's span classes for one cell, and a tally of anything untokened.

    An untokened foreground gets `untok` PLUS an inline colour, so the reader sees the real value
    the renderer emitted rather than a stand-in — the point of the outline is that you can see WHAT
    is wrong, not merely that something is.
    """
    palette = CREAM if cream else DARK
    grounds = CREAM_BG if cream else DARK_BG
    names: list[str] = []
    if bg is not None:
        if bg in grounds:
            names.append(grounds[bg])
        elif palette.get(bg, "missing") is None:
            pass  # the canvas ground painted explicitly; not a span
        else:
            defects[f"bg {bg}"] += 1
    if fg is not None:
        if fg in DEFECTS:
            names.append(DEFECTS[fg])
            defects[fg] += 1
        elif fg in palette:
            role = palette[fg]
            if role is not None:
                names.append(role)
        else:
            names.append(UNTOKENED_CLASS)
            defects[fg] += 1
    # `sel` already sets its own colour; a role class after it would fight the highlight.
    if "sel" in names and len(names) > 1:
        names = ["sel"]
    return " ".join(dict.fromkeys(names))


def render_pre(rows, cream: bool, defects: dict) -> str:
    """One `<pre>` block: runs of identical class, with trailing blanks trimmed off each row."""
    out: list[str] = []
    for cells in rows:
        # Trim the right-hand run of unstyled blanks so the book does not carry 90 columns of
        # padding on every row — the frame scrolls horizontally and the padding is not information.
        end = len(cells)
        while end > 0 and cells[end - 1][0] == " " and cells[end - 1][2] is None:
            end -= 1
        line: list[str] = []
        run_class: str | None = None
        run_text: list[str] = []

        def flush() -> None:
            if not run_text:
                return
            text = html.escape("".join(run_text), quote=False)
            line.append(f'<span class="{run_class}">{text}</span>' if run_class else text)

        for glyph, fg, bg, _bold in cells[:end]:
            names = classes_for(fg, bg, cream, defects) or None
            if names and UNTOKENED_CLASS in names:
                names = f"{names}\" style=\"color:{fg}"
            if names != run_class:
                flush()
                run_class, run_text = names, []
            run_text.append(glyph)
        flush()
        out.append("".join(line))
    # Drop trailing blank rows: an empty frame bottom is padding, not a screen.
    while out and not out[-1].strip():
        out.pop()
    return "<pre>" + "\n".join(out) + "</pre>"


def frame_html(name: str, gallery: Path, defects: dict) -> tuple[str, int, int]:
    cream = name in CREAM_FRAMES
    rows, width, height = parse_svg(gallery / f"{name}.svg")
    body = render_pre(rows, cream, defects)
    css = "frame cream" if cream else "frame"
    return f'<div class="{css}">{body}</div>', width, height


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gallery", required=True, type=Path)
    parser.add_argument("--book", required=True, type=Path, help="the existing book, read only")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    gallery = args.gallery
    frames = sorted(path.stem for path in gallery.glob("*.svg"))
    if not frames:
        print(f"no frames in {gallery} — did the gallery run?", file=sys.stderr)
        return 2

    book = args.book.read_text(encoding="utf-8")
    defects: dict[str, int] = defaultdict(int)

    # ── 1. The cream tokens in the book's own CSS moved, so the book's swatches must too. ──
    # The founder said the light ground *"kind of hurt my eye"*; `theme.rs` came down 5% and these
    # two variables are the book's copy of that value. A book still painting #E9E6DC would be
    # showing him the thing he asked to change.
    book = book.replace("--c-ground:#E9E6DC;", "--c-ground:#DDDAD1;")
    book = book.replace("--c-tint:#DCD7C9;", "--c-tint:#D1CCBE;")
    # The class an untokened colour lands in. Outlined so it cannot be mistaken for a design choice.
    if ".untok{" not in book:
        book = book.replace(
            ".bad{color:var(--x-blue)",
            ".untok{outline:1px dotted #E8776A;outline-offset:1px}\n"
            ".bad{color:var(--x-blue)",
            1,
        )

    # ── 2. Replace every frame the gallery now renders. ──────────────────────────────────
    sections = list(
        re.finditer(
            r'<section class="screen" id="(s\d+)">\n<div class="sh">(.*?)</div>\n(.*?)\n</section>',
            book,
            re.S,
        )
    )
    replaced: dict[str, str] = {}
    for match in sections:
        header = match.group(2)
        src = re.search(r'<span class="src">(.*?)</span>', header)
        if src is None:
            continue
        named = [frame for frame in frames if frame in src.group(1)]
        if not named:
            continue
        frame = named[0]
        block, width, height = frame_html(frame, gallery, defects)
        body = match.group(3)
        updated = re.sub(
            r'<div class="frame[^"]*">.*?</div>(?=\n<p class="keys")',
            lambda _m, block=block: block,
            body,
            count=1,
            flags=re.S,
        )
        if updated == body:  # no keys line after the frame; replace the last frame div
            updated = re.sub(
                r'<div class="frame[^"]*">.*?</div>', block, body, count=1, flags=re.S
            )
        new_header = re.sub(
            r'<span class="src">.*?</span>',
            f'<span class="src">{frame} · {width}x{height} · rendered in Rust</span>',
            header,
            count=1,
        )
        replaced[frame] = match.group(1)
        book = book.replace(match.group(0), (
            f'<section class="screen" id="{match.group(1)}">\n'
            f'<div class="sh">{new_header}</div>\n{updated}\n</section>'
        ), 1)

    # ── 3. The screens the book described and could not show. ────────────────────────────
    #
    # ⚠️ APPENDED AS A NAMED SECTION rather than spliced into the founder's numbering. His review
    # cites screens by number — "rebuild 14 this way" — and renumbering the book underneath those
    # notes would silently redirect every one of them. The `book #` column carries his number.
    known = {frame for frame, *_ in NEW_SCREENS}
    blocks = [
        '\n<h2 class="sec">M &middot; Rendered in Rust &mdash; the screens the book could only '
        "describe</h2>\n"
        '<p class="lede">Every frame below came out of <code>cargo test -p estelle-tui --bin estelle '
        "actual_renderer_gallery</code>, the same command that produced the shipped screens above. "
        "They were hand-drawn HTML in the last book, which meant their columns were spaces somebody "
        "counted. <b>These are the real renderer's columns, in the product's own palette, under the "
        "same no-box guard as every live screen.</b> The DATA is fixture; the LAYOUT is not.</p>\n"
    ]
    added = 0
    for frame, number, title, badge, purpose in NEW_SCREENS:
        if frame in replaced or frame not in frames:
            continue
        block, width, height = frame_html(frame, gallery, defects)
        label = {"b-ship": "SHIPPED", "b-spec": "SPEC", "b-prop": "PROPOSED"}[badge]
        number_html = f"{number}" if number != "—" else "&mdash;"
        blocks.append(
            f'<section class="screen" id="{frame}">\n'
            f'<div class="sh"><span class="num">{number_html}</span>'
            f'<span class="name">{title}</span>'
            f'<span class="badge {badge}">{label}</span>'
            f'<span class="src">{frame} · {width}x{height} · rendered in Rust</span></div>\n'
            f'<p class="purpose">{purpose}</p>\n{block}\n</section>'
        )
        added += 1
    if added:
        anchor = '<hr class="big">'
        marker = anchor if anchor in book else '<p class="foot"'
        book = book.replace(marker, "\n".join(blocks) + "\n" + marker, 1)

    # ── 3b. One section in the founder's book carries TWO frames. ────────────────────────
    #
    # Screen 26 is titled "Todos, expanded and collapsed" and the previous book showed one of them.
    # ⚠️ A title that promises two states and a frame that shows one is the small version of the
    # partial-guard defect: the section reads as complete. The second frame goes in beside the
    # first rather than becoming a screen 42 the founder never numbered.
    if "10-todo-collapsed" in frames and "10-todo-collapsed" not in replaced:
        host = re.search(
            r'(<section class="screen" id="s\d+">\n<div class="sh">(?:(?!</section>).)*?'
            r'09-todo-expanded(?:(?!</section>).)*?</div>)',
            book,
            re.S,
        )
        if host is not None:
            block, width, height = frame_html("10-todo-collapsed", gallery, defects)
            book = book.replace(
                host.group(0) + "\n",
                host.group(0) + "\n" + block + "\n",
                1,
            )
            replaced["10-todo-collapsed"] = "s26"
            added += 1

    # ── 4. The contents list is a SECOND copy of every title and badge. Extend it or it drifts. ──
    rows = "".join(
        f'<li><span class="n">{number if number != "—" else "&mdash;"}</span>'
        f'<a class="t" href="#{frame}">{title}</a>'
        f'<span class="badge {badge}">'
        f'{ {"b-ship": "SHIPPED", "b-spec": "SPEC", "b-prop": "PROPOSED"}[badge] }</span></li>'
        for frame, number, title, badge, _purpose in NEW_SCREENS
        if frame in frames and frame not in replaced
    )
    if rows:
        book = book.replace("</ol>", rows + "</ol>", 1)

    # ⚠️ COMPUTED HERE, NOT EARLIER. The first version measured this before step 3b ran and
    # reported a frame as unplaced that the very next block had placed — a report that describes an
    # earlier state of the world is a report nobody can act on.
    unlisted = [frame for frame in frames if frame not in replaced and frame not in known]
    args.out.write_text(book, encoding="utf-8")

    print(f"frames in gallery      : {len(frames)}")
    print(f"book frames replaced   : {len(replaced)}")
    print(f"new sections appended   : {added}")
    print(f"frames still unplaced   : {len(unlisted)}")
    for frame in unlisted:
        print(f"  · {frame}")
    if defects:
        print("untokened colours still on the page:")
        for value, count in sorted(defects.items()):
            print(f"  · {value}  {count} cells")
    else:
        print("untokened colours still on the page: none")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
