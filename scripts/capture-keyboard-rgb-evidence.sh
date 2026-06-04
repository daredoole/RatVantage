#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-keyboard-rgb-evidence.sh --output <bundle-dir> [options]

Capture read-only keyboard RGB HID candidate evidence.

This script never opens /dev/hidraw and never writes hardware state. It runs
legion-probe against sysfs, records candidate metadata, and stores report
descriptor hashes/hex when sysfs exposes report_descriptor.

Options:
  --output <dir>         Required bundle directory.
  --sysfs-root <root>    Sysfs root to probe. Default: /
  --observed-hotkey <s>  Optional operator-observed RGB hotkey, for example Fn+Space.
  --observed-effect <s>  Optional operator-observed visible effect, for example breathing.
  --operator-note <s>    Optional operator note to store in the evidence bundle.
  -h, --help             Show this help.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output=""
sysfs_root="/"
observed_hotkey=""
observed_effect=""
operator_notes=()

while (($#)); do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --observed-hotkey)
      observed_hotkey="${2:?missing value for --observed-hotkey}"
      shift 2
      ;;
    --observed-effect)
      observed_effect="${2:?missing value for --observed-effect}"
      shift 2
      ;;
    --operator-note)
      operator_notes+=("${2:?missing value for --operator-note}")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

command -v cargo >/dev/null 2>&1 || {
  echo "missing cargo; run from a Rust/Cargo environment" >&2
  exit 1
}

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate keyboard RGB evidence" >&2
  exit 1
}

mkdir -p "$output"/{candidates,logs}

probe_json="$output/probe.json"
evidence_json="$output/keyboard-rgb-evidence.json"
evidence_md="$output/keyboard-rgb-evidence.md"
commands_log="$output/commands.log"

{
  printf '$ cargo run -p legion-probe -- --json --sysfs-root %q\n' "$sysfs_root"
} >>"$commands_log"

(
  cd "$repo_root"
  cargo run -p legion-probe -- --json --sysfs-root "$sysfs_root"
) >"$probe_json" 2>"$output/logs/legion-probe.stderr"

python3 - "$probe_json" "$evidence_json" "$evidence_md" "$output/candidates" "$sysfs_root" "$observed_hotkey" "$observed_effect" "${operator_notes[@]}" <<'PY'
import datetime as _dt
import hashlib
import json
import pathlib
import sys

probe_path = pathlib.Path(sys.argv[1])
evidence_path = pathlib.Path(sys.argv[2])
markdown_path = pathlib.Path(sys.argv[3])
candidate_dir = pathlib.Path(sys.argv[4])
sysfs_root = sys.argv[5]
observed_hotkey = sys.argv[6]
observed_effect = sys.argv[7]
operator_notes = sys.argv[8:]

probe = json.loads(probe_path.read_text())
candidates = probe.get("keyboard_rgb_candidates") or []
hardware = probe.get("hardware") or {}

def descriptor_path(candidate):
    path = pathlib.Path(candidate.get("path") or "")
    return path / "device" / "report_descriptor"

def hex_lines(data):
    lines = []
    for offset in range(0, len(data), 16):
        chunk = data[offset : offset + 16]
        lines.append(f"{offset:04x}: " + " ".join(f"{byte:02x}" for byte in chunk))
    return "\n".join(lines) + ("\n" if lines else "")

def report_shape(report):
    report_id = report.get("report_id")
    report_id = "none" if report_id is None else str(report_id)
    return f"{report_id}/{report.get('kind')}:{report.get('byte_length')}B"

def protocol_signature(summary):
    ids = ",".join(str(value) for value in summary.get("report_ids") or []) or "none"
    shapes = ",".join(report_shape(report) for report in summary.get("hid_reports") or []) or "none"
    digest = summary.get("report_descriptor_sha256") or "missing"
    return (
        f"{summary.get('vendor_id')}:{summary.get('product_id')}"
        f"|bytes={summary.get('report_descriptor_bytes')}"
        f"|reports={ids}"
        f"|shapes={shapes}"
        f"|sha256={digest}"
    )

def classify_protocol(summary):
    vendor = (summary.get("vendor_id") or "").upper()
    product = (summary.get("product_id") or "").upper()
    shapes = [report_shape(report) for report in summary.get("hid_reports") or []]
    report_ids = set(summary.get("report_ids") or [])
    reasons = []

    if vendor == "048D" and product in {"C985", "C103"}:
        family = "ite_legion_hid_research_candidate"
        if 90 in report_ids:
            reasons.append("contains Report ID 90, matching current live ITE Legion candidates")
        if any(shape == "90/feature:16B" for shape in shapes):
            reasons.append("has a 16-byte feature report on Report ID 90")
        if any(shape.endswith("/output:64B") or shape.endswith("/feature:64B") for shape in shapes):
            reasons.append("has 64-byte HID report shapes often used by vendor command channels")
        confidence = "medium" if reasons else "low"
    else:
        family = "not_keyboard_rgb_protocol_candidate"
        confidence = "none"
        reasons.append("VID:PID is outside the current ITE Legion RGB research allowlist")

    return {
        "family": family,
        "confidence": confidence,
        "protocol_signature": protocol_signature(summary),
        "backend_ready": False,
        "write_support_claimed": False,
        "reasons": reasons,
        "blockers": [
            "no proven Linux read-back command for current RGB state",
            "no proven reset-to-previous-mode command",
            "no live HID feature/output report write evidence captured by RatVantage",
        ],
        "next_steps": [
            "compare protocol_signature across multiple evidence bundles",
            "map reports to a documented userspace or kernel protocol reference",
            "promote only after fake and live backend read-back/rollback tests pass",
        ],
    }

summaries = []
for candidate in candidates:
    summary = {
        "backend": candidate.get("backend"),
        "device_id": candidate.get("device_id"),
        "path": candidate.get("path"),
        "vendor_id": candidate.get("vendor_id"),
        "product_id": candidate.get("product_id"),
        "name": candidate.get("name"),
        "modalias": candidate.get("modalias"),
        "report_descriptor_bytes": candidate.get("report_descriptor_bytes"),
        "report_ids": candidate.get("report_ids") or [],
        "hid_reports": candidate.get("hid_reports") or [],
        "evidence": candidate.get("evidence") or [],
    }

    descriptor = descriptor_path(candidate)
    if descriptor.exists():
        data = descriptor.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        device_id = candidate.get("device_id") or f"candidate-{len(summaries) + 1}"
        hex_file = candidate_dir / f"{device_id}-report_descriptor.hex"
        hex_file.write_text(hex_lines(data))
        summary["report_descriptor_sha256"] = digest
        summary["report_descriptor_hex_file"] = str(hex_file.relative_to(evidence_path.parent))

    summary["protocol_research"] = classify_protocol(summary)
    summaries.append(summary)

protocol_matrix = [
    {
        "device_id": summary.get("device_id"),
        "family": summary["protocol_research"]["family"],
        "confidence": summary["protocol_research"]["confidence"],
        "protocol_signature": summary["protocol_research"]["protocol_signature"],
        "backend_ready": summary["protocol_research"]["backend_ready"],
    }
    for summary in summaries
]

report = {
    "schema_version": 1,
    "generated_at_utc": _dt.datetime.now(_dt.timezone.utc).replace(microsecond=0).isoformat(),
    "sysfs_root": sysfs_root,
    "hardware": hardware,
    "operator_observations": {
        "hotkey": observed_hotkey or None,
        "visible_effect": observed_effect or None,
        "notes": operator_notes,
        "source": "operator-observed; not read back through RatVantage",
    },
    "candidate_count": len(summaries),
    "candidates": summaries,
    "protocol_matrix": protocol_matrix,
    "safety_notes": [
        "read-only evidence only",
        "does not open /dev/hidraw",
        "does not send HID feature/output reports",
        "does not claim keyboard RGB write support",
    ],
}
evidence_path.write_text(json.dumps(report, indent=2) + "\n")

lines = [
    "# Keyboard RGB Evidence",
    "",
    f"- sysfs_root: `{sysfs_root}`",
    f"- candidate_count: {len(summaries)}",
    "- safety: read-only sysfs metadata/report_descriptor capture; no `/dev/hidraw` access",
    "",
]
if observed_hotkey or observed_effect or operator_notes:
    lines.extend(
        [
            "## Operator Observations",
            "",
            f"- hotkey: `{observed_hotkey or 'not recorded'}`",
            f"- visible_effect: `{observed_effect or 'not recorded'}`",
            "- source: operator-observed; not read back through RatVantage",
            "",
        ]
    )
    for note in operator_notes:
        lines.append(f"- note: {note}")
    if operator_notes:
        lines.append("")
if not summaries:
    lines.append("No keyboard RGB HID candidates detected.")
else:
    for candidate in summaries:
        ids = ",".join(str(value) for value in candidate["report_ids"]) or "none"
        shapes = ",".join(
            f"{report.get('report_id', 'none')}/{report.get('kind')}:{report.get('byte_length')}B"
            for report in candidate["hid_reports"]
        ) or "none"
        protocol = candidate["protocol_research"]
        lines.extend(
            [
                f"## {candidate.get('device_id')}",
                "",
                f"- ids: `{candidate.get('vendor_id')}:{candidate.get('product_id')}`",
                f"- name: `{candidate.get('name')}`",
                f"- descriptor_bytes: `{candidate.get('report_descriptor_bytes')}`",
                f"- descriptor_sha256: `{candidate.get('report_descriptor_sha256', 'missing')}`",
                f"- report_ids: `{ids}`",
                f"- report_shapes: `{shapes}`",
                f"- protocol_family: `{protocol.get('family')}`",
                f"- protocol_confidence: `{protocol.get('confidence')}`",
                f"- backend_ready: `{protocol.get('backend_ready')}`",
                f"- protocol_signature: `{protocol.get('protocol_signature')}`",
                "",
            ]
        )
markdown_path.write_text("\n".join(lines) + "\n")
PY

python3 - "$evidence_json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(
    "keyboard_rgb_candidates="
    + str(report.get("candidate_count", 0))
    + f" evidence={sys.argv[1]}"
)
PY
