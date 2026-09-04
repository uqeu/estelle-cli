//! Which playbook does this prompt look like? — the free, deterministic first stage.
//!
//! 🔴 **THE PICKER IS A MENU; THIS IS AN INFERENCE, AND THEY ARE NOT THE SAME PRODUCT.**
//! `/skills` opens a list the user has to remember exists and then read. The founder asked for the
//! other thing: *"you would be typing your prompt on the bottom and then on the top it would appear
//! — hey this looks like this skill, press tab to use it."* A menu makes the user do the
//! recognising. This module does the recognising, and the picker stays exactly where it is.
//!
//! ## What is built, and what is NOT
//!
//! The catalog's `skill` screen (`screens.rs`) draws a THREE-STAGE matcher: symbol overlap (free) →
//! embedding match (`$0.0001`) → a cheap model that only runs when 1 and 2 disagree (`$0.0007`).
//! **Only stage 1 is built here.** There is no embedding call and no model call on this path; a
//! keystroke must not cost money or block the composer. Where stage 1 cannot separate the top two
//! candidates, this module **says nothing** rather than guessing — the stage that would have broken
//! that tie does not exist yet, and a wrong confident suggestion is precisely the failure the
//! product exists to prevent.
//!
//! ## Why the bar is set where it is
//!
//! A suggester that fires on everything is worse than no suggester: it trains the user to ignore the
//! band, and the one time it is right they have already stopped reading. So three independent
//! floors must all clear before anything is drawn — a score floor, a margin over the runner-up, and
//! a minimum amount of actual content in the prompt. `suggest` returns `None` far more often than it
//! returns `Some`, and that is the design, not a limitation.

use std::collections::HashSet;

/// One playbook the session has learned, as the matcher needs it.
///
/// Built from the same server reply the picker is built from, so there is one owner for "which
/// playbooks exist" and this cannot drift from what `/skills` lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkillEntry {
    pub(crate) name: String,
    pub(crate) summary: String,
}

/// A confident match: what to show, and the two numbers that earned it.
///
/// `score` and `runner_up` are carried out of the matcher deliberately. A suggestion that cannot say
/// how far ahead it was is a verdict with no evidence, and the tests assert on the margin rather
/// than only on the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub(crate) name: String,
    pub(crate) summary: String,
    pub(crate) score: u32,
    pub(crate) runner_up: u32,
}

/// A term drawn from the skill's NAME is worth three summary words.
///
/// The name is what the author chose to call the thing; the summary is prose around it. Weighting
/// them equally let a skill win on incidental prose overlap, which is how "fix the typo in the
/// readme" first matched a review playbook whose summary happened to contain "change".
const NAME_TERM_WEIGHT: u32 = 3;

/// A term drawn from the skill's SUMMARY.
const SUMMARY_TERM_WEIGHT: u32 = 1;

/// The floor a candidate must clear on its own before anything is drawn.
///
/// Six is exactly two name terms, or one name term plus three summary words. One name term alone
/// (three) is NOT enough: every prompt containing "review" would otherwise summon a review
/// playbook, including "review is not what I want here".
const MIN_SCORE: u32 = 6;

/// How far ahead of the runner-up the winner must be.
///
/// The catalog's design breaks a near-tie with a cheap model. That stage is not built, so a near-tie
/// is resolved the only honest way available: by saying nothing.
const MIN_MARGIN: u32 = 2;

/// Meaningful (non-stopword) tokens the prompt must contain before it is even scored.
///
/// `hi`, `ok`, `thanks` and a bare `?` must never raise a band.
const MIN_PROMPT_TERMS: usize = 2;

/// The shortest token either side that is allowed to count as a term.
///
/// Below four characters the matches are `the`, `for`, `and`, `git`, `api` — noise that carries no
/// evidence about intent.
const MIN_TERM_CHARS: usize = 4;

/// 🔴 **BOUND THE RESOURCE BEFORE YOU TAKE IT.** The server has returned 247 playbooks in the
/// field and nothing about the wire format caps it. This runs on the keystroke path, so the scan is
/// bounded here rather than trusted to stay small. Past the bound the extra playbooks are simply not
/// candidates — which is a worse suggestion, never a slower composer.
const MAX_SCANNED_SKILLS: usize = 512;

/// The longest prompt the matcher will read.
///
/// A pasted stack trace is not a prompt, and tokenising 200KB on every keystroke is a hang. Past the
/// cap the prompt is not truncated and scored — it is refused outright, because a suggestion drawn
/// from the first 2,000 characters of a 200,000-character paste is a claim about text the matcher
/// never read.
const MAX_PROMPT_CHARS: usize = 2_000;

/// Words that carry no evidence about which playbook is wanted.
///
/// Two kinds live here: ordinary English function words, and a small set of DOMAIN words
/// (`repo`, `code`, `file`, `change`) that appear in almost every prompt a coding agent ever sees and
/// in almost every playbook summary. Leaving the domain words in made the matcher fire on
/// "what does this repo do", which is the exact failure this list exists to stop.
const STOPWORDS: &[&str] = &[
    "about",
    "after",
    "again",
    "against",
    "also",
    "always",
    "another",
    "anything",
    "around",
    "back",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "both",
    "call",
    "called",
    "change",
    "changed",
    "changes",
    "code",
    "codes",
    "could",
    "current",
    "currently",
    "does",
    "doing",
    "done",
    "down",
    "during",
    "each",
    "else",
    "even",
    "ever",
    "every",
    "file",
    "files",
    "find",
    "first",
    "from",
    "gets",
    "give",
    "goes",
    "going",
    "gone",
    "good",
    "have",
    "having",
    "help",
    "here",
    "into",
    "issue",
    "issues",
    "just",
    "keep",
    "know",
    "less",
    "like",
    "line",
    "lines",
    "look",
    "looks",
    "made",
    "make",
    "makes",
    "many",
    "maybe",
    "mine",
    "more",
    "most",
    "much",
    "must",
    "need",
    "needs",
    "never",
    "next",
    "none",
    "only",
    "other",
    "others",
    "over",
    "part",
    "please",
    "problem",
    "project",
    "really",
    "repo",
    "repos",
    "repository",
    "right",
    "same",
    "should",
    "since",
    "some",
    "something",
    "still",
    "such",
    "sure",
    "take",
    "tell",
    "than",
    "that",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "thing",
    "things",
    "think",
    "this",
    "those",
    "through",
    "time",
    "todo",
    "together",
    "under",
    "until",
    "used",
    "uses",
    "using",
    "very",
    "want",
    "wants",
    "well",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "will",
    "with",
    "within",
    "without",
    "work",
    "working",
    "works",
    "would",
    "your",
    "yours",
];

/// The one place a prompt is turned into a suggestion. Returns `None` unless every floor clears.
///
/// ⚠️ **THIS NEVER MAKES A NETWORK CALL AND NEVER TOUCHES A MODEL.** It runs on the keystroke path.
/// The only cost is tokenising a bounded prompt against a bounded catalog.
pub(crate) fn suggest(prompt: &str, catalog: &[SkillEntry]) -> Option<Suggestion> {
    if !is_suggestible(prompt) {
        return None;
    }
    let prompt_terms = terms(prompt);
    if prompt_terms.len() < MIN_PROMPT_TERMS {
        return None;
    }

    let mut best: Option<(u32, &SkillEntry)> = None;
    let mut runner_up: u32 = 0;
    for entry in catalog.iter().take(MAX_SCANNED_SKILLS) {
        let score = score_entry(entry, &prompt_terms);
        match best {
            Some((top, _)) if score <= top => runner_up = runner_up.max(score),
            Some((top, _)) => {
                runner_up = runner_up.max(top);
                best = Some((score, entry));
            }
            None => best = Some((score, entry)),
        }
    }

    let (score, entry) = best?;
    // Three independent floors. Each one has killed a real false positive in the tests below, which
    // is the only reason to believe any of them is load-bearing.
    if score < MIN_SCORE || score < runner_up.saturating_add(MIN_MARGIN) {
        return None;
    }
    Some(Suggestion {
        name: entry.name.clone(),
        summary: entry.summary.clone(),
        score,
        runner_up,
    })
}

/// Insert the playbook at the FRONT of what the user typed, preserving their text byte for byte.
///
/// 🔴 **THE USER'S TEXT IS NOT REWRITTEN, REFLOWED OR TRIMMED.** Tab is an accelerator, not an
/// editor: whatever they had typed is still there, in order, after the command. The form is the one
/// `/skills` already inserts (`/skill:<name> `), so the result is directly runnable and the existing
/// parser in `commands.rs` accepts it with no new namespace.
pub(crate) fn apply(prompt: &str, name: &str) -> String {
    format!("/skill:{name} {prompt}")
}

/// Whether a prompt is even a candidate, before any scoring happens.
///
/// Split out so the renderer and the key handler cannot disagree about it, and so each refusal has a
/// test naming it.
fn is_suggestible(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    !trimmed.is_empty()
        && prompt.chars().count() <= MAX_PROMPT_CHARS
        // A slash command is a decision the user has already made. Suggesting a playbook over the
        // top of `/review` would be arguing with them, and Tab already belongs to the command
        // popup once the line starts with a slash.
        && !trimmed.starts_with('/')
        // They are already running a playbook. Suggesting a second one is noise.
        && !trimmed.contains("skill:")
}

/// The evidence one playbook has that this prompt is about it.
fn score_entry(entry: &SkillEntry, prompt_terms: &HashSet<String>) -> u32 {
    let name_terms = name_terms(&entry.name);
    let name_hits = name_terms
        .iter()
        .filter(|term| prompt_terms.contains(*term))
        .count();
    let summary_hits = terms(&entry.summary)
        .iter()
        // A word that already scored as a name term does not score twice. Without this a skill
        // whose summary restates its own name — which most summaries do — was paid for the same
        // evidence twice and cleared the floor on a single overlapping word.
        .filter(|term| !name_terms.contains(*term) && prompt_terms.contains(*term))
        .count();
    u32::try_from(name_hits)
        .unwrap_or(u32::MAX)
        .saturating_mul(NAME_TERM_WEIGHT)
        .saturating_add(
            u32::try_from(summary_hits)
                .unwrap_or(u32::MAX)
                .saturating_mul(SUMMARY_TERM_WEIGHT),
        )
}

/// `improve-codebase-architecture` → `{improve, codebase, architecture}`.
fn name_terms(name: &str) -> HashSet<String> {
    name.split(['-', '_', ':', '.'])
        .filter_map(normalize)
        .collect()
}

/// Free text → the set of terms that carry evidence.
fn terms(text: &str) -> HashSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter_map(normalize)
        .collect()
}

/// One token, lowercased and crudely singularised, or `None` if it carries no evidence.
///
/// ⚠️ **THE STEMMING IS ONE RULE — A TRAILING PLURAL `s` — AND THAT IS DELIBERATE.**
/// The first draft also stripped `ing`, `ed` and `es`, and it was WRONG IN BOTH DIRECTIONS at once:
/// `traced` became `trac` while `trace` stayed `trace`, so the stemmer that existed to make those
/// two match is what stopped them matching. A rule that has to be right about English morphology to
/// be safe is a rule this module should not own. So the cost is paid openly: `reviewing` does not
/// match `review`, and the matcher stays silent on prompts it could have caught. **Silence is the
/// side we choose to be wrong on.**
fn normalize(token: &str) -> Option<String> {
    let lowered = token.trim().to_ascii_lowercase();
    if !lowered.chars().all(char::is_alphanumeric) {
        return None;
    }
    let stem = lowered
        .strip_suffix('s')
        .filter(|stem| stem.chars().count() >= MIN_TERM_CHARS && !stem.ends_with('s'))
        .unwrap_or(&lowered);
    (stem.chars().count() >= MIN_TERM_CHARS && !STOPWORDS.contains(&stem)).then(|| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, summary: &str) -> SkillEntry {
        SkillEntry {
            name: name.to_string(),
            summary: summary.to_string(),
        }
    }

    /// The catalog the founder's own `/skills` returns in the gallery, plus the playbook from the
    /// design screen, so the tests score against names that actually ship.
    fn catalog() -> Vec<SkillEntry> {
        vec![
            entry(
                "review",
                "Review the current change against production evidence",
            ),
            entry("trace", "Trace an issue to a bound repository symbol"),
            entry("ground", "Check an answer against the current repo graph"),
            entry(
                "improve-codebase-architecture",
                "Find deepening opportunities in a codebase and make it more testable",
            ),
            entry(
                "security-review",
                "Audit authentication and input handling for injection and secrets",
            ),
        ]
    }

    #[test]
    fn a_prompt_that_names_the_playbook_gets_the_playbook() {
        let hit = suggest(
            "improve the architecture of the retrieval layer",
            &catalog(),
        )
        .expect("a confident match");

        assert_eq!(hit.name, "improve-codebase-architecture");
        assert!(
            hit.score >= MIN_SCORE,
            "cleared on {} which is under the floor",
            hit.score
        );
        assert!(
            hit.score >= hit.runner_up + MIN_MARGIN,
            "won by {} over {}, which is inside the margin",
            hit.score,
            hit.runner_up
        );
    }

    #[test]
    fn naming_both_halves_of_a_playbook_picks_it_over_its_single_word_neighbour() {
        let hit = suggest("run a security review on the login handler", &catalog())
            .expect("the security playbook");

        // `review` is in this catalog too and shares one of the two name terms. The margin is what
        // separates them, so it is asserted rather than only the winner.
        assert_eq!(hit.name, "security-review");
        assert!(hit.score >= hit.runner_up + MIN_MARGIN, "{hit:?}");
    }

    /// 🔴 **THE NEGATIVE CONTROL. A FENCE THAT SUGGESTS ON EVERYTHING IS USELESS.**
    ///
    /// Every string here is an ordinary thing a person types at this composer, and every one of them
    /// must draw NOTHING. Without this the matcher's positive test passes against a `suggest` that
    /// returns `catalog[0]` unconditionally, which is exactly the mutant the mutation run kills.
    #[test]
    fn ordinary_prompts_suggest_nothing_at_all() {
        for prompt in [
            "fix the typo in the readme",
            "what does this repo do",
            // 🔴 THIS ONE IS HELD OUT BY THE STOPWORD LIST ALONE. Scored with an empty list it
            // reaches SIX against `review` on `current` + `change` + `production` and fires — three
            // words that appear in almost every prompt a coding agent ever sees. Without this row
            // the list is untested and could be deleted with every other test still green.
            "review the current change in production",
            "hi",
            "ok thanks",
            "why is the build red",
            "rename the variable on line 40",
            "",
            "   ",
            "?",
        ] {
            assert_eq!(
                suggest(prompt, &catalog()),
                None,
                "{prompt:?} raised a suggestion band"
            );
        }
    }

    #[test]
    fn a_slash_command_is_a_decision_already_made_and_is_never_second_guessed() {
        assert_eq!(
            suggest(
                "/review improve the architecture of the codebase",
                &catalog()
            ),
            None
        );
        assert_eq!(
            suggest("  /skills improve the architecture", &catalog()),
            None
        );
    }

    #[test]
    fn a_prompt_already_running_a_playbook_is_left_alone() {
        assert_eq!(
            suggest(
                "skill:improve-codebase-architecture improve the architecture",
                &catalog()
            ),
            None
        );
    }

    /// A near-tie is a question the built stages cannot answer, so it answers nothing.
    ///
    /// ⚠️ Both candidates here score SIX — they clear the floor comfortably. The only thing refusing
    /// them is [`MIN_MARGIN`], which is what makes this a test of the margin and not of the floor.
    #[test]
    fn two_playbooks_that_match_equally_well_cancel_each_other_out() {
        let ambiguous = vec![
            entry(
                "architecture-review",
                "Compare a proposed design against the system map",
            ),
            entry(
                "architecture-audit",
                "Compare a proposed design against the system map",
            ),
        ];
        let prompt = "review and audit the architecture";

        for candidate in &ambiguous {
            assert!(
                score_entry(candidate, &terms(prompt)) >= MIN_SCORE,
                "{} did not clear the floor, so this tests the wrong thing",
                candidate.name
            );
        }
        assert_eq!(suggest(prompt, &ambiguous), None);
    }

    /// 🔴 **[`MIN_PROMPT_TERMS`] IS AN EARLY-OUT, NOT A FLOOR — PROVEN, NOT ASSERTED.**
    ///
    /// A mutation run removed it and every test stayed green, which is the signature of a guard
    /// that cannot fail. It is not dead code: it skips scoring the whole catalog for `hi` and `ok`.
    /// But it is NOT load-bearing for correctness, because the arithmetic below shows a prompt with
    /// fewer meaningful terms than the floor **cannot reach [`MIN_SCORE`] by any route** — the best
    /// a single term can do is one name hit. Saying that out loud is the difference between a
    /// redundant guard and a guard nobody checked. ⚠️ This turns red the moment anyone lowers
    /// `MIN_SCORE` or raises `NAME_TERM_WEIGHT`, at which point the early-out becomes load-bearing
    /// and needs a behavioural test of its own.
    #[test]
    fn the_prompt_term_floor_is_subsumed_by_the_score_floor() {
        let best_possible =
            NAME_TERM_WEIGHT * u32::try_from(MIN_PROMPT_TERMS - 1).expect("a small floor");
        assert!(
            best_possible < MIN_SCORE,
            "a {}-term prompt can now reach {best_possible}, which clears the floor of {MIN_SCORE}",
            MIN_PROMPT_TERMS - 1
        );
    }

    /// 🔴 **THE ONE ASSERTION ON THE EXACT ARITHMETIC, AND WHY IT HAS TO EXIST.**
    ///
    /// `score_entry` refuses to pay a skill twice for a word that is both in its name and restated
    /// in its summary — which most summaries do. A mutation run showed that removing that filter
    /// changes **no outcome anywhere**: it only inflates every candidate, so the winner and the
    /// margin usually survive it. A rule that cannot be falsified by behaviour is a rule with no
    /// test, so it is pinned on the number instead, and the number is on the public `Suggestion`.
    #[test]
    fn a_summary_that_restates_its_own_name_is_not_paid_for_twice() {
        let restating = vec![entry(
            "deepen-architecture",
            "Deepen the architecture of a system",
        )];
        let hit = suggest("deepen the architecture", &restating).expect("a match");

        // Two name terms at three each. `deepen` and `architecture` also appear in the summary and
        // must contribute NOTHING further; `system` is a summary word the prompt does not contain.
        assert_eq!(hit.score, 6, "the summary was paid for the name's words");
        assert_eq!(hit.runner_up, 0);
    }

    #[test]
    fn one_shared_name_word_never_clears_the_floor() {
        // `review` is one name term, worth three. The floor is six. A single name hit must not fire
        // even though the prompt is otherwise a perfectly good prompt.
        assert_eq!(suggest("review the retrieval layer", &catalog()), None);
    }

    #[test]
    fn a_pasted_wall_of_text_is_refused_rather_than_read_in_part() {
        let paste = "improve architecture codebase ".repeat(200);
        assert!(paste.chars().count() > MAX_PROMPT_CHARS);
        assert_eq!(suggest(&paste, &catalog()), None);
    }

    #[test]
    fn the_scan_is_bounded_and_the_bound_is_the_named_constant() {
        let mut padded = (0..MAX_SCANNED_SKILLS)
            .map(|index| entry(&format!("filler-{index}"), "nothing at all"))
            .collect::<Vec<_>>();
        padded.push(entry(
            "improve-codebase-architecture",
            "Find deepening opportunities in a codebase",
        ));
        assert_eq!(
            suggest("improve the architecture of the codebase", &padded),
            None,
            "a playbook past the scan bound was still scored"
        );
    }

    /// 🔴 **TAB PRESERVES THE USER'S TEXT EXACTLY.** Byte equality on the tail, not a substring
    /// check: a substring assertion passes on an implementation that reflows or re-cases the draft.
    #[test]
    fn tab_inserts_at_the_front_and_changes_nothing_the_user_typed() {
        let typed = "improve the architecture of the RETRIEVAL layer, please  ";
        let applied = apply(typed, "improve-codebase-architecture");

        assert_eq!(
            applied,
            format!("/skill:improve-codebase-architecture {typed}")
        );
        assert!(applied.starts_with("/skill:improve-codebase-architecture "));
        assert_eq!(
            applied
                .strip_prefix("/skill:improve-codebase-architecture ")
                .expect("the command prefix"),
            typed,
            "the draft was rewritten on the way through"
        );
    }

    #[test]
    fn the_inserted_command_is_the_form_the_picker_already_inserts() {
        // `PickerAction::InvokeSkill` sets the composer to `/skill:<name> `. Tab must produce the
        // same runnable shape, or the two doors into a playbook disagree.
        assert!(apply("anything", "trace").starts_with("/skill:trace "));
    }

    #[test]
    fn stopwords_and_short_tokens_carry_no_evidence() {
        assert_eq!(normalize("the"), None);
        assert_eq!(normalize("repo"), None, "a domain stopword still counted");
        assert_eq!(normalize("api"), None, "a three-letter token still counted");
        assert_eq!(normalize("architectures").as_deref(), Some("architecture"));
        assert_eq!(normalize("REVIEW").as_deref(), Some("review"));
    }

    /// 🔴 **THE LIMIT, STATED OUT LOUD RATHER THAN LEFT FOR A READER TO FIND.**
    ///
    /// One plural rule means `reviewing` and `traced` are different words from `review` and
    /// `trace`, so real prompts go unmatched. This test PINS that miss so nobody reads the module's
    /// silence as coverage — and so the day someone adds a stemmer, this is the test that turns red
    /// and forces them to say what else it changed.
    #[test]
    fn the_stemmer_misses_verb_forms_and_that_is_a_known_cost() {
        assert_eq!(normalize("reviewing").as_deref(), Some("reviewing"));
        assert_eq!(normalize("traced").as_deref(), Some("traced"));
        assert_eq!(
            suggest("reviewing the security of the login handler", &catalog()),
            None,
            "the stemmer grew a verb rule without this test being updated"
        );
    }
}
