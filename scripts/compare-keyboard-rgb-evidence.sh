#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/compare-keyboard-rgb-evidence.sh --output <report-dir> <bundle-dir|evidence.json>...

Compare read-only keyboard RGB evidence bundles.

The comparison clusters captured HID candidates by protocol_signature and
reports backend readiness blockers. It never opens /dev/hidraw and never sends
HID feature/output reports.

Options:
  --output <dir>       Required report directory.
  -h, --help           Show this help.
EOF
}

output=""
inputs=()

while (($#)); do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      inputs+=("$1")
      shift
      ;;
  esac
done

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

if ((${#inputs[@]} == 0)); then
  echo "at least one evidence bundle or JSON file is required" >&2
  usage >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to compare keyboard RGB evidence" >&2
  exit 1
}

mkdir -p "$output"
report_json="$output/keyboard-rgb-protocol-comparison.json"
report_md="$output/keyboard-rgb-protocol-comparison.md"

python3 - "$report_json" "$report_md" "${inputs[@]}" <<'PY'
import datetime as _dt
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
markdown_path = pathlib.Path(sys.argv[2])
inputs = [pathlib.Path(value) for value in sys.argv[3:]]

def evidence_json_path(path):
    if path.is_dir():
        return path / "keyboard-rgb-evidence.json"
    return path

def load_bundle(path):
    evidence_path = evidence_json_path(path)
    data = json.loads(evidence_path.read_text())
    return evidence_path, data

rows = []
for input_path in inputs:
    evidence_path, bundle = load_bundle(input_path)
    candidates = bundle.get("candidates") or []
    if candidates:
        for candidate in candidates:
            research = candidate.get("protocol_research") or {}
            signature = research.get("protocol_signature")
            if not signature:
                signature = next(
                    (
                        row.get("protocol_signature")
                        for row in bundle.get("protocol_matrix") or []
                        if row.get("device_id") == candidate.get("device_id")
                    ),
                    "missing",
                )
            rows.append(
                {
                    "bundle": str(evidence_path),
                    "sysfs_root": bundle.get("sysfs_root"),
                    "device_id": candidate.get("device_id"),
                    "vendor_id": candidate.get("vendor_id"),
                    "product_id": candidate.get("product_id"),
                    "family": research.get("family", "unknown"),
                    "confidence": research.get("confidence", "unknown"),
                    "protocol_signature": signature,
                    "backend_ready": bool(research.get("backend_ready")),
                    "write_support_claimed": bool(research.get("write_support_claimed")),
                    "blockers": research.get("blockers") or [],
                    "next_steps": research.get("next_steps") or [],
                }
            )
    else:
        for matrix_row in bundle.get("protocol_matrix") or []:
            rows.append(
                {
                    "bundle": str(evidence_path),
                    "sysfs_root": bundle.get("sysfs_root"),
                    "device_id": matrix_row.get("device_id"),
                    "vendor_id": None,
                    "product_id": None,
                    "family": matrix_row.get("family", "unknown"),
                    "confidence": matrix_row.get("confidence", "unknown"),
                    "protocol_signature": matrix_row.get("protocol_signature", "missing"),
                    "backend_ready": bool(matrix_row.get("backend_ready")),
                    "write_support_claimed": False,
                    "blockers": [],
                    "next_steps": [],
                }
            )

clusters_by_signature = {}
for row in rows:
    clusters_by_signature.setdefault(row["protocol_signature"], []).append(row)

clusters = []
all_blockers = set()
all_next_steps = set()
for signature, members in sorted(clusters_by_signature.items()):
    blockers = sorted({item for row in members for item in row["blockers"]})
    next_steps = sorted({item for row in members for item in row["next_steps"]})
    all_blockers.update(blockers)
    all_next_steps.update(next_steps)
    clusters.append(
        {
            "protocol_signature": signature,
            "candidate_count": len(members),
            "families": sorted({row["family"] for row in members}),
            "confidences": sorted({row["confidence"] for row in members}),
            "backend_ready": all(row["backend_ready"] for row in members) and bool(members),
            "write_support_claimed": any(row["write_support_claimed"] for row in members),
            "devices": [
                {
                    "bundle": row["bundle"],
                    "sysfs_root": row["sysfs_root"],
                    "device_id": row["device_id"],
                    "vid_pid": (
                        f"{row['vendor_id']}:{row['product_id']}"
                        if row.get("vendor_id") or row.get("product_id")
                        else None
                    ),
                }
                for row in members
            ],
            "blockers": blockers,
            "next_steps": next_steps,
        }
    )

backend_ready = bool(clusters) and all(cluster["backend_ready"] for cluster in clusters)
if not all_blockers and not backend_ready:
    all_blockers.update(
        [
            "no proven Linux read-back command for current RGB state",
            "no proven reset-to-previous-mode command",
            "no live HID feature/output report write evidence captured by RatVantage",
        ]
    )

report = {
    "schema_version": 1,
    "generated_at_utc": _dt.datetime.now(_dt.timezone.utc).replace(microsecond=0).isoformat(),
    "bundle_count": len(inputs),
    "candidate_count": len(rows),
    "cluster_count": len(clusters),
    "backend_ready": backend_ready,
    "write_support_claimed": any(row["write_support_claimed"] for row in rows),
    "clusters": clusters,
    "promotion_blockers": sorted(all_blockers),
    "next_steps": sorted(all_next_steps)
    or [
        "capture more read-only evidence bundles",
        "map stable protocol signatures to a documented read-back/reset protocol",
    ],
    "safety_notes": [
        "comparison only; does not open /dev/hidraw",
        "comparison only; does not send HID feature/output reports",
        "backend_ready must remain false until read-back and reset evidence exist",
    ],
}
report_path.write_text(json.dumps(report, indent=2) + "\n")

lines = [
    "# Keyboard RGB Protocol Comparison",
    "",
    f"- bundle_count: {report['bundle_count']}",
    f"- candidate_count: {report['candidate_count']}",
    f"- cluster_count: {report['cluster_count']}",
    f"- backend_ready: `{report['backend_ready']}`",
    "- safety: comparison-only; no `/dev/hidraw` access",
    "",
]
for cluster in clusters:
    lines.extend(
        [
            "## Protocol Signature",
            "",
            f"- signature: `{cluster['protocol_signature']}`",
            f"- candidate_count: {cluster['candidate_count']}",
            f"- families: `{','.join(cluster['families'])}`",
            f"- confidences: `{','.join(cluster['confidences'])}`",
            f"- backend_ready: `{cluster['backend_ready']}`",
            "",
        ]
    )
if report["promotion_blockers"]:
    lines.extend(["## Promotion Blockers", ""])
    lines.extend(f"- {blocker}" for blocker in report["promotion_blockers"])
    lines.append("")
markdown_path.write_text("\n".join(lines) + "\n")

print(
    f"keyboard_rgb_protocol_clusters={report['cluster_count']} "
    f"candidates={report['candidate_count']} backend_ready={report['backend_ready']} "
    f"report={report_path}"
)
PY
