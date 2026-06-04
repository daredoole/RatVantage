#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh [options] <bundle-dir|json>

Review an OpenRGB keyboard RGB bridge evidence bundle.

Options:
  --require-promotable   Fail unless the bundle proves execute-mode backend readiness.
  -h, --help             Show this help.

Promotion requires: execute mode, Lenovo keyboard device detected, mode
read-back match, restore read-back match, before/after profiles saved, profile
color-byte evidence or SDK color read-back, all command exits zero,
backend_ready_evidence=true, and no promotion blockers. SDK-backed color
evidence must also prove restored colors match the before snapshot.
EOF
}

require_promotable=0
target=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --require-promotable)
      require_promotable=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [[ -n "$target" ]]; then
        echo "unexpected extra argument: $1" >&2
        usage >&2
        exit 2
      fi
      target="$1"
      shift
      ;;
  esac
done

if [[ -z "$target" ]]; then
  echo "bundle directory or JSON path is required" >&2
  usage >&2
  exit 2
fi

json_path="$target"
if [[ -d "$target" ]]; then
  json_path="$target/openrgb-keyboard-rgb-bridge-evidence.json"
fi
if [[ ! -f "$json_path" ]]; then
  echo "missing OpenRGB bridge evidence JSON: $json_path" >&2
  exit 1
fi

python3 - "$json_path" "$require_promotable" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
require_promotable = bool(int(sys.argv[2]))
report = json.loads(path.read_text())
failures = []

def require(condition, message):
    if not condition:
        failures.append(message)

schema = report.get("schema_version")
execute = bool(report.get("execute"))
openrgb = report.get("openrgb") or {}
request = report.get("request") or {}
readback = report.get("readback") or {}
profiles = report.get("profiles") or {}
commands = report.get("commands") or {}
result = report.get("result") or {}
before_profile = profiles.get("before_profile") or {}
after_profile = profiles.get("after_profile") or {}
promotion_blockers = result.get("promotion_blockers") or []
profile_color_readback_supported = bool(
    readback.get("profile_color_readback_supported")
    or after_profile.get("all_requested_colors_found")
)
sdk_color_readback_supported = bool(readback.get("sdk_color_readback_supported"))

require(schema == 1, f"schema_version is {schema!r}, expected 1")
require(bool(openrgb.get("installed")), "OpenRGB is not installed in evidence")
require(bool(request.get("device")), "request.device is missing")
require(bool(request.get("command_preview")), "request.command_preview is missing")
require(bool(readback.get("before_mode")), "readback.before_mode is missing")
require(bool(profiles.get("before_profile_saved")), "before profile was not saved")
require(bool(before_profile.get("saved")), "before_profile.saved is false")
require((before_profile.get("size_bytes") or 0) > 0, "before profile is empty")

if require_promotable:
    require(execute, "bundle is not execute mode")
    require(result.get("status") == "executed", f"result.status is {result.get('status')!r}, expected 'executed'")
    require(bool(readback.get("mode_readback_matches")), "mode read-back did not match requested effect")
    require(bool(readback.get("restore_mode_matches")), "restore read-back did not match previous mode")
    require(bool(readback.get("color_readback_supported")), "color read-back was not proven")
    require(bool(profiles.get("after_profile_saved")), "after profile was not saved")
    require(bool(after_profile.get("saved")), "after_profile.saved is false")
    require((after_profile.get("size_bytes") or 0) > 0, "after profile is empty")
    require(
        profile_color_readback_supported or sdk_color_readback_supported,
        "neither profile bytes nor SDK read-back proved requested colors",
    )
    if profile_color_readback_supported:
        require(bool(after_profile.get("all_requested_colors_found")), "after profile does not contain every requested color byte triplet")
    if sdk_color_readback_supported:
        require(bool(readback.get("sdk_restore_color_matches")), "SDK colors did not restore to the before snapshot")
    for key in ("save_before_exit", "apply_exit", "save_after_exit", "restore_exit"):
        require(commands.get(key) == 0, f"{key} is {commands.get(key)!r}, expected 0")
    require(bool(result.get("backend_ready_evidence")), "backend_ready_evidence is false")
    require(not promotion_blockers, f"promotion blockers remain: {promotion_blockers}")

summary = {
    "status": result.get("status"),
    "device": request.get("device"),
    "before_mode": readback.get("before_mode"),
    "after_mode": readback.get("after_mode"),
    "restored_mode": readback.get("restored_mode"),
    "color_readback_supported": bool(readback.get("color_readback_supported")),
    "profile_color_readback_supported": profile_color_readback_supported,
    "sdk_color_readback_supported": sdk_color_readback_supported,
    "sdk_restore_color_matches": bool(readback.get("sdk_restore_color_matches")),
    "backend_ready_evidence": bool(result.get("backend_ready_evidence")),
    "promotion_blockers": len(promotion_blockers),
}
print(json.dumps(summary, sort_keys=True))

if failures:
    for failure in failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    raise SystemExit(1)

print("openrgb_bridge_evidence_review=pass")
PY
