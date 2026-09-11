---
description: Put a design decision to every frontier model we hold a key for, with the real code, and report where they DISAGREE
---

Use this at a **design decision point** — not for routine work, and never to ask a model a fact about
this repository. That distinction is the whole discipline:

- ✅ **Ask them to argue about a DESIGN.** "Here is the schema and the write path I intend; break it."
- ⛔ **Never ask them what our code does.** They cannot know, they will answer anyway, and a confident
  hallucination about Estelle's own design is exactly the failure Estelle exists to prevent. Facts about
  this repo come from `verify` / `find_definition` / reading the file.

🔬 **WHY THIS EXISTS, MEASURED 2026-09-07.** Six models were asked to attack a memory-graph design. The
value was not their answers — it was their **contradictions**. Two labs independently killed the same
load-bearing assumption (symbol identity was never specified, so every rename manufactures dangling
edges). One corrected a complexity claim that was simply false. One predicted, from the design alone, a
defect that a separate measurement pass found in the data an hour later. **An agreeing panel taught us
nothing; a disagreeing panel found four real bugs before a line was written.**

## How to run it

1. **Write the question to a file.** It MUST contain real code excerpts pasted from the repo — not
   descriptions of code. A model given a description critiques the description.
2. Structure it as: what we do → what we measured → **what I intend to build** → numbered questions that
   can be answered wrongly. End with: *"END WITH: WHAT I WOULD CHALLENGE — the weakest assumption in my
   framing above."* That last instruction is what produces the useful half.
3. Run it — it takes **200–800 seconds**, so background it and poll:

```bash
nohup python3 scripts/panel/ask_frontier.py <question.md> <out_dir> > <out_dir>/run.log 2>&1 &
```

It asks `gpt-6-astra`, `kimi-k3`, `glm-5.3`, `deepseek-v4-pro`, `grok-4.6` and `gemini-3.1-pro` in
parallel and writes one file per model. Keys are sourced from `.env` **by name** and never printed.

## How to read the result back to the user

⛔ **Do not average the panel.** Consensus is the least informative thing it produces.

1. **Lead with CONTRADICTIONS** — where two models disagree, or where one contradicts *me*. That is the
   finding. Quote both sides.
2. **Where they converge INDEPENDENTLY on the same flaw, say so explicitly** — that is the strongest
   signal available and it should change the plan.
3. **Report every model that returned nothing.** A reasoning model spends its budget thinking first and
   can return `finish=length` with an EMPTY body; a panel of six where two were silent is a panel of
   four, and reporting it as six is the vacuity defect this repo has paid for repeatedly.
4. Never present a model's claim about THIS repo as fact. Attribute it, then verify it yourself.
