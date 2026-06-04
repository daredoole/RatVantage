#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh --output <dir> [options]

Capture OpenRGB bridge evidence for keyboard RGB promotion.

Default mode is dry-run: it lists devices, detects the Lenovo keyboard RGB
device, and records the command that would run. It does not set colors/modes.

Options:
  --output <dir>          Required output directory.
  --openrgb-bin <path>    OpenRGB binary to run. Default: openrgb from PATH.
  --effect <mode>         Requested OpenRGB mode. Default: Breathing.
  --brightness <0-100>    Requested brightness. Default: 75.
  --speed <0-100>         Requested speed. Default: 30.
  --colors <csv>          LED-order colors without '#'. Default: FF0000,00FF00,0000FF,FFFFFF.
  --device-selector <s>   Device selector for apply: name or index. Default: name.
  --sdk-evidence-bin <p>  SDK read-back helper. Default: repo helper/PATH lookup.
  --no-sdk-evidence       Do not capture SDK read-back snapshots.
  --allow-autoconnect     Let OpenRGB CLI connect to an existing local server instead of forcing --noautoconnect.
  --execute               Save profile, apply request, read back mode, restore saved profile.
  -h, --help              Show this help.

Execute mode changes keyboard RGB briefly. Use only when the operator can
visually confirm apply/restore behavior.
EOF
}

output=""
openrgb_bin="openrgb"
effect="Breathing"
brightness="75"
speed="30"
colors="FF0000,00FF00,0000FF,FFFFFF"
device_selector="name"
execute=0
sdk_evidence_bin=""
use_sdk_evidence=1
noautoconnect=1

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
    --effect)
      effect="${2:?missing value for --effect}"
      shift 2
      ;;
    --brightness)
      brightness="${2:?missing value for --brightness}"
      shift 2
      ;;
    --speed)
      speed="${2:?missing value for --speed}"
      shift 2
      ;;
    --colors)
      colors="${2:?missing value for --colors}"
      shift 2
      ;;
    --device-selector)
      device_selector="${2:?missing value for --device-selector}"
      shift 2
      ;;
    --sdk-evidence-bin)
      sdk_evidence_bin="${2:?missing value for --sdk-evidence-bin}"
      shift 2
      ;;
    --no-sdk-evidence)
      use_sdk_evidence=0
      shift
      ;;
    --allow-autoconnect)
      noautoconnect=0
      shift
      ;;
    --execute)
      execute=1
      shift
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

case "$brightness" in
  ''|*[!0-9]*) echo "--brightness must be 0-100" >&2; exit 2 ;;
esac
case "$speed" in
  ''|*[!0-9]*) echo "--speed must be 0-100" >&2; exit 2 ;;
esac
if (( brightness > 100 || speed > 100 )); then
  echo "--brightness and --speed must be 0-100" >&2
  exit 2
fi
if [[ ! "$colors" =~ ^[0-9A-Fa-f]{6}(,[0-9A-Fa-f]{6})*$ ]]; then
  echo "--colors must be comma-separated RRGGBB hex values" >&2
  exit 2
fi
if [[ "$device_selector" != "name" && "$device_selector" != "index" ]]; then
  echo "--device-selector must be name or index" >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate OpenRGB bridge evidence" >&2
  exit 1
}

openrgb_connect_args=()
if [[ "$noautoconnect" -eq 1 ]]; then
  openrgb_connect_args=(--noautoconnect)
fi

mkdir -p "$output/logs" "$output/profiles"
output="$(cd "$output" && pwd)"

json_out="$output/openrgb-keyboard-rgb-bridge-evidence.json"
md_out="$output/openrgb-keyboard-rgb-bridge-evidence.md"
before_stdout="$output/logs/openrgb-list-before.stdout"
before_stderr="$output/logs/openrgb-list-before.stderr"
after_stdout="$output/logs/openrgb-list-after.stdout"
after_stderr="$output/logs/openrgb-list-after.stderr"
restored_stdout="$output/logs/openrgb-list-restored.stdout"
restored_stderr="$output/logs/openrgb-list-restored.stderr"
apply_stdout="$output/logs/openrgb-apply.stdout"
apply_stderr="$output/logs/openrgb-apply.stderr"
restore_stdout="$output/logs/openrgb-restore.stdout"
restore_stderr="$output/logs/openrgb-restore.stderr"
save_before_stdout="$output/logs/openrgb-save-before.stdout"
save_before_stderr="$output/logs/openrgb-save-before.stderr"
save_after_stdout="$output/logs/openrgb-save-after.stdout"
save_after_stderr="$output/logs/openrgb-save-after.stderr"
sdk_before_stdout="$output/logs/openrgb-sdk-before.stdout"
sdk_before_stderr="$output/logs/openrgb-sdk-before.stderr"
sdk_after_stdout="$output/logs/openrgb-sdk-after.stdout"
sdk_after_stderr="$output/logs/openrgb-sdk-after.stderr"
sdk_restored_stdout="$output/logs/openrgb-sdk-restored.stdout"
sdk_restored_stderr="$output/logs/openrgb-sdk-restored.stderr"
before_profile_base="$output/profiles/before"
after_profile_base="$output/profiles/after"
before_profile="$before_profile_base.orp"
after_profile="$after_profile_base.orp"
sdk_before_dir="$output/sdk-before"
sdk_after_dir="$output/sdk-after"
sdk_restored_dir="$output/sdk-restored"
sdk_before_json="$sdk_before_dir/openrgb-keyboard-rgb-sdk-evidence.json"
sdk_after_json="$sdk_after_dir/openrgb-keyboard-rgb-sdk-evidence.json"
sdk_restored_json="$sdk_restored_dir/openrgb-keyboard-rgb-sdk-evidence.json"

rm -f \
  "$json_out" "$md_out" \
  "$before_profile_base" "$after_profile_base" \
  "$before_profile" "$after_profile" \
  "$before_profile.orp" "$after_profile.orp"
rm -rf "$sdk_before_dir" "$sdk_after_dir" "$sdk_restored_dir"

if [[ "$use_sdk_evidence" -eq 1 && -z "$sdk_evidence_bin" ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  if [[ -x "$script_dir/capture-keyboard-rgb-openrgb-sdk-evidence.sh" ]]; then
    sdk_evidence_bin="$script_dir/capture-keyboard-rgb-openrgb-sdk-evidence.sh"
  elif command -v ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence >/dev/null 2>&1; then
    sdk_evidence_bin="$(command -v ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence)"
  fi
fi

capture_sdk_snapshot() {
  local label="$1"
  local dir="$2"
  local stdout="$3"
  local stderr="$4"
  if [[ "$use_sdk_evidence" -ne 1 || -z "$sdk_evidence_bin" ]]; then
    : >"$stdout"
    : >"$stderr"
    return 127
  fi
  set +e
  timeout 30 "$sdk_evidence_bin" --output "$dir" --openrgb-bin "$openrgb_path" >"$stdout" 2>"$stderr"
  local exit_code=$?
  set -e
  if [[ "$exit_code" -ne 0 ]]; then
    printf 'SDK %s snapshot failed with exit %s\n' "$label" "$exit_code" >>"$stderr"
  fi
  return "$exit_code"
}

openrgb_path=""
if command -v "$openrgb_bin" >/dev/null 2>&1; then
  openrgb_path="$(command -v "$openrgb_bin")"
else
  : >"$before_stdout"
  printf 'openrgb binary not found: %s\n' "$openrgb_bin" >"$before_stderr"
  python3 - "$json_out" "$md_out" "$openrgb_path" "$execute" "$effect" "$brightness" "$speed" "$colors" "$before_stdout" "$after_stdout" "$restored_stdout" "$before_profile" "$after_profile" <<'PY'
import datetime as dt, json, pathlib, sys
json_path, md_path = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "execute": bool(int(sys.argv[4])),
    "openrgb": {"installed": False, "path": None},
    "request": {"effect": sys.argv[5], "brightness": int(sys.argv[6]), "speed": int(sys.argv[7]), "colors": sys.argv[8]},
    "result": {"status": "openrgb_missing", "backend_ready_evidence": False},
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
md_path.write_text("# OpenRGB Keyboard RGB Bridge Evidence\n\n- status: `openrgb_missing`\n")
PY
  echo "openrgb_bridge_evidence=$json_out"
  exit 0
fi

timeout 15 "$openrgb_path" --list-devices >"$before_stdout" 2>"$before_stderr" || true

device_name="$(
  python3 - "$before_stdout" <<'PY'
import pathlib, re, sys
raw = pathlib.Path(sys.argv[1]).read_text(errors="replace")
devices = []
current = None
for line in raw.splitlines():
    match = re.match(r"^(\d+):\s+(.+)$", line)
    if match:
        current = {"index": int(match.group(1)), "name": match.group(2).strip()}
        devices.append(current)
        continue
    if current is None or ":" not in line:
        continue
    key, value = line.split(":", 1)
    current[key.strip().lower().replace(" ", "_")] = value.strip()
for device in devices:
    haystack = " ".join(str(device.get(k, "")) for k in ("name", "description", "zones", "leds")).lower()
    if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack):
        print(device["name"])
        raise SystemExit(0)
PY
)"
device_index="$(
  python3 - "$before_stdout" <<'PY'
import pathlib, re, sys
raw = pathlib.Path(sys.argv[1]).read_text(errors="replace")
devices = []
current = None
for line in raw.splitlines():
    match = re.match(r"^(\d+):\s+(.+)$", line)
    if match:
        current = {"index": int(match.group(1)), "name": match.group(2).strip()}
        devices.append(current)
        continue
    if current is None or ":" not in line:
        continue
    key, value = line.split(":", 1)
    current[key.strip().lower().replace(" ", "_")] = value.strip()
for device in devices:
    haystack = " ".join(str(device.get(k, "")) for k in ("name", "description", "zones", "leds")).lower()
    if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack):
        print(device["index"])
        raise SystemExit(0)
PY
)"
device_apply_arg="$device_name"
if [[ "$device_selector" == "index" && -n "$device_index" ]]; then
  device_apply_arg="$device_index"
fi

apply_exit=""
restore_exit=""
save_before_exit=""
save_after_exit=""
sdk_before_exit=""
sdk_after_exit=""
sdk_restored_exit=""
if [[ -n "$device_name" ]]; then
  set +e
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --save-profile "$before_profile_base" >"$save_before_stdout" 2>"$save_before_stderr"
  save_before_exit=$?
  set -e
  capture_sdk_snapshot before "$sdk_before_dir" "$sdk_before_stdout" "$sdk_before_stderr" || sdk_before_exit=$?
  sdk_before_exit="${sdk_before_exit:-0}"
else
  : >"$save_before_stdout"; : >"$save_before_stderr"
  : >"$sdk_before_stdout"; : >"$sdk_before_stderr"
fi

if [[ "$execute" -eq 1 && -n "$device_name" ]]; then
  set +e
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --device "$device_apply_arg" --mode "$effect" --brightness "$brightness" --speed "$speed" --color "$colors" >"$apply_stdout" 2>"$apply_stderr"
  apply_exit=$?
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --list-devices >"$after_stdout" 2>"$after_stderr"
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --save-profile "$after_profile_base" >"$save_after_stdout" 2>"$save_after_stderr"
  save_after_exit=$?
  capture_sdk_snapshot after "$sdk_after_dir" "$sdk_after_stdout" "$sdk_after_stderr" || sdk_after_exit=$?
  sdk_after_exit="${sdk_after_exit:-0}"
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --profile "$before_profile" >"$restore_stdout" 2>"$restore_stderr"
  restore_exit=$?
  timeout 15 "$openrgb_path" "${openrgb_connect_args[@]}" --list-devices >"$restored_stdout" 2>"$restored_stderr"
  set -e
  capture_sdk_snapshot restored "$sdk_restored_dir" "$sdk_restored_stdout" "$sdk_restored_stderr" || sdk_restored_exit=$?
  sdk_restored_exit="${sdk_restored_exit:-0}"
else
  : >"$after_stdout"; : >"$after_stderr"
  : >"$restored_stdout"; : >"$restored_stderr"
  : >"$apply_stdout"; : >"$apply_stderr"
  : >"$restore_stdout"; : >"$restore_stderr"
  : >"$save_after_stdout"; : >"$save_after_stderr"
  : >"$sdk_after_stdout"; : >"$sdk_after_stderr"
  : >"$sdk_restored_stdout"; : >"$sdk_restored_stderr"
fi

python3 - \
  "$json_out" "$md_out" "$openrgb_path" "$execute" "$effect" "$brightness" "$speed" "$colors" \
  "$before_stdout" "$after_stdout" "$restored_stdout" "$before_profile" "$after_profile" \
  "$device_name" "$device_index" "$device_selector" "$device_apply_arg" "${apply_exit:-}" "${restore_exit:-}" "${save_before_exit:-}" "${save_after_exit:-}" \
  "${sdk_evidence_bin:-}" "$sdk_before_json" "$sdk_after_json" "$sdk_restored_json" \
  "${sdk_before_exit:-}" "${sdk_after_exit:-}" "${sdk_restored_exit:-}" "$noautoconnect" <<'PY'
import datetime as dt
import json
import pathlib
import re
import shlex
import sys

(
    json_path,
    md_path,
    openrgb_path,
    execute,
    effect,
    brightness,
    speed,
    colors,
    before_stdout,
    after_stdout,
    restored_stdout,
    before_profile,
    after_profile,
    device_name,
    device_index,
    device_selector,
    device_apply_arg,
    apply_exit,
    restore_exit,
    save_before_exit,
    save_after_exit,
    sdk_evidence_bin,
    sdk_before_json,
    sdk_after_json,
    sdk_restored_json,
    sdk_before_exit,
    sdk_after_exit,
    sdk_restored_exit,
    noautoconnect,
) = sys.argv[1:]

json_path = pathlib.Path(json_path)
md_path = pathlib.Path(md_path)
before_stdout = pathlib.Path(before_stdout)
after_stdout = pathlib.Path(after_stdout)
restored_stdout = pathlib.Path(restored_stdout)
before_profile = pathlib.Path(before_profile)
after_profile = pathlib.Path(after_profile)
sdk_before_json = pathlib.Path(sdk_before_json)
sdk_after_json = pathlib.Path(sdk_after_json)
sdk_restored_json = pathlib.Path(sdk_restored_json)
execute = bool(int(execute))
brightness_i = int(brightness)
speed_i = int(speed)
requested_colors = [color.upper() for color in colors.split(",") if color]
noautoconnect_b = bool(int(noautoconnect))

def parse_devices(path):
    raw = path.read_text(errors="replace")
    devices = []
    current = None
    for line in raw.splitlines():
        match = re.match(r"^(\d+):\s+(.+)$", line)
        if match:
            current = {"index": int(match.group(1)), "name": match.group(2).strip()}
            devices.append(current)
            continue
        if current is None or ":" not in line:
            continue
        key, value = line.split(":", 1)
        current[key.strip().lower().replace(" ", "_")] = value.strip()
    return devices

def keyboard_device(devices):
    for device in devices:
        haystack = " ".join(str(device.get(k, "")) for k in ("name", "description", "zones", "leds")).lower()
        if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack):
            return device
    return None

def current_mode(device):
    if not device:
        return None
    modes = device.get("modes") or ""
    match = re.search(r"\[([^\]]+)\]", modes)
    return match.group(1).strip() if match else device.get("current_mode")

def parse_openrgb_profile(path, requested_colors):
    if not path.exists():
        return {
            "path": str(path),
            "saved": False,
            "size_bytes": 0,
            "strings": [],
            "requested_rgb_triplets_found": [],
            "requested_bgr_triplets_found": [],
            "all_requested_colors_found": False,
        }
    raw = path.read_bytes()
    strings = [
        match.group(0).decode("utf-8", errors="replace")
        for match in re.finditer(rb"[ -~]{4,}", raw)
    ]
    colors = [color.upper() for color in requested_colors.split(",") if color]
    rgb_found = []
    bgr_found = []
    for color in colors:
        try:
            rgb = bytes.fromhex(color)
        except ValueError:
            continue
        bgr = bytes([rgb[2], rgb[1], rgb[0]])
        if rgb in raw:
            rgb_found.append(color)
        if bgr in raw:
            bgr_found.append(color)
    found = set(rgb_found) | set(bgr_found)
    return {
        "path": str(path),
        "saved": True,
        "size_bytes": len(raw),
        "strings": strings[:32],
        "requested_rgb_triplets_found": rgb_found,
        "requested_bgr_triplets_found": bgr_found,
        "all_requested_colors_found": bool(colors) and all(color in found for color in colors),
    }

def normalize_sdk_color(color):
    if not isinstance(color, str):
        return None
    color = color.strip().upper()
    if color.startswith("#"):
        color = color[1:]
    return color if re.fullmatch(r"[0-9A-F]{6}", color) else None

def parse_sdk_snapshot(path):
    snapshot = {
        "path": str(path),
        "captured": path.exists(),
        "status": None,
        "connected": False,
        "controller_count": None,
        "keyboard_detected": False,
        "read_back_supported": False,
        "active_mode": None,
        "colors": [],
        "led_count": 0,
        "color_count": 0,
        "promotion_blockers": [],
    }
    if not path.exists():
        return snapshot
    try:
        report = json.loads(path.read_text())
    except Exception as error:
        snapshot["promotion_blockers"] = [f"SDK snapshot parse failed: {error}"]
        return snapshot
    result = report.get("result") or {}
    sdk = report.get("sdk") or {}
    keyboard = report.get("keyboard") or {}
    controller = keyboard.get("controller") or {}
    colors_v = [
        color
        for color in (normalize_sdk_color(color) for color in controller.get("colors") or [])
        if color
    ]
    snapshot.update({
        "status": result.get("status"),
        "connected": bool(sdk.get("connected")),
        "controller_count": sdk.get("controller_count"),
        "keyboard_detected": bool(keyboard.get("detected")),
        "read_back_supported": bool(result.get("read_back_supported")),
        "active_mode": controller.get("active_mode"),
        "colors": colors_v,
        "led_count": len(controller.get("leds") or []),
        "color_count": len(colors_v),
        "promotion_blockers": result.get("promotion_blockers") or [],
    })
    return snapshot

def sdk_colors_match(snapshot, expected):
    colors_v = snapshot.get("colors") or []
    return bool(expected) and colors_v[: len(expected)] == expected

before_device = keyboard_device(parse_devices(before_stdout))
after_device = keyboard_device(parse_devices(after_stdout)) if execute else None
restored_device = keyboard_device(parse_devices(restored_stdout)) if execute else None
sdk_before = parse_sdk_snapshot(sdk_before_json)
sdk_after = parse_sdk_snapshot(sdk_after_json) if execute else parse_sdk_snapshot(pathlib.Path(""))
sdk_restored = parse_sdk_snapshot(sdk_restored_json) if execute else parse_sdk_snapshot(pathlib.Path(""))
cli_before_mode = current_mode(before_device)
cli_after_mode = current_mode(after_device)
cli_restored_mode = current_mode(restored_device)
before_mode = sdk_before.get("active_mode") or cli_before_mode
after_mode = sdk_after.get("active_mode") or cli_after_mode
restored_mode = sdk_restored.get("active_mode") or cli_restored_mode

command = [
    "openrgb",
]
if noautoconnect_b:
    command.append("--noautoconnect")
command.extend([
    "--device",
    device_apply_arg or "<detected-device>",
    "--mode",
    effect,
    "--brightness",
    str(brightness_i),
    "--speed",
    str(speed_i),
    "--color",
    colors.upper(),
])
command_preview = " ".join(shlex.quote(part) for part in command)
cli_mode_readback_matches = execute and cli_after_mode is not None and cli_after_mode.lower() == effect.lower()
sdk_mode_readback_matches = execute and sdk_after.get("active_mode") is not None and sdk_after["active_mode"].lower() == effect.lower()
mode_readback_matches = bool(cli_mode_readback_matches or sdk_mode_readback_matches)
cli_restore_mode_matches = execute and cli_restored_mode == cli_before_mode and cli_before_mode is not None
sdk_restore_mode_matches = execute and sdk_restored.get("active_mode") == sdk_before.get("active_mode") and sdk_before.get("active_mode") is not None
restore_mode_matches = bool(cli_restore_mode_matches or sdk_restore_mode_matches)
profiles_saved = before_profile.exists() and after_profile.exists()
before_profile_info = parse_openrgb_profile(before_profile, colors)
after_profile_info = parse_openrgb_profile(after_profile, colors)
profile_color_readback_supported = execute and after_profile_info["all_requested_colors_found"]
sdk_color_readback_supported = execute and sdk_after.get("read_back_supported") and sdk_colors_match(sdk_after, requested_colors)
sdk_restore_color_matches = execute and sdk_before.get("read_back_supported") and sdk_restored.get("read_back_supported") and sdk_restored.get("colors") == sdk_before.get("colors")
color_readback_supported = bool(profile_color_readback_supported or sdk_color_readback_supported)
restore_color_matches = bool((not sdk_color_readback_supported) or sdk_restore_color_matches)
backend_ready_evidence = bool(
    mode_readback_matches
    and restore_mode_matches
    and restore_color_matches
    and profiles_saved
    and color_readback_supported
)

promotion_blockers = []
if not execute:
    promotion_blockers.append("dry-run only; execute mode has not captured apply/read-back/restore evidence")
if not before_device:
    promotion_blockers.append("OpenRGB Lenovo keyboard device was not detected")
if execute and not mode_readback_matches:
    promotion_blockers.append("mode read-back did not match requested effect")
if execute and not restore_mode_matches:
    promotion_blockers.append("restore read-back did not return to the saved previous mode")
if execute and sdk_color_readback_supported and not sdk_restore_color_matches:
    promotion_blockers.append("SDK color read-back did not return to the saved previous colors after restore")
if execute and not profiles_saved:
    promotion_blockers.append("before/after OpenRGB profiles were not both saved")
if execute and profiles_saved and not profile_color_readback_supported and not sdk_color_readback_supported:
    promotion_blockers.append("neither saved OpenRGB profile nor SDK read-back proved every requested color")
if not color_readback_supported:
    promotion_blockers.append("per-zone color read-back is not proven; use profile byte evidence or SDK before/after read-back before daemon execution")

report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "execute": execute,
    "openrgb": {"installed": True, "path": openrgb_path, "noautoconnect": noautoconnect_b},
    "request": {
        "device": device_name or None,
        "device_index": int(device_index) if device_index else None,
        "device_selector": device_selector,
        "device_apply_arg": device_apply_arg or None,
        "effect": effect,
        "brightness": brightness_i,
        "speed": speed_i,
        "colors": colors.upper(),
        "command_preview": command_preview,
    },
    "readback": {
        "before_mode": before_mode,
        "after_mode": after_mode,
        "restored_mode": restored_mode,
        "cli_before_mode": cli_before_mode,
        "cli_after_mode": cli_after_mode,
        "cli_restored_mode": cli_restored_mode,
        "sdk_before_mode": sdk_before.get("active_mode"),
        "sdk_after_mode": sdk_after.get("active_mode"),
        "sdk_restored_mode": sdk_restored.get("active_mode"),
        "cli_mode_readback_matches": cli_mode_readback_matches,
        "sdk_mode_readback_matches": sdk_mode_readback_matches,
        "mode_readback_matches": mode_readback_matches,
        "cli_restore_mode_matches": cli_restore_mode_matches,
        "sdk_restore_mode_matches": sdk_restore_mode_matches,
        "restore_mode_matches": restore_mode_matches,
        "profile_color_readback_supported": profile_color_readback_supported,
        "sdk_color_readback_supported": sdk_color_readback_supported,
        "color_readback_supported": color_readback_supported,
        "sdk_restore_color_matches": sdk_restore_color_matches,
        "restore_color_matches": restore_color_matches,
    },
    "sdk_readback": {
        "helper": sdk_evidence_bin or None,
        "before": sdk_before,
        "after": sdk_after if execute else None,
        "restored": sdk_restored if execute else None,
    },
    "profiles": {
        "before_profile": before_profile_info,
        "after_profile": after_profile_info,
        "before_profile_saved": before_profile_info["saved"],
        "after_profile_saved": after_profile_info["saved"],
    },
    "commands": {
        "save_before_exit": int(save_before_exit) if save_before_exit else None,
        "apply_exit": int(apply_exit) if apply_exit else None,
        "save_after_exit": int(save_after_exit) if save_after_exit else None,
        "restore_exit": int(restore_exit) if restore_exit else None,
        "sdk_before_exit": int(sdk_before_exit) if sdk_before_exit else None,
        "sdk_after_exit": int(sdk_after_exit) if sdk_after_exit else None,
        "sdk_restored_exit": int(sdk_restored_exit) if sdk_restored_exit else None,
    },
    "result": {
        "status": "executed" if execute else "dry_run",
        "backend_ready_evidence": backend_ready_evidence,
        "promotion_blockers": promotion_blockers,
    },
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

lines = [
    "# OpenRGB Keyboard RGB Bridge Evidence",
    "",
    f"- status: `{report['result']['status']}`",
    f"- device: `{report['request']['device']}`",
    f"- command_preview: `{command_preview}`",
    f"- before_mode: `{before_mode}`",
    f"- after_mode: `{after_mode}`",
    f"- restored_mode: `{restored_mode}`",
    f"- mode_readback_matches: `{mode_readback_matches}`",
    f"- restore_mode_matches: `{restore_mode_matches}`",
    f"- sdk_color_readback_supported: `{sdk_color_readback_supported}`",
    f"- sdk_restore_color_matches: `{sdk_restore_color_matches}`",
    f"- color_readback_supported: `{color_readback_supported}`",
    f"- backend_ready_evidence: `{backend_ready_evidence}`",
    "",
    "## Promotion Blockers",
]
lines.extend(f"- {blocker}" for blocker in promotion_blockers)
lines.append("")
md_path.write_text("\n".join(lines))
PY

echo "openrgb_bridge_evidence=$json_out"
