#!/usr/bin/env python3
"""Shared helpers for deterministic RatVantage GUI QA scripts."""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SYSFS_ROOT = REPO_ROOT / "tests/fixtures/sysfs-82wm-confirmed"
PAGES = [
    "status",
    "profiles",
    "battery",
    "gpu",
    "fans",
    "appearance",
    "automations",
    "settings",
    "diagnostics",
]
PAGE_TITLES = {
    "status": "Overview",
    "profiles": "Power",
    "battery": "Battery",
    "gpu": "GPU",
    "fans": "Fans",
    "appearance": "Devices",
    "automations": "Automations",
    "settings": "Settings",
    "diagnostics": "Diagnostics",
}


def report_dirs(report_dir: Path) -> None:
    for name in ["screenshots", "visual-diffs", "widget-trees", "semantic-ui", "dbus", "logs"]:
        (report_dir / name).mkdir(parents=True, exist_ok=True)


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def command_available(name: str) -> bool:
    return shutil.which(name) is not None


def run_logged(cmd: list[str], log_path: Path, env: dict[str, str] | None = None, timeout: int = 120) -> subprocess.CompletedProcess:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    proc = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        env=merged_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    log_path.write_text(proc.stdout, encoding="utf-8")
    return proc


@dataclass
class PrivateDaemon:
    sysfs_root: Path
    log_dir: Path
    tmp: tempfile.TemporaryDirectory | None = None
    bus: subprocess.Popen | None = None
    daemon: subprocess.Popen | None = None
    bus_address: str = ""

    def start(self) -> str:
        self.tmp = tempfile.TemporaryDirectory(prefix="ratvantage-gui-qa-")
        tmp_path = Path(self.tmp.name)
        self.log_dir.mkdir(parents=True, exist_ok=True)
        dbus_log = (self.log_dir / "dbus.log").open("w", encoding="utf-8")
        self.bus = subprocess.Popen(
            ["dbus-daemon", "--session", "--print-address=1", "--nofork"],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=dbus_log,
        )
        assert self.bus.stdout is not None
        self.bus_address = self.bus.stdout.readline().strip()
        if not self.bus_address:
            raise RuntimeError("failed to start private D-Bus session")

        daemon_log_path = self.log_dir / "daemon.log"
        daemon_log = daemon_log_path.open("w", encoding="utf-8")
        env = os.environ.copy()
        env["DBUS_SESSION_BUS_ADDRESS"] = self.bus_address
        self.daemon = subprocess.Popen(
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "legion-control-daemon",
                "--",
                "--session",
                "--sysfs-root",
                str(self.sysfs_root),
                "--state-path",
                str(tmp_path / "state.toml"),
            ],
            cwd=REPO_ROOT,
            env=env,
            stdout=daemon_log,
            stderr=subprocess.STDOUT,
            text=True,
        )
        for _ in range(160):
            if self.daemon.poll() is not None:
                raise RuntimeError(f"private daemon exited early; see {daemon_log_path}")
            if daemon_log_path.exists() and "serving interface=" in daemon_log_path.read_text(encoding="utf-8", errors="replace"):
                return self.bus_address
            time.sleep(0.1)
        raise RuntimeError(f"private daemon did not become ready; see {daemon_log_path}")

    def stop(self) -> None:
        for proc in [self.daemon, self.bus]:
            if proc and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
        if self.tmp:
            self.tmp.cleanup()

    def __enter__(self) -> "PrivateDaemon":
        self.start()
        return self

    def __exit__(self, _exc_type, _exc, _tb) -> None:
        self.stop()


def ui_env(bus_address: str, theme: str = "Adwaita:light") -> dict[str, str]:
    return {
        "GSK_RENDERER": "cairo",
        "GTK_THEME": theme,
        "NO_AT_BRIDGE": "0",
    }


def launch_ui(bus_address: str, page: str, log_path: Path, auto_quit_ms: int | None = None) -> subprocess.Popen:
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "legion-control-ui",
        "--features",
        "gtk-ui",
        "--",
        "--bus-address",
        bus_address,
        "--gtk-page",
        page,
    ]
    if auto_quit_ms is not None:
        cmd.extend(["--gtk-auto-quit-ms", str(auto_quit_ms)])
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log = log_path.open("w", encoding="utf-8")
    return subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
        env={**os.environ.copy(), **ui_env(bus_address)},
        stdout=log,
        stderr=subprocess.STDOUT,
        text=True,
    )


def wait_for(condition, timeout: float = 12.0, interval: float = 0.2):
    deadline = time.time() + timeout
    while time.time() < deadline:
        value = condition()
        if value:
            return value
        time.sleep(interval)
    return None


def platform_metadata() -> dict:
    return {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "display": os.environ.get("DISPLAY", ""),
        "xvfb": os.environ.get("RATVANTAGE_QA_UNDER_XVFB") == "1",
        "dbus_run_session": os.environ.get("RATVANTAGE_QA_UNDER_DBUS") == "1",
    }


def iter_tree(node, depth: int = 0, max_depth: int = 8, max_nodes: int = 600):
    state = {"count": 0}

    def walk(current, current_depth: int):
        if current_depth > max_depth or state["count"] >= max_nodes:
            return
        state["count"] += 1
        role = ""
        try:
            role = current.getRoleName()
        except Exception:
            pass
        name = getattr(current, "name", "") or ""
        yield {"name": name, "role": role, "depth": current_depth}
        try:
            count = min(current.childCount, max_nodes - state["count"])
        except Exception:
            count = 0
        for index in range(count):
            try:
                child = current.getChildAtIndex(index)
            except Exception:
                continue
            yield from walk(child, current_depth + 1)

    yield from walk(node, depth)


def compact_tree(node, depth: int = 0, max_depth: int = 6) -> dict:
    role = ""
    try:
        role = node.getRoleName()
    except Exception:
        pass
    item = {"name": getattr(node, "name", "") or "", "role": role}
    if depth < max_depth:
        children = []
        try:
            count = node.childCount
        except Exception:
            count = 0
        for index in range(count):
            try:
                child = node.getChildAtIndex(index)
            except Exception:
                continue
            child_item = compact_tree(child, depth + 1, max_depth)
            if child_item["name"] or child_item.get("children"):
                children.append(child_item)
        if children:
            item["children"] = children
    return item


def names_by_role(nodes: Iterable[dict]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for node in nodes:
        name = node.get("name") or ""
        if not name:
            continue
        out.setdefault(node.get("role") or "unknown", []).append(name)
    return out
