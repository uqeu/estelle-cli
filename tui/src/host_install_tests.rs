//! Tests for [`crate::host_install`].
//!
//! Split out because the table plus its two verbs plus their tests ran past this repo's 800-line
//! hard limit, and the test half is the half that grows.

use super::*;
use std::fs;
use std::path::PathBuf;

/// Every row must name at least one real path, or it is a host we advertise and cannot reach.
#[test]
fn no_host_is_advertised_without_a_path_that_reaches_it() {
    for host in HOSTS {
        assert!(
            host.project.is_some() || host.user.is_some(),
            "{} has no path in either scope",
            host.name
        );
        assert!(!host.note.is_empty(), "{} has no note", host.name);
    }
}

/// **Every file `brief` can WRITE must be a file `uninstall` can CLEAR.**
///
/// The two tables are separate owners of "which files carry the managed block", and the half
/// they disagree about is exactly the half a customer cannot get rid of. When this table was
/// first written it covered eleven of [`agent_brief::AGENT_FILES`]'s sixteen entries, so
/// `.cursorrules`, `.windsurfrules`, `.rules`, `CONVENTIONS.md` and `AGENT.md` could be
/// written by `estelle brief` and never removed by anything. This test names the clause
/// rather than counting rows: a count would go green the day someone added a twelfth row for
/// a file that is not in `AGENT_FILES` at all.
#[test]
fn every_written_file_can_also_be_uninstalled() {
    let reachable: Vec<&str> = HOSTS
        .iter()
        .flat_map(|host| host.project.into_iter().chain(host.legacy.iter().copied()))
        .collect();
    let orphans: Vec<&&str> = agent_brief::AGENT_FILES
        .iter()
        .filter(|file| !reachable.contains(*file))
        .collect();
    assert!(
        orphans.is_empty(),
        "estelle brief writes these and estelle uninstall cannot clear them: {orphans:?}"
    );
}

/// The inverse clause, so the two tables cannot drift the other way either: a path this table
/// creates that `brief` does not know about is a file `brief` would never keep current.
#[test]
fn every_installed_project_path_is_a_file_brief_also_knows() {
    for host in HOSTS {
        let Some(project) = host.project else {
            continue;
        };
        assert!(
            agent_brief::AGENT_FILES.contains(&project),
            "{} installs {project}, which is not in AGENT_FILES",
            host.name
        );
    }
}

/// The mechanism split, pinned. `install-hooks` writes for exactly two hosts, and those two
/// are the only rows that may claim a hook — a third row claiming one would be advertising a
/// hook nothing installs.
#[test]
fn only_the_hosts_install_hooks_writes_for_may_claim_a_hook() {
    let hooked: Vec<&str> = HOSTS
        .iter()
        .filter(|host| host.shapes.contains(&Shape::Hook))
        .map(|host| host.name)
        .collect();
    assert_eq!(hooked, vec!["claude", "codex"]);
    for host in HOSTS {
        assert!(
            host.shapes.contains(&Shape::Instructions),
            "{} is in an instruction-file table and claims no instruction file",
            host.name
        );
    }
}

#[test]
fn host_names_are_unique_so_a_lookup_cannot_be_ambiguous() {
    let mut names: Vec<&str> = HOSTS.iter().map(|host| host.name).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate host name in HOSTS");
}

#[test]
fn an_unknown_host_is_refused_and_the_refusal_names_the_real_ones() {
    let error = lookup("emacs").expect_err("unknown host must refuse");
    assert!(error.contains("unknown host emacs"));
    assert!(error.contains("claude"), "the refusal must list real hosts");
}

#[test]
fn the_listing_covers_every_row_and_says_which_mechanism_each_one_uses() {
    let lines = listing();
    assert_eq!(lines.len(), HOSTS.len() + 1, "one header plus one row each");
    // Anchored at the start of its own line, not a substring of the whole listing: "pi" is a
    // substring of "copilot", so a `contains` over the joined text would pass for a row that
    // was never printed. A vacuity guard that cannot fail is the defect this repo pays for
    // most often.
    for (host, line) in HOSTS.iter().zip(lines.iter().skip(1)) {
        assert!(
            line.starts_with(host.name),
            "row {} does not start with {}",
            line,
            host.name
        );
        assert!(
            line.contains(host.shape_labels().as_str()),
            "row {} does not say how {} is reached",
            line,
            host.name
        );
    }
    let rendered = lines.join("\n");
    assert!(rendered.contains("instruction file + hook"));
}

/// A named host is a request to REACH it, so the file must appear where it was not before —
/// and the bytes must be the managed block, not an empty file.
#[test]
fn naming_a_host_creates_its_file_with_the_managed_block() {
    let root = tempfile::tempdir().expect("root");
    let lines = install(
        root.path(),
        None,
        &["copilot".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("install");
    let path = root.path().join(".github/copilot-instructions.md");
    assert!(path.is_file(), "the file must exist: {lines:?}");
    let bytes = fs::read_to_string(&path).expect("written bytes");
    assert!(bytes.contains(agent_brief::BEGIN));
    assert!(bytes.contains("api.fatelabs.ca/mcp"));
}

/// The pair that must never both be true: a host reported as installed with nothing on disk.
#[test]
fn a_reported_install_and_an_absent_file_are_unreachable_together() {
    for host in HOSTS.iter().filter(|host| host.project.is_some()) {
        // A fresh root per host: two rows may legitimately share a path (Codex and the
        // cross-vendor AGENTS.md row both read `AGENTS.md`), and a shared root would make the
        // second one report `already current` — which is a true statement about a file the
        // FIRST row wrote, and would let this pair-check pass without proving anything.
        let root = tempfile::tempdir().expect("root");
        let lines = install(
            root.path(),
            None,
            &[host.name.to_string()],
            Some(Scope::Project),
            false,
        )
        .expect("install");
        let reported = lines
            .iter()
            .any(|line| line.contains("managed Estelle block"));
        let on_disk = root.path().join(host.project.expect("filtered")).is_file();
        assert!(
            reported == on_disk,
            "{} reported {reported} and on disk {on_disk}",
            host.name
        );
    }
}

#[test]
fn a_cursor_rule_is_created_with_the_frontmatter_that_makes_it_apply() {
    let root = tempfile::tempdir().expect("root");
    install(
        root.path(),
        None,
        &["cursor".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("install");
    let bytes =
        fs::read_to_string(root.path().join(".cursor/rules/estelle.mdc")).expect("cursor rule");
    assert!(
        bytes.starts_with("---\n"),
        "a .mdc rule with no frontmatter is not an always-on rule: {bytes:.80}"
    );
    assert!(bytes.contains("alwaysApply: true"));
}

/// Uninstall must take back exactly what install wrote, across every host, in one command.
#[test]
fn uninstall_clears_every_host_install_wrote_and_leaves_no_file_behind() {
    let root = tempfile::tempdir().expect("root");
    let every: Vec<String> = HOSTS
        .iter()
        .filter(|host| host.project.is_some())
        .map(|host| host.name.to_string())
        .collect();
    install(root.path(), None, &every, Some(Scope::Project), false).expect("install");
    for host in HOSTS.iter().filter(|host| host.project.is_some()) {
        assert!(
            root.path().join(host.project.expect("filtered")).is_file(),
            "{} was not installed",
            host.name
        );
    }
    uninstall(root.path(), None, &[], Some(Scope::Project), false).expect("uninstall");
    for host in HOSTS.iter().filter(|host| host.project.is_some()) {
        let path = root.path().join(host.project.expect("filtered"));
        assert!(
            !path.exists(),
            "{} survived uninstall at {}",
            host.name,
            path.display()
        );
    }
}

#[test]
fn uninstall_keeps_every_byte_the_customer_wrote() {
    let root = tempfile::tempdir().expect("root");
    let customer = "# House rules\n\nNo force pushes.\n";
    fs::write(root.path().join("CLAUDE.md"), customer).expect("customer file");
    install(
        root.path(),
        None,
        &["claude".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("install");
    assert_ne!(
        fs::read_to_string(root.path().join("CLAUDE.md")).expect("installed"),
        customer
    );
    uninstall(
        root.path(),
        None,
        &["claude".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("uninstall");
    assert_eq!(
        fs::read_to_string(root.path().join("CLAUDE.md")).expect("restored"),
        customer
    );
}

#[test]
fn a_dry_run_writes_nothing_at_all() {
    let root = tempfile::tempdir().expect("root");
    let lines = install(
        root.path(),
        None,
        &["claude".to_string()],
        Some(Scope::Project),
        true,
    )
    .expect("dry run");
    assert!(lines.iter().any(|line| line.contains("would write")));
    assert!(!root.path().join("CLAUDE.md").exists());
}

/// An unnamed sweep is a REFRESH, not a scatter: a repository with no agent file must come
/// back with no agent file.
#[test]
fn an_unnamed_install_never_creates_a_file_the_repository_did_not_have() {
    let root = tempfile::tempdir().expect("root");
    install(root.path(), None, &[], Some(Scope::Project), false).expect("sweep");
    for host in HOSTS.iter().filter_map(|host| host.project) {
        assert!(
            !root.path().join(host).exists(),
            "an unnamed sweep created {host}"
        );
    }
}

#[test]
fn an_unnamed_sweep_refreshes_a_file_that_already_exists() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("AGENTS.md"), "# Mine\n").expect("customer file");
    install(root.path(), None, &[], Some(Scope::Project), false).expect("sweep");
    let bytes = fs::read_to_string(root.path().join("AGENTS.md")).expect("refreshed");
    assert!(bytes.contains(agent_brief::BEGIN));
    assert!(bytes.contains("# Mine"));
}

/// THE case the whole feature exists for: a user-profile install makes Estelle present
/// before any repository is opened. Measured 2026-09-19, `~/.claude/skills/` held a
/// 52,595-byte competitor skill and zero Estelle files.
#[test]
fn a_user_scope_install_puts_a_skill_where_the_host_loads_it_without_a_project() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    install(
        root.path(),
        Some(home.path()),
        &["claude".to_string()],
        Some(Scope::User),
        false,
    )
    .expect("install");
    let skill = home.path().join(".claude/skills/estelle/SKILL.md");
    let bytes = fs::read_to_string(&skill).expect("skill file");
    assert!(
        bytes.starts_with("---\nname: estelle\n"),
        "a SKILL.md without name and description front matter is not discoverable"
    );
    assert!(bytes.contains("description:"));
    assert!(bytes.contains(agent_brief::BEGIN));
    // And nothing was written into the repository, because that is not what was asked for.
    assert!(!root.path().join("CLAUDE.md").exists());
}

/// A global instruction file is a higher bar than a project one: it is loaded into every
/// session on every project the customer has. Install must ADD to it, a second install must
/// change nothing, and uninstall must hand back the original byte for byte.
#[test]
fn a_global_instruction_file_survives_install_reinstall_and_uninstall_byte_for_byte() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let original = "# My standing brief\n\n- Never force-push.\n- Ask before deploying.\n";
    fs::create_dir_all(home.path().join(".claude")).expect("claude dir");
    let global = home.path().join(".claude/CLAUDE.md");
    fs::write(&global, original).expect("global file");

    let args = ["claude".to_string()];
    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("install");
    let after_first = fs::read_to_string(&global).expect("after install");
    assert!(
        after_first.contains(original),
        "customer bytes must survive"
    );
    assert!(after_first.contains(agent_brief::BEGIN));

    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("again");
    assert_eq!(
        fs::read_to_string(&global).expect("after second install"),
        after_first,
        "a second install must be idempotent, not additive"
    );

    uninstall(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("uninstall");
    assert_eq!(
        fs::read_to_string(&global).expect("after uninstall"),
        original,
        "uninstall must return the global file exactly as the customer left it"
    );
}

/// Uninstall must take back the directories as well as the files.
///
/// When Estelle created every directory in the chain — which is what a clean home means — all of
/// them go. This was asserted the other way round at first ("`~/.claude/skills` is not ours to
/// delete"), and that assertion was simply wrong: in this scenario we made it, nothing of the
/// host's or the customer's is in it, and leaving it is litter. The protection is not a list of
/// directories we promise not to touch, it is that `remove_dir` refuses anything non-empty —
/// see `pruning_stops_at_a_directory_the_customer_also_uses`.
#[test]
fn uninstall_takes_the_skill_directory_with_it() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let args = ["claude".to_string()];
    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("install");
    assert!(home.path().join(".claude/skills/estelle").is_dir());
    uninstall(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("uninstall");
    let leftovers: Vec<String> = walk(home.path())
        .into_iter()
        .map(|path| {
            path.strip_prefix(home.path())
                .expect("under home")
                .display()
                .to_string()
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "uninstall left these in the home directory: {leftovers:?}"
    );
}

/// The same, on a home where the HOST's own files sit beside ours: its directory stays.
#[test]
fn a_hosts_own_directory_survives_uninstall_when_it_holds_the_hosts_files() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(home.path().join(".claude")).expect("claude dir");
    let theirs = home.path().join(".claude/settings.json");
    fs::write(&theirs, "{}\n").expect("host file");
    let args = ["claude".to_string()];
    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("install");
    uninstall(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("uninstall");
    assert!(theirs.is_file(), "the host's own settings must survive");
    assert!(home.path().join(".claude").is_dir());
    assert!(
        !home.path().join(".claude/skills").exists(),
        "the skills tree we created, and only we used, still goes"
    );
}

/// ...but only when it is ours alone. A file the customer put beside our SKILL.md keeps its
/// folder.
#[test]
fn a_skill_directory_holding_the_customers_own_file_is_not_deleted() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let args = ["claude".to_string()];
    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("install");
    let theirs = home.path().join(".claude/skills/estelle/notes.md");
    fs::write(&theirs, "mine\n").expect("customer file");
    uninstall(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("uninstall");
    assert!(theirs.is_file(), "a customer file beside ours must survive");
}

/// An unnamed install reaches a host this machine RUNS, and only that host.
#[test]
fn an_unnamed_user_install_reaches_a_detected_host_and_skips_an_absent_one() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    // Claude Code is "installed": its own config directory exists. Codex is not.
    fs::create_dir_all(home.path().join(".claude")).expect("claude dir");
    install(
        root.path(),
        Some(home.path()),
        &[],
        Some(Scope::User),
        false,
    )
    .expect("install");
    assert!(
        home.path().join(".claude/CLAUDE.md").is_file(),
        "a detected host must be reached"
    );
    assert!(
        home.path()
            .join(".claude/skills/estelle/SKILL.md")
            .is_file(),
        "a detected host must get the skill, which is the point of the user scope"
    );
    assert!(
        !home.path().join(".codex/AGENTS.md").exists(),
        "a host this machine does not run must not have files created for it"
    );
}

/// An uninstall that deletes the file and leaves the folder has not uninstalled.
///
/// Install calls `create_dir_all`, so `.cursor/rules/`, `.github/`, `.kiro/steering/` and the rest
/// are directories WE made. Measured 2026-09-19 before this was wired: a full install followed by
/// a full uninstall left eleven empty directories in the repository.
#[test]
fn uninstall_leaves_no_empty_directory_the_install_created() {
    let root = tempfile::tempdir().expect("root");
    let every: Vec<String> = HOSTS.iter().map(|host| host.name.to_string()).collect();
    install(root.path(), None, &every, Some(Scope::Project), false).expect("install");
    uninstall(root.path(), None, &[], Some(Scope::Project), false).expect("uninstall");
    let leftovers: Vec<String> = walk(root.path())
        .into_iter()
        .map(|path| {
            path.strip_prefix(root.path())
                .expect("under root")
                .display()
                .to_string()
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "uninstall left these behind: {leftovers:?}"
    );
}

/// A directory holding anything of the customer's survives, content and all.
#[test]
fn pruning_stops_at_a_directory_the_customer_also_uses() {
    let root = tempfile::tempdir().expect("root");
    install(
        root.path(),
        None,
        &["copilot".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("install");
    let theirs = root.path().join(".github/workflows.yml");
    fs::write(&theirs, "on: push\n").expect("customer file");
    uninstall(root.path(), None, &[], Some(Scope::Project), false).expect("uninstall");
    assert!(theirs.is_file(), "a customer file in .github must survive");
    assert!(root.path().join(".github").is_dir());
}

/// Everything still under `root`, files and directories both.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    // Bounded: a table row is at most MAX_PRUNE_DEPTH deep, and a runaway here would hang the
    // suite rather than fail it.
    for _ in 0..64 {
        let Some(directory) = stack.pop() else { break };
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            found.push(path);
        }
    }
    found
}

/// **A bare install must not be able to make a host look present.**
///
/// Detection reads a host's own config directory, and several of those directories are ones a
/// user-scope install also writes into — so if an unnamed sweep ever CREATED one, the next sweep
/// would read our own file as proof the host is installed, and the check would be reporting on
/// itself. The invariant that rules that out is NOT "no marker overlaps a path we write" — several
/// do, and a carve-out list would rot the first time a row moved. It is this: **an unnamed install
/// on a clean home writes nothing at all**, so the only way a marker can appear is a customer
/// naming that host, which is them telling us it is installed.
#[test]
fn an_unnamed_install_cannot_make_an_absent_host_look_present() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    for host in HOSTS {
        assert!(
            !detected(host, Some(home.path())).expect("detect"),
            "{} looked present on an empty home",
            host.name
        );
    }
    install(root.path(), Some(home.path()), &[], None, false).expect("sweep");
    let created: Vec<&str> = HOSTS
        .iter()
        .filter(|host| detected(host, Some(home.path())).unwrap_or(false))
        .map(|host| host.name)
        .collect();
    assert!(
        created.is_empty(),
        "an unnamed install created the marker that makes these look installed: {created:?}"
    );
    assert_eq!(
        fs::read_dir(home.path()).expect("home").count(),
        0,
        "an unnamed install on a clean home must write nothing at all"
    );
}

#[test]
fn a_user_scope_install_refuses_rather_than_guessing_when_there_is_no_home() {
    let root = tempfile::tempdir().expect("root");
    let error = install(
        root.path(),
        None,
        &["claude".to_string()],
        Some(Scope::User),
        false,
    )
    .expect_err("a user install with no home must refuse");
    assert!(error.contains("home directory"));
    // The same command scoped to the project must still work: an absent home has nothing to
    // do with writing a file into this repository.
    install(
        root.path(),
        None,
        &["claude".to_string()],
        Some(Scope::Project),
        false,
    )
    .expect("a project install must not need a home");
    assert!(root.path().join("CLAUDE.md").is_file());
}

#[test]
fn scopes_visits_both_halves_when_none_is_named() {
    assert_eq!(scopes(None), EVERY_SCOPE);
    assert_eq!(scopes(None).len(), 2);
    assert_eq!(scopes(Some(Scope::User)), &[Scope::User]);
}

/// **A file the customer wrote must survive uninstall, even when its bytes look like ours.**
///
/// Cursor's `.mdc` frontmatter is four lines long and Estelle configures the exact same four
/// lines as this host's preamble, so a customer whose rule file held nothing else was
/// indistinguishable from a file Estelle created — to a check comparing CONTENT. Install
/// correctly preserved those bytes and appended the managed block; uninstall then read the
/// remainder, found it equal to the configured preamble, and deleted the whole file with no
/// backup. The same shape reaches `SKILL.md`, `.kiro/steering/estelle.md` and
/// `.devin/rules/estelle.md`, which are the other three rows with a preamble.
#[test]
fn a_customer_file_holding_only_our_frontmatter_is_not_deleted_by_uninstall() {
    let root = tempfile::tempdir().expect("root");
    let rule = root.path().join(".cursor/rules/estelle.mdc");
    fs::create_dir_all(rule.parent().expect("rule parent")).expect("rule directory");
    fs::write(&rule, CURSOR_MDC_FRONTMATTER).expect("the customer's own rule file");
    let args = ["cursor".to_string()];
    install(root.path(), None, &args, Some(Scope::Project), false).expect("install");
    uninstall(root.path(), None, &args, Some(Scope::Project), false).expect("uninstall");
    assert!(
        rule.is_file(),
        "uninstall deleted a file the customer wrote, because its bytes matched our preamble"
    );
    assert_eq!(
        fs::read_to_string(&rule).expect("kept bytes"),
        CURSOR_MDC_FRONTMATTER,
        "uninstall must return the customer's file byte-for-byte"
    );
}

/// **A directory that was here before we arrived is not ours to remove.**
///
/// `create_dir_all` records no ownership, so an uninstall that pruned upward until `remove_dir`
/// refused would take a `~/.claude` the customer already had — and that directory is exactly the
/// marker [`detected`] reads to decide whether this host is installed at all, so removing it
/// also silently disables the next unnamed install.
#[test]
fn a_directory_that_existed_before_the_install_survives_the_uninstall() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(home.path().join(".claude")).expect("the customer's own .claude");
    let args = ["claude".to_string()];
    install(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("install");
    uninstall(
        root.path(),
        Some(home.path()),
        &args,
        Some(Scope::User),
        false,
    )
    .expect("uninstall");
    assert!(
        home.path().join(".claude").is_dir(),
        "a directory that existed before the install must still be here after the uninstall"
    );
    assert!(
        !home.path().join(".claude/skills").exists(),
        "the tree we did create still goes"
    );
}

/// The bound, asserted against the thing it bounds.
///
/// [`MAX_PRUNE_DEPTH`] stated its invariant in a docstring — `.claude/skills/estelle/SKILL.md`
/// "is the deepest row today" — and nothing compared it against the table. A clause with no line
/// enforcing it is a silent exemption, and this one rots the day somebody adds a deeper row:
/// the install creates every level and the uninstall stops short, leaving an empty directory in
/// a customer's home with no test going red.
#[test]
fn no_host_row_is_deeper_than_the_prune_bound() {
    for host in HOSTS {
        for path in host
            .project
            .into_iter()
            .chain(host.user)
            .chain(host.skill)
            .chain(host.legacy.iter().copied())
        {
            let depth = Path::new(path).components().count() - 1;
            assert!(
                depth <= MAX_PRUNE_DEPTH,
                "{} installs {path}, which is {depth} directories deep and past \
                 MAX_PRUNE_DEPTH ({MAX_PRUNE_DEPTH}); raise the bound in the commit that adds \
                 the row, or the uninstall leaves a directory behind",
                host.name
            );
        }
    }
}

/// **A host the customer NAMED is always reported on, even when nothing was written.**
///
/// `--host` is the customer telling us which assistant they use. Answering it with the generic
/// "No coding assistant was detected" — or with silence, because the only line was filtered as
/// an uninteresting `absent; nothing written` — reports an install that did not happen. This is
/// the outer-layer-reports-success shape on an installer: the command completed, so it reads as
/// a success, while the named host was never reached.
#[test]
fn a_named_host_is_reported_on_at_every_scope_even_when_nothing_is_written() {
    for host in HOSTS {
        for pass in EVERY_SCOPE {
            let home = tempfile::tempdir().expect("home");
            let root = tempfile::tempdir().expect("root");
            let lines = install(
                root.path(),
                Some(home.path()),
                &[host.name.to_string()],
                Some(*pass),
                false,
            )
            .expect("install");
            assert!(
                lines.iter().any(|line| line.contains(host.label)),
                "{} at {} scope produced no line naming it: {lines:?}",
                host.name,
                pass.label()
            );
            assert!(
                !lines
                    .iter()
                    .any(|line| line.contains("No coding assistant was detected")),
                "{} was NAMED at {} scope and answered with the no-host message: {lines:?}",
                host.name,
                pass.label()
            );
        }
    }
}
