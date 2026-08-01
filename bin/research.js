"use strict";
// ESTELLE RESEARCH FROM THE TERMINAL — the enrolment switch, the scan, and the grounded fix.
//
// The Vendor Drift Monitor works on production: the watchlist store is durable, the injection guard is
// measured, the scheduler ticks. What it did not have was a door. `GET /vendor-drift/watchlist` returning
// `{"cadence":"off","enrolled":false}` is only useful to someone who already knows the endpoint exists and
// is willing to write the bearer header by hand. So the standing scan — the whole point of a MONITOR — had
// no switch a customer could flip.
//
// This is the pure half: what to send, and how to read what comes back. The impure half (the HTTP calls, and
// reading a call-site file off the developer's own disk) lives in estelle.js.
//
// WHY THE FILE TRAVELS WITH THE REPAIR REQUEST. `plan_repair` refuses to draft against a file it cannot read,
// because a live run once named the target file without carrying it and the model rewrote a different module
// entirely. The server holds the repo as a chunked grounding surface, not as bytes, so reconstructing the
// file server-side would hand the model a subtly WRONG copy of its own code. The CLI has the real file. It
// sends it, exactly as `estelle gate` sends the real staged diff.

//: The cadences the server accepts. Mirrored here only so a typo fails in the terminal with a readable list
//: instead of being silently stored as `off` — the server is still the enforcement point.
const CADENCES = ["off", "hourly", "daily", "weekly"];

/** The PUT body for `estelle research watch stripe sentry --cadence daily`.
 *
 * An omitted cadence is OMITTED from the body rather than defaulted here: the server keeps what is stored
 * when the key is absent, so defaulting it in the client would silently overwrite a customer's weekly
 * schedule every time they added a vendor. An unknown cadence returns `null` so the caller can refuse
 * loudly — resolving it to `off` here would turn a typo into a silent un-enrolment. */
function watchBody(vendors, cadence) {
  const named = (vendors || []).map((v) => String(v || "").trim().toLowerCase()).filter(Boolean);
  const body = {};
  if (cadence !== undefined && cadence !== null && String(cadence).trim()) {
    const wanted = String(cadence).trim().toLowerCase();
    if (!CADENCES.includes(wanted)) return null;
    body.cadence = wanted;
  }
  if (named.length) body.vendors = named.map((name) => ({ name }));
  return body;
}

/** What the account is enrolled in, as lines. Says NOT ENROLLED loudly, because the failure mode this whole
 *  command exists for is an account that believes it is being watched and is not. */
function watchlistLines(payload) {
  const p = payload || {};
  const vendors = Array.isArray(p.vendors) ? p.vendors : [];
  const enrolled = p.enrolled === true;
  const lines = [enrolled
    ? `Watching ${vendors.length} vendor(s), ${p.cadence || "daily"}`
    : "NOT enrolled — nothing is being watched on a schedule."];
  for (const v of vendors) {
    const name = typeof v === "string" ? v : (v && v.name) || "";
    const apis = v && Array.isArray(v.apis) ? v.apis : [];
    if (name) lines.push(`  ${name}${apis.length ? `  ${apis.slice(0, 6).join(" ")}` : ""}`);
  }
  if (!enrolled) {
    lines.push("Run: estelle research watch stripe openai --cadence daily");
  }
  if (Array.isArray(p.rejected) && p.rejected.length) {
    // A PUT that stored four of the five vendors you sent must never read as a PUT that stored five.
    lines.push(`Not watching ${p.rejected.join(", ")} — pass --api <symbol> for a vendor Estelle doesn't know.`);
  }
  return lines;
}

/** A scan result as lines. The three outcomes are kept APART on purpose: findings, a clean run, and a run
 *  that resolved no vendors at all. Only the middle one is good news, and the previous version of this
 *  monitor stamped the third as if it were the second. */
function driftLines(payload) {
  const p = payload || {};
  const findings = Array.isArray(p.findings) ? p.findings : [];
  const scanned = Number(p.vendors_scanned) || 0;
  if (!scanned) {
    return [String(p.note || "Nothing was scanned — no vendor list resolved for this account.")];
  }
  const lines = [`Scanned ${scanned} vendor(s) from your ${p.source || "watchlist"}.`];
  if (!findings.length) lines.push("No drift found — every watched API still exists as you call it.");
  for (const f of findings) {
    lines.push(`${f.vendor}.${f.api} is ${f.kind}`);
    for (const site of (f.usage_sites || []).slice(0, 5)) lines.push(`    ${site}`);
  }
  if (Array.isArray(p.quarantined) && p.quarantined.length) {
    // "We refused to read that source" is not "that dependency is fine", and the difference is the product.
    lines.push(`${p.quarantined.length} source(s) QUARANTINED (they tried to instruct the model) — not judged.`);
  }
  if (p.note && scanned) lines.push(String(p.note));
  return lines;
}

/** Every distinct local file a set of findings points at — what the repair request must carry. Deduped, in
 *  first-seen order, and capped: this list becomes file reads and then prompt bytes. */
function targetPaths(findings, cap = 6) {
  const seen = [];
  for (const f of findings || []) {
    for (const site of (f && f.usage_sites) || []) {
      const path = String(site || "").trim();
      if (path && !seen.includes(path)) seen.push(path);
      if (seen.length >= cap) return seen;
    }
  }
  return seen;
}

/** The `POST /vendor-drift/repair` body: the findings we already have (so the scan is not paid for twice)
 *  and the real text of the call sites. `sources` omits anything that could not be read — the server refuses
 *  by name for a missing target, which is a better answer than an empty string pretending to be a file. */
function repairBody(findings, sources, draft) {
  const body = { findings: findings || [] };
  const usable = {};
  for (const [path, text] of Object.entries(sources || {})) {
    if (typeof text === "string" && text.trim()) usable[path] = text;
  }
  if (Object.keys(usable).length) body.sources = usable;
  if (draft === true) body.draft = true;
  return body;
}

/** A repair response as lines: for each plan, whether it may proceed, what it was grounded on, and — when it
 *  refused — the refusal IN FULL. The refusal is the most useful line on the screen (it says whether to go
 *  fix the query or accept that the vendor published nothing), so it is never truncated. */
function repairLines(payload) {
  const p = payload || {};
  const plans = Array.isArray(p.plans) ? p.plans : [];
  if (!plans.length) return [String(p.note || "No repair plan — run estelle research drift first.")];
  const lines = [`${p.proceed || 0} of ${p.planned || 0} finding(s) have a grounded fix. Nothing was applied.`];
  for (const plan of plans) {
    const f = plan.finding || {};
    lines.push(`${f.vendor}.${f.api} (${f.kind}) → ${plan.target || "no call site"}`);
    const research = plan.research || {};
    if (research.urls || plan.urls) {
      for (const url of (plan.urls || research.urls || []).slice(0, 4)) lines.push(`    grounded on ${url}`);
    }
    if (!plan.proceed) {
      lines.push(`    REFUSED: ${plan.refused || "no reason given"}`);
    } else {
      lines.push(`    ready: a ${plan.task_chars || 0}-char grounded task, ${research.pages_read || 0} page(s) read`);
      const draft = plan.draft;
      if (draft && draft.drafted) {
        lines.push(draft.no_change
          ? "    drafted: NO CHANGE — the model refused to invent an API the docs never showed"
          : `    drafted: ${draft.path}  gate ${draft.gate && draft.gate.verdict ? draft.gate.verdict : "unknown"}`);
      } else if (draft) {
        lines.push(`    draft unavailable: ${draft.reason || "unknown"}`);
      }
    }
  }
  return lines;
}

// ── the commands ────────────────────────────────────────────────────────────────────────────────────
// Same shape as monitor.js: `estelle.js` dispatches and hands each command a CTX carrying the HTTP verbs,
// the colours, the argv accessors and the output sink.

/** `estelle research [watch|off|drift|repair|ask]`. */
async function run(ctx) {
  ctx.banner();
  const key = ctx.needKey();
  if (!key) return;
  const sub = (ctx.argv[3] || "").trim();
  if (sub === "watch") return watch(ctx, key);
  if (sub === "off") return off(ctx, key);
  if (sub === "drift") return drift(ctx, key);
  if (sub === "repair") return repair(ctx, key);
  if (sub === "ask") return ask(ctx, key);
  return status(ctx, key);
}

/** The Ultra/Team gate and the missing-web-key refusal are the two answers a customer actually hits here,
 *  and both are ACTIONABLE — so they are rendered as the server's own instruction, not as an HTTP code. */
function refused(ctx, r) {
  if (r.status === 402) {
    ctx.out("  " + ctx.c.amber(ctx.errText(r) || "Vendor drift is an Ultra/Team capability."));
    return ctx.markFailed();
  }
  return ctx.failWith(r);
}

async function status(ctx, key) {
  const c = ctx.c;
  const r = await ctx.get("/vendor-drift/watchlist", key);
  if (!r.ok) return refused(ctx, r);
  ctx.out("  " + c.bold("Vendor watch"));
  for (const line of watchlistLines(r.json)) {
    ctx.out("  " + (line.startsWith("Run:") || line.startsWith("NOT") ? c.amber(line) : line));
  }
  ctx.out("");
  ctx.out("  " + c.dim("estelle research drift · repair · watch <vendor…> --cadence daily · off"));
  ctx.out("");
}

async function watch(ctx, key) {
  const c = ctx.c;
  const vendors = ctx.argText(4).split(/[\s,]+/).filter(Boolean);
  const body = watchBody(vendors, ctx.flag("--cadence", vendors.length ? "daily" : ""));
  if (body === null) {
    ctx.out("  " + c.amber(`Unknown cadence — use one of: ${CADENCES.join(", ")}`));
    return ctx.markFailed();
  }
  // A vendor Estelle has never heard of is REJECTED server-side unless the caller names the symbols they
  // call — a watched vendor with no APIs produces a scan that can only ever come back clean.
  const named = ctx.flag("--api", "");
  if (named && body.vendors && body.vendors.length === 1) {
    body.vendors[0].apis = named.split(/[\s,]+/).filter(Boolean);
  }
  const r = await ctx.put("/vendor-drift/watchlist", body, key);
  if (!r.ok) return refused(ctx, r);
  for (const line of watchlistLines(r.json)) ctx.out("  " + line);
  ctx.out("");
}

async function off(ctx, key) {
  const c = ctx.c;
  const r = await ctx.put("/vendor-drift/watchlist", { cadence: "off" }, key);
  if (!r.ok) return refused(ctx, r);
  ctx.out("  " + c.green("Vendor watch is off.") + c.dim("  Your vendor list is kept — re-enrol any time."));
  ctx.out("");
}

async function drift(ctx, key) {
  const c = ctx.c;
  ctx.write("  " + c.dim("Scanning your vendors' live docs… "));
  const r = await ctx.post("/vendor-drift", {}, key);
  if (!r.ok) { ctx.out(""); return refused(ctx, r); }
  ctx.out(c.ok); ctx.out("");
  for (const line of driftLines(r.json)) ctx.out("  " + line);
  ctx.out("");
  if (ctx.asArray(r.json && r.json.findings).length) {
    ctx.out("  " + c.dim("Ground a fix on the vendor's live docs: ") + c.teal("estelle research repair"));
    ctx.out("");
  }
}

async function repair(ctx, key) {
  const c = ctx.c;
  // Scan FIRST, then send the real call sites. Two calls rather than one because the second needs files that
  // only this machine has — the server holds a chunked grounding surface, not the bytes of your repo, and a
  // rewrite authored against a reconstructed file is how a drift fix rewrites the wrong module.
  ctx.write("  " + c.dim("Finding what drifted… "));
  const scan = await ctx.post("/vendor-drift", {}, key);
  if (!scan.ok) { ctx.out(""); return refused(ctx, scan); }
  const findings = ctx.asArray(scan.json && scan.json.findings);
  ctx.out(c.ok);
  if (!findings.length) {
    ctx.out("");
    for (const line of driftLines(scan.json)) ctx.out("  " + line);
    ctx.out("");
    return;
  }
  const sources = {};
  const missing = [];
  for (const rel of targetPaths(findings)) {
    const text = ctx.readFile(rel);
    if (text === null) missing.push(rel); else sources[rel] = text;
  }
  ctx.announceSkipped(`${missing.length} call site(s) are not on this machine — run this from the repo root:`,
                      missing, "Estelle refuses to rewrite a file whose current contents it cannot see.");
  ctx.write("  " + c.dim("Researching the vendor's current API… "));
  const r = await ctx.post("/vendor-drift/repair", repairBody(findings, sources, ctx.has("--draft")), key);
  if (!r.ok) { ctx.out(""); return refused(ctx, r); }
  ctx.out(c.ok); ctx.out("");
  for (const line of repairLines(r.json)) ctx.out("  " + line);
  ctx.out("");
  ctx.out("  " + c.dim("Nothing was applied. Add ") + c.bold("--draft")
    + c.dim(" to have Estelle author the change for review."));
  ctx.out("");
}

async function ask(ctx, key) {
  const c = ctx.c;
  const question = ctx.argText(4);
  if (!question) {
    ctx.out("  " + c.amber("Ask what?") + c.dim('  estelle research ask "how does billing retry?"'));
    return;
  }
  ctx.write("  " + c.dim("Researching your codebase… "));
  const repo = ctx.flag("--repo", "");
  const r = await ctx.post("/deep-search", repo ? { question, repo } : { question }, key);
  if (!r.ok) { ctx.out(""); return refused(ctx, r); }
  ctx.out(c.ok); ctx.out("");
  const v = r.json || {};
  if (v.scope_ask) {
    // Several swept repos and nothing settles which — Estelle asks rather than blending two codebases.
    ctx.out(ctx.indent(String(v.question || "Which repo should I answer about?")));
    ctx.out("");
    ctx.out("  " + c.dim('Re-run with the repo: estelle research ask "…" --repo owner/name'));
    ctx.out("");
    return;
  }
  ctx.out(v.answer ? ctx.indent(v.answer) : "  " + c.dim("(no answer)"));
  const cites = ctx.asArray(v.sources);
  if (cites.length) {
    ctx.out(""); ctx.out("  " + c.bold("Cited"));
    // The server's `sources` records are `{file, line?}` (deep_search.cited_sources). Rendering the raw JSON
    // blob was wrong and it SHOWED on the first live probe — a citation a human cannot read does not cite.
    for (const cite of cites.slice(0, 8)) {
      if (typeof cite === "string") { ctx.out("  " + c.dot + " " + c.teal(cite)); continue; }
      const where = cite.file || cite.source_file || cite.path || "";
      ctx.out("  " + c.dot + " " + c.teal(where + (cite.line ? `:${cite.line}` : "")));
    }
  }
  // "Grounded in your repo, and NOT the web" is the honest boundary of this command — deep-search never
  // leaves the codebase, and a reader who assumes otherwise is the whole reason this line exists.
  if (v.grounded === false) ctx.out("  " + c.amber("Not grounded — Estelle did not certify this answer."));
  ctx.out("");
}

module.exports = { CADENCES, watchBody, watchlistLines, driftLines, targetPaths, repairBody, repairLines,
                   run };
