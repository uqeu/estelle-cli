"use strict";
// THE TERMINAL HALF OF GITHUB CONNECT. Until this shipped, `estelle` had no GitHub path at all — a
// terminal-only user could not give Estelle their code, which is the one thing the product runs on.
//
// The property these tests defend is that the CLI is not a SOFTER door than the dashboard. The server
// demands two proofs for a bind — an OAuth code (control of the GitHub installation) and an authenticated
// caller (control of the Estelle account) — and the CLI satisfies the second with the API key it already
// holds and the first over a LOOPBACK listener, so a forwarded link delivers its code to the recipient's own
// machine and never to the sender's. Hence: the listener binds 127.0.0.1 only, and an ambiguous installation
// is NEVER resolved by picking the first one.
const test = require("node:test");
const assert = require("node:assert");
const http = require("node:http");
const gh = require("../bin/github-connect.js");

test("the redirect is a fixed loopback port — GitHub matches the registered URL including the port", () => {
  assert.equal(gh.redirectUri(), "http://127.0.0.1:8788/github/callback");
  assert.equal(gh.redirectUri(9999), "http://127.0.0.1:9999/github/callback");
});

test("parseCallback pulls the code and state GitHub redirects with", () => {
  assert.deepEqual(gh.parseCallback("/github/callback?code=abc&state=xyz"), { code: "abc", state: "xyz" });
});

test("parseCallback reports a denial instead of inventing a code", () => {
  const denied = gh.parseCallback("/github/callback?error=access_denied&error_description=User+said+no");
  assert.equal(denied.error, "User said no");
  assert.equal(gh.parseCallback("/github/callback?state=only").error, "GitHub redirected without a code");
});

test("a request to any other path is not the callback", () => {
  // A favicon probe or a stray fetch must not be mistaken for the redirect and end the wait early.
  assert.equal(gh.parseCallback("/favicon.ico"), null);
  assert.equal(gh.parseCallback("::::"), null);
});

test("one visible installation is chosen, and none is reported as none", () => {
  const rows = [{ id: 147117265, account: "uqeu", type: "User" }];
  assert.deepEqual(gh.pickInstallation(rows), { chosen: rows[0] });
  assert.deepEqual(gh.pickInstallation([]), { none: true });
  assert.deepEqual(gh.pickInstallation(null), { none: true });
});

test("an ambiguous choice is handed back to the human, never resolved by picking the first", () => {
  // Binding the wrong installation sweeps the wrong org's PRIVATE code into this namespace.
  const rows = [{ id: 1, account: "acme" }, { id: 2, account: "other" }];
  assert.deepEqual(gh.pickInstallation(rows), { needs: rows });
});

test("an installation can be named by id or by owner login", () => {
  const rows = [{ id: 1, account: "acme" }, { id: 2, account: "Other" }];
  assert.deepEqual(gh.pickInstallation(rows, "2"), { chosen: rows[1] });
  assert.deepEqual(gh.pickInstallation(rows, "other"), { chosen: rows[1] });
  assert.deepEqual(gh.pickInstallation(rows, "nope"), { unknown: "nope", needs: rows });
});

test("status names the one next command in every state", () => {
  assert.match(gh.statusLines({ linked: false }).join("\n"), /not linked[\s\S]*estelle github link/);
  const linked = gh.statusLines({ linked: true, login: "uqeu" }, []).join("\n");
  assert.match(linked, /linked as uqeu/);
  assert.match(linked, /install the Estelle GitHub App/i);   // linked, but nothing installed yet
  const ready = gh.statusLines({ linked: true }, [{ id: 7, account: "acme", type: "Organization" }]).join("\n");
  assert.match(ready, /7\s+acme \(Organization\)/);
  assert.match(ready, /estelle github connect/);
});

test("an already-connected installation is REPORTED, not re-offered as the next step", () => {
  // The bug: `estelle github` listed the installations a linked identity can see and then always printed
  // "Run: estelle github connect [id|owner]" — including when one of them was already bound. The one
  // command a connected user needs is the NEXT one, and telling them to redo the step they finished reads
  // as though the connect silently failed.
  const rows = [{ id: 7, account: "acme", type: "Organization" }, { id: 9, account: "other" }];
  const out = gh.statusLines({ linked: true, login: "uqeu" }, rows,
                             { connected: true, installations: [7] }).join("\n");
  assert.match(out, /Connected/);
  assert.match(out, /7\s+acme \(Organization\)\s+·\s+connected/);   // WHICH one is bound
  assert.doesNotMatch(out, /Run: estelle github connect/);
  assert.match(out, /estelle github repos/);                        // the next step, not the last one
});

test("an unconnected installation list still asks for the connect", () => {
  const rows = [{ id: 7, account: "acme" }];
  for (const repos of [undefined, null, {}, { connected: false, installations: [] }]) {
    const out = gh.statusLines({ linked: true }, rows, repos).join("\n");
    assert.match(out, /Run: estelle github connect/, `repos=${JSON.stringify(repos)}`);
    assert.doesNotMatch(out, /Connected/);
  }
});

test("a connection to an installation this identity cannot see is still reported", () => {
  // A teammate who never linked their own GitHub identity sees no installations of their own, but the
  // TEAM's connection is real — telling them to connect would be wrong.
  const out = gh.statusLines({ linked: true }, [], { connected: true, installations: [7] }).join("\n");
  assert.match(out, /Connected/);
  assert.doesNotMatch(out, /Run: estelle github connect/);
});

test("the listener answers the browser and resolves the code", async () => {
  const waiting = gh.awaitCallback({ port: 8791, timeoutMs: 5000 });
  const body = await new Promise((resolve, reject) => {
    const req = http.request({ host: "127.0.0.1", port: 8791, path: "/github/callback?code=c1&state=s1",
                              timeout: 10000 },
                             (res) => {
                               let text = "";
                               res.on("data", (d) => { text += d; });
                               res.on("end", () => resolve(text));
                             });
    // `timeout` only EMITS — without this handler the socket idles and the promise never settles, which is
    // the same unbounded wait the option looks like it closes.
    req.on("timeout", () => { req.destroy(new Error("callback request timed out")); });
    req.on("error", reject);
    req.end();
  });
  assert.deepEqual(await waiting, { code: "c1", state: "s1" });
  assert.match(body, /close this tab/i);   // the browser is told where to go back to
});

test("the listener binds loopback only — the code must not be reachable from the network", async () => {
  const addresses = [];
  const waiting = gh.awaitCallback({ port: 8792, timeoutMs: 1000, onListen: (u) => addresses.push(u) });
  await assert.rejects(waiting, /timed out/);   // and it gives the prompt back rather than hanging forever
  assert.deepEqual(addresses, ["http://127.0.0.1:8792/github/callback"]);
});

test("a busy port is a named failure, not a stack trace", async () => {
  const blocker = http.createServer(() => {});
  await new Promise((r) => blocker.listen(8793, "127.0.0.1", r));
  await assert.rejects(gh.awaitCallback({ port: 8793, timeoutMs: 1000 }), /already in use/);
  await new Promise((r) => blocker.close(r));
});

test("a denied authorization rejects with GitHub's reason", async () => {
  const waiting = gh.awaitCallback({ port: 8794, timeoutMs: 5000 });
  const rejected = assert.rejects(waiting, /User said no/);
  await new Promise((resolve, reject) => {
    const req = http.request(
      { host: "127.0.0.1", port: 8794, path: "/github/callback?error=access_denied&error_description=User+said+no",
        timeout: 10000 },
      (res) => { res.resume(); res.on("end", resolve); });
    // A raw http.request has NO default timeout: a server that ACCEPTS and never answers holds this promise
    // open forever, and node --test had no per-test limit either -- so one such request could hold a CI job
    // to GitHub's 6-hour ceiling. Same unbounded-wait defect as the product's own fetch calls, one layer up.
    req.on("timeout", () => { req.destroy(new Error("callback request timed out")); });
    req.on("error", reject);
    req.end();
  });
  await rejected;
});
