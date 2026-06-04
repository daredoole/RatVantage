#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/check-keyboard-rgb-openrgb.sh --output <dir> [options]

Capture read-only OpenRGB keyboard RGB readiness evidence.

This script does not set colors or modes. It runs `openrgb --list-devices`,
checks current user access to i2c/hidraw nodes, and records whether OpenRGB
detects a Lenovo keyboard RGB device.

Options:
  --output <dir>          Required output directory.
  --openrgb-bin <path>    OpenRGB binary to run. Default: openrgb from PATH.
  --dev-root <root>       Device root for tests. Default: /dev.
  -h, --help              Show this help.
EOF
}

output=""
openrgb_bin="openrgb"
dev_root="/dev"

while (($#)); do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --openrgb-bin)
      openrgb_bin="${2:?missing value for --openrgb-bin}"
      shift 2
      ;;
    --dev-root)
      dev_root="${2:?missing value for --dev-root}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate OpenRGB readiness evidence" >&2
  exit 1
}

mkdir -p "$output/logs"

json_out="$output/openrgb-keyboard-rgb-readiness.json"
md_out="$output/openrgb-keyboard-rgb-readiness.md"
stdout_log="$output/logs/openrgb-list-devices.stdout"
stderr_log="$output/logs/openrgb-list-devices.stderr"

openrgb_path=""
if command -v "$openrgb_bin" >/dev/null 2>&1; then
  openrgb_path="$(command -v "$openrgb_bin")"
  timeout 15 "$openrgb_path" --list-devices >"$stdout_log" 2>"$stderr_log" || true
else
  : >"$stdout_log"
  printf 'openrgb binary not found: %s\n' "$openrgb_bin" >"$stderr_log"
fi

python3 - "$json_out" "$md_out" "$stdout_log" "$stderr_log" "$openrgb_path" "$dev_root" <<'PY'
import datetime as dt
import glob
import grp
import json
import os
import pathlib
import pwd
import re
import stat
import sys

json_path = pathlib.Path(sys.argv[1])
md_path = pathlib.Path(sys.argv[2])
stdout_path = pathlib.Path(sys.argv[3])
stderr_path = pathlib.Path(sys.argv[4])
openrgb_path = sys.argv[5] or None
dev_root = pathlib.Path(sys.argv[6])

stdout = stdout_path.read_text(errors="replace")
stderr = stderr_path.read_text(errors="replace")

def current_groups():
    names = []
    gids = os.getgroups()
    for gid in gids:
        try:
            names.append(grp.getgrgid(gid).gr_name)
        except KeyError:
            names.append(str(gid))
    return sorted(set(names))

groups = current_groups()

def user_name(uid):
    try:
        return pwd.getpwuid(uid).pw_name
    except KeyError:
        return str(uid)

def group_name(gid):
    try:
        return grp.getgrgid(gid).gr_name
    except KeyError:
        return str(gid)

def node_info(pattern):
    nodes = []
    for raw in sorted(glob.glob(str(dev_root / pattern))):
        path = pathlib.Path(raw)
        try:
            st = path.stat()
        except OSError:
            continue
        nodes.append({
            "path": str(path),
            "mode": stat.filemode(st.st_mode),
            "uid": st.st_uid,
            "user": user_name(st.st_uid),
            "gid": st.st_gid,
            "group": group_name(st.st_gid),
            "readable": os.access(path, os.R_OK),
            "writable": os.access(path, os.W_OK),
        })
    return nodes

def parse_devices(raw):
    devices = []
    current = None
    for line in raw.splitlines():
        match = re.match(r"^(\d+):\s+(.+)$", line)
        if match:
            current = {"index": int(match.group(1)), "name": match.group(2).strip()}
            devices.append(current)
            continue
        if current is None:
            continue
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        current[key.strip().lower().replace(" ", "_")] = value.strip()
    return devices

devices = parse_devices(stdout)
keyboard_devices = [
    device for device in devices
    if "lenovo" in (device.get("name", "") + " " + device.get("description", "")).lower()
    and ("keyboard" in (device.get("zones", "") + " " + device.get("leds", "")).lower()
         or "4-zone" in device.get("description", "").lower())
]

modules = pathlib.Path("/proc/modules").read_text(errors="replace") if pathlib.Path("/proc/modules").exists() else ""
i2c_dev_loaded = any(line.startswith("i2c_dev ") for line in modules.splitlines())
i2c_nodes = node_info("i2c-*")
hidraw_nodes = node_info("hidraw*")
has_i2c_access = any(node["readable"] and node["writable"] for node in i2c_nodes)
has_hidraw_access = any(node["readable"] and node["writable"] for node in hidraw_nodes)
user_in_i2c_group = "i2c" in groups
missing_access = []
if not i2c_dev_loaded:
    missing_access.append("i2c-dev module")
if not user_in_i2c_group:
    missing_access.append("persistent i2c group membership")
if not has_i2c_access:
    missing_access.append("read/write access to /dev/i2c-*")
if not has_hidraw_access:
    missing_access.append("read/write access to /dev/hidraw*")

report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "openrgb": {
        "installed": openrgb_path is not None,
        "path": openrgb_path,
        "list_devices_exit_captured": True,
        "detected_devices": devices,
        "keyboard_rgb_devices": keyboard_devices,
        "detects_lenovo_keyboard_rgb": bool(keyboard_devices),
    },
    "linux_access": {
        "user": os.environ.get("USER") or os.environ.get("LOGNAME") or "unknown",
        "groups": groups,
        "user_in_i2c_group": user_in_i2c_group,
        "i2c_dev_loaded": i2c_dev_loaded,
        "i2c_nodes": i2c_nodes,
        "hidraw_nodes": hidraw_nodes,
        "has_i2c_rw_access": has_i2c_access,
        "has_hidraw_rw_access": has_hidraw_access,
        "missing_access": missing_access,
        "setup_recommended": bool(missing_access),
        "setup_command": "ratvantage-setup-keyboard-rgb-openrgb-access",
    },
    "ratvantage": {
        "openrgb_backend_candidate": bool(keyboard_devices),
        "backend_ready": False,
        "write_support_claimed": False,
        "promotion_blockers": [
            "no RatVantage OpenRGB command/read-back contract yet",
            "no reset-to-previous-mode evidence captured through RatVantage",
            "daemon policy and rollback tests must pass before exposing writes",
        ],
    },
}

json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

lines = [
    "# OpenRGB Keyboard RGB Readiness",
    "",
    f"- openrgb_installed: `{report['openrgb']['installed']}`",
    f"- detects_lenovo_keyboard_rgb: `{report['openrgb']['detects_lenovo_keyboard_rgb']}`",
    f"- i2c_dev_loaded: `{i2c_dev_loaded}`",
    f"- user_in_i2c_group: `{user_in_i2c_group}`",
    f"- has_i2c_rw_access: `{has_i2c_access}`",
    f"- has_hidraw_rw_access: `{has_hidraw_access}`",
    f"- setup_recommended: `{bool(missing_access)}`",
    f"- ratvantage_backend_ready: `{report['ratvantage']['backend_ready']}`",
    "",
    "## Detected Keyboard Devices",
]
if keyboard_devices:
    for device in keyboard_devices:
        lines.extend([
            f"- index: `{device.get('index')}`",
            f"  name: `{device.get('name')}`",
            f"  description: `{device.get('description', 'unknown')}`",
            f"  modes: `{device.get('modes', 'unknown')}`",
            f"  zones: `{device.get('zones', 'unknown')}`",
            f"  leds: `{device.get('leds', 'unknown')}`",
        ])
else:
    lines.append("- none")
lines.extend([
    "",
    "## Setup Guidance",
    "- Prefer reporting the exact OpenRGB/access state before changing groups.",
    "- If i2c access is missing, use `sudo /usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access --user <user>` and then log out/in.",
    "- A GUI button should not silently modify Linux groups; use setup guidance or a one-time privileged installer.",
    "- Keep RatVantage RGB writes disabled until OpenRGB bridge read-back/reset behavior is tested.",
    "",
])
md_path.write_text("\n".join(lines))
PY

echo "openrgb_readiness=$json_out"
