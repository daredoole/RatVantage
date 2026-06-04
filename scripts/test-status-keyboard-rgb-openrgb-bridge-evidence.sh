#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

make_bundle() {
  local slug="$1"
  local execute="$2"
  local status="$3"
  local ready="$4"
  local color="$5"
  local blockers="$6"
  mkdir -p "$tmp/$slug"
  python3 - "$tmp/$slug/openrgb-keyboard-rgb-bridge-evidence.json" "$execute" "$status" "$ready" "$color" "$blockers" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
execute = sys.argv[2] == "true"
status = sys.argv[3]
ready = sys.argv[4] == "true"
color = sys.argv[5] == "true"
blockers = json.loads(sys.argv[6])
path.write_text(json.dumps({
    "schema_version": 1,
    "execute": execute,
    "request": {"device": "Lenovo 5 2023"},
    "readback": {
        "before_mode": "Direct",
        "after_mode": "Breathing" if execute else None,
        "restored_mode": "Direct" if execute else None,
        "mode_readback_matches": execute,
        "restore_mode_matches": execute,
        "color_readback_supported": color,
    },
    "result": {
        "status": status,
        "backend_ready_evidence": ready,
        "promotion_blockers": blockers,
    },
}, indent=2) + "\n")
PY
}

make_readiness() {
  local ready="$1"
  local out="$tmp/readiness/openrgb-keyboard-rgb-readiness.json"
  mkdir -p "$(dirname "$out")"
  python3 - "$out" "$ready" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
ready = sys.argv[2] == "true"
path.write_text(json.dumps({
    "schema_version": 1,
    "openrgb": {
        "installed": True,
        "detects_lenovo_keyboard_rgb": ready,
    },
    "linux_access": {
        "user": "ratvantage-test",
        "user_in_i2c_group": False,
        "has_i2c_rw_access": ready,
        "has_hidraw_rw_access": ready,
        "setup_recommended": not ready,
        "missing_access": [] if ready else ["read/write access to /dev/i2c-*"],
    },
    "ratvantage": {
        "openrgb_backend_candidate": ready,
        "backend_ready": False,
        "write_support_claimed": False,
    },
}, indent=2) + "\n")
PY
}

make_sdk() {
  local status="$1"
  local connected="$2"
  local keyboard="$3"
  local readback="$4"
  local blockers="$5"
  local out="$tmp/sdk/openrgb-keyboard-rgb-sdk-evidence.json"
  mkdir -p "$(dirname "$out")"
  python3 - "$out" "$status" "$connected" "$keyboard" "$readback" "$blockers" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
status = sys.argv[2]
connected = sys.argv[3] == "true"
keyboard = sys.argv[4] == "true"
readback = sys.argv[5] == "true"
blockers = json.loads(sys.argv[6])
controller = {
    "name": "Lenovo 5 2023",
    "active_mode": "Breathing" if readback else None,
    "colors": ["#333333", "#333333"] if readback else [],
} if keyboard else None
path.write_text(json.dumps({
    "schema_version": 1,
    "sdk": {
        "connected": connected,
        "server_started": True,
        "protocol_version": 5,
    },
    "controllers": [controller] if controller else [],
    "keyboard": {
        "detected": keyboard,
        "controller": controller,
    },
    "result": {
        "status": status,
        "read_back_supported": readback,
        "promotion_blockers": blockers,
    },
}, indent=2) + "\n")
PY
}

make_sdk_write() {
  local ready="$1"
  local mode_ready="$2"
  local blockers="$3"
  local out="$tmp/sdk-write/openrgb-keyboard-rgb-sdk-write-evidence.json"
  mkdir -p "$(dirname "$out")"
  python3 - "$out" "$ready" "$mode_ready" "$blockers" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
ready = sys.argv[2] == "true"
mode_ready = sys.argv[3] == "true"
blockers = json.loads(sys.argv[4])
path.write_text(json.dumps({
    "schema_version": 1,
    "execute": True,
    "request": {"mode": "Breathing" if mode_ready else None, "colors": ["FF0000", "00FF00"]},
    "readback": {
        "after": {
            "active_mode": "Breathing" if mode_ready else "Direct",
            "colors": ["FF0000", "00FF00"] if ready else ["000000", "000000"],
        },
        "mode_readback_matches": mode_ready,
        "color_readback_matches": ready,
        "restore_color_matches": ready,
        "restore_mode_matches": ready,
    },
    "result": {
        "status": "executed",
        "sdk_write_ready_evidence": ready,
        "promotion_blockers": blockers,
    },
}, indent=2) + "\n")
PY
}

missing_output="$("$script" --root "$tmp")"
grep -q "dry_run=missing" <<<"$missing_output"
grep -q "execute=missing" <<<"$missing_output"
grep -q "readiness=missing" <<<"$missing_output"
grep -q "sdk=missing" <<<"$missing_output"
grep -q "sdk_write=missing" <<<"$missing_output"
grep -q "next_action=capture or fix OpenRGB readiness before execute evidence" <<<"$missing_output"

make_readiness false
blocked_output="$("$script" --root "$tmp" --readiness "$tmp/readiness")"
grep -q "readiness=present" <<<"$blocked_output"
grep -q "ready_for_execute=false" <<<"$blocked_output"
grep -q "next_action=capture or fix OpenRGB readiness before execute evidence" <<<"$blocked_output"

make_readiness true
ready_output="$("$script" --root "$tmp" --readiness "$tmp/readiness")"
grep -q "readiness=present" <<<"$ready_output"
grep -q "ready_for_execute=true" <<<"$ready_output"
grep -q "next_action=run dry-run evidence capture" <<<"$ready_output"

make_bundle keyboard-rgb-openrgb-bridge-dry-run false dry_run false false '["dry-run"]'
dry_output="$("$script" --root "$tmp" --readiness "$tmp/readiness")"
grep -q "dry_run=present status=dry_run" <<<"$dry_output"
grep -q "execute=missing" <<<"$dry_output"
grep -q "next_action=operator may run execute evidence capture" <<<"$dry_output"

make_bundle keyboard-rgb-openrgb-bridge-execute true executed false false '["mode mismatch"]'
sdk_missing_output="$("$script" --root "$tmp" --readiness "$tmp/readiness")"
grep -q "execute=present status=executed" <<<"$sdk_missing_output"
grep -q "promotable=false" <<<"$sdk_missing_output"
grep -q "sdk=missing" <<<"$sdk_missing_output"
grep -q "next_action=capture OpenRGB SDK read-back evidence" <<<"$sdk_missing_output"

make_sdk keyboard_not_found true false false '["OpenRGB SDK did not report a Lenovo keyboard controller"]'
sdk_blocked_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk")"
grep -q "sdk=present status=keyboard_not_found" <<<"$sdk_blocked_output"
grep -q "controllers=0" <<<"$sdk_blocked_output"
grep -q "read_back=false" <<<"$sdk_blocked_output"
grep -q "next_action=review execute bundle and SDK read-back failures before promotion" <<<"$sdk_blocked_output"

make_sdk ok true true true '[]'
sdk_ready_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk")"
grep -q "sdk=present status=ok" <<<"$sdk_ready_output"
grep -q "controllers=1" <<<"$sdk_ready_output"
grep -q "read_back=true" <<<"$sdk_ready_output"
grep -q "promotable=true" <<<"$sdk_ready_output"
grep -q "sdk_write=missing" <<<"$sdk_ready_output"
grep -q "next_action=find an OpenRGB apply path that changes SDK mode/color read-back" <<<"$sdk_ready_output"

make_sdk_write true false '[]'
sdk_write_ready_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk" --sdk-write "$tmp/sdk-write")"
grep -q "sdk_write=present status=executed" <<<"$sdk_write_ready_output"
grep -q "mode_readback=false" <<<"$sdk_write_ready_output"
grep -q "color_write_ready=true" <<<"$sdk_write_ready_output"
grep -q "promotable=false" <<<"$sdk_write_ready_output"
grep -q "next_action=prove OpenRGB SDK mode write/read-back before daemon promotion" <<<"$sdk_write_ready_output"

make_sdk_write true true '[]'
sdk_write_promotable_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk" --sdk-write "$tmp/sdk-write")"
grep -q "sdk_write=present status=executed" <<<"$sdk_write_promotable_output"
grep -q "mode_readback=true" <<<"$sdk_write_promotable_output"
grep -q "color_write_ready=true" <<<"$sdk_write_promotable_output"
grep -q "promotable=true" <<<"$sdk_write_promotable_output"
grep -q "next_action=wire real OpenRGB SDK helper and daemon policy gates" <<<"$sdk_write_promotable_output"

make_bundle keyboard-rgb-openrgb-bridge-execute true executed true true '[]'
promoted_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk" --sdk-write "$tmp/sdk-write")"
grep -q "execute=present status=executed" <<<"$promoted_output"
grep -q "promotable=true" <<<"$promoted_output"
grep -q "next_action=promote only after production backend policy gates are added" <<<"$promoted_output"

json_output="$("$script" --root "$tmp" --readiness "$tmp/readiness" --sdk "$tmp/sdk" --sdk-write "$tmp/sdk-write" --json)"
python3 - <<'PY' "$json_output"
import json
import sys
report = json.loads(sys.argv[1])
if not report["execute"]["promotable"]:
    raise SystemExit("execute bundle should be promotable in JSON output")
if not report["readiness"]["ready_for_execute_evidence"]:
    raise SystemExit("readiness should allow execute evidence in JSON output")
if not report["sdk"]["promotable"]:
    raise SystemExit("SDK evidence should be promotable in JSON output")
if not report["sdk_write"]["promotable"]:
    raise SystemExit("SDK write mode/color evidence should be promotable in JSON output")
if not report["sdk_write"]["mode_readback_matches"]:
    raise SystemExit("SDK write evidence should include mode read-back in JSON output")
PY

echo "status-keyboard-rgb-openrgb-bridge-evidence tests passed"
