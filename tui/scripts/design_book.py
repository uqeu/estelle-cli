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
    # 🔴 A ROLE THE READ-BACK COULD NOT SEE. `tint` is a `theme::Palette` field like every other
    # value here and the dict simply did not list it, so a cell painted with a real token was
    # reported as untokened. That is the census UNDER-reading its own subject — the safe direction
    # to be wrong in, and still wrong. It appears as a FOREGROUND on the dither's faintest step and
    # as a BACKGROUND on a selected row; the background maps below own the second meaning.
    "#241F19": "tn",
}
# ⚠️ RE-SEATED 2026-09-02, AND THIS DICT IS WHY THAT IS A TWO-FILE CHANGE. Every cell is matched
# against these hexes EXACTLY; a value the product ships that is missing here is counted as an
# untokened DEFECT, so a palette change made in `theme.rs` alone makes the book report the product's
# own new colours as a regression. Six accents and four bands moved — see the cream block in
# `theme.rs` for the measurements.
CREAM = {
    "#DDDAD1": None,  # ground
    "#645F56": "d",
    "#575043": "m",
    "#1F1C17": "b",
    "#B0210F": "r",
    "#2D553A": "g",
    "#5C4810": "w",
    "#264B5E": "c",
    "#1B3247": "pl",
    "#6F2046": "sk",
    "#D1CCBE": "tn",
}
# `adg`/`delg` are the diff GUTTER grounds. They were four `Color::from_u32` literals
# inside `live_renderer::github_diff_lines` with a written reason, which made the diff pane
# a second owner of a product colour; they are `Palette::diff_add_gutter` /
# `diff_del_gutter` now, same values, one owner.
DARK_BG = {"#241F19": "sel", "#1B2E1D": "add", "#361A18": "del",
           "#162E20": "adg", "#4A221D": "delg"}
CREAM_BG = {"#D1CCBE": "sel", "#B1C8A7": "add", "#E4C3BE": "del",
            "#8FBE85": "adg", "#EFA49E": "delg"}

# The two raw-ANSI values `test_gallery::color_hex` emits for a colour that is in no palette. They
# are rendered with a dotted outline so a defect is visible in the book rather than merely present.
DEFECTS = {"#65A8FF": "bad", "#70C6CC": "badc"}

CREAM_FRAMES = {"13-cream-ink"}

# ── Which gallery frame belongs to which numbered screen ───────────────────────────────────
#
# 🔴 **THIS TABLE USED TO CARRY TITLES, BADGES AND PROSE, AND THAT WAS THE DEFECT THE FOUNDER
# FOUND.** It held a second copy of each screen's title and a second paragraph of purpose, which
# the generator APPENDED under the one already in the book — so a screen ended up with two
# descriptions, one of them written about the hand-drawn mock it had replaced. He read the result
# and said the notes were decayed, and pointed at a screen whose own note argued for showing a
# different screen instead while the badge said SHIPPED.
#
# So the source book owns every word now. This table is one fact per row: **this frame is the
# picture for that numbered screen.** Sections whose `src` already names their frame are not
# listed here at all — step 2 finds those by name.
FRAME_FOR_SCREEN: dict[str, int] = {
    "02-login-two-stage": 2,
    "06-no-repository-here": 6,
    "09-gate-refused": 9,
    "10-navigation-stale": 10,
    "11-compaction-refused": 11,
    "12-skills-typed": 12,
    "13-skills-offered": 13,
    "14-skills-browse": 14,
    "18-every-command": 16,
    "19-shell-mode": 17,
    "25-panels-one-terminal": 23,
    "30-provider-keys": 28,
    "32-memory-remaining": 30,
    "33-usage-spend": 31,
    "34-answer-table-diagram": 34,
    "35-session-tabs": 35,
    "36-doctor-failing": 36,
    "37-resume-session": 37,
    "38-sweep-running": 38,
    "39-tool-calls": 39,
    "40-code-graph": 40,
    "41-memory-correct": 41,
}

# 🔴 **A FRAME THE BOOK DELIBERATELY DOES NOT SHOW, AND THE REASON, SO IT IS NOT A LEAK.**
#
# `12-skills` is the skills picker AS IT SHIPS TODAY — three rows, no counts, no cost column, no
# toggle. It was screen 15, beside screen 14's redesign of the same surface, and the founder's
# instruction was exact: *"We are making NEW screens, so I want to see the new screens — not what we
# have now in our old CLI."* The frame stays in the gallery, because the gallery is a record of what
# the binary renders; it is out of the BOOK, because the book is what we are building.
#
# ⚠️ Listed rather than deleted so `frames still unplaced` stays a real number. A frame that fell out
# of the book by accident and one taken out on purpose look identical in a directory listing.
NOT_IN_THE_BOOK = {
    "12-skills": "the old skills picker — screen 14 is the redesign of the same surface",
}


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


def read_badges(gallery: Path) -> dict[str, str]:
    """`frame -> "SHIPPED" | "DESIGN"`, derived from the code rather than typed here.

    🔴 **EVERY SCREEN IN THE PREVIOUS BOOK SAID SHIPPED, AND ONE OF THEM ARGUED AGAINST ITSELF.**
    Screen 14's own note read *"until `/skills` returns tokens per skill, the honest screen is 15,
    not this one"* — under a SHIPPED badge. The founder read that and said the notes were decayed.
    They were; so was the badge. Both cannot be right and neither was checkable.

    The distinction already had exactly one owner in the Rust: `design_book::BookScreen::contract`
    begins `shipped ·` when the live app renders that screen from real state, and names the missing
    endpoint otherwise. `actual_renderer_gallery` writes those out as `contracts.tsv`. A frame with
    no row there is a plain live-renderer state — boot, home, the settings list — and is SHIPPED.

    ⚠️ **DESIGN IS NOT `SPEC` COMING BACK.** SPEC meant *drawn by hand, not built*. Every frame in
    this book is drawn by the production renderer at a real terminal size. DESIGN means the LAYOUT
    is real and the DATA under it is a fixture, because the contract printed on the screen does not
    exist on the wire yet. That is the difference between "we sketched this" and "this needs one
    server field", and it is the difference the reader is trying to make a decision about.
    """
    manifest = gallery / "contracts.tsv"
    if not manifest.exists():
        raise SystemExit(
            f"{manifest} is missing — run the gallery with ESTELLE_ACTUAL_GALLERY_DIR set, "
            "otherwise every badge would silently default to SHIPPED"
        )
    badges: dict[str, str] = {}
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        name, _, contract = line.partition("\t")
        badges[name] = "SHIPPED" if contract.startswith("shipped ·") else "DESIGN"
    if not badges:
        raise SystemExit(f"{manifest} parsed to zero rows — the badge would mean nothing")
    return badges


def badge_html(kind: str) -> str:
    css = {"SHIPPED": "b-ship", "DESIGN": "b-design"}[kind]
    return f'<span class="badge {css}">{kind}</span>'


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

    badges = read_badges(gallery)
    book = args.book.read_text(encoding="utf-8")
    defects: dict[str, int] = defaultdict(int)

    # ── 1. The book's CSS palette is WRITTEN from the dicts above, not patched value by value. ──
    #
    # 🔴 **IT USED TO BE A LIST OF (STALE, CURRENT) PAIRS AND THAT LIST WENT STALE, WHICH IS THE
    # ONLY THING SUCH A LIST CAN DO.** The book's `<style>` block holds a `--t-*` and a `--c-*`
    # variable per role — a third copy of the palette, after `theme.rs` and the dicts at the top of
    # this file. Patching named pairs means a role nobody remembered keeps its old value silently:
    # `--t-skill` was `#CA92AF` while the product shipped `#D48FB0`, for months, in the book that
    # exists to show the founder what the product looks like.
    #
    # Now every variable is rewritten from the same dict the cell read-back matches against. A
    # value that disagrees with `theme.rs` still shows up — as untokened cells in the census — so
    # the two remaining owners cannot drift apart quietly in either direction.
    ROLE_VARS = {
        "d": "dim", "m": "mid", "b": "bright", "r": "red", "g": "green",
        "w": "warn", "c": "cite", "pl": "plan", "sk": "skill", "tn": "tint",
    }
    for prefix, palette, grounds in (("t", DARK, DARK_BG), ("c", CREAM, CREAM_BG)):
        wrote = 0
        for value, code in palette.items():
            role = "ground" if code is None else ROLE_VARS.get(code)
            if role is None:
                continue
            book, count = re.subn(
                rf"--{prefix}-{role}:#[0-9A-Fa-f]{{6}};", f"--{prefix}-{role}:{value};", book
            )
            wrote += count
        for value, code in grounds.items():
            if code in ("add", "del"):
                book, count = re.subn(
                    rf"--{prefix}-{code}:#[0-9A-Fa-f]{{6}};", f"--{prefix}-{code}:{value};", book
                )
                wrote += count
        # ⚠️ A rewrite that matched nothing reports success identically. The dark block declares
        # every role; cream declares all but the two diff grounds.
        if wrote < 10:
            raise SystemExit(f"only {wrote} --{prefix}-* variables were written; the CSS moved")
    # The class an untokened colour lands in. Outlined so it cannot be mistaken for a design choice.
    if ".untok{" not in book:
        book = book.replace(
            ".bad{color:var(--x-blue)",
            ".untok{outline:1px dotted #E8776A;outline-offset:1px}\n"
            ".bad{color:var(--x-blue)",
            1,
        )
    # The three palette roles the read-back could not name until this pass. Declared for BOTH
    # grounds, because a class defined once against the dark variables paints the cream frame with
    # a dark value and the reader sees a bug that is not in the product.
    if ".tn{" not in book:
        book = book.replace(
            ".untok{",
            ".tn{color:var(--t-tint)}\n"
            ".adg{background:#162E20}\n"
            ".delg{background:#4A221D}\n"
            ".frame.cream .tn{color:var(--c-tint)}\n"
            ".frame.cream .adg{background:#8FBE85}\n"
            ".frame.cream .delg{background:#EFA49E}\n"
            ".untok{",
            1,
        )
    if ".b-design{" not in book:
        book = book.replace(
            ".b-ship{",
            ".b-design{background:#241d24;color:var(--t-skill);border:1px solid #40303a}\n"
            ".b-ship{",
            1,
        )

    # ── 2. EVERY SECTION GETS ITS FRAME, ITS SOURCE LINE AND ITS BADGE. ──────────────────────
    #
    # A section names its frame one of two ways: its `src` already contains the frame name, or
    # `FRAME_FOR_SCREEN` says which frame belongs to its number. Both paths land here, so there is
    # one place that writes a frame into the book and one place that writes a badge.
    sections = list(
        re.finditer(
            r'<section class="screen" id="(s\d+)">\n<div class="sh">(.*?)</div>\n(.*?)\n</section>',
            book,
            re.S,
        )
    )
    by_number = {number: frame for frame, number in FRAME_FOR_SCREEN.items()}
    placed: dict[str, str] = {}
    unframed: list[str] = []

    for match in sections:
        sid, header, body = match.group(1), match.group(2), match.group(3)
        number = int(sid[1:])
        src = re.search(r'<span class="src">(.*?)</span>', header)
        named = [frame for frame in frames if src and frame in src.group(1)]
        frame = named[0] if named else by_number.get(number)
        if frame is None or frame not in frames:
            unframed.append(sid)
            continue

        block, width, height = frame_html(frame, gallery, defects)
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
        new_header = re.sub(
            r'<span class="badge b-[a-z]+">[A-Z]+</span>',
            badge_html(badges.get(frame, "SHIPPED")),
            new_header,
            count=1,
        )
        placed[frame] = sid
        book = book.replace(
            match.group(0),
            f'<section class="screen" id="{sid}">\n'
            f'<div class="sh">{new_header}</div>\n{updated}\n</section>',
            1,
        )

    # ── 2b. One section carries TWO frames. ─────────────────────────────────────────────────
    #
    # "Todos, expanded and collapsed" promises two states. ⚠️ A title that promises two and a frame
    # that shows one is the small version of the partial-guard defect: the section reads complete.
    if "10-todo-collapsed" in frames and "10-todo-collapsed" not in placed:
        host = re.search(
            r'(<section class="screen" id="(s\d+)">\n<div class="sh">(?:(?!</section>).)*?'
            r'09-todo-expanded(?:(?!</section>).)*?</div>)',
            book,
            re.S,
        )
        if host is not None:
            block, _width, _height = frame_html("10-todo-collapsed", gallery, defects)
            book = book.replace(host.group(0) + "\n", host.group(0) + "\n" + block + "\n", 1)
            placed["10-todo-collapsed"] = host.group(2)

    # ── 3. THE CONTENTS LIST IS BUILT FROM THE SECTIONS, NOT MAINTAINED BESIDE THEM. ────────
    #
    # 🔴 It was a hand-written second copy of every number, title and badge, and it went stale
    # exactly where nobody re-reads: a section re-badged in one place and not the other. It is
    # derived now, so a contents row can only be wrong by its section being wrong.
    headers = re.findall(
        r'<section class="screen" id="(s\d+)">\n<div class="sh">(.*?)</div>', book, re.S
    )
    rows = []
    for sid, header in headers:
        number = re.search(r'<span class="num">(.*?)</span>', header).group(1)
        title = re.search(r'<span class="name">(.*?)</span>', header).group(1)
        kind = re.search(r'<span class="badge b-[a-z]+">([A-Z]+)</span>', header).group(1)
        rows.append(
            f'<li><span class="n">{number}</span>'
            f'<a class="t" href="#{sid}">{title}</a>{badge_html(kind)}</li>'
        )
    book = re.sub(r"<ol>.*?</ol>", "<ol>" + "".join(rows) + "</ol>", book, count=1, flags=re.S)

    # ── 4. THE GUARDS. Each one names a way the book could report complete while being wrong. ──
    numbers = [
        int(re.search(r'<span class="num">(\d+)</span>', header).group(1))
        for _sid, header in headers
    ]
    gaps = numbers != list(range(1, len(numbers) + 1))
    strays = [
        sid for sid, header in headers if not re.search(r'<span class="num">\d+</span>', header)
    ]
    unplaced = [
        frame for frame in frames if frame not in placed and frame not in NOT_IN_THE_BOOK
    ]
    hand_drawn = [sid for sid, header in headers if "rendered in Rust" not in header]

    # The book's own summary of its badge vocabulary. Two owners of one count, and the summary is
    # the one nobody re-reads — so it is computed from the output rather than typed.
    shipped = sum(1 for _sid, header in headers if 'b-ship">SHIPPED' in header)
    design = sum(1 for _sid, header in headers if 'b-design">DESIGN' in header)
    book = re.sub(
        r'<p class="lede" style="margin-top:14px"><span class="badge b-ship">SHIPPED</span>.*?(?=</p>)',
        '<p class="lede" style="margin-top:14px">'
        f'<span class="badge b-ship">SHIPPED</span> {shipped} &middot; '
        f'<span class="badge b-design">DESIGN</span> {design} &nbsp;&nbsp;'
        f"total {len(headers)}, numbered 1 to {len(headers)} with no gaps. "
        "<b>Every frame in this book is drawn by the production renderer</b> at a real terminal "
        "size, so the badge is not about the picture. SHIPPED means the live app fills that layout "
        "from real state today. DESIGN means the layout is real and the numbers in it are a "
        "fixture, because the screen is waiting on a server field it names on its own face.",
        book,
        count=1,
        flags=re.S,
    )
    book = re.sub(
        r'<tr><td><span class="badge b-spec">SPEC</span></td><td>.*?</tr>',
        '<tr><td><span class="badge b-design">DESIGN</span></td>'
        "<td>the layout is the production renderer's; the DATA is a fixture</td>"
        "<td>Same renderer as SHIPPED. What is missing is the server field the screen names, "
        "not the screen. Without <code>--demo</code> it draws that sentence instead of numbers.</td></tr>",
        book,
        count=1,
        flags=re.S,
    )
    book = re.sub(
        r'<tr><td><span class="badge b-prop">PROPOSED</span></td><td>.*?</tr>',
        "",
        book,
        count=1,
        flags=re.S,
    )

    args.out.write_text(book, encoding="utf-8")

    print(f"frames in gallery           : {len(frames)}")
    print(f"sections                    : {len(headers)}  ({shipped} shipped, {design} design)")
    print(f"contents rows rebuilt       : {len(rows)}")
    print(f"sections with no frame      : {len(unframed)}")
    for sid in unframed:
        print(f"  · {sid}")
    print(f"frames still unplaced       : {len(unplaced)}")
    for frame in unplaced:
        print(f"  · {frame}")
    print(f"frames deliberately omitted : {len(NOT_IN_THE_BOOK)}")
    for frame, reason in NOT_IN_THE_BOOK.items():
        print(f"  · {frame} — {reason}")
    print(f"frames still hand-drawn     : {len(hand_drawn)}")
    for sid in hand_drawn:
        print(f"  · {sid}")
    if defects:
        print("untokened colours still on the page:")
        for value, count in sorted(defects.items()):
            print(f"  · {value}  {count} cells")
    else:
        print("untokened colours still on the page: none")

    failed = False
    if unframed:
        print("  a numbered screen has no frame", file=sys.stderr)
        failed = True
    if unplaced:
        print(
            "  a rendered frame reaches no screen and is not on the deliberate-omission list",
            file=sys.stderr,
        )
        failed = True
    if hand_drawn:
        print("  a section still shows a picture nothing rendered", file=sys.stderr)
        failed = True
    if gaps or strays:
        # 🔴 THE FOUNDER'S STRUCTURAL NOTE, AS A CHECK. The previous book had 41 numbered ids and
        # 43 rows: two screens were appended with no number at all, because appending was easier
        # than integrating. A contents list that skips a number, or carries a row that has none,
        # is the same defect in the one place a reader uses to check for completeness.
        print(f"  the numbering is not 1..{len(headers)}: {numbers} strays={strays}", file=sys.stderr)
        failed = True
    return 3 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
