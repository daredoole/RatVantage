#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
report_dir="$repo_root/target/qa-report"
sysfs_root="$repo_root/tests/fixtures/sysfs-82wm-confirmed"

if [[ "${RATVANTAGE_QA_UNDER_DBUS:-}" != "1" ]]; then
  exec dbus-run-session -- env RATVANTAGE_QA_UNDER_DBUS=1 "$0" "$@"
fi

if [[ -z "${DISPLAY:-}" && "${RATVANTAGE_QA_UNDER_XVFB:-}" != "1" ]]; then
  exec xvfb-run -a env RATVANTAGE_QA_UNDER_XVFB=1 "$0" "$@"
fi

while (($#)); do
  case "$1" in
    --report-dir)
      report_dir="${2:?missing value for --report-dir}"
      shift 2
      ;;
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$report_dir"/{screenshots,visual-diffs,widget-trees,semantic-ui,dbus,logs}

for tool in cargo dbus-daemon gdbus import python3; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "missing $tool; run scripts/install-dev-deps-fedora.sh" >&2
    exit 1
  }
done

python3 - <<'PY'
for mod in ("pyatspi", "PIL"):
    __import__(mod)
PY

atspi_pid=""
atspi_registry_pid=""
cleanup_atspi() {
  if [[ -n "$atspi_registry_pid" ]]; then
    kill "$atspi_registry_pid" 2>/dev/null || true
    wait "$atspi_registry_pid" 2>/dev/null || true
  fi
  if [[ -n "$atspi_pid" ]]; then
    kill "$atspi_pid" 2>/dev/null || true
    wait "$atspi_pid" 2>/dev/null || true
  fi
}
trap cleanup_atspi EXIT
atspi_launcher=""
atspi_registry=""
for candidate in /usr/libexec/at-spi-bus-launcher /usr/lib/at-spi2-core/at-spi-bus-launcher; do
  if [[ -x "$candidate" ]]; then
    atspi_launcher="$candidate"
    break
  fi
done
for candidate in /usr/libexec/at-spi2-registryd /usr/lib/at-spi2-core/at-spi2-registryd; do
  if [[ -x "$candidate" ]]; then
    atspi_registry="$candidate"
    break
  fi
done
if [[ -n "$atspi_launcher" ]]; then
  "$atspi_launcher" --launch-immediately >"$report_dir/logs/at-spi-bus.log" 2>&1 &
  atspi_pid="$!"
  sleep 0.5
  AT_SPI_BUS_ADDRESS="$(
    gdbus call --session \
      --dest org.a11y.Bus \
      --object-path /org/a11y/bus \
      --method org.a11y.Bus.GetAddress |
      sed -E "s/^\('(.*)',\)$/\1/"
  )"
  export AT_SPI_BUS_ADDRESS
  if [[ -n "$atspi_registry" ]]; then
    AT_SPI_BUS_ADDRESS="$AT_SPI_BUS_ADDRESS" "$atspi_registry" --use-gnome-session >"$report_dir/logs/at-spi-registry.log" 2>&1 &
    atspi_registry_pid="$!"
    sleep 0.5
  fi
fi

overall=0
stages_json="$report_dir/logs/stages.jsonl"
: >"$stages_json"

run_stage() {
  local name="$1"
  shift
  echo "==> $name"
  local start
  start="$(date --iso-8601=seconds)"
  if "$@"; then
    python3 - "$stages_json" "$name" "$start" passed <<'PY'
import json, pathlib, sys
pathlib.Path(sys.argv[1]).open("a").write(json.dumps({"stage": sys.argv[2], "started_at": sys.argv[3], "status": sys.argv[4]}) + "\n")
PY
  else
    local code=$?
    overall=1
    python3 - "$stages_json" "$name" "$start" failed "$code" <<'PY'
import json, pathlib, sys
pathlib.Path(sys.argv[1]).open("a").write(json.dumps({"stage": sys.argv[2], "started_at": sys.argv[3], "status": sys.argv[4], "exit_code": int(sys.argv[5])}) + "\n")
PY
  fi
}

cargo build -q -p legion-control-daemon
cargo build -q -p legion-control-ui --features gtk-ui

run_stage "GTK UI state" python3 "$repo_root/tests/ui/test_gtk_accessibility.py" --report-dir "$report_dir" --sysfs-root "$sysfs_root"
run_stage "Screenshot capture" python3 "$repo_root/tests/ui/capture_tab_screenshots.py" --report-dir "$report_dir" --sysfs-root "$sysfs_root"
run_stage "Visual regression" python3 "$repo_root/tests/ui/compare_screenshots.py" --report-dir "$report_dir"
run_stage "Semantic UI capture" python3 "$repo_root/tests/ui/capture_semantic_ui.py" --report-dir "$report_dir"
run_stage "Semantic UI snapshot" python3 "$repo_root/tests/ui/compare_semantic_ui.py" --report-dir "$report_dir"
run_stage "Widget tree snapshot" python3 "$repo_root/tests/ui/dump_widget_tree.py" --report-dir "$report_dir" --sysfs-root "$sysfs_root"
run_stage "D-Bus contract" "$repo_root/scripts/capture-dbus-contract.sh" --output-dir "$report_dir/dbus" --sysfs-root "$sysfs_root"

python3 - "$stages_json" "$report_dir/logs/stages.json" <<'PY'
import json, pathlib, sys
rows = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines() if line.strip()]
pathlib.Path(sys.argv[2]).write_text(json.dumps({"stages": rows}, indent=2, sort_keys=True) + "\n")
PY

if ! python3 "$repo_root/tests/ui/write_qa_summary.py" --report-dir "$report_dir"; then
  overall=1
fi

echo "GUI QA report written to $report_dir"
echo "GUI QA review report: $report_dir/review.md"
exit "$overall"
