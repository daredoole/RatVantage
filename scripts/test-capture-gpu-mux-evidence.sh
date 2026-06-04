#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-gpu-mux-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

out="$tmp/gpu-mux"
mkdir -p "$out/pre" "$out/post"

cat >"$out/pre/envycontrol-mode.txt" <<'EOF'
integrated
EOF
cat >"$out/post/envycontrol-mode.txt" <<'EOF'
hybrid
EOF
cat >"$out/pre/lsmod.txt" <<'EOF'
amdgpu 1 0
EOF
cat >"$out/post/lsmod.txt" <<'EOF'
amdgpu 1 0
nvidia 1 0
EOF
cat >"$out/pre/drm-providers.txt" <<'EOF'
card0=amdgpu
EOF
cat >"$out/post/drm-providers.txt" <<'EOF'
card0=amdgpu
card1=nvidia
EOF
cat >"$out/pre/nvidia-pci-enable.txt" <<'EOF'
unknown
EOF
cat >"$out/post/nvidia-pci-enable.txt" <<'EOF'
0000:01:00.0 enable=1 runtime=active
EOF

"$script" --phase compare --output "$out" >/tmp/ratvantage-gpu-mux-compare.txt

grep -q "SESSION-RESTART SWITCHING: CONFIRMED WORKING" "$out/compare-report.txt"
grep -q '"read_only": true' "$out/compare-summary.json"
grep -q '"mode_changed": true' "$out/compare-summary.json"
grep -q '"kernel_modules_changed": true' "$out/compare-summary.json"
grep -q '"drm_topology_changed": true' "$out/compare-summary.json"
grep -q '"nvidia_pci_state_changed": true' "$out/compare-summary.json"
grep -q '"session_restart_switching_confirmed": true' "$out/compare-summary.json"

partial="$tmp/partial"
mkdir -p "$partial/pre" "$partial/post"
echo "hybrid" >"$partial/pre/envycontrol-mode.txt"
echo "hybrid" >"$partial/post/envycontrol-mode.txt"
"$script" --phase compare --output "$partial" >/tmp/ratvantage-gpu-mux-partial.txt

grep -q "SESSION-RESTART SWITCHING: NOT CONFIRMED" "$partial/compare-report.txt"
grep -q '"mode_changed": false' "$partial/compare-summary.json"
grep -q '"session_restart_switching_confirmed": false' "$partial/compare-summary.json"

mux_only="$tmp/mux-only"
"$script" --phase mux-only --output "$mux_only" >/tmp/ratvantage-gpu-mux-only.txt
test -f "$mux_only/pre/mux-summary.json"
grep -q '"read_only": true' "$mux_only/pre/mux-summary.json"
grep -q '"phase": "mux-only"' "$mux_only/pre/mux-summary.json"
grep -q '"current_mode"' "$mux_only/pre/mux-summary.json"
grep -q '"drm_provider_count"' "$mux_only/pre/mux-summary.json"

"$script" --help >/tmp/ratvantage-gpu-mux-help.txt
grep -q "mux-only" /tmp/ratvantage-gpu-mux-help.txt
grep -q "This script is READ-ONLY" /tmp/ratvantage-gpu-mux-help.txt

echo "capture-gpu-mux-evidence tests passed"
