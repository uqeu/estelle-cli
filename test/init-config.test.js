"use strict";
// THE SIBLING OF F0, found by reading openai/codex's config loader and looking back at ours.
//
// `install-hooks` conflated "no settings file" with "a settings file I cannot parse" and destroyed the
// customer's Claude Code config. That was fixed and shipped in 0.1.7. The comment written at the time said
// `init` "had always done it correctly" because `writeClient` copies to `.bak` before writing.
//
// THAT CLAIM WAS HALF TRUE, AND HALF TRUE IS WRONG. `writeClient` (estelle.js:127) has the SAME
// conflation:
//
//     try { existing = JSON.parse(fs.readFileSync(p, "utf8")); } catch (_) { existing = null; }
//
// so an unparseable editor MCP config is silently replaced by an Estelle-only one. The `.bak` does save
// the bytes — but the report line is `backup: existing !== null`, which is FALSE in exactly the
// unparseable case, so the "(backed up)" note is suppressed precisely when the customer needs to be told
// their file was replaced and where the copy is.
//
// ONE PREDICATE DOING DOUBLE DUTY: `existing !== null` means "a prior file parsed" and is reported as "we
// backed up". Those agree in every case except the one that matters.
//
// These tests drive the REAL `init` command through a temp HOME, for the same reason the F0 tests do: the
// defect is in the wiring, and a helper test would pass throughout.

const { test } = require("node:test");
const assert = require("node:assert");
const { execFile } = require("node:child_process");
const http = require("node:http");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const CLI = path.join(__dirname, "..", "bin", "estelle.js");
const REL = ".cursor/mcp.json";           // the cursor client's config, relative to HOME

function withHome(body) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-init-"));
  const file = path.join(home, REL);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  if (body !== null) fs.writeFileSync(file, body);
  return { home, file };
}

// `init` ends by VERIFYING that Estelle actually answers, and sets exitCode 1 when it does not. That is
// correct behaviour, but it would make these tests assert on the network instead of on the config write —
// so they stand up a local MCP stub and point the CLI at it with ESTELLE_MCP_URL, the same way
// reindex.test.js:165 and sweep-size.test.js:105 already do. Reusing that spelling on purpose: a second
// way to stub one thing is the same defect class as a second way to resolve one path.
function withStub(fn) {
  const server = http.createServer((req, res) => {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-03-26" } }));
  });
  return new Promise((resolve, reject) => {
    server.listen(0, "127.0.0.1", async () => {
      const url = `http://127.0.0.1:${server.address().port}/mcp`;   // port 0 -> the OS picks a free one
      try { resolve(await fn(url)); } catch (e) { reject(e); } finally { server.close(); }
    });
  });
}

// ASYNC on purpose, and this cost a real debugging loop worth recording. The first version used
// execFileSync — which BLOCKS the Node event loop, so the stub server above (same process) could never
// accept the CLI's request. The CLI then sat out its own 15s AbortSignal on every case and the file blew
// past the runner's timeout. A synchronous child + an in-process server is a deadlock, not a slow test.
function run(home, url) {
  return new Promise((resolve) => {
    execFile(process.execPath, [CLI, "init", "--client", "cursor", "--key", "ek-test-123"],
             { env: { ...process.env, HOME: home, NO_COLOR: "1", ESTELLE_API_KEY: "", ESTELLE_MCP_URL: url },
               encoding: "utf8" },
             (err, stdout, stderr) => resolve({ code: err ? (err.code === undefined ? 1 : err.code) : 0,
                                                stdout: (stdout || "") + (stderr || "") }));
  });
}

test("an unparseable editor config is NOT silently replaced", () => withStub(async (url) => {
  const original = '{\n  "mcpServers": {"mine": {"url": "http://localhost:1"}},\n}\n';   // trailing comma
  const { home, file } = withHome(original);
  const r = await run(home, url);
  assert.strictEqual(fs.readFileSync(file, "utf8"), original,
                     "the customer's config was overwritten by one it could not read");
  assert.notStrictEqual(r.code, 0, "it must exit non-zero rather than report success");
}));

// THE PAIRED POSITIVE. A guard that refuses every config is the same defect with the sign flipped.
test("a valid editor config still gets Estelle merged in, keeping the customer's own servers", () => withStub(async (url) => {
  const { home, file } = withHome('{\n  "mcpServers": {"mine": {"url": "http://localhost:1"}}\n}\n');
  const r = await run(home, url);
  assert.strictEqual(r.code, 0, "a valid config must still be written");
  const after = JSON.parse(fs.readFileSync(file, "utf8"));
  assert.ok(after.mcpServers.mine, "the customer's own MCP server was dropped");
  assert.ok(after.mcpServers.estelle, "estelle was not added");
  assert.ok(fs.existsSync(file + ".bak"), "an existing file must be backed up before it is rewritten");
}));

test("a first-run install with no config at all still works", () => withStub(async (url) => {
  const { home, file } = withHome(null);
  assert.strictEqual((await run(home, url)).code, 0, "absent config is the normal first run, not an error");
  assert.ok(JSON.parse(fs.readFileSync(file, "utf8")).mcpServers.estelle, "estelle was not added");
  assert.ok(!fs.existsSync(file + ".bak"), "there was nothing to back up");
}));
