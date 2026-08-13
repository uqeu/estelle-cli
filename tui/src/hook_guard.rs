//! The Bash guard — the classic shell foot-guns, ported pattern-for-pattern from the JS hook
//! (cli/bin/hook.js). Conservative by design: it flags shapes that are almost always a mistake
//! to run blind, and stays silent on ordinary work, because a guard that cries wolf gets muted
//! within a day. Advisory only — the caller warns, it never blocks.

use std::sync::LazyLock;

use regex_lite::Regex;

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid guard pattern {pattern:?}: {error}"),
    }
}

/// The classic destructive shapes. Only a recursive-force rm whose TARGET is genuinely
/// catastrophic — root, home, a wildcard at root, or a system directory. A plain
/// `rm -rf /tmp/x` or `rm -rf ~/Downloads/build` is ordinary cleanup and must NOT fire, or the
/// guard gets muted and then misses the real `rm -rf /`. A DEEP path under /Users or /home is
/// normal work; the bare roots are not. /private and /tmp are excluded — that is where scratch
/// lives.
static DANGER: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            compile(
                r"\brm\s+(?:-\S+\s+)*(-[a-z]*r[a-z]*f[a-z]*|-[a-z]*f[a-z]*r[a-z]*)\s+(?:-\S+\s+)*(/(\s|\*|$)|~/?(\s|$)|\$HOME/?(\s|$)|/(Users|home)(\s|\*|$)|/(etc|usr|var|bin|lib|sbin|boot|opt|root|dev|sys|proc|System|Library)(/\S*)?(\s|$))",
            ),
            "recursive force-delete of a critical path",
        ),
        (
            compile(r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:"),
            "a fork bomb",
        ),
        (
            compile(r"\bcurl\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b"),
            "piping a download straight into a shell",
        ),
        (
            compile(r"\bwget\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b"),
            "piping a download straight into a shell",
        ),
        (
            compile(r"\b(dd|mkfs\.\w+)\b.*\bof=/dev/(disk|sd|nvme)"),
            "writing directly to a disk device",
        ),
        (
            compile(r">\s*/dev/(disk|sd|nvme)\w*"),
            "overwriting a disk device",
        ),
        (
            compile(r"\bgit\s+push\b.*--force\b.*\b(origin\s+)?(main|master)\b"),
            "a force-push to the main branch",
        ),
        (
            compile(r"\bchmod\s+-R\s+777\s+/"),
            "making a broad path world-writable",
        ),
    ]
});

/// The reason a command looks dangerous, or `None` when it doesn't. Pure, conservative, errs
/// toward silence.
pub fn dangerous_command(command: &str) -> Option<&'static str> {
    DANGER
        .iter()
        .find(|(pattern, _)| pattern.is_match(command))
        .map(|(_, reason)| *reason)
}
