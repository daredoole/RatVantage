#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-keyboard-rgb-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

root="$tmp/sysfs"
ite="$root/sys/class/hidraw/hidraw7/device"
elan="$root/sys/class/hidraw/hidraw8/device"
mkdir -p "$ite" "$elan"
cat >"$ite/uevent" <<'EOF'
DRIVER=hid-generic
HID_ID=0003:0000048D:0000C985
HID_NAME=ITE Tech. Inc. ITE Device(8295)
MODALIAS=hid:b0003g0001v0000048Dp0000C985
EOF
python3 - "$ite/report_descriptor" <<'PY'
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_bytes(bytes([
    0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x5a,
    0x75, 0x08, 0x95, 0x10, 0xb1, 0x02, 0xc0,
]))
PY
cat >"$elan/uevent" <<'EOF'
DRIVER=hid-generic
HID_ID=0003:000004F3:0000327E
HID_NAME=ELAN Touchpad
MODALIAS=hid:b0003g0001v000004F3p0000327E
EOF

output="$tmp/out"
"$script" \
  --sysfs-root "$root" \
  --output "$output" \
  --observed-hotkey "Fn+Space" \
  --observed-effect "breathing" \
  --operator-note "firmware hotkey cycles keyboard RGB modes" \
  >/tmp/ratvantage-keyboard-rgb-evidence-test.txt

python3 - "$output/keyboard-rgb-evidence.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["candidate_count"] != 1:
    raise SystemExit(f"expected one candidate, got {report['candidate_count']}")
observations = report.get("operator_observations") or {}
if observations.get("hotkey") != "Fn+Space":
    raise SystemExit(f"missing observed hotkey: {observations}")
if observations.get("visible_effect") != "breathing":
    raise SystemExit(f"missing observed visible effect: {observations}")
if "not read back" not in observations.get("source", ""):
    raise SystemExit(f"operator observation source is unclear: {observations}")
candidate = report["candidates"][0]
if candidate["device_id"] != "hidraw7":
    raise SystemExit(f"unexpected candidate device_id: {candidate['device_id']}")
if candidate["vendor_id"] != "048D" or candidate["product_id"] != "C985":
    raise SystemExit(f"unexpected VID:PID {candidate['vendor_id']}:{candidate['product_id']}")
if candidate["report_descriptor_bytes"] != 15:
    raise SystemExit(f"unexpected descriptor length: {candidate['report_descriptor_bytes']}")
if candidate["report_ids"] != [90]:
    raise SystemExit(f"unexpected report IDs: {candidate['report_ids']}")
if not candidate.get("report_descriptor_sha256"):
    raise SystemExit("missing descriptor sha256")
research = candidate.get("protocol_research") or {}
if research.get("family") != "ite_legion_hid_research_candidate":
    raise SystemExit(f"unexpected protocol family: {research}")
if research.get("confidence") != "medium":
    raise SystemExit(f"unexpected protocol confidence: {research}")
if research.get("backend_ready") is not False or research.get("write_support_claimed") is not False:
    raise SystemExit(f"protocol research must not claim write readiness: {research}")
signature = research.get("protocol_signature", "")
if "048D:C985" not in signature or "90/feature:16B" not in signature:
    raise SystemExit(f"protocol signature is incomplete: {signature}")
matrix = report.get("protocol_matrix") or []
if len(matrix) != 1 or matrix[0].get("protocol_signature") != signature:
    raise SystemExit(f"protocol matrix did not include the candidate signature: {matrix}")
hex_file = pathlib.Path(sys.argv[1]).parent / candidate["report_descriptor_hex_file"]
if not hex_file.exists() or "0000:" not in hex_file.read_text():
    raise SystemExit("missing descriptor hex evidence")
notes = " ".join(report.get("safety_notes", []))
if "/dev/hidraw" not in notes or "does not send" not in notes:
    raise SystemExit(f"safety notes are incomplete: {notes}")
markdown = pathlib.Path(sys.argv[1]).with_suffix(".md").read_text()
if "protocol_family" not in markdown or "backend_ready" not in markdown:
    raise SystemExit("markdown protocol research fields are missing")
if "Fn+Space" not in markdown or "breathing" not in markdown:
    raise SystemExit("markdown operator observation fields are missing")
PY

echo "capture-keyboard-rgb-evidence tests passed"
