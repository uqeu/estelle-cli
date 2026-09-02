//! `estelle leaked` — the offline self-audit: scan the customer's OWN agent configuration
//! directories (`~/.claude`, `~/.codex`) for credentials, using the shared secret engine. No
//! network, no account, no telemetry. The report names the rule, the 12-hex fingerprint, the
//! path and the line — NEVER the value.

use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

/// Files larger than this are skipped (counted, not scanned) — a secret in an 8MB+ blob is the
/// sweep's job, not the config-audit wedge's.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Directory recursion cap: agent config trees are shallow; anything deeper is a cache, not a
/// config.
const MAX_DEPTH: usize = 12;

/// Directory names that are dependency trees, VCS data, or caches — never the customer's own
/// configuration, and (measured on a real home) the overwhelming majority of the bytes under
/// ~/.claude + ~/.codex. Skipping them is what turns the audit from minutes into seconds.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "site-packages",
    "target",
    ".cache",
    "cache",
];

pub(crate) struct LeakedFinding {
    pub path: PathBuf,
    pub rule: String,
    pub fingerprint: String,
    pub line: usize,
}

pub(crate) struct LeakedReport {
    pub roots: Vec<PathBuf>,
    pub files_scanned: usize,
    pub files_skipped: usize,
    pub findings: Vec<LeakedFinding>,
}

/// The customer's own agent config directories. A missing home yields an empty list; missing
/// directories are handled by the scan itself (they count as nothing, not as an error).
pub(crate) fn default_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![home.join(".claude"), home.join(".codex")]
}

pub(crate) fn scan_roots(roots: &[PathBuf]) -> LeakedReport {
    let mut files = Vec::new();
    for root in roots {
        collect_files(root, 0, &mut files);
    }
    files.sort();

    // Files are scanned in parallel over the shared (thread-safe) engine; the round-robin
    // assignment keeps one batch from inheriting all the multi-MB transcripts, and per-file
    // results are merged back in sorted order so the report is deterministic.
    let threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(4)
        .min(16);
    let batches: Vec<Vec<&PathBuf>> = {
        let mut batches: Vec<Vec<&PathBuf>> = (0..threads).map(|_| Vec::new()).collect();
        for (index, path) in files.iter().enumerate() {
            batches[index % threads].push(path);
        }
        batches
    };
    let scanned: Vec<(usize, usize, Vec<LeakedFinding>)> = std::thread::scope(|scope| {
        // Spawn every batch BEFORE joining any — a spawn/join interleave would serialise.
        let mut handles = Vec::with_capacity(batches.len());
        for batch in &batches {
            handles.push(scope.spawn(|| scan_files(batch)));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or((0, 0, Vec::new())))
            .collect()
    });
    let mut report = LeakedReport {
        roots: roots.to_vec(),
        files_scanned: 0,
        files_skipped: 0,
        findings: Vec::new(),
    };
    for (scanned_count, skipped_count, mut findings) in scanned {
        report.files_scanned += scanned_count;
        report.files_skipped += skipped_count;
        report.findings.append(&mut findings);
    }
    report
        .findings
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    report
}

/// One batch of files, returning (scanned, skipped, findings).
fn scan_files(files: &[&PathBuf]) -> (usize, usize, Vec<LeakedFinding>) {
    let mut scanned = 0;
    let mut skipped = 0;
    let mut findings = Vec::new();
    for path in files {
        let Ok(metadata) = path.metadata() else {
            skipped += 1;
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            skipped += 1;
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            skipped += 1;
            continue;
        };
        // A NUL in the head marks a binary; lossy-decoding binaries only buys noise.
        if bytes.iter().take(8192).any(|byte| *byte == 0) {
            skipped += 1;
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        scanned += 1;
        for finding in estelle_client::find_secret_shapes(&text) {
            findings.push(LeakedFinding {
                path: (*path).clone(),
                rule: finding.rule.to_string(),
                fingerprint: finding.fingerprint.clone(),
                line: finding.line,
            });
        }
    }
    (scanned, skipped, findings)
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // a missing or unreadable directory is empty, never an error
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Symlinks are not followed: the audit covers the customer's own config trees, not
        // wherever a link might point.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let skip = entry
                .file_name()
                .to_str()
                .is_some_and(|name| SKIP_DIRS.contains(&name));
            if !skip {
                collect_files(&path, depth + 1, out);
            }
        } else if file_type.is_file() {
            out.push(path);
        }
    }
}

pub(crate) fn render(report: &LeakedReport) -> Vec<String> {
    let mut lines: Vec<String> = report
        .findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{}: {} (fingerprint {})",
                finding.path.display(),
                finding.line,
                finding.rule,
                finding.fingerprint
            )
        })
        .collect();
    lines.push(summary_line(report));
    lines
}

/// The one-liner — screenshot-able either way.
fn summary_line(report: &LeakedReport) -> String {
    let roots = if report.roots.is_empty() {
        "the agent config directories".to_string()
    } else {
        report
            .roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    let files_with_findings = {
        let mut seen: Vec<&Path> = report
            .findings
            .iter()
            .map(|finding| finding.path.as_path())
            .collect();
        seen.sort();
        seen.dedup();
        seen.len()
    };
    if report.findings.is_empty() {
        format!(
            "estelle leaked: {} files scanned under {roots} — no exposed credentials found.",
            report.files_scanned
        )
    } else {
        format!(
            "estelle leaked: {} exposed credential(s) in {files_with_findings} file(s) under {roots} ({} files scanned; values never printed — rule + fingerprint only). Rotate them.",
            report.findings.len(),
            report.files_scanned
        )
    }
}

pub(crate) fn run() -> ExitCode {
    let report = scan_roots(&default_roots());
    let lines = render(&report);
    let mut stdout = std::io::stdout().lock();
    let written = stdout
        .write_all(format!("{}\n", lines.join("\n")).as_bytes())
        .is_ok();
    match (written, report.findings.is_empty()) {
        // Like doctor: the exit code carries the verdict, so CI and scripts can gate on it.
        (true, true) => ExitCode::SUCCESS,
        (true, false) => ExitCode::FAILURE,
        (false, _) => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every fixture here is an invented string, shaped like a credential; none is real. It is
    // ASSEMBLED rather than a literal because a verbatim scanner-shaped token in source trips
    // GitHub push protection (the Python port derives its fixtures for the same reason).
    fn planted_slack() -> String {
        format!(
            "xoxb-{}-{}-{}",
            "123456789012", "123456789012", "AbCdEfGhIjKlMnOpQrStUvWx"
        )
    }

    #[test]
    fn a_planted_invented_credential_is_found_with_path_rule_and_line() {
        let planted = planted_slack();
        let root = tempfile::tempdir().expect("tempdir");
        let claude = root.path().join(".claude");
        std::fs::create_dir_all(&claude).expect("mkdir");
        let settings = claude.join("settings.json");
        std::fs::write(&settings, format!("{{\n  \"token\": \"{planted}\"\n}}\n"))
            .expect("plant fixture");

        let report = scan_roots(&[root.path().to_path_buf()]);
        assert_eq!(report.files_scanned, 1);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.rule == "slack-bot-token")
            .expect("the planted credential must be found");
        assert_eq!(finding.path, settings);
        assert_eq!(finding.line, 2);

        let rendered = render(&report).join("\n");
        assert!(rendered.contains("slack-bot-token"));
        assert!(rendered.contains(&finding.fingerprint));
        assert!(rendered.contains("settings.json:2"));
        // Both slack rules (strict + Estelle-local loose) fire on the one planted value — pin
        // the FILE count, not the finding count.
        assert!(rendered.contains("in 1 file(s)"));
        // The one rule above all: the value NEVER appears in the report.
        assert!(!rendered.contains(&planted));
    }

    #[test]
    fn an_empty_tree_and_a_missing_tree_both_report_clean() {
        let root = tempfile::tempdir().expect("tempdir");
        let report = scan_roots(&[
            root.path().to_path_buf(),
            root.path().join("does-not-exist"),
        ]);
        assert!(report.findings.is_empty());
        assert_eq!(report.files_scanned, 0);
        let rendered = render(&report).join("\n");
        assert!(
            rendered.contains("no exposed credentials found"),
            "clean summary expected: {rendered}"
        );
    }

    #[test]
    fn mutation_without_the_engine_the_planted_credential_slips_through() {
        // Prove the scan is the engine and not theatre: the same planted file, scanned with the
        // slack rules deleted, must come back clean.
        let root = tempfile::tempdir().expect("tempdir");
        let settings = root.path().join("settings.json");
        std::fs::write(&settings, format!("\"token\": \"{}\"\n", planted_slack()))
            .expect("plant fixture");
        let text = std::fs::read_to_string(&settings).expect("read back");
        let rules: Vec<estelle_client::secret_engine::SecretRule> =
            estelle_client::secret_engine::load_rules()
                .into_iter()
                .filter(|rule| !rule.id.starts_with("slack"))
                .collect();
        let crippled = estelle_client::secret_engine::SecretEngine::new(rules);
        assert!(crippled.find_secrets(&text).is_empty());
        // Positive control in the same test: the full engine DOES catch it.
        assert!(
            estelle_client::find_secret_shapes(&text)
                .iter()
                .any(|finding| finding.rule == "slack-bot-token")
        );
    }
}
