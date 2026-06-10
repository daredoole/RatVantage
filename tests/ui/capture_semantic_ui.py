#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path

from ratvantage_gui_qa import report_dirs, run_logged, write_json


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report-dir", default="target/qa-report")
    args = parser.parse_args()

    report_dir = Path(args.report_dir)
    report_dirs(report_dir)
    output = (report_dir / "semantic-ui" / "current.json").resolve()
    output.parent.mkdir(parents=True, exist_ok=True)

    proc = run_logged(
        [
            "cargo",
            "test",
            "-p",
            "legion-control-ui",
            "--features",
            "gtk-ui",
            "--test",
            "gtk_shell",
            "semantic_ui_snapshot_can_be_emitted",
            "--",
            "--exact",
        ],
        report_dir / "logs/semantic-ui-capture.log",
        env={"RATVANTAGE_SEMANTIC_UI_SNAPSHOT": str(output)},
        timeout=120,
    )
    result = {
        "status": "passed" if proc.returncode == 0 and output.exists() else "failed",
        "returncode": proc.returncode,
        "current": str(output),
        "test_mode": "fixture-native-gtk-read-only",
        "real_hardware_writes_enabled": False,
    }
    if proc.returncode != 0:
        result["failure"] = "native GTK semantic snapshot test failed"
    elif not output.exists():
        result["failure"] = "semantic UI snapshot was not produced"
    write_json(report_dir / "semantic-ui" / "capture.json", result)
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
