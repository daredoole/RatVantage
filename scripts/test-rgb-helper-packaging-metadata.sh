#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
spec="$repo_root/packaging/rpm/legion-control.spec"
user_install="$repo_root/scripts/install-user-session.sh"

python3 - "$spec" "$user_install" <<'PY'
from pathlib import Path
import sys

spec = Path(sys.argv[1]).read_text()
user_install = Path(sys.argv[2]).read_text()

helpers = {
    "ratvantage-check-keyboard-rgb-openrgb": True,
    "ratvantage-capture-keyboard-rgb-evidence": True,
    "ratvantage-compare-keyboard-rgb-evidence": True,
    "ratvantage-setup-keyboard-rgb-openrgb-access": True,
    "ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence": True,
    "ratvantage-review-keyboard-rgb-openrgb-bridge-evidence": True,
    "ratvantage-keyboard-rgb-openrgb-bridge-status": True,
    "ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence": True,
    "ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence": True,
    "ratvantage-openrgb-keyboard-rgb-sdk-helper": True,
    "ratvantage-openrgb-sdk-server": True,
    "ratvantage-capture-compatibility-bundle": True,
    "ratvantage-capture-gpu-mux-evidence": True,
}

if "%package helpers" not in spec or "%files helpers" not in spec:
    raise SystemExit("RPM spec must define a helpers subpackage")
if "Requires:       %{name}-helpers = %{version}-%{release}" not in spec:
    raise SystemExit("UI package must require the helpers subpackage")
if "no setuid helper is packaged" not in spec:
    raise SystemExit("helpers package description must preserve the no-setuid policy")
if "4755" in spec or "setuid" in spec.replace("no setuid helper is packaged", ""):
    raise SystemExit("RPM spec must not package a setuid helper")

for helper, required_in_user_install in helpers.items():
    if helper not in spec:
        raise SystemExit(f"RPM spec is missing {helper}")
    if required_in_user_install and helper not in user_install:
        raise SystemExit(f"user-session install is missing {helper}")

print("rgb-helper-packaging metadata tests passed")
PY
