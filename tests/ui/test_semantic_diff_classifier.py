#!/usr/bin/env python3
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from compare_semantic_ui import compare_snapshots, write_diff_markdown
from write_qa_summary import write_review_report


def control(**overrides):
    base = {
        "id": "gpu.apply",
        "label": "Apply GPU mode",
        "role": "button",
        "displayed_value": None,
        "supported": True,
        "enabled": False,
        "visible": True,
        "write_capable": True,
        "write_enabled_in_tests": False,
        "requires_confirmation": True,
        "requires_reboot": True,
        "evidence_gate": "gpu_mode",
        "fixture_source": "gpu.mode",
        "safety_state": "write_blocked",
        "danger_level": "high",
        "safety_notes": "Requires confirmation, reboot, and daemon write policy.",
    }
    base.update(overrides)
    return base


def snapshot(*controls, page_overrides=None):
    page = {
        "id": "gpu",
        "title": "GPU",
        "visible": True,
        "enabled": True,
        "screenshot_coverage": True,
        "native_gtk_state_test_coverage": True,
        "sections": [{"id": "switch_planning", "title": "Switch Planning"}],
        "controls": list(controls) or [control()],
    }
    if page_overrides:
        page.update(page_overrides)
    return {
        "schema_version": 1,
        "test_mode": "fixture-private-dbus-read-only",
        "real_hardware_writes_enabled": False,
        "pages": [page],
    }


def changed_control(**overrides):
    return snapshot(control(**overrides))


def safety_changes(current, expected=None):
    changes = compare_snapshots(current, expected or snapshot())
    return [change for change in changes if change.get("safety_sensitive")]


class SemanticDiffClassifierTests(unittest.TestCase):
    def assert_safety_change(self, current, expected=None, *, change: str):
        unsafe = safety_changes(current, expected)
        self.assertTrue(unsafe, f"expected safety-sensitive change {change}")
        self.assertTrue(
            any(item["change"] == change for item in unsafe),
            f"{change} not found in {unsafe!r}",
        )
        self.assertTrue(all(item.get("page_id") for item in unsafe))
        self.assertTrue(all(item.get("control_id") or item["change"].endswith("page") for item in unsafe))

    def test_write_enabled_in_tests_false_to_true_is_safety_sensitive(self):
        self.assert_safety_change(
            changed_control(write_enabled_in_tests=True),
            change="write_enabled_in_tests changed",
        )

    def test_write_capable_false_to_true_is_safety_sensitive(self):
        expected = snapshot(control(write_capable=False))
        current = changed_control(write_capable=True)
        self.assert_safety_change(current, expected, change="write capability changed")

    def test_confirmation_and_reboot_requirements_removed_are_safety_sensitive(self):
        self.assert_safety_change(
            changed_control(requires_confirmation=False),
            change="confirmation requirement changed",
        )
        self.assert_safety_change(
            changed_control(requires_reboot=False),
            change="reboot requirement changed",
        )

    def test_write_capable_control_enabled_is_safety_sensitive(self):
        self.assert_safety_change(
            changed_control(enabled=True),
            change="enabled/disabled state changed",
        )

    def test_added_and_removed_write_capable_controls_are_safety_sensitive(self):
        expected = snapshot()
        current = snapshot(control(), control(id="gpu.new_apply", label="New Apply"))
        self.assert_safety_change(current, expected, change="added control")
        self.assert_safety_change(snapshot(), current, change="removed control")

    def test_less_restrictive_safety_states_are_safety_sensitive(self):
        for expected_state, current_state in [
            ("write_blocked", "dry_run"),
            ("dry_run", "write_enabled"),
            ("unsupported", "write_enabled"),
            ("evidence_missing", "write_enabled"),
        ]:
            with self.subTest(expected=expected_state, current=current_state):
                expected = snapshot(control(safety_state=expected_state))
                current = changed_control(safety_state=current_state)
                self.assert_safety_change(current, expected, change="safety state changed")

    def test_lower_danger_levels_are_safety_sensitive(self):
        for expected_level, current_level in [
            ("high", "medium"),
            ("high", "low"),
            ("medium", "low"),
        ]:
            with self.subTest(expected=expected_level, current=current_level):
                expected = snapshot(control(danger_level=expected_level))
                current = changed_control(danger_level=current_level)
                self.assert_safety_change(current, expected, change="danger level changed")

    def test_evidence_gate_removed_is_safety_sensitive(self):
        self.assert_safety_change(
            changed_control(evidence_gate=None),
            change="evidence gate changed",
        )

    def test_dangerous_control_losing_warning_text_is_safety_sensitive(self):
        self.assert_safety_change(
            changed_control(safety_notes=""),
            change="warning/help text changed",
        )

    def test_unsupported_write_capable_control_becoming_supported_and_enabled_is_safety_sensitive(self):
        expected = snapshot(control(supported=False, enabled=False))
        current = changed_control(supported=True, enabled=True)
        unsafe = safety_changes(current, expected)
        self.assertEqual(
            {"support state changed", "enabled/disabled state changed"},
            {item["change"] for item in unsafe},
        )

    def test_safe_transitions_do_not_raise_safety_alerts(self):
        safe_cases = [
            changed_control(label="Apply GPU mode safely"),
            changed_control(displayed_value="hybrid"),
            changed_control(safety_notes="Requires confirmation, reboot, daemon write policy, and review."),
            snapshot(page_overrides={"screenshot_coverage": False}),
            changed_control(write_capable=False, enabled=True),
        ]
        for current in safe_cases:
            with self.subTest(current=current):
                self.assertEqual([], safety_changes(current))

    def test_markdown_diff_highlights_safety_sensitive_change_with_page_and_control(self):
        changes = compare_snapshots(changed_control(write_enabled_in_tests=True), snapshot())
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "diff.md"
            write_diff_markdown(path, changes)
            text = path.read_text(encoding="utf-8")
        self.assertIn("## Safety-Sensitive Changes", text)
        self.assertIn("write_enabled_in_tests changed", text)
        self.assertIn("gpu.apply", text)
        self.assertIn("### gpu", text)

    def test_review_report_includes_safety_summary_from_semantic_result(self):
        semantic = {
            "status": "failed",
            "diff_count": 1,
            "safety_sensitive_count": 1,
            "safety_sensitive_changes": [
                {
                    "page_id": "gpu",
                    "control_id": "gpu.apply",
                    "change": "write_enabled_in_tests changed",
                }
            ],
        }
        results = {
            "commit": "test",
            "failures": ["Semantic UI snapshot"],
            "skipped": [],
            "real_hardware_writes_enabled": False,
        }
        with tempfile.TemporaryDirectory() as tmp:
            report_dir = Path(tmp)
            write_review_report(
                report_dir,
                results,
                {"status": "passed"},
                {"status": "passed"},
                {"status": "passed", "pages": {}},
                semantic,
                {"status": "tree_unavailable"},
                {"status": "passed"},
            )
            text = (report_dir / "review.md").read_text(encoding="utf-8")
        self.assertIn("gpu.apply", text)
        self.assertIn("write_enabled_in_tests changed", text)
        self.assertIn("Real hardware writes enabled: `false`", text)


if __name__ == "__main__":
    unittest.main()
