#!/usr/bin/env bash
# Review a bundle produced by scripts/capture-write-validation-report.sh
# Usage: scripts/review-write-validation-bundle.sh <bundle-dir>
# Example:
#   scripts/review-write-validation-bundle.sh target/validation/82wm-live-platform_profile

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/review-write-validation-bundle.sh <bundle-dir>

Pretty-print metadata, per-control statuses, and platform_profile details
from validation-report.json. Requires jq.

Example:
  scripts/review-write-validation-bundle.sh target/validation/82wm-live-platform_profile
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ -z "${1:-}" ]]; then
  usage >&2
  exit 2
fi

BUNDLE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"

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
jq '.metadata | {mode, target_bus_mode, sysfs_root, execute_only, bus_address}' "$BUNDLE/validation-report.json"
echo

echo "=== per-control status ==="
jq -r '.controls[] | "\(.control_id)\t\(.status)\tavailable=\(.available)"' "$BUNDLE/validation-report.json" | column -t -s $'\t' || true
echo

echo "=== platform_profile row (if present) ==="
jq '.controls[] | select(.control_id == "platform_profile")' "$BUNDLE/validation-report.json"
echo

echo "=== platform_profile artifact paths ==="
jq -r '.controls[] | select(.control_id == "platform_profile") |
  "plan_file: \(.plan_file // "none")\nset_file: \(.set_file // "none")\nrevert_file: \(.revert_file // "none")"' "$BUNDLE/validation-report.json"
echo

echo "Done. To archive: zip -r bundle.zip \"$(basename "$BUNDLE")\"" && echo "  (run from the parent directory of the bundle)"
