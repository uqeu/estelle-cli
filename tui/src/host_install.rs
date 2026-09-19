//! One table of every coding host Estelle installs itself into, and the two verbs that act on it.
//!
//! **The mechanism split is the whole job.** A host is reached in one of two ways and they are not
//! interchangeable: a *hook platform* runs a command before a tool call, and an *instruction-file
//! platform* reads a persistent file every session. Handing a hook to a host with no hook runtime
//! installs nothing; handing an instruction file to a host that ignores it installs nothing. Both
//! failures are silent, which is why the shape is a field in this table rather than a comment.
//!
//! Before 2026-09-19 there was no per-host install verb at all. `brief` refreshed whatever agent
//! instruction file *already existed* in the repository ([`agent_brief::detected`] filters on
//! `is_file()`), so a host whose file the customer had never created was never reached, and no
//! verb could reach it on request. Uninstall was worse: [`super::remove_editor_configs`] clears
//! MCP entries and `uninstall_hooks` clears hook settings, and **neither has ever removed a
//! managed instruction block**. Sixteen file shapes could be written and none could be taken back.
//!
//! ## What a row may claim
//!
//! A path in this table is a path a customer's editor config will be written to, so **no path is
//! here that was not read out of that host's own documentation**. A host whose path could not be
//! established from a primary source is absent from this table and named in the release notes
//! instead — a wrong path corrupts somebody's editor config and is strictly worse than an absent
//! feature.

use std::path::Path;
use std::path::PathBuf;

use crate::agent_brief;

#[path = "host_table.rs"]
mod host_table;
pub(crate) use host_table::HOSTS;

/// Which of the two integration mechanisms reaches this host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape {
    /// A persistent instruction file the host loads into every session.
    Instructions,
    /// A command the host runs before a tool call. Installed by `estelle install-hooks`, which
    /// owns the hook payload; this table only records that the host is reached that way.
    Hook,
    /// A native skill directory the host loads before the user has chosen a repository.
    ///
    /// **This is the mechanism that decides whether a tool is present or invisible.** An
    /// instruction file lives inside a project, so it reaches nobody until they open that
    /// project; a user-profile skill is loaded by the host itself, on every session, at zero
    /// tokens per turn. Measured 2026-09-19: `~/.claude/skills/` held a 52,595-byte competitor
    /// skill and ZERO Estelle files.
    Skill,
}

impl Shape {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Instructions => "instruction file",
            Self::Hook => "hook",
            Self::Skill => "skill",
        }
    }
}

/// Where the host keeps its configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Scope {
    /// Inside the repository, shared with everyone who clones it.
    Project,
    /// In the user's home directory, applying to every repository they open.
    User,
}

impl Scope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

/// The YAML frontmatter a Cursor project rule needs to be applied at all.
///
/// Cursor's rule loader reads `alwaysApply` out of the frontmatter; a `.mdc` file with no
/// frontmatter is not an always-on rule. Estelle has shipped `.cursor/rules/estelle.mdc` since the
/// `AGENT_FILES` table was written and has never written frontmatter into it.
/// The front matter a `SKILL.md` needs to be discovered.
///
/// `name` and `description` are the two required keys in the SKILL.md format (agentskills.io,
/// and the same two are required by Claude Code and by Goose). The description is what a host
/// matches a user's request against, so it names the moments Estelle is for rather than
/// describing the product.
pub(crate) const SKILL_FRONTMATTER: &str = concat!(
    "---\n",
    "name: estelle\n",
    "description: Ground any claim about this codebase in the real indexed repository before ",
    "stating it, and gate any diff before proposing it. Use when about to assert that a symbol, ",
    "function, class or signature exists; when proposing a patch or merge; when asked who calls ",
    "something or what breaks if it changes; when resuming work the team already decided; or ",
    "when a dependency may have drifted.\n",
    "---\n",
);

/// The YAML front matter a Kiro steering file needs to be loaded into every session.
///
/// Kiro's steering docs define `inclusion: always` (the default), `fileMatch` and `manual`. The
/// default is stated as `always`, but it is written out here rather than relied on: a default is a
/// fact about today's Kiro, and a file that says what it means keeps meaning it.
/// Source: https://kiro.dev/docs/steering/
pub(crate) const KIRO_STEERING_FRONTMATTER: &str = "---\ninclusion: always\n---\n";

/// The front matter a Devin granular rule needs to be always-on.
///
/// Devin's `.devin/rules/*.md` files take `trigger: always_on`; its plain `AGENTS.md` needs no
/// front matter and is always on by default.
/// Source: https://docs.devin.ai/cli/extensibility/rules.md
pub(crate) const DEVIN_RULE_FRONTMATTER: &str = "---\ntrigger: always_on\n---\n";

pub(crate) const CURSOR_MDC_FRONTMATTER: &str = "---\ndescription: Estelle — ground every claim about this repository before stating it.\nalwaysApply: true\n---\n";

/// One host: the name a customer types, and the two paths that reach it.
///
/// `project` and `user` are both `Option` because the two scopes are genuinely independent — a
/// host can read a repository file and no home file (`.github/copilot-instructions.md`), or the
/// reverse. A `None` here means *this host has no such scope*, never *we did not look*.
#[derive(Debug)]
pub(crate) struct Host {
    /// The token a customer types after `--host`.
    pub(crate) name: &'static str,
    /// The product's own name, for the report line.
    pub(crate) label: &'static str,
    /// Every mechanism that reaches this host.
    ///
    /// A host can be reached BOTH ways — Claude Code and Codex run hooks *and* read an
    /// instruction file — so this is a list, not a single value. A one-value field would have
    /// forced a lie on the two hosts where both mechanisms are shipped, and "which mechanism"
    /// is precisely the question that decides whether an install lands at all.
    pub(crate) shapes: &'static [Shape],
    /// Repository-relative path a NEW install is written to.
    pub(crate) project: Option<&'static str>,
    /// Home-relative path.
    pub(crate) user: Option<&'static str>,
    /// Older or alternate repository filenames the same host also reads.
    ///
    /// These are **refreshed and uninstalled but never created**: a customer who already keeps
    /// `.cursorrules` should have it kept current and should get it cleaned up, and a customer
    /// who does not should not suddenly acquire a deprecated file. Without this column
    /// [`agent_brief::AGENT_FILES`] and this table would be two owners of "which files carry the
    /// block" that disagree — and the half they disagreed about is the half `uninstall` would
    /// silently leave behind. `every_written_file_can_also_be_uninstalled` is the guard.
    pub(crate) legacy: &'static [&'static str],
    /// A home-relative directory whose existence means this host is installed on the machine.
    ///
    /// This is what lets a bare `estelle install` CREATE user-scope files for the assistants the
    /// customer actually runs, without scattering files for a dozen they do not. `None` means we
    /// have no reliable presence signal for this host, so a bare install only refreshes what is
    /// already there and reaching it needs `--host <name>`.
    pub(crate) detect: Option<&'static str>,
    /// The host's native skill path, relative to the repository root or the home directory.
    ///
    /// Deliberately one string for both scopes: every host that has skills reads them from the
    /// same relative location under either root, so a second column would be two owners of one
    /// fact. `None` means this host has no skill directory we could establish from its own
    /// documentation — never that we did not look.
    pub(crate) skill: Option<&'static str>,
    /// Format bytes this host requires around the managed block.
    pub(crate) preamble: Option<&'static str>,
    /// What a reader needs to know about this row that the paths do not say.
    pub(crate) note: &'static str,
}

impl Host {
    /// "instruction file" / "instruction file + hook" — what a customer needs to know about how
    /// this host is reached.
    pub(crate) fn shape_labels(&self) -> String {
        self.shapes
            .iter()
            .map(|shape| shape.label())
            .collect::<Vec<_>>()
            .join(" + ")
    }

    /// The relative path this host reads at one scope, or `None` when the host has no such scope.
    pub(crate) fn path_for(&self, scope: Scope) -> Option<&'static str> {
        match scope {
            Scope::Project => self.project,
            Scope::User => self.user,
        }
    }
}

/// The host a customer named, or a refusal that lists what they could have named.
pub(crate) fn lookup(name: &str) -> Result<&'static Host, String> {
    HOSTS
        .iter()
        .find(|host| host.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            format!(
                "unknown host {name}; Estelle installs into {}",
                HOSTS
                    .iter()
                    .map(|host| host.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// One line per host, for `estelle install --list`.
///
/// This exists because "which assistants does it support" is the question a customer asks before
/// they install anything, and until now the only honest answer was to read the source.
pub(crate) fn listing() -> Vec<String> {
    let mut lines = vec![format!(
        "Estelle installs into {} hosts. --project writes into this repository, --user into your home directory.",
        HOSTS.len()
    )];
    lines.extend(HOSTS.iter().map(|host| {
        let project = host.project.unwrap_or("—");
        let user = host
            .user
            .map(|user| format!("~/{user}"))
            .unwrap_or_else(|| "—".to_string());
        format!(
            "{:<12} {:<24} {:<26} project: {:<32} user: {:<28} {}",
            host.name,
            host.label,
            host.shape_labels(),
            project,
            user,
            host.note
        )
    }));
    lines
}

/// The home directory this run writes user-scope files into.
///
/// Passed in rather than read from the environment at each use, for two reasons. It makes the
/// user scope testable against a temporary directory without mutating `$HOME` — and a test that
/// mutates a process-global to reach the code path it is checking is a test that races every
/// other test in the binary. And it gives the lookup one owner: `dirs::home_dir()` is called once,
/// at the command boundary, instead of once per host per scope.
pub(crate) fn current_home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// The home directory, or a refusal that names the scope that needed it.
///
/// Absent is only fatal for a USER-scope write. A `--project` install on a machine with no
/// resolvable home must still work: refusing it would be a refusal caused by a fact that has
/// nothing to do with what was asked for.
fn need_home(home: Option<&Path>) -> Result<&Path, String> {
    home.ok_or_else(|| {
        "could not locate the home directory, which a user-scope install needs; \
         --project does not"
            .to_string()
    })
}

/// Resolve one host+scope to an absolute path.
///
/// The home directory is looked up ONLY for a user-scope row. Resolving it eagerly would make a
/// `--project` install fail on a machine with no resolvable home — a refusal caused by a fact that
/// has nothing to do with what was asked for.
fn resolve_for(
    host: &Host,
    scope: Scope,
    root: &Path,
    home: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    Ok(match scope {
        Scope::Project => match host.path_for(scope) {
            Some(project) => Some(agent_brief::confine(root, Path::new(project))?),
            None => None,
        },
        Scope::User => match host.path_for(scope) {
            Some(user) => Some(need_home(home)?.join(user)),
            None => None,
        },
    })
}

/// Install into one host at one scope.
///
/// `create` is what separates "refresh what is already here" from "reach a host for the first
/// time". `estelle brief` has only ever done the former; naming a host is an explicit request for
/// the latter, so a named host creates and an unnamed sweep does not.
pub(crate) fn install_one(
    host: &Host,
    scope: Scope,
    root: &Path,
    home: Option<&Path>,
    create: bool,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    if let Some(path) = resolve_for(host, scope, root, home)? {
        let outcome = agent_brief::write_at(&path, host.preamble, create, dry_run)?;
        lines.push(format!(
            "{}: {}",
            host.label,
            agent_brief::outcome_line(outcome)
        ));
    }
    // A legacy filename is REFRESHED, never created: `create` is deliberately false here even
    // when the customer named this host. Creating `.cursorrules` for somebody who does not have
    // one would hand them a deprecated file they never asked for.
    if scope == Scope::Project {
        for legacy in host.legacy {
            let path = agent_brief::confine(root, Path::new(legacy))?;
            let outcome = agent_brief::write_at(&path, None, false, dry_run)?;
            if !matches!(outcome, agent_brief::WriteOutcome::Skipped(_)) {
                lines.push(format!(
                    "{}: {}",
                    host.label,
                    agent_brief::outcome_line(outcome)
                ));
            }
        }
    }
    if let Some(path) = skill_path(host, scope, root, home)? {
        let outcome = agent_brief::write_at(&path, Some(SKILL_FRONTMATTER), create, dry_run)?;
        lines.push(format!(
            "{} skill: {}",
            host.label,
            agent_brief::outcome_line(outcome)
        ));
    }
    Ok(lines)
}

/// Where this host's `SKILL.md` goes, or `None` when the host has no skill directory.
///
/// The same relative path under either root — see [`Host::skill`].
fn skill_path(
    host: &Host,
    scope: Scope,
    root: &Path,
    home: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(skill) = host.skill else {
        return Ok(None);
    };
    Ok(Some(match scope {
        Scope::Project => agent_brief::confine(root, Path::new(skill))?,
        Scope::User => need_home(home)?.join(skill),
    }))
}

/// Uninstall from one host at one scope, legacy filenames included.
///
/// The legacy sweep is the half that matters: a block written into `.cursorrules` by `brief` and
/// left there by an uninstall that only knew about `.cursor/rules/estelle.mdc` is a file we put in
/// a customer's repository and cannot take out.
pub(crate) fn uninstall_one(
    host: &Host,
    scope: Scope,
    root: &Path,
    home: Option<&Path>,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    if let Some(path) = resolve_for(host, scope, root, home)? {
        let outcome = agent_brief::remove_at(&path, host.preamble, dry_run)?;
        lines.push(format!(
            "{}: {}",
            host.label,
            agent_brief::remove_line(outcome)
        ));
    }
    if scope == Scope::Project {
        for legacy in host.legacy {
            let path = agent_brief::confine(root, Path::new(legacy))?;
            let outcome = agent_brief::remove_at(&path, None, dry_run)?;
            lines.push(format!(
                "{}: {}",
                host.label,
                agent_brief::remove_line(outcome)
            ));
        }
    }
    if let Some(path) = skill_path(host, scope, root, home)? {
        let outcome = agent_brief::remove_at(&path, Some(SKILL_FRONTMATTER), dry_run)?;
        let emptied = matches!(
            outcome,
            agent_brief::RemoveOutcome::Removed { deleted: true, .. }
        );
        lines.push(format!(
            "{} skill: {}",
            host.label,
            agent_brief::remove_line(outcome)
        ));
        // A skill is a DIRECTORY named after us, so deleting only the file leaves an empty
        // `estelle/` behind — our litter, invisible to the customer who asked us to leave.
        // `remove_dir` refuses a non-empty directory, so a customer who put something of their
        // own beside our SKILL.md keeps it and keeps the folder.
        if emptied && let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    Ok(lines)
}

/// Every scope a sweep visits. Named rather than inlined so a caller cannot visit one and believe
/// it visited both — the defect this repository has paid for most often is a check that covers one
/// half of a contract and reports on the whole.
const EVERY_SCOPE: &[Scope] = &[Scope::Project, Scope::User];

fn scopes(scope: Option<Scope>) -> &'static [Scope] {
    match scope {
        Some(Scope::Project) => &[Scope::Project],
        Some(Scope::User) => &[Scope::User],
        None => EVERY_SCOPE,
    }
}

/// `estelle install`.
///
/// With `--host`, reach exactly that host and create its file. Without, refresh every host file
/// that already exists — the behaviour `brief` had, kept so an upgrade does not suddenly scatter
/// files into a repository nobody asked us to write to.
pub(crate) fn install(
    root: &Path,
    home: Option<&Path>,
    named: &[String],
    scope: Option<Scope>,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let selected = select(named)?;
    let named_explicitly = !named.is_empty();
    let mut lines = Vec::new();
    for host in selected {
        for pass in scopes(scope) {
            // No home means no user scope. An explicit `--user` gets a refusal that names the
            // reason; an unnamed sweep quietly does the project half, because failing a whole
            // command over a scope nobody asked for is a refusal about the wrong thing.
            if *pass == Scope::User && home.is_none() {
                if scope.is_some() {
                    need_home(home)?;
                }
                continue;
            }
            // What "install" means depends on how much we were told, and the three cases are
            // deliberately different:
            //
            // - a host named on the command line is CREATED, at either scope. The customer said
            //   which assistant they use.
            // - an unnamed user-scope install is CREATED only for a host this machine actually
            //   runs, proven by its own config directory existing. This is the case that matters:
            //   a user-profile install is what makes Estelle present before a repository is
            //   opened, and it is the only one a competitor with 119k stars has that we do not.
            // - an unnamed project-scope install only REFRESHES, never creates. Scattering
            //   instruction files into somebody's repository because they ran one command is not
            //   an install, it is a mess.
            let create = named_explicitly || (*pass == Scope::User && detected(host, home)?);
            for line in install_one(host, *pass, root, home, create, dry_run)? {
                if line.contains("absent; nothing written") {
                    continue;
                }
                lines.push(format!("[{}] {line}", pass.label()));
            }
        }
    }
    if lines.is_empty() {
        lines.push(
            "No coding assistant was detected and no instruction file exists yet; nothing was \
             written. Name a host with `estelle install --host claude`, or list them with \
             `estelle install --list`."
                .to_string(),
        );
    }
    Ok(lines)
}

/// Is this host installed on this machine?
///
/// Presence is asserted from the host's OWN config directory, never from ours: a directory we
/// created would make every host look present the moment we wrote to it, and the check would
/// then be reporting on itself.
fn detected(host: &Host, home: Option<&Path>) -> Result<bool, String> {
    let Some(marker) = host.detect else {
        return Ok(false);
    };
    Ok(need_home(home)?.join(marker).is_dir())
}

/// `estelle uninstall` — clears every host at once unless one is named, because a customer who
/// wants Estelle out wants it out of all of them, and a tool that can only be removed from the one
/// host you remember is not removable.
pub(crate) fn uninstall(
    root: &Path,
    home: Option<&Path>,
    named: &[String],
    scope: Option<Scope>,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let selected = select(named)?;
    let mut lines = Vec::new();
    let mut changed = false;
    for host in selected {
        for pass in scopes(scope) {
            // No home means no user scope. An explicit `--user` gets a refusal that names the
            // reason; an unnamed sweep quietly does the project half, because failing a whole
            // command over a scope nobody asked for is a refusal about the wrong thing.
            if *pass == Scope::User && home.is_none() {
                if scope.is_some() {
                    need_home(home)?;
                }
                continue;
            }
            for line in uninstall_one(host, *pass, root, home, dry_run)? {
                // Only a host we actually touched earns a line; otherwise the report is a wall of
                // "absent" and the one real removal is invisible in it.
                if line.contains("absent") || line.contains("no Estelle block") {
                    continue;
                }
                changed = true;
                lines.push(format!("[{}] {line}", pass.label()));
            }
        }
    }
    if !changed {
        lines.push("No Estelle instruction block was installed; nothing changed.".to_string());
    }
    lines.push(
        "MCP entries are removed by `estelle remove`, and hooks by `estelle uninstall-hooks`."
            .to_string(),
    );
    Ok(lines)
}

fn select(named: &[String]) -> Result<Vec<&'static Host>, String> {
    if named.is_empty() {
        return Ok(HOSTS.iter().collect());
    }
    named.iter().map(|name| lookup(name)).collect()
}

#[cfg(test)]
#[path = "host_install_tests.rs"]
mod tests;
