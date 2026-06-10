#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

from PIL import Image, ImageChops

from ratvantage_gui_qa import PAGES, REPO_ROOT, report_dirs, write_json


def diff_images(current: Path, baseline: Path, diff_path: Path) -> dict:
    with Image.open(current).convert("RGB") as cur, Image.open(baseline).convert("RGB") as base:
        if cur.size != base.size:
            return {"status": "failed", "reason": f"size differs: current={cur.size} baseline={base.size}"}
        diff = ImageChops.difference(cur, base)
        pixel_iter = diff.get_flattened_data() if hasattr(diff, "get_flattened_data") else diff.getdata()
        pixels = list(pixel_iter)
        changed = 0
        severe = 0
        total_delta = 0
        for pixel in pixels:
            delta = max(pixel)
            total_delta += delta
            if delta > 12:
                changed += 1
            if delta > 48:
                severe += 1
        ratio = changed / max(1, len(pixels))
        severe_ratio = severe / max(1, len(pixels))
        diff_path.parent.mkdir(parents=True, exist_ok=True)
        diff.save(diff_path)
        status = "passed"
        if ratio > 0.015 or severe_ratio > 0.002:
            status = "failed"
        return {
            "status": status,
            "changed_pixels": changed,
            "total_pixels": len(pixels),
            "changed_ratio": ratio,
            "changed_percent": ratio * 100.0,
            "severe_pixels": severe,
            "severe_ratio": severe_ratio,
            "average_delta": total_delta / max(1, len(pixels)),
            "diff": str(diff_path),
        }


def write_visual_index(path: Path, result: dict) -> None:
    pages = result.get("pages") or {}
    failed = [
        (page, data)
        for page, data in pages.items()
        if data.get("status") not in {"passed", "updated"}
    ]
    passed = [
        (page, data)
        for page, data in pages.items()
        if data.get("status") in {"passed", "updated"}
    ]
    rows = failed + passed
    lines = [
        "# Visual Regression Review",
        "",
        "If intentional, inspect current/diff images before updating baseline.",
        "",
        "| Page | Status | Changed | Baseline | Current | Diff |",
        "|---|---:|---:|---|---|---|",
    ]
    for page, data in rows:
        changed = "n/a"
        if "changed_pixels" in data:
            changed = f"{data['changed_pixels']} px ({data.get('changed_percent', 0.0):.4f}%)"
        baseline = data.get("baseline") or str(REPO_ROOT / "tests/fixtures/gui-baselines" / f"{page}.png")
        current = data.get("current") or str(Path("target/qa-report/screenshots") / f"{page}.png")
        diff = data.get("diff") or ""
        lines.append(
            f"| `{page}` | `{data.get('status')}` | {changed} | `{baseline}` | `{current}` | `{diff}` |"
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    parser.add_argument("--baseline-dir", default=str(REPO_ROOT / "tests/fixtures/gui-baselines"))
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    baseline_dir = Path(args.baseline_dir)
    report_dirs(report_dir)
    current_dir = report_dir / "screenshots"
    diff_dir = report_dir / "visual-diffs"
    update = os.environ.get("UPDATE_GUI_BASELINES") == "1"
    result = {"status": "passed", "updated": False, "pages": {}, "missing_baselines": []}

    baseline_dir.mkdir(parents=True, exist_ok=True)
    for page in PAGES:
        current = current_dir / f"{page}.png"
        baseline = baseline_dir / f"{page}.png"
        if not current.exists():
            result["status"] = "failed"
            result["pages"][page] = {
                "status": "failed",
                "reason": "current screenshot missing",
                "current": str(current),
                "baseline": str(baseline),
            }
            continue
        if update:
            shutil.copy2(current, baseline)
            result["updated"] = True
            result["pages"][page] = {
                "status": "updated",
                "baseline": str(baseline),
                "current": str(current),
            }
            continue
        if not baseline.exists():
            result["missing_baselines"].append(str(baseline))
            result["pages"][page] = {
                "status": "missing_baseline",
                "baseline": str(baseline),
                "current": str(current),
            }
            continue
        page_result = diff_images(current, baseline, diff_dir / f"{page}.diff.png")
        page_result["current"] = str(current)
        page_result["baseline"] = str(baseline)
        result["pages"][page] = page_result
        if page_result["status"] != "passed":
            result["status"] = "failed"

    if update:
        result["status"] = "updated_baselines"
    elif result["missing_baselines"] and result["status"] == "passed":
        result["status"] = "missing_baseline"
    write_visual_index(diff_dir / "index.md", result)
    write_json(diff_dir / "results.json", result)
    return 1 if result["status"] == "failed" else 0


if __name__ == "__main__":
    sys.exit(main())
