#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/check-keyboard-rgb-openrgb.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fake_openrgb="$tmp/openrgb"
cat >"$fake_openrgb" <<'EOF'
#!/usr/bin/env bash
cat <<'OUT'
0: Lenovo 5 2023
  Type:           Laptop
  Description:    Lenovo 4-Zone device
  Modes: [Direct] Breathing 'Rainbow Wave' 'Spectrum Cycle'
  Zones: Keyboard
  LEDs: 'Left side' 'Left center' 'Right center' 'Right side'
OUT
EOF
chmod +x "$fake_openrgb"

mkdir -p "$tmp/dev"
: >"$tmp/dev/i2c-0"
: >"$tmp/dev/hidraw2"

"$script" --output "$tmp/out" --openrgb-bin "$fake_openrgb" --dev-root "$tmp/dev" >/dev/null

python3 - "$tmp/out/openrgb-keyboard-rgb-readiness.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if not report["openrgb"]["installed"]:
    raise SystemExit("fake OpenRGB should be detected")
if not report["openrgb"]["detects_lenovo_keyboard_rgb"]:
    raise SystemExit("Lenovo keyboard RGB device should be parsed")
if not report["ratvantage"]["openrgb_backend_candidate"]:
    raise SystemExit("OpenRGB device should be a RatVantage backend candidate")
if report["ratvantage"]["backend_ready"]:
    raise SystemExit("backend_ready must stay false until RatVantage read-back/reset exists")
access = report["linux_access"]
for field in ("user_in_i2c_group", "missing_access", "setup_recommended", "setup_command"):
    if field not in access:
        raise SystemExit(f"linux access report is missing {field}")
if access["setup_command"] != "ratvantage-setup-keyboard-rgb-openrgb-access":
    raise SystemExit(f"unexpected setup command: {access['setup_command']}")
for node in access["i2c_nodes"] + access["hidraw_nodes"]:
    if "group" not in node or "user" not in node:
        raise SystemExit(f"node access entry is missing names: {node}")
device = report["openrgb"]["keyboard_rgb_devices"][0]
if device["description"] != "Lenovo 4-Zone device":
    raise SystemExit(f"unexpected parsed device: {device}")
PY

grep -q "detects_lenovo_keyboard_rgb: \`True\`" "$tmp/out/openrgb-keyboard-rgb-readiness.md"
grep -q "setup_recommended:" "$tmp/out/openrgb-keyboard-rgb-readiness.md"
echo "check-keyboard-rgb-openrgb tests passed"
