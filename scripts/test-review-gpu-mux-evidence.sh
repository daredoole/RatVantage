#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reviewer="$repo_root/scripts/review-gpu-mux-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/mux-only/pre" "$tmp/compare-pass" "$tmp/compare-fail"

cat >"$tmp/mux-only/pre/mux-summary.json" <<'JSON'
{
  "schema_version": 1,
  "read_only": true,
  "phase": "mux-only",
  "current_mode": "integrated",
  "drm_provider_count": 2,
  "nvidia_pci_entry_count": 0,
  "d3cold_indicator_count": 1,
  "first_d3cold_indicator": "0000:01:00.0  driver=nvidia  d3cold_allowed=1"
}
JSON

cat >"$tmp/compare-pass/compare-summary.json" <<'JSON'
{
  "schema_version": 1,
  "read_only": true,
  "pre_mode": "integrated",
  "post_mode": "hybrid",
  "mode_changed": true,
  "pre_modules": "amdgpu",
  "post_modules": "amdgpu nvidia",
  "kernel_modules_changed": true,
  "pre_drm_providers": "card0=amdgpu|",
  "post_drm_providers": "card0=amdgpu|card1=nvidia|",
  "drm_topology_changed": true,
  "pre_nvidia_pci": "unknown",
  "post_nvidia_pci": "0000:01:00.0 enable=1 runtime=active",
  "nvidia_pci_state_changed": true,
  "session_restart_switching_confirmed": true
}
JSON

cat >"$tmp/compare-fail/compare-summary.json" <<'JSON'
{
  "schema_version": 1,
  "read_only": true,
  "pre_mode": "hybrid",
  "post_mode": "hybrid",
  "mode_changed": false,
  "kernel_modules_changed": false,
  "drm_topology_changed": false,
  "nvidia_pci_state_changed": false,
  "session_restart_switching_confirmed": false
}
JSON

"$reviewer" "$tmp/mux-only" >/tmp/ratvantage-gpu-mux-review-phase.txt
grep -q '"kind": "phase"' /tmp/ratvantage-gpu-mux-review-phase.txt
grep -q "gpu_mux_evidence_review=pass" /tmp/ratvantage-gpu-mux-review-phase.txt

if "$reviewer" --require-session-restart-confirmed "$tmp/mux-only" >/tmp/ratvantage-gpu-mux-review-phase-strict.txt 2>&1; then
  echo "phase-only evidence should not satisfy session-restart confirmation" >&2
  exit 1
fi
grep -q "compare-summary.json is required" /tmp/ratvantage-gpu-mux-review-phase-strict.txt

"$reviewer" --require-session-restart-confirmed "$tmp/compare-pass" >/tmp/ratvantage-gpu-mux-review-pass.txt
grep -q '"kind": "compare"' /tmp/ratvantage-gpu-mux-review-pass.txt
grep -q '"session_restart_switching_confirmed": true' /tmp/ratvantage-gpu-mux-review-pass.txt
grep -q "gpu_mux_evidence_review=pass" /tmp/ratvantage-gpu-mux-review-pass.txt

if "$reviewer" --require-session-restart-confirmed "$tmp/compare-fail" >/tmp/ratvantage-gpu-mux-review-fail.txt 2>&1; then
  echo "failed compare evidence should not satisfy session-restart confirmation" >&2
  exit 1
fi
grep -q "session-restart switching was not confirmed" /tmp/ratvantage-gpu-mux-review-fail.txt

"$reviewer" --help >/tmp/ratvantage-gpu-mux-review-help.txt
grep -q "require-session-restart-confirmed" /tmp/ratvantage-gpu-mux-review-help.txt

echo "review-gpu-mux-evidence tests passed"
