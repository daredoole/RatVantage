#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from ratvantage_gui_qa import PAGES, platform_metadata, report_dirs, write_json


def load_json(path: Path) -> dict:
    if not path.exists():
        return {"status": "missing", "path": str(path)}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        return {"status": "invalid", "path": str(path), "error": str(exc)}


def git_commit() -> str:
    proc = subprocess.run(["git", "rev-parse", "HEAD"], text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return proc.stdout.strip() if proc.returncode == 0 else "unknown"


def gate_rows(ui_state: dict, screenshots: dict, visual: dict, semantic: dict, dbus: dict, widget: dict) -> list[tuple[str, str, str]]:
    return [
        ("Native GTK behavioral tests", ui_state.get("status", "missing"), "blocking"),
        ("Screenshot nonblank checks", screenshots.get("status", "missing"), "blocking"),
        ("Visual regression baselines", visual.get("status", "missing"), "blocking"),
        ("Semantic UI structural/safety snapshot", semantic.get("status", "missing"), "blocking"),
        ("D-Bus contract snapshot", dbus.get("status", "missing"), "blocking"),
        ("AT-SPI child widget export", widget.get("status", "missing"), "supplemental"),
    ]


def visual_failures(visual: dict) -> list[str]:
    pages = visual.get("pages") or {}
    return [
        f"{page}: {data.get('status')}"
        for page, data in sorted(pages.items())
        if data.get("status") not in {"passed", "updated"}
    ]


def review_control_path(page_id: str | None, control_id: str | None) -> str:
    if not page_id:
        return control_id or "unknown"
    if not control_id:
        return page_id
    if control_id == page_id or control_id.startswith(f"{page_id}."):
        return control_id
    return f"{page_id}.{control_id}"


def write_review_report(
    report_dir: Path,
    results: dict,
    ui_state: dict,
    screenshots: dict,
    visual: dict,
    semantic: dict,
    widget: dict,
    dbus: dict,
) -> None:
    safety = semantic.get("safety_sensitive_changes") or []
    semantic_status = semantic.get("status", "missing")
    visual_bad = visual_failures(visual)
    dbus_status = dbus.get("status", "missing")
    overall = "failed" if results.get("failures") else "passed"
    if semantic_status in {"missing_baseline", "missing"} or dbus_status in {"missing_baseline", "missing"}:
        overall = "needs baseline review"

    lines = [
        "# RatVantage QA Review",
        "",
        f"- Overall status: `{overall}`",
        f"- Commit: `{results['commit']}`",
        f"- Real hardware writes enabled: `{str(results['real_hardware_writes_enabled']).lower()}`",
        "",
        "## Gates",
        "",
        "| Gate | Status | Mode |",
        "|---|---:|---:|",
    ]
    for name, status, mode in gate_rows(ui_state, screenshots, visual, semantic, dbus, widget):
        lines.append(f"| {name} | `{status}` | `{mode}` |")

    lines.extend(["", "## Safety-Sensitive Changes", ""])
    if safety:
        for item in safety:
            lines.append(
                f"- `{review_control_path(item.get('page_id'), item.get('control_id'))}`: {item.get('change')}"
            )
    else:
        lines.append("- None detected by semantic UI diff.")

    lines.extend(
        [
            "",
            "## Semantic UI Changes",
            "",
            f"- Status: `{semantic_status}`",
            f"- Total changes: `{semantic.get('diff_count', 0)}`",
            f"- Safety-sensitive changes: `{semantic.get('safety_sensitive_count', 0)}`",
            "- Detail: `target/qa-report/semantic-ui/diff.md`",
            "",
            "## Visual Changes",
            "",
            f"- Status: `{visual.get('status')}`",
            f"- Failed/changed pages: `{len(visual_bad)}`",
        ]
    )
    if visual_bad:
        lines.extend(f"- {item}" for item in visual_bad)
    lines.extend(
        [
            "- Detail: `target/qa-report/visual-diffs/index.md`",
            "",
            "## D-Bus Changes",
            "",
            f"- Status: `{dbus_status}`",
            "- Detail: `target/qa-report/dbus/diff.md`",
            "",
            "## Skipped Or Supplemental",
            "",
        ]
    )
    if results.get("skipped"):
        lines.extend(f"- {item}" for item in results["skipped"])
    else:
        lines.append("- None")
    lines.extend(
        [
            f"- AT-SPI: `{widget.get('status')}`; semantic UI snapshot is the enforced structural gate.",
            "",
            "## Artifact Paths",
            "",
            "- `target/qa-report/review.md`",
            "- `target/qa-report/summary.md`",
            "- `target/qa-report/results.json`",
            "- `target/qa-report/semantic-ui/current.json`",
            "- `target/qa-report/semantic-ui/diff.md`",
            "- `target/qa-report/visual-diffs/index.md`",
            "- `target/qa-report/dbus/diff.md`",
            "- `target/qa-report/screenshots/`",
            "",
            "## Reviewer Checklist",
            "",
            "- [ ] Are real hardware writes disabled in tests?",
            "- [ ] Did any write-capable control become enabled?",
            "- [ ] Did any confirmation/reboot/evidence gate become weaker?",
            "- [ ] Did any dangerous control lose warning/help text?",
            "- [ ] Did any visual screenshot change intentionally?",
            "- [ ] Did the D-Bus contract change intentionally?",
            "- [ ] Were baselines updated only after review?",
        ]
    )
    (report_dir / "review.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    args = parser.parse_args()
    report_dir = Path(args.report_dir)
    report_dirs(report_dir)

    ui_state = load_json(report_dir / "logs/ui-state.json")
    screenshots = load_json(report_dir / "logs/screenshots.json")
    visual = load_json(report_dir / "visual-diffs/results.json")
    semantic_capture = load_json(report_dir / "semantic-ui/capture.json")
    semantic = load_json(report_dir / "semantic-ui/result.json")
    widget = load_json(report_dir / "widget-trees/result.json")
    dbus = load_json(report_dir / "dbus/result.json")
    stages = load_json(report_dir / "logs/stages.json")

    failures = []
    skipped = []
    for name, data in [
        ("GUI state", ui_state),
        ("Screenshots", screenshots),
        ("Visual regression", visual),
        ("Semantic UI capture", semantic_capture),
        ("Semantic UI snapshot", semantic),
        ("Widget tree", widget),
        ("D-Bus contract", dbus),
    ]:
        status = data.get("status")
        if status == "failed":
            failures.append(name)
        if status in {"missing_baseline", "missing_expected", "missing", "tree_unavailable"}:
            skipped.append(f"{name}: {status}")
    for stage in stages.get("stages", []):
        if stage.get("status") == "failed":
            failures.append(f"Stage: {stage.get('stage', 'unknown')}")

    results = {
        "commit": git_commit(),
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "platform": platform_metadata(),
        "test_mode": "fixture-private-dbus-read-only",
        "real_hardware_writes_enabled": False,
        "implemented_pages": PAGES,
        "stage_results": {
            "gui_state": ui_state,
            "screenshots": screenshots,
            "visual_regression": visual,
            "semantic_ui_capture": semantic_capture,
            "semantic_ui_snapshot": semantic,
            "widget_tree": widget,
            "dbus_contract": dbus,
            "stages": stages,
        },
        "failures": failures,
        "skipped": skipped,
    }
    write_json(report_dir / "results.json", results)
    write_review_report(report_dir, results, ui_state, screenshots, visual, semantic, widget, dbus)

    screenshot_paths = screenshots.get("screenshots") or []
    diff_paths = []
    for page in (visual.get("pages") or {}).values():
        if page.get("diff"):
            diff_paths.append(page["diff"])

    lines = [
        "# RatVantage GUI QA Summary",
        "",
        f"- Commit: `{results['commit']}`",
        f"- Generated: `{results['generated_at']}`",
        f"- Platform: `{results['platform']['platform']}`",
        "- Test mode: fixture private D-Bus daemon, no hardware writes",
        "- Real hardware writes enabled: `false`",
        f"- GUI state test: `{ui_state.get('status')}`",
        f"- Screenshot capture: `{screenshots.get('status')}`",
        f"- Visual regression: `{visual.get('status')}`",
        f"- Semantic UI capture: `{semantic_capture.get('status')}`",
        f"- Semantic UI snapshot: `{semantic.get('status')}`",
        f"- Widget tree snapshot: `{widget.get('status')}`",
        f"- D-Bus contract snapshot: `{dbus.get('status')}`",
        f"- AT-SPI tree export: `{ui_state.get('atspi_state', 'unknown')}`",
        f"- Screenshots captured: `{len(screenshot_paths)}`",
        f"- Visual diffs produced: `{len(diff_paths)}`",
        "- Review report: `target/qa-report/review.md`",
        "- Semantic diff report: `target/qa-report/semantic-ui/diff.md`",
        "- Visual diff index: `target/qa-report/visual-diffs/index.md`",
        "- D-Bus diff report: `target/qa-report/dbus/diff.md`",
        "",
        "## Existing Controls Exercised",
        "",
        "- platform profile",
        "- battery charge type / conservation mode",
        "- CPU governor, EPP, and boost",
        "- AMD GPU DPM force level",
        "- firmware PPT attributes",
        "- curve optimizer all-core",
        "- fan preset planning/write-blocked state",
        "- GPU mode switching planning/reboot-required state",
        "- device toggles and RGB/LED planning surfaces when present",
        "- tray status via existing Rust tests and local CI stages",
        "",
        "## Safety State",
        "",
        "- GUI tests run unprivileged.",
        "- Private daemon starts without `--enable-*-write` flags.",
        "- Test sysfs root is the fixture tree.",
        "- Dangerous control findings are recorded in `logs/ui-state.json`.",
        "- Semantic UI safety/write-state findings are recorded in `semantic-ui/current.json`.",
        "",
        "## Skipped Or Baseline-Gated",
    ]
    if skipped:
        lines.extend(f"- {item}" for item in skipped)
    else:
        lines.append("- None")
    if widget.get("status") == "tree_unavailable":
        lines.extend(
            [
                "",
                "## Widget Tree Fallback",
                "",
                f"- Reason: {widget.get('reason')}",
                "- AT-SPI child export remains unavailable on this host.",
                "- Semantic UI snapshot is the enforced structural UI gate.",
                "- Native GTK page-state tests remain the enforced behavioral UI gate.",
                "- Screenshot capture and visual regression remain the enforced visual UI gates.",
                "- No widget-tree baseline was approved from the shallow AT-SPI tree.",
            ]
        )
    lines.extend(["", "## Failures"])
    if failures:
        lines.extend(f"- {item}" for item in failures)
    else:
        lines.append("- None")
    lines.extend(
        [
            "",
            "## Evidence Paths",
            "",
            "- `target/qa-report/results.json`",
            "- `target/qa-report/review.md`",
            "- `target/qa-report/logs/`",
            "- `target/qa-report/screenshots/`",
            "- `target/qa-report/visual-diffs/`",
            "- `target/qa-report/semantic-ui/current.json`",
            "- `target/qa-report/widget-trees/current.json`",
            "- `target/qa-report/dbus/current.xml`",
            "",
            "## Recommended Next Fixes",
            "",
        ]
    )
    if failures:
        lines.append("- Fix failed stages shown above, then rerun `scripts/qa-gui.sh`.")
    elif widget.get("status") == "tree_unavailable" and len(skipped) == 1:
        lines.append(
            "- Keep the semantic UI snapshot blocking while investigating GTK/AT-SPI child widget export separately."
        )
    elif skipped:
        lines.append("- Review generated screenshots, semantic UI JSON, widget tree, and D-Bus XML before approving any missing baseline with its explicit update variable.")
    else:
        lines.append("- Keep this QA stage blocking in local and GitHub CI.")
    (report_dir / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
