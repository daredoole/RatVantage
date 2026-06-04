#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/install-user-session.sh"

grep -q 'ratvantage-check-keyboard-rgb-openrgb' "$script"
grep -q 'ratvantage-capture-keyboard-rgb-evidence' "$script"
grep -q 'ratvantage-compare-keyboard-rgb-evidence' "$script"
grep -q 'ratvantage-capture-compatibility-bundle' "$script"
grep -q 'ratvantage-capture-gpu-mux-evidence' "$script"
grep -q 'ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence' "$script"
grep -q 'ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence' "$script"
grep -q 'ratvantage-openrgb-keyboard-rgb-sdk-helper' "$script"
grep -q 'ratvantage-openrgb-sdk-server' "$script"
grep -q 'legion-probe' "$script"
grep -q 'ratvantage-setup-keyboard-rgb-openrgb-access' "$script"
grep -q 'sudo -n "$root_helper"' "$script"
grep -q '\.local/libexec/ratvantage' "$script"
grep -q 'compatibility/RGB access/evidence helpers' "$script"

echo "install-user-session metadata tests passed"
