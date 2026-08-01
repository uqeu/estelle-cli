"use strict";
// THE LOCAL APPLY PATH — the primitive the session was missing. /work rendered a diff and threw it away,
// so "accept edits" had nothing to accept. These tests pin the SAFETY of writing to a real working tree:
// containment, the clobber refusal, the min(local, server) gate, and reversibility.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const apply = require("../bin/apply.js");

const C = new Proxy({}, { get: () => (t) => String(t) });

/** A throwaway git repo with one committed file — the only honest way to test a patch application. */
function repo(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-apply-"));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8",
                                                      env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null",
                                                             GIT_CONFIG_SYSTEM: "/dev/null" } });
  git("init", "-q");
  git("config", "user.email", "t@t.t");
  git("config", "user.name", "t");
  for (const [p, body] of Object.entries(files || {})) {
    fs.mkdirSync(path.dirname(path.join(root, p)), { recursive: true });
    fs.writeFileSync(path.join(root, p), body);
  }
  git("add", "-A");
  git("commit", "-qm", "base");
  return { root, git };
}

const HELLO = "line one\nline two\nline three\n";
const PATCH = [
  "diff --git a/a.txt b/a.txt",
  "index 1111111..2222222 100644",
  "--- a/a.txt",
  "+++ b/a.txt",
  "@@ -1,3 +1,3 @@",
  " line one",
  "-line two",
  "+LINE TWO",
  " line three",
  "",
].join("\n");

// ── reading the patch ───────────────────────────────────────────────────────────

test("the targets of a patch are read from its headers, with what happens to each", () => {
  assert.deepEqual(apply.patchTargets(PATCH), [{ path: "a.txt", kind: "modify" }]);
});

test("a new file and a deleted file are distinguished from a modification", () => {
  const add = ["diff --git a/new.py b/new.py", "--- /dev/null", "+++ b/new.py", "@@ -0,0 +1 @@", "+x = 1"].join("\n");
  const del = ["diff --git a/old.py b/old.py", "--- a/old.py", "+++ /dev/null", "@@ -1 +0,0 @@", "-x = 1"].join("\n");
  assert.deepEqual(apply.patchTargets(add), [{ path: "new.py", kind: "add" }]);
  assert.deepEqual(apply.patchTargets(del), [{ path: "old.py", kind: "delete" }]);
});

test("a patch with no file headers yields no targets — it is never treated as an empty success", () => {
  assert.deepEqual(apply.patchTargets("just some prose\n"), []);
  assert.deepEqual(apply.patchTargets(""), []);
});

// ── containment ─────────────────────────────────────────────────────────────────

test("a patch that writes OUTSIDE the repo root is refused, and says which path", () => {
  // The same rule `reindex` and the PostToolUse sync hook already enforce. A diff is attacker-reachable
  // (it comes back over the wire), so this is the boundary, not a formality.
  const escape = ["--- a/../../etc/passwd", "+++ b/../../etc/passwd", "@@ -1 +1 @@", "-a", "+b"].join("\n");
  const why = apply.unsafePatch("/tmp/repo", escape);
  assert.match(why, /outside/i);
  assert.match(why, /passwd/);
});

test("an ABSOLUTE path in a patch header is refused", () => {
  const abs = ["--- a/etc/passwd", "+++ /etc/passwd", "@@ -1 +1 @@", "-a", "+b"].join("\n");
  assert.match(apply.unsafePatch("/tmp/repo", abs), /outside|absolute/i);
});

test("a patch with nothing to apply is refused rather than reported as applied", () => {
  assert.match(apply.unsafePatch("/tmp/repo", "no headers here"), /no file/i);
});

test("an ordinary in-repo patch is safe", () => {
  assert.equal(apply.unsafePatch("/tmp/repo", PATCH), "");
});

// ── the clobber refusal ─────────────────────────────────────────────────────────

test("a target with uncommitted local edits is a CONFLICT — named, never clobbered", () => {
  const porcelain = " M a.txt\n?? scratch.md\n M b/c.py\n";
  assert.deepEqual(apply.conflicts([{ path: "a.txt" }, { path: "d.txt" }], porcelain), ["a.txt"]);
  assert.deepEqual(apply.conflicts([{ path: "b/c.py" }], porcelain), ["b/c.py"]);
});

test("an untracked file the patch CREATES is a conflict — applying would overwrite it", () => {
  assert.deepEqual(apply.conflicts([{ path: "scratch.md", kind: "add" }], "?? scratch.md\n"), ["scratch.md"]);
});

test("a clean tree conflicts with nothing", () => {
  assert.deepEqual(apply.conflicts([{ path: "a.txt" }], ""), []);
});

test("staged-and-clean is still a conflict — a staged edit is uncommitted work too", () => {
  assert.deepEqual(apply.conflicts([{ path: "a.txt" }], "M  a.txt\n"), ["a.txt"]);
});

// ── the gate: min(local, server) ────────────────────────────────────────────────

test("a read_only LOCAL mode refuses the write — the user asked for that", () => {
  const d = apply.applyDecision("read_only", "execute");
  assert.equal(d.decision, "refuse");
  assert.match(d.why, /read_only/);
});

test("a read_only ACCOUNT can never auto-apply, no matter what the client says", () => {
  // The whole point of the ceiling. A client-side toggle that wrote to disk on a read_only account would
  // be the bypass the product's safety claim rests on not existing.
  const d = apply.applyDecision("execute", "read_only");
  assert.equal(d.decision, "refuse");
  assert.match(d.why, /account|dial/i);
});

test("propose applies, but ALWAYS with a human confirmation — the ADR 0012 default", () => {
  assert.equal(apply.applyDecision("propose", "propose").decision, "confirm");
  assert.equal(apply.applyDecision("propose", "execute").decision, "confirm");
});

test("branch and above may auto-apply — that rung is where a human already acknowledged the risk", () => {
  assert.equal(apply.applyDecision("branch", "branch").decision, "auto");
  assert.equal(apply.applyDecision("execute", "execute").decision, "auto");
});

test("the LOWER of the two always wins", () => {
  assert.equal(apply.applyDecision("execute", "propose").decision, "confirm");
  assert.equal(apply.applyDecision("propose", "execute").decision, "confirm");
});

test("an UNVERIFIED dial degrades to confirm — never to auto", () => {
  // "I could not check" is not "you may." The human typing y is the trusted trigger; silence is not.
  const d = apply.applyDecision("execute", "");
  assert.equal(d.decision, "confirm");
  assert.match(d.why, /unverified|unknown/i);
});

// ── applying, for real ──────────────────────────────────────────────────────────

test("a patch is applied to the working tree and the file actually changes", () => {
  const { root } = repo({ "a.txt": HELLO });
  const r = apply.applyPatch(PATCH, { root });
  assert.equal(r.ok, true, r.error);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "line one\nLINE TWO\nline three\n");
  assert.deepEqual(r.applied, ["a.txt"]);
});

test("a patch that does not fit is refused WITHOUT touching the tree", () => {
  const { root } = repo({ "a.txt": "totally different\n" });
  const r = apply.applyPatch(PATCH, { root });
  assert.equal(r.ok, false);
  assert.ok(r.error, "a failure must carry git's reason, not a bare false");
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "totally different\n");
});

test("every apply is reversible — the undo record restores the file byte for byte", () => {
  const { root } = repo({ "a.txt": HELLO });
  const undoDir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-undo-"));
  const r = apply.applyPatch(PATCH, { root, undoRoot: undoDir });
  assert.equal(r.ok, true, r.error);
  assert.ok(r.undo, "an apply with no undo record is not something to ship");
  const back = apply.restoreUndo(r.undo);
  assert.deepEqual(back.errors, []);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), HELLO);
});

test("undoing a patch that CREATED a file removes it — restoring 'absent' is still a restore", () => {
  const { root } = repo({ "a.txt": HELLO });
  const undoDir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-undo-"));
  const add = ["diff --git a/new.txt b/new.txt", "new file mode 100644", "--- /dev/null", "+++ b/new.txt",
               "@@ -0,0 +1 @@", "+created", ""].join("\n");
  const r = apply.applyPatch(add, { root, undoRoot: undoDir });
  assert.equal(r.ok, true, r.error);
  assert.equal(fs.existsSync(path.join(root, "new.txt")), true);
  apply.restoreUndo(r.undo);
  assert.equal(fs.existsSync(path.join(root, "new.txt")), false);
});

test("the newest undo record is the one /undo reaches for", () => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-undos-"));
  for (const name of ["20260101-000000-aaa", "20260703-121212-bbb", "20260202-000000-ccc"]) {
    fs.mkdirSync(path.join(base, name));
    fs.writeFileSync(path.join(base, name, "manifest.json"), "{}");
  }
  assert.equal(apply.latestUndo(base), path.join(base, "20260703-121212-bbb"));
  assert.equal(apply.latestUndo(path.join(base, "nope")), "");
});

test("the root is the repo TOP-LEVEL, so applying from a subdirectory still lands correctly", () => {
  const { root } = repo({ "deep/nested/a.txt": HELLO });
  // macOS hands out /var/… symlinked to /private/var/…; the comparison is on the resolved path.
  assert.equal(fs.realpathSync(apply.repoRoot(path.join(root, "deep", "nested"))), fs.realpathSync(root));
  assert.equal(apply.repoRoot(os.tmpdir()), "", "outside a repo there is no root, and we say so");
});

test("/undo restores the last apply, and a second /undo does not silently 'succeed' again", () => {
  const { root } = repo({ "a.txt": HELLO });
  const r = apply.applyPatch(PATCH, { root });
  assert.equal(r.ok, true, r.error);
  const first = apply.undoLast(root);
  assert.equal(first.ok, true, first.why);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), HELLO);
  const second = apply.undoLast(root);
  assert.equal(second.ok, false, "a spent record must not be replayed over newer work");
  assert.match(second.why, /nothing to undo/i);
});

// ── the flow a human sees ───────────────────────────────────────────────────────

/** The injected I/O the interactive flow runs on, with everything recorded. */
function harness(over) {
  const out = [];
  return {
    out: (l) => out.push(String(l)), c: C, lines: out,
    prompt: async () => "y",
    ...over,
  };
}

test("no diff at all is a refusal, not a silent success", async () => {
  const h = harness();
  const code = await apply.runApply("", { ...h, root: "/tmp/x", localMode: "propose", serverMode: "execute" });
  assert.notEqual(code, 0, "an error always exits non-zero");
});

test("a confirm-mode apply shows the diff and does nothing when the human says no", async () => {
  const { root } = repo({ "a.txt": HELLO });
  const h = harness({ prompt: async () => "n" });
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "propose", serverMode: "propose" });
  assert.notEqual(code, 0);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), HELLO, "a refusal must write nothing");
  assert.ok(h.lines.join("\n").includes("LINE TWO"), "the diff must be shown before the question");
});

test("a confirm-mode apply writes when the human says yes", async () => {
  const { root } = repo({ "a.txt": HELLO });
  const h = harness();
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "propose", serverMode: "propose",
                                             undoRoot: fs.mkdtempSync(path.join(os.tmpdir(), "u-")) });
  assert.equal(code, 0);
  assert.match(fs.readFileSync(path.join(root, "a.txt"), "utf8"), /LINE TWO/);
});

test("an auto-mode apply never asks", async () => {
  const { root } = repo({ "a.txt": HELLO });
  let asked = 0;
  const h = harness({ prompt: async () => { asked += 1; return "n"; } });
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "branch", serverMode: "branch",
                                             undoRoot: fs.mkdtempSync(path.join(os.tmpdir(), "u-")) });
  assert.equal(code, 0);
  assert.equal(asked, 0, "auto means auto — asking anyway would make the rung meaningless");
  assert.match(fs.readFileSync(path.join(root, "a.txt"), "utf8"), /LINE TWO/);
});

test("a read_only account is refused even in 'auto' — and is told where the refusal came from", async () => {
  const { root } = repo({ "a.txt": HELLO });
  const h = harness();
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "execute", serverMode: "read_only" });
  assert.notEqual(code, 0);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), HELLO);
  assert.match(h.lines.join("\n"), /account|dial/i);
});

test("a dirty target is refused by NAME before anything is written", async () => {
  const { root } = repo({ "a.txt": HELLO });
  fs.writeFileSync(path.join(root, "a.txt"), "my own uncommitted work\n");
  const h = harness();
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "branch", serverMode: "branch" });
  assert.notEqual(code, 0);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "my own uncommitted work\n");
  assert.match(h.lines.join("\n"), /a\.txt/);
});

test("a patch that escapes the repo is refused in the flow too, not only in the checker", async () => {
  const { root } = repo({ "a.txt": HELLO });
  const escape = ["--- a/../../evil.txt", "+++ b/../../evil.txt", "@@ -1 +1 @@", "-a", "+b"].join("\n");
  const h = harness();
  const code = await apply.runApply(escape, { ...h, root, localMode: "branch", serverMode: "branch" });
  assert.notEqual(code, 0);
  assert.match(h.lines.join("\n"), /outside/i);
});

test("a confirm that reaches EOF (piped stdin, CI) refuses — silence is never consent", async () => {
  // The CI path: there is no human to answer y, so the answer is no. An auto-apply here would be a write
  // nobody authorised, which is the whole thing the ceiling exists to prevent.
  const { root } = repo({ "a.txt": HELLO });
  const h = harness({ prompt: async () => null });
  const code = await apply.runApply(PATCH, { ...h, root, localMode: "propose", serverMode: "propose" });
  assert.notEqual(code, 0);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), HELLO);
});
