#!/usr/bin/env bash
set -euo pipefail

desktop-file-validate data/desktop/org.ratvantage.LegionControl.desktop
desktop-file-validate data/desktop/org.ratvantage.LegionControl.Tray.desktop
appstreamcli validate --no-net data/metainfo/org.ratvantage.LegionControl.metainfo.xml
unit_tmp="$(mktemp --suffix=.service)"
trap 'rm -f "$unit_tmp"' EXIT
sed 's#^ExecStart=.*#ExecStart=/bin/true#' \
  data/systemd/legion-control-daemon.service >"$unit_tmp"
systemd-analyze verify "$unit_tmp"
python3 - <<'PY'
from pathlib import Path
import tomllib
from xml.etree import ElementTree

for path in [
    "data/dbus/org.ratvantage.LegionControl1.conf",
    "data/polkit/org.ratvantage.LegionControl1.policy",
    "data/metainfo/org.ratvantage.LegionControl.metainfo.xml",
]:
    ElementTree.parse(Path(path))

preset_dir = Path("data/presets")
expected = {"quiet-office", "balanced-daily", "gaming", "max-safe"}
seen = set()
for path in sorted(preset_dir.glob("*.toml")):
    data = tomllib.loads(path.read_text())
    preset_id = data.get("id")
    seen.add(preset_id)
    if preset_id != path.stem:
        raise SystemExit(f"{path}: id must match file name")
    if data.get("schema_version") != 1:
        raise SystemExit(f"{path}: schema_version must be 1")
    for key in ["label", "description", "safety_note"]:
        if not isinstance(data.get(key), str) or not data[key].strip():
            raise SystemExit(f"{path}: missing {key}")
    profiles = data.get("target_profiles")
    if not isinstance(profiles, list) or not profiles or not all(isinstance(item, str) for item in profiles):
        raise SystemExit(f"{path}: target_profiles must be non-empty string list")
    points = data.get("points")
    if not isinstance(points, list) or len(points) != 10:
        raise SystemExit(f"{path}: expected exactly 10 points")
    previous_temp = -1
    previous_pwm = -1
    for index, point in enumerate(points, start=1):
        temp = point.get("temperature_c")
        pwm = point.get("pwm")
        if not isinstance(temp, int) or temp <= previous_temp:
            raise SystemExit(f"{path}: point {index} temperature_c must ascend")
        if not isinstance(pwm, int) or not 0 <= pwm <= 255:
            raise SystemExit(f"{path}: point {index} pwm must be 0..255")
        if pwm < previous_pwm:
            raise SystemExit(f"{path}: point {index} pwm must be non-decreasing")
        previous_temp = temp
        previous_pwm = pwm
if seen != expected:
    raise SystemExit(f"preset ids mismatch: {sorted(seen)}")
PY

echo "packaging assets validated"
