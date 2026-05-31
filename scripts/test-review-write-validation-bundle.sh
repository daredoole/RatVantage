#!/usr/bin/env bash
# Regression tests for scripts/review-write-validation-bundle.sh gates.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reviewer="$repo_root/scripts/review-write-validation-bundle.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

bundle="$tmp/82wm-live-cpu_boost"
mkdir -p "$bundle"
python3 - "$bundle/validation-report.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
report = {
    "schema_version": 1,
    "metadata": {
        "mode": "execute",
        "target_bus_mode": "system",
        "sysfs_root": "/",
        "execute_only": "cpu_boost",
        "seed_hardware_profile_count": 0,
        "seed_hardware_profile_trigger_count": 0,
    },
    "controls": [
        {
            "control_id": "cpu_boost",
            "status": "pass",
            "available": True,
            "requested": "0",
            "plan_file": "steps/cpu_boost-plan.json",
            "set_file": "steps/cpu_boost-set.json",
            "revert_file": "steps/cpu_boost-revert.json",
        },
        {
            "control_id": "cpu_epp",
            "status": "planned",
            "available": True,
            "requested": "balance_performance",
            "plan_file": "steps/cpu_epp-plan.json",
        },
    ],
}
path.write_text(json.dumps(report, indent=2) + "\n")
PY

"$reviewer" \
  --require-mode execute \
  --require-control cpu_boost=pass \
  "$bundle" >/tmp/ratvantage-review-bundle-pass.txt

if "$reviewer" --require-mode plan-only "$bundle" >/tmp/ratvantage-review-bundle-mode.txt 2>&1; then
  echo "expected --require-mode plan-only to fail against execute bundle" >&2
  exit 1
fi
if ! grep -q "required metadata.mode=plan-only but report has execute" /tmp/ratvantage-review-bundle-mode.txt; then
  echo "expected mode failure to explain the actual execute mode" >&2
  exit 1
fi

if "$reviewer" --require-control cpu_boost=planned "$bundle" >/tmp/ratvantage-review-bundle-status.txt 2>&1; then
  echo "expected wrong --require-control status to fail" >&2
  exit 1
fi
if ! grep -q "required control cpu_boost status=planned but report has pass" /tmp/ratvantage-review-bundle-status.txt; then
  echo "expected control status failure to explain the actual status" >&2
  exit 1
fi

if "$reviewer" --require-control fan_mode=pass "$bundle" >/tmp/ratvantage-review-bundle-missing.txt 2>&1; then
  echo "expected missing --require-control to fail" >&2
  exit 1
fi
if ! grep -q "required control fan_mode is missing" /tmp/ratvantage-review-bundle-missing.txt; then
  echo "expected missing control failure to identify fan_mode" >&2
  exit 1
fi

echo "review-write-validation-bundle tests passed"
