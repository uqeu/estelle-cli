use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const BEGIN: &str =
    "<!-- BEGIN ESTELLE — managed block, safe to move, do not edit inside -->";
pub(crate) const END: &str = "<!-- END ESTELLE -->";
pub(crate) const TAIL_BEGIN: &str = "<!-- BEGIN ESTELLE FOOTER — managed -->";
pub(crate) const TAIL_END: &str = "<!-- END ESTELLE FOOTER -->";

pub(crate) const AGENT_FILES: &[&str] = &[
    "AGENTS.md",
    "AGENT.md",
    "CLAUDE.md",
    "GEMINI.md",
    "QWEN.md",
    ".cursorrules",
    ".cursor/rules/estelle.mdc",
    ".windsurfrules",
    ".windsurf/rules/estelle.md",
    ".github/copilot-instructions.md",
    ".clinerules",
    ".roorules",
    ".junie/guidelines.md",
    ".goosehints",
    ".rules",
    "CONVENTIONS.md",
    // Added 2026-09-19 with the per-host installer. Each one is a path read out of that host's
    // own documentation, cited on the row in `host_install::HOSTS` that owns it.
    ".kiro/steering/estelle.md",
    ".devin/rules/estelle.md",
    ".agents/rules/estelle.md",
    ".kilo/rules/estelle.md",
    ".trae/rules/estelle.md",
];

const BODY: &str = r#"## Estelle is wired into this repo. Use it — do not work from memory.

Estelle is the trust layer under this session: it answers from the **real, indexed repository** and
refuses what it cannot verify. It is reachable over MCP at `https://api.fatelabs.ca/mcp`. A confident
answer about this codebase that you did not check is the exact failure Estelle exists to prevent.

**Before you state that a symbol exists in this repo — or paste code you recalled rather than read —
check it.** `verify` and the navigation tools answer from the indexed graph with no model call, so they
cannot be argued into agreeing with you.

| the moment you are in | reach for |
|---|---|
| starting a session, or returning after a gap | `estelle_resume` — what this team already decided, so you do not re-litigate it |
| about to claim a function/class/method exists | `verify`, `find_definition`, `locate` |
| about to propose a diff, patch or merge | `gate` (deterministic, no model call), then `review` when correctness is arguable |
| asked "who calls this / what breaks if I change it" | `find_usages`, `blast_radius`, `dependency_path` |
| the user refers to earlier work or "what we agreed" | the memory and session tools |
| something is broken in production | `monitor_issues`, `monitor_logs`, `monitor_alerts` |
| a dependency may have drifted, or you need a library's real API | `research_ask`, `research_drift` — reads the installed package, not the web |
| the session is about to end or overflow | `estelle_checkpoint` |

**Standing rules for this repo**

1. **Ground before you assert.** If the claim is about THIS codebase, it comes from Estelle or it comes
   with "I did not check."
2. **Gate before you propose.** Run `gate` on a diff before offering it. A blocked verdict is
   information, not an obstacle — it names the API that does not exist.
3. **Never present a recalled signature as a read one.** Names are stable; signatures drift.
4. **Estelle proposes, a human merges.** Nothing here merges or deploys on its own."#;

const FOOTER: &str = r#"**Estelle:** ground claims about this repo with `verify`/`find_definition`, and run
`gate` on any diff before proposing it. Do not answer from memory about this codebase."#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WriteOutcome {
    Skipped(PathBuf),
    Unchanged(PathBuf),
    WouldWrite(PathBuf),
    Wrote { path: PathBuf, backup: bool },
}

pub(crate) fn render_block() -> String {
    format!("{BEGIN}\n{BODY}\n{END}")
}

pub(crate) fn render_footer() -> String {
    format!("{TAIL_BEGIN}\n{FOOTER}\n{TAIL_END}")
}

fn replace_region(
    text: &str,
    begin: &str,
    end: &str,
    replacement: &str,
) -> Result<Option<String>, String> {
    let Some(start) = text.find(begin) else {
        if text.contains(end) {
            return Err(format!("found {end} without its opening marker"));
        }
        return Ok(None);
    };
    let after_begin = start + begin.len();
    let Some(relative_end) = text[after_begin..].find(end) else {
        return Err(format!("found {begin} without its closing marker"));
    };
    let after_end = after_begin + relative_end + end.len();
    Ok(Some(format!(
        "{}{}{}",
        &text[..start],
        replacement,
        &text[after_end..]
    )))
}

pub(crate) fn apply(existing: Option<&str>) -> Result<String, String> {
    let block = render_block();
    let footer = render_footer();
    let Some(existing) = existing else {
        return Ok(format!("{block}\n\n{footer}\n"));
    };

    let mut output = match replace_region(existing, BEGIN, END, &block)? {
        Some(replaced) => replaced,
        None => format!("{block}\n\n{existing}"),
    };
    output = match replace_region(&output, TAIL_BEGIN, TAIL_END, &footer)? {
        Some(replaced) => replaced,
        None => {
            let separator = if output.ends_with('\n') { "\n" } else { "\n\n" };
            format!("{output}{separator}{footer}\n")
        }
    };
    Ok(output)
}

pub(crate) fn is_current(existing: &str) -> bool {
    existing.contains(&render_block()) && existing.contains(&render_footer())
}

/// How many newlines `apply` may have written as a separator around a managed region. `apply`
/// writes `\n\n` after the head block and either `\n` or `\n\n` before the footer, so an
/// uninstall that absorbs more than this would be eating customer blank lines.
const SEPARATOR_NEWLINES: usize = 2;

/// Delete one managed region and the separator `apply` wrote with it.
///
/// Returns `Ok(None)` when the opening marker is absent, and refuses a half-open region for the
/// same reason [`replace_region`] does: a file whose markers do not pair is a file we cannot edit
/// without guessing where the customer's bytes begin.
fn cut_region(text: &str, begin: &str, end: &str) -> Result<Option<String>, String> {
    let Some(start) = text.find(begin) else {
        if text.contains(end) {
            return Err(format!("found {end} without its opening marker"));
        }
        return Ok(None);
    };
    let after_begin = start + begin.len();
    let Some(relative_end) = text[after_begin..].find(end) else {
        return Err(format!("found {begin} without its closing marker"));
    };
    let mut head = start;
    let mut tail = after_begin + relative_end + end.len();
    // `apply` put the separator AFTER the head block and BEFORE the footer, so absorb in both
    // directions rather than assuming which region this is.
    let mut absorbed = 0;
    while absorbed < SEPARATOR_NEWLINES && text[tail..].starts_with('\n') {
        tail += 1;
        absorbed += 1;
    }
    while absorbed < SEPARATOR_NEWLINES && text[..head].ends_with('\n') {
        head -= 1;
        absorbed += 1;
    }
    Ok(Some(format!("{}{}", &text[..head], &text[tail..])))
}

/// Every byte of `existing` that Estelle did not write, or `None` when Estelle wrote none of it.
///
/// This is the inverse `write` never had. A tool that writes into a customer's instruction file
/// and cannot take itself back out is not finished, and for sixteen file shapes there was no
/// removal path at all — `remove_editor_configs` clears MCP entries and `uninstall_hooks` clears
/// hook settings, and neither one has ever touched a managed block.
pub(crate) fn strip(existing: &str) -> Result<Option<String>, String> {
    let head = cut_region(existing, BEGIN, END)?;
    let footer = cut_region(head.as_deref().unwrap_or(existing), TAIL_BEGIN, TAIL_END)?;
    Ok(match (head, footer) {
        (_, Some(stripped)) => Some(stripped),
        (Some(stripped), None) => Some(stripped),
        (None, None) => None,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RemoveOutcome {
    Absent(PathBuf),
    NotInstalled(PathBuf),
    WouldRemove(PathBuf),
    Removed { path: PathBuf, deleted: bool },
}

/// Take the managed block out of one already-resolved path. Sibling of [`write_at`]: one owner
/// for the bytes, two callers for the two scopes.
///
/// `preamble` must be whatever [`write_at`] was given for this host, or a `.mdc` rule file that
/// held nothing but Estelle's frontmatter and Estelle's block survives uninstall as an orphan
/// header — written by us, left behind by us, and invisible to the customer who asked us to go.
pub(crate) fn remove_at(
    path: &Path,
    preamble: Option<&str>,
    dry_run: bool,
) -> Result<RemoveOutcome, String> {
    let path = path.to_path_buf();
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing to rewrite symlinked agent instruction file: {}",
                path.display()
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "refusing to rewrite non-file agent instruction path: {}",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RemoveOutcome::Absent(path));
        }
        Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
    }
    let existing = fs::read_to_string(&path)
        .map_err(|error| format!("refusing to rewrite unreadable {}: {error}", path.display()))?;
    let Some(stripped) = strip(&existing)? else {
        return Ok(RemoveOutcome::NotInstalled(path));
    };
    if dry_run {
        return Ok(RemoveOutcome::WouldRemove(path));
    }
    let remainder = stripped.trim();
    let deleted =
        remainder.is_empty() || preamble.is_some_and(|preamble| remainder == preamble.trim());
    if deleted {
        // No backup: what is being deleted is bytes ESTELLE wrote, and nothing else — that is
        // exactly what `deleted` means. A `.bak` here would leave a copy of our own content in
        // the customer's configuration directory after they asked us to leave, and in a skill
        // folder it also keeps the folder alive, because `remove_dir` refuses a directory that
        // still holds a file. (Found 2026-09-19 by `uninstall_takes_the_skill_directory_with_it`,
        // which went red on a real leftover rather than on a contrived one.)
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    } else {
        fs::copy(&path, backup_path(&path)).map_err(|error| error.to_string())?;
        fs::write(&path, stripped).map_err(|error| error.to_string())?;
    }
    Ok(RemoveOutcome::Removed { path, deleted })
}

pub(crate) fn remove_line(outcome: RemoveOutcome) -> String {
    match outcome {
        RemoveOutcome::Absent(path) => format!("{}: absent; nothing changed", path.display()),
        RemoveOutcome::NotInstalled(path) => {
            format!("{}: no Estelle block; nothing changed", path.display())
        }
        RemoveOutcome::WouldRemove(path) => {
            format!(
                "{}: would remove the Estelle block; nothing changed",
                path.display()
            )
        }
        RemoveOutcome::Removed { path, deleted } => {
            if deleted {
                format!(
                    "{}: held only Estelle's bytes, so the file was deleted and no backup was kept",
                    path.display()
                )
            } else {
                format!(
                    "{}: removed the Estelle block; original copied to .bak",
                    path.display()
                )
            }
        }
    }
}

/// Resolve a repository-relative instruction path, refusing anything that leaves the repository.
///
/// Both the escape check (`..`, absolute, rooted) and the symlinked-ancestor check live here, so
/// every project-scope write goes through one gate. `host_install` calls this rather than joining
/// the path itself: a table row is a constant today, but a path that reaches `fs::write` without
/// passing this function is one refactor away from being the hole.
pub(crate) fn confine(root: &Path, file: &Path) -> Result<PathBuf, String> {
    if file.is_absolute()
        || file.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "refusing agent instruction path outside this repository: {}",
            file.display()
        ));
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root {}: {error}", root.display()))?;
    let path = root.join(file);
    let mut existing_ancestor = path.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor.parent().ok_or_else(|| {
            format!(
                "cannot resolve parent of agent instruction path {}",
                path.display()
            )
        })?;
    }
    let canonical_ancestor = existing_ancestor.canonicalize().map_err(|error| {
        format!(
            "cannot resolve agent instruction path {}: {error}",
            existing_ancestor.display()
        )
    })?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        return Err(format!(
            "refusing agent instruction path through a symlink outside this repository: {}",
            file.display()
        ));
    }
    Ok(path)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".bak");
    PathBuf::from(value)
}

pub(crate) fn write(
    root: &Path,
    file: &Path,
    create: bool,
    dry_run: bool,
) -> Result<WriteOutcome, String> {
    write_at(&confine(root, file)?, None, create, dry_run)
}

/// Write the managed block into one already-resolved path.
///
/// Split out of [`write`] so a user-scope target (`~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`)
/// gets the SAME bytes, the same backup-before-write and the same symlink refusal as a
/// repository-scope one. [`write`] keeps the repository confinement; this function is the single
/// owner of what lands on disk, so the two scopes cannot drift apart.
///
/// `preamble` is prepended when the file is created and is host-format, not content: a Cursor
/// `.mdc` rule needs YAML frontmatter or the editor never applies it.
pub(crate) fn write_at(
    path: &Path,
    preamble: Option<&str>,
    create: bool,
    dry_run: bool,
) -> Result<WriteOutcome, String> {
    let path = path.to_path_buf();
    let existed = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing to replace symlinked agent instruction file: {}",
                path.display()
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "refusing to replace non-file agent instruction path: {}",
                path.display()
            ));
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
    };
    if !existed && !create {
        return Ok(WriteOutcome::Skipped(path));
    }
    let existing = if existed {
        Some(fs::read_to_string(&path).map_err(|error| {
            format!("refusing to replace unreadable {}: {error}", path.display())
        })?)
    } else {
        None
    };
    if existing.as_deref().is_some_and(is_current) {
        return Ok(WriteOutcome::Unchanged(path));
    }
    let rendered = match (preamble, existing.as_deref()) {
        // Only a file we are CREATING gets the preamble. Prepending frontmatter to a file the
        // customer already wrote would move their first line out of position, and a second run
        // would stack a second copy of it.
        (Some(preamble), None) => format!("{preamble}\n{}", apply(None)?),
        (_, existing) => apply(existing)?,
    };
    if dry_run {
        return Ok(WriteOutcome::WouldWrite(path));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if existed {
        fs::copy(&path, backup_path(&path)).map_err(|error| error.to_string())?;
    }
    fs::write(&path, rendered).map_err(|error| error.to_string())?;
    Ok(WriteOutcome::Wrote {
        path,
        backup: existed,
    })
}

pub(crate) fn detected(root: &Path) -> Vec<PathBuf> {
    AGENT_FILES
        .iter()
        .map(PathBuf::from)
        .filter(|file| root.join(file).is_file())
        .collect()
}

pub(crate) fn outcome_line(outcome: WriteOutcome) -> String {
    match outcome {
        WriteOutcome::Skipped(path) => format!("{}: absent; nothing written", path.display()),
        WriteOutcome::Unchanged(path) => {
            format!("{}: already current; nothing written", path.display())
        }
        WriteOutcome::WouldWrite(path) => {
            format!("{}: would write; nothing changed", path.display())
        }
        WriteOutcome::Wrote { path, backup } => {
            if backup {
                format!(
                    "{}: wrote managed Estelle block; original copied to .bak",
                    path.display()
                )
            } else {
                format!("{}: created with managed Estelle block", path.display())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_bytes_survive_and_the_transform_is_a_fixed_point() {
        let customer = "# Customer rules\n\nNever force-push.  \n";
        let once = apply(Some(customer)).expect("first transform");
        assert!(once.contains(customer));
        let twice = apply(Some(&once)).expect("second transform");
        assert_eq!(twice, once);
        assert_eq!(twice.matches(BEGIN).count(), 1);
        assert_eq!(twice.matches(TAIL_BEGIN).count(), 1);
    }

    #[test]
    fn malformed_managed_regions_refuse_instead_of_guessing() {
        let malformed = format!("{BEGIN}\nunfinished\n# customer content\n");
        let error = apply(Some(&malformed)).expect_err("open region must refuse");
        assert!(error.contains("without its closing marker"));
        assert!(malformed.contains("# customer content"));
    }

    #[test]
    fn a_second_filesystem_run_writes_nothing() {
        let root = tempfile::tempdir().expect("root");
        let file = Path::new("CLAUDE.md");
        fs::write(root.path().join(file), "# Mine\n").expect("customer file");
        assert!(matches!(
            write(root.path(), file, false, false).expect("first"),
            WriteOutcome::Wrote { backup: true, .. }
        ));
        let after_first = fs::read(root.path().join(file)).expect("first bytes");
        assert!(matches!(
            write(root.path(), file, false, false).expect("second"),
            WriteOutcome::Unchanged(_)
        ));
        assert_eq!(
            fs::read(root.path().join(file)).expect("second bytes"),
            after_first
        );
    }

    #[test]
    fn explicit_creation_and_repository_confinement_are_separate() {
        let root = tempfile::tempdir().expect("root");
        assert!(matches!(
            write(root.path(), Path::new("AGENTS.md"), false, false).expect("skip"),
            WriteOutcome::Skipped(_)
        ));
        assert!(matches!(
            write(root.path(), Path::new("AGENTS.md"), true, false).expect("create"),
            WriteOutcome::Wrote { backup: false, .. }
        ));
        assert!(write(root.path(), Path::new("../CLAUDE.md"), true, false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_targets_are_refused_even_when_they_point_inside_the_repo() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("real.md"), "# Customer\n").expect("real file");
        symlink("real.md", root.path().join("CLAUDE.md")).expect("symlink");
        let error = write(root.path(), Path::new("CLAUDE.md"), false, false)
            .expect_err("symlink must refuse");
        assert!(error.contains("symlinked"));
        assert_eq!(
            fs::read_to_string(root.path().join("real.md")).expect("customer bytes"),
            "# Customer\n"
        );
    }

    #[test]
    fn an_install_followed_by_an_uninstall_returns_the_exact_customer_bytes() {
        for customer in [
            "# Customer rules\n\nNever force-push.\n",
            "# Mine\n",
            "a single line and nothing else\n",
            "# Head\n\n\n\nfour blank lines the customer chose\n",
        ] {
            let installed = apply(Some(customer)).expect("install");
            assert!(installed.contains(BEGIN), "install must write the block");
            let stripped = strip(&installed)
                .expect("uninstall")
                .expect("the block was installed, so strip must find it");
            assert_eq!(
                stripped, customer,
                "uninstall must return the file byte-for-byte"
            );
        }
    }

    #[test]
    fn uninstall_leaves_no_marker_and_is_idempotent() {
        let customer = "# Customer\n";
        let installed = apply(Some(customer)).expect("install");
        let stripped = strip(&installed).expect("uninstall").expect("installed");
        for marker in [BEGIN, END, TAIL_BEGIN, TAIL_END] {
            assert!(!stripped.contains(marker), "marker survived: {marker}");
        }
        assert_eq!(
            strip(&stripped).expect("second uninstall"),
            None,
            "a file with no block must report nothing to remove, not remove something"
        );
    }

    #[test]
    fn a_file_estelle_created_is_deleted_rather_than_left_as_an_empty_husk() {
        let root = tempfile::tempdir().expect("root");
        let file = Path::new("AGENTS.md");
        assert!(matches!(
            write(root.path(), file, true, false).expect("create"),
            WriteOutcome::Wrote { backup: false, .. }
        ));
        assert!(root.path().join(file).is_file());
        assert!(matches!(
            remove_at(&root.path().join(file), None, false).expect("remove"),
            RemoveOutcome::Removed { deleted: true, .. }
        ));
        assert!(
            !root.path().join(file).exists(),
            "a file holding only Estelle's block must not survive uninstall"
        );
        assert!(
            !root.path().join("AGENTS.md.bak").exists(),
            "a backup of Estelle's own bytes is litter, not a safety net: there is nothing of \
             the customer's in a file we created and are now deleting"
        );
    }

    #[test]
    fn uninstall_keeps_a_file_the_customer_also_wrote_in() {
        let root = tempfile::tempdir().expect("root");
        let file = Path::new("CLAUDE.md");
        let customer = "# House rules\n\nNo force pushes.\n";
        fs::write(root.path().join(file), customer).expect("customer file");
        write(root.path(), file, false, false).expect("install");
        assert!(matches!(
            remove_at(&root.path().join(file), None, false).expect("remove"),
            RemoveOutcome::Removed { deleted: false, .. }
        ));
        assert_eq!(
            fs::read_to_string(root.path().join(file)).expect("kept bytes"),
            customer
        );
    }

    #[test]
    fn uninstall_reports_a_file_it_never_touched_instead_of_rewriting_it() {
        let root = tempfile::tempdir().expect("root");
        let file = Path::new("CLAUDE.md");
        let customer = "# House rules\n";
        fs::write(root.path().join(file), customer).expect("customer file");
        assert!(matches!(
            remove_at(&root.path().join(file), None, false).expect("remove"),
            RemoveOutcome::NotInstalled(_)
        ));
        assert_eq!(
            fs::read_to_string(root.path().join(file)).expect("untouched bytes"),
            customer
        );
        assert!(
            !root.path().join("CLAUDE.md.bak").exists(),
            "a no-op uninstall must not leave a backup behind"
        );
    }

    #[test]
    fn a_half_open_managed_region_refuses_instead_of_eating_customer_bytes() {
        let malformed = format!("{BEGIN}\nunfinished\n# customer content\n");
        let error = strip(&malformed).expect_err("open region must refuse");
        assert!(error.contains("without its closing marker"));
    }

    #[test]
    fn the_managed_rule_names_the_tools_it_requires() {
        let block = render_block();
        for tool in [
            "estelle_resume",
            "verify",
            "find_definition",
            "gate",
            "review",
            "research_drift",
        ] {
            assert!(block.contains(tool), "missing {tool}");
        }
    }
}
