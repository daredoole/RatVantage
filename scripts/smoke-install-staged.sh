#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
destdir=""
keep=0

usage() {
  cat <<'EOF'
Usage: scripts/smoke-install-staged.sh [--destdir PATH] [--keep]

Stages RatVantage packaging metadata and helper scripts into a temporary root
and validates file placement/permissions. This does not modify the host system,
start services, enable autostart, or perform hardware writes.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --destdir)
      destdir="${2:?missing path after --destdir}"
      shift 2
      ;;
    --keep)
      keep=1
      shift
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

if [[ -z "$destdir" ]]; then
  destdir="$(mktemp -d)"
fi

cleanup() {
  if [[ "$keep" -eq 0 ]]; then
    rm -rf "$destdir"
  fi
}
trap cleanup EXIT

cd "$repo_root"
mkdir -p "$destdir"

install -Dpm0644 data/systemd/legion-control-daemon.service \
  "$destdir/usr/lib/systemd/system/legion-control-daemon.service"
install -Dpm0644 data/dbus/org.ratvantage.LegionControl1.service \
  "$destdir/usr/share/dbus-1/system-services/org.ratvantage.LegionControl1.service"
install -Dpm0644 data/dbus/org.ratvantage.LegionControl1.conf \
  "$destdir/usr/share/dbus-1/system.d/org.ratvantage.LegionControl1.conf"
install -Dpm0644 data/polkit/org.ratvantage.LegionControl1.policy \
  "$destdir/usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy"
install -Dpm0644 data/desktop/org.ratvantage.LegionControl.desktop \
  "$destdir/usr/share/applications/org.ratvantage.LegionControl.desktop"
install -Dpm0644 data/desktop/org.ratvantage.LegionControl.Tray.desktop \
  "$destdir/etc/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop"
install -Dpm0644 data/metainfo/org.ratvantage.LegionControl.metainfo.xml \
  "$destdir/usr/share/metainfo/org.ratvantage.LegionControl.metainfo.xml"
install -Dpm0644 data/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg \
  "$destdir/usr/share/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg"
install -dm0755 "$destdir/usr/share/legion-control/presets"
install -pm0644 data/presets/*.toml "$destdir/usr/share/legion-control/presets/"

while IFS='|' read -r src dest; do
  [[ -z "$src" ]] && continue
  install -Dpm0755 "scripts/$src" "$destdir/usr/bin/$dest"
done <<'EOF'
check-keyboard-rgb-openrgb.sh|ratvantage-check-keyboard-rgb-openrgb
capture-keyboard-rgb-evidence.sh|ratvantage-capture-keyboard-rgb-evidence
compare-keyboard-rgb-evidence.sh|ratvantage-compare-keyboard-rgb-evidence
setup-keyboard-rgb-openrgb-access.sh|ratvantage-setup-keyboard-rgb-openrgb-access
capture-keyboard-rgb-openrgb-bridge-evidence.sh|ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence
review-keyboard-rgb-openrgb-bridge-evidence.sh|ratvantage-review-keyboard-rgb-openrgb-bridge-evidence
status-keyboard-rgb-openrgb-bridge-evidence.sh|ratvantage-keyboard-rgb-openrgb-bridge-status
capture-keyboard-rgb-openrgb-sdk-evidence.sh|ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence
capture-keyboard-rgb-openrgb-sdk-write-evidence.sh|ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence
openrgb-keyboard-rgb-sdk-helper.sh|ratvantage-openrgb-keyboard-rgb-sdk-helper
openrgb-sdk-server-session.sh|ratvantage-openrgb-sdk-server
capture-compatibility-bundle.sh|ratvantage-capture-compatibility-bundle
capture-gpu-mux-evidence.sh|ratvantage-capture-gpu-mux-evidence
review-gpu-mux-evidence.sh|ratvantage-review-gpu-mux-evidence
EOF

python3 - "$destdir" <<'PY'
import os
import pathlib
import stat
import sys

root = pathlib.Path(sys.argv[1])
required = {
    "usr/lib/systemd/system/legion-control-daemon.service": 0o644,
    "usr/share/dbus-1/system-services/org.ratvantage.LegionControl1.service": 0o644,
    "usr/share/dbus-1/system.d/org.ratvantage.LegionControl1.conf": 0o644,
    "usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy": 0o644,
    "usr/share/applications/org.ratvantage.LegionControl.desktop": 0o644,
    "etc/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop": 0o644,
    "usr/share/metainfo/org.ratvantage.LegionControl.metainfo.xml": 0o644,
    "usr/share/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg": 0o644,
    "usr/bin/ratvantage-check-keyboard-rgb-openrgb": 0o755,
    "usr/bin/ratvantage-capture-compatibility-bundle": 0o755,
    "usr/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper": 0o755,
}
for rel, expected_mode in required.items():
    path = root / rel
    if not path.exists():
        raise SystemExit(f"missing staged file: {rel}")
    actual = stat.S_IMODE(path.stat().st_mode)
    if actual != expected_mode:
        raise SystemExit(f"{rel}: mode {actual:o}, expected {expected_mode:o}")

service = (root / "usr/lib/systemd/system/legion-control-daemon.service").read_text()
if "legion-control-ui" in service or "legion-control-tray" in service:
    raise SystemExit("systemd service must not start GUI or tray")
if "--enable-" in service:
    raise SystemExit("packaged systemd service must not enable write flags by default")

desktop = (root / "usr/share/applications/org.ratvantage.LegionControl.desktop").read_text()
if "sudo" in desktop or "pkexec" in desktop:
    raise SystemExit("desktop file must not launch through sudo/pkexec")
for rel in [
    "usr/share/applications/org.ratvantage.LegionControl.desktop",
    "etc/xdg/autostart/org.ratvantage.LegionControl.Tray.desktop",
]:
    text = (root / rel).read_text()
    if "Icon=org.ratvantage.LegionControl" not in text:
        raise SystemExit(f"{rel}: desktop icon does not match packaged app icon")

metainfo = (root / "usr/share/metainfo/org.ratvantage.LegionControl.metainfo.xml").read_text()
if "<icon type=\"stock\">org.ratvantage.LegionControl</icon>" not in metainfo:
    raise SystemExit("AppStream icon metadata must reference the packaged app icon")

print("staged install smoke passed")
PY
