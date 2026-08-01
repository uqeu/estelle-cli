"use strict";
// CONNECT YOUR REPO FROM THE TERMINAL — the half of GitHub connect the CLI never had.
//
// Estelle's whole loop runs on the customer's code, and until this file the ONLY way to connect a repo was
// the dashboard. A terminal-only user had no path at all: `grep -n github cli/bin/*.js` found nothing but
// bare github.com URLs. The flagship product could not do the flagship thing.
//
// THE SECURITY SHAPE IS NOT NEGOTIABLE, AND THE CLI IS THE EASY CASE. Binding a GitHub App installation to
// an Estelle account needs TWO independent proofs:
//
//   1. control of the GITHUB installation — an OAuth authorization code, exchanged server-side for a user
//      token whose GET /user/installations contains the installation;
//   2. control of the ESTELLE account — an authenticated caller.
//
// A terminal client already holds proof 2: the API key it authenticates every other command with. Proof 1
// arrives on a LOOPBACK redirect, which is what keeps a forwarded link harmless — a link sent to someone
// else delivers its code to THEIR localhost, never to the sender's. So the CLI is not a softer door than
// the browser; the server enforces exactly the same two checks on POST /github/identity/link either way.
//
// Everything decidable is a pure function here (parsing the callback, picking an installation, shaping the
// lines a human reads); the socket and the browser launch are the only impure parts.

const http = require("node:http");

// The loopback port the redirect comes back on. FIXED, not ephemeral: GitHub matches a redirect URL against
// the ones registered on the App including the port, so a random port could never be registered.
const LOOPBACK_PORT = 8788;
const CALLBACK_PATH = "/github/callback";

const redirectUri = (port = LOOPBACK_PORT) => `http://127.0.0.1:${port}${CALLBACK_PATH}`;

/** What the browser is left looking at. Deliberately plain text: it must never look like a page that wants
 *  anything else from the user, and it must say where to go back to. */
const DONE_PAGE = (ok, detail) =>
  ok ? "Estelle: GitHub authorized. You can close this tab and return to your terminal.\n"
     : `Estelle: GitHub authorization failed (${detail}). Close this tab and try again in your terminal.\n`;

/**
 * The `code` + `state` out of the loopback request, or an `error` GitHub reported instead.
 * Pure: takes the raw request URL, returns what the exchange needs. A request to any other path is ignored
 * (`null`) so a stray browser probe — favicon, a preflight — can't be mistaken for the callback.
 */
function parseCallback(rawUrl) {
  let url;
  try {
    url = new URL(rawUrl, `http://127.0.0.1:${LOOPBACK_PORT}`);
  } catch (_) {
    return null;
  }
  if (url.pathname !== CALLBACK_PATH) return null;
  const error = url.searchParams.get("error");
  if (error) return { error: url.searchParams.get("error_description") || error };
  const code = url.searchParams.get("code");
  const state = url.searchParams.get("state");
  if (!code || !state) return { error: "GitHub redirected without a code" };
  return { code, state };
}

/**
 * Which installation to bind, from what the linked identity can actually see.
 * `want` may be an installation id or an owner login; with exactly one installation and no `want`, that one
 * is chosen. Ambiguity is NEVER resolved by picking the first — `{ needs: rows }` sends it back to the human,
 * because binding the wrong org sweeps the wrong private code.
 */
function pickInstallation(rows, want) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) return { none: true };
  if (want) {
    const asked = String(want).trim().toLowerCase();
    const hit = list.find((r) => String(r.id) === asked || String(r.account || "").toLowerCase() === asked);
    return hit ? { chosen: hit } : { unknown: want, needs: list };
  }
  return list.length === 1 ? { chosen: list[0] } : { needs: list };
}

/**
 * The status a human reads: linked or not, WHAT IS ALREADY CONNECTED, and the ONE next command.
 *
 * `repos` is the `GET /github/repos` payload (`{ connected, installations }`) — the server's own answer to
 * "is a GitHub App installation bound to this account?". Without it this listed the installations a linked
 * identity can SEE and then always printed "Run: estelle github connect", including when one of them was
 * already bound: the finished step re-offered as the next one, which reads as a connect that silently
 * failed. On a team the connection may also belong to a teammate's Connect, so the bound set is not
 * derivable from this identity's own installations at all — only the server knows.
 *
 * Omitted or unconnected, the output is exactly what it was.
 */
function statusLines(identity, installations, repos) {
  const linked = Boolean(identity && identity.linked);
  const who = linked && identity.login ? ` as ${identity.login}` : "";
  const lines = [linked ? `GitHub identity: linked${who}` : "GitHub identity: not linked"];
  if (!linked) {
    lines.push("Run: estelle github link   (opens your browser once, then binds to this account)");
    return lines;
  }
  const bound = connectedIds(repos);
  const rows = Array.isArray(installations) ? installations : [];
  if (!rows.length) {
    if (bound.length) {
      // Connected, but through an installation this identity cannot see — a teammate connected it, or the
      // identity link was re-pointed. "Connect" would be the wrong advice; the connection is real.
      lines.push(`Connected: installation ${bound.join(", ")}`);
      lines.push("Run: estelle github repos   (pick what to ingest)");
      return lines;
    }
    lines.push("No App installations visible to that identity — install the Estelle GitHub App on the org");
    lines.push("or user that owns the repos, then run: estelle github connect");
    return lines;
  }
  for (const row of rows) {
    const mark = bound.includes(String(row.id)) ? "  ·  connected" : "";
    lines.push(`  ${row.id}  ${row.account || ""}${row.type ? ` (${row.type})` : ""}${mark}`);
  }
  if (bound.length) {
    lines.push(`Connected: installation ${bound.join(", ")}`);
    lines.push("Run: estelle github repos   (pick what to ingest)");
    return lines;
  }
  lines.push("Run: estelle github connect [id|owner]   then: estelle github repos");
  return lines;
}

/** The installation ids the SERVER says are bound to this account, as strings. `[]` when it says none, or
 *  when the payload is missing/unshaped — never a guess that something is connected. */
function connectedIds(repos) {
  if (!repos || repos.connected !== true) return [];
  const ids = Array.isArray(repos.installations) ? repos.installations : [];
  const listed = ids.map((i) => String(i)).filter(Boolean);
  if (listed.length) return listed;
  return repos.installation_id ? [String(repos.installation_id)] : [];
}

/**
 * Wait for GitHub's redirect on the loopback listener. Resolves `{ code, state }` or rejects.
 * One-shot and bounded: it stops listening the moment it has an answer, and gives up after `timeoutMs` so a
 * user who closes the tab gets their prompt back instead of a hung process.
 */
function awaitCallback({ port = LOOPBACK_PORT, timeoutMs = 300000, onListen } = {}) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (fn, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      server.close(() => fn(value));
    };
    const server = http.createServer((req, res) => {
      const parsed = parseCallback(req.url || "");
      // Connection: close on every reply — a keep-alive socket would hold server.close() open for the
      // browser's full idle timeout, which reads to the user as a CLI that hung after it already succeeded.
      if (parsed === null) {         // not the callback path — answer, but keep waiting for the real one
        res.writeHead(404, { "Content-Type": "text/plain", "Connection": "close" });
        res.end("not found\n");
        return;
      }
      const ok = !parsed.error;
      res.writeHead(ok ? 200 : 400, { "Content-Type": "text/plain", "Connection": "close" });
      res.end(DONE_PAGE(ok, parsed.error || ""));
      if (ok) finish(resolve, parsed);
      else finish(reject, new Error(parsed.error));
    });
    const timer = setTimeout(() => finish(reject, new Error("timed out waiting for GitHub")), timeoutMs);
    server.on("error", (err) => {
      clearTimeout(timer);
      settled = true;
      reject(err.code === "EADDRINUSE"
        ? new Error(`port ${port} is already in use — close whatever is on it and retry`)
        : err);
    });
    // 127.0.0.1, never 0.0.0.0: the authorization code must not be reachable from the network.
    server.listen(port, "127.0.0.1", () => { if (onListen) onListen(redirectUri(port)); });
  });
}

module.exports = {
  LOOPBACK_PORT, CALLBACK_PATH, DONE_PAGE,
  redirectUri, parseCallback, pickInstallation, statusLines, connectedIds, awaitCallback,
};
