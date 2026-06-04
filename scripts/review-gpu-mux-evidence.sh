#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/review-gpu-mux-evidence.sh [options] <bundle-dir|summary-json>

Review read-only GPU mux/session-restart evidence.

Options:
  --require-session-restart-confirmed
      Fail unless compare-summary.json proves a display-manager/session restart
      changed mode and GPU driver state. This does not approve runtime switching.
  -h, --help
      Show this help.

Accepted inputs:
  - bundle root with compare-summary.json
  - bundle root with pre/mux-summary.json
  - direct compare-summary.json or mux-summary.json path

The review is read-only and never performs GPU writes.
EOF
}

require_session_restart_confirmed=0
target=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --require-session-restart-confirmed)
      require_session_restart_confirmed=1
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
  echo "bundle directory or summary JSON path is required" >&2
  usage >&2
  exit 2
fi

json_path="$target"
if [[ -d "$target" ]]; then
  if [[ -f "$target/compare-summary.json" ]]; then
    json_path="$target/compare-summary.json"
  elif [[ -f "$target/pre/mux-summary.json" ]]; then
    json_path="$target/pre/mux-summary.json"
  elif [[ -f "$target/mux-summary.json" ]]; then
    json_path="$target/mux-summary.json"
  fi
fi

if [[ ! -f "$json_path" ]]; then
  echo "missing GPU mux evidence summary JSON: $json_path" >&2
  exit 1
fi

python3 - "$json_path" "$require_session_restart_confirmed" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
require_session_restart_confirmed = bool(int(sys.argv[2]))
report = json.loads(path.read_text())
failures = []

def require(condition, message):
    if not condition:
        failures.append(message)

schema = report.get("schema_version")
read_only = bool(report.get("read_only"))
is_compare = "session_restart_switching_confirmed" in report

require(schema == 1, f"schema_version is {schema!r}, expected 1")
require(read_only, "read_only marker is not true")

if is_compare:
    require(bool(report.get("pre_mode")), "pre_mode is missing")
    require(bool(report.get("post_mode")), "post_mode is missing")
    if require_session_restart_confirmed:
        require(bool(report.get("mode_changed")), "mode did not change")
        require(bool(report.get("kernel_modules_changed")), "kernel modules did not change")
        require(
            bool(report.get("session_restart_switching_confirmed")),
            "session-restart switching was not confirmed",
        )
    summary = {
        "kind": "compare",
        "pre_mode": report.get("pre_mode"),
        "post_mode": report.get("post_mode"),
        "mode_changed": bool(report.get("mode_changed")),
        "kernel_modules_changed": bool(report.get("kernel_modules_changed")),
        "drm_topology_changed": bool(report.get("drm_topology_changed")),
        "nvidia_pci_state_changed": bool(report.get("nvidia_pci_state_changed")),
        "session_restart_switching_confirmed": bool(
            report.get("session_restart_switching_confirmed")
        ),
    }
else:
    require(report.get("phase") in {"mux-only", "pre", "post"}, f"unexpected phase {report.get('phase')!r}")
    require(bool(report.get("current_mode")), "current_mode is missing")
    require(isinstance(report.get("drm_provider_count"), int), "drm_provider_count is missing")
    if require_session_restart_confirmed:
        require(False, "compare-summary.json is required for session-restart confirmation")
    summary = {
        "kind": "phase",
        "phase": report.get("phase"),
        "current_mode": report.get("current_mode"),
        "drm_provider_count": report.get("drm_provider_count"),
        "nvidia_pci_entry_count": report.get("nvidia_pci_entry_count"),
        "d3cold_indicator_count": report.get("d3cold_indicator_count"),
        "first_d3cold_indicator": report.get("first_d3cold_indicator"),
    }

print(json.dumps(summary, sort_keys=True))

if failures:
    for failure in failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    raise SystemExit(1)

print("gpu_mux_evidence_review=pass")
PY
