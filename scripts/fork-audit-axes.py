#!/usr/bin/env python3
"""Mechanical axis scan of ADDED lines in `git diff <audited> -- <path>`.

Counts are LEXICAL and deliberately over-inclusive: every hit must be read by a human
before it is reported. A zero is meaningful; a non-zero is a reading assignment.
"""
import re
import subprocess
import sys

AUD = "3ea4936a74e9345e3f1f8331bdb012a4688088bc"

AXES = {
    "url_host": re.compile(
        r"https?://|\b[a-z0-9][a-z0-9-]*\.(?:com|net|org|io|ca|sh|dev|invalid|example|local|ai|app)\b"
    ),
    "network": re.compile(
        r"\breqwest\b|\bhyper\b|\bureq\b|TcpStream|TcpListener|UdpSocket|"
        r"\bpost_scoped\b|\bget_scoped\b|Endpoint::|\bapi\.(?:get|post|put|delete)\b|"
        r"\bClient::new\b|\.send\(\)\s*\.await|\bfetch\b\("
    ),
    "process": re.compile(r"Command::new|std::process|\.spawn\(|exec\(|execvp|posix_spawn"),
    "fs_write": re.compile(
        r"fs::write|fs::create_dir|fs::remove|fs::rename|fs::copy|File::create|"
        r"OpenOptions|write_all|create_dir_all|remove_file|remove_dir|set_permissions|"
        r"\btempfile\b|persist\("
    ),
    "env_read": re.compile(r"env::var|std::env|env!\(|\bgetenv\b|option_env!"),
    "cred_read": re.compile(
        r"(?i)\bapi[_-]?key\b|\bsecret\b|\btoken\b|\bpassword\b|\bcredential\b|"
        r"\bbearer\b|\bauth(?:orization)?\b|keyring|\.env\b"
    ),
}


def added_lines(path):
    out = subprocess.run(
        ["git", "diff", "--no-color", AUD, "--", path],
        capture_output=True, text=True, check=True,
    ).stdout
    return [l[1:] for l in out.splitlines() if l.startswith("+") and not l.startswith("+++")]


def removed_lines(path):
    out = subprocess.run(
        ["git", "diff", "--no-color", AUD, "--", path],
        capture_output=True, text=True, check=True,
    ).stdout
    return [l[1:] for l in out.splitlines() if l.startswith("-") and not l.startswith("---")]


def scan(path, verbose=False):
    add = added_lines(path)
    counts = {}
    hits = {}
    for name, rx in AXES.items():
        h = [(i, l.strip()) for i, l in enumerate(add) if rx.search(l)]
        counts[name] = len(h)
        hits[name] = h
    return add, counts, hits


if __name__ == "__main__":
    verbose = "-v" in sys.argv
    paths = [a for a in sys.argv[1:] if a != "-v"]
    for p in paths:
        add, counts, hits = scan(p)
        rem = removed_lines(p)
        print(f"=== {p}  (+{len(add)} / -{len(rem)})")
        print("    " + "  ".join(f"{k}={v}" for k, v in counts.items()))
        if verbose:
            for k, h in hits.items():
                for i, l in h:
                    print(f"    [{k}] {l[:200]}")
