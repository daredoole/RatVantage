#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

python3 - <<'PY'
from __future__ import annotations

import os
import re
import subprocess
import sys

tracked = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
    text=True,
).splitlines()

text_suffixes = {
    ".desktop",
    ".in",
    ".json",
    ".md",
    ".policy",
    ".rs",
    ".service",
    ".sh",
    ".toml",
    ".tsv",
    ".txt",
    ".yaml",
    ".yml",
}
text_names = {
    ".gitignore",
    "AGENTS.md",
    "CHANGELOG.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "LICENSE",
    "README.md",
    "SECURITY.md",
    "SUPPORT.md",
}

allowed_paths = {
    "LICENSE",
    "Cargo.toml",
}

patterns = [
    (
        "absolute personal home path",
        re.compile(r"/home/(?!test\b|user\b|runner\b|[^/\s]+>\b)[A-Za-z0-9._-]+"),
    ),
    (
        "private repository wording",
        re.compile(r"\bprivate for now\b|\bnonpublic\b|\bnon-public\b", re.IGNORECASE),
    ),
    (
        "secret-like assignment",
        re.compile(
            r"(?i)\b(api[_-]?key|token|secret|password)\s*[:=]\s*['\"]?[A-Za-z0-9_./+=:-]{8,}"
        ),
    ),
    (
        "machine identifier wording",
        re.compile(r"(?i)\b(machine-id|machine id|mac address|serial number)\b"),
    ),
    (
        "stale beta docs wording",
        re.compile(r"(?i)\blicense placeholder\b|\bpre-alpha implementation scaffold\b|\brecommended working name\b"),
    ),
]

findings: list[tuple[str, int, str, str]] = []

for path in tracked:
    if path == "scripts/audit-public-release.sh":
        continue
    name = os.path.basename(path)
    suffix = os.path.splitext(path)[1]
    if suffix not in text_suffixes and name not in text_names:
        continue
    try:
        data = open(path, "r", encoding="utf-8", errors="ignore").read()
    except OSError:
        continue
    for line_no, line in enumerate(data.splitlines(), 1):
        for label, regex in patterns:
            if regex.search(line):
                if path in allowed_paths and label in {
                    "machine identifier wording",
                    "absolute personal home path",
                }:
                    continue
                findings.append((path, line_no, label, line.strip()))

if findings:
    print("Public release audit failed:")
    for path, line_no, label, line in findings:
        print(f"{path}:{line_no}: {label}: {line[:180]}")
    sys.exit(1)

print(f"Public release audit passed ({len(tracked)} tracked and untracked files scanned).")
PY
