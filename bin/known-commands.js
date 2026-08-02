"use strict";
// WHAT A LEADING `/` IS ALLOWED TO COST — register #93.
//
// 🔴 THE DEFECT THIS EXISTS TO KILL. Typing a slash command the CLI does not know did not error. It fell
// through `routeInput`'s `default:` to an MCP `tools/call`, and the session tried it as a SKILL first — so
// one typo cost **two network round-trips** (`POST /skill/run` → 404, then `POST /mcp` → "unknown tool")
// before the customer was told nothing at all had happened. That is the "the CLI is slow" complaint, and
// it is us: a leading slash is an unambiguous statement of intent, and answering it should cost ZERO.
//
// ⛔ THE FALL-THROUGH ITSELF IS NOT THE BUG AND MUST SURVIVE. It is why a skill added on the server today
// is callable from the CLI today, with no release (`repl.js` routeInput's `default:`). Hand-enumerating
// ~278 MCP tools and ~246 skill playbooks in the client would guarantee the two surfaces drift, which is
// the defect class this whole campaign is closing. So the rule is not "refuse anything unlisted" — it is:
//
//        REFUSE ONLY WHAT WE POSITIVELY KNOW IS NOT THERE.
//
// Hence three states, never two. `known` · `unknown` · **`unverified`** — we could not read the registry,
// so we have no basis to refuse and we fall through, saying so. That is the same three-valued discipline
// as `repoStatusLine`'s `indexed: null` and #76's "a failure to ASK is not evidence": a registry we could
// not fetch must never be rendered as a registry that is empty.
//
// Pure on purpose. The fetching lives in repl.js; every decision here is testable without a socket, and
// per E-027 the SEAM between them is tested by name rather than each half separately.

/** The registry a classification is made against.
 *
 * Every field is THREE-VALUED by construction: a `Set` means "we asked and this is the answer", `null`
 * means "we could not ask". `new Set()` and `null` are different facts and the whole design turns on it —
 * an account with no skills must be able to refuse `/grill-me`, while an unreachable `/skills` must not.
 */
function registry({ commands, skills, tools } = {}) {
  return {
    commands: commands instanceof Set ? commands : null,
    skills: skills instanceof Set ? skills : null,
    tools: tools instanceof Set ? tools : null,
  };
}

/** The bare skill name behind a `/skill_<name>` or `/skill-<name>` row. "" when it is not one. */
function skillName(name) {
  const m = /^skill[-_](.+)$/.exec(String(name || ""));
  return m ? m[1] : "";
}

/**
 * Where `/name` goes: `builtin` · `skill` · `tool` · `unknown` · `unverified`.
 *
 * `unknown` is the ONLY verdict that may refuse, and it is only ever reached when every registry that
 * could have claimed the name was actually read. If any of them is `null` the answer is `unverified` and
 * the caller must fall through — being wrong in that direction costs a round-trip, and being wrong in the
 * other direction tells a customer their working command does not exist.
 */
function classify(name, reg) {
  const n = String(name || "").trim().toLowerCase();
  const r = reg || registry();
  if (!n) return { verdict: "unknown", name: n };
  if (r.commands && r.commands.has(n)) return { verdict: "builtin", name: n };
  const bare = skillName(n);
  if (bare) {
    // A `skill_`-prefixed name can ONLY ever be a skill — it can never be an MCP tool, so an unreadable
    // tool list is irrelevant to it and must not soften the refusal. repl.js already refuses to fall a
    // `skill_` name through to a raw MCP call, because that would hand back the playbook markdown.
    if (!r.skills) return { verdict: "unverified", name: n, why: "the skill list could not be read" };
    return r.skills.has(bare) ? { verdict: "skill", name: n, skill: bare }
                              : { verdict: "unknown", name: n, skill: bare };
  }
  if (r.skills && r.skills.has(n)) return { verdict: "skill", name: n, skill: n };
  if (r.tools && r.tools.has(n)) return { verdict: "tool", name: n };
  // Only now, with the built-ins checked and BOTH remote registries actually read, may we say no.
  const missing = [!r.skills ? "the skill list" : "", !r.tools ? "the tool list" : ""].filter(Boolean);
  if (missing.length) return { verdict: "unverified", name: n, why: `${missing.join(" and ")} could not be read` };
  return { verdict: "unknown", name: n };
}

/** Every name a `/` could complete to, from whichever registries were readable. Sorted, deduped. */
function knownNames(reg) {
  const r = reg || registry();
  const all = new Set();
  for (const n of r.commands || []) all.add(n);
  for (const n of r.skills || []) all.add(`skill_${n}`);
  for (const n of r.tools || []) all.add(n);
  return [...all].sort();
}

/** Up to `max` names a typo most plausibly meant. Prefix matches first, then substring, then a
 * one-edit neighbour — enough to catch `/sesions` and `/statu` without inventing a suggestion for
 * `/blorp`, which must get none. A wrong did-you-mean is worse than no did-you-mean. */
function didYouMean(name, reg, max) {
  const q = String(name || "").trim().toLowerCase();
  const cap = Math.max(1, max || 3);
  if (!q) return [];
  const names = knownNames(reg);
  const tier = (n) => (n.startsWith(q) ? 0 : n.includes(q) ? 1 : q.includes(n) ? 2 : oneEdit(n, q) ? 3 : 4);
  return names.map((n) => ({ n, t: tier(n) }))
    .filter((x) => x.t < 4)
    .sort((a, b) => a.t - b.t || a.n.length - b.n.length || a.n.localeCompare(b.n))
    .slice(0, cap)
    .map((x) => x.n);
}

/** Within one insertion, deletion or substitution. Deliberately not a full edit distance: two typos in a
 * five-letter command is not a typo, it is a different word, and guessing at it is how we would start
 * inventing answers again. */
function oneEdit(a, b) {
  if (a === b) return true;
  if (Math.abs(a.length - b.length) > 1) return false;
  const [s, t] = a.length >= b.length ? [a, b] : [b, a];
  let i = 0, j = 0, slips = 0;
  while (i < s.length && j < t.length) {
    if (s[i] === t[j]) { i += 1; j += 1; continue; }
    slips += 1;
    if (slips > 1) return false;
    i += 1;
    if (s.length === t.length) j += 1;
  }
  return slips + (s.length - i) + (t.length - j) <= 1;
}

/** The refusal a customer reads, as lines. Names what was typed, offers the menu, and — only when there
 * is a real candidate — one did-you-mean. Never a stack, never a server error, and never a model call. */
function refusalLines(name, reg, c) {
  const near = didYouMean(name, reg, 3);
  const lines = [`  ${c.amber("!")} ${c.dim("unknown command ")}${c.bold("/" + name)}${c.dim(" — nothing ran, and nothing was sent.")}`];
  if (near.length) {
    lines.push(`  ${c.dim("did you mean ")}${near.map((n) => c.teal("/" + n)).join(c.dim(" · "))}${c.dim("?")}`);
  }
  lines.push(`  ${c.dim("press ")}${c.teal("/")}${c.dim(" to see everything, or ")}${c.teal("/help")}${c.dim(" for the built-ins.")}`);
  return lines;
}

module.exports = { registry, classify, knownNames, didYouMean, oneEdit, refusalLines, skillName };
