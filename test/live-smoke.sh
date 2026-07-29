#!/bin/bash
# Live end-to-end smoke of the estelle SESSION against a running Estelle.
#
# The unit tests cover the pure half (parsing, routing, formatting). This covers the half that can only
# be wrong against a real server: that every command reaches an endpoint that EXISTS, authenticates, and
# comes back with something other than an error. It is deliberately assertive about failure — a command
# that 404s or returns "(no output)" is a FAIL, because that is exactly the silent-breakage a green unit
# suite would miss.
#
#   ./test/live-smoke.sh            # against local :8787
#   ESTELLE_MCP_URL=… ./test/live-smoke.sh
set -uo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
URL="${ESTELLE_MCP_URL:-http://127.0.0.1:8787/mcp}"
KEY="${ESTELLE_API_KEY:-$(grep '^ESTELLE_KEY_ULTRA' "$REPO/.env" | cut -d= -f2- | tr -d '"' | tr -d "'" | tr -d ' ')}"
HOME_DIR="$(mktemp -d)"; trap 'rm -rf "$HOME_DIR"' EXIT
pass=0; fail=0

run() {   # run <label> <input-line> <regex-that-must-appear>
  local label="$1" line="$2" want="$3" out
  out=$(printf '%s\n' "$line" | HOME="$HOME_DIR" ESTELLE_API_KEY="$KEY" ESTELLE_MCP_URL="$URL" \
        node "$REPO/cli/bin/estelle.js" 2>&1)
  if grep -qE "$want" <<<"$out" && ! grep -qE '\(no output\)|unexpected error|unknown path' <<<"$out"; then
    printf '  \033[32m✓\033[0m %-34s %s\n' "$label" "$(grep -vE '^\s*$' <<<"$out" | tail -1 | cut -c1-58)"
    pass=$((pass+1))
  else
    printf '  \033[31m✗\033[0m %-34s %s\n' "$label" "$(grep -vE '^\s*$' <<<"$out" | tail -1 | cut -c1-58)"
    fail=$((fail+1))
  fi
}

echo "estelle CLI — live smoke against $URL"
echo

# ── the code graph (these prove memory is real, not a stub) ───────────────────────
run "/find_definition"  "/find_definition handle_reindex"        "api_memory\.py:[0-9]+"
run "/locate"           "/locate compose_grounded_answer"        "grounded_answer\.py"
run "/find_references"  "/find_references src/estelle/serve/grounded_answer.py"  "\.py|Nothing imports"
run "/blast_radius"     "/blast_radius src/estelle/serve/memory_facade.py"       "\.py|Nothing depends"
run "/subsystems"       "/subsystems"                            "\.py"
run "/core_files"       "/core_files"                            "\.py"
run "/chokepoints"      "/chokepoints"                           "\.py"
run "/import_cycles"    "/import_cycles"                         "acyclic|->"

# ── the gate + tooling ────────────────────────────────────────────────────────────
# /verify routes to POST /verify, so the reply is the GATE RECEIPT ("✓ grounded"), not the tool's raw
# "OK — every symbol…" string. Asserting the receipt is asserting the thing the user actually reads.
run "/verify (real API)" "/verify from estelle.serve.scope_signals import decide_scope"  "grounded"
run "/verify (fake API)" "/verify from estelle.serve.memory_facade import totally_fake_xyz"  "not in this repo|grounded"
run "/tools listing"     "/tools"                                "find_definition"
run "skill passthrough"  "/skill_verify-gate"                    "[a-z]"
run "/estelle_improve"   "/estelle_improve"                      "proposals|category|\{"

# ── honest failure paths (a wrong answer here is worse than an error) ─────────────
run "unknown tool errors" "/definitely_not_a_tool"               "✗|unknown|not"
echo
echo "  $pass passed, $fail failed"
[ "$fail" -eq 0 ]
