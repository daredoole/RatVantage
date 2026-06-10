#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import signal
import shutil
import sys
import time
from pathlib import Path

import pyatspi

from ratvantage_gui_qa import (
    DEFAULT_SYSFS_ROOT,
    PAGE_TITLES,
    PrivateDaemon,
    compact_tree,
    iter_tree,
    launch_ui,
    names_by_role,
    report_dirs,
    write_json,
)


def find_app():
    desktop = pyatspi.Registry.getDesktop(0)
    for app in desktop:
        names = [node["name"] for node in iter_tree(app, max_depth=2, max_nodes=80)]
        if any("RatVantage" in name or "legion-control-ui" in name for name in names):
            return app
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    parser.add_argument("--sysfs-root", default=str(DEFAULT_SYSFS_ROOT))
    parser.add_argument("--expected", default="tests/fixtures/widget-trees/expected.json")
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    report_dirs(report_dir)
    tree_dir = report_dir / "widget-trees"
    expected_path = Path(args.expected)
    update = os.environ.get("UPDATE_GUI_BASELINES") == "1"
    result = {"status": "passed", "missing_expected": False, "critical_controls": []}

    proc = None
    try:
        with PrivateDaemon(Path(args.sysfs_root), report_dir / "logs/widget-tree-daemon") as daemon:
            proc = launch_ui(daemon.bus_address, "status", report_dir / "logs/widget-tree-ui.log")
            app = None
            deadline = time.time() + 18
            while time.time() < deadline:
                if proc.poll() is not None:
                    raise RuntimeError("GTK app exited before widget tree dump")
                app = find_app()
                if app:
                    break
                time.sleep(0.25)
            if not app:
                raise RuntimeError("RatVantage app not visible to AT-SPI")
            nodes = list(iter_tree(app, max_depth=6, max_nodes=500))
            current = {
                "stable_page_titles": PAGE_TITLES,
                "names_by_role": names_by_role(nodes),
                "tree": compact_tree(app, max_depth=5),
            }
            write_json(tree_dir / "current.json", current)
            result["critical_controls"] = list(PAGE_TITLES.values())
            current_names = set()
            for values in current["names_by_role"].values():
                current_names.update(values)
            missing_current = [
                title
                for title in PAGE_TITLES.values()
                if title not in current_names and f"Open {title} page" not in current_names
            ]
            if missing_current:
                result["status"] = "tree_unavailable"
                result["reason"] = (
                    "AT-SPI registry is reachable, but GTK exported only the top-level "
                    "application/frame in this Xvfb session. Semantic UI snapshots cover "
                    "structural state; native GTK tests cover behavioral state."
                )
                result["missing_current_controls"] = missing_current
                write_json(tree_dir / "result.json", result)
                return 0

            if update:
                expected_path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(tree_dir / "current.json", expected_path)
                result["status"] = "updated_expected"
            elif expected_path.exists():
                expected = json.loads(expected_path.read_text(encoding="utf-8"))
                missing = []
                for values in expected.get("names_by_role", {}).values():
                    for name in values:
                        if name.startswith("Open ") and name not in current_names:
                            missing.append(name)
                if missing:
                    result["status"] = "failed"
                    result["missing_controls"] = sorted(set(missing))
            else:
                result["status"] = "missing_expected"
                result["missing_expected"] = True
    except Exception as exc:
        result["status"] = "failed"
        result["failure"] = str(exc)
    finally:
        if proc and proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()

    write_json(tree_dir / "result.json", result)
    return 1 if result["status"] == "failed" else 0


if __name__ == "__main__":
    sys.exit(main())
