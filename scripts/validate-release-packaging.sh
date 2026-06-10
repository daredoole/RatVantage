#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0
warnings=0
skips=0

pass() { printf 'PASS  %s\n' "$*"; }
warn() { warnings=$((warnings + 1)); printf 'WARN  %s\n' "$*"; }
skip() { skips=$((skips + 1)); printf 'SKIP  %s\n' "$*"; }
fail() { failures=$((failures + 1)); printf 'FAIL  %s\n' "$*"; }

run_required() {
  local label="$1"
  shift
  if "$@"; then
    pass "$label"
  else
    fail "$label"
  fi
}

if command -v desktop-file-validate >/dev/null 2>&1; then
  for desktop in data/desktop/*.desktop; do
    run_required "desktop-file-validate $desktop" desktop-file-validate "$desktop"
  done
else
  skip "desktop-file-validate unavailable"
fi

if command -v appstreamcli >/dev/null 2>&1; then
  run_required "AppStream metadata" appstreamcli validate --no-net data/metainfo/org.ratvantage.LegionControl.metainfo.xml
else
  skip "appstreamcli unavailable"
fi

if command -v systemd-analyze >/dev/null 2>&1; then
  unit_tmp="$(mktemp --suffix=.service)"
  trap 'rm -f "$unit_tmp"' EXIT
  sed \
    -e 's#^ExecStart=.*#ExecStart=/bin/true#' \
    -e 's#^BusName=.*#BusName=org.ratvantage.LegionControl1.Validation#' \
    data/systemd/legion-control-daemon.service >"$unit_tmp"
  run_required "systemd unit verification" systemd-analyze verify "$unit_tmp"
else
  skip "systemd-analyze unavailable"
fi

if command -v rpmspec >/dev/null 2>&1; then
  run_required "RPM spec parse" bash -c 'rpmspec -P packaging/rpm/legion-control.spec >/dev/null'
else
  skip "rpmspec unavailable"
fi

run_required "staged install smoke" scripts/smoke-install-staged.sh

if python3 - <<'PY'
import configparser
import pathlib
import re
import sys
import tomllib
from xml.etree import ElementTree

ROOT = pathlib.Path(".")
DBUS_NAME = "org.ratvantage.LegionControl1"
EXPECTED_POLKIT = {
    f"{DBUS_NAME}.read",
    f"{DBUS_NAME}.set-platform-profile",
    f"{DBUS_NAME}.set-battery-charge-type",
    f"{DBUS_NAME}.set-led-state",
    f"{DBUS_NAME}.set-keyboard-rgb",
    f"{DBUS_NAME}.set-ideapad-toggle",
    f"{DBUS_NAME}.set-gpu-mode",
    f"{DBUS_NAME}.set-cpu-governor",
    f"{DBUS_NAME}.set-cpu-epp",
    f"{DBUS_NAME}.set-firmware-attribute",
    f"{DBUS_NAME}.set-cpu-boost",
    f"{DBUS_NAME}.set-conservation-mode",
    f"{DBUS_NAME}.set-amd-gpu-dpm-force-level",
    f"{DBUS_NAME}.set-curve-optimizer",
    f"{DBUS_NAME}.setup-openrgb-access",
    f"{DBUS_NAME}.apply-hardware-profile",
}
WRITE_ENABLE_RE = re.compile(r"--enable-[a-z0-9-]+(?:-write|apply|observer|setup)")

def die(message: str) -> None:
    raise SystemExit(message)

def parse_xml(path: pathlib.Path):
    try:
        return ElementTree.parse(path)
    except ElementTree.ParseError as error:
        die(f"{path}: XML parse failed: {error}")

for path in [
    ROOT / "data/dbus/org.ratvantage.LegionControl1.conf",
    ROOT / "data/polkit/org.ratvantage.LegionControl1.policy",
    ROOT / "data/metainfo/org.ratvantage.LegionControl.metainfo.xml",
]:
    parse_xml(path)

policy = parse_xml(ROOT / "data/polkit/org.ratvantage.LegionControl1.policy").getroot()
actions = {action.attrib.get("id"): action for action in policy.findall("action")}
if set(actions) != EXPECTED_POLKIT:
    missing = sorted(EXPECTED_POLKIT - set(actions))
    extra = sorted(set(actions) - EXPECTED_POLKIT)
    die(f"polkit action mismatch; missing={missing} extra={extra}")
for action_id, action in actions.items():
    defaults = action.find("defaults")
    if defaults is None:
        die(f"{action_id}: missing defaults")
    values = {
        child.tag: (child.text or "").strip()
        for child in defaults
    }
    if action_id.endswith(".read"):
        expected = {"allow_any": "yes", "allow_inactive": "yes", "allow_active": "yes"}
    else:
        expected = {
            "allow_any": "no",
            "allow_inactive": "no",
            "allow_active": "auth_admin_keep",
        }
    if values != expected:
        die(f"{action_id}: defaults {values!r}, expected {expected!r}")

busconfig = parse_xml(ROOT / "data/dbus/org.ratvantage.LegionControl1.conf").getroot()
if busconfig.tag != "busconfig":
    die("D-Bus policy root must be busconfig")
if not busconfig.findall(".//allow[@own='org.ratvantage.LegionControl1']"):
    die("D-Bus policy must allow root to own service name")
if not busconfig.findall(".//allow[@send_destination='org.ratvantage.LegionControl1']"):
    die("D-Bus policy must allow clients to send to service")

service = configparser.ConfigParser()
service.read(ROOT / "data/dbus/org.ratvantage.LegionControl1.service")
dbus_service = service["D-BUS Service"]
if dbus_service.get("Name") != DBUS_NAME:
    die("D-Bus service Name mismatch")
if dbus_service.get("User") != "root":
    die("D-Bus service must run daemon as root")
if dbus_service.get("SystemdService") != "legion-control-daemon.service":
    die("D-Bus service SystemdService mismatch")
dbus_exec = dbus_service.get("Exec", "")
if "legion-control-daemon" not in dbus_exec:
    die("D-Bus service Exec must point to daemon")
if "legion-control-ui" in dbus_exec or "legion-control-tray" in dbus_exec:
    die("D-Bus service must not launch GUI/tray")
if WRITE_ENABLE_RE.search(dbus_exec):
    die("D-Bus service Exec must not enable writes by default")

unit = configparser.ConfigParser()
unit.optionxform = str
unit.read(ROOT / "data/systemd/legion-control-daemon.service")
svc = unit["Service"]
if svc.get("Type") != "dbus" or svc.get("BusName") != DBUS_NAME:
    die("systemd service must be Type=dbus with matching BusName")
exec_start = svc.get("ExecStart", "")
if "legion-control-daemon" not in exec_start:
    die("systemd service ExecStart must point to daemon")
if "legion-control-ui" in exec_start or "legion-control-tray" in exec_start:
    die("systemd service must not launch GUI/tray")
if WRITE_ENABLE_RE.search(exec_start):
    die("packaged systemd service must not enable writes by default")

desktop_icons = set()
desktop_icon_by_path = {}
for desktop_path in sorted((ROOT / "data/desktop").glob("*.desktop")):
    desktop = configparser.ConfigParser(interpolation=None)
    desktop.optionxform = str
    desktop.read(desktop_path)
    entry = desktop["Desktop Entry"]
    exec_value = entry.get("Exec", "")
    if "sudo" in exec_value or "pkexec" in exec_value:
        die(f"{desktop_path}: Exec must not run through sudo/pkexec")
    if desktop_path.name.endswith("Tray.desktop"):
        if "legion-control-tray" not in exec_value:
            die(f"{desktop_path}: tray desktop Exec mismatch")
    elif "legion-control-ui" not in exec_value:
        die(f"{desktop_path}: UI desktop Exec mismatch")
    if not entry.get("Categories"):
        die(f"{desktop_path}: missing Categories")
    if not entry.get("Keywords", "").strip():
        die(f"{desktop_path}: missing Keywords")
    icon = entry.get("Icon", "").strip()
    if icon:
        desktop_icons.add(icon)
        desktop_icon_by_path[str(desktop_path)] = icon

metainfo = parse_xml(ROOT / "data/metainfo/org.ratvantage.LegionControl.metainfo.xml").getroot()
launchable = metainfo.find("launchable")
if launchable is None or (launchable.text or "").strip() != "org.ratvantage.LegionControl.desktop":
    die("AppStream launchable must match desktop id")
appstream_icon = metainfo.find("icon")
if appstream_icon is None or appstream_icon.attrib.get("type") != "stock":
    die("AppStream metadata must include a stock icon")
appstream_icon_name = (appstream_icon.text or "").strip()
if not appstream_icon_name:
    die("AppStream icon name must not be empty")
if appstream_icon_name not in desktop_icons:
    die("AppStream icon must match a desktop Icon value")

for icon in sorted(desktop_icons):
    icon_path = ROOT / "data/icons/hicolor/scalable/apps" / f"{icon}.svg"
    if not icon_path.is_file():
        die(f"missing packaged hicolor SVG icon for desktop Icon={icon}: {icon_path}")

preset_dir = ROOT / "data/presets"
expected_presets = {"quiet-office", "balanced-daily", "gaming", "max-safe"}
seen = set()
for path in sorted(preset_dir.glob("*.toml")):
    data = tomllib.loads(path.read_text())
    preset_id = data.get("id")
    seen.add(preset_id)
    if preset_id != path.stem:
        die(f"{path}: id must match file name")
    if data.get("schema_version") != 1:
        die(f"{path}: schema_version must be 1")
    for key in ["label", "description", "safety_note"]:
        if not isinstance(data.get(key), str) or not data[key].strip():
            die(f"{path}: missing {key}")
if seen != expected_presets:
    die(f"preset ids mismatch: {sorted(seen)}")

spec = (ROOT / "packaging/rpm/legion-control.spec").read_text()
for required in [
    "%systemd_post legion-control-daemon.service",
    "%systemd_preun legion-control-daemon.service",
    "%systemd_postun_with_restart legion-control-daemon.service",
    "%{_libexecdir}/legion-control/legion-control-daemon",
    "%{_datadir}/dbus-1/system-services/org.ratvantage.LegionControl1.service",
    "%{_datadir}/dbus-1/system.d/org.ratvantage.LegionControl1.conf",
    "%{_datadir}/polkit-1/actions/org.ratvantage.LegionControl1.policy",
    "%{_datadir}/icons/hicolor/scalable/apps/org.ratvantage.LegionControl.svg",
]:
    if required not in spec:
        die(f"RPM spec missing {required}")

print("release packaging static checks passed")
PY
then
  pass "release packaging static checks"
else
  fail "release packaging static checks"
fi

printf '\nrelease packaging validation summary: failures=%s warnings=%s skips=%s\n' \
  "$failures" "$warnings" "$skips"

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi
