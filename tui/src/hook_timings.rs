//! The context hook's own flight recorder.
//!
//! 🔴 **THE HOOK COULD SAY IT GAVE UP AND NOT WHERE THE TIME WENT.** The founder sees
//! *"Estelle did not ground this turn: it did not answer within the 20s this hook allows
//! itself"* repeatedly, and that sentence is unactionable: it names OUR deadline and nothing
//! about the work. Six measurements on 2026-09-07 put every reachable condition at **5–9 s**
//! against that 20 s budget (server-side `/search` 1.7 s recall / 5 s wall warm, 7.6 s cold;
//! npx vs direct binary 5.36 s vs 5.09 s; four concurrent searches 5.19 s; two 25 s syncs in
//! flight 5.02 s; a real 1,613-char prompt 8.67 s cold, 4.99 s warm; cold npx resolve 5.48 s) —
//! so the abandonment is NOT reproducible on demand, and the only way it becomes diagnosable is
//! if the run that fails leaves a record behind.
//!
//! ⚠️ **AND THE ANSWER WAS ALREADY ON THE WIRE, UNREAD.** `POST /search` returns a `timings`
//! object — `auth`, `concurrency_slot`, `account_admission`, `recall.embedder_init`,
//! `recall.dense`, `recall.sparse`, `recall.rerank_provider`, `recall.retrieve_context`, plus
//! `elapsed_s` and `unattributed_s`. The hook parsed `recall` and dropped the rest on the floor,
//! every prompt, for as long as the field has existed.
//!
//! ⚠️ **THE LIMIT, SAID OUT LOUD AND FIRST.** A timeout means **no response arrived**, so there
//! is no server `timings` block for the run that failed — this module CANNOT name a server stage
//! for an abandoned call, and anything claiming to would be a fabricated cause on a real symptom.
//! What it can do is exactly two things, and it claims exactly those two:
//!   1. Report the phase THIS PROCESS reached and the REAL elapsed time (measured, not the budget
//!      restated), so "we never got a response head" is distinguishable from "we parsed a slow
//!      one".
//!   2. Carry the last ANSWERED call's server stage breakdown forward, so the next abandonment is
//!      read next to the most recent profile of the same endpoint from the same machine.
//!
//! That is a breadcrumb, not a diagnosis, and the caller's wording says so: it reports the
//! slowest stage of the last ANSWERED call, never a cause for the abandoned one.
//!
//! Deliberately NOT raising the budget: the work fits inside it in every condition anyone has
//! been able to measure, so a bigger budget hides the defect instead of fixing it.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use serde_json::json;

/// Hard ceiling on the live breadcrumb file before it rotates. Power-of-Ten rule 3: the resource
/// is bounded BEFORE it is taken, and the bound is a named constant.
///
/// One record is ~200–600 bytes, so 64 KiB holds on the order of 150 recent prompts — enough to
/// see a pattern, small enough that a laptop never notices. At most `2 ×` this ever exists on
/// disk (the live file plus one rotation), which is the whole retention policy.
const MAX_LOG_BYTES: u64 = 64 * 1024;

/// How many trailing bytes [`last_answered`] will read back. A bounded read, and a bounded read
/// means **"cannot answer"** — never "that's all there is": when no answered record is found in
/// this window the summary says the window was empty, it does not claim there were no calls.
const TAIL_READ_BYTES: u64 = 16 * 1024;

/// Fixed bound on the backwards scan of the tail (Power-of-Ten rule 2). At ~200 bytes a record
/// this is far more lines than `TAIL_READ_BYTES` can hold, so the bound is slack by construction
/// and exists to make the loop's termination independent of the file's contents.
const MAX_TAIL_LINES: usize = 512;

/// Which side of the call the hook was on when it stopped. **This is a fact about THIS process**,
/// which is why it is the only "stage" an abandoned run is allowed to name.
///
/// 🔑 ONE MEANING PER NAME. `AwaitingResponse` says the request was issued and nothing came back
/// — it does NOT say the server was slow, the network was slow, or the request was even received.
/// Those are four different facts and this process cannot tell them apart, so the variant is named
/// for what it observed rather than for a cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    /// The request was in flight and no response head had arrived when the budget expired.
    AwaitingResponse,
    /// A response arrived and was parsed. The server's own `timings` may accompany it.
    Answered,
    /// The transport itself failed (refusal, 429, connection error) before any parse.
    TransportFailed,
}

impl Phase {
    /// The wire spelling. Kept separate from `Debug` so a derive change cannot silently rewrite
    /// a field that on-disk records already carry.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingResponse => "awaiting_response",
            Self::Answered => "answered",
            Self::TransportFailed => "transport_failed",
        }
    }
}

/// One line of the flight recorder.
#[derive(Clone, Debug)]
pub struct Breadcrumb {
    pub phase: Phase,
    /// MEASURED wall time for the network half, not the budget restated. The distinction is the
    /// point: a run that abandons at 20.0 s and a run that abandons at 4.1 s because the budget
    /// was mis-read are the same sentence today and different numbers here.
    pub elapsed: Duration,
    pub budget: Duration,
    /// The server's own `timings` object, verbatim, when one was returned. `None` on every
    /// abandoned call — see the module's stated limit.
    pub server_timings: Option<Value>,
}

impl Breadcrumb {
    /// The JSON line written to disk. Kept to scalars plus the server's own object so a reader
    /// with `jq` and no source tree can still use it.
    fn to_line(&self) -> String {
        let mut record = json!({
            "ts": unix_millis(),
            "phase": self.phase.as_str(),
            "elapsed_ms": u64::try_from(self.elapsed.as_millis()).unwrap_or(u64::MAX),
            "budget_ms": u64::try_from(self.budget.as_millis()).unwrap_or(u64::MAX),
        });
        if let Some(timings) = &self.server_timings
            && let Some(object) = record.as_object_mut()
        {
            object.insert("server_timings".to_string(), timings.clone());
        }
        format!("{record}\n")
    }
}

/// Milliseconds since the epoch, saturating rather than panicking on a clock before 1970.
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

/// The breadcrumb file — resolved on CALL, never at module load, for the same reason
/// `hook_distil::spill_dir` is: a home lookup can block in a container in a way it never does on
/// a laptop.
pub fn log_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("context-hook.jsonl"))
}

/// Append one record, rotating first if the live file has reached [`MAX_LOG_BYTES`].
///
/// **Best-effort by construction.** Every fallible step degrades to "no breadcrumb", never to a
/// failed hook: this runs on the hot path of every prompt, and a flight recorder that can ground
/// the aircraft is worse than none. `record` is the caller's, unmodified.
pub fn append(record: &Breadcrumb, path: Option<&Path>) {
    let Some(target) = path.map(PathBuf::from).or_else(log_path) else {
        return;
    };
    let Some(parent) = target.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    rotate_if_full(&target);
    let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
    else {
        return;
    };
    let _ = file.write_all(record.to_line().as_bytes());
    restrict(&target);
}

/// Move the live file aside once it reaches the cap, so at most `2 × MAX_LOG_BYTES` is ever held.
///
/// 🔑 A rename, not a truncate: a truncate races an in-flight append and can leave a half line
/// that then poisons every later parse of the file.
fn rotate_if_full(target: &Path) {
    let Ok(meta) = fs::metadata(target) else {
        return;
    };
    if meta.len() < MAX_LOG_BYTES {
        return;
    }
    let mut rotated = target.as_os_str().to_os_string();
    rotated.push(".1");
    let _ = fs::rename(target, PathBuf::from(rotated));
}

/// `0600` on unix. The file holds no credential, but it does hold this machine's request timings,
/// and a file this crate creates in `~/.estelle` should not be world-readable by default.
fn restrict(target: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(target, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = target;
}

/// The most recent ANSWERED record in the tail of the live file, rendered as one short clause.
///
/// Returns `None` when the file is absent, unreadable, or holds no answered record **within the
/// bounded window** — and the caller's wording must not turn that `None` into "there were no
/// successful calls", because this function cannot see past [`TAIL_READ_BYTES`].
pub fn last_answered(path: Option<&Path>) -> Option<String> {
    let target = path.map(PathBuf::from).or_else(log_path)?;
    let text = read_tail(&target)?;
    for line in text.lines().rev().take(MAX_TAIL_LINES) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("phase").and_then(Value::as_str) != Some(Phase::Answered.as_str()) {
            continue;
        }
        let elapsed_ms = record.get("elapsed_ms").and_then(Value::as_u64)?;
        let stages = record
            .get("server_timings")
            .map(summarise_timings)
            .unwrap_or_else(|| "no server timings on that call".to_string());
        return Some(format!("{:.1}s ({stages})", elapsed_ms as f64 / 1000.0));
    }
    None
}

/// Read at most [`TAIL_READ_BYTES`] from the END of the file, dropping any leading partial line.
fn read_tail(target: &Path) -> Option<String> {
    let bytes = fs::read(target).ok()?;
    let start = bytes
        .len()
        .saturating_sub(usize::try_from(TAIL_READ_BYTES).ok()?);
    let window = &bytes[start..];
    let text = String::from_utf8_lossy(window).into_owned();
    if start == 0 {
        return Some(text);
    }
    // The window almost certainly begins mid-record; that fragment is not a record.
    text.find('\n').map(|cut| text[cut + 1..].to_string())
}

/// The single slowest LEAF stage in a server `timings` object, plus its cost.
///
/// 🔴 **THE SHAPE HERE IS THE MEASURED ONE, AND MY FIRST VERSION WAS WRITTEN AGAINST THE
/// ASSUMED ONE.** The brief this was built from described the timings as flat keys — `auth`,
/// `recall.dense`, `recall.rerank_provider`, `elapsed_s`, `unattributed_s`. The real 200 body,
/// captured off production 2026-09-07, nests them:
/// `{"stages": {...}, "stages_sum_s": 14.831, "counts": {...}, "elapsed_s": 6.175,
///   "total_s": 6.175, "attributed_s": 5.915, "unattributed_s": 0.26}`.
/// A generic "largest number wins" walk over that returns
/// **`counts.recall.retrieve_context` = 17095**, i.e. a ROW COUNT reported as seventeen thousand
/// seconds. Validated against the wrong representation is how a summariser ships a confident
/// wrong number, so the parser below reads `stages` BY NAME and the test drives it through the
/// real captured body.
///
/// ⚠️ **ROLL-UPS ARE NOT STAGES.** Inside `stages`, `recall` (4.984) is the SUM of `recall.*`, and
/// `recall.embedder_init` (0.621) is the sum of `recall.embedder_init.*`. Reporting a roll-up as
/// "the slowest stage" points the reader at a subtree instead of a line, so any name that is a
/// proper dotted prefix of another name is excluded — leaving `recall.retrieve_context` 4.24s,
/// which is the actionable one. Same rule that excludes `unattributed_s`: a total is not a stage.
fn summarise_timings(timings: &Value) -> String {
    let stages = match timings.get("stages") {
        Some(stages) if stages.is_object() => stages,
        // Older/other servers may return the stages flat. Fall back rather than claim nothing,
        // but never scan `counts` and never let an aggregate compete.
        _ => timings,
    };
    match slowest_leaf(stages) {
        Some((name, seconds)) => format!("slowest stage {name} {seconds:.2}s"),
        None => "no named stages".to_string(),
    }
}

/// Aggregate keys that describe the WHOLE call rather than one stage. Naming any of these as
/// "the slowest stage" would be reporting the total, or our own ignorance, as a finding.
const AGGREGATE_KEYS: &[&str] = &[
    "elapsed_s",
    "total_s",
    "attributed_s",
    "unattributed_s",
    "stages_sum_s",
];

/// The largest numeric leaf that is not an aggregate and not a dotted roll-up of other leaves.
fn slowest_leaf(stages: &Value) -> Option<(String, f64)> {
    let object = stages.as_object()?;
    let names: Vec<&String> = object
        .keys()
        .filter(|key| !AGGREGATE_KEYS.contains(&key.as_str()) && key.as_str() != "counts")
        .collect();
    let mut slowest: Option<(String, f64)> = None;
    for name in &names {
        // A roll-up: some other reported stage lives underneath this one.
        if names
            .iter()
            .any(|other| other.len() > name.len() && other.starts_with(&format!("{name}.")))
        {
            continue;
        }
        let Some(seconds) = object.get(name.as_str()).and_then(Value::as_f64) else {
            continue;
        };
        if slowest.as_ref().is_none_or(|(_, best)| seconds > *best) {
            slowest = Some(((*name).clone(), seconds));
        }
    }
    slowest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("estelle-hook-timings-{name}-{}", unix_millis()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir.join("context-hook.jsonl")
    }

    fn crumb(phase: Phase, elapsed_ms: u64, timings: Option<Value>) -> Breadcrumb {
        Breadcrumb {
            phase,
            elapsed: Duration::from_millis(elapsed_ms),
            budget: Duration::from_secs(20),
            server_timings: timings,
        }
    }

    /// The whole point of the module: an ABANDONED run leaves a record, and it records the
    /// MEASURED elapsed time rather than the budget.
    #[test]
    fn an_abandoned_call_is_recorded_with_its_real_elapsed_time() {
        let path = temp_log("abandon");
        append(&crumb(Phase::AwaitingResponse, 19_400, None), Some(&path));
        let text = fs::read_to_string(&path).expect("log written");
        let record: Value = serde_json::from_str(text.trim()).expect("one json line");
        assert_eq!(record["phase"], "awaiting_response");
        assert_eq!(record["elapsed_ms"], 19_400);
        assert_eq!(record["budget_ms"], 20_000);
        // The limit, asserted rather than merely documented: an abandoned call carries NO server
        // timings, because none arrived.
        assert!(record.get("server_timings").is_none());
    }

    /// The REAL production body, captured 2026-09-07 from `POST /search` by running the built
    /// `estelle hook context` binary against `api.fatelabs.ca` and reading the breadcrumb back.
    ///
    /// 🔬 THE DOUBLE MUST NOT BE FRIENDLIER THAN PRODUCTION. An earlier version of this test used
    /// the flat shape the brief described; the summariser passed it and would have reported
    /// `counts.recall.retrieve_context` — a row count — as "slowest stage 17095.00s" against the
    /// real body. This is the real body, verbatim.
    fn measured_production_timings() -> Value {
        json!({
            "stages": {
                "request_body": 0.0, "auth": 0.435, "commercial": 0.0, "rate_limit": 0.003,
                "concurrency_slot": 0.147, "account_admission": 0.346, "caller_identity": 0.0,
                "repo_scope": 0.0, "team_decisions": 0.0, "recall.asserted": 0.0,
                "recall.embedder_init.namespace": 0.0, "recall.embedder_init.select": 0.621,
                "recall.embedder_init": 0.621, "recall.embed_query": 0.112,
                "recall.dense": 1.055, "recall.sparse": 1.942, "recall.rerank-pool": 0.163,
                "recall.rerank_provider": 0.163, "recall.rerank_gates": 0.0,
                "recall.retrieve_context": 4.239, "recall": 4.984
            },
            "stages_sum_s": 14.831,
            "counts": {
                "recall.asserted": 0, "recall.dense": 20, "recall.sparse": 20,
                "recall.rerank-pool": 35, "recall.retrieve_context": 17095, "recall": 17095
            },
            "elapsed_s": 6.175, "total_s": 6.175, "attributed_s": 5.915, "unattributed_s": 0.26
        })
    }

    /// The read-back path, which is what makes the NEXT abandonment diagnosable — driven through
    /// the real body.
    #[test]
    fn the_last_answered_call_is_read_back_with_its_slowest_leaf_stage() {
        let path = temp_log("readback");
        append(
            &crumb(Phase::Answered, 6_819, Some(measured_production_timings())),
            Some(&path),
        );
        append(&crumb(Phase::AwaitingResponse, 19_900, None), Some(&path));
        let summary = last_answered(Some(&path)).expect("an answered record exists");
        assert!(summary.starts_with("6.8s"), "got {summary}");
        // The actionable leaf, not the `recall` roll-up (4.98) that contains it, not the
        // `stages_sum_s` total (14.83), and above all not the 17095-row COUNT.
        assert!(
            summary.contains("slowest stage recall.retrieve_context 4.24s"),
            "got {summary}"
        );
    }

    /// The three ways a naive "largest number wins" walk gets this wrong, each asserted as
    /// UNREACHABLE rather than merely absent from one happy path.
    #[test]
    fn a_count_a_total_and_a_rollup_can_never_be_reported_as_a_stage() {
        let summary = summarise_timings(&measured_production_timings());
        assert!(
            !summary.contains("17095"),
            "a row count leaked in: {summary}"
        );
        assert!(
            !summary.contains("14.83"),
            "stages_sum_s leaked in: {summary}"
        );
        assert!(
            !summary.contains("stage recall 4.98"),
            "the recall roll-up leaked in: {summary}"
        );
    }

    /// A bounded read means "cannot answer". With no answered record present the function must
    /// return `None` rather than inventing one.
    #[test]
    fn a_log_with_no_answered_record_reads_back_nothing() {
        let path = temp_log("empty");
        append(&crumb(Phase::AwaitingResponse, 19_000, None), Some(&path));
        append(&crumb(Phase::TransportFailed, 600, None), Some(&path));
        assert_eq!(last_answered(Some(&path)), None);
        assert_eq!(last_answered(Some(Path::new("/nonexistent/x.jsonl"))), None);
    }

    /// The bound is real: the live file rotates instead of growing without limit.
    #[test]
    fn the_log_rotates_at_its_cap_instead_of_growing() {
        let path = temp_log("rotate");
        let filler = "x".repeat(1024);
        // Enough records to cross 64 KiB with slack, but a FIXED count — never `while`.
        for _ in 0..96 {
            append(
                &crumb(Phase::Answered, 1_000, Some(json!({"pad": filler}))),
                Some(&path),
            );
        }
        let live = fs::metadata(&path).expect("live file").len();
        assert!(live < MAX_LOG_BYTES, "live file {live} exceeded the cap");
        let rotated = fs::metadata(PathBuf::from(format!("{}.1", path.display())))
            .expect("a rotation exists")
            .len();
        assert!(rotated >= MAX_LOG_BYTES, "rotation {rotated} is undersized");
    }
}
