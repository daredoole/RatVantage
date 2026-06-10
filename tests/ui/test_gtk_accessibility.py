#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import signal
import sys
import time
from pathlib import Path

import pyatspi

from ratvantage_gui_qa import (
    DEFAULT_SYSFS_ROOT,
    PAGES,
    PAGE_TITLES,
    PrivateDaemon,
    REPO_ROOT,
    compact_tree,
    iter_tree,
    launch_ui,
    platform_metadata,
    report_dirs,
    run_logged,
    write_json,
)


EXPECTED_NAV = list(PAGE_TITLES.values())
DANGEROUS_WORDS = ("Apply", "Set", "Clear", "Capture", "Enable", "Reset")
SAFE_BUTTON_PREFIXES = ("Open ", "Copy ", "Try ", "Plan ", "Preview ")


def find_ratvantage_app():
    desktop = pyatspi.Registry.getDesktop(0)
    for app in desktop:
        app_name = (getattr(app, "name", "") or "").lower()
        if "legion-control-ui" in app_name or "ratvantage" in app_name:
            return app
        for node in iter_tree(app, max_depth=2, max_nodes=80):
            if "RatVantage" in node["name"]:
                return app
    return None


def assert_page(bus_address: str, page: str, report_dir: Path) -> dict:
    log_path = report_dir / "logs" / f"ui-state-{page}.log"
    proc = launch_ui(bus_address, page, log_path)
    app = None
    try:
        deadline = time.time() + 18
        while time.time() < deadline:
            if proc.poll() is not None:
                raise AssertionError(f"GTK app exited before AT-SPI inspection; see {log_path}")
            app = find_ratvantage_app()
            if app:
                break
            time.sleep(0.25)
        if not app:
            raise AssertionError("RatVantage GTK app was not visible to AT-SPI")

        nodes = list(iter_tree(app, max_depth=6, max_nodes=500))
        names = [node["name"] for node in nodes if node["name"]]
        missing_nav = [
            label
            for label in EXPECTED_NAV
            if label not in names and f"Open {label} page" not in names
        ]
        if missing_nav:
            raise AssertionError(f"missing navigation accessible labels: {missing_nav}")
        title = PAGE_TITLES[page]
        if title not in names and f"Open {title} page" not in names:
            raise AssertionError(f"page {page} has no visible title/header text")
        if len(set(names)) < 12:
            raise AssertionError(f"page {page} appears empty: {names}")

        dangerous = []
        for node in nodes:
            name = node["name"]
            role = node["role"].lower()
            if "button" not in role or not name:
                continue
            if name.startswith(SAFE_BUTTON_PREFIXES):
                continue
            if any(word in name for word in DANGEROUS_WORDS):
                dangerous.append({"name": name, "role": node["role"]})

        return {
            "page": page,
            "status": "passed",
            "node_count": len(nodes),
            "named_node_count": len(names),
            "dangerous_controls": dangerous,
            "tree": compact_tree(app, max_depth=4),
        }
    finally:
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    parser.add_argument("--sysfs-root", default=str(DEFAULT_SYSFS_ROOT))
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    report_dirs(report_dir)
    result = {
        "status": "passed",
        "native_gtk_state": "not_run",
        "atspi_state": "not_run",
        "test_mode": "fixture-private-dbus-read-only",
        "real_hardware_writes_enabled": False,
        "unprivileged": os.geteuid() != 0,
        "platform": platform_metadata(),
        "pages": [],
        "failures": [],
        "atspi_failures": [],
    }
    if os.geteuid() == 0:
        result["status"] = "failed"
        result["failures"].append("GUI state tests must not run as root")
        write_json(report_dir / "logs/ui-state.json", result)
        return 1

    native = run_logged(
        [
            "cargo",
            "test",
            "-p",
            "legion-control-ui",
            "--features",
            "gtk-ui",
            "--test",
            "gtk_shell",
            "dashboard_pages_render_quick_apply_and_gpu_controls",
            "--",
            "--exact",
        ],
        report_dir / "logs/native-gtk-state.log",
        timeout=120,
    )
    if native.returncode != 0:
        result["status"] = "failed"
        result["native_gtk_state"] = "failed"
        result["failures"].append("native GTK state test failed; see logs/native-gtk-state.log")
        write_json(report_dir / "logs/ui-state.json", result)
        return 1
    result["native_gtk_state"] = "passed"

    try:
        with PrivateDaemon(Path(args.sysfs_root), report_dir / "logs/ui-state-daemon") as daemon:
            for page in PAGES:
                try:
                    result["pages"].append(assert_page(daemon.bus_address, page, report_dir))
                except Exception as exc:
                    result["atspi_failures"].append(f"{page}: {exc}")
    except Exception as exc:
        result["atspi_state"] = "failed"
        result["atspi_failure"] = str(exc)

    if result["pages"]:
        result["atspi_state"] = "passed" if not result["atspi_failures"] else "partial"
    elif result["atspi_state"] == "not_run":
        result["atspi_state"] = "tree_unavailable"

    write_json(report_dir / "logs/ui-state.json", result)
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
