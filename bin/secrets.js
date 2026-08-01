"use strict";
// ONE definition of "this must never leave the machine", for every path that uploads a file.
//
// It used to be three: `estelle sweep` had this rule, the shipped hook had none at all, and the dev hook had
// none either — so the ALWAYS-ON path, the one that fires on every edit without anyone choosing it, was the
// only path with no check. A file the sweep would have refused to upload was uploaded anyway the moment an
// agent edited it. Splitting the rule out into its own leaf module is what makes "all three agree" a fact
// about the code rather than a promise in a comment.
//
// It matches key SHAPES, and it is deliberately NOT a general secret scanner: `PASSWORD=hunter2` is not a
// shape and will travel. It is a last-line refusal for credentials that are unmistakably live, not a licence
// to ingest anything it does not match. The extension allowlist and the repo-boundary check do the rest.

// Kept in sync with scripts/hooks/estelle_hook.py's SECRET_RE — the contract test in
// tests/test_hook_contract.py runs both against the same fixtures and fails if they ever disagree.
const SECRET_RE = /sk-[A-Za-z0-9_-]{20,}|sk_live_[A-Za-z0-9]{10,}|ghp_[A-Za-z0-9]{36}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----/;

/** True when `text` embeds something shaped like a live credential. Pure. */
function hasSecret(text) {
  return SECRET_RE.test(String(text || ""));
}

module.exports = { SECRET_RE, hasSecret };
