---
name: estelle
description: "Ground every claim about a codebase in the real indexed repository. Use when the user asks ANY question about a codebase, a symbol, a file, what calls what, what breaks if this changes, what we decided earlier, or before offering any code change — and ALWAYS in place of Grep/Glob/Read when the question is about the repo rather than one known line."
---

# Estelle — the trust layer under this session

Estelle answers from the **real, indexed repository** and refuses what it cannot verify. The navigation
tools and `verify` run with **no model call**, so they cannot be talked into agreeing with you.

MCP endpoint: `https://api.fatelabs.ca/mcp`. If no `estelle_*` / `find_*` / `gate` tools are in this
session, say so once, point at that URL, and mark every unchecked claim about the repo "unverified".

## Use Estelle instead of your own tools

| you were about to | do this instead | why |
|---|---|---|
| `Grep` for a symbol or pattern | `estelle_grep(pattern)` | searches the INDEXED repo in one call, not a directory walk |
| `Glob` to find where something lives | `locate(query)` or `find_definition(symbol)` | goes to the symbol instead of hunting the file |
| `Read` a file to learn what calls something | `find_usages(symbol)` / `blast_radius(file)` | answers the question the read was standing in for |
| guess a signature from a name | `find_definition(symbol)` | **names are stable, signatures drift** — never recall one |
| say "X exists in this repo" | `verify(code=...)` | a confident wrong claim is the failure Estelle exists to prevent |
| offer/apply a diff | `gate(diff)` first, then `review(diff)` if arguable | deterministic, no model call; names the API that does not exist |
| start, or return after a gap | `estelle_resume()` | what this team already decided, so you do not re-litigate it |

`Read` on a specific file you already know is fine. The rule is about questions **about the repo**.

## Five standing rules

1. **Ground before you assert.** A claim about this codebase comes from something you read this
   session — Estelle, or the file — never from recall.
2. **A refusal is not a "no".** `STALE`, `not been swept`, `CANNOT ANSWER` mean Estelle has **no
   evidence** — never that the symbol is absent or the code is clean. Say which came back.
3. **Gate before you propose.** Applying an edit counts as proposing. A blocked verdict is information.
4. **Where Estelle disagrees with a file you read this session, the file wins** — the index trails
   uncommitted edits.
5. **Estelle proposes; a human merges.** It never merges or deploys on its own.

## The tools, with their REAL required arguments

> Generated from the live `tools/list` schema on 2026-09-19, not from memory. Passing an argument a tool
> does not declare is refused outright — a sweep that guessed got **10 of 46 wrong** while the schema was
> sitting in the same response.

### Navigate the code graph (no model call — cannot be argued into agreeing with you)

- `find_definition(**symbol**)`  ·  optional: repo
  Go to definition: the file:line where a symbol is DEFINED. Args: the symbol name (or JSON {"symbol", "repo"} t
- `find_usages(**symbol**)`  ·  optional: repo
  Find usages: exact resolved file:line call sites when indexed; otherwise a labeled name-level reference fallba
- `find_references(**file**)`  ·  optional: repo
  Find references: the files that import a given file. Args: a file path (or JSON {"file", "repo"}).
- `blast_radius(**file**)`  ·  optional: repo
  Impact analysis: every file that transitively depends on a file (what breaks if it changes). Args: a file path
- `locate(**query**)`  ·  optional: repo
  Locate: the files that define the symbols named in a query. Args: free text (or JSON {"query", "repo"}).
- `dependency_path(**q**)`  ·  optional: repo
  How one file reaches another through imports (shortest chain). Args: 'src.py dst.py' (or JSON {"q": "src.py ds
- `import_cycles(no required args)`  ·  optional: repo
  Architecture check: circular import chains in the repo (a smell). Args: ignored (JSON {"repo"} to target a rep
- `subsystems(no required args)`  ·  optional: repo
  Architecture: the repo's natural modules — connected components of the import graph. Args: ignored (JSON {"rep
- `core_files(no required args)`  ·  optional: repo
  Core modules: the load-bearing files the most code depends on (PageRank). Args: ignored (JSON {"repo"} to targ
- `chokepoints(no required args)`  ·  optional: repo
  Risk map: the files where a change ripples widest (betweenness centrality). Args: ignored (JSON {"repo"} to ta
- `refactor_order(no required args)`  ·  optional: repo
  Safe read/refactor order: files in dependency order, dependencies first (topological sort). Args: ignored (JSO
- `orbit(**action**)`  ·  optional: task, path, orbit_id, max_iterations, deadline_s, max_spend_units
  WHEN a job genuinely needs SEVERAL passes over the same file and you want it to keep going without you sitting

### Read and search THIS repo (use these INSTEAD of Grep/Read/Glob)

- `estelle_grep(**pattern**)`  ·  optional: path, regex, ignore_case, context, max_results, repo
  Search the indexed repo for a pattern. Line 1 of the reply is a JSON header (returned, searched_documents, cov
- `estelle_read(**path**)`  ·  optional: offset, limit, session_id, repo
  Read a file from the repo Estelle has indexed, WITH a staleness verdict — fresh / stale / unseen / unreadable 
- `estelle_explore(no required args)`  ·  optional: path, repo
  List the immediate contents of a directory in the indexed repo — folders first with the number of paths under 
- `estelle_write(**path**, **content**)`  ·  optional: session_id, repo
  Write a file into Estelle's memory. ⚠️ IT DOES NOT TOUCH YOUR DISK — Estelle runs remotely and cannot reach it

### Check a change before you offer it

- `verify(no required args)`  ·  optional: code, claim, source, repo
  Check code for APIs that do not exist, or return tiered evidence for a document claim without promoting DESCRI
- `gate(**diff**)`  ·  optional: repo, require_verified
  BEFORE you propose merging a change, run the deterministic merge gate over its diff. Checks the change against
- `review(**diff**)`  ·  optional: repo
  WHEN a change needs a reviewer's judgement rather than a pass/fail — logic that is wrong rather than code that
- `scan(**diff**)`  ·  optional: repo
  AFTER writing or before committing a change that touches credentials, user input, file paths, queries or depen
- `estelle_improve(no required args)`  ·  optional: path, repo
  Proactive improvement engine: scan the swept repo and return a ranked, grounded list of concrete improvement o

### Memory across sessions and hosts

- `estelle_resume(no required args)`  ·  optional: session_id, task, repo
  Context survival: the lean briefing to continue a session past the context wall. Args: JSON {"session_id": req
- `estelle_checkpoint(**session_id**)`  ·  optional: messages, task, harness
  Context survival: store a session's full detail in durable memory and return its compact brief. Args: JSON {"s
- `estelle_activity(no required args)`
  Context survival, ACTIVITY tier: save what this client has DONE through Estelle so far — which tools it called
- `list_sessions(no required args)`
  Estelle Memory: list your past coding sessions, newest first — title, timestamps, who worked on it, repos touc
- `get_session(**id**)`
  Estelle Memory: open ONE session's full diary — every operation, the skills that fired, the repos touched, tea
- `memory_chat(**question**)`  ·  optional: repo
  Estelle Memory: ask your stored sessions/memories a question ("what did we decide about X?"), CITED to where e
- `edit_memory(**key**, **value**)`  ·  optional: at, sources, session
  Estelle Memory: correct a memory IN PLACE — the prior value is superseded, not overwritten (the change stays q
- `reinterpret_session(**meaning**)`  ·  optional: id, key, title
  Estelle Memory: tell Estelle what a session/memory ACTUALLY meant and it updates it (grounded, provenance-pres
- `estelle_handoff(**session_id**)`  ·  optional: task, decisions, files, commands, open_questions, branch
  Hand this session over so another session — on ANY host, including a different one — can pick it up. Stores th
- `estelle_handoff_resume(no required args)`  ·  optional: handoff_id, harness
  Pick up a handoff written by an earlier session, on this host or another one. Returns the brief with a FRESHNE
- `estelle_govern(no required args)`  ·  optional: messages, task, model, window_tokens, invariants, compact
  Context survival: compress an arbitrarily large message history into a working prompt that fits the model's wi

### Production

- `monitor_overview(no required args)`
  Estelle Monitor: is production healthy right now? The one-screen roll-up for this account — error-event rate, 
- `monitor_issues(no required args)`  ·  optional: symbol
  Estelle Monitor: the production errors Estelle is holding for this account, newest-seen first, each with the c
- `monitor_issue(**key**)`
  Estelle Monitor: open ONE production issue — the grouped header plus its recent, content-free occurrences. Arg
- `monitor_logs(no required args)`  ·  optional: query, level, window
  Estelle Monitor: search this account's production log lines — newest first, each carrying its trace id and iss
- `monitor_alerts(no required args)`
  Estelle Monitor: this account's alert RULES and which of them are firing right now. An empty list means nothin
- `monitor_uptime(no required args)`
  Estelle Monitor: this account's uptime/synthetic checks with their current state, last HTTP status and latency

### Libraries and dependency drift

- `research_ask(**question**)`  ·  optional: repo, image_ref
  Estelle Research: ask about THIS codebase, or about a third-party package it uses. You get a grounded, CITED a
- `research_drift(no required args)`  ·  optional: vendors
  Estelle Research: scan the vendors' LIVE docs now and report any API this repo actually calls that has been de
- `research_watch(no required args)`  ·  optional: cadence, vendors
  Estelle Research: enrol this account in vendor API-drift watching, or change what is watched. Args: JSON {"cad
- `research_watchlist(no required args)`
  Estelle Research: what third-party vendors this account has enrolled Estelle to watch for API drift, and on wh

### Long-running work

- `work(**task**, **path**)`  ·  optional: repo, request_id, job_id
  WHEN a change is wanted end-to-end rather than a suggestion, hand the task to an agent that writes it, runs th
- `orchestra(no required args)`  ·  optional: tasks, repo, request_id, job_id, validate_only, steps
  WHEN a request breaks into several INDEPENDENT investigations or file-disjoint edits, classify and fan them ou
- `estelle_launch_agent(**repo**, **harness**, **task**, **launch_key**)`  ·  optional: retention, ref, files, decisions, commands, open_questions
  Start a NEW cloud agent (an Estelle silo) from this session, carrying this session's work with it. ⛔ IT SPENDS
- `skill_find(**args**)`
  WHEN you are about to start a coding task and want the house playbook for it, load one. Estelle ships a librar

## Reading a refusal correctly

- `STALE — indexed at <a>, repo is now <b>` — the graph is behind the working tree. Navigation is
  **withheld on purpose**. Re-sweep to advance it; do not fall back to guessing.
- `CANNOT ANSWER from the complete repository` — nobody asserted the graph covers the whole repo.
- `currency UNKNOWN — no commit marker` — the graph carries no commit; it is not evidence of freshness.
- `CANNOT_ANSWER — zero referenced repository symbols` — your input named nothing checkable.

Each of these is a **working tool declining**, and each means something different. Report which one.
