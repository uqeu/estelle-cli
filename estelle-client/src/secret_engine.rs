//! Secret engine — a Rust port of the vendored whatileaked scanner (MIT © 2026 Selan),
//! matching the estelle repo's committed Python port (`src/estelle/serve/secret_engine.py`).
//!
//! Provenance of the rule data: `secret_rules.json` (next to this crate's manifest, embedded
//! below with `include_str!`) is a byte-identical copy of the estelle repo's generated file —
//! sha256 f16d825ea56117eadc4a6ac870a34045881a6b409c80b3e152ed20d23cb40ec8. Its `metadata`
//! block records the gitleaks pin it was generated from (219 rules, 3 dropped upstream, 22
//! load-time rewrites, gitleaks is MIT-licensed and these are its rule definitions verbatim).
//! The Estelle-local extension rules (LOCAL_RULES in the Python module — Anthropic/OpenAI
//! shapes, Estelle's own key, the private-key header, loose Slack, DSN credentials, Railway,
//! Vercel, and the high-entropy .env assignment) are ported inline below; they are NOT from gitleaks.
//!
//! Architecture (same as the Python port, same as the vendor's `src/scan/`):
//!   - a keyword prefilter selects the rules worth running; each selected rule's pattern runs
//!     over a [`WINDOW_CHARS`]-char window around the keyword hit, never over the whole line;
//!   - the text is scanned in [`CHUNK_CHARS`] chunks with [`OVERLAP_CHARS`] of overlap so no
//!     pathological line pins the CPU and no secret straddling a boundary is split;
//!   - rules compile LAZILY on first use — most of the 230 never fire on a given corpus;
//!   - per-rule entropy gate and per-rule upstream allowlist apply to every candidate;
//!   - a base64 sweep decodes plausible base64 spans and rescans their content, because agent
//!     payloads carry encoded blobs and a secret inside one is invisible to the rules.
//!
//! Regex dialect: gitleaks patterns are Go RE2 regexes, and Rust's `regex` crate is
//! RE2-compatible — it compiles scoped `(?i:…)` AND bare mid-pattern `(?i)` toggles natively,
//! so the Python port's case-fold rewrite is NOT needed here. A pattern that still fails to
//! compile is skipped and recorded (see [`compile_report`]) — never silently dropped.
//!
//! Keyword prefilter: the workspace has no aho-corasick, so — exactly the Python port's
//! measured decision — this uses one lowercased copy plus `str::find` per keyword (the Python
//! side measured a combined keyword alternation at 3.4 s over a 2MB line with zero hits,
//! which trips the fence's DoS budget). The semantics are a strict superset of the
//! alternation's (overlapping keywords each select their own rules), and a superset is the
//! safe direction for a scanner.
//!
//! The public API never carries a secret value: [`SecretFinding`] has the rule id, a 12-hex
//! SHA-256 fingerprint (48 bits — ample to correlate one credential across history, pointless
//! to attack with no plaintext anywhere to confirm a guess against), and a line number.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::OnceLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use regex::Regex;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;

const RULES_JSON: &str = include_str!("../secret_rules.json");

// Tunables, ported verbatim from the vendor (see its keyword-index.ts / scanner.ts).
const WINDOW_CHARS: usize = 512; // kept either side of a keyword hit; the widest gitleaks look-behind is 50 chars
const CHUNK_CHARS: usize = 16 * 1024; // bytes handed to the engine at once
const OVERLAP_CHARS: usize = 1024; // exceeds the longest secret plus the longest look-behind

// A base64'd secret is invisible to every rule, and agent payloads are full of encoded blobs.
// The vendor's guardrails against false positives on ordinary prose/code are the shape itself:
// >= 40 chars of the base64/base64url charset, and the decoded bytes must be clean UTF-8.
static BASE64_RUN: LazyLock<Regex> = LazyLock::new(|| {
    // A compile-time constant pattern — a failure here is a build bug, not a runtime condition.
    #[expect(clippy::expect_used)]
    Regex::new(r"[A-Za-z0-9+/_-]{40,}={0,2}").expect("base64 run pattern")
});

pub fn shannon_entropy(value: &str) -> f64 {
    // Shannon entropy of `value` in bits per character (a randomness proxy: a real key scores
    // high, a dictionary word or template placeholder scores low). Ported from entropy.ts.
    let mut counts: HashMap<char, usize> = HashMap::new();
    let mut n = 0usize;
    for char in value.chars() {
        *counts.entry(char).or_insert(0) += 1;
        n += 1;
    }
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    -counts
        .values()
        .map(|c| {
            let p = *c as f64 / n;
            p * p.log2()
        })
        .sum::<f64>()
}

pub fn fingerprint(secret: &str) -> String {
    // Twelve hex characters of SHA-256 — correlates one credential without storing plaintext.
    let digest = Sha256::digest(secret.as_bytes());
    digest[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// A rule as the generated JSON stores it: plain data, no engine objects, so loading the rule
/// set compiles nothing.
#[derive(Clone, Debug, Deserialize)]
pub struct SecretRule {
    pub id: String,
    pub keywords: Vec<String>, // lowercased literals, one of which must appear before the pattern runs
    pub pattern: String,
    pub group: usize, // capture group holding the secret; 0 means the whole match
    pub entropy: f64, // minimum bits/char for a match to count, or 0 for no threshold
    pub allowlist: Vec<String>, // values upstream says are not credentials (each a regex)
}

#[derive(Deserialize)]
struct RawRuleSet {
    #[expect(
        dead_code,
        reason = "metadata carries the gitleaks pin; runtime reads only rules"
    )]
    metadata: serde_json::Value,
    rules: Vec<SecretRule>,
}

/// The shared rule set: the embedded gitleaks JSON plus the Estelle-local extensions.
pub fn load_rules() -> Vec<SecretRule> {
    // The JSON is embedded at compile time and the generator validates it; a parse failure is a
    // build bug, not a runtime condition.
    #[expect(clippy::expect_used)]
    let raw: RawRuleSet = serde_json::from_str(RULES_JSON).expect("embedded secret_rules.json");
    let mut rules = raw.rules;
    rules.extend(local_rules());
    rules
}

/// Estelle-local extensions, ported from the Python module's LOCAL_RULES. NOT from gitleaks —
/// each exists because the fence contract requires a shape the pinned gitleaks set lacks or
/// covers more narrowly. Keywords and house style follow gitleaks name-anchored rules.
fn local_rules() -> Vec<SecretRule> {
    let rule = |id: &str, keywords: &[&str], pattern: &str, group, entropy, allowlist: &[&str]| {
        SecretRule {
            id: id.to_string(),
            keywords: keywords.iter().map(|k| (*k).to_string()).collect(),
            pattern: pattern.to_string(),
            group,
            entropy,
            allowlist: allowlist.iter().map(|a| (*a).to_string()).collect(),
        }
    };
    vec![
        // Anthropic subscription/OAuth shapes (sk-ant-oat…, sk-ant-sid…) and the lenient
        // >= 80-char body; gitleaks's anthropic rules match only the exact 93-char form.
        rule(
            "anthropic-api-key-loose",
            &["sk-ant-"],
            r"\bsk-ant-(?:api|admin|oat|sid)[0-9]{2}-[A-Za-z0-9_-]{80,}\b",
            0,
            0.0,
            &[],
        ),
        // gitleaks's openai-api-key requires the T3BlbkFJ heritage structure; the fence
        // contract is the plain sk-proj- shape.
        rule(
            "openai-project-key-loose",
            &["sk-proj-"],
            r"\bsk-proj-[A-Za-z0-9_-]{40,}\b",
            0,
            0.0,
            &[],
        ),
        // Classic OpenAI API keys remain valid in customer BYOK configurations. The pinned
        // gitleaks rule recognises only the T3BlbkFJ subset; the classic shape is exactly
        // `sk-` plus 48 alphanumeric characters.
        rule(
            "openai-classic-key",
            &["sk-"],
            r"\bsk-[A-Za-z0-9]{48}\b",
            0,
            0.0,
            &[],
        ),
        // gitleaks's private-key needs the full block (header + 64+ chars + footer) in ONE
        // scanned string; the line-oriented fence must flag the header alone.
        rule(
            "private-key-header",
            &["-----begin"],
            r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----",
            0,
            0.0,
            &[],
        ),
        // The fence's historical slack shape; gitleaks covers the same prefixes with stricter
        // per-type shapes.
        rule(
            "slack-token-loose",
            &["xox"],
            r"\bxox[baprs]-[0-9A-Za-z-]{10,}\b",
            0,
            0.0,
            &[],
        ),
        // Credentials embedded in a datastore connection string — postgres://user:PASSWORD@host.
        // Scoped to datastore schemes, length- and entropy-gated so docs fills like
        // postgres://user:pass@localhost stay clean. Group 1 is the password only.
        rule(
            "dsn-credential",
            &[
                "postgres://",
                "postgresql://",
                "mysql://",
                "mariadb://",
                "mongodb://",
                "redis://",
                "amqp://",
            ],
            r#"(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|redis|amqp)://[^\s:/@'"]+:([^\s:/@'"]{8,})@"#,
            1,
            3.0,
            &[r"(?i)^(?:pass(?:word)?|passwd|user(?:name)?|changeme|secret|xxx+|\$\{?\w+\}?)$"],
        ),
        // Railway and Vercel are not in the pinned gitleaks set at all. Name-anchored house
        // style; the token shapes are the documented ones (Railway API tokens are UUIDs;
        // Vercel tokens are 24-char alnum).
        rule(
            "railway-api-token",
            &["railway"],
            r#"(?i)[\w.-]{0,50}?(?:railway)(?:[ \t\w.-]{0,20})[\s'"]{0,3}(?:=|>|:{1,3}=|\|\||:|=>|\?=|,)[\x60'"\s=]{0,5}([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})"#,
            1,
            3.0,
            &[],
        ),
        rule(
            "vercel-access-token",
            &["vercel"],
            r#"(?i)[\w.-]{0,50}?(?:vercel)(?:[ \t\w.-]{0,20})[\s'"]{0,3}(?:=|>|:{1,3}=|\|\||:|=>|\?=|,)[\x60'"\s=]{0,5}([a-z0-9]{24})"#,
            1,
            3.0,
            &[],
        ),
        // Estelle's own minted API key: exact current format plus a loose companion so a
        // format change degrades to still-caught rather than silently uncovered.
        rule(
            "estelle-live-key",
            &["estelle_live_"],
            r"\bestelle_live_[0-9a-f]{48}\b",
            0,
            0.0,
            &[],
        ),
        rule(
            "estelle-live-key-loose",
            &["estelle_live_"],
            r"\bestelle_live_[A-Za-z0-9_-]{16,}\b",
            0,
            0.0,
            &[],
        ),
        // A bare high-entropy value under a secret-y name — the plain `.env` shape
        // (SOMEKEY=<random>). Gated on length (>= 20), the token charset, and a 3.8 bits/char
        // entropy floor, which placeholders and words do not clear. Group 1 is the value only.
        rule(
            "env-high-entropy-assignment",
            &["key", "secret", "token", "password"],
            r#"(?i)\b[\w.-]*(?:key|secret|token|password)[\w.-]*\s*=\s*['"]?([A-Za-z0-9_+\-/=]{20,})"#,
            1,
            3.8,
            &[],
        ),
    ]
}

/// One rule, compiled the first time it is actually needed, and never rebuilt. A pattern the
/// regex crate cannot compile yields `None` from [`CompiledRule::matcher`] and is recorded in
/// the engine's skip list — never silently dropped.
struct CompiledRule {
    source: SecretRule,
    matcher: OnceLock<Option<Regex>>,
    allowlist: OnceLock<Vec<Regex>>,
}

impl CompiledRule {
    fn new(source: SecretRule) -> Self {
        Self {
            source,
            matcher: OnceLock::new(),
            allowlist: OnceLock::new(),
        }
    }

    fn matcher(&self) -> Option<&Regex> {
        self.matcher
            .get_or_init(|| {
                // A few pinned rules repeat a unicode class up to 1000 times
                // (pypi-upload-token's [\w-]{50,1000}); the counted repetition unrolls and
                // blows the regex crate's default 10MB compiled-size cap. The cap is a
                // memory guard, not a correctness gate — raise it, keep the pattern verbatim.
                regex::RegexBuilder::new(&self.source.pattern)
                    .size_limit(256 * (1 << 20))
                    .build()
                    .ok()
            })
            .as_ref()
    }

    fn allowlist(&self) -> &[Regex] {
        self.allowlist.get_or_init(|| {
            self.source
                .allowlist
                .iter()
                .filter_map(|entry| Regex::new(entry).ok())
                .collect()
        })
    }

    /// Every `(secret, start offset)` this rule finds in `text`, after its entropy threshold
    /// and its upstream allowlist have had their say. The candidate is the rule's `group`
    /// capture when that group exists and participated, else the whole match.
    fn findings_in(&self, text: &str) -> Vec<(String, usize)> {
        let Some(matcher) = self.matcher() else {
            return Vec::new();
        };
        let group = self.source.group;
        let mut out = Vec::new();
        for capture in matcher.captures_iter(text) {
            let found = if group > 0 { capture.get(group) } else { None };
            let found = found.or_else(|| capture.get(0));
            let Some(found) = found else { continue };
            let value = found.as_str();
            if self.source.entropy > 0.0 && shannon_entropy(value) < self.source.entropy {
                continue;
            }
            if self
                .allowlist()
                .iter()
                .any(|allowed| allowed.is_match(value))
            {
                continue;
            }
            out.push((value.to_string(), found.start()));
        }
        out
    }
}

/// Which rules are worth running, and over how little of the text — ported from the vendor's
/// keyword-index.ts. A keyword hit says where the candidate is, so a selected rule's pattern
/// runs over a [`WINDOW_CHARS`] window rather than the whole line.
struct KeywordIndex {
    /// keyword -> rule indices, sorted by keyword for deterministic scans.
    by_keyword: Vec<(String, Vec<usize>)>,
}

impl KeywordIndex {
    fn new(rules: &[CompiledRule]) -> Self {
        let mut by_keyword: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, rule) in rules.iter().enumerate() {
            for word in &rule.source.keywords {
                by_keyword.entry(word.clone()).or_default().push(index);
            }
        }
        Self {
            by_keyword: by_keyword.into_iter().collect(),
        }
    }

    /// `(rule index, slice start, slice end)` — each rule paired with the merged span(s) of
    /// `text` its keyword hits select. Hits within a window's reach merge into one span, so
    /// dense keyword text costs one span per rule, not one per hit.
    fn windows(&self, text: &str) -> Vec<(usize, usize, usize)> {
        let lowered = text.to_lowercase(); // once; offsets below are byte offsets in `text`
        let mut spans: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for (word, owners) in &self.by_keyword {
            let mut search_from = 0;
            while let Some(at) = lowered[search_from..].find(word).map(|i| i + search_from) {
                let start = floor_boundary(text, at.saturating_sub(WINDOW_CHARS));
                let end = ceil_boundary(text, (at + word.len() + WINDOW_CHARS).min(text.len()));
                for &rule in owners {
                    let spans = spans.entry(rule).or_default();
                    match spans.last_mut() {
                        Some(last) if start <= last.1 => last.1 = last.1.max(end),
                        _ => spans.push((start, end)),
                    }
                }
                search_from = at + word.len().max(1);
            }
        }
        let mut out: Vec<(usize, usize, usize)> = spans
            .into_iter()
            .flat_map(|(rule, ranges)| ranges.into_iter().map(move |(s, e)| (rule, s, e)))
            .collect();
        out.sort_unstable();
        out
    }
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Overlapping `(slice, offset)` pairs, sized so no single match runs over a whole 100KB+
/// line. The overlap means a secret near a boundary is found twice, which costs nothing —
/// matches are deduplicated by rule and fingerprint.
fn chunks(text: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        let end = ceil_boundary(text, (offset + CHUNK_CHARS + OVERLAP_CHARS).min(text.len()));
        out.push((&text[offset..end], offset));
        // Byte offsets, char-snapped: the Python port slices by code point, and a chunk edge
        // that lands inside a multibyte char would panic the slice (found by scanning a real
        // ~/.claude log with an emoji at byte 16384+).
        offset = floor_boundary(text, offset + CHUNK_CHARS);
    }
    out
}

fn decode_base64(run: &str) -> Option<String> {
    // Decode a plausible base64/base64url span, or None when it is not text. A blob that was
    // not text round-trips into undecodable bytes; scanning those can only produce noise.
    let translated: String = run
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    let padded = format!(
        "{}{}",
        translated,
        "=".repeat((4 - translated.len() % 4) % 4)
    );
    let raw = STANDARD.decode(padded).ok()?;
    String::from_utf8(raw).ok()
}

/// One kind of secret found in one string. There is deliberately no field here that could
/// carry the secret: this object crosses log lines, receipts and review comments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretFinding<'a> {
    pub rule: &'a str,       // gitleaks rule id (or Estelle-local extension id)
    pub fingerprint: String, // 12 hex of sha256(secret) — correlates, never reveals
    pub line: usize, // 1-based line in the scanned text (the base64 run's line for an encoded finding)
}

/// The redaction marker. Only `[A-Za-z0-9:._-]` — no character JSON would escape, so the
/// marker survives a JSON encode byte-identically and the redacted text can be grepped for it.
fn placeholder(rule: &str, secret: &str) -> String {
    format!("[REDACTED:{rule}:{}]", fingerprint(secret))
}

/// The scanner itself — keyword index + chunking + base64 sweep, over a fixed rule list.
pub struct SecretEngine {
    rules: Vec<CompiledRule>,
    index: KeywordIndex,
    rule_order: HashMap<String, usize>,
}

impl SecretEngine {
    pub fn new(rules: Vec<SecretRule>) -> Self {
        let rules: Vec<CompiledRule> = rules.into_iter().map(CompiledRule::new).collect();
        let index = KeywordIndex::new(&rules);
        let rule_order = rules
            .iter()
            .enumerate()
            .map(|(i, r)| (r.source.id.clone(), i))
            .collect();
        Self {
            rules,
            index,
            rule_order,
        }
    }

    /// Every secret in `text`, deduplicated by (rule, fingerprint), ordered by rule definition
    /// then line. NEVER carries the secret value — that is the whole point of the type.
    pub fn find_secrets<'a>(&'a self, text: &str) -> Vec<SecretFinding<'a>> {
        let line_of = |offset: usize| {
            let end = offset.min(text.len());
            text[..end].bytes().filter(|b| *b == b'\n').count() + 1
        };
        let mut found: HashMap<(String, String), SecretFinding<'a>> = HashMap::new();
        for (chunk, base) in chunks(text) {
            self.collect(chunk, &mut found, &|relative| line_of(base + relative));
            for run in BASE64_RUN.find_iter(chunk) {
                let Some(decoded) = decode_base64(run.as_str()) else {
                    continue;
                };
                // A finding inside a decoded blob is attributed to the run's own line —
                // offsets in decoded space say nothing about the original text.
                let at = base + run.start();
                self.collect(&decoded, &mut found, &|_relative| line_of(at));
            }
        }
        let mut findings: Vec<SecretFinding<'a>> = found.into_values().collect();
        findings.sort_by_key(|f| {
            (
                self.rule_order.get(f.rule).copied().unwrap_or(1 << 30),
                f.line,
            )
        });
        findings
    }

    fn collect<'a>(
        &'a self,
        text: &str,
        found: &mut HashMap<(String, String), SecretFinding<'a>>,
        locate: &dyn Fn(usize) -> usize,
    ) {
        for (rule_index, start, end) in self.index.windows(text) {
            let rule = &self.rules[rule_index];
            for (value, at) in rule.findings_in(&text[start..end]) {
                let key = (rule.source.id.clone(), fingerprint(&value));
                found.entry(key).or_insert_with(|| SecretFinding {
                    rule: &rule.source.id,
                    fingerprint: fingerprint(&value),
                    line: locate(start + at),
                });
            }
        }
    }

    /// The secret VALUES themselves, mapped to the rule that found them — the vendor's
    /// `secretsOf`. This exists because redaction has to know what string to replace; the
    /// result is held in memory long enough to rewrite the text and is never printed, logged,
    /// stored or returned to a caller.
    fn secrets_of(&self, text: &str) -> BTreeMap<String, String> {
        let mut secrets = BTreeMap::new();
        for (chunk, _base) in chunks(text) {
            for (rule_index, start, end) in self.index.windows(chunk) {
                let rule = &self.rules[rule_index];
                for (value, _at) in rule.findings_in(&chunk[start..end]) {
                    secrets
                        .entry(value)
                        .or_insert_with(|| rule.source.id.clone());
                }
            }
            // A secret inside a base64 blob does not appear in the text literally, so there is
            // no substring to replace — the whole run has to go. Missing this leaves behind
            // exactly the findings find_secrets keeps reporting.
            for run in BASE64_RUN.find_iter(chunk) {
                let Some(decoded) = decode_base64(run.as_str()) else {
                    continue;
                };
                for (rule_index, start, end) in self.index.windows(&decoded) {
                    let rule = &self.rules[rule_index];
                    if !rule.findings_in(&decoded[start..end]).is_empty() {
                        secrets
                            .entry(run.as_str().to_string())
                            .or_insert_with(|| rule.source.id.clone());
                    }
                }
            }
        }
        secrets
    }

    /// `text` with every found secret replaced by the redaction marker — a plain textual
    /// substitution (NOT parse-and-reserialise), longest secret first so a secret that is a
    /// substring of another cannot be half-replaced.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        let mut secrets: Vec<(String, String)> = self.secrets_of(text).into_iter().collect();
        secrets.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        for (secret, rule) in secrets {
            out = out.replace(&secret, &placeholder(&rule, &secret));
        }
        out
    }

    /// True when any rule's upstream allowlist says `value` is not a credential (the engine's
    /// allowlists are compiled lazily; the first call may compile them all).
    pub fn is_allowlisted(&self, value: &str) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.allowlist().iter().any(|a| a.is_match(value)))
    }

    /// Force-compile every rule (tests only — production compiles lazily on first keyword
    /// hit) and report `(compiled, skipped rule ids)`. A skipped rule is a rule-set/regex-crate
    /// drift bug; the count and the names make it visible instead of silent.
    pub fn compile_report(&self) -> (usize, Vec<String>) {
        let skipped: Vec<String> = self
            .rules
            .iter()
            .filter(|rule| rule.matcher().is_none())
            .map(|rule| rule.source.id.clone())
            .collect();
        (self.rules.len() - skipped.len(), skipped)
    }
}

static ENGINE: OnceLock<SecretEngine> = OnceLock::new();

/// The process-wide engine over the shared rule set, built once. Rules still compile lazily on
/// first use — building the engine itself is just the keyword index.
pub fn engine() -> &'static SecretEngine {
    ENGINE.get_or_init(|| SecretEngine::new(load_rules()))
}

/// Module-level convenience over [`engine`].
pub fn find_secret_shapes(text: &str) -> Vec<SecretFinding<'static>> {
    engine().find_secrets(text)
}

/// Module-level convenience over [`engine`].
pub fn redact_secrets_engine(text: &str) -> String {
    engine().redact(text)
}

// -------------------------------------------------------------------------------------------------
// Tests. EVERY fixture below is an invented string, constructed to satisfy a rule's shape and
// entropy gate; none is or ever was a real credential.
#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;

    // One invented fixture per rule family the CLI wire claims to cover. The expected rule id is
    // the one the family is KNOWN to resolve to (a supabase service-role key is a JWT; a GCP
    // service-account JSON file is caught by its private-key header). Every fixture is ASSEMBLED
    // from pieces, never written as one literal: a verbatim scanner-shaped token in source trips
    // GitHub push protection (the Python port derives its fixtures for the same reason).
    // Assembled or not, none of these is or ever was a real credential.
    fn families() -> Vec<(&'static str, String)> {
        vec![
            ("slack-bot-token", invented_slack()),
            (
                "gcp-api-key",
                format!("AIza{}", "SyD4iE2xKvF9mQpRtUwYzB3nHjL6sV8cX0a"),
            ),
            (
                "sendgrid-api-token",
                format!(
                    "SG.{}.{}",
                    "xK9mQ2vR7pL4wT8yZ3nB6j", "aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789abcdEfG"
                ),
            ),
            (
                "twilio-api-key",
                format!("SK{}", "9f8e7d6c5b4a39f8e7d6c5b4a39281d0e1"),
            ),
            (
                "npm-access-token",
                format!("npm_{}", "x9mq2vr7pl4wt8yz3nb6jh5sf1dga7k2c0e8"),
            ),
            (
                "pypi-upload-token",
                format!(
                    "pypi-{}",
                    "AgEIcHlwaS5vcmcjE2OTk5NzAwMDAwMDAwMDAwMDBhY2JkZWZnaGlqa2xtbm9wcXJzdA"
                ),
            ),
            (
                "huggingface-access-token",
                format!("hf_{}", "AxKpQmZvRwTyBnLcJdFgHsUiEoPaXqWeRz"),
            ),
        ]
    }

    // The slack fixture is ASSEMBLED, never written as one literal: a verbatim scanner-shaped
    // token in source trips GitHub push protection (the Python port derives its fixtures for
    // the same reason). Assembled or not, it is invented — it was never a real credential.
    fn invented_slack() -> String {
        format!(
            "xoxb-{}-{}-{}",
            "123456789012", "123456789012", "AbCdEfGhIjKlMnOpQrStUvWx"
        )
    }

    fn supabase_service_role_jwt() -> String {
        // Invented header/payload/signature — the payload SAYS service_role, nothing here is real.
        format!(
            "{}.{}.{}",
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImludmVudGVkcHJvaiIsInJvbGUiOiJzZXJ2aWNlX3JvbGUiLCJpYXQiOjE3MDAwMDAwMDAsImV4cCI6MTgwMDAwMDAwMH0",
            "X9mQ2vR7pL4wT8yZ3nB6jH5sF1dG0aKcMeNgRiJkU"
        )
    }

    fn assert_family_fires(text: &str, expected_rule: &str, secret: &str) {
        let findings = find_secret_shapes(text);
        assert!(
            findings.iter().any(|f| f.rule == expected_rule),
            "{expected_rule} did not fire on its fixture; got {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
        // The wire path must drop the VALUE and keep the finding attributable.
        let redacted = redact_secrets_engine(text);
        assert!(
            !redacted.contains(secret),
            "{expected_rule}: value survived"
        );
        assert!(
            redacted.contains(&format!("[REDACTED:{expected_rule}:")),
            "{expected_rule}: marker missing from {redacted}"
        );
        // The finding and the redaction must agree: the wire marker carries the finding's
        // fingerprint. (Group-0 rules with a terminator capture a trailing quote into the
        // matched value — the vendor's semantics — so the fingerprint is of the CAPTURE.)
        let finding = findings.iter().find(|f| f.rule == expected_rule).unwrap();
        let marker = format!("[REDACTED:{}:{}]", expected_rule, finding.fingerprint);
        assert!(
            redacted.contains(&marker),
            "marker {marker} missing from {redacted}"
        );
    }

    #[test]
    fn one_invented_fixture_per_rule_family_fires_and_redacts() {
        for (expected_rule, secret) in families() {
            let text = format!("token: \"{secret}\"\n");
            assert_family_fires(&text, expected_rule, &secret);
        }
        // supabase service-role JWT is, to the rule set, a JWT.
        let jwt = supabase_service_role_jwt();
        assert_family_fires(&format!("Authorization: Bearer {jwt}\n"), "jwt", &jwt);
        // DSN passwords — group 1 redacts the password only, the DSN survives.
        for (scheme, password) in [
            ("postgres://estelle", "Xk9mQ2vR7pL4wT8y"),
            ("mysql://root", "Zx8Qw3Er6Ty9Ui2P"),
        ] {
            let text = format!("DATABASE_URL={scheme}:{password}@db.internal:5432/app\n");
            let findings = find_secret_shapes(&text);
            assert!(
                findings.iter().any(|f| f.rule == "dsn-credential"),
                "dsn-credential missed {scheme}"
            );
            let redacted = redact_secrets_engine(&text);
            assert!(
                !redacted.contains(password),
                "password survived: {redacted}"
            );
            assert!(
                redacted.contains(scheme),
                "the DSN itself should survive: {redacted}"
            );
        }
        // A GCP service-account JSON file is caught by its private-key header line.
        let gcp_json = "{\n  \"type\": \"service_account\",\n  \"project_id\": \"invented-project\",\n  \"private_key\": \"-----BEGIN PRIVATE KEY-----\\nMIIEinvented\\n\"\n}\n";
        let findings = find_secret_shapes(gcp_json);
        assert!(
            findings.iter().any(|f| f.rule == "private-key-header"),
            "private-key-header missed the service-account JSON: {findings:?}"
        );
        // Name-anchored families.
        let datadog = "dd9f8e7d6c5b4a39f8e7d6c5b4a39281706f5e4d";
        assert_family_fires(
            &format!("datadog_api_key = \"{datadog}\"\n"),
            "datadog-access-token",
            datadog,
        );
        let cloudflare = "vr7pl4wt8yz3nb6jh5sf1dg0akcmengrijku2x9q";
        assert_family_fires(
            &format!("CLOUDFLARE_API_KEY = \"{cloudflare}\"\n"),
            "cloudflare-api-key",
            cloudflare,
        );
        let railway = "9f8e7d6c-5b4a-439f-8e7d-6c5b4a392817";
        assert_family_fires(
            &format!("railway_api_token={railway}\n"),
            "railway-api-token",
            railway,
        );
        let vercel = "x9mq2vr7pl4wt8yz3nb6jh5s";
        assert_family_fires(
            &format!("vercel_token = \"{vercel}\"\n"),
            "vercel-access-token",
            vercel,
        );
        // The plain .env shape: SOMEKEY=<high-entropy>.
        let env_value = "X9vQw2Er7Ty5Ui1Op3As6Df8Gh0Jk4Lz";
        assert_family_fires(
            &format!("SOMEKEY={env_value}\n"),
            "env-high-entropy-assignment",
            env_value,
        );
    }

    #[test]
    fn the_fingerprint_is_sha256_of_the_captured_value() {
        // slack-bot-token is group 0 with no trailing terminator, so the captured value IS the
        // fixture — this pins the 12-hex-of-sha256 correlation contract exactly.
        let slack = invented_slack();
        let findings = find_secret_shapes(&format!("token: {slack}\n"));
        let finding = findings
            .iter()
            .find(|f| f.rule == "slack-bot-token")
            .expect("slack fixture");
        assert_eq!(finding.fingerprint, fingerprint(&slack));
    }

    #[test]
    fn mutation_a_deleted_rule_lets_its_family_through() {
        // Prove the instrument can fail: with twilio-api-key removed, its fixture must pass
        // through UNflagged. If this test can never go red, the family coverage above is theatre.
        let twilio = format!("SK{}", "9f8e7d6c5b4a39f8e7d6c5b4a39281d0e1f2a3");
        let text = format!("twilio api key\n{twilio}\n");
        let rules: Vec<SecretRule> = load_rules()
            .into_iter()
            .filter(|rule| rule.id != "twilio-api-key")
            .collect();
        let crippled = SecretEngine::new(rules);
        assert!(
            crippled.find_secrets(&text).is_empty(),
            "the fixture should slip through once its rule is deleted"
        );
        assert!(crippled.redact(&text).contains(&twilio));
        // Positive control in the same test: the full engine DOES catch it.
        assert!(
            engine()
                .find_secrets(&text)
                .iter()
                .any(|f| f.rule == "twilio-api-key")
        );
    }

    #[test]
    fn r11_estelle_and_classic_openai_rules_match_python_with_kill_switches() {
        let classic_body: String = "aB3xK9mQ7wZ2pL5nR8tV".chars().cycle().take(48).collect();
        let classic = format!("sk-{classic_body}");
        let estelle_body: String = "9f86d081884c7d65".chars().cycle().take(48).collect();
        let estelle = format!("estelle_live_{estelle_body}");

        for (family, text, value, expected, dropped) in [
            (
                "OpenAI classic",
                format!("provider value: {classic}\n"),
                classic,
                "openai-classic-key",
                vec!["openai-classic-key"],
            ),
            (
                "Estelle live",
                format!("account value: {estelle}\n"),
                estelle,
                "estelle-live-key",
                vec!["estelle-live-key", "estelle-live-key-loose"],
            ),
        ] {
            assert_family_fires(&text, expected, &value);
            let crippled = SecretEngine::new(
                load_rules()
                    .into_iter()
                    .filter(|rule| !dropped.contains(&rule.id.as_str()))
                    .collect(),
            );
            assert!(
                crippled.redact(&text).contains(&value),
                "{family}: the exact fixture did not reopen after deleting {dropped:?}"
            );
        }

        // Negative controls: prefix prose and near-miss lengths are not credentials.
        for text in [
            "Estelle keys begin with estelle_live_".to_string(),
            "classic OpenAI keys begin with sk-".to_string(),
            format!("sk-{}", "aB3x".repeat(11)), // 44, not the exact 48-character body
            format!("sk-{}x", classic_body),     // 49, not the exact shape
        ] {
            assert!(
                find_secret_shapes(&text).is_empty(),
                "negative fired: {text}"
            );
            assert_eq!(redact_secrets_engine(&text), text);
        }
    }

    #[test]
    fn negative_controls_published_examples_and_checksums_survive() {
        // AWS's own documentation example key: allowlisted upstream, and its entropy actually
        // clears the gate — the ALLOWLIST, not the entropy, is what spares it.
        let example = "AKIAIOSFODNN7EXAMPLE";
        let text = format!("docs say the key looks like {example}\n");
        assert!(find_secret_shapes(&text).is_empty());
        assert_eq!(redact_secrets_engine(&text), text);
        // A git SHA-256 checksum: 64 hex chars, high entropy, no credential shape.
        let sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let text = format!("checksum {sha}\n");
        assert!(
            find_secret_shapes(&text).is_empty(),
            "false positive on a checksum: {:?}",
            find_secret_shapes(&text)
        );
        assert_eq!(redact_secrets_engine(&text), text);
        // Positive control paired with the negatives: the same engine DOES fire on a real
        // shape (real AWS keys are base32-ish — A-Z2-7 only after the prefix). Assembled like
        // every other credential-shaped fixture, so no scanner-shaped literal sits in source.
        let live = format!("AKIA{}", "QF7DMC5BAZ2W7XKP");
        assert!(!find_secret_shapes(&format!("key {live}\n")).is_empty());
    }

    #[test]
    fn base64_sweep_finds_encoded_secrets_and_attributes_the_runs_line() {
        let slack = invented_slack();
        let blob = B64.encode(format!("config note: {slack}"));
        let text = format!("line one is boring\npayload: {blob}\nline three\n");
        let findings = find_secret_shapes(&text);
        let finding = findings
            .iter()
            .find(|f| f.rule == "slack-bot-token")
            .expect("the encoded slack token must be found");
        assert_eq!(finding.line, 2, "attributed to the run's own line");
        assert_eq!(finding.fingerprint, fingerprint(&slack));
        // Redaction removes the whole run — the secret has no literal substring to replace.
        let redacted = redact_secrets_engine(&text);
        assert!(!redacted.contains(&blob));
        assert!(redacted.contains("[REDACTED:slack-bot-token:"));
        assert!(redacted.contains("line three"));
    }

    #[test]
    fn a_multibyte_char_on_a_chunk_boundary_neither_panics_nor_hides_a_secret() {
        // Regression for the probe-found panic: a chunk edge landing inside an emoji used to
        // slice mid-codepoint. The secret sits AFTER the boundary; the overlap must still find it.
        let slack = invented_slack();
        let mut text = "a".repeat(16 * 1024 - 2);
        text.push('🛠'); // bytes 16382..16386 — straddles the 16KB chunk edge
        text.push_str(&"b".repeat(600));
        text.push_str(&format!("\ntoken: {slack}\n"));
        let findings = find_secret_shapes(&text);
        assert!(findings.iter().any(|f| f.rule == "slack-bot-token"));
        let redacted = redact_secrets_engine(&text);
        assert!(!redacted.contains(&slack));
    }

    #[test]
    fn every_rule_compiles_or_is_named() {
        // The regex crate is RE2-compatible, so the expectation is ZERO skipped. If a rule ever
        // fails to compile, this test names it instead of silently dropping the coverage.
        let (compiled, skipped) = engine().compile_report();
        assert_eq!(compiled + skipped.len(), load_rules().len());
        assert!(
            skipped.is_empty(),
            "rules the regex crate rejected: {skipped:?}"
        );
        assert_eq!(compiled, 230); // 219 pinned gitleaks + 11 Estelle-local extensions
    }

    #[test]
    fn the_placeholder_survives_a_json_encode_byte_identically() {
        let marker = placeholder("slack-bot-token", "xoxb-invented");
        assert!(
            marker
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "[]:._-".contains(c))
        );
        let encoded = serde_json::to_string(&marker).expect("json");
        assert!(encoded.contains(&marker));
    }

    #[test]
    fn entropy_gate_holds_back_placeholders() {
        assert!(shannon_entropy("hunter2") < 3.0);
        assert!(shannon_entropy("X9vQw2Er7Ty5Ui1Op3As6Df8Gh0Jk4Lz") > 3.8);
        // hunter2 under a secret-y NAME is still not a secret.
        let text = "PASSWORD = \"hunter2\"\n";
        assert!(find_secret_shapes(text).is_empty());
    }
}
