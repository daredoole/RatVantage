#!/usr/bin/env bash
# Review a bundle produced by scripts/capture-write-validation-report.sh
# Usage: scripts/review-write-validation-bundle.sh [options] <bundle-dir>
# Example:
#   scripts/review-write-validation-bundle.sh target/validation/82wm-live-platform_profile

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/review-write-validation-bundle.sh <bundle-dir>

Pretty-print metadata, per-control statuses, and selected control details from
validation-report.json. Requires jq.

Options:
  --control <control_id>
      Print the full JSON row and artifact paths for a specific control.
      May be passed more than once. Defaults to the bundle's execute_only
      control, or platform_profile when no execute_only filter exists.
  --require-control <control_id=status>
      Fail unless the report contains control_id with exactly this status.
      May be passed more than once.
  --require-mode <plan-only|execute>
      Fail unless metadata.mode matches.

Example:
  scripts/review-write-validation-bundle.sh target/validation/82wm-live-platform_profile

  scripts/review-write-validation-bundle.sh \
    --require-mode execute \
    --require-control cpu_boost=pass \
    target/validation/82wm-live-cpu_boost
EOF
}

detail_controls=()
required_controls=()
required_mode=""
bundle_arg=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --control)
      detail_controls+=("${2:?missing value for --control}")
      shift 2
      ;;
    --require-control)
      required_controls+=("${2:?missing value for --require-control}")
      shift 2
      ;;
    --require-mode)
      required_mode="${2:?missing value for --require-mode}"
      shift 2
      ;;
    --*)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -n "$bundle_arg" ]]; then
        echo "only one bundle directory may be provided" >&2
        usage >&2
        exit 2
      fi
      bundle_arg="$1"
      shift
      ;;
  esac
done

if [[ -z "$bundle_arg" ]]; then
  usage >&2
  exit 2
fi

if [[ -n "$required_mode" && "$required_mode" != "plan-only" && "$required_mode" != "execute" ]]; then
  echo "--require-mode must be plan-only or execute" >&2
  exit 2
fi

BUNDLE="$(cd "$(dirname "$bundle_arg")" && pwd)/$(basename "$bundle_arg")"

if [[ ! -d "$BUNDLE" ]]; then
  echo "not a directory: $BUNDLE" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required (e.g. sudo dnf install -y jq)" >&2
  exit 1
fi

if [[ ! -f "$BUNDLE/validation-report.json" ]]; then
  echo "missing $BUNDLE/validation-report.json" >&2
  exit 2
fi

REPORT="$BUNDLE/validation-report.json"

if [[ -n "$required_mode" ]]; then
  actual_mode="$(jq -r '.metadata.mode // ""' "$REPORT")"
  if [[ "$actual_mode" != "$required_mode" ]]; then
    echo "required metadata.mode=$required_mode but report has $actual_mode" >&2
    exit 1
  fi
fi

for requirement in "${required_controls[@]}"; do
  if [[ "$requirement" != *=* ]]; then
    echo "invalid --require-control value; expected control_id=status" >&2
    exit 2
  fi
  control_id="${requirement%%=*}"
  expected_status="${requirement#*=}"
  actual_status="$(jq -r --arg control_id "$control_id" '
    (.controls[]? | select(.control_id == $control_id) | .status) // empty
  ' "$REPORT")"
  if [[ -z "$actual_status" ]]; then
    echo "required control $control_id is missing from validation-report.json" >&2
    exit 1
  fi
  if [[ "$actual_status" != "$expected_status" ]]; then
    echo "required control $control_id status=$expected_status but report has $actual_status" >&2
    exit 1
  fi
done

if [[ ${#detail_controls[@]} -eq 0 ]]; then
  execute_only="$(jq -r '.metadata.execute_only // empty' "$REPORT")"
  if [[ -n "$execute_only" ]]; then
    detail_controls+=("$execute_only")
  else
    detail_controls+=("platform_profile")
  fi
fi

echo "=== Bundle: $BUNDLE ==="
echo
ls -la "$BUNDLE"
echo
if [[ -d "$BUNDLE/steps" ]]; then
  echo "=== steps/ (first 30 names) ==="
  ls -1 "$BUNDLE/steps" | head -30
  echo
fi

if [[ -f "$BUNDLE/validation-report.md" ]]; then
  echo "=== validation-report.md (first 80 lines) ==="
  sed -n '1,80p' "$BUNDLE/validation-report.md"
  echo
fi

echo "=== metadata (mode, bus, execute_only) ==="
jq '.metadata | {mode, target_bus_mode, sysfs_root, execute_only, bus_address, seed_hardware_profile_count, seed_hardware_profile_trigger_count}' "$REPORT"
echo

echo "=== per-control status ==="
jq -r '.controls[] | "\(.control_id)\t\(.status)\tavailable=\(.available)"' "$REPORT" | column -t -s $'\t' || true
echo

for control_id in "${detail_controls[@]}"; do
  echo "=== ${control_id} row (if present) ==="
  jq --arg control_id "$control_id" '.controls[] | select(.control_id == $control_id)' "$REPORT"
  echo

  echo "=== ${control_id} artifact paths ==="
  jq -r --arg control_id "$control_id" '.controls[] | select(.control_id == $control_id) |
    "plan_file: \(.plan_file // "none")\nset_file: \(.set_file // "none")\nrevert_file: \(.revert_file // "none")\nbefore_overview_file: \(.before_overview_file // "none")\nafter_overview_file: \(.after_overview_file // "none")\nreverted_overview_file: \(.reverted_overview_file // "none")"' "$REPORT"
  echo
done

base="$(basename "$BUNDLE")"
echo "To archive:"
echo "  scripts/archive-validation-bundle.sh \"$BUNDLE\""
echo "Or manually:"
echo "  zip -r \"${base}.zip\" \"$BUNDLE\""
