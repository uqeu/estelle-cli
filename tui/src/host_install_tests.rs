//! Tests for [`crate::host_install`].
//!
//! Split out because the table plus its two verbs plus their tests ran past this repo's 800-line
//! hard limit, and the test half is the half that grows.

use super::*;
use std::fs;

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

/// The skill directory is named after us, so leaving it behind is our litter.
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
    assert!(
        !home.path().join(".claude/skills/estelle").exists(),
        "the skill directory must not survive uninstall"
    );
    assert!(
        home.path().join(".claude/skills").is_dir(),
        "the host's own skills directory is not ours to delete"
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
