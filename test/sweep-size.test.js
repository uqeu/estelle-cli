"use strict";
// The sweep SIZE GATE, from the CLI side.
//
// The bug: `estelle sweep` started blind. Point a Free account (10M memory-tokens) at a 70M-token repo and
// the whole tree went up the wire while the server quietly declined file after file at the cap — the first
// thing a new customer saw was a half-indexed repo, with no warning it was ever going to fit.
//
// So this file protects three things:
//   1. the estimate is asked FIRST, and it is CHEAP — paths and byte sizes, never file content;
//   2. a repo that will not fit means /sync and /ingest/start are never called at all, and the exit is red;
//   3. a refusal the SERVER makes (a stale CLI, a raced upgrade, a repo that grew between the two calls)
//      renders the same actionable block instead of a bare HTTP code.
// The wording decisions are unit-tested directly; what actually goes on the wire is checked against the REAL
// CLI talking to a stub Estelle, because that cannot be verified any other way.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const http = require("node:http");
const { execFile } = require("node:child_process");
const e = require("../bin/estelle.js");

const BIN = path.join(__dirname, "..", "bin", "estelle.js");

// ── the pure decisions ────────────────────────────────────────────────────────────────────────────────

test("estimateBody sends sizes, never content — measuring a 70M repo must not cost a 70M upload", () => {
  const files = [{ path: "a.py", content: "def a(): pass\n" }, { path: "b.ts", content: "export const b=1;" }];
  const body = e.estimateBody(files, "owner/name");
  assert.equal(body.repo, "owner/name");
  assert.deepEqual(body.files, [{ path: "a.py", bytes: 14 }, { path: "b.ts", bytes: 17 }]);
  // The whole point of a pre-flight is that it is cheap. Content in this body would make the estimate cost
  // exactly what it exists to avoid.
  assert.ok(!JSON.stringify(body).includes("def a()"), "file content must never travel in an estimate");
});

test("estimateBody omits repo when we could not work one out", () => {
  assert.deepEqual(Object.keys(e.estimateBody([{ path: "a.py", content: "x" }], "")), ["files"]);
});

test("estimateBody counts BYTES, not characters — a UTF-8 repo is bigger than its character count", () => {
  const body = e.estimateBody([{ path: "a.py", content: "# héllo →\n" }], "");
  assert.equal(body.files[0].bytes, Buffer.byteLength("# héllo →\n", "utf8"));
  assert.ok(body.files[0].bytes > "# héllo →\n".length);
});

test("mtok speaks the units the pricing page does", () => {
  assert.equal(e.mtok(42_000_000), "42M");
  assert.equal(e.mtok(1_200_000), "1.2M");
  assert.equal(e.mtok(12_000), "12K");
  assert.equal(e.mtok(900), "900");
  assert.equal(e.mtok(null), "0");
});

test("sweepFitLines says what it needs, what you have, and the two ways out that are not 'pay us'", () => {
  const lines = e.sweepFitLines({
    repo: "estelle-cli", estimated_tokens: 42_000_000, cap: 10_000_000, remaining_tokens: 10_000_000,
    blocked_tokens: 32_000_000, suggested_plan: { plan: "pro", monthly_usd: 20, cap: 60_000_000 },
    largest_paths: [{ path: "vendor_copy/", tokens: 30_000_000 }, { path: "src/", tokens: 9_000_000 }],
  }).join("\n");
  assert.match(lines, /estelle-cli needs about 42M/);
  assert.match(lines, /holds 10M/);
  assert.match(lines, /pro \(60M\) fits — \$20\/mo/);
  // A block a customer cannot act on is a dead end. Naming the big directories and the narrower-path escape
  // is what makes "upgrade" a choice rather than the only door.
  assert.match(lines, /vendor_copy\/ 30M/);
  assert.match(lines, /--path/);
});

test("sweepFitLines never throws on a partial or missing estimate", () => {
  assert.deepEqual(e.sweepFitLines(null), []);
  assert.deepEqual(e.sweepFitLines(undefined), []);
  assert.ok(e.sweepFitLines({}).length >= 1);   // degraded, but still says something true
});

test("failedOnSize recognises the server's size refusal and nothing else", () => {
  assert.equal(e.failedOnSize({ status: 402, json: { estimate: { fits: false } } }), true);
  assert.equal(e.failedOnSize({ status: 402, json: { error: { message: "no credit" } } }), undefined);
  assert.equal(e.failedOnSize({ status: 500, json: { estimate: { fits: false } } }), false);
});

// ── the real CLI against a stub Estelle ───────────────────────────────────────────────────────────────

const JSON_HEADERS = { "content-type": "application/json" };

function tmpRepo(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-sweep-"));
  for (const [rel, content] of Object.entries(files || {})) {
    const full = path.join(root, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, content);
  }
  return root;
}

// A $HOME with no ~/.estelle in it — see reindex.test.js. Blanking ESTELLE_API_KEY alone stopped being
// enough once commands learned to read the key the session saves to disk.
const EMPTY_HOME = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-sweepsize-home-"));

function runCli(args, url) {
  return new Promise((resolve) => {
    execFile(process.execPath, [BIN, ...args], {
      env: { ...process.env, HOME: EMPTY_HOME, NO_COLOR: "1", ESTELLE_API_KEY: "", ESTELLE_MCP_URL: url },
      timeout: 30_000,
    }, (err, stdout) => resolve({ code: err && typeof err.code === "number" ? err.code : 0, stdout }));
  });
}

/** Run `estelle sweep …` for real and hand back every request it made. `routes` maps a path to a
 * `(res) => …` responder; anything unrouted answers 200 `{}`. */
async function sweep(args, routes) {
  const seen = [];
  const server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => { raw += c; });
    req.on("end", () => {
      let body = null;
      try { body = JSON.parse(raw); } catch (_) { /* non-JSON body */ }
      seen.push({ path: req.url, method: req.method, body });
      const responder = (routes || {})[req.url];
      if (responder) return responder(res);
      res.writeHead(200, JSON_HEADERS);
      res.end(JSON.stringify({ indexed: 1, skipped: 0, chunks: 1 }));
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const url = `http://127.0.0.1:${server.address().port}/mcp`;
  const run = await runCli(["sweep", ...args], url);
  if (server.closeAllConnections) server.closeAllConnections();
  server.close();
  return { ...run, seen };
}

const json = (payload) => (res) => { res.writeHead(200, JSON_HEADERS); res.end(JSON.stringify(payload)); };
const status = (code, payload) => (res) => {
  res.writeHead(code, JSON_HEADERS);
  res.end(JSON.stringify(payload));
};

const TOO_BIG = {
  repo: "owner/name", estimated_tokens: 42_000_000, cap: 10_000_000, remaining_tokens: 10_000_000,
  blocked_tokens: 32_000_000, fits: false,
  suggested_plan: { plan: "pro", monthly_usd: 20, cap: 60_000_000 },
  largest_paths: [{ path: "vendor_copy/", tokens: 30_000_000 }],
};

test("the repo is SIZED before a single file is uploaded", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await sweep(["--key", "k", "--repo", "owner/name", "--path", root],
                        { "/sweep/estimate": json({ fits: true, estimated_tokens: 4, cap: 10_000_000 }) });
  assert.equal(r.seen[0].path, "/sweep/estimate", "the estimate must come FIRST, before any upload");
  assert.equal(r.seen[1].path, "/sync");
  assert.deepEqual(Object.keys(r.seen[0].body.files[0]), ["path", "bytes"]);
  assert.equal(r.code, 0);
});

test("a repo that will not fit is never uploaded, and the exit is RED", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n", "vendor_copy/big.py": "x = 1\n" });
  const r = await sweep(["--key", "k", "--repo", "owner/name", "--path", root],
                        { "/sweep/estimate": json(TOO_BIG) });

  // THE assertion of this file. Uploading anyway is the old bug: the tree travels in full and lands
  // half-indexed, and the customer finds out from a recall that cannot cite their code.
  assert.deepEqual(r.seen.map((s) => s.path), ["/sweep/estimate"]);
  assert.match(r.stdout, /will not fit/);
  assert.match(r.stdout, /42M/);
  assert.match(r.stdout, /pro \(60M\) fits/);
  assert.doesNotMatch(r.stdout, /Repo swept/, "a refused sweep must never claim the repo was swept");
  // Green here means a CI step "succeeds" having ingested nothing — the same fail-open reindex closed.
  assert.equal(r.code, 1);
});

test("a size refusal made by the SERVER renders the estimate, not a bare 402", async () => {
  // The preflight can be stale (an older CLI, a repo that grew, a plan that changed between the two calls),
  // so the server is the real enforcement point and its refusal has to read just as well.
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await sweep(["--key", "k", "--repo", "owner/name", "--path", root], {
    "/sweep/estimate": json({ fits: true, estimated_tokens: 4, cap: 10_000_000 }),
    "/sync": status(402, { error: { message: "Sweep refused before ingesting: …" }, estimate: TOO_BIG }),
  });
  assert.match(r.stdout, /will not fit/);
  assert.match(r.stdout, /pro \(60M\) fits/);
  assert.doesNotMatch(r.stdout, /Repo swept/);
  assert.equal(r.code, 1);
});

test("a server with no /sweep/estimate still sweeps — the gate lives server-side, not here", async () => {
  // The CLI's pre-flight buys an earlier, kinder failure; it is not the thing standing between a 70M repo
  // and a 10M cap. Refusing locally on a 404 would break `sweep` against every server predating this
  // release, and those servers have no gate on /sync either — so nothing is lost by proceeding.
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await sweep(["--key", "k", "--repo", "owner/name", "--path", root],
                        { "/sweep/estimate": status(404, { error: { message: "not found" } }) });
  assert.deepEqual(r.seen.map((s) => s.path), ["/sweep/estimate", "/sync"]);
  assert.equal(r.code, 0);
  assert.match(r.stdout, /Repo swept/);
});
