#!/usr/bin/env bash
# Zip a bundle produced by scripts/capture-write-validation-report.sh
# Usage: scripts/archive-validation-bundle.sh <bundle-dir> [output.zip]

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/archive-validation-bundle.sh <bundle-dir> [output.zip]

Create a zip of a write-validation bundle. The directory must exist and
contain validation-report.json.

Default output: <parent-of-bundle>/<basename(bundle)>.zip (next to the bundle,
not inside it).

Options:
  -h, --help    Show this help

Requires: zip

Example:
  scripts/archive-validation-bundle.sh target/validation/my-bundle
  scripts/archive-validation-bundle.sh target/validation/my-bundle /tmp/out.zip
EOF
}

case "${1:-}" in
  -h | --help)
    usage
    exit 0
    ;;
esac

if [[ "$#" -lt 1 || "$#" -gt 2 ]]; then
  usage >&2
  exit 2
fi

if ! command -v zip >/dev/null 2>&1; then
  echo "zip is required (e.g. sudo dnf install -y zip)" >&2
  exit 1
fi

bundle_arg="$1"
if [[ ! -d "$bundle_arg" ]]; then
  echo "not a directory: $bundle_arg" >&2
  exit 2
fi

bundle_abs="$(cd "$bundle_arg" && pwd)"
if [[ ! -f "$bundle_abs/validation-report.json" ]]; then
  echo "missing $bundle_abs/validation-report.json" >&2
  exit 2
fi

bundle_base="$(basename "$bundle_abs")"
parent_abs="$(dirname "$bundle_abs")"

if [[ -n "${2:-}" ]]; then
  out_arg="$2"
  if [[ "$out_arg" == /* ]]; then
    out_zip="$out_arg"
  else
    out_zip="$(cd "$(dirname "$out_arg")" && pwd)/$(basename "$out_arg")"
  fi
else
  out_zip="${parent_abs}/${bundle_base}.zip"
fi

(
  cd "$parent_abs"
  zip -r -o "$out_zip" "$bundle_base"
)

echo "wrote $out_zip"
