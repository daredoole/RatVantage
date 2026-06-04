#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reviewer="$repo_root/scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write_bundle() {
  local dir="$1"
  local execute="$2"
  local status="$3"
  local backend_ready="$4"
  local color_ready="$5"
  local blockers_json="$6"
  mkdir -p "$dir"
  python3 - "$dir/openrgb-keyboard-rgb-bridge-evidence.json" "$execute" "$status" "$backend_ready" "$color_ready" "$blockers_json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
execute = sys.argv[2] == "true"
status = sys.argv[3]
backend_ready = sys.argv[4] == "true"
color_ready = sys.argv[5] == "true"
blockers = json.loads(sys.argv[6])
report = {
    "schema_version": 1,
    "generated_at_utc": "2026-06-03T00:00:00+00:00",
    "execute": execute,
    "openrgb": {"installed": True, "path": "/usr/bin/openrgb"},
    "request": {
        "device": "Lenovo 5 2023",
        "effect": "Breathing",
        "brightness": 75,
        "speed": 30,
        "colors": "FF0000,00FF00,0000FF,FFFFFF",
        "command_preview": "openrgb --device 'Lenovo 5 2023' --mode Breathing --brightness 75 --speed 30 --color FF0000,00FF00,0000FF,FFFFFF",
    },
    "readback": {
        "before_mode": "Direct",
        "after_mode": "Breathing" if execute else None,
        "restored_mode": "Direct" if execute else None,
        "mode_readback_matches": execute,
        "restore_mode_matches": execute,
        "color_readback_supported": color_ready,
    },
    "profiles": {
        "before_profile_saved": True,
        "after_profile_saved": execute,
        "before_profile": {"saved": True, "size_bytes": 477, "all_requested_colors_found": False},
        "after_profile": {"saved": execute, "size_bytes": 512 if execute else 0, "all_requested_colors_found": color_ready},
    },
    "commands": {
        "save_before_exit": 0,
        "apply_exit": 0 if execute else None,
        "save_after_exit": 0 if execute else None,
        "restore_exit": 0 if execute else None,
    },
    "result": {
        "status": status,
        "backend_ready_evidence": backend_ready,
        "promotion_blockers": blockers,
    },
}
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY
}

write_bundle "$tmp/dry" false dry_run false false '["dry-run only"]'
write_bundle "$tmp/pass" true executed true true '[]'
write_bundle "$tmp/no_color" true executed false false '["color missing"]'
write_bundle "$tmp/sdk_pass" true executed true true '[]'
python3 - "$tmp/sdk_pass/openrgb-keyboard-rgb-bridge-evidence.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
report["readback"]["profile_color_readback_supported"] = False
report["readback"]["sdk_color_readback_supported"] = True
report["readback"]["sdk_restore_color_matches"] = True
report["profiles"]["after_profile"]["all_requested_colors_found"] = False
report["sdk_readback"] = {
    "before": {"active_mode": "Direct", "colors": ["000000", "000000", "000000", "000000"]},
    "after": {"active_mode": "Breathing", "colors": ["FF0000", "00FF00", "0000FF", "FFFFFF"]},
    "restored": {"active_mode": "Direct", "colors": ["000000", "000000", "000000", "000000"]},
}
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY

"$reviewer" "$tmp/dry" >/tmp/ratvantage-openrgb-bridge-review-dry.txt

if "$reviewer" --require-promotable "$tmp/dry" >/tmp/ratvantage-openrgb-bridge-review-dry-promote.txt 2>&1; then
  echo "dry-run bundle should not satisfy --require-promotable" >&2
  exit 1
fi
grep -q "bundle is not execute mode" /tmp/ratvantage-openrgb-bridge-review-dry-promote.txt

"$reviewer" --require-promotable "$tmp/pass" >/tmp/ratvantage-openrgb-bridge-review-pass.txt
grep -q "openrgb_bridge_evidence_review=pass" /tmp/ratvantage-openrgb-bridge-review-pass.txt

"$reviewer" --require-promotable "$tmp/sdk_pass" >/tmp/ratvantage-openrgb-bridge-review-sdk-pass.txt
grep -q '"sdk_color_readback_supported": true' /tmp/ratvantage-openrgb-bridge-review-sdk-pass.txt
grep -q "openrgb_bridge_evidence_review=pass" /tmp/ratvantage-openrgb-bridge-review-sdk-pass.txt

if "$reviewer" --require-promotable "$tmp/no_color" >/tmp/ratvantage-openrgb-bridge-review-no-color.txt 2>&1; then
  echo "missing color evidence should not satisfy --require-promotable" >&2
  exit 1
fi
grep -q "color read-back was not proven" /tmp/ratvantage-openrgb-bridge-review-no-color.txt

echo "review-keyboard-rgb-openrgb-bridge-evidence tests passed"
