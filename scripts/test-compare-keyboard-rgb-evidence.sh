#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
capture_script="$repo_root/scripts/capture-keyboard-rgb-evidence.sh"
compare_script="$repo_root/scripts/compare-keyboard-rgb-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write_candidate() {
  local root="$1"
  local hidraw="$2"
  local product="$3"
  local descriptor_kind="$4"
  local device="$root/sys/class/hidraw/$hidraw/device"
  mkdir -p "$device"
  cat >"$device/uevent" <<EOF
DRIVER=hid-generic
HID_ID=0003:0000048D:0000${product}
HID_NAME=ITE Tech. Inc. ITE Device(${product})
MODALIAS=hid:b0003g0001v0000048Dp0000${product}
EOF
  python3 - "$device/report_descriptor" "$descriptor_kind" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
kind = sys.argv[2]
if kind == "feature-90":
    data = [
        0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x5a,
        0x75, 0x08, 0x95, 0x10, 0xb1, 0x02, 0xc0,
    ]
else:
    data = [
        0x06, 0x00, 0xff, 0x09, 0x01, 0xa1, 0x01,
        0x75, 0x08, 0x95, 0x40, 0xb1, 0x02,
        0x75, 0x08, 0x95, 0x40, 0x91, 0x02, 0xc0,
    ]
path.write_bytes(bytes(data))
PY
}

root_a="$tmp/sysfs-a"
root_b="$tmp/sysfs-b"
write_candidate "$root_a" hidraw7 C985 feature-90
write_candidate "$root_b" hidraw8 C103 feature-90
write_candidate "$root_b" hidraw9 C985 vendor-64

"$capture_script" --sysfs-root "$root_a" --output "$tmp/bundle-a" >/tmp/ratvantage-rgb-compare-capture-a.txt
"$capture_script" --sysfs-root "$root_b" --output "$tmp/bundle-b" >/tmp/ratvantage-rgb-compare-capture-b.txt
"$compare_script" --output "$tmp/compare" "$tmp/bundle-a" "$tmp/bundle-b/keyboard-rgb-evidence.json" >/tmp/ratvantage-rgb-compare-test.txt

python3 - "$tmp/compare/keyboard-rgb-protocol-comparison.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["bundle_count"] != 2:
    raise SystemExit(f"expected two bundles, got {report['bundle_count']}")
if report["candidate_count"] != 3:
    raise SystemExit(f"expected three candidates, got {report['candidate_count']}")
if report["cluster_count"] != 3:
    raise SystemExit(f"expected three clusters, got {report['cluster_count']}")
if report["backend_ready"] is not False or report["write_support_claimed"] is not False:
    raise SystemExit(f"comparison must not claim backend readiness: {report}")
signatures = [cluster["protocol_signature"] for cluster in report["clusters"]]
if not any("048D:C985" in signature and "90/feature:16B" in signature for signature in signatures):
    raise SystemExit(f"missing C985 feature-90 signature: {signatures}")
if not any("048D:C103" in signature for signature in signatures):
    raise SystemExit(f"missing C103 signature: {signatures}")
blockers = " ".join(report.get("promotion_blockers") or [])
if "read-back" not in blockers or "reset" not in blockers:
    raise SystemExit(f"promotion blockers are incomplete: {blockers}")
markdown = pathlib.Path(sys.argv[1]).with_suffix(".md").read_text()
if "Promotion Blockers" not in markdown or "backend_ready" not in markdown:
    raise SystemExit("markdown comparison report is missing readiness details")
PY

echo "compare-keyboard-rgb-evidence tests passed"
