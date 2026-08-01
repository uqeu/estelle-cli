"use strict";
// `reindex` is the INCREMENTAL counterpart to `sweep`, and the difference is the whole point of the command.
//
//   sweep   → POST /sync    → a WHOLE-REPO REBUILD. The grounding surface becomes exactly what was sent, so
//                             syncing three changed files silently guts the graph: every other file's symbols
//                             vanish and the gate starts flagging real APIs as ungrounded.
//   reindex → POST /reindex → changed files replace their own nodes, `removed` paths are dropped, and every
//                             path NOT named survives untouched.
//
// So these tests protect two things above all: that the request goes to /reindex with the right body, and
// that nothing which should not travel (a file outside the repo, a live-looking secret) ends up in it.
// The pure decisions are unit-tested directly; the request body is checked against the REAL CLI talking to a
// stub Estelle, because "what actually goes on the wire" cannot be verified any other way.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const http = require("node:http");
const { execFile, execFileSync } = require("node:child_process");
const e = require("../bin/estelle.js");

const BIN = path.join(__dirname, "..", "bin", "estelle.js");

// ── the pure decisions ────────────────────────────────────────────────────────────────────────────────

test("positionalPaths: a flag's VALUE never becomes a file to upload", () => {
  assert.deepEqual(e.positionalPaths(["src/a.py", "src/b.ts"]), ["src/a.py", "src/b.ts"]);
  // The failure this prevents is concrete: without the skip, `--key ESTELLE_KEY` makes the CLI try to read a
  // file named after the key, and a typo'd path then arrives at the server as a DELETION instruction.
  assert.deepEqual(e.positionalPaths(["--key", "sk-secret", "a.py"]), ["a.py"]);
  assert.deepEqual(e.positionalPaths(["--path", "/repo", "--key", "K", "a.py", "b.py"]), ["a.py", "b.py"]);
  assert.deepEqual(e.positionalPaths([]), []);
  // Only a flag that TAKES a value swallows the next argument. When the skip was unconditional a valueless
  // flag ate the path behind it, so `reindex --dry-run a.py` reindexed nothing and said "Nothing changed" —
  // a green run that did no work is indistinguishable from a green run that did.
  assert.deepEqual(e.positionalPaths(["--dry-run", "a.py"]), ["a.py"]);
  assert.deepEqual(e.positionalPaths(["--help", "a.py", "--key", "K"]), ["a.py"]);
});

test("partitionIngestable: the extension allowlist is not just for git's list", () => {
  // git's own list is filtered upstream by changedFromGitOutput; a path the USER names reaches the uploader
  // directly. Verified live before this filter existed: `estelle reindex logo.png secrets.env` uploaded a PNG
  // read as UTF-8 and a .env file. SECRET_RE only catches key-SHAPED content, so `STRIPE=whatever` sailed
  // through it — a binary in the code graph and a .env in memory, both silent and both durable.
  const { kept, skipped } = e.partitionIngestable(["a.py", "logo.png", "secrets.env", ".env", "sub/b.ts", "yarn.lock"]);
  assert.deepEqual(kept, ["a.py", "sub/b.ts"]);
  assert.deepEqual(skipped, ["logo.png", "secrets.env", ".env", "yarn.lock"]);
  assert.deepEqual(e.partitionIngestable([]), { kept: [], skipped: [] });
});

test("repoRelative: normalises to the key the graph uses, and refuses anything outside the repo", () => {
  const root = "/repo";
  // The stored key is the repo-relative path. An absolute or "./"-prefixed path would not MATCH it, so a
  // changed file would land beside its old copy instead of replacing it, and a `removed` entry would drop
  // nothing at all while the CLI printed a green "Memory current."
  assert.equal(e.repoRelative(root, "src/a.py"), path.join("src", "a.py"));
  assert.equal(e.repoRelative(root, "./src/a.py"), path.join("src", "a.py"));
  assert.equal(e.repoRelative(root, "/repo/src/a.py"), path.join("src", "a.py"));
  assert.equal(e.repoRelative(root, "src/../src/a.py"), path.join("src", "a.py"));
  // Outside the repo → "" (refused). `reindex ../other-repo/x.py` must not file someone else's code under
  // this repo's namespace — the same rule hook.js already enforces on the PostToolUse sync path.
  assert.equal(e.repoRelative(root, "../elsewhere/x.py"), "");
  assert.equal(e.repoRelative(root, "/etc/passwd"), "");
  assert.equal(e.repoRelative(root, ".."), "");
  assert.equal(e.repoRelative(root, ""), "");            // the root itself is not a file
  assert.equal(e.repoRelative(root, "."), "");
});

test("changedFromGitOutput: only what belongs in a code graph", () => {
  const stdout = "src/a.py\nweb/app.tsx\n  spaced.go  \n\npackage-lock.json\nlogo/cream logo.png\nREADME.md\n";
  assert.deepEqual(e.changedFromGitOutput(stdout),
    ["src/a.py", "web/app.tsx", "spaced.go", "README.md"]);
  // git happily reports lockfiles and binaries as "changed"; ingesting them costs memory tokens and grounds
  // nothing. Docs (.md) ARE kept — they are what recall cites for intent.
  assert.ok(!e.changedFromGitOutput(stdout).some((p) => /\.(json|png)$/.test(p)));
  assert.deepEqual(e.changedFromGitOutput(""), []);
  assert.deepEqual(e.changedFromGitOutput(undefined), []);
  for (const ext of [".py", ".ts", ".tsx", ".go", ".rs", ".java", ".swift", ".md"]) assert.ok(e.EXT.has(ext), ext);
  for (const ext of [".json", ".png", ".lock", ".yaml", ""]) assert.ok(!e.EXT.has(ext), ext);
});

test("reindexBody: replace these, drop those, leave everything else alone", () => {
  const files = [{ path: "a.py", content: "def a(): pass", mtime: 123 }];
  const body = e.reindexBody(files, "owner/name", ["gone.py"]);
  assert.deepEqual(body, { files: [{ path: "a.py", content: "def a(): pass" }], repo: "owner/name", removed: ["gone.py"] });
  // Only path+content travel — no local metadata leaks onto the wire just because it was on the object.
  assert.deepEqual(Object.keys(body.files[0]), ["path", "content"]);

  // `removed` is OMITTED when empty rather than sent as [] — an ordinary update must not read as deletion
  // bookkeeping, and this is the field that makes nodes disappear.
  assert.equal("removed" in e.reindexBody(files, "owner/name", []), false);
  assert.equal("removed" in e.reindexBody(files, "owner/name", undefined), false);

  // No repo name → the key is omitted, never sent empty. An empty string would file the repo under a blank
  // name; omitting it lets the server fall back deliberately.
  assert.equal("repo" in e.reindexBody(files, "", []), false);

  // A pure deletion is legal: no files, just the paths that are gone.
  assert.deepEqual(e.reindexBody([], "owner/name", ["gone.py"]),
    { files: [], repo: "owner/name", removed: ["gone.py"] });
});

test("partitionSecrets: a file that embeds a live-looking key never reaches the body", () => {
  const fake = "sk-" + "A".repeat(28);                    // built at runtime so no key-shape lives in this file
  const { files, flagged } = e.partitionSecrets([
    { path: "clean.py", content: "def a(): pass" },
    { path: "leak.py", content: `KEY = "${fake}"` },
    { path: "aws.py", content: "AKIA" + "0123456789ABCDEF" },
  ]);
  assert.deepEqual(files.map((f) => f.path), ["clean.py"]);
  assert.deepEqual(flagged.map((f) => f.path), ["leak.py", "aws.py"]);
  assert.ok(!JSON.stringify(e.reindexBody(files, "r", [])).includes(fake));
});

test("errText: a failed reindex says WHAT went wrong, never '[object Object]'", () => {
  // The server's envelope is {"error": {"message", "code"}} — an error a human cannot read is its own bug.
  assert.equal(e.errText({ json: { error: { message: "repo not found" } } }), "repo not found");
  assert.equal(e.errText({ json: { error: "plain string" } }), "plain string");
  assert.equal(e.errText({ json: null, text: "502 Bad Gateway" }), "502 Bad Gateway");
  assert.equal(e.errText({}), "");
});

// ── the real CLI against a stub Estelle ───────────────────────────────────────────────────────────────

const JSON_HEADERS = { "content-type": "application/json" };
const OK = (res) => { res.writeHead(200, JSON_HEADERS); res.end("{}"); };

/** A throwaway directory containing `files`. NOT a git repo until a test calls `gitCommit`, so
 * `changedFiles` stays empty and the explicit-paths route is driven with no dependency on the machine's
 * git state. */
function tmpRepo(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-reindex-"));
  for (const [rel, content] of Object.entries(files || {})) {
    const full = path.join(root, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, content);
  }
  return root;
}

/** Commit everything in `root` to a fresh git repo, so git can TESTIFY about what happened afterwards.
 * Load-bearing for every deletion test: absence from disk is not evidence a file was deleted — a mistyped
 * path is absent too — so only git's own "this was deleted" may become a `removed` entry. */
function gitCommit(root) {
  const git = (...argv) => execFileSync("git", ["-C", root, ...argv], { stdio: "ignore" });
  git("init", "-q");
  git("config", "user.email", "t@t.co");
  git("config", "user.name", "t");
  git("add", "-A");
  git("commit", "-qm", "init");
}

// A $HOME with no ~/.estelle in it. Blanking ESTELLE_API_KEY stopped being enough the moment commands
// learned to read the key the session stores on disk: without this, "no key" tests would pass on CI and
// quietly resolve the developer's own saved credential on their laptop.
const EMPTY_HOME = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-reindex-home-"));

function runCli(args, url) {
  return new Promise((resolve) => {
    execFile(process.execPath, [BIN, ...args], {
      // ESTELLE_API_KEY is blanked and HOME is redirected so neither a key in the developer's own shell nor
      // one saved by their own `estelle` session can make a test pass.
      env: { ...process.env, HOME: EMPTY_HOME, NO_COLOR: "1", ESTELLE_API_KEY: "", ESTELLE_MCP_URL: url },
      timeout: 30_000,
    }, (err, stdout) => resolve({ code: err && typeof err.code === "number" ? err.code : 0, stdout }));
  });
}

/** Run `estelle reindex …` for real and hand back every request it made. */
async function reindex(args, respond = OK) {
  const seen = [];
  const server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => { raw += c; });
    req.on("end", () => {
      let body = null;
      try { body = JSON.parse(raw); } catch (_) { /* non-JSON body */ }
      seen.push({ path: req.url, method: req.method, auth: req.headers.authorization, body });
      respond(res);
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const url = `http://127.0.0.1:${server.address().port}/mcp`;
  const run = await runCli(["reindex", ...args], url);
  if (server.closeAllConnections) server.closeAllConnections();
  server.close();
  return { ...run, seen };
}

test("the changed files go to /reindex — never to /sync, which would rebuild the world", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n", "sub/b.ts": "export const b = 1;\n",
                         "gone.py": "def gone(): pass\n" });
  gitCommit(root);
  fs.unlinkSync(path.join(root, "gone.py"));               // a REAL deletion, so git will testify to it
  const r = await reindex(["--key", "test-key", "--repo", "owner/name", "--path", root,
                           "a.py", "./sub/b.ts", "gone.py"]);

  assert.equal(r.seen.length, 1, "exactly one call");
  // THE assertion of this file. /sync replaces the whole grounding surface with what it receives, so sending
  // two changed files there erases every other file's symbols. Routing to /reindex is the entire command.
  assert.equal(r.seen[0].path, "/reindex");
  assert.equal(r.seen[0].method, "POST");
  assert.equal(r.seen[0].auth, "Bearer test-key");

  const body = r.seen[0].body;
  // Repo-relative, "./" normalised away — the same key the sweep stored, so these REPLACE rather than duplicate.
  assert.deepEqual(body.files.map((f) => f.path), ["a.py", path.join("sub", "b.ts")]);
  assert.equal(body.files[0].content, "def a(): pass\n");
  assert.deepEqual(Object.keys(body.files[0]), ["path", "content"]);
  // A path GIT reports as deleted must still travel in `removed`, or its symbols linger as ground truth for
  // code that is gone, which is a hallucination the gate would then certify as real.
  assert.deepEqual(body.removed, ["gone.py"]);
  assert.equal(body.repo, "owner/name");
  assert.equal(r.code, 0);
  assert.match(r.stdout, /Memory current/);
});

test("a reindex is always FILED under a repo, never left in the shared unfiled namespace", async () => {
  // Filing is not cosmetic: unfiled sweeps piled every codebase into one namespace, `/repos` reported none,
  // and recall blended every repo the customer ever swept into a single answer.
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await reindex(["--key", "k", "--path", root, "a.py"]);
  assert.equal(r.seen[0].body.repo, path.basename(root), "no git remote → the directory name, never blank");
  assert.ok(r.seen[0].body.repo);
});

test("a path outside the repo is refused, not filed under this repo", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const elsewhere = tmpRepo({ "outsider.py": "def stolen(): pass\n" });
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root,
                           "a.py", path.join(elsewhere, "outsider.py"), "../outsider.py"]);

  const body = r.seen[0].body;
  assert.deepEqual(body.files.map((f) => f.path), ["a.py"]);
  // Nothing from outside may appear ANYWHERE in the body — not as a file, and not as a `removed` path either
  // (which would ask the server to drop a node belonging to a different repo).
  const blob = JSON.stringify(body);
  assert.ok(!blob.includes("outsider"), "a file outside the repo must never reach the wire");
  assert.ok(!blob.includes(".."), "no escaping path may reach the wire");
  for (const p of [...body.files.map((f) => f.path), ...(body.removed || [])]) {
    assert.equal(path.isAbsolute(p), false, `absolute path leaked: ${p}`);
  }
  // and it says so out loud — a silent skip is how a customer concludes the reindex worked
  assert.match(r.stdout, /outside this repo/);
});

test("naming a binary or a dotfile does not force it into the graph", async () => {
  // A user who types the path is not a licence to ingest it: a PNG read as UTF-8 poisons the code graph and a
  // .env lands the customer's config in memory, and BOTH are silent and durable once stored. `SECRET_RE`
  // is no backstop — it matches key SHAPES, so `STRIPE=whatever` is not a secret to it and travels.
  const root = tmpRepo({ "a.py": "def a(): pass\n", "logo.png": "\x89PNG\r\n\x1a\n binary-ish\n",
                         "secrets.env": "STRIPE=whatever\n" });
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root,
                           "a.py", "logo.png", "secrets.env"]);

  const body = r.seen[0].body;
  assert.deepEqual(body.files.map((f) => f.path), ["a.py"]);
  const blob = JSON.stringify(body);
  assert.ok(!blob.includes("logo.png"), "a binary must never reach the wire");
  assert.ok(!blob.includes("STRIPE"), "a .env must never reach the wire");
  // Refusing to INDEX a file is not the same as saying it was deleted — both still exist on disk.
  assert.equal("removed" in body, false);
  // and the skip is announced: a silent skip is how a customer concludes the file was indexed.
  assert.match(r.stdout, /logo\.png/);
  assert.match(r.stdout, /secrets\.env/);
});

test("only git decides what was REMOVED — a mistyped path is an error, not a deletion", async () => {
  // `estelle reindex src/atuh.py` (a typo) used to send `removed: ["src/atuh.py"]` with no confirmation,
  // because the CLI read "absent from disk" as "deleted". Those are different facts: one is git's, the other
  // is a slip of the fingers, and only the first may drop nodes from the graph.
  const root = tmpRepo({ "kept.py": "def k(): pass\n", "gone.py": "def g(): pass\n" });
  gitCommit(root);
  fs.unlinkSync(path.join(root, "gone.py"));
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root,
                           "kept.py", "gone.py", "atuh.py"]);

  const body = r.seen[0].body;
  assert.deepEqual(body.files.map((f) => f.path), ["kept.py"]);
  assert.deepEqual(body.removed, ["gone.py"], "git said deleted → the node must go");
  assert.ok(!JSON.stringify(body).includes("atuh"), "a typo must never become a deletion instruction");
  assert.match(r.stdout, /atuh\.py/);
  // Non-zero: the user named three paths and one did nothing. A green exit means the typo is never noticed
  // and the file the user MEANT stays stale in the graph forever.
  assert.equal(r.code, 1);
  // …and the closing line must not claim otherwise. "Memory current." over a run that skipped a named path is
  // the false-completeness claim the sweep's dropped-file warning already refuses to make.
  assert.doesNotMatch(r.stdout, /Memory current/);
});

test("a typo alone sends nothing at all — and still exits non-zero", async () => {
  const root = tmpRepo({ "src/auth.py": "def auth(): pass\n" });
  gitCommit(root);
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root, "src/atuh.py"]);
  assert.deepEqual(r.seen, [], "nothing real was named → nothing may be sent, least of all a deletion");
  assert.match(r.stdout, /atuh\.py/);
  assert.equal(r.code, 1);
});

test("a file git reports as deleted is still dropped when git picks the set", async () => {
  // The other direction of the same rule: tightening `removed` must not cost the deletions that are REAL,
  // or a deleted file's symbols stay in the graph as ground truth for code that no longer exists.
  const root = tmpRepo({ "live.py": "def a(): pass\n", "gone.py": "def g(): pass\n" });
  gitCommit(root);
  fs.writeFileSync(path.join(root, "live.py"), "def a(): changed\n");
  fs.unlinkSync(path.join(root, "gone.py"));
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root]);   // no names → git decides

  const body = r.seen[0].body;
  assert.deepEqual(body.files.map((f) => f.path), ["live.py"]);
  assert.deepEqual(body.removed, ["gone.py"]);
  assert.equal(r.code, 0);
});

test("a file that looks like it holds a live secret stays on the machine", async () => {
  const fake = "sk-" + "B".repeat(28);
  const root = tmpRepo({ "clean.py": "def a(): pass\n", "leak.py": `KEY = "${fake}"\n` });
  const r = await reindex(["--key", "k", "--path", root, "clean.py", "leak.py"]);

  const body = r.seen[0].body;
  assert.deepEqual(body.files.map((f) => f.path), ["clean.py"]);
  assert.ok(!JSON.stringify(body).includes(fake), "the secret must never leave the machine");
  // and skipping it must NOT be mistaken for a deletion: leak.py is still on disk, so telling the server to
  // drop it would quietly erase real code from the graph.
  assert.equal("removed" in body, false);
  assert.match(r.stdout, /live secret/);
  assert.equal(r.code, 0);
});

test("nothing changed → no request at all, not an empty rebuild", async () => {
  // The dangerous shape here is POSTing {files: []} — /reindex would drop nothing, but the same reflex on the
  // /sync path is exactly how a graph gets gutted. When there is nothing to say, say nothing.
  const root = tmpRepo({});
  const r = await reindex(["--key", "k", "--path", root]);
  assert.deepEqual(r.seen, []);
  assert.equal(r.code, 0);
  assert.match(r.stdout, /Nothing changed/);
});

test("a named path with nothing ingestable makes no call either", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await reindex(["--key", "k", "--path", root, "/etc/hosts"]);
  assert.deepEqual(r.seen, [], "an out-of-repo path is the whole request → nothing to send");
  assert.match(r.stdout, /Nothing ingestable/);
});

test("a server error exits NON-ZERO — never a silent green", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await reindex(["--key", "k", "--path", root, "a.py"], (res) => {
    res.writeHead(500, JSON_HEADERS);
    res.end(JSON.stringify({ error: { message: "reindex blew up" } }));
  });
  // A green exit on a failed reindex means CI passes while the graph silently goes stale, and the gate then
  // grounds against code that no longer exists.
  assert.equal(r.code, 1);
  assert.match(r.stdout, /HTTP 500/);
  assert.match(r.stdout, /reindex blew up/);            // the OBJECT envelope is unwrapped, not "[object Object]"
  assert.doesNotMatch(r.stdout, /Memory current/);
});

test("an unreachable Estelle exits NON-ZERO too", async () => {
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await runCli(["reindex", "--key", "k", "--path", root, "a.py"], "http://127.0.0.1:1/mcp");
  assert.equal(r.code, 1);
  assert.match(r.stdout, /connection failed/);
  assert.doesNotMatch(r.stdout, /Memory current/);
});

test("--dry-run sends nothing — the flag survives the parser, so it must be honoured", async () => {
  // Consequence of teaching positionalPaths which flags take a value: `--dry-run` used to swallow the path
  // behind it, so the run did nothing by accident. Now the path survives — and a flag the help advertises
  // must not end in an upload the user explicitly said not to make.
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", root, "--dry-run", "a.py"]);
  assert.deepEqual(r.seen, [], "a dry run must reach the network never");
  assert.match(r.stdout, /1 changed/, "…but it still says what it WOULD have sent");
  assert.equal(r.code, 0);
});

test("no key is a RED exit — for reindex and for sweep alike", async () => {
  // "Need --key" then exit 0 is the worst shape a CI step can have: the pipeline is green, the memory was
  // never touched, and every later gate grounds against a graph that stopped moving. Same door, same rule as
  // an API failure — sweep is checked here because it is the same one-line omission in the same file.
  // "No key" now means no --key, no ESTELLE_API_KEY *and* nothing saved at ~/.estelle/auth.json — runCli
  // isolates all three. The refusal names every way to supply one, so it is matched on its stable half.
  const root = tmpRepo({ "a.py": "def a(): pass\n" });
  const noKey = await runCli(["reindex", "--path", root, "a.py"], "http://127.0.0.1:1/mcp");
  assert.match(noKey.stdout, /Need an Estelle key to reindex/);
  assert.equal(noKey.code, 1);

  const sweepNoKey = await runCli(["sweep", "--path", root], "http://127.0.0.1:1/mcp");
  assert.match(sweepNoKey.stdout, /Need an Estelle key to sweep/);
  assert.equal(sweepNoKey.code, 1);
});

// ── the subdirectory trap, now closed ─────────────────────────────────────────────────────────────────
// This one guards the worst bug reindex can have: telling the server to delete a file that exists.
//
// `git diff --name-only HEAD` prints paths relative to the REPO ROOT, while `git ls-files --others` prints
// them relative to the CWD. cmdReindex used to resolve both against `--path`/cwd. Run `estelle reindex` from
// any subdirectory and a modified top-level file resolved to a path that does not exist, so it was classified
// as DELETED and sent in `removed` — while the CLI printed "the rest of the graph is intact."
test("reindex run from a SUBDIRECTORY must not report live files as deleted", async () => {
  const root = tmpRepo({ "top.py": "def a(): pass\n", "sub/deep/b.py": "def b(): pass\n" });
  gitCommit(root);
  fs.writeFileSync(path.join(root, "top.py"), "def a(): changed\n");          // modified, still very much alive

  const r = await reindex(["--key", "k", "--repo", "owner/name", "--path", path.join(root, "sub", "deep")]);
  const body = (r.seen[0] || {}).body || {};
  assert.deepEqual(body.removed, undefined, "a modified file must NEVER be sent as removed");
  assert.ok((body.files || []).every((f) => fs.existsSync(path.join(root, f.path))),
    "every path sent must be the key the sweep stored it under");
});

// ── a missing key is a RED pipeline, never a silent green ─────────────────────────────────────────
// `gate` and `verify` are the CI commands. Exiting 0 with no key means: no key -> the gate never ran ->
// the pipeline goes GREEN. That is a fail-OPEN, and it sits three lines above `failClosed`, which exists
// to state the opposite rule — "an error is NEVER a pass. A revoked key, an unpaid plan, a 5xx, or an
// unreachable server all BLOCK." A missing key is the same class and must behave the same way.
test("every keyed command exits non-zero without a key — gate and verify especially", async () => {
  for (const cmd of ["gate", "verify", "ask", "recall"]) {
    const r = await runCli([cmd, "x"], "http://127.0.0.1:1");   // no --key, env key blanked by runCli
    assert.equal(r.code, 1, `\`estelle ${cmd}\` exited ${r.code} with no key — CI would read that as a pass`);
    assert.match(r.stdout, /Need an Estelle key/);
  }
});
