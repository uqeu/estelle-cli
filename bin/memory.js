"use strict";
// ERASURE FROM THE TERMINAL — and, more to the point, the PROOF of it.
//
// `POST /purge` and `POST /forget` have been the right-to-be-forgotten primitives for a while, and both
// write an append-only receipt naming the namespace, the scope, the row count and who asked. The receipt
// was the whole point of the design: a purge that returns a number nobody can check afterwards is exactly
// the failure register this product sells against. And nothing could read one back — no route, no CLI, no
// dashboard. A deletion nobody can verify is a deletion nobody will believe.
//
// This file is the WHOLE door: the pure renderers/body-shapers, plus the `run(ctx)` the shipped CLI
// dispatches `estelle memory` to — the same split monitor.js and research.js use, so the terminal I/O is
// injected and every rendering decision above stays unit-testable without a server.
//
// THE RULE THAT RUNS THROUGH IT: never make an erasure look bigger or smaller than it was. A zero-row
// retraction says so in words; an empty receipt list distinguishes "you have never erased anything" from
// "the proof could not be read"; and the namespace column is printed exactly as the server redacted it,
// because on a solo account that string is a live bearer credential and re-expanding it here would put a
// working key on the customer's scrollback.

/** An ISO-8601 stamp as a compact local date-time, or "" for a missing/garbage one — never "now". */
function when(iso) {
  const at = Date.parse(String(iso || ""));
  if (!Number.isFinite(at)) return "";
  return new Date(at).toISOString().replace("T", " ").slice(0, 16);
}

/** One receipt as a line: what scope was cleared, over what, how many rows, when, and who asked. */
function receiptLine(receipt) {
  const r = receipt || {};
  const rows = Number(r.rows) || 0;
  const bits = [`${rows} ${rows === 1 ? "chunk" : "chunks"}`];
  const stamp = when(r.deleted_at);
  if (stamp) bits.push(stamp);
  if (r.requested_by) bits.push(r.requested_by);
  if (r.reason) bits.push(r.reason);
  return {
    scope: String(r.scope || "namespace"),
    // the server hands back a REDACTED namespace prefix; printed as-is, never re-expanded
    target: String(r.target || r.namespace || ""),
    meta: bits.join("  ·  "),
  };
}

/** The receipt list a human reads, or the honest explanation of an empty one.
 *  `{ lines: [...], total: <chunks erased>, empty: "" | <why> }`. */
function receiptView(payload) {
  const p = payload || {};
  const rows = Array.isArray(p.receipts) ? p.receipts : [];
  if (!rows.length) {
    // "no receipts" and "nothing was erased" are the same statement here and it is worth saying plainly:
    // an empty proof list over an account that HAS purged would be the bug this endpoint exists to catch.
    return { lines: [], total: 0, empty: "No erasures on record for this account — nothing has been purged, forgotten or retracted." };
  }
  return {
    lines: rows.map(receiptLine),
    total: rows.reduce((sum, r) => sum + (Number(r.rows) || 0), 0),
    empty: "",
  };
}

/** How a completed erasure reads back. `purged: 0` is a TRUTHFUL answer (nothing under that subject was
 *  live), so it gets its own sentence rather than a bare "0" a reader would take for a failure.
 *
 *  A retraction spans TWO stores — the decision log `POST /facts` reads and the recall copy `/search` and
 *  `/memories` read — and the server says on the envelope when only one of them could be confirmed closed
 *  (`partial`). That case must never print the same ✓ as a completed one: a customer who believes a claim
 *  was withdrawn and is then handed it back by `POST /facts` is worse off than one who saw it fail. */
function erasureLine(payload, verb) {
  const p = payload || {};
  const rows = Number(p.purged) || 0;
  const subject = String(p.retracted || p.forgotten || "");
  const spaces = Array.isArray(p.namespaces) ? p.namespaces.length : 0;
  if (p.partial) {
    return {
      rows,
      partial: true,
      text: `${verb} "${subject}" did NOT complete — `
        + String(p.warning || "one half of it may still be served. Check POST /facts and GET /memories."),
    };
  }
  if (!rows) {
    return { rows: 0, text: `Nothing live under "${subject}" — no memory in this account held it.` };
  }
  return {
    rows,
    text: `${verb} "${subject}" — ${rows} ${rows === 1 ? "chunk" : "chunks"} across `
      + `${spaces} ${spaces === 1 ? "namespace" : "namespaces"}.`,
  };
}

/** The POST /retract body. `reason` is dropped when blank rather than sent empty — an empty reason string
 *  lands in the permanent receipt and reads as "a reason was given and it was nothing". */
function retractBody(subject, reason) {
  const body = { subject: String(subject || "").trim() };
  const why = String(reason || "").trim();
  return why ? { ...body, reason: why } : body;
}

/** The GET path for a receipts read, with an optional page cap. */
function receiptsPath(limit) {
  const n = Number(limit);
  return Number.isFinite(n) && n > 0 ? `/deletion-receipts?limit=${Math.floor(n)}` : "/deletion-receipts";
}

/** The graduated reflexes, or the honest explanation of an empty list. An account that has never had a
 *  reflex graduate is a DIFFERENT statement from one whose reflexes were all revoked, but both read the
 *  same from here, so the wording says what is true of both: nothing is currently learned. */
function learnedView(payload) {
  const rows = payload && Array.isArray(payload.instincts) ? payload.instincts : [];
  if (!rows.length) {
    return { lines: [], empty: "Estelle has not graduated any reflexes for this account yet — nothing is being applied on its own." };
  }
  return {
    lines: rows.map((s) => ({ name: String(s.name || "(unnamed)"), summary: String(s.summary || "") })),
    empty: "",
  };
}

/** The POST /unlearn body, or `null` when the caller named nothing. A skill wins over a reflex ONLY when
 *  `--skill` was given: the server refuses both at once, so this must never send both. */
function unlearnBody(skill, trigger, response) {
  const name = String(skill || "").trim();
  if (name) return { skill: name };
  const when = String(trigger || "").trim();
  const then = String(response || "").trim();
  if (!when || !then || when.startsWith("--") || then.startsWith("--")) return null;
  return { instinct: { trigger: when, response: then } };
}

/** How a revoke reads back. `removed: false` is a TRUTHFUL no-op — the reflex was never learned — and it
 *  is deliberately not styled as a failure, since a revoke has to stay safely repeatable. */
function unlearnLine(payload) {
  const p = payload || {};
  const removed = p.removed === true;
  if (p.unlearned === "skill") {
    return {
      removed,
      text: removed ? `Reset "${p.skill}" — its learned track record is back to unproven.`
                    : `Nothing learned about "${p.skill}" — no track record to reset.`,
    };
  }
  const label = `${p.trigger || ""} → ${p.response || ""}`;
  return {
    removed,
    text: removed ? `Revoked "${label}" — Estelle will not apply it again.`
                  : `No such reflex ("${label}") — nothing to revoke.`,
  };
}

// ── the command ────────────────────────────────────────────────────────────────────────────────────
// `ctx` is the injected terminal (argv, api verbs, colours, out/markFailed) — see `suiteCtx` in
// estelle.js. Nothing here touches process or the network directly, which is what keeps the decisions
// above testable.

async function run(ctx) {
  ctx.banner();
  const key = ctx.needKey();
  if (!key) return;
  const sub = (ctx.argv[3] || "").trim();
  if (sub === "receipts" || sub === "proof") return receipts(ctx, key);
  if (sub === "retract") return retract(ctx, key);
  if (sub === "forget") return forget(ctx, key);
  if (sub === "learned") return learned(ctx, key);
  if (sub === "unlearn") return unlearn(ctx, key);
  return usage(ctx);
}

function usage(ctx) {
  const c = ctx.c;
  const row = (cmd, what) => ctx.out("  " + c.teal(cmd.padEnd(34)) + c.dim(what));
  ctx.out("  " + c.bold("Your memory, and the record of what you erased from it"));
  ctx.out("");
  row("estelle memory receipts", "every erasure on record — the proof a purge happened");
  row("estelle memory retract <subject>", "stop serving a claim; the record it was held survives");
  row("estelle memory forget <source>", "delete a memory source outright (right-to-be-forgotten)");
  row("estelle memory learned", "the reflexes Estelle graduated from your own outcomes");
  row("estelle memory unlearn …", "revoke one of them, or reset a skill's track record (admin)");
  ctx.out("");
  ctx.out("  " + c.dim("A <subject>/<source> is what the dashboard and ") + c.teal("estelle recall")
    + c.dim(" call the memory's source —"));
  ctx.out("  " + c.dim("a swept file path, or ") + c.grey("key:<decision-key>") + c.dim(" for something asserted as a fact."));
  ctx.out("");
}

async function receipts(ctx, key) {
  const r = await ctx.get(receiptsPath(ctx.flag("--limit", "")), key);
  if (!r.ok) return ctx.failWith(r);
  const c = ctx.c;
  const view = receiptView(r.json);
  if (view.empty) { ctx.out("  " + c.dim(view.empty)); ctx.out(""); return; }
  ctx.out("  " + c.bold("Erasures on record")
    + c.dim(`  ${view.total} chunks destroyed across ${view.lines.length} receipts`));
  ctx.out("");
  for (const line of view.lines) {
    ctx.out("  " + c.dot + " " + c.teal(line.scope.padEnd(10)) + c.bold(line.target));
    ctx.out("    " + c.dim(line.meta));
  }
  ctx.out("");
  ctx.out("  " + c.dim("The namespace column is redacted by the server on purpose — on a solo account it is a live key."));
  ctx.out("");
}

/** The shared tail of retract/forget: one line that never makes an erasure look bigger than it was. */
function erased(ctx, payload, verb) {
  const c = ctx.c;
  const line = erasureLine(payload, verb);
  const mark = line.partial ? c.amber("!") : (line.rows ? c.green("✓") : c.dim("·"));
  ctx.out("  " + mark + " " + line.text);
  ctx.out("  " + c.dim("On record now: ") + c.teal("estelle memory receipts"));
  ctx.out("");
}

async function retract(ctx, key) {
  const subject = ctx.argv[4];
  if (!subject || subject.startsWith("--")) {
    ctx.out("  " + ctx.c.amber("Retract what?")
      + ctx.c.dim('  estelle memory retract key:deploy-target --reason "withdrawn"'));
    return ctx.markFailed();
  }
  const r = await ctx.post("/retract", retractBody(subject, ctx.flag("--reason", "")), key);
  if (!r.ok) return ctx.failWith(r);
  erased(ctx, r.json, "Retracted");
}

async function forget(ctx, key) {
  const source = ctx.argv[4];
  if (!source || source.startsWith("--")) {
    ctx.out("  " + ctx.c.amber("Forget what?") + ctx.c.dim("  estelle memory forget src/pay.py"));
    return ctx.markFailed();
  }
  const r = await ctx.post("/forget", { source }, key);
  if (!r.ok) return ctx.failWith(r);
  erased(ctx, r.json, "Forgot");
}

async function learned(ctx, key) {
  const r = await ctx.get("/instincts", key);
  if (!r.ok) return ctx.failWith(r);
  const c = ctx.c;
  const view = learnedView(r.json);
  if (view.empty) { ctx.out("  " + c.dim(view.empty)); ctx.out(""); return; }
  ctx.out("  " + c.bold("Reflexes Estelle graduated from your outcomes") + c.dim(`  ${view.lines.length}`));
  ctx.out("");
  for (const line of view.lines) {
    ctx.out("  " + c.dot + " " + c.bold(line.name));
    if (line.summary) ctx.out("    " + c.dim(line.summary));
  }
  ctx.out("");
  ctx.out("  " + c.dim("Revoke one: ") + c.teal('estelle memory unlearn "when X" "do Y"'));
  ctx.out("");
}

async function unlearn(ctx, key) {
  const c = ctx.c;
  const body = unlearnBody(ctx.flag("--skill", ""), ctx.argv[4], ctx.argv[5]);
  if (body === null) {
    ctx.out("  " + c.amber("Unlearn what?"));
    ctx.out("  " + c.dim("  a reflex:  ")
      + c.teal('estelle memory unlearn "when touching auth" "run the security suite"'));
    ctx.out("  " + c.dim("  a skill:   ") + c.teal("estelle memory unlearn --skill test-gen"));
    return ctx.markFailed();
  }
  const r = await ctx.post("/unlearn", body, key);
  if (!r.ok) return ctx.failWith(r);
  const line = unlearnLine(r.json);
  ctx.out("  " + (line.removed ? c.green("✓") : c.dim("·")) + " " + line.text);
  ctx.out("");
}

module.exports = { when, receiptLine, receiptView, erasureLine, retractBody, receiptsPath,
                   learnedView, unlearnBody, unlearnLine, run };
