#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="$repo_root/target/qa-report/dbus"
sysfs_root="$repo_root/tests/fixtures/sysfs-82wm-confirmed"
expected="$repo_root/tests/fixtures/dbus/ratvantage-daemon.xml"

while (($#)); do
  case "$1" in
    --output-dir)
      output_dir="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --expected)
      expected="${2:?missing value for --expected}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$output_dir" "$repo_root/target/qa-report/logs"
tmpdir="$(mktemp -d)"
bus_address_file="$tmpdir/bus-address.txt"
dbus_log="$repo_root/target/qa-report/logs/dbus-contract-bus.log"
daemon_log="$repo_root/target/qa-report/logs/dbus-contract-daemon.log"
state_path="$tmpdir/state.toml"
bus_pid=""
daemon_pid=""

cleanup() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
  if [[ -n "$bus_pid" ]]; then
    kill "$bus_pid" 2>/dev/null || true
    wait "$bus_pid" 2>/dev/null || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

dbus-daemon --session --print-address=1 --nofork >"$bus_address_file" 2>"$dbus_log" &
bus_pid="$!"
for _ in {1..100}; do
  [[ -s "$bus_address_file" ]] && break
  sleep 0.1
done
bus_address="$(head -n1 "$bus_address_file")"
if [[ -z "$bus_address" ]]; then
  echo "failed to start private session bus" >&2
  exit 1
fi

env DBUS_SESSION_BUS_ADDRESS="$bus_address" \
  cargo run -q -p legion-control-daemon -- --session --sysfs-root "$sysfs_root" --state-path "$state_path" \
  >"$daemon_log" 2>&1 &
daemon_pid="$!"

for _ in {1..160}; do
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    echo "private daemon exited before becoming ready; see $daemon_log" >&2
    exit 1
  fi
  grep -q 'serving interface=' "$daemon_log" && break
  sleep 0.1
done

current="$output_dir/current.xml"
env DBUS_SESSION_BUS_ADDRESS="$bus_address" \
  gdbus introspect --session \
    --dest org.ratvantage.LegionControl1 \
    --object-path /org/ratvantage/LegionControl1 \
    --xml >"$current"

canonicalize_xml() {
  local input="$1"
  local output="$2"
  python3 - "$input" "$output" <<'PY'
import pathlib
import sys
import xml.etree.ElementTree as ET

source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
tree = ET.parse(source)
root = tree.getroot()
root[:] = sorted(
    list(root),
    key=lambda node: (
        node.tag,
        node.attrib.get("name", ""),
        node.attrib.get("type", ""),
    ),
)
for iface in root.findall("interface"):
    iface[:] = sorted(
        list(iface),
        key=lambda node: (
            node.tag,
            node.attrib.get("name", ""),
            node.attrib.get("type", ""),
            node.attrib.get("direction", ""),
        ),
    )
ET.indent(tree, space="  ")
target.write_text(
    '<?xml version="1.0"?>\n'
    '<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"\n'
    ' "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">\n'
    + ET.tostring(root, encoding="unicode")
    + "\n",
    encoding="utf-8",
)
PY
}

write_dbus_diff_md() {
  local expected_xml="$1"
  local current_xml="$2"
  local output_md="$3"
  local status_value="$4"
  python3 - "$expected_xml" "$current_xml" "$output_md" "$status_value" <<'PY'
import pathlib
import sys
import xml.etree.ElementTree as ET

expected_path = pathlib.Path(sys.argv[1])
current_path = pathlib.Path(sys.argv[2])
output_path = pathlib.Path(sys.argv[3])
status = sys.argv[4]


def contract(path):
    if not path.exists():
        return {}
    root = ET.parse(path).getroot()
    out = {}
    for iface in root.findall("interface"):
        name = iface.attrib.get("name", "")
        methods = {}
        props = {}
        signals = {}
        for node in iface:
            node_name = node.attrib.get("name", "")
            if node.tag == "method":
                args = [
                    (
                        arg.attrib.get("direction", "in"),
                        arg.attrib.get("name", ""),
                        arg.attrib.get("type", ""),
                    )
                    for arg in node.findall("arg")
                ]
                methods[node_name] = args
            elif node.tag == "property":
                props[node_name] = (
                    node.attrib.get("type", ""),
                    node.attrib.get("access", ""),
                )
            elif node.tag == "signal":
                signals[node_name] = [
                    (arg.attrib.get("name", ""), arg.attrib.get("type", ""))
                    for arg in node.findall("arg")
                ]
        out[name] = {"methods": methods, "properties": props, "signals": signals}
    return out


expected = contract(expected_path)
current = contract(current_path)
compat = []
lines = ["# D-Bus Contract Diff", "", f"- Status: `{status}`", ""]

if status == "missing_baseline":
    lines.append("- Baseline missing; review `current.xml` before approving.")
else:
    for iface in sorted(set(expected) - set(current)):
        compat.append(f"removed interface {iface}")
    for iface in sorted(set(current) - set(expected)):
        lines.append(f"- Added interface: `{iface}`")
    for iface in sorted(set(expected) & set(current)):
        exp = expected[iface]
        cur = current[iface]
        for method in sorted(set(exp["methods"]) - set(cur["methods"])):
            compat.append(f"removed method {iface}.{method}")
        for method in sorted(set(cur["methods"]) - set(exp["methods"])):
            lines.append(f"- Added method: `{iface}.{method}`")
        for method in sorted(set(exp["methods"]) & set(cur["methods"])):
            if exp["methods"][method] != cur["methods"][method]:
                compat.append(f"changed method signature {iface}.{method}")
        for prop in sorted(set(exp["properties"]) - set(cur["properties"])):
            compat.append(f"removed property {iface}.{prop}")
        for prop in sorted(set(cur["properties"]) - set(exp["properties"])):
            lines.append(f"- Added property: `{iface}.{prop}`")
        for prop in sorted(set(exp["properties"]) & set(cur["properties"])):
            if exp["properties"][prop] != cur["properties"][prop]:
                compat.append(f"changed property access/type {iface}.{prop}")
        for sig in sorted(set(exp["signals"]) - set(cur["signals"])):
            lines.append(f"- Removed signal: `{iface}.{sig}`")
        for sig in sorted(set(cur["signals"]) - set(exp["signals"])):
            lines.append(f"- Added signal: `{iface}.{sig}`")
        for sig in sorted(set(exp["signals"]) & set(cur["signals"])):
            if exp["signals"][sig] != cur["signals"][sig]:
                lines.append(f"- Changed signal signature: `{iface}.{sig}`")

    if compat:
        lines.extend(["## Compatibility-Sensitive Changes", ""])
        lines.extend(f"- **{item}**" for item in compat)
    elif len(lines) == 4:
        lines.append("- No D-Bus contract changes.")

output_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
PY
}

canonicalize_xml "$current" "$current.canonical"
mv "$current.canonical" "$current"

status="passed"
dbus_diff_md="$output_dir/diff.md"
if [[ "${UPDATE_GUI_BASELINES:-}" == "1" ]]; then
  mkdir -p "$(dirname "$expected")"
  cp "$current" "$expected"
  status="updated_baseline"
elif [[ ! -f "$expected" ]]; then
  status="missing_baseline"
  write_dbus_diff_md "$expected" "$current" "$dbus_diff_md" "$status"
else
  expected_canonical="$tmpdir/expected-canonical.xml"
  canonicalize_xml "$expected" "$expected_canonical"
  if ! diff -u "$expected_canonical" "$current" >"$output_dir/diff.patch"; then
    status="failed"
  fi
  write_dbus_diff_md "$expected_canonical" "$current" "$dbus_diff_md" "$status"
fi

if [[ "${UPDATE_GUI_BASELINES:-}" == "1" ]]; then
  write_dbus_diff_md "$expected" "$current" "$dbus_diff_md" "$status"
fi

python3 - "$output_dir/result.json" "$status" "$current" "$expected" "$dbus_diff_md" <<'PY'
import json
import pathlib
import sys

payload = {
    "status": sys.argv[2],
    "current": sys.argv[3],
    "expected": sys.argv[4],
    "diff": str(pathlib.Path(sys.argv[1]).with_name("diff.patch")),
    "diff_md": sys.argv[5],
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY

[[ "$status" == "failed" ]] && exit 1
exit 0
