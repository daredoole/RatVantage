#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


REQUIRED = [
    ("summary", "markdown", Path("summary.md"), "# RatVantage GUI QA Summary"),
    ("review", "markdown", Path("review.md"), "# RatVantage QA Review"),
    ("results", "json", Path("results.json"), None),
    ("semantic-current", "json", Path("semantic-ui/current.json"), None),
    ("semantic-diff", "markdown", Path("semantic-ui/diff.md"), "# Semantic UI Snapshot Diff"),
    ("visual-index", "markdown", Path("visual-diffs/index.md"), "# Visual Regression Review"),
    ("dbus-current", "xml", Path("dbus/current.xml"), None),
    ("dbus-diff", "markdown", Path("dbus/diff.md"), "# D-Bus Contract Diff"),
]


def check_artifact(report_dir: Path, name: str, kind: str, relative: Path, marker: str | None) -> dict:
    path = report_dir / relative
    result = {"name": name, "path": str(path), "kind": kind, "status": "passed"}
    if not path.exists():
        result["status"] = "failed"
        result["reason"] = "missing"
        return result
    if path.stat().st_size == 0:
        result["status"] = "failed"
        result["reason"] = "empty"
        return result

    try:
        text = path.read_text(encoding="utf-8")
        if kind == "json":
            json.loads(text)
        elif kind == "markdown":
            if marker and marker not in text:
                raise ValueError(f"missing marker {marker!r}")
            if not any(line.startswith("# ") for line in text.splitlines()):
                raise ValueError("missing top-level heading")
        elif kind == "xml":
            root = ET.fromstring(text)
            if root.tag != "node" or not root.findall("interface"):
                raise ValueError("missing D-Bus node/interface")
        else:
            raise ValueError(f"unknown artifact kind {kind}")
    except Exception as exc:
        result["status"] = "failed"
        result["reason"] = str(exc)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    rows = [
        check_artifact(report_dir, name, kind, relative, marker)
        for name, kind, relative, marker in REQUIRED
    ]
    width = max(len(row["name"]) for row in rows)
    print("QA artifact sanity")
    print("artifact".ljust(width), "status", "path")
    for row in rows:
        suffix = f" ({row['reason']})" if row["status"] == "failed" else ""
        print(row["name"].ljust(width), row["status"] + suffix, row["path"])
    return 1 if any(row["status"] != "passed" for row in rows) else 0


if __name__ == "__main__":
    sys.exit(main())
