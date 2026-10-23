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
EXPECTED_TOOLS_BASE = 28


def surface_ok(surface: object) -> bool:
    """True when ``surface`` is production's, judged on the invariant rather than on content."""
    if not isinstance(surface, dict):
        return False
    if surface.get("tools_base") != EXPECTED_TOOLS_BASE:
        return False
    prompts = surface.get("prompts")
    return isinstance(prompts, int) and not isinstance(prompts, bool) and prompts > 0
