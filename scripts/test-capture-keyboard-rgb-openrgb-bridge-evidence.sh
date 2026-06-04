#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

state="$tmp/mode"
colors_state="$tmp/colors"
printf 'Direct\n' >"$state"
printf '000000,000000,000000,000000\n' >"$colors_state"
fake_openrgb="$tmp/openrgb"
cat >"$fake_openrgb" <<EOF
#!/usr/bin/env bash
set -euo pipefail
state="$state"
colors_state="$colors_state"
if [[ "\${1:-}" == "--noautoconnect" ]]; then
  shift
fi
if [[ "\${1:-}" == "--list-devices" ]]; then
  mode="\$(cat "\$state")"
  cat <<OUT
0: Lenovo 5 2023
  Type:           Laptop
  Description:    Lenovo 4-Zone device
  Modes: [\$mode] Breathing 'Rainbow Wave' 'Spectrum Cycle'
  Zones: Keyboard
  LEDs: 'Left side' 'Left center' 'Right center' 'Right side'
OUT
  exit 0
fi
if [[ "\${1:-}" == "--save-profile" ]]; then
  python3 - "\${2:?missing profile path}.orp" "\$(cat "\$state")" "\$(cat "\$colors_state")" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
mode = sys.argv[2]
colors = sys.argv[3].split(",")
raw = bytearray(f"OPENRGB_PROFILE\\0Lenovo 5 2023\\0Lenovo 4-Zone device\\0{mode}\\0Breathing\\0Keyboard\\0Left side\\0Left center\\0Right center\\0Right side\\0mode={mode}\\ncolors={','.join(colors)}\\n".encode())
for color in colors:
    raw.extend(bytes.fromhex(color))
path.write_bytes(bytes(raw))
PY
  exit 0
fi
if [[ "\${1:-}" == "--profile" ]]; then
  python3 - "\${2:?missing profile path}" "\$state" "\$colors_state" <<'PY'
import pathlib
import re
import sys

raw = pathlib.Path(sys.argv[1]).read_bytes()
mode = re.search(rb"mode=([^\n]+)", raw)
colors = re.search(rb"colors=([^\n]+)", raw)
pathlib.Path(sys.argv[2]).write_text((mode.group(1).decode() if mode else "Direct") + "\n")
pathlib.Path(sys.argv[3]).write_text((colors.group(1).decode() if colors else "000000,000000,000000,000000") + "\n")
PY
  exit 0
fi
if [[ "\${1:-}" == "--device" ]]; then
  while [[ \$# -gt 0 ]]; do
    case "\$1" in
      --mode)
        printf '%s\n' "\${2:?missing mode}" >"\$state"
        shift 2
        ;;
      --color)
        printf '%s\n' "\${2:?missing colors}" >"\$colors_state"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done
  exit 0
fi
echo "unexpected args: \$*" >&2
exit 2
EOF
chmod +x "$fake_openrgb"

"$script" --output "$tmp/dry" --openrgb-bin "$fake_openrgb" --no-sdk-evidence >/dev/null
python3 - "$tmp/dry/openrgb-keyboard-rgb-bridge-evidence.json" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["result"]["status"] != "dry_run":
    raise SystemExit("default run must be dry-run")
if report["result"]["backend_ready_evidence"]:
    raise SystemExit("dry-run must not claim backend readiness")
if report["readback"]["before_mode"] != "Direct":
    raise SystemExit(f"unexpected before mode: {report['readback']}")
if not report["profiles"]["before_profile_saved"]:
    raise SystemExit("dry-run should save the current profile for read-only evidence")
PY

"$script" --output "$tmp/execute" --openrgb-bin "$fake_openrgb" --execute --no-sdk-evidence >/dev/null
python3 - "$tmp/execute/openrgb-keyboard-rgb-bridge-evidence.json" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["result"]["status"] != "executed":
    raise SystemExit("execute run should be marked executed")
if report["readback"]["after_mode"] != "Breathing":
    raise SystemExit(f"requested mode should be read back: {report['readback']}")
if report["readback"]["restored_mode"] != "Direct":
    raise SystemExit(f"saved mode should be restored: {report['readback']}")
if not report["profiles"]["before_profile_saved"] or not report["profiles"]["after_profile_saved"]:
    raise SystemExit("profiles should be saved in execute mode")
if not report["readback"]["color_readback_supported"]:
    raise SystemExit(f"profile color bytes should prove color read-back: {report['profiles']['after_profile']}")
if not report["result"]["backend_ready_evidence"]:
    raise SystemExit(f"fake execute should satisfy backend evidence: {report['result']}")
PY

"$script" --output "$tmp/execute" --openrgb-bin "$fake_openrgb" --no-sdk-evidence >/dev/null
python3 - "$tmp/execute/openrgb-keyboard-rgb-bridge-evidence.json" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["result"]["status"] != "dry_run":
    raise SystemExit("rerun without --execute should be dry-run")
if report["profiles"]["after_profile_saved"]:
    raise SystemExit("dry-run rerun must not reuse stale execute after-profile evidence")
if report["readback"]["color_readback_supported"]:
    raise SystemExit("dry-run rerun must not reuse stale color read-back evidence")
if report["result"]["backend_ready_evidence"]:
    raise SystemExit("dry-run rerun must not reuse stale backend-ready evidence")
PY

printf 'Direct\n' >"$state"
printf '000000,000000,000000,000000\n' >"$colors_state"
fake_sdk="$tmp/fake-sdk-evidence"
cat >"$fake_sdk" <<EOF
#!/usr/bin/env bash
set -euo pipefail
state="$state"
colors_state="$colors_state"
output=""
while [[ \$# -gt 0 ]]; do
  case "\$1" in
    --output)
      output="\${2:?missing output}"
      shift 2
      ;;
    --openrgb-bin)
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "\$output"
python3 - "\$output/openrgb-keyboard-rgb-sdk-evidence.json" "\$(cat "\$state")" "\$(cat "\$colors_state")" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
mode = sys.argv[2]
colors = ["#" + color for color in sys.argv[3].split(",")]
report = {
    "schema_version": 1,
    "sdk": {"connected": True, "controller_count": 1},
    "keyboard": {
        "detected": True,
        "controller": {
            "name": "Lenovo 5 2023",
            "active_mode": mode,
            "colors": colors,
            "leds": [{"name": "Left side"}, {"name": "Left center"}, {"name": "Right center"}, {"name": "Right side"}],
        },
    },
    "result": {"status": "ok", "read_back_supported": True, "promotion_blockers": []},
}
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY
EOF
chmod +x "$fake_sdk"

"$script" --output "$tmp/sdk-execute" --openrgb-bin "$fake_openrgb" --sdk-evidence-bin "$fake_sdk" --execute >/dev/null
python3 - "$tmp/sdk-execute/openrgb-keyboard-rgb-bridge-evidence.json" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
readback = report["readback"]
sdk = report["sdk_readback"]
if not readback["sdk_mode_readback_matches"]:
    raise SystemExit(f"SDK mode should prove apply: {readback}")
if not readback["sdk_color_readback_supported"]:
    raise SystemExit(f"SDK colors should prove apply: {readback}")
if not readback["sdk_restore_color_matches"]:
    raise SystemExit(f"SDK colors should prove restore: {readback}")
if sdk["before"]["colors"] != sdk["restored"]["colors"]:
    raise SystemExit(f"SDK restore colors should match before: {sdk}")
if not report["result"]["backend_ready_evidence"]:
    raise SystemExit(f"SDK-backed execute should satisfy backend evidence: {report['result']}")
PY

echo "capture-keyboard-rgb-openrgb-bridge-evidence tests passed"
