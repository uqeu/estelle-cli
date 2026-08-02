"use strict";
// ONE definition of "which repo am I in", for every path that writes to Estelle's memory.
//
// It used to be one definition and one ABSENCE, and that is worse than two that disagree. `estelle sweep`
// derives `owner/name` from the git origin remote and sends it as `repo`, so the swept surface lands in the
// repo sub-namespace (`base::owner/name`). The always-on hooks — `scripts/hooks/estelle_hook.py:288` and
// `cli/bin/hook.js` — posted `/reindex` with NO `repo` at all, so `handle_reindex` (api_memory.py:130) wrote
// the BASE namespace, while the PreToolUse gate on the very next edit read the repo-scoped one.
//
// MEASURED ON PROD `dc74bd0c`, with a synthetic symbol that exists in no repo: reindexed WITHOUT `repo` the
// gate still answered `ungrounded`; the identical write WITH `repo` answered `grounded`. So "keep memory
// current as I edit" — the whole purpose of the PostToolUse sync — had never once updated the surface the
// gate reads. Every symbol a customer wrote after their last full sweep was flagged "not defined in this
// repo" on the next edit that used it, which is a false REFUSAL in the path that fires on every edit.
//
// Splitting it into a leaf module is what makes "the sweep and the hook agree about the namespace" a fact
// about the code rather than something to remember at each call site — the same reasoning as `auth.js`.
// Zero dependencies beyond Node built-ins. The pure half (`repoFromRemoteUrl`) is contract-tested against
// the Python hook's `repo_from_remote_url` on the same fixtures, because a DIFFERENT name is the same
// defect as no name: it writes to a namespace nothing reads.

const path = require("path");
const { execFileSync } = require("child_process");

/**
 * `owner/name` from a git remote URL, or "" when the URL is not of that shape.
 *
 * Pure and objective — a string question with a string answer, so it can be contract-tested rather than
 * argued about (PLAN-ERRATA E-007: the objective predicate survived every rewrite; the prose heuristic did
 * not). Handles the three shapes git actually emits:
 *   git@host:owner/name.git · https://host/owner/name.git · ssh://git@host/owner/name
 */
function repoFromRemoteUrl(url) {
  const trimmed = String(url || "").trim().replace(/\/+$/, "").replace(/\.git$/, "");
  if (!trimmed) return "";
  const m = trimmed.match(/[:/]([^/:]+)\/([^/]+)$/);
  return m ? `${m[1]}/${m[2]}` : "";
}

/**
 * The repo name this directory sweeps under: the origin remote's `owner/name`, else the directory name.
 *
 * The fallback matters as much as the happy path — a repo with no origin still has to land SOMEWHERE
 * consistent, and the directory name is what `estelle sweep` already falls back to. Returning "" here
 * instead would put the hook back in base while the sweep sat in `dirname`, recreating the split.
 */
function repoNameFor(root) {
  try {
    const url = execFileSync("git", ["-C", root, "remote", "get-url", "origin"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    const named = repoFromRemoteUrl(url);
    if (named) return named;
  } catch (_) { /* not a git repo, no origin, or no git binary — fall through */ }
  return path.basename(path.resolve(root)) || "";
}

module.exports = { repoFromRemoteUrl, repoNameFor };
