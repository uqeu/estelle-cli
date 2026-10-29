"""ONE owner for "is this /health surface the production surface we expect".

🔴 THE DEFECT THIS CLOSES, AND IT HAS NOW BILLED THREE TIMES. The same derived fact was pinned in TWO
places -- ``public-install-receipts.EXPECTED_SURFACE`` and
``public-binary-receipts.EXPECTED_PRODUCTION_SURFACE`` -- as an exact dict, and the second file's own
comment records the previous occurrence verbatim: *"246 -> 247 on 2026-08-29. SECOND OWNER of the same
fact ... and BOTH were stale by one."* The remedy applied then was to bump the number, so on 2026-09-03
both were stale again, this time by two: production served ``prompts: 249``.

⚠️ AND THE EQUALITY HAD A SECOND, INDEPENDENT FAILURE MODE. The pins compared the WHOLE dict with
``==``. Production later ADDED a ``tools_sha256`` field, so the comparison could not have matched even
with the right prompt count -- an added field breaks an equality that a subset check survives.

▶ SO THIS ASSERTS THE INVARIANT, NOT THE CONTENT. ``tools_base`` is a contract: the tool surface the CLI
speaks to. ``prompts`` is CONTENT -- it moves whenever a playbook is added, which is a server release
concern and must not be able to fail a CLI release. Pinning it made every CLI release hostage to server
content, and the failure it produced ("a HEALTHY build was refused") reads exactly like a real outage.

⚠️ STATED LIMIT: asserting ``prompts`` is merely present and positive is a VACUITY guard -- it proves
the field was populated, never that the right playbooks loaded. It is paired here with an exact
``tools_base`` so the pair is a shape assertion plus a non-emptiness check, not non-emptiness alone.
"""

from __future__ import annotations

#: The tool surface the CLI is built against. A CONTRACT: a change here is a CLI change, so it is pinned.
#: 🔴 16 -> 28 on 2026-10-23. The v0.4.0 release's public-install-receipts DISCARDED all three setup
#: receipts as "production not stable_and_verified" while every customer-facing check inside them passed
#: (/mcp, /sync, /sweep/estimate and /v1/chat/completions all 200 against build 78fee09044c8). The cause
#: was this pin: production had grown to tools_base 28 and nothing told the CLI. Those receipts are the
#: evidence that the 0.4.0 binary works against the 28-tool surface; that is why the pin moved, rather
#: than moving it because the number differed.
#:
#: 🔴 AND THEN IT BILLED A FOURTH TIME, SO THE PIN IS NOW A FLOOR (2026-10-29). The v0.4.6 run
#: 40844663734 discarded the TypeScript, Go and mixed setup receipts with every functional field
#: passing and `production_build: {before: 3cf96242df8c, after: 3cf96242df8c, stable_and_verified:
#: false}`. Production had not moved: read back the same day, /health on 3cf96242df8c says
#: `build_verified: true`, `tools_base: 35`. The pin said 28. The server adds a base tool most weeks
#: (28 -> 35 in six days), and every one of them was going to discard a healthy CLI release.
#: ▶ What the CLI really needs is the 28-tool surface it was built against, or more. It calls tools by
#: name, so a tool added later cannot break it, and a surface SMALLER than 28 means a build older than
#: this CLI. So the bar is `tools_base >= MIN_TOOLS_BASE`.
#: ⚠️ A floor alone would let a mid-run deploy that kept the label slip through, so the window check
#: ALSO requires the whole surface to be byte-identical before and after (`same_surface`).
#: `tools_sha256` is a digest of the tool contract, so a deploy that changes any tool moves it even when
#: the build label lies.
#: LIMIT: this decides "prod is verified and did not change under the run". It does NOT decide "prod is
#: the right build"; that is the server's `scripts/verify_prod.sh`, run after every push to main.
MIN_TOOLS_BASE = 28

_HEX = frozenset("0123456789abcdef")


def _positive_int(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def surface_ok(surface: object) -> bool:
    """True when ``surface`` is a production surface this CLI can speak to, judged on shape and floor."""
    if not isinstance(surface, dict):
        return False
    tools_base = surface.get("tools_base")
    if not _positive_int(tools_base) or tools_base < MIN_TOOLS_BASE:
        return False
    digest = surface.get("tools_sha256")
    if not (isinstance(digest, str) and len(digest) == 64 and set(digest) <= _HEX):
        return False
    return _positive_int(surface.get("prompts"))


def same_surface(before: object, after: object) -> bool:
    """True when two /health reads served the SAME surface. A deploy that changes any tool changes
    ``tools_sha256`` even when the build label did not move, so this is the half a label cannot fake."""
    return isinstance(before, dict) and isinstance(after, dict) and before == after
