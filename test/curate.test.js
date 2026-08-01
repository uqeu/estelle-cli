"use strict";
// THE CURATED TURN — the eviction half of the thesis, which only works where Estelle assembles the prompt.
//
// What these tests hold to account is exactly the measured claim and nothing beyond it: a curated turn is
// SMALLER and it RETAINS the lesson. The lever that earns that (`curate_errors`) took a benchmark from 50% to
// 100% at 3.4x fewer tokens; the levers that did not earn anything are deliberately not implemented, so there
// is nothing here asserting that reciting a goal or measuring repetition helps.
const test = require("node:test");
const assert = require("node:assert");
const curate = require("../bin/curate.js");

/** What a failed attempt actually looks like in a session: an intent line, then the wreckage.
 * `tag` makes each attempt's frames identifiable, so a test can say WHOSE mass survived. */
function attempt(what, error, tag, frames = 6) {
  return [`${what}.`, `The run failed: ${error}`, ...Array.from({ length: frames },
    (_, i) => `  File "/repo/src/estelle/serve/sweep.py", line ${120 + i * 7}, in _upload_${tag}\n`
      + `    resp = self._session.post(url, json=body, timeout=self._timeout)`)].join("\n");
}

/** A session that fails four times, learns something each time, then asks a question that needs the record. */
function failingSession() {
  let t = curate.emptyTranscript();
  const attempts = [
    ["why is the sweep slow?", "The sweep walks the tree serially and uploads one file per request."],
    ["try batching the uploads", attempt("Batched the uploads at 500 files", "413 Payload Too Large", "batch")],
    ["try raising the timeout", attempt("Raised the timeout to 120s", "the upload timed out at the proxy", "wait")],
    ["try gzipping the body", attempt("Gzipped the body", "the server refused content-encoding gzip", "gzip")],
    ["try streaming it", attempt("Streamed the upload", "the load balancer denied the connection", "stream")],
    ["what is left to try?", "Let me look at what has not been tried."],
  ];
  for (const [user, assistant] of attempts) {
    t = curate.record(t, { role: "user", content: user });
    t = curate.record(t, { role: "assistant", content: assistant });
  }
  return t;
}

test("a curated turn is SMALLER than the raw transcript and RETAINS the lesson", () => {
  const t = failingSession();
  const raw = t.turns.map((m) => m.content).join("\n\n");
  const set = curate.workingSet(t, { recentTurns: 4 });
  const carried = curate.renderTurn(set, "what should I try next?");

  // SMALLER: the verbose failed attempts are not carried. This is the shape the 3.4x came from — a failed
  // attempt in a real session is a traceback, not a sentence.
  const rawTokens = curate.estimateTokens(raw), curatedTokens = curate.estimateTokens(carried);
  assert.ok(curatedTokens < rawTokens, `curated (${curatedTokens}) must be smaller than raw (${rawTokens})`);
  assert.ok(!carried.includes("_upload_batch"), "the OLD attempt's stack frames are mass, and must not travel");
  assert.ok(!carried.includes("_upload_gzip"), "nor the one after it");
  assert.ok(carried.includes("413 Payload Too Large"), "but the REASON it failed must, inside the lesson");
  assert.ok(carried.includes("_upload_stream"), "the attempt still being worked on stays verbatim");

  // RETAINED: what each attempt TAUGHT survives, which is the whole measured value of the lever.
  assert.ok(set.lessons.length >= 2, `expected lessons, got ${JSON.stringify(set.lessons)}`);
  assert.match(carried, /LESSONS/);
  assert.match(carried, /Batched the uploads/, "the batching lesson must survive the turn it came from");
  assert.match(carried, /Gzipped the body/, "the gzip lesson must survive too");
});

test("HONEST LIMIT: on a session whose failures are already terse, curation costs MORE", () => {
  // The saving comes from evicting MASS. When a failed attempt is a single line, the lesson is nearly as long
  // as the turn it replaces, and the framing (a header, the role labels, the LESSONS block) is pure overhead.
  // Recorded as a test rather than left for someone to discover: the lever is worth what it is worth.
  let t = curate.emptyTranscript();
  for (const line of ["batching failed with a 413", "the timeout expired", "gzip was refused"]) {
    t = curate.record(t, { role: "user", content: "try something" });
    t = curate.record(t, { role: "assistant", content: line });
  }
  const raw = t.turns.map((m) => m.content).join("\n\n");
  const carried = curate.renderTurn(curate.workingSet(t, { recentTurns: 4 }), "next?");
  assert.ok(curate.estimateTokens(carried) > curate.estimateTokens(raw),
    "terse failures: the curated turn is LARGER, and that is the honest result");
});

test("the lesson survives the truncation that deletes the turn — the whole point", () => {
  const t = failingSession();
  // A naive last-N transcript keeps only the tail: every failed attempt is simply gone.
  const naive = t.turns.slice(-4).map((m) => m.content).join("\n");
  assert.ok(!/413|gzip/i.test(naive), "precondition: naive truncation loses the failed attempts");
  // The curated turn keeps the same number of verbatim turns and still knows what was tried.
  const set = curate.workingSet(t, { recentTurns: 4 });
  assert.equal(set.verbatim.length, 4, "same verbatim window as the naive arm");
  assert.ok(set.lessons.some((l) => /413/.test(l)), "and the lesson the naive arm lost");
});

test("a user turn is NEVER evicted as an error — intent is the one thing a session may not lose", () => {
  let t = curate.emptyTranscript();
  t = curate.record(t, { role: "user", content: "the deploy failed and I cannot work out why" });
  for (let i = 0; i < 8; i++) t = curate.record(t, { role: "assistant", content: `step ${i} was fine` });
  const set = curate.workingSet(t, { recentTurns: 2 });
  assert.equal(set.lessons.length, 0, "a user turn full of failure words is intent, not a failed attempt");
});

test("a failure that was RESOLVED is kept, never distilled into a false 'avoid'", () => {
  const turn = { role: "assistant", content: "The import failed, so I added the module - tests now pass." };
  assert.equal(curate.looksLikeError(turn), false);
  const { kept, lessons } = curate.curateErrors([turn, { role: "user", content: "good" }], 0);
  assert.equal(lessons.length, 0);
  assert.equal(kept.length, 2, "both turns survive");
});

test("recent failures are kept VERBATIM — a turn still being worked is not curated away", () => {
  const turns = [{ role: "assistant", content: "old: the build failed with E401" },
                 { role: "assistant", content: "new: the test failed with AssertionError" }];
  const { kept, lessons } = curate.curateErrors(turns, 1);
  assert.equal(kept.length, 1);
  assert.match(kept[0].content, /new:/, "the recent failure stays verbatim");
  assert.equal(lessons.length, 1, "only the old one is distilled");
});

test("lessons outlive the live ring — the 5,000-turn claim, in miniature", () => {
  let t = curate.emptyTranscript();
  t = curate.record(t, { role: "assistant", content: "tried the Redis cache: it failed, connection refused" });
  for (let i = 0; i < curate.MAX_LIVE_TURNS + 20; i++) {
    t = curate.record(t, { role: "user", content: `turn ${i}` });
  }
  assert.equal(t.turns.length, curate.MAX_LIVE_TURNS, "the ring is bounded");
  assert.ok(!t.turns.some((m) => /Redis/.test(m.content)), "the verbose turn fell off the ring");
  assert.ok(t.lessons.some((l) => /Redis/.test(l)), "but what it taught did not");
  assert.match(curate.renderTurn(curate.workingSet(t), "now what?"), /Redis/,
    "and it is still carried into the next turn, 400 turns later");
});

test("the working set is bounded — a huge session cannot produce an unbounded prompt", () => {
  let t = curate.emptyTranscript();
  for (let i = 0; i < 200; i++) t = curate.record(t, { role: "user", content: "x".repeat(4000) });
  const set = curate.workingSet(t, { budgetTokens: 500 });
  assert.ok(set.tokens <= 500, `carried ${set.tokens} tokens, budget was 500`);
});

test("the lessons block is reserved FIRST, so a verbose tail cannot crowd it out", () => {
  let t = curate.emptyTranscript();
  t = curate.record(t, { role: "assistant", content: "tried the socket path: it failed, permission denied" });
  for (let i = 0; i < 6; i++) t = curate.record(t, { role: "user", content: "y".repeat(8000) });
  const set = curate.workingSet(t, { budgetTokens: 400, recentTurns: 6 });
  assert.equal(set.lessons.length, 1, "the lesson is kept even when the recent turns would fill the budget");
  assert.match(curate.renderTurn(set, "next?"), /permission denied/);
});

test("with nothing recorded, a turn is just the question — no ceremony on turn one", () => {
  const set = curate.workingSet(curate.emptyTranscript());
  assert.equal(curate.renderTurn(set, "what does this repo do?"), "what does this repo do?");
  assert.equal(set.tokens, 0);
});

test("lessons land at the tail, immediately before the question", () => {
  const t = curate.record(curate.emptyTranscript(),
    { role: "assistant", content: "tried the flag: it failed, invalid option" });
  const text = curate.renderTurn(curate.workingSet(t, { recentTurns: 0 }), "why?");
  assert.ok(text.indexOf("LESSONS") < text.indexOf("CURRENT QUESTION"),
    "the measured arm put the lessons at the high-attention tail; keep them there");
});

test("a reply's failure is recorded in words the classifier can actually see", () => {
  assert.match(curate.replyText({ error: { message: "402 no provider key" } }), /^failed: /);
  assert.match(curate.replyText({ answer: "here", verdict: "blocked", merge: false }), /failed the merge gate/);
  assert.match(curate.replyText({ answer: "here", ungrounded: ["frobnicate"] }), /not found in this repo/);
  // and each of those is then classified AS a failed attempt, or the lever never fires on real replies
  for (const res of [{ error: { message: "boom" } }, { answer: "x", merge: false, verdict: "blocked" },
                     { answer: "x", ungrounded: ["frob"] }]) {
    assert.ok(curate.looksLikeError({ role: "assistant", content: curate.replyText(res) }),
      `must read as a failed attempt: ${curate.replyText(res)}`);
  }
  // a clean reply is NOT a failed attempt
  assert.equal(curate.looksLikeError({ role: "assistant",
    content: curate.replyText({ answer: "the sweep walks the tree", merge: true }) }), false);
});

test("record and curate are pure — the caller's state is never mutated", () => {
  const before = curate.emptyTranscript();
  const after = curate.record(before, { role: "user", content: "hello" });
  assert.equal(before.turns.length, 0, "the input state is untouched");
  assert.equal(after.turns.length, 1);
  const turns = [{ role: "assistant", content: "it failed hard" }];
  curate.curateErrors(turns, 0);
  assert.equal(turns.length, 1, "curateErrors does not mutate its input");
  // a blank turn is not a turn
  assert.equal(curate.record(after, { role: "user", content: "   " }).turns.length, 1);
});

test("the /context receipt says exactly what the next turn carries", () => {
  const report = curate.contextReport(failingSession(), { recentTurns: 4 });
  assert.equal(report.total, 12);
  assert.match(report.lines.find((l) => l[0] === "turns")[1], /4 verbatim of 12 recorded/);
  assert.match(report.lines.find((l) => l[0] === "evicted")[1], /distilled/);
  assert.match(report.lines.find((l) => l[0] === "cost")[1], /~\d+ tokens/);
});

test("a lesson names what was tried AND what went wrong", () => {
  const lesson = curate.lessonFrom({ role: "assistant",
    content: "Raised the pool size to 200.\nThe connection was refused by pgbouncer." });
  assert.match(lesson, /^tried: Raised the pool size/);
  assert.match(lesson, /-> failed: The connection was refused/);
  // a one-line failure has nothing to contrast, so it becomes an 'avoid'
  assert.match(curate.lessonFrom({ role: "assistant", content: "the migration timed out" }), /^avoid: /);
});

test("lessons are deduped, so a loop cannot fill the budget with one repeated failure", () => {
  const same = { role: "assistant", content: "tried the retry: it failed, connection refused" };
  const { lessons } = curate.curateErrors([same, { ...same }, { ...same }], 0);
  assert.equal(lessons.length, 1);
});
