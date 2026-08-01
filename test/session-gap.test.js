"use strict";
// Session-gap awareness on the customer's machine: how long they were away, what moved underneath them, and
// — mostly — when to say nothing at all.
//
// The properties are the product's own thesis pointed at its welcome message. Every line is a claim, so every
// line is grounded in a git record with an author and a timestamp. Silence is the default. And an unknown
// time zone never becomes a guessed time of day.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const sg = require("../bin/session-gap.js");

const LEFT = "2026-07-31T03:05:00+00:00";     // 23:05 Thursday in Toronto
const NOW = "2026-07-31T11:12:00+00:00";      // 07:12 Friday in Toronto — eight hours later
const TORONTO = "America/Toronto";

const MINE = ["src/estelle/serve/memory_facade.py", "cli/bin/hook.js"];
const DANA = { at: "2026-07-31T08:00:00+00:00", actor: "dana",
               path: "src/estelle/serve/memory_facade.py", what: "batch the per-file reads" };
const REPAIR = { at: "2026-07-31T05:00:00+00:00", actor: "estelle auto-repair",
                 path: "cli/bin/hook.js", what: "repair PR #212" };
const SAM = { at: "2026-07-31T07:00:00+00:00", actor: "sam", path: "web/app/page.tsx", what: "home hero" };

const eightHours = (over) => sg.brief({ now: NOW, lastSeen: LEFT, myFiles: MINE,
                                        changes: [DANA, REPAIR, SAM], tzName: TORONTO, ...over });

test("the eight-hour return, verbatim", () => {
  const out = eightHours();
  assert.equal(out.show, true);
  assert.deepEqual(out.lines, [
    "Welcome back. You were away about 8 hours. It is now 07:12 Friday (America/Toronto).",
    "Code you touched has changed since:",
    "- src/estelle/serve/memory_facade.py — by dana, about 3 hours ago — batch the per-file reads",
    "- cli/bin/hook.js — by estelle auto-repair, about 6 hours ago — repair PR #212",
    "Elsewhere while you were away: 1 change, by sam.",
  ]);
});

test("it stays short, and the files you were in lead", () => {
  const out = eightHours();
  assert.ok(out.lines.length <= 3 + sg.MAX_FILES);
  assert.ok(out.text.indexOf("Code you touched") < out.text.indexOf("Elsewhere while you were away"));
  // Order follows the customer's own touched-file order, so the file they stopped in is named first.
  assert.deepEqual(sg.brief({ now: NOW, lastSeen: LEFT, myFiles: [MINE[1], MINE[0]],
                              changes: [DANA, REPAIR] }).moved, [REPAIR, DANA]);
});

test("a two-minute gap says nothing, and neither does a first session", () => {
  assert.deepEqual(sg.brief({ now: NOW, lastSeen: "2026-07-31T11:10:00+00:00", changes: [DANA] }),
                   { show: false, reason: sg.SILENT_SAME_SESSION, seconds: 120, lines: [], text: "", moved: [] });
  assert.equal(sg.brief({ now: NOW, lastSeen: "", changes: [DANA] }).reason, sg.SILENT_FIRST_SESSION);
  assert.equal(sg.brief({ now: NOW, lastSeen: "   " }).reason, sg.SILENT_FIRST_SESSION);
});

test("the threshold is one working session, not a fresh guess", () => {
  assert.equal(sg.MIN_GAP_SECONDS, 1800);                      // session_diary.DEFAULT_WINDOW_SECONDS
  assert.equal(sg.brief({ now: "2026-07-31T11:41:00+00:00", lastSeen: "2026-07-31T11:12:00+00:00",
                          changes: [DANA] }).reason, sg.SILENT_SAME_SESSION);
});

test("a modest gap must EARN the interruption with something real", () => {
  const quiet = { now: "2026-07-31T11:52:00+00:00", lastSeen: "2026-07-31T11:12:00+00:00" };
  assert.equal(sg.brief({ ...quiet, changes: [] }).reason, sg.SILENT_NO_NEWS);
  const news = { at: "2026-07-31T11:30:00+00:00", actor: "dana", path: "a.py", what: "hotfix" };
  assert.equal(sg.brief({ ...quiet, myFiles: ["a.py"], changes: [news] }).show, true);
});

test("an unknown gap says nothing rather than inventing a narrative", () => {
  assert.equal(sg.brief({ now: NOW, lastSeen: "corrupt-state" }).reason, sg.SILENT_UNKNOWN);
  assert.equal(sg.brief({ now: "not-a-time", lastSeen: LEFT }).reason, sg.SILENT_UNKNOWN);
  assert.equal(sg.brief({ now: NOW, lastSeen: LEFT, enabled: false }).reason, sg.SILENT_DISABLED);
  assert.equal(sg.brief().reason, sg.SILENT_FIRST_SESSION);                  // no arguments at all
});

test("only what provably happened while you were away is reported", () => {
  const stale = { at: "2026-07-30T09:00:00+00:00", actor: "dana", path: MINE[0], what: "last week" };
  assert.equal(eightHours({ changes: [stale] }).moved.length, 0);
  // A change stamped exactly when you left is yours, on the way out — not news.
  assert.equal(eightHours({ changes: [{ ...DANA, at: LEFT }] }).moved.length, 0);
  // Untimed and future-stamped records are dropped, not narrated.
  assert.deepEqual(sg.changesSince([{ actor: "dana", path: "a.py" }], LEFT, NOW), []);
  assert.deepEqual(sg.changesSince([{ at: "2027-01-01T00:00:00Z", path: "a.py" }], LEFT, NOW), []);
  assert.deepEqual(sg.changesSince(["nope", null, DANA], LEFT, NOW), [DANA]);
  assert.deepEqual(sg.changesSince([REPAIR, SAM, DANA], LEFT, NOW), [DANA, SAM, REPAIR]);   // newest first
});

test("'I could not look' is not 'nothing changed'", () => {
  // null = the git read failed. The gap is still true and still worth saying; the reassurance is not.
  const blind = sg.brief({ now: NOW, lastSeen: LEFT, myFiles: MINE, changes: null, tzName: TORONTO });
  assert.equal(blind.lines.length, 1);
  assert.ok(!blind.text.includes("Nothing"));
  // [] = Estelle looked and the repo was quiet, which it MAY say.
  const looked = sg.brief({ now: NOW, lastSeen: LEFT, myFiles: MINE, changes: [], tzName: TORONTO });
  assert.equal(looked.lines[looked.lines.length - 1], "Nothing you track changed while you were away.");
});

test("an unknown time zone never yields a time-of-day greeting", () => {
  const line = eightHours({ tzName: "" }).lines[0];
  assert.match(line, /11:12 Friday UTC \(time zone not set/);
  assert.match(line, /not your local time/);
  for (const word of ["morning", "afternoon", "evening", "night", "late", "early"]) {
    assert.ok(!line.toLowerCase().includes(word), `must not say "${word}" without a zone`);
  }
  assert.match(eightHours({ tzName: "Mars/Olympus" }).lines[0], /time zone not set/);   // junk zone → unknown
  assert.equal(sg.knownZone(""), false);
  assert.equal(sg.knownZone(null), false);
  assert.equal(sg.knownZone("Asia/Kolkata"), true);
  assert.equal(sg.localTimeLine("garbage", TORONTO), "");
  assert.ok(typeof sg.machineZone() === "string");
});

test("the moved-file tail is counted, never dropped in silence", () => {
  const mine = ["a.py", "b.py", "c.py", "d.py", "e.py"];
  const changes = mine.map((p) => ({ at: "2026-07-31T08:00:00+00:00", actor: "dana", path: p }));
  const out = sg.brief({ now: NOW, lastSeen: LEFT, myFiles: mine, changes });
  assert.equal(out.lines.filter((l) => l.startsWith("- ")).length, sg.MAX_FILES);
  assert.ok(out.lines.includes("(+2 more files you touched also changed)"));
  const one = sg.brief({ now: NOW, lastSeen: LEFT, myFiles: mine.slice(0, 4), changes });
  assert.ok(one.lines.includes("(+1 more file you touched also changed)"));
});

test("an unrecorded author is said, never guessed", () => {
  const anon = { at: "2026-07-31T08:00:00+00:00", path: "a.py", what: "a merge" };
  assert.match(sg.brief({ now: NOW, lastSeen: LEFT, myFiles: ["a.py"], changes: [anon] }).text,
               /author not recorded/);
  assert.equal(sg.actorsPhrase([{ actor: "" }, { actor: "dana" }]), "by dana");
  assert.equal(sg.actorsPhrase([]), "");
  assert.equal(sg.actorsPhrase([{ actor: "a" }, { actor: "b" }]), "by a and b");
  assert.equal(sg.actorsPhrase("abcde".split("").map((a) => ({ actor: a }))), "by a, b and c and 2 more");
  assert.equal(sg.restLine([]), "");
  assert.equal(sg.restLine([{ actor: "" }, { actor: "" }]), "Elsewhere while you were away: 2 changes.");
});

test("spans read the way a person would say them", () => {
  assert.equal(sg.humanize(30), "under a minute");
  assert.equal(sg.humanize(60), "1 minute");
  assert.equal(sg.humanize(2700), "45 minutes");
  assert.equal(sg.humanize(3600), "about 1 hour");
  assert.equal(sg.humanize(29160), "about 8 hours");
  assert.equal(sg.humanize(259200), "about 3 days");
  assert.equal(sg.humanize(-5), "under a minute");
  assert.equal(sg.humanize("junk"), "under a minute");
  assert.equal(sg.secondsBetween("junk", NOW), null);
});

test("moved files ignore blanks on both sides and never double-report a path", () => {
  const change = { at: NOW, path: "a.py", actor: "dana" };
  assert.deepEqual(sg.movedFiles(["", "a.py", "a.py", 5], [{ at: NOW, path: "" }, change]), [change]);
  assert.deepEqual(sg.movedFiles(null, null), []);
});

// --- the git half: the only place "your file moved" is actually true ---------------------------------

function tempRepo() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-gap-"));
  const run = (...args) => execFileSync("git", args, { cwd: dir, stdio: "ignore" });
  run("init", "-q");
  run("config", "user.email", "dana@x.io");
  run("config", "user.name", "dana");
  fs.writeFileSync(path.join(dir, "a.py"), "one\n");
  run("add", "-A");
  run("commit", "-qm", "first");
  return { dir, run };
}

test("git is the ground truth for what moved, and who moved it", () => {
  const { dir, run } = tempRepo();
  const before = sg.gitHead(dir);
  assert.deepEqual(sg.collectChanges(dir, before), [], "nothing committed yet → looked, found nothing");
  fs.writeFileSync(path.join(dir, "a.py"), "two\n");
  run("add", "-A");
  run("commit", "-qm", "batch the per-file reads");
  const changes = sg.collectChanges(dir, before);
  assert.equal(changes.length, 1);
  assert.equal(changes[0].path, "a.py");
  assert.equal(changes[0].actor, "dana");
  assert.equal(changes[0].what, "batch the per-file reads");
  assert.ok(Date.parse(changes[0].at) > 0);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("git that cannot answer returns null — never a plausible guess", () => {
  const { dir } = tempRepo();
  assert.equal(sg.collectChanges(dir, ""), null, "no recorded commit → cannot look");
  assert.equal(sg.collectChanges(dir, "0".repeat(40)), null, "unknown commit → cannot look");
  assert.equal(sg.gitHead(os.tmpdir()), null, "not a repo → cannot look");
  assert.equal(sg.collectChanges(os.tmpdir(), "0".repeat(40)), null);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("a rewritten history is refused, not mis-reported", () => {
  // `A..HEAD` across a reset quietly returns a WRONG set rather than erroring, so the ancestry is checked
  // first and a non-ancestor recorded commit produces null.
  const { dir, run } = tempRepo();
  fs.writeFileSync(path.join(dir, "a.py"), "two\n");
  run("add", "-A");
  run("commit", "-qm", "second");
  const orphan = sg.gitHead(dir);
  run("reset", "-q", "--hard", "HEAD~1");
  assert.equal(sg.collectChanges(dir, orphan), null);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("a commit with no timestamp is not evidence", () => {
  assert.deepEqual(sg.parseLog("\x01sha\x1fdana\x1f\x1fsubject\na.py"), []);
  assert.deepEqual(sg.parseLog(""), []);
  assert.deepEqual(sg.parseLog(null), []);
});

test("the welcome decision is silent on every failure", () => {
  const quiet = (deps) => sg.welcome("/tmp/nowhere", NOW, deps);
  assert.equal(quiet({ lastSession: () => null }).show, false, "first session here → silence");
  assert.equal(quiet({ lastSession: () => { throw new Error("bad state"); } }).show, false);
  const previous = { at: LEFT, head: "abc", files: MINE };
  assert.equal(quiet({ lastSession: () => previous,
                       collectChanges: () => { throw new Error("git blew up"); },
                       machineZone: () => TORONTO }).lines.length, 1, "git threw → gap only, no repo claim");
  const full = quiet({ lastSession: () => previous, collectChanges: () => [DANA, REPAIR, SAM],
                       machineZone: () => TORONTO });
  assert.equal(full.lines[0],
               "Welcome back. You were away about 8 hours. It is now 07:12 Friday (America/Toronto).");
});

test("state round-trips per repo, and a missing file is a first session", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-home-"));
  const realHome = os.homedir;
  os.homedir = () => home;
  try {
    assert.equal(sg.lastSession("/repo/a"), null);
    assert.equal(sg.recordSession("/repo/a", ["x.py", "y.py"], LEFT), true);
    assert.equal(sg.recordSession("/repo/b", ["z.py"], NOW), true);
    assert.deepEqual(sg.lastSession("/repo/a").files, ["x.py", "y.py"]);
    assert.equal(sg.lastSession("/repo/a").at, LEFT);
    assert.equal(sg.lastSession("/repo/b").at, NOW, "each repo gets its own gap");
    assert.equal(sg.lastSession("/repo/c"), null);
    assert.equal(sg.recordSession("", ["x.py"], NOW), false, "no cwd → nothing to key on");
    // The tracked-file list is bounded so a marathon session cannot grow the state file without limit.
    sg.recordSession("/repo/a", Array.from({ length: 500 }, (_, i) => `f${i}.py`), NOW);
    assert.equal(sg.lastSession("/repo/a").files.length, sg.MAX_TRACKED_FILES);
    fs.writeFileSync(path.join(home, ".estelle", "last-session.json"), "{not json");
    assert.equal(sg.lastSession("/repo/a"), null, "corrupt state → first session, never a crash");
  } finally {
    os.homedir = realHome;
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test("a paragraph-long commit subject is cut with a visible ellipsis", () => {
  // Real subjects run long — one in this repo is 96 characters. The file and the author, which are what the
  // line is FOR, are never truncated; the subject is, and a cut is never mistaken for the whole thing.
  const long = { at: "2026-07-31T08:00:00+00:00", actor: "dana", path: "a.py", what: "feat: " + "x".repeat(120) };
  const line = sg.brief({ now: NOW, lastSeen: LEFT, myFiles: ["a.py"], changes: [long] }).lines[2];
  assert.ok(line.endsWith("…"));
  assert.equal(line.split(" — ").pop().length, sg.MAX_WHAT);
  assert.ok(line.includes("a.py") && line.includes("by dana"));
  const exact = { ...long, what: "x".repeat(sg.MAX_WHAT) };
  assert.ok(sg.brief({ now: NOW, lastSeen: LEFT, myFiles: ["a.py"], changes: [exact] }).lines[2].endsWith(exact.what));
});
