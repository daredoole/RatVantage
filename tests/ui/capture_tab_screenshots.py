#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image

from ratvantage_gui_qa import DEFAULT_SYSFS_ROOT, PAGES, REPO_ROOT, report_dirs, run_logged, write_json


def is_meaningful_screenshot(path: Path) -> bool:
    with Image.open(path).convert("RGB") as image:
        colors = image.getcolors(maxcolors=256)
        if colors is not None and len(colors) <= 2:
            return False
        extrema = image.getextrema()
        return any(low != high for low, high in extrema)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    parser.add_argument("--sysfs-root", default=str(DEFAULT_SYSFS_ROOT))
    parser.add_argument("--pages", default=",".join(PAGES))
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    report_dirs(report_dir)
    capture_dir = report_dir / "screenshot-capture"
    proc = run_logged(
        [
            str(REPO_ROOT / "scripts/capture-gtk-smoke-report.sh"),
            "--sysfs-root",
            args.sysfs_root,
            "--pages",
            args.pages,
            "--output",
            str(capture_dir),
        ],
        report_dir / "logs/screenshot-capture.log",
        timeout=240,
    )
    screenshots = []
    source_dir = capture_dir / "screenshots"
    target_dir = report_dir / "screenshots"
    target_dir.mkdir(parents=True, exist_ok=True)
    if source_dir.exists():
        for png in sorted(source_dir.glob("*.png")):
            target = target_dir / png.name
            target.write_bytes(png.read_bytes())
            screenshots.append({"path": str(target), "meaningful": is_meaningful_screenshot(target)})
    result = {
        "status": "passed"
        if proc.returncode == 0 and screenshots and all(item["meaningful"] for item in screenshots)
        else "failed",
        "returncode": proc.returncode,
        "screenshots": screenshots,
        "source_report": str(capture_dir / "report.md"),
    }
    if proc.returncode != 0:
        result["failure"] = "GTK screenshot capture command failed"
    elif not screenshots:
        result["failure"] = "no screenshots were produced"
    elif not all(item["meaningful"] for item in screenshots):
        result["failure"] = "one or more screenshots were blank or effectively empty"
    write_json(report_dir / "logs/screenshots.json", result)
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
