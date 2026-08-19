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

fn target(root: &Path, file: &Path) -> Result<PathBuf, String> {
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
    let path = target(root, file)?;
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
    let rendered = apply(existing.as_deref())?;
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
