"use strict";
// ESTELLE MONITOR FROM THE TERMINAL — the only first-party door this suite has.
//
// Every Monitor read path works on production and is entitlement-gated correctly. None of it was REACHABLE:
// there is no dashboard page (that lives in the web worktree), so the sole way a paying customer could see
// their own production errors was a hand-written curl with a bearer header. A capability with no door is
// indistinguishable, from the customer's chair, from a capability that does not exist.
//
// This file is the pure half of that door: it shapes the request bodies and renders what comes back. The
// terminal wiring lives in estelle.js, exactly as github-connect.js splits from `estelle github`. Pure means
// the rendering decisions — how a down check reads, what an empty issue list SAYS instead of printing
// nothing, whether a repair status is worth a column — are unit-testable without a server.
//
// ONE RULE RUNS THROUGH ALL OF IT: an empty result must say WHY it is empty. "No issues" over a monitor that
// was never wired up is a green dashboard that has measured nothing, which is the failure this whole product
// exists to refuse. So every list renderer distinguishes "nothing has been sent to Estelle yet" from
// "Estelle has your telemetry and nothing is wrong".

/** A unix timestamp as a compact relative age ("4m ago"). `""` for a missing/garbage stamp — never "0s ago",
 *  which reads as "just now" for a check that has never run. */
function ago(ts, now) {
  const at = Number(ts);
  if (!Number.isFinite(at) || at <= 0) return "";
  const secs = Math.max(0, Math.round((Number(now) || 0) / 1000 - at));
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.round(secs / 3600)}h ago`;
  return `${Math.round(secs / 86400)}d ago`;
}

/** One issue as a single line: what broke, where the code graph says it lives, how often, and whether a fix
 *  is already proposed. The SYMBOL is the differentiator — a telemetry-only tool cannot print that column —
 *  so it leads when it bound. */
function issueLine(issue, now) {
  const i = issue || {};
  const where = i.symbol || i.culprit || i.transaction || "";
  const seen = ago(i.last_seen, now);
  const bits = [`${i.count || 0}x`];
  if (seen) bits.push(seen);
  if (i.status && i.status !== "unresolved") bits.push(i.status);
  // A repair that was merely ATTEMPTED is not news; one that produced a reviewable PR is the whole point.
  if (i.repair_status === "pr" && i.repair_pr) bits.push(i.repair_pr);
  else if (i.repair_status && i.repair_status !== "none") bits.push(`repair: ${i.repair_status}`);
  return { title: i.title || i.error_type || "(untitled)", where, meta: bits.join("  ·  "), key: i.key || "" };
}

/** The issue list a human reads, or the honest explanation of an empty one.
 *  `{ lines: [{title, where, meta, key}], empty: "" | <why> }`. */
function issueView(payload, now) {
  const rows = payload && Array.isArray(payload.issues) ? payload.issues : [];
  if (!rows.length) {
    const counts = (payload && payload.counts) || {};
    // events > 0 with no issues means retention pruned them — a materially different fact from "you have
    // never sent us anything", and the one that would otherwise be silently misread as a clean system.
    return { lines: [], empty: Number(counts.events) > 0
      ? "No issues in the current retention window — earlier ones aged out."
      : "No errors have reached Estelle yet. Point your OTLP or Sentry SDK at api.fatelabs.ca/monitor/ingest." };
  }
  return { lines: rows.map((r) => issueLine(r, now)), empty: "" };
}

/** The one-screen health summary: how much is flowing, how many issues, what is currently firing, what is
 *  down. Returns plain `{label, value}` rows so the terminal formatting stays in estelle.js. */
function overviewRows(payload) {
  const p = payload || {};
  const counts = p.counts || {};
  const alerts = p.alerts || {};
  const uptime = p.uptime || {};
  const rows = [
    { label: "Issues", value: `${counts.issues || 0}  (${counts.bound || 0} bound to a symbol)` },
    { label: "Events", value: String(counts.events || 0) },
    { label: "Logs", value: String((p.logs || {}).logs || 0) },
    { label: "Traces", value: String((p.traces || {}).traces || 0) },
    { label: "Metrics", value: String((p.metrics || {}).metrics || 0) },
    { label: "Alert rules", value: `${alerts.rules || 0}  (${alerts.enabled || 0} enabled, ${alerts.active || 0} firing)` },
    { label: "Uptime checks", value: `${uptime.checks || 0}  (${uptime.up || 0} up, ${uptime.down || 0} down)` },
  ];
  return rows;
}

/** The currently-FIRING alerts, newest breach last. Empty array when nothing is firing — the caller prints
 *  the reassuring line, because "nothing firing" and "no rules exist" are different and both matter. */
function firingLines(payload) {
  const active = payload && Array.isArray(payload.active) ? payload.active : [];
  return active.map((a) => `${a.name || a.kind || "rule"}  ${a.observed} ${a.comparator || ">"} ${a.threshold}`);
}

/** Alert RULES as lines, plus whether any exist at all. */
function alertView(payload) {
  const rules = payload && Array.isArray(payload.rules) ? payload.rules : [];
  return {
    rules: rules.map((r) => ({
      id: r.id || "",
      text: `${r.name || r.kind || "rule"}  ${r.kind || ""} ${r.comparator || ">"} ${r.threshold}`
        + (r.enabled === false ? "  (disabled)" : ""),
    })),
    firing: firingLines(payload),
  };
}

/** Uptime checks with their current state. `up: null` for a check that has never run — rendered as "pending"
 *  rather than as "up", because a check that has not run yet has proven nothing. */
function uptimeView(payload, now) {
  const rows = payload && Array.isArray(payload.status) ? payload.status : [];
  return rows.map((r) => {
    const ran = ago(r.last_checked, now);
    const state = !ran ? "pending" : (r.up ? "up" : "DOWN");
    const detail = [r.last_status ? `HTTP ${r.last_status}` : "", r.last_latency_ms ? `${Math.round(r.last_latency_ms)}ms` : "",
                    ran].filter(Boolean).join("  ·  ");
    return { id: r.check_id || "", name: r.name || r.url || "", url: r.url || "", state, detail,
             enabled: r.enabled !== false };
  });
}

/** The query string for the log explorer — only the parameters the caller actually supplied, so an omitted
 *  filter means "the server's default" rather than an empty-string filter that matches nothing. */
function logQuery({ query = "", level = "", window = "" } = {}) {
  const parts = [];
  if (String(query || "").trim()) parts.push(`query=${encodeURIComponent(String(query).trim())}`);
  if (String(level || "").trim()) parts.push(`level=${encodeURIComponent(String(level).trim())}`);
  if (String(window || "").trim()) parts.push(`window=${encodeURIComponent(String(window).trim())}`);
  return parts.length ? `/monitor/logs?${parts.join("&")}` : "/monitor/logs";
}

/** Log lines for the terminal, newest first as the server returns them. */
function logLines(payload, now) {
  const rows = payload && Array.isArray(payload.logs) ? payload.logs : [];
  return rows.map((l) => ({
    level: String(l.level || "info").toUpperCase(),
    when: ago(l.timestamp, now),
    body: String(l.message || "").replace(/\s+/g, " ").slice(0, 160),
    service: l.service || "",
  }));
}

// ── the commands ────────────────────────────────────────────────────────────────────────────────────
// `estelle.js` stays a dispatcher and hands each command a CTX: the HTTP verbs, the terminal colours, the
// argv accessors and the output sink. The commands live beside the renderers they use rather than in the
// entry point, which is what keeps that file a table of contents instead of a 1,700-line pile.

/** `estelle monitor [issues|issue|alerts|uptime|logs]` — the whole suite from a terminal. */
async function run(ctx) {
  ctx.banner();
  const key = ctx.needKey();
  if (!key) return;
  const sub = (ctx.argv[3] || "").trim();
  if (sub === "issues") return issues(ctx, key);
  if (sub === "issue") return issue(ctx, key, ctx.argv[4]);
  if (sub === "alerts") return alerts(ctx, key);
  if (sub === "uptime") return uptime(ctx, key);
  if (sub === "logs") return logs(ctx, key);
  return status(ctx, key);
}

/** A monitor read that answered 402/503 is not an empty monitor — say WHICH it was. "0 issues" printed over
 *  an unentitled account, or over a deployment with Monitor switched off, is a green dashboard that has
 *  measured nothing, and that is the one thing this product exists to refuse. */
function refused(ctx, r) {
  const c = ctx.c;
  if (r.status === 402) {
    ctx.out("  " + c.amber("Estelle Monitor is a Pro feature on this account.") + c.dim("  fatelabs.ca/pricing"));
    return ctx.markFailed();
  }
  if (r.status === 503) {
    ctx.out("  " + c.amber("Monitor is not enabled on this deployment — nothing is being collected."));
    return ctx.markFailed();
  }
  return ctx.failWith(r);
}

async function status(ctx, key) {
  const c = ctx.c;
  const r = await ctx.get("/monitor/overview", key);
  if (!r.ok) return refused(ctx, r);
  ctx.out("  " + c.bold("Production") + c.dim("  what Estelle is holding for this account"));
  ctx.out("");
  for (const row of overviewRows(r.json)) ctx.out("  " + c.dim(row.label.padEnd(16)) + row.value);
  const firing = firingLines(r.json && { active: r.json.active_alerts });
  if (firing.length) {
    ctx.out("");
    ctx.out("  " + c.red("Firing now"));
    for (const line of firing) ctx.out("    " + c.red("!") + " " + line);
  }
  ctx.out("");
  ctx.out("  " + c.dim("estelle monitor issues · issue <key> · alerts · uptime · logs [text]"));
  ctx.out("");
}

async function issues(ctx, key) {
  const c = ctx.c;
  const symbol = ctx.flag("--symbol", "");
  const r = await ctx.get(symbol ? "/monitor/issues?symbol=" + encodeURIComponent(symbol) : "/monitor/issues", key);
  if (!r.ok) return refused(ctx, r);
  const view = issueView(r.json, ctx.now());
  if (view.empty) { ctx.out("  " + c.dim(view.empty)); ctx.out(""); return; }
  ctx.out("  " + c.bold(symbol ? `Issues owned by ${symbol}` : "Issues") + c.dim("  newest first"));
  ctx.out("");
  for (const line of view.lines) {
    ctx.out("  " + c.arrow + " " + c.bold(line.title) + (line.where ? c.teal("  " + line.where) : ""));
    ctx.out("    " + c.dim(line.meta) + (line.key ? c.grey("   " + line.key) : ""));
  }
  ctx.out("");
  ctx.out("  " + c.dim("Open one: ") + c.teal("estelle monitor issue <key>"));
  ctx.out("");
}

async function issue(ctx, key, id) {
  const c = ctx.c;
  if (!id || id.startsWith("--")) {
    ctx.out("  " + c.amber("Which issue?") + c.dim("  estelle monitor issue <key>  (keys come from estelle monitor issues)"));
    return ctx.markFailed();
  }
  const r = await ctx.post("/monitor/issue", { key: id }, key);
  if (!r.ok) return refused(ctx, r);
  const found = (r.json || {}).issue || {};
  const line = issueLine(found, ctx.now());
  ctx.out("  " + c.bold(line.title) + (line.where ? c.teal("  " + line.where) : ""));
  ctx.out("  " + c.dim(line.meta));
  if (found.sample) ctx.out("  " + c.dim(ctx.oneLine(found.sample)));
  const events = ctx.asArray((r.json || {}).events);
  if (events.length) {
    ctx.out("");
    ctx.out("  " + c.bold("Recent occurrences") + c.dim(`  ${events.length}`));
    for (const e of events.slice(0, 10)) {
      ctx.out("    " + c.dot + " " + c.dim(ago(e.ts, ctx.now()) || "—")
        + "  " + c.grey(ctx.oneLine(e.message || e.error_type || "")));
    }
  }
  ctx.out("");
}

async function alerts(ctx, key) {
  const c = ctx.c;
  const r = await ctx.get("/monitor/alerts", key);
  if (!r.ok) return refused(ctx, r);
  const view = alertView(r.json);
  ctx.out("  " + c.bold("Alert rules"));
  if (!view.rules.length) ctx.out("  " + c.dim("None. Nothing will page you when production breaks."));
  for (const rule of view.rules) ctx.out("  " + c.dot + " " + rule.text + c.grey("   " + rule.id));
  if (view.firing.length) {
    ctx.out("");
    ctx.out("  " + c.red("Firing now"));
    for (const line of view.firing) ctx.out("    " + c.red("!") + " " + line);
  } else if (view.rules.length) {
    ctx.out("  " + c.green("Nothing firing."));
  }
  ctx.out("");
}

async function uptime(ctx, key) {
  const c = ctx.c;
  const r = await ctx.get("/monitor/uptime", key);
  if (!r.ok) return refused(ctx, r);
  const rows = uptimeView(r.json, ctx.now());
  ctx.out("  " + c.bold("Uptime checks"));
  if (!rows.length) ctx.out("  " + c.dim("None registered."));
  for (const row of rows) {
    const state = row.state === "up" ? c.green("up") : row.state === "DOWN" ? c.red("DOWN") : c.amber("pending");
    ctx.out("  " + c.dot + " " + state + "  " + c.bold(row.name) + (row.enabled ? "" : c.dim("  (disabled)")));
    ctx.out("    " + c.dim(row.url) + (row.detail ? c.dim("   " + row.detail) : ""));
  }
  ctx.out("");
}

async function logs(ctx, key) {
  const c = ctx.c;
  const r = await ctx.get(logQuery({ query: ctx.argText(4), level: ctx.flag("--level", ""),
                                     window: ctx.flag("--window", "") }), key);
  if (!r.ok) return refused(ctx, r);
  const rows = logLines(r.json, ctx.now());
  if (!rows.length) { ctx.out("  " + c.dim("No log lines matched.")); ctx.out(""); return; }
  for (const row of rows) {
    const level = row.level === "ERROR" || row.level === "FATAL" ? c.red(row.level) : c.dim(row.level);
    ctx.out("  " + level.padEnd(12) + c.dim(row.when.padEnd(10)) + row.body
      + (row.service ? c.grey("  " + row.service) : ""));
  }
  ctx.out("");
}

module.exports = { ago, issueLine, issueView, overviewRows, alertView, firingLines, uptimeView, logQuery,
                   logLines, run };
