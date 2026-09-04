//! The bounded, stoppable, non-self-widening loop primitive.
//!
//! A loop is an **unattended actor**: it fires turns at a terminal nobody is watching, and every
//! turn it fires can cost money. So the guardrails are not policy layered on top of a feature,
//! they are the shape of the feature. Four laws, each enforced by CONSTRUCTION rather than by a
//! caller remembering to check:
//!
//! 1. **Bounded.** [`ArmedLoop`] cannot be built without an iteration count AND a wall clock, both
//!    clamped to named constants inside [`ArmedLoop::arm`]. There is no constructor that skips the
//!    clamp, because there is no other constructor.
//! 2. **Non-widening.** [`ArmedLoop::remaining`] and [`ArmedLoop::deadline`] are private and no
//!    method in this file writes them upward — `consume` only subtracts, and the deadline is
//!    written exactly once, in `arm`. The autonomy rank in force when the loop was armed is
//!    captured and [`ArmedLoop::stop_reason`] ENDS the loop if the live rank ever exceeds it.
//! 3. **Fail closed on capability.** A step is refused unless its command is written out in
//!    [`LOOP_ALLOWED_STEPS`]. A command added to the catalog tomorrow is refused by default, and
//!    `every_catalog_command_is_classified` fails until somebody classifies it deliberately.
//! 4. **Spend takes the path spend already takes — no more, and no less.** This module holds no
//!    client, sends nothing, and cannot await. It returns strings for the caller to submit down
//!    the ordinary path, so whatever metering, rate limiting and refusal apply to a turn the user
//!    typed apply here unchanged.
//!
//! 🔴 **AND THAT IS WEAKER THAN IT SOUNDS, WHICH IS WHY [`MAX_LOOP_TURNS`] EXISTS.** Measured on
//! the server tree, 2026-09-04: an ordinary model call is **counted and not capped**.
//! `serve/byok_ask.py:193` charges the ledger and commits; it never consults `decide_budget`, and
//! `charge_within_budget` (`serve/budget.py:52`) has exactly two call sites — `op_meter.py:113`
//! and `ingest_gate.py:281` — neither of them on the model path. A hard `402` comes from
//! `op_meter.enforce_op`, and only for the six metered ops (`rerank, repair, improve, sweep,
//! research, monitor`). So *"it inherits the account's spend cap"* would have been a comfortable
//! sentence and a false one: for a plain question there is no dollar ceiling to inherit. The only
//! hard money-shaped bound on a CLI loop today is the turn cap in this file.
//!
//! ⚠️ **What this file does NOT prove.** It proves the primitive is bounded. It cannot prove the
//! wiring calls it — that is `main.rs`'s job and the tests that press keys. The spend paragraph is
//! a reading of the server's code, not a measurement of a bill. And a bound that lives in the
//! CLIENT is advisory to anyone who writes their own client: when this ports to MCP the server has
//! to own these numbers, or there are two owners of one derived fact and they will disagree.

use std::time::Duration;
use std::time::Instant;

/// The most times any loop may fire, however it was armed and whatever it asked for.
pub(crate) const MAX_LOOP_ITERATIONS: u32 = 12;

/// The longest any loop may stay armed. A loop that outlives the session it was armed in is a
/// process nobody remembers starting.
pub(crate) const MAX_LOOP_WALL_CLOCK: Duration = Duration::from_secs(4 * 60 * 60);

/// The floor under a fixed cadence. Below this a "loop" is a denial-of-service against your own
/// account, and the user almost certainly meant minutes.
///
/// ⚠️ **ONE MINUTE IS THE REFERENCE IMPLEMENTATION'S FLOOR, NOT A NUMBER I PICKED.** Claude Code's
/// own `/loop` clamps `delaySeconds` to `[60, 3600]` and states *"cron minimum granularity is 1
/// minute"*, rounding `Ns` up to `ceil(N/60)m`. Matching it means a user who knows one surface is
/// not surprised by the other, and it is the stricter of the two floors I considered.
pub(crate) const MIN_LOOP_INTERVAL: Duration = Duration::from_secs(60);

/// How long a self-paced loop waits after an iteration LANDS before re-arming.
///
/// Self-paced means "when the last one finished", not "immediately": the settle keeps a loop whose
/// steps all fail fast from spinning through its whole budget in one second, and it gives a human
/// a window in which `esc` lands between iterations rather than during one.
pub(crate) const SELF_PACED_SETTLE: Duration = Duration::from_secs(5);

/// The most steps one iteration may carry.
pub(crate) const MAX_LOOP_STEPS: usize = 4;

/// 🔴 **THE BOUND THAT IS ACTUALLY ABOUT MONEY.**
///
/// Iterations and steps multiply, and the billable unit is the PRODUCT, not either factor: 12
/// iterations of 4 steps is 48 unattended server turns, which is not what someone typing a
/// two-token command has in mind. So the product is capped too, and [`ArmedLoop::arm`] lowers the
/// iteration count until it fits rather than accepting a number it will not honour.
pub(crate) const MAX_LOOP_TURNS: u32 = 24;

/// Consecutive failed iterations after which the loop disarms itself.
///
/// A loop hammering a `402` all night is the worst version of this feature. Three is enough to
/// ride out one flaky answer and few enough to stop before a bill.
pub(crate) const MAX_LOOP_CONSECUTIVE_FAILURES: u32 = 3;

/// The token that separates steps in a mixed submission: `/gate && /verify serve/api.py`.
///
/// ⚠️ **SPACES ARE PART OF THE SEPARATOR AND THAT IS DELIBERATE.** Splitting on a bare `&&` would
/// cut prose (`a&&b`) and, worse, would cut a `!` shell line in half at the exact place a shell
/// author meant "and then". [`split_steps`] therefore splits on ` && ` only, and its caller never
/// hands it a shell line at all.
const STEP_SEPARATOR: &str = " && ";

/// The most steps one ordinary (non-loop) mixed submission may carry.
///
/// Lower than the composer queue's own cap so a chain can never be the thing that fills the queue.
pub(crate) const MAX_CHAIN_STEPS: usize = 8;

/// Every command a loop is allowed to run, written out.
///
/// 🔴 **THIS IS AN ALLOWLIST BECAUSE A DENYLIST GRANTS EVERY COMMAND WE HAVE NOT WRITTEN YET.**
/// The catalog grows; a denylist would silently hand each new command to an unattended actor the
/// day it lands. Here the default is refusal, and
/// `every_catalog_command_is_classified` goes red until a human puts a new name on one side or
/// the other.
///
/// The membership rule, stated so it can be argued with: a step may READ anything, may run the
/// grounded verification surfaces, and may run the agentic payload (`work`, `orchestra`, `sweep`)
/// which is propose-only and server-gated. A step may NOT change credentials, the autonomy dial,
/// the routing table, the working tree, the session, or the client's own panels — those are the
/// authority-widening and state-mutating shapes, and an unattended actor gets none of them.
pub(crate) const LOOP_ALLOWED_STEPS: &[&str] = &[
    "activity",
    "analytics",
    "audit",
    "automations",
    "cards",
    "diff",
    "entities",
    "gate",
    "graph",
    "grep",
    "hardware",
    "improve",
    "init",
    "leaderboard",
    "marketplace",
    "me",
    "memory",
    "orchestra",
    "outcomes",
    "presence",
    "requests",
    "review",
    "routing",
    "runs",
    "scan",
    "sessions",
    "skills",
    "status",
    "suites",
    "sweep",
    "task",
    "team",
    "tools",
    "usage",
    "verify",
    "work",
];

/// Every command deliberately withheld from a loop, written out, with the reason it is withheld.
///
/// This exists so [`LOOP_ALLOWED_STEPS`] can be checked for COMPLETENESS against the catalog. A
/// name in neither list is a name nobody classified, which is the state this pair makes visible.
pub(crate) const LOOP_REFUSED_STEPS: &[(&str, &str)] = &[
    ("apply", "writes the working tree"),
    ("billing", "changes what the account is charged"),
    (
        "clear",
        "destroys the record the user would read afterwards",
    ),
    ("compact", "rewrites the session's own history"),
    ("context", "a panel toggle, and a loop has no eyes"),
    ("exit", "ends the session the loop is visible in"),
    (
        "help",
        "a loop that prints help is a loop nobody armed on purpose",
    ),
    ("keymap", "a panel toggle, and a loop has no keyboard"),
    ("keys", "credential surface"),
    ("login", "credential mutation"),
    ("logout", "credential mutation"),
    (
        "marks",
        "not a command; reserved so the classifier cannot be fooled by a near-miss",
    ),
    ("mcp", "duplicate of /tools; one owner per derived fact"),
    (
        "memories",
        "duplicate of /memory; one owner per derived fact",
    ),
    (
        "mode",
        "RAISES OR LOWERS THE AUTONOMY CEILING — the widening move itself",
    ),
    ("model", "changes which engine spends the budget"),
    (
        "permissions",
        "the autonomy boundary is read by a human, not polled by a robot",
    ),
    ("plan", "enters a ceiling mode, i.e. changes authority"),
    ("presets", "sets the server-owned routing table"),
    ("prod", "a panel toggle, and a loop has no eyes"),
    ("resume", "switches which session the user is in"),
    (
        "settings",
        "opens an interactive surface a loop cannot answer",
    ),
    ("shell", "unattended arbitrary code execution"),
    ("todo", "a panel toggle, and a loop has no eyes"),
    ("undo", "writes the working tree"),
    (
        "version",
        "harmless and pointless; withheld to keep the list honest",
    ),
    ("whoami", "credential surface"),
    ("doctor", "an interactive diagnosis for a human reading it"),
    ("loop", "🔴 A LOOP MAY NOT ARM A LOOP — see `may_arm`"),
    (
        "skill:",
        "a playbook is an open-ended agent turn with no step-level ceiling",
    ),
    ("skills:", "alias of skill:"),
];

/// How a loop decides when to fire again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cadence {
    /// Fire every `Duration`, measured from the previous FIRING.
    Fixed(Duration),
    /// Fire [`SELF_PACED_SETTLE`] after the previous iteration LANDED.
    ///
    /// This is the mode with no clock in it, and it is the safer of the two: a self-paced loop
    /// cannot overlap itself or outrun a slow server, because the next firing does not exist until
    /// the last one is done.
    SelfPaced,
}

impl Cadence {
    pub(crate) fn label(self) -> String {
        match self {
            Self::Fixed(interval) => format!("every {}", human_duration(interval)),
            Self::SelfPaced => "self-paced".to_string(),
        }
    }
}

/// Who asked for this loop.
///
/// The distinction is load-bearing, not cosmetic: [`ArmOrigin::Agent`] means the request was read
/// out of MODEL OUTPUT, and model output is downstream of whatever content the model was grounded
/// in. Treating an agent-armed loop exactly like a user-typed one would make a poisoned file in a
/// swept repo able to arm an unattended actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArmOrigin {
    User,
    Agent,
}

/// Why an arming request was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ArmRefusal {
    /// A loop may not arm a loop. The single rule that makes the whole thing finite.
    NoNesting,
    AlreadyArmed,
    /// The model asked to arm a loop and the session has not opted in.
    AgentNotOptedIn,
    Empty,
    TooManySteps(usize),
    IntervalTooShort(Duration),
    IntervalOutlivesCeiling,
    /// A step whose command is not on [`LOOP_ALLOWED_STEPS`].
    StepNotAllowed(String),
    /// A shell step. Named separately from `StepNotAllowed` because the answer is different: there
    /// is no spelling of a shell command that a loop will run.
    ShellStep,
}

impl ArmRefusal {
    pub(crate) fn line(&self) -> String {
        match self {
            Self::NoNesting => "A loop may not arm a loop. Nothing was armed.".to_string(),
            Self::AlreadyArmed => {
                "A loop is already armed. Use /loop stop first, or esc.".to_string()
            }
            Self::AgentNotOptedIn => format!(
                "Estelle asked to arm a loop and this session has not opted in. \
                 Run /loop auto on to allow it for this session only (it is never persisted), \
                 or arm it yourself. Bounds are the same either way: \
                 {MAX_LOOP_ITERATIONS} iterations, {MAX_LOOP_TURNS} turns, \
                 {} wall clock.",
                human_duration(MAX_LOOP_WALL_CLOCK)
            ),
            Self::Empty => "/loop needs something to run, for example /loop 10m /gate.".to_string(),
            Self::TooManySteps(count) => {
                format!("A loop carries at most {MAX_LOOP_STEPS} steps and that one has {count}.")
            }
            Self::IntervalTooShort(interval) => format!(
                "{} is below the {} floor for a loop interval.",
                human_duration(*interval),
                human_duration(MIN_LOOP_INTERVAL)
            ),
            Self::IntervalOutlivesCeiling => format!(
                "That interval is longer than the {} ceiling, so the loop could never fire.",
                human_duration(MAX_LOOP_WALL_CLOCK)
            ),
            // 🔴 THE REASON COMES OUT OF THE CLASSIFICATION TABLE, NOT OUT OF A SECOND STRING.
            // One owner: the line a human reads and the line a reviewer reads while deciding
            // whether the classification is right are the same bytes, so they cannot drift.
            Self::StepNotAllowed(step) => {
                let why = LOOP_REFUSED_STEPS
                    .iter()
                    .find_map(|(name, reason)| (*name == step).then_some(*reason))
                    .unwrap_or("it is not on the loop allowlist, and the allowlist fails closed");
                format!(
                    "/{step} is not a step a loop may run: {why}. \
                     A loop reads, verifies and proposes; it does not change credentials, \
                     the autonomy dial, the routing table, the working tree or the session. \
                     Run /loop allowed for the list."
                )
            }
            Self::ShellStep => {
                "A loop will not run a shell command. Unattended arbitrary code execution \
                 has no ceiling to put on it."
                    .to_string()
            }
        }
    }
}

/// Why a running loop ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StopReason {
    IterationsSpent,
    DeadlineReached,
    /// The live autonomy rank rose above the rank in force when the loop was armed.
    AutonomyRaised,
    ConsecutiveFailures,
    /// A human said stop.
    Stopped,
}

impl StopReason {
    pub(crate) fn line(self, fired: u32) -> String {
        let ran = format!("Loop stopped after {fired} iteration{}", plural(fired));
        match self {
            Self::IterationsSpent => format!("{ran}: its iteration budget is spent."),
            Self::DeadlineReached => format!("{ran}: its wall clock ran out."),
            Self::AutonomyRaised => format!(
                "{ran}: the autonomy dial was raised while it was armed, and a loop never \
                 inherits authority it was not armed with."
            ),
            Self::ConsecutiveFailures => {
                format!("{ran}: {MAX_LOOP_CONSECUTIVE_FAILURES} iterations failed in a row.")
            }
            Self::Stopped => format!("{ran}: stopped."),
        }
    }
}

/// A validated, bounded, armed loop.
///
/// 🔴 **EVERY FIELD THAT COULD WIDEN THIS IS PRIVATE AND WRITTEN ONCE.** `deadline` is assigned in
/// [`Self::arm`] and appears on no left-hand side anywhere else in this file. `remaining` appears
/// on exactly one left-hand side, in [`Self::begin_iteration`], where it is `saturating_sub(1)`.
/// That is the whole non-widening argument, and it is short on purpose so a reader can check it.
#[derive(Clone, Debug)]
pub(crate) struct ArmedLoop {
    steps: Vec<String>,
    cadence: Cadence,
    origin: ArmOrigin,
    armed_at: Instant,
    deadline: Instant,
    granted: u32,
    remaining: u32,
    fired: u32,
    next_fire: Instant,
    consecutive_failures: u32,
    /// The autonomy rank in force at arm time, or `None` when the client did not know one.
    autonomy_rank: Option<i64>,
    /// Session spend at arm time, so the band can report what THIS loop has cost.
    spend_at_arm: Option<f64>,
}

// 🔴 **THERE IS NO `stopped` FLAG HERE, AND ITS ABSENCE IS THE DESIGN.**
//
// The first version carried `stopped: Option<StopReason>` plus a `stop()` that set it — and
// `cargo clippy` reported `stop` was never called, because `App::stop_loop` disarms by taking the
// loop out of its `Option` entirely. That is two owners of one derived fact ("is this loop
// stopped"), and the weaker of the two was the one nothing used. One owner now: a loop that is
// stopped is a loop that no longer exists. `StopReason::Stopped` survives as the WORDING for the
// transcript line, which is a different job.

/// The decision law: may a loop be armed at all, right now, by this asker?
///
/// 🔴 **WRITTEN AS ONE BOOLEAN SO IT CAN BE READ RATHER THAN REMEMBERED.**
///
/// ```text
/// allowed = !inside_iteration && !already_armed && (origin == User || agent_opt_in)
/// ```
///
/// `!inside_iteration` is the clause that makes a loop finite: it is checked FIRST and it refuses
/// whoever is asking, so there is no combination of the other three that lets an iteration arm
/// anything. `every_arming_combination_obeys_the_law` walks all 16 input combinations.
pub(crate) fn may_arm(
    already_armed: bool,
    inside_iteration: bool,
    origin: ArmOrigin,
    agent_opt_in: bool,
) -> Option<ArmRefusal> {
    if inside_iteration {
        return Some(ArmRefusal::NoNesting);
    }
    if already_armed {
        return Some(ArmRefusal::AlreadyArmed);
    }
    if origin == ArmOrigin::Agent && !agent_opt_in {
        return Some(ArmRefusal::AgentNotOptedIn);
    }
    None
}

/// A parsed but not-yet-armed request: `[interval] step [&& step…]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoopDraft {
    pub(crate) cadence: Cadence,
    pub(crate) steps: Vec<String>,
}

/// Split a submission into ordered steps on [`STEP_SEPARATOR`].
///
/// This is the whole of "mix commands": one submission, several steps, run in order down the
/// ordinary queue. Empty fragments are dropped so a trailing `&&` is a typo rather than an error,
/// and the result is capped by the caller's own bound.
pub(crate) fn split_steps(raw: &str, max: usize) -> Vec<String> {
    raw.split(STEP_SEPARATOR)
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .take(max)
        .map(str::to_string)
        .collect()
}

/// True when `raw` actually asks for more than one step.
///
/// ⚠️ **TWO SUBMISSIONS OWN THEIR OWN `&&` AND MUST NOT BE CUT HERE.**
///
/// * A `!` shell line: `!git add -A && git commit` is one line whose `&&` belongs to the shell.
///   Cutting it would run `git add -A` and then a *different*, separate `git commit`.
/// * 🔴 A `/loop` submission: `/loop 10m /gate && /scan` is ONE arming request carrying a
///   two-step payload, and [`parse_draft`] is the thing that splits it. Without this clause the
///   mixer cut it first and armed `/loop 10m /gate` while sending `/scan` as an unrelated turn —
///   a loop with half the payload the user asked for, silently, and it RAN. Found by writing the
///   two features in the same session; the separator has one owner per submission and the
///   outermost command is that owner.
pub(crate) fn is_chain(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.starts_with('!') || owns_its_own_separator(trimmed) {
        return false;
    }
    split_steps(trimmed, MAX_CHAIN_STEPS + 1).len() > 1
}

/// Commands whose ARGUMENT is itself a step list, so the outer mixer must leave them alone.
const SEPARATOR_OWNERS: &[&str] = &["/loop"];

fn owns_its_own_separator(trimmed: &str) -> bool {
    let head = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    SEPARATOR_OWNERS.contains(&head.as_str())
}

/// Parse `10m /gate && /verify` into a cadence and steps, or say why not.
///
/// The leading token is an interval when — and only when — it is digits followed by `s`, `m` or
/// `h`. Anything else is the first step, which is how `/loop /gate` reaches self-paced mode
/// without a keyword.
pub(crate) fn parse_draft(argument: &str) -> Result<LoopDraft, ArmRefusal> {
    let argument = argument.trim();
    if argument.is_empty() {
        return Err(ArmRefusal::Empty);
    }
    // Rule 1: a leading `5m`. Rule 2: a trailing `every 20m`. Rule 3: neither, so self-paced.
    let (cadence, rest) = match argument.split_once(char::is_whitespace) {
        Some((head, tail)) => match parse_interval(&head.to_ascii_lowercase()) {
            Some(interval) => (Cadence::Fixed(interval), tail.trim().to_string()),
            None => match take_trailing_every(argument) {
                Some((interval, prompt)) => (Cadence::Fixed(interval), prompt),
                None => (Cadence::SelfPaced, argument.to_string()),
            },
        },
        None => match parse_interval(&argument.to_ascii_lowercase()) {
            // `/loop 10m` with nothing to run is a cadence and no payload.
            Some(_) => return Err(ArmRefusal::Empty),
            None => (Cadence::SelfPaced, argument.to_string()),
        },
    };
    if let Cadence::Fixed(interval) = cadence {
        if interval < MIN_LOOP_INTERVAL {
            return Err(ArmRefusal::IntervalTooShort(interval));
        }
        if interval > MAX_LOOP_WALL_CLOCK {
            return Err(ArmRefusal::IntervalOutlivesCeiling);
        }
    }
    let steps = split_steps(&rest, MAX_LOOP_STEPS + 1);
    if steps.is_empty() {
        return Err(ArmRefusal::Empty);
    }
    if steps.len() > MAX_LOOP_STEPS {
        return Err(ArmRefusal::TooManySteps(steps.len()));
    }
    for step in &steps {
        check_step(step)?;
    }
    Ok(LoopDraft { cadence, steps })
}

/// `10m` / `90s` / `2h` / `1d` → a duration; anything else → `None`.
///
/// 🔴 **`d` IS PARSED PRECISELY SO THAT `/loop 1d /gate` IS REFUSED RATHER THAN MISREAD.** The
/// reference surface accepts `Ns|Nm|Nh|Nd`, so a user will type `1d`. If this returned `None` for
/// it, `1d` would fall through as the FIRST STEP and `/gate` as the second — a day-long cadence
/// silently becoming a two-step self-paced loop, which is the worst kind of wrong: it runs. Parsed
/// here, it meets [`ArmRefusal::IntervalOutlivesCeiling`] and says so.
fn parse_interval(token: &str) -> Option<Duration> {
    let (digits, unit) = token.split_at(token.len().checked_sub(1)?);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let count: u64 = digits.parse().ok()?;
    let seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return None,
    };
    Some(Duration::from_secs(count.checked_mul(seconds)?))
}

/// The unit words a trailing `every …` clause may end with, mapped to the short spelling.
const EVERY_UNITS: &[(&str, char)] = &[
    ("seconds", 's'),
    ("second", 's'),
    ("secs", 's'),
    ("sec", 's'),
    ("s", 's'),
    ("minutes", 'm'),
    ("minute", 'm'),
    ("mins", 'm'),
    ("min", 'm'),
    ("m", 'm'),
    ("hours", 'h'),
    ("hour", 'h'),
    ("hrs", 'h'),
    ("hr", 'h'),
    ("h", 'h'),
    ("days", 'd'),
    ("day", 'd'),
    ("d", 'd'),
];

/// Strip a trailing `every 20m` / `every 5 minutes` and return the interval with the prompt.
///
/// 🔴 **THE CLAUSE ONLY COUNTS WHEN WHAT FOLLOWS `every` IS A TIME.** `check every PR` ends in
/// `every PR`, and reading that as a cadence would silently drop the word `PR` from the user's
/// task. The reference implementation names this exact case; the guard is that the token after
/// `every` must parse as a number and the one after it as a unit word, with NOTHING following.
fn take_trailing_every(input: &str) -> Option<(Duration, String)> {
    let words: Vec<&str> = input.split_whitespace().collect();
    // `every 20m` (2 words) or `every 5 minutes` (3 words).
    for span in [2usize, 3] {
        let start = words.len().checked_sub(span)?;
        if !words[start].eq_ignore_ascii_case("every") {
            continue;
        }
        let interval = if span == 2 {
            parse_interval(&words[start + 1].to_ascii_lowercase())?
        } else {
            let count = words[start + 1];
            if count.is_empty() || !count.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let word = words[start + 2].to_ascii_lowercase();
            let unit = EVERY_UNITS
                .iter()
                .find_map(|(spelling, short)| (*spelling == word).then_some(*short))?;
            parse_interval(&format!("{count}{unit}"))?
        };
        let prompt = words[..start].join(" ");
        if prompt.trim().is_empty() {
            return None;
        }
        return Some((interval, prompt));
    }
    None
}

/// Is this one step something a loop may run?
///
/// A step that does not start with `/` is a plain question — a model call with no authority, which
/// is the most ordinary thing a loop does ("check the deploy every 5 minutes"). A step that starts
/// with `!` is a shell line and is refused outright. A step that starts with `/` must name a
/// command written out in [`LOOP_ALLOWED_STEPS`].
fn check_step(step: &str) -> Result<(), ArmRefusal> {
    let step = step.trim();
    if step.starts_with('!') {
        return Err(ArmRefusal::ShellStep);
    }
    let Some(command) = step.strip_prefix('/') else {
        return Ok(());
    };
    let name = command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // A namespaced invocation (`/skill:x`) keeps its namespace, so the classifier sees the same
    // token the refusal list writes out rather than a bare `skill`.
    let head = match name.split_once(':') {
        Some((namespace, _)) => format!("{namespace}:"),
        None => name,
    };
    if LOOP_ALLOWED_STEPS.contains(&head.as_str()) {
        return Ok(());
    }
    Err(ArmRefusal::StepNotAllowed(head))
}

impl ArmedLoop {
    /// Build an armed loop, clamping every bound to its ceiling.
    ///
    /// 🔴 **THE CLAMP IS HERE, AT CREATION, AND THERE IS NO OTHER WAY IN.** A ceiling enforced
    /// where the loop RUNS is a ceiling that a future caller can construct its way around. `arm`
    /// is the only constructor of [`ArmedLoop`], the struct's fields are private, and every bound
    /// is `min`-ed against a named constant before the value is stored.
    ///
    /// `requested` of `None` means "as many as you will give me", which is the ceiling.
    pub(crate) fn arm(
        draft: LoopDraft,
        origin: ArmOrigin,
        now: Instant,
        autonomy_rank: Option<i64>,
        spend_at_arm: Option<f64>,
    ) -> Self {
        let steps = draft.steps;
        // Bound the RESOURCE — turns are what costs money — before taking it.
        let by_turns = MAX_LOOP_TURNS / u32::try_from(steps.len().max(1)).unwrap_or(u32::MAX);
        let granted = MAX_LOOP_ITERATIONS.min(by_turns).max(1);
        let deadline = now
            .checked_add(MAX_LOOP_WALL_CLOCK)
            .unwrap_or_else(|| now + Duration::from_secs(60));
        // Both cadences fire their FIRST iteration immediately. `/loop 10m /gate` that sits doing
        // nothing for ten minutes reads exactly like a loop that failed to arm, which is the
        // complaint this whole feature exists to answer.
        let first = now;
        Self {
            steps,
            cadence: draft.cadence,
            origin,
            armed_at: now,
            deadline,
            granted,
            remaining: granted,
            fired: 0,
            next_fire: first,
            consecutive_failures: 0,
            autonomy_rank,
            spend_at_arm,
        }
    }

    pub(crate) fn fired(&self) -> u32 {
        self.fired
    }

    /// Why this loop should end, or `None` to keep going.
    ///
    /// Checked BEFORE every firing and after every landing, so a loop cannot fire once past its
    /// own bound. `live_autonomy_rank` is passed in rather than stored so the comparison is always
    /// against the CURRENT dial, never a copy that went stale.
    pub(crate) fn stop_reason(
        &self,
        now: Instant,
        live_autonomy_rank: Option<i64>,
    ) -> Option<StopReason> {
        if self.remaining == 0 {
            return Some(StopReason::IterationsSpent);
        }
        if now >= self.deadline {
            return Some(StopReason::DeadlineReached);
        }
        if self.consecutive_failures >= MAX_LOOP_CONSECUTIVE_FAILURES {
            return Some(StopReason::ConsecutiveFailures);
        }
        // 🔴 THE NON-WIDENING CHECK. `None` on either side is not evidence of a raise, so it is
        // not treated as one — an unknown dial stops nothing, and says nothing.
        if let (Some(armed), Some(live)) = (self.autonomy_rank, live_autonomy_rank)
            && live > armed
        {
            return Some(StopReason::AutonomyRaised);
        }
        None
    }

    /// Is it time to fire?
    pub(crate) fn due(&self, now: Instant, live_autonomy_rank: Option<i64>) -> bool {
        self.stop_reason(now, live_autonomy_rank).is_none() && now >= self.next_fire
    }

    /// Take one iteration's steps, spending one of the budget.
    ///
    /// ⚠️ The caller MUST submit what this returns or drop the loop — the budget is spent here, at
    /// the decision, not at the reply. Spending on the reply would let an iteration that never
    /// landed be retried forever, which is an unbounded loop wearing a bounded loop's clothes.
    pub(crate) fn begin_iteration(&mut self, now: Instant) -> Vec<String> {
        self.remaining = self.remaining.saturating_sub(1);
        self.fired = self.fired.saturating_add(1);
        if let Cadence::Fixed(interval) = self.cadence {
            self.next_fire = now.checked_add(interval).unwrap_or(self.deadline);
        } else {
            // Self-paced: no next firing exists until `settle` says the last one landed.
            self.next_fire = self.deadline;
        }
        self.steps.clone()
    }

    /// Record that the iteration landed, and re-arm a self-paced loop.
    pub(crate) fn settle(&mut self, now: Instant, ok: bool) {
        if ok {
            self.consecutive_failures = 0;
        } else {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        }
        if self.cadence == Cadence::SelfPaced {
            self.next_fire = now.checked_add(SELF_PACED_SETTLE).unwrap_or(self.deadline);
        }
    }

    /// The one-line band the status bar draws while this loop is armed.
    ///
    /// 🔴 **DISCOVERABILITY IS THE REQUIREMENT, NOT THE POLISH.** The complaint that produced this
    /// feature was *"I don't see you doing your loop"*, so an armed loop that is merely WAITING
    /// still draws — the idle state is precisely the state that used to be invisible. It names the
    /// count, the budget, the time left, the money this loop has spent, and the key that stops it.
    pub(crate) fn band(&self, now: Instant, live_spend: Option<f64>) -> String {
        let left = self.deadline.saturating_duration_since(now);
        let mut band = format!(
            "loop {}/{} \u{b7} {} \u{b7} {} left",
            self.fired,
            self.granted,
            self.cadence.label(),
            human_duration(left)
        );
        if self.origin == ArmOrigin::Agent {
            band.push_str(" \u{b7} armed by Estelle");
        }
        if let (Some(start), Some(live)) = (self.spend_at_arm, live_spend) {
            let spent = (live - start).max(0.0);
            band.push_str(&format!(" \u{b7} ${spent:.3} this loop"));
        }
        band.push_str(" \u{b7} esc stops");
        band
    }

    /// The multi-line answer `/loop` prints when asked for status.
    pub(crate) fn status_lines(&self, now: Instant, live_spend: Option<f64>) -> Vec<String> {
        let mut lines = vec![
            self.band(now, live_spend),
            format!(
                "armed {} ago by {}",
                human_duration(now.saturating_duration_since(self.armed_at)),
                match self.origin {
                    ArmOrigin::User => "you",
                    ArmOrigin::Agent => "Estelle",
                }
            ),
        ];
        for (index, step) in self.steps.iter().enumerate() {
            lines.push(format!("  step {} \u{b7} {step}", index + 1));
        }
        lines.push(format!(
            "{} of {granted} iterations left \u{b7} {} consecutive failures of {MAX_LOOP_CONSECUTIVE_FAILURES} allowed",
            self.remaining,
            self.consecutive_failures,
            granted = self.granted,
        ));
        lines.push("/loop stop or esc ends it now.".to_string());
        lines
    }

    /// The band announcing one firing, pushed to the transcript so the record shows every turn a
    /// loop caused rather than a stream of turns nobody typed.
    pub(crate) fn firing_line(&self, step_count: usize) -> String {
        format!(
            "loop {}/{} fires {step_count} step{} \u{b7} {} \u{b7} esc stops",
            self.fired,
            self.granted,
            plural(u32::try_from(step_count).unwrap_or(u32::MAX)),
            self.cadence.label(),
        )
    }
}

/// The directive an assistant answer uses to ask for a loop.
///
/// 🔴 **THIS IS MODEL OUTPUT, WHICH MEANS IT IS DOWNSTREAM OF INGESTED CONTENT.** A poisoned file
/// in a swept repo can influence what the model writes, so this parser's job is not to be
/// permissive — it is to be a narrow, single-shot, length-capped reader whose successful parse
/// still lands on [`may_arm`]'s `agent_opt_in` clause and on every bound in [`ArmedLoop::arm`].
/// The worst outcome of a successful injection is therefore a VISIBLE, bounded, allowlisted,
/// esc-stoppable loop that the session had already opted into — not arbitrary unattended work.
const DIRECTIVE_OPEN: &str = "<estelle:loop>";
const DIRECTIVE_CLOSE: &str = "</estelle:loop>";

/// The longest directive body that will be read. Longer than any legitimate step list.
const MAX_DIRECTIVE_LEN: usize = 400;

/// The FIRST loop directive in an answer, and the answer with every directive removed.
///
/// ⚠️ Only the first is honoured. An answer carrying six directives is either confused or hostile,
/// and in both cases arming six loops is the wrong reading — the rest are stripped and dropped.
pub(crate) fn take_loop_directive(answer: &str) -> (Option<String>, String) {
    let mut found: Option<String> = None;
    let mut visible = String::with_capacity(answer.len());
    let mut rest = answer;
    while let Some(open) = rest.find(DIRECTIVE_OPEN) {
        let after_open = open + DIRECTIVE_OPEN.len();
        let Some(close_offset) = rest[after_open..].find(DIRECTIVE_CLOSE) else {
            break;
        };
        let body = rest[after_open..after_open + close_offset].trim();
        visible.push_str(&rest[..open]);
        if found.is_none() && !body.is_empty() && body.len() <= MAX_DIRECTIVE_LEN {
            found = Some(body.to_string());
        }
        rest = &rest[after_open + close_offset + DIRECTIVE_CLOSE.len()..];
    }
    visible.push_str(rest);
    (found, visible.trim().to_string())
}

fn plural(count: u32) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// `4h` / `10m` / `45s`, choosing the largest unit that does not round to nothing.
pub(crate) fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h{minutes}m")
        }
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands;

    fn draft(argument: &str) -> LoopDraft {
        parse_draft(argument).expect("a valid draft")
    }

    /// 🔴 **THE ARMING LAW, WALKED EXHAUSTIVELY — ALL 16 COMBINATIONS.**
    ///
    /// The shape is deliberate: a law you verify by ENUMERATION cannot hide a combination nobody
    /// thought of, which is exactly how a guard that "looks right" ships with one hole in it. The
    /// expectation is recomputed from the written law rather than from the implementation, so an
    /// implementation that drifts fails here instead of redefining what it was supposed to do.
    #[test]
    fn every_arming_combination_obeys_the_law() {
        let mut checked = 0;
        for already_armed in [false, true] {
            for inside_iteration in [false, true] {
                for origin in [ArmOrigin::User, ArmOrigin::Agent] {
                    for agent_opt_in in [false, true] {
                        let allowed = !inside_iteration
                            && !already_armed
                            && (origin == ArmOrigin::User || agent_opt_in);
                        let decision =
                            may_arm(already_armed, inside_iteration, origin, agent_opt_in);
                        assert_eq!(
                            decision.is_none(),
                            allowed,
                            "armed={already_armed} inside={inside_iteration} \
                             origin={origin:?} opt_in={agent_opt_in} decided {decision:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 16, "the law was not walked exhaustively");
    }

    /// 🔴 **NESTING IS REFUSED FOR EVERY ASKER, WHICH IS WHAT MAKES THE FEATURE FINITE.**
    ///
    /// A separate test from the exhaustive walk because it asserts the PRECEDENCE, not the truth
    /// table: `inside_iteration` must beat every other clause, including the combination where
    /// everything else says yes. Without that precedence, a loop could arm a loop by being the
    /// only thing armed, and depth would be unbounded.
    #[test]
    fn no_asker_can_arm_a_loop_from_inside_an_iteration() {
        for origin in [ArmOrigin::User, ArmOrigin::Agent] {
            for already_armed in [false, true] {
                for agent_opt_in in [false, true] {
                    assert_eq!(
                        may_arm(already_armed, true, origin, agent_opt_in),
                        Some(ArmRefusal::NoNesting),
                        "origin={origin:?} armed={already_armed} opt_in={agent_opt_in}"
                    );
                }
            }
        }
    }

    /// 🔴 **THE BUDGET NEVER GOES UP — ASSERTED AFTER EVERY OPERATION, NOT JUST AT THE END.**
    ///
    /// The non-widening claim is about a MONOTONE, so a test that checks only the final state
    /// would pass on an implementation that doubled the budget and then spent it. This drives a
    /// loop through its whole life and asserts, at each step, that neither the remaining count nor
    /// the deadline moved outward.
    #[test]
    fn a_loop_can_never_widen_its_own_budget() {
        let now = Instant::now();
        let mut looping = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, Some(1), None);
        let deadline = looping.deadline;
        let mut last_remaining = looping.remaining;
        for tick in 0..40u64 {
            let now = now + Duration::from_secs(tick * 60);
            if looping.due(now, Some(1)) {
                looping.begin_iteration(now);
                looping.settle(now, true);
            }
            assert!(
                looping.remaining <= last_remaining,
                "remaining rose from {last_remaining} to {}",
                looping.remaining
            );
            assert_eq!(looping.deadline, deadline, "the deadline moved");
            last_remaining = looping.remaining;
        }
        assert_eq!(looping.remaining, 0);
        assert_eq!(
            looping.stop_reason(now, Some(1)),
            Some(StopReason::IterationsSpent)
        );
    }

    /// A loop stops the moment the autonomy dial rises above what it was armed with.
    #[test]
    fn raising_the_autonomy_dial_ends_a_running_loop() {
        let now = Instant::now();
        let looping = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, Some(1), None);
        assert_eq!(looping.stop_reason(now, Some(1)), None, "same rank runs");
        assert_eq!(looping.stop_reason(now, Some(0)), None, "a LOWER rank runs");
        assert_eq!(
            looping.stop_reason(now, Some(2)),
            Some(StopReason::AutonomyRaised)
        );
        // ⚠️ THE CONTROL. An unknown dial is not evidence of a raise and must not stop the loop,
        // or every client that has not yet learned its rank would be unable to loop at all.
        assert_eq!(looping.stop_reason(now, None), None);
        let unknown = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, None, None);
        assert_eq!(unknown.stop_reason(now, Some(9)), None);
    }

    /// The wall clock ends a loop that still has iterations left.
    #[test]
    fn the_wall_clock_ends_a_loop_with_budget_to_spare() {
        let now = Instant::now();
        let looping = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, Some(1), None);
        assert!(looping.remaining > 0);
        assert_eq!(
            looping.stop_reason(now + MAX_LOOP_WALL_CLOCK, Some(1)),
            Some(StopReason::DeadlineReached)
        );
        assert!(!looping.due(now + MAX_LOOP_WALL_CLOCK, Some(1)));
    }

    /// 🔴 **THE TURN CAP IS THE MONEY CAP, AND IT LOWERS THE ITERATION COUNT TO FIT.**
    #[test]
    fn the_turn_cap_lowers_the_iteration_count_for_a_wide_step_list() {
        let now = Instant::now();
        let one = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, None, None);
        assert_eq!(one.granted, MAX_LOOP_ITERATIONS);
        let four = ArmedLoop::arm(
            draft("60s /gate && /scan && /verify a && /improve"),
            ArmOrigin::User,
            now,
            None,
            None,
        );
        assert_eq!(four.steps.len(), 4);
        assert_eq!(four.granted, MAX_LOOP_TURNS / 4);
        assert!(
            four.granted * 4 <= MAX_LOOP_TURNS,
            "{} iterations of 4 steps exceeds the {MAX_LOOP_TURNS}-turn cap",
            four.granted
        );
    }

    /// Three failures in a row disarm the loop; a success in between resets the count.
    #[test]
    fn consecutive_failures_disarm_and_a_success_resets_the_count() {
        let now = Instant::now();
        let mut looping = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, None, None);
        looping.settle(now, false);
        looping.settle(now, false);
        assert_eq!(looping.stop_reason(now, None), None, "two is not three");
        looping.settle(now, true);
        looping.settle(now, false);
        looping.settle(now, false);
        assert_eq!(looping.stop_reason(now, None), None, "the success reset it");
        looping.settle(now, false);
        assert_eq!(
            looping.stop_reason(now, None),
            Some(StopReason::ConsecutiveFailures)
        );
    }

    /// A self-paced loop has no next firing until the previous iteration lands.
    #[test]
    fn a_self_paced_loop_does_not_refire_until_the_last_one_landed() {
        let now = Instant::now();
        let mut looping = ArmedLoop::arm(draft("/gate"), ArmOrigin::User, now, None, None);
        assert_eq!(looping.cadence, Cadence::SelfPaced);
        assert!(looping.due(now, None), "the first firing is immediate");
        looping.begin_iteration(now);
        let much_later = now + Duration::from_secs(600);
        assert!(
            !looping.due(much_later, None),
            "it refired while an iteration was still in flight"
        );
        looping.settle(much_later, true);
        assert!(!looping.due(much_later, None), "the settle window applies");
        assert!(looping.due(much_later + SELF_PACED_SETTLE, None));
    }

    /// A fixed-cadence loop fires immediately, then on its interval.
    #[test]
    fn a_fixed_cadence_loop_fires_now_and_then_on_the_interval() {
        let now = Instant::now();
        let mut looping = ArmedLoop::arm(draft("60s /gate"), ArmOrigin::User, now, None, None);
        assert!(
            looping.due(now, None),
            "an armed loop that waits looks dead"
        );
        looping.begin_iteration(now);
        looping.settle(now, true);
        assert!(!looping.due(now + Duration::from_secs(59), None));
        assert!(looping.due(now + Duration::from_secs(60), None));
    }

    #[test]
    fn intervals_parse_in_seconds_minutes_hours_days_and_nothing_else() {
        assert_eq!(parse_interval("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_interval("10m"), Some(Duration::from_secs(600)));
        assert_eq!(parse_interval("2h"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_interval("1d"), Some(Duration::from_secs(86_400)));
        for token in ["", "m", "10", "10x", "-5m", "1.5h", "/gate", "ten", "10 m"] {
            assert_eq!(
                parse_interval(token),
                None,
                "{token:?} parsed as an interval"
            );
        }
    }

    /// The interval floor and the ceiling both refuse, with their own words.
    #[test]
    fn an_interval_below_the_floor_or_past_the_ceiling_is_refused() {
        assert_eq!(
            parse_draft("1s /gate"),
            Err(ArmRefusal::IntervalTooShort(Duration::from_secs(1)))
        );
        assert_eq!(
            parse_draft("99h /gate"),
            Err(ArmRefusal::IntervalOutlivesCeiling)
        );
        // ⚠️ THE CONTROL. Exactly the floor is allowed, or the message would be a lie.
        assert!(parse_draft("60s /gate").is_ok());
    }

    /// 🔴 **`1d` IS REFUSED, NOT MISREAD AS A STEP.**
    ///
    /// The reference surface accepts `Nd`, so a user WILL type it. The failure this pins is the
    /// silent one: if `1d` did not parse as an interval it would become the first step of a
    /// two-step self-paced loop and RUN, having quietly dropped the cadence the user asked for.
    #[test]
    fn a_day_long_cadence_is_refused_rather_than_becoming_a_step() {
        assert_eq!(
            parse_draft("1d /gate"),
            Err(ArmRefusal::IntervalOutlivesCeiling)
        );
        assert_eq!(
            parse_draft("check the deploy every 1 day"),
            Err(ArmRefusal::IntervalOutlivesCeiling)
        );
    }

    /// Rule 2: a trailing `every …` clause is a cadence and is stripped from the prompt.
    #[test]
    fn a_trailing_every_clause_is_a_cadence_and_leaves_the_prompt_clean() {
        let parsed = draft("check the deploy every 20m");
        assert_eq!(parsed.cadence, Cadence::Fixed(Duration::from_secs(1200)));
        assert_eq!(parsed.steps, vec!["check the deploy".to_string()]);

        let worded = draft("run the gate every 5 minutes");
        assert_eq!(worded.cadence, Cadence::Fixed(Duration::from_secs(300)));
        assert_eq!(worded.steps, vec!["run the gate".to_string()]);

        assert_eq!(
            draft("watch it every 2 hours").cadence,
            Cadence::Fixed(Duration::from_secs(7200))
        );
    }

    /// 🔴 **THE CONTROL FOR RULE 2, AND IT IS THE WHOLE REASON THE RULE IS NARROW.**
    ///
    /// `check every PR` ends in the word `every`. Reading that as a cadence would drop `PR` from
    /// the user's task and run a DIFFERENT job than the one they asked for — silently, forever.
    #[test]
    fn every_not_followed_by_a_time_is_not_a_cadence() {
        for prompt in [
            "check every PR",
            "review every file that changed",
            "every",
            "look at every 3 files",
        ] {
            let parsed = draft(prompt);
            assert_eq!(
                parsed.cadence,
                Cadence::SelfPaced,
                "{prompt:?} was read as a cadence"
            );
            assert_eq!(parsed.steps, vec![prompt.to_string()], "{prompt:?} was cut");
        }
    }

    /// A missing interval is self-paced, and a first word that is not an interval is a step.
    #[test]
    fn an_omitted_interval_means_self_paced() {
        assert_eq!(draft("/gate").cadence, Cadence::SelfPaced);
        assert_eq!(
            draft("check whether the deploy is green").cadence,
            Cadence::SelfPaced
        );
        assert_eq!(
            draft("check whether the deploy is green").steps,
            vec!["check whether the deploy is green".to_string()]
        );
        assert_eq!(
            draft("10m /gate").cadence,
            Cadence::Fixed(Duration::from_secs(600))
        );
    }

    #[test]
    fn an_empty_or_cadence_only_loop_is_refused() {
        // ⚠️ Only these are genuinely EMPTY: nothing at all, whitespace, or a cadence with no
        // payload. `" && "` and `"/gate && "` were in this list and did not belong — the argument
        // is trimmed before parsing, so both leave a non-empty step behind. See
        // `a_bare_ampersand_pair_is_a_step_because_the_separator_needs_its_spaces`.
        for argument in ["", "   ", "10m", "30s", "2h", "1d", "90s"] {
            assert_eq!(
                parse_draft(argument),
                Err(ArmRefusal::Empty),
                "{argument:?} armed something"
            );
        }
    }

    /// ⚠️ **`&&` WITH NO SPACES IS PROSE, AND THIS TEST EXISTS BECAUSE I EXPECTED OTHERWISE.**
    ///
    /// The first version of the test above listed a bare `"&&"` as an empty payload. It is not:
    /// [`STEP_SEPARATOR`] is ` && ` *with its spaces*, and that rule is what stops the mixer
    /// cutting `a&&b` in prose or `!git add -A && git commit` in a shell line. Special-casing a
    /// bare `&&` here would have contradicted the rule three lines above its own definition, so
    /// the code stands and the expectation was corrected — recorded rather than quietly swapped.
    #[test]
    fn a_bare_ampersand_pair_is_a_step_because_the_separator_needs_its_spaces() {
        assert_eq!(
            parse_draft("&&"),
            Ok(LoopDraft {
                cadence: Cadence::SelfPaced,
                steps: vec!["&&".to_string()]
            })
        );
    }

    /// 🔴 **THE FAIL-CLOSED CLAUSE: A COMMAND NOBODY ALLOWED IS REFUSED, INCLUDING A NEW ONE.**
    #[test]
    fn a_step_outside_the_allowlist_is_refused_at_arm_time() {
        for (step, expected) in [
            ("/mode accept-edits", "mode"),
            ("/apply", "apply"),
            ("/login", "login"),
            ("/loop 10m /gate", "loop"),
            ("/presets", "presets"),
            ("/skill:review", "skill:"),
            // A command that does not exist at all is refused by the same clause, which is the
            // point of an allowlist: absence is refusal, not a fall-through.
            (
                "/a-command-invented-tomorrow",
                "a-command-invented-tomorrow",
            ),
        ] {
            assert_eq!(
                parse_draft(&format!("60s {step}")),
                Err(ArmRefusal::StepNotAllowed(expected.to_string())),
                "{step} was allowed"
            );
        }
        assert_eq!(parse_draft("60s !rm -rf /"), Err(ArmRefusal::ShellStep));
        // ⚠️ THE CONTROL. The allowlisted and the prose cases must still pass, or the guard is
        // just "refuse everything" wearing a list.
        assert!(parse_draft("60s /gate").is_ok());
        assert!(parse_draft("60s is the build green?").is_ok());
    }

    /// 🔴 **EVERY COMMAND IN THE CATALOG IS CLASSIFIED, AND THE TWO LISTS ARE WRITTEN OUT.**
    ///
    /// This is the completeness clause. `LOOP_ALLOWED_STEPS` alone can only say what IS allowed;
    /// it cannot notice a command nobody considered. Pairing it with a written-out refusal list
    /// and asserting the union covers the catalog turns "somebody forgot" into a red test.
    ///
    /// ⚠️ Both lists are written out by hand on purpose. Deriving either from the catalog would
    /// make this test pass by construction and catch nothing — the failure mode this repo has paid
    /// for more than once.
    #[test]
    fn every_catalog_command_is_classified() {
        let refused: Vec<&str> = LOOP_REFUSED_STEPS.iter().map(|(name, _)| *name).collect();
        for name in LOOP_ALLOWED_STEPS {
            assert!(
                !refused.contains(name),
                "/{name} is both allowed and refused"
            );
        }
        let mut unclassified = Vec::new();
        for (name, _) in commands::composer_commands() {
            if !LOOP_ALLOWED_STEPS.contains(&name) && !refused.contains(&name) {
                unclassified.push(name);
            }
        }
        assert!(
            unclassified.is_empty(),
            "these commands are neither allowed nor refused for a loop, so nobody decided \
             whether an unattended actor may run them: {unclassified:?}. \
             Add each to LOOP_ALLOWED_STEPS or to LOOP_REFUSED_STEPS with a reason."
        );
    }

    /// A refusal list entry that names nothing is a rule guarding a command that no longer exists.
    ///
    /// ⚠️ LIMIT, STATED: `marks` is deliberately not a catalog command — it is a sentinel proving
    /// this test can fail — so it is the one exemption and it is named here rather than skipped
    /// silently.
    #[test]
    fn no_classification_names_a_command_that_does_not_exist() {
        let catalog: Vec<&str> = commands::composer_commands()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let mut stale = Vec::new();
        for name in LOOP_ALLOWED_STEPS {
            if !catalog.contains(name) {
                stale.push(*name);
            }
        }
        for (name, _) in LOOP_REFUSED_STEPS {
            if !catalog.contains(name) && *name != "marks" {
                stale.push(*name);
            }
        }
        assert!(
            stale.is_empty(),
            "these classifications name commands the catalog does not have: {stale:?}"
        );
    }

    /// Mixing: a submission splits into ordered steps on ` && `, and prose does not.
    #[test]
    fn steps_split_on_the_separator_and_prose_does_not() {
        assert_eq!(
            split_steps("/gate && /verify serve/api.py && /scan", MAX_CHAIN_STEPS),
            vec![
                "/gate".to_string(),
                "/verify serve/api.py".to_string(),
                "/scan".to_string()
            ]
        );
        // No spaces around it is not a separator.
        assert_eq!(
            split_steps("true&&false", MAX_CHAIN_STEPS),
            vec!["true&&false".to_string()]
        );
        assert_eq!(
            split_steps("/gate &&  && /scan", MAX_CHAIN_STEPS),
            vec!["/gate".to_string(), "/scan".to_string()],
            "an empty fragment is a typo, not a step"
        );
        assert_eq!(split_steps("", MAX_CHAIN_STEPS), Vec::<String>::new());
    }

    /// 🔴 **A SHELL LINE IS NEVER A CHAIN, AND CUTTING ONE WOULD RUN HALF A COMMAND.**
    #[test]
    fn a_shell_submission_is_never_split_into_steps() {
        assert!(!is_chain("!git add -A && git commit -m x"));
        assert!(!is_chain("/gate"));
        assert!(!is_chain("just some prose"));
        assert!(is_chain("/gate && /scan"));
    }

    /// 🔴 **`/loop` OWNS ITS OWN `&&`, AND THE MIXER MUST NOT REACH INSIDE IT.**
    ///
    /// This is a defect the two features created in each other: the mixer split
    /// `/loop 10m /gate && /scan` into an arming request carrying HALF the payload plus an
    /// unrelated `/scan` turn — and it ran, which is the worst shape of wrong. The separator has
    /// exactly one owner per submission, and it is the outermost command.
    #[test]
    fn the_mixer_never_reaches_inside_a_loop_payload() {
        assert!(!is_chain("/loop 10m /gate && /scan"));
        assert!(
            !is_chain("/LOOP 10m /gate && /scan"),
            "case must not defeat it"
        );
        let parsed = parse_draft("10m /gate && /scan").expect("a two-step payload");
        assert_eq!(parsed.steps, vec!["/gate".to_string(), "/scan".to_string()]);
    }

    #[test]
    fn a_chain_is_capped_at_its_own_bound() {
        let many = (0..40).map(|_| "/gate").collect::<Vec<_>>().join(" && ");
        assert_eq!(split_steps(&many, MAX_CHAIN_STEPS).len(), MAX_CHAIN_STEPS);
    }

    #[test]
    fn a_loop_carrying_more_than_the_step_cap_is_refused() {
        let many = (0..MAX_LOOP_STEPS + 1)
            .map(|_| "/gate")
            .collect::<Vec<_>>()
            .join(" && ");
        assert_eq!(
            parse_draft(&format!("60s {many}")),
            Err(ArmRefusal::TooManySteps(MAX_LOOP_STEPS + 1))
        );
    }

    /// The directive reader takes the first, strips them all, and refuses an oversized body.
    #[test]
    fn the_agent_directive_is_single_shot_stripped_and_length_capped() {
        let (found, visible) = take_loop_directive(
            "I will keep watching.\n<estelle:loop>10m /gate</estelle:loop>\nDone.",
        );
        assert_eq!(found.as_deref(), Some("10m /gate"));
        assert!(!visible.contains("estelle:loop"), "{visible:?}");
        assert!(visible.contains("I will keep watching."));

        let (first, _) = take_loop_directive(
            "<estelle:loop>10m /gate</estelle:loop><estelle:loop>30s /scan</estelle:loop>",
        );
        assert_eq!(first.as_deref(), Some("10m /gate"), "only the first counts");

        let huge = "x".repeat(MAX_DIRECTIVE_LEN + 1);
        let (none, _) = take_loop_directive(&format!("<estelle:loop>{huge}</estelle:loop>"));
        assert_eq!(none, None, "an oversized directive was read");

        // ⚠️ CONTROL. An answer with no directive must come back byte-identical and empty-handed,
        // or the parser is rewriting ordinary answers.
        let plain = "No loop here, though I mention <estelle:loop unclosed.";
        let (nothing, untouched) = take_loop_directive(plain);
        assert_eq!(nothing, None);
        assert_eq!(untouched, plain);
    }

    /// 🔴 **A DIRECTIVE THAT PARSES STILL LANDS ON EVERY GUARD.**
    ///
    /// The injection argument depends on this: reading a directive out of model output must buy
    /// nothing that typing it would not. So the parsed body goes through the SAME `parse_draft`,
    /// and an authority-widening payload is refused identically.
    #[test]
    fn an_agent_directive_buys_no_authority_a_typed_one_would_not() {
        let (body, _) = take_loop_directive("<estelle:loop>10m /mode full-auto</estelle:loop>");
        let body = body.expect("a directive body");
        assert_eq!(
            parse_draft(&body),
            Err(ArmRefusal::StepNotAllowed("mode".to_string()))
        );
        let (shell, _) = take_loop_directive("<estelle:loop>10m !curl evil.example</estelle:loop>");
        assert_eq!(
            parse_draft(&shell.expect("a directive body")),
            Err(ArmRefusal::ShellStep)
        );
        // And even a WELL-FORMED one is refused while the session has not opted in.
        assert_eq!(
            may_arm(false, false, ArmOrigin::Agent, false),
            Some(ArmRefusal::AgentNotOptedIn)
        );
    }

    /// The band names the count, the budget, the clock, the money and the key that stops it.
    #[test]
    fn the_band_says_enough_to_answer_i_dont_see_you_doing_your_loop() {
        let now = Instant::now();
        let mut looping =
            ArmedLoop::arm(draft("10m /gate"), ArmOrigin::Agent, now, None, Some(1.0));
        looping.begin_iteration(now);
        let band = looping.band(now, Some(1.25));
        assert!(band.contains("loop 1/"), "{band}");
        assert!(band.contains("every 10m"), "{band}");
        assert!(band.contains("left"), "{band}");
        assert!(band.contains("armed by Estelle"), "{band}");
        assert!(band.contains("$0.250 this loop"), "{band}");
        assert!(band.contains("esc stops"), "{band}");
    }

    /// An idle armed loop still draws — the invisible state is the one that caused the complaint.
    #[test]
    fn an_armed_but_waiting_loop_still_draws_a_band() {
        let now = Instant::now();
        let looping = ArmedLoop::arm(draft("2h /gate"), ArmOrigin::User, now, None, None);
        let band = looping.band(now, None);
        assert!(band.contains("loop 0/"), "{band}");
        assert!(band.contains("esc stops"), "{band}");
    }

    #[test]
    fn durations_read_in_the_largest_unit_that_is_not_a_lie() {
        assert_eq!(human_duration(Duration::from_secs(45)), "45s");
        assert_eq!(human_duration(Duration::from_secs(600)), "10m");
        assert_eq!(human_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(human_duration(Duration::from_secs(5400)), "1h30m");
        assert_eq!(human_duration(MAX_LOOP_WALL_CLOCK), "4h");
    }

    /// Every refusal has words a user can act on, and none of them is empty.
    #[test]
    fn every_refusal_says_what_to_do_next() {
        for refusal in [
            ArmRefusal::NoNesting,
            ArmRefusal::AlreadyArmed,
            ArmRefusal::AgentNotOptedIn,
            ArmRefusal::Empty,
            ArmRefusal::TooManySteps(9),
            ArmRefusal::IntervalTooShort(Duration::from_secs(1)),
            ArmRefusal::IntervalOutlivesCeiling,
            ArmRefusal::StepNotAllowed("mode".to_string()),
            ArmRefusal::ShellStep,
        ] {
            let line = refusal.line();
            assert!(line.len() > 20, "{refusal:?} says {line:?}");
        }
        for reason in [
            StopReason::IterationsSpent,
            StopReason::DeadlineReached,
            StopReason::AutonomyRaised,
            StopReason::ConsecutiveFailures,
            StopReason::Stopped,
        ] {
            assert!(reason.line(3).contains("3 iterations"), "{reason:?}");
        }
    }

    /// ⚠️ **WHERE 'STOPPED' IS ASSERTED, NOW THAT THIS FILE NO LONGER OWNS IT.**
    ///
    /// The deleted `stop()` had a test here. Stopping is now `App::stop_loop` dropping the loop,
    /// so the assertion that means anything is behavioural and lives in `main.rs`:
    /// `esc_disarms_a_loop_and_it_never_fires_again` presses the key and then drives the ticker's
    /// entry point twice, asserting nothing is enqueued. This stub exists so a reader looking for
    /// the missing test finds the pointer instead of concluding there is a hole.
    #[test]
    fn stopping_is_owned_by_the_app_and_asserted_there() {
        assert_eq!(
            StopReason::Stopped.line(3),
            "Loop stopped after 3 iterations: stopped."
        );
    }
}
