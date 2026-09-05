//! The house voice, enforced over the inherited Codex surfaces.
//!
//! 🔴 **WHY A TEST AND NOT A STYLE NOTE.** The founder read the CLI's own copy and said one thing:
//! *"lose all AI speak, use actual regular person English."* A pass of hand edits fixed 31 strings
//! and the next sweep found 111 more in the four trees this crate inherited from Codex — because
//! nothing counted them. **A number nobody measures goes back up.** This module is the meter and
//! the gate: it reads the shipped source, extracts the prose the user actually sees, and fails on
//! the constructions the founder named.
//!
//! ## What counts as AI speak here
//!
//! Not "informal". The rule is that the software does not narrate its own helpfulness, hedge,
//! apologise, greet, or address the reader as *you*:
//!
//! - **A control names what happens.** `Publish` → `Published`; never `I've published it`.
//! - **An error says what broke and what to do.** `Could not reset usage. Run /usage again.`
//! - **A wait says what is being waited on.** The model already in the book is
//!   *"still waiting for Estelle · no response received yet"* — no percentage, no ETA, no apology.
//!
//! ## What this can and cannot catch — say the limit out loud
//!
//! It reads STRING LITERALS out of the source. It therefore cannot see copy assembled at runtime
//! from pieces that are each innocent, copy that arrives from the server, or a `format!` whose
//! banned word lives in an argument. It is a floor, not a proof. What it does guarantee is that
//! the 111 constructions found on 2026-09-03 cannot come back one paste at a time.

/// The four trees this rule covers: everything the CLI inherited and did not write.
///
/// ⚠️ Scoped on purpose. `live_renderer`, `screens` and `design_book` are Estelle's own surfaces
/// and were written in this voice from the start; widening the scope is a separate measurement,
/// not a free win, and a rule that fires on 400 lines nobody has read gets disabled.
const SURFACES: [&str; 4] = ["chatwidget", "bottom_pane", "onboarding", "keymap_setup"];

/// How a rule matches. Kept explicit because `contains` on a bare word is the classic false
/// positive — `us` inside `usage`, `we` inside `weekly` — and a rule that cries wolf gets deleted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Match {
    /// The needle must appear as a whole ASCII word (apostrophes count as part of the word).
    Word,
    /// The needle must appear as a substring, lowercased.
    Phrase,
}

/// One banned construction, and the reason it is banned.
struct Rule {
    needle: &'static str,
    how: Match,
    why: &'static str,
}

const fn word(needle: &'static str, why: &'static str) -> Rule {
    Rule {
        needle,
        how: Match::Word,
        why,
    }
}

const fn phrase(needle: &'static str, why: &'static str) -> Rule {
    Rule {
        needle,
        how: Match::Phrase,
        why,
    }
}

const SECOND_PERSON: &str =
    "second person — the interface states what is true, it does not address the reader";
const FIRST_PERSON: &str = "first person — the software narrating itself as a helper";
const GREETING: &str = "a greeting or a thank-you — the software being sociable instead of useful";
const APOLOGY: &str = "an apology — say what broke and what to do instead";
const HEDGE: &str = "a hedge — specific beats friendly";
const REASSURANCE: &str = "reassurance — a wait says what is being waited on, nothing else";
const POLITENESS: &str = "politeness filler — an instruction is not a request";

/// Every construction the founder named, plus the families each one belongs to.
const RULES: &[Rule] = &[
    word("you", SECOND_PERSON),
    word("your", SECOND_PERSON),
    word("yours", SECOND_PERSON),
    word("yourself", SECOND_PERSON),
    word("you're", SECOND_PERSON),
    word("you'll", SECOND_PERSON),
    word("you've", SECOND_PERSON),
    word("you'd", SECOND_PERSON),
    word("i", FIRST_PERSON),
    word("i'm", FIRST_PERSON),
    word("i'll", FIRST_PERSON),
    word("i've", FIRST_PERSON),
    word("we", FIRST_PERSON),
    word("we're", FIRST_PERSON),
    word("we'll", FIRST_PERSON),
    word("we've", FIRST_PERSON),
    word("our", FIRST_PERSON),
    word("ours", FIRST_PERSON),
    word("let's", FIRST_PERSON),
    word("lets", FIRST_PERSON),
    word("welcome", GREETING),
    word("thanks", GREETING),
    phrase("thank you", GREETING),
    phrase("hi there", GREETING),
    phrase("happy to", GREETING),
    phrase("great!", GREETING),
    word("awesome", GREETING),
    word("sorry", APOLOGY),
    phrase("apolog", APOLOGY),
    word("unfortunately", APOLOGY),
    word("oops", APOLOGY),
    word("whoops", APOLOGY),
    word("please", POLITENESS),
    word("kindly", POLITENESS),
    word("perhaps", HEDGE),
    phrase("might want", HEDGE),
    phrase("may want", HEDGE),
    phrase("seems like", HEDGE),
    phrase("should be fine", HEDGE),
    phrase("a bit", HEDGE),
    phrase("hang tight", REASSURANCE),
    phrase("no action is required", REASSURANCE),
    phrase("don't worry", REASSURANCE),
    phrase("no worries", REASSURANCE),
    phrase("feel free", REASSURANCE),
    phrase("sit tight", REASSURANCE),
    phrase("just a moment", REASSURANCE),
];

/// A literal this rule may not touch, and the reason.
///
/// 🔴 **AN EXEMPTION WITH A REASON IS NOT A SILENT EXEMPTION — AND IT IS CHECKED.** The test below
/// asserts every entry here is still PRESENT in the tree, so an exemption that outlives the string
/// it protects fails loudly instead of quietly widening the rule.
struct Exempt {
    literal: &'static str,
    why: &'static str,
}

const EXEMPT: &[Exempt] = &[Exempt {
    literal: "Invalid prompt: we've limited access to this content for safety reasons.",
    why: "not our voice: `LEGACY_SAFETY_ACCESS_BLOCK_PREFIX` in chatwidget/turn_runtime.rs is \
          matched with `starts_with` against a message the SERVER sends. Rewriting it would not \
          change one rendered character and would silently break the detection.",
}];

/// One offending literal, located.
#[derive(Debug)]
struct Finding {
    file: String,
    line: usize,
    needle: &'static str,
    why: &'static str,
    text: String,
}

/// True when `needle` appears in `haystack` as a whole word.
///
/// The word alphabet is ASCII letters plus the apostrophe, so `you're` matches as one word and
/// `we` does not match inside `weekly`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let is_word = |byte: u8| byte.is_ascii_alphabetic() || byte == b'\'';
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_word(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        // Bounded by construction: `from` strictly increases, so this terminates in at most
        // `haystack.len()` iterations.
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    false
}

/// Prose the user can read, as opposed to an identifier, a path, or a format skeleton.
fn is_prose(literal: &str) -> bool {
    let trimmed = literal.trim();
    if trimmed.len() < 3 || !trimmed.contains(' ') {
        return false;
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
    {
        return false;
    }
    true
}

/// Every double-quoted literal on one source line, with escapes left as written.
///
/// ⚠️ Deliberately naive: it does not understand raw strings or a literal split across lines, and
/// it will read a `"` inside a comment. Both directions of that error are safe here — a missed
/// literal is a miss this rule already admits to, and a comment that trips the rule is a comment
/// worth rewriting.
fn literals_on(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut current = String::new();
        let mut closed = false;
        while let Some((_, next)) = chars.next() {
            match next {
                '\\' => {
                    if let Some((_, escaped)) = chars.next() {
                        current.push(escaped);
                    }
                }
                '"' => {
                    closed = true;
                    break;
                }
                other => current.push(other),
            }
        }
        if closed {
            out.push(current);
        }
    }
    out
}

#[cfg(test)]
mod scan {
    use super::*;
    use std::path::Path;
    use std::path::PathBuf;

    /// Files under `dir` that ship, i.e. not the ones whose whole job is testing.
    fn shipped_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        // Bounded: the tree is finite and nothing pushes a path twice.
        while let Some(path) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let child = entry.path();
                if child.is_dir() {
                    if child.file_name().is_some_and(|name| name == "tests") {
                        continue;
                    }
                    stack.push(child);
                } else if child.extension().is_some_and(|ext| ext == "rs") {
                    let name = child
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string();
                    if !name.ends_with("_tests.rs") && name != "tests.rs" {
                        out.push(child);
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// The `chatwidget/` tree plus the `chatwidget.rs` beside it, for each surface.
    fn surface_files(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for surface in SURFACES {
            let dir = root.join(surface);
            if dir.is_dir() {
                out.extend(shipped_files(&dir));
            }
            let sibling = root.join(format!("{surface}.rs"));
            if sibling.is_file() {
                out.push(sibling);
            }
        }
        out
    }

    /// Line numbers that live inside a `#[cfg(test)] mod … { … }`.
    ///
    /// ⚠️ A brace count, not a parser. It over-skips a test module containing an unbalanced brace
    /// inside a string, which would HIDE a finding — the safe direction for a rule that already
    /// admits to being a floor, and the reason the vacuity guard below counts what it read.
    fn test_module_lines(source: &str) -> Vec<bool> {
        let mut inside = Vec::with_capacity(source.lines().count());
        let mut in_test = false;
        let mut pending = false;
        let mut depth: i32 = 0;
        for line in source.lines() {
            let trimmed = line.trim_start();
            if !in_test && trimmed.starts_with("#[cfg(test)]") {
                pending = true;
            }
            if pending && trimmed.contains("mod ") && trimmed.ends_with('{') {
                in_test = true;
                pending = false;
                depth = braces(line);
                inside.push(true);
                continue;
            }
            if in_test {
                depth += braces(line);
                inside.push(true);
                if depth <= 0 {
                    in_test = false;
                }
                continue;
            }
            inside.push(false);
        }
        inside
    }

    fn braces(line: &str) -> i32 {
        let open = line.matches('{').count() as i32;
        let close = line.matches('}').count() as i32;
        open - close
    }

    /// What one sweep saw, so a green can be checked for vacuity.
    struct Sweep {
        files: usize,
        prose: usize,
        findings: Vec<Finding>,
        seen_exempt: Vec<&'static str>,
    }

    fn sweep(root: &Path) -> Sweep {
        let mut result = Sweep {
            files: 0,
            prose: 0,
            findings: Vec::new(),
            seen_exempt: Vec::new(),
        };
        for path in surface_files(root) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            result.files += 1;
            let in_test = test_module_lines(&source);
            for (index, line) in source.lines().enumerate() {
                if in_test.get(index).copied().unwrap_or(false) {
                    continue;
                }
                let trimmed = line.trim_start();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    continue;
                }
                for literal in literals_on(line) {
                    if !is_prose(&literal) {
                        continue;
                    }
                    result.prose += 1;
                    judge(&path, root, index + 1, &literal, &mut result);
                }
            }
        }
        result
    }

    fn judge(path: &Path, root: &Path, line: usize, literal: &str, result: &mut Sweep) {
        if let Some(exempt) = EXEMPT.iter().find(|entry| literal.contains(entry.literal)) {
            result.seen_exempt.push(exempt.literal);
            return;
        }
        let lowered = literal.to_lowercase();
        for rule in RULES {
            let hit = match rule.how {
                Match::Word => contains_word(&lowered, rule.needle),
                Match::Phrase => lowered.contains(rule.needle),
            };
            if hit {
                result.findings.push(Finding {
                    file: path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .display()
                        .to_string(),
                    line,
                    needle: rule.needle,
                    why: rule.why,
                    text: literal.to_string(),
                });
                return;
            }
        }
    }

    fn source_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// The floors a vacuous sweep cannot clear.
    ///
    /// 🔴 **A SCAN THAT READ NOTHING REPORTS ZERO FINDINGS, WHICH READS EXACTLY LIKE A PASS.** A
    /// moved directory, a renamed surface or a `read_to_string` that quietly failed would each
    /// turn this gate green. The numbers are written out rather than derived: 130 files and 1,540
    /// prose literals were what the base commit held, and these floors sit under both.
    const FILE_FLOOR: usize = 100;
    const PROSE_FLOOR: usize = 1_200;

    #[test]
    fn the_inherited_surfaces_speak_plain_english() {
        let root = source_root();
        let result = sweep(&root);

        assert!(
            result.files >= FILE_FLOOR,
            "the sweep read {} files, under the floor of {FILE_FLOOR} — it is measuring nothing",
            result.files
        );
        assert!(
            result.prose >= PROSE_FLOOR,
            "the sweep read {} prose literals, under the floor of {PROSE_FLOOR}",
            result.prose
        );

        let report = result
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "  {}:{}  [{}]  {}\n      {}\n",
                    finding.file, finding.line, finding.needle, finding.why, finding.text
                )
            })
            .collect::<String>();
        assert!(
            result.findings.is_empty(),
            "{} AI-speak string{} in the inherited surfaces (111 on 2026-09-03, 0 after):\n{report}",
            result.findings.len(),
            if result.findings.len() == 1 { "" } else { "s" }
        );
    }

    /// 🔴 A STALE EXEMPTION IS A HOLE NOBODY OPENED ON PURPOSE.
    ///
    /// Each entry must still match something in the tree. If the string it protects is deleted or
    /// reworded, this fails and the exemption goes with it, rather than sitting in the list
    /// quietly excusing a future paste that happens to contain the same words.
    #[test]
    fn every_exemption_still_protects_a_string_that_exists() {
        let result = sweep(&source_root());
        for exempt in EXEMPT {
            assert!(
                result.seen_exempt.contains(&exempt.literal),
                "exemption no longer matches anything in the tree: {:?}\n  reason on file: {}",
                exempt.literal,
                exempt.why
            );
        }
    }

    /// The positive control. Without it the sweep above is a rule that has never been shown to
    /// fire, and `0 findings` would be indistinguishable from `0 rules that work`.
    #[test]
    fn the_rules_fire_on_the_sentences_the_founder_named() {
        let banned = [
            "I understood the request, but the file was missing.",
            "Our systems are thinking a bit more about this.",
            "Hang tight or retry in a moment.",
            "No action is required. Codex will keep waiting.",
            "Alright, let's build together.",
            "Sorry, that did not work.",
            "Welcome to the setting screen.",
            "Please try again.",
            "Your changes are saved automatically.",
        ];
        for sentence in banned {
            let lowered = sentence.to_lowercase();
            let fired = RULES.iter().any(|rule| match rule.how {
                Match::Word => contains_word(&lowered, rule.needle),
                Match::Phrase => lowered.contains(rule.needle),
            });
            assert!(fired, "no rule fires on {sentence:?}");
        }
    }

    /// The negative control. A rule set that fires on everything is a rule set nobody can satisfy,
    /// and the copy that is already right must survive it — including the wait line the design
    /// book holds up as the model.
    #[test]
    fn the_rules_stay_silent_on_copy_that_is_already_right() {
        let allowed = [
            "still waiting for Estelle · no response received yet",
            "Could not reset usage. Run /usage again.",
            "Archive the current session and exit Codex?",
            "Turn skills on or off. Changes save automatically.",
            "The workspace is out of credits. The workspace owner can add more. Notify owner?",
            "Less than 5% of the weekly limit is left. Run /status for a breakdown.",
            "Estelle, Fate Labs' grounded coding agent",
        ];
        for sentence in allowed {
            let lowered = sentence.to_lowercase();
            let fired = RULES
                .iter()
                .filter(|rule| match rule.how {
                    Match::Word => contains_word(&lowered, rule.needle),
                    Match::Phrase => lowered.contains(rule.needle),
                })
                .map(|rule| rule.needle)
                .collect::<Vec<_>>();
            assert!(fired.is_empty(), "{sentence:?} tripped {fired:?}");
        }
    }

    /// `contains_word` is the whole reason the rule set can name bare words at all.
    #[test]
    fn a_word_rule_does_not_fire_inside_a_longer_word() {
        assert!(contains_word("less than 5% of the weekly limit", "the"));
        assert!(!contains_word("less than 5% of the weekly limit", "we"));
        assert!(!contains_word("usage limit reached", "us"));
        assert!(contains_word("you're out of credits", "you're"));
        assert!(!contains_word("yourself", "you"));
    }
}
