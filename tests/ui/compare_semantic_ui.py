#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
from collections import defaultdict
from pathlib import Path
from typing import Any

from ratvantage_gui_qa import REPO_ROOT, report_dirs, write_json


CLASSIFIED_FIELDS = {
    "label": "label changed",
    "displayed_value": "displayed value changed",
    "supported": "support state changed",
    "enabled": "enabled/disabled state changed",
    "safety_state": "safety state changed",
    "write_capable": "write capability changed",
    "write_enabled_in_tests": "write_enabled_in_tests changed",
    "requires_confirmation": "confirmation requirement changed",
    "requires_reboot": "reboot requirement changed",
    "evidence_gate": "evidence gate changed",
    "danger_level": "danger level changed",
    "safety_notes": "warning/help text changed",
}
SAFETY_ORDER = {
    "unsupported": 90,
    "evidence_missing": 80,
    "write_blocked": 70,
    "confirmation_required": 60,
    "reboot_required": 55,
    "dry_run": 40,
    "daemon_state_only": 30,
    "read_only": 20,
    "none": 0,
}
DANGER_ORDER = {"none": 0, "low": 1, "medium": 2, "high": 3, "critical": 4}


def canonical(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: canonical(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        items = [canonical(item) for item in value]
        if all(isinstance(item, dict) and "id" in item for item in items):
            return sorted(items, key=lambda item: str(item["id"]))
        return items
    return value


def load_json(path: Path) -> Any:
    return canonical(json.loads(path.read_text(encoding="utf-8")))


def by_id(items: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {str(item.get("id")): item for item in items}


def page_title(page: dict[str, Any] | None, page_id: str) -> str:
    if not page:
        return page_id
    title = page.get("title")
    return f"{title} ({page_id})" if title else page_id


def is_safety_sensitive(
    change: str,
    current: Any,
    expected: Any,
    current_control: dict[str, Any] | None,
    expected_control: dict[str, Any] | None,
) -> bool:
    if change == "added control":
        return bool(current_control and current_control.get("write_capable"))
    if change == "write_enabled_in_tests changed":
        return expected is False and current is True
    if change == "write capability changed":
        return expected is False and current is True
    if change == "confirmation requirement changed":
        return expected is True and current is False
    if change == "reboot requirement changed":
        return expected is True and current is False
    if change == "evidence gate changed":
        return expected is not None and current != expected
    if change == "safety state changed":
        return SAFETY_ORDER.get(str(current), 0) < SAFETY_ORDER.get(str(expected), 0)
    if change == "danger level changed":
        return DANGER_ORDER.get(str(current), 0) < DANGER_ORDER.get(str(expected), 0)
    if change == "support state changed":
        return (
            expected is False
            and current is True
            and bool((current_control or expected_control or {}).get("write_capable"))
        )
    if change == "enabled/disabled state changed":
        return (
            expected is False
            and current is True
            and bool((current_control or expected_control or {}).get("write_capable"))
        )
    if change == "warning/help text changed":
        expected_note = str(expected or "")
        current_note = str(current or "")
        dangerous = DANGER_ORDER.get(str((expected_control or current_control or {}).get("danger_level")), 0) >= 2
        return dangerous and len(current_note) < len(expected_note)
    return False


def compare_controls(
    page_id: str,
    current_page: dict[str, Any],
    expected_page: dict[str, Any],
) -> list[dict[str, Any]]:
    changes: list[dict[str, Any]] = []
    current_controls = by_id(current_page.get("controls") or [])
    expected_controls = by_id(expected_page.get("controls") or [])
    for control_id in sorted(set(current_controls) | set(expected_controls)):
        current = current_controls.get(control_id)
        expected = expected_controls.get(control_id)
        if current is None:
            changes.append(
                {
                    "page_id": page_id,
                    "control_id": control_id,
                    "change": "removed control",
                    "expected": expected,
                    "current": None,
                    "safety_sensitive": bool(expected and expected.get("write_capable")),
                }
            )
            continue
        if expected is None:
            changes.append(
                {
                    "page_id": page_id,
                    "control_id": control_id,
                    "change": "added control",
                    "expected": None,
                    "current": current,
                    "safety_sensitive": is_safety_sensitive("added control", current, expected, current, expected),
                }
            )
            continue
        for field, change in CLASSIFIED_FIELDS.items():
            cur_value = current.get(field)
            exp_value = expected.get(field)
            if cur_value != exp_value:
                changes.append(
                    {
                        "page_id": page_id,
                        "control_id": control_id,
                        "control_label": current.get("label") or expected.get("label"),
                        "field": field,
                        "change": change,
                        "expected": exp_value,
                        "current": cur_value,
                        "safety_sensitive": is_safety_sensitive(
                            change, cur_value, exp_value, current, expected
                        ),
                    }
                )
    return changes


def compare_snapshots(current: dict[str, Any], expected: dict[str, Any]) -> list[dict[str, Any]]:
    changes: list[dict[str, Any]] = []
    current_pages = by_id(current.get("pages") or [])
    expected_pages = by_id(expected.get("pages") or [])
    for page_id in sorted(set(current_pages) | set(expected_pages)):
        current_page = current_pages.get(page_id)
        expected_page = expected_pages.get(page_id)
        if current_page is None:
            changes.append(
                {
                    "page_id": page_id,
                    "page_title": page_title(expected_page, page_id),
                    "change": "removed page",
                    "expected": expected_page,
                    "current": None,
                    "safety_sensitive": True,
                }
            )
            continue
        if expected_page is None:
            changes.append(
                {
                    "page_id": page_id,
                    "page_title": page_title(current_page, page_id),
                    "change": "added page",
                    "expected": None,
                    "current": current_page,
                    "safety_sensitive": any(
                        bool(control.get("write_capable"))
                        for control in current_page.get("controls") or []
                    ),
                }
            )
            continue
        for field in ("title", "visible", "enabled", "screenshot_coverage", "native_gtk_state_test_coverage"):
            if current_page.get(field) != expected_page.get(field):
                changes.append(
                    {
                        "page_id": page_id,
                        "page_title": page_title(current_page, page_id),
                        "change": f"page {field} changed",
                        "field": field,
                        "expected": expected_page.get(field),
                        "current": current_page.get(field),
                        "safety_sensitive": field in {"visible", "enabled"},
                    }
                )
        changes.extend(compare_controls(page_id, current_page, expected_page))
    return changes


def fmt_value(value: Any) -> str:
    if isinstance(value, (dict, list)):
        text = json.dumps(value, sort_keys=True)
    else:
        text = str(value)
    return text if len(text) <= 120 else text[:117] + "..."


def write_diff_markdown(path: Path, changes: list[dict[str, Any]]) -> None:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for change in changes:
        grouped[str(change.get("page_id", "global"))].append(change)
    safety = [change for change in changes if change.get("safety_sensitive")]

    lines = ["# Semantic UI Snapshot Diff", ""]
    lines.append("## Safety-Sensitive Changes")
    lines.append("")
    if safety:
        for change in safety:
            label = change.get("control_label") or change.get("control_id") or change.get("page_id")
            control_id = change.get("control_id")
            control_path = f" `{control_id}`" if control_id and control_id != label else ""
            lines.append(
                f"- **{change['change']}** on `{label}`{control_path} in `{change.get('page_id')}`"
            )
    else:
        lines.append("- None detected")
    lines.append("")
    lines.append("## Changes By Page")
    lines.append("")
    if not changes:
        lines.append("- No semantic UI changes.")
    for page_id in sorted(grouped):
        lines.append(f"### {page_id}")
        lines.append("")
        for change in grouped[page_id]:
            target = change.get("control_label") or change.get("control_id") or page_id
            control_id = change.get("control_id")
            control_path = f" (`{control_id}`)" if control_id and control_id != target else ""
            marker = " **SAFETY**" if change.get("safety_sensitive") else ""
            lines.append(f"- `{target}`{control_path}: {change['change']}{marker}")
            if "field" in change or change["change"] not in {"added control", "removed control", "added page", "removed page"}:
                lines.append(f"  - expected: `{fmt_value(change.get('expected'))}`")
                lines.append(f"  - current: `{fmt_value(change.get('current'))}`")
        lines.append("")
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    parser.add_argument(
        "--baseline",
        default=str(REPO_ROOT / "tests/fixtures/semantic-ui/expected.json"),
    )
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    baseline = Path(args.baseline)
    report_dirs(report_dir)
    semantic_dir = report_dir / "semantic-ui"
    current_path = semantic_dir / "current.json"
    diff_json = semantic_dir / "diff.json"
    diff_md = semantic_dir / "diff.md"
    update = os.environ.get("UPDATE_SEMANTIC_UI_BASELINE") == "1"

    result = {
        "status": "passed",
        "blocking": baseline.exists(),
        "updated": False,
        "current": str(current_path),
        "baseline": str(baseline),
        "diff_json": str(diff_json),
        "diff_md": str(diff_md),
        "safety_sensitive_count": 0,
    }

    if not current_path.exists():
        result["status"] = "failed"
        result["failure"] = "semantic UI current snapshot missing"
        write_json(semantic_dir / "result.json", result)
        return 1

    baseline.parent.mkdir(parents=True, exist_ok=True)
    if update:
        shutil.copy2(current_path, baseline)
        result["status"] = "updated_baseline"
        result["blocking"] = True
        result["updated"] = True
        write_json(semantic_dir / "result.json", result)
        return 0

    if not baseline.exists():
        result["status"] = "missing_baseline"
        result["blocking"] = False
        result["failure"] = "baseline missing; review current.json, then rerun with UPDATE_SEMANTIC_UI_BASELINE=1"
        write_diff_markdown(diff_md, [])
        write_json(semantic_dir / "result.json", result)
        return 0

    current = load_json(current_path)
    expected = load_json(baseline)
    changes = compare_snapshots(current, expected)
    result["diff_count"] = len(changes)
    result["safety_sensitive_count"] = sum(1 for change in changes if change.get("safety_sensitive"))
    result["safety_sensitive_changes"] = [
        {
            "page_id": change.get("page_id"),
            "control_id": change.get("control_id"),
            "change": change.get("change"),
            "field": change.get("field"),
        }
        for change in changes
        if change.get("safety_sensitive")
    ]
    write_json(diff_json, {"changes": changes, "safety_sensitive_changes": result["safety_sensitive_changes"]})
    write_diff_markdown(diff_md, changes)
    if changes:
        result["status"] = "failed"
        result["failure"] = "semantic UI snapshot differs from approved baseline"
    write_json(semantic_dir / "result.json", result)
    return 1 if result["status"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
