#!/usr/bin/env bash
# Install a locally built legion-control-daemon under /usr/local and register
# systemd + D-Bus activation so `systemctl enable --now legion-control-daemon`
# works without the RPM.
#
# Prerequisites: scripts/install-dev-system-integration.sh (D-Bus policy + polkit).
#
# Usage:
#   sudo ./scripts/install-dev-systemd-ratvantage.sh /path/to/legion-control-daemon [-- daemon args...]
# Example (platform profile writes only):
#   sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-platform-profile-write
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: sudo ./scripts/install-dev-systemd-ratvantage.sh <daemon-binary> [-- <args...>]

Copies the binary to /usr/local/libexec/ratvantage/legion-control-daemon and
writes:
  /etc/systemd/system/legion-control-daemon.service
  /etc/dbus-1/system-services/org.ratvantage.LegionControl1.service

Refuses to run if any installed RPM matches ^legion-control (avoid fighting the
packaged unit). Stop any foreground `sudo ./target/.../legion-control-daemon`
before `systemctl enable --now`.

After this script:
  sudo systemctl daemon-reload
  sudo busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig
  sudo systemctl enable --now legion-control-daemon.service
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$(id -u)" -ne 0 ]]; then
  echo "run with sudo: sudo $0 ..." >&2
  exit 2
fi

if rpm -qa | grep -E '^legion-control' >/dev/null 2>&1; then
  echo "refusing: a legion-control* RPM is installed. Remove it before dev install." >&2
  exit 2
fi

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi

bin_src="$(readlink -f "$1")"
shift
extra=()
if (($#)) && [[ "$1" == "--" ]]; then
  shift
  extra=("$@")
elif (($#)); then
  echo "extra daemon arguments must follow -- (see --help)" >&2
  exit 2
fi

if [[ ! -f "$bin_src" || ! -x "$bin_src" ]]; then
  echo "daemon binary must exist and be executable: $bin_src" >&2
  exit 2
fi

inst_bin=/usr/local/libexec/ratvantage/legion-control-daemon
install -d /usr/local/libexec/ratvantage
install -m0755 "$bin_src" "$inst_bin"
# ReadWritePaths in the unit requires these paths to exist before systemd sets up mount namespaces.
install -d -m0755 /etc/legion-control

exec_cmd=("$inst_bin")
exec_cmd+=("${extra[@]}")
# systemd ExecStart= single string with spaces between argv pieces
exec_start=""
for part in "${exec_cmd[@]}"; do
  if [[ -z "$exec_start" ]]; then
    exec_start="$part"
  else
    exec_start+=" $part"
  fi
done

unit_file=/etc/systemd/system/legion-control-daemon.service
dbus_svc=/etc/dbus-1/system-services/org.ratvantage.LegionControl1.service

install -d /etc/dbus-1/system-services

cat >"$unit_file" <<UNIT
[Unit]
Description=Legion Control hardware daemon (RatVantage dev install)
Documentation=https://github.com/daredoole/RatVantage
After=dbus.service multi-user.target

[Service]
Type=dbus
BusName=org.ratvantage.LegionControl1
ExecStart=$exec_start
Restart=on-failure
StateDirectory=legion-control
ReadWritePaths=/sys/firmware/acpi /sys/class/power_supply /sys/class/leds /sys/class/hwmon /sys/class/firmware-attributes /var/lib/legion-control /etc/legion-control /sys/bus/platform/drivers

[Install]
WantedBy=multi-user.target
UNIT

dbus_exec=""
for part in "${exec_cmd[@]}"; do
  dbus_exec+=" $part"
done
dbus_exec="${dbus_exec# }"

cat >"$dbus_svc" <<DBUS
[D-BUS Service]
Name=org.ratvantage.LegionControl1
Exec=$dbus_exec
User=root
SystemdService=legion-control-daemon.service
DBUS

unit_tmp="$(mktemp --suffix=.service)"
trap 'rm -f "$unit_tmp"' EXIT
cp "$unit_file" "$unit_tmp"
if ! systemd-analyze verify "$unit_tmp" 2>/dev/null; then
  echo "warning: systemd-analyze verify reported issues; check $unit_file" >&2
fi

if busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig 2>/dev/null; then
  echo "Reloaded D-Bus system configuration."
else
  echo "warning: D-Bus ReloadConfig failed; reboot may be needed before activation works." >&2
fi

systemctl daemon-reload

echo
echo "Installed dev unit + D-Bus activation."
echo "Stop any foreground legion-control-daemon, then:"
echo "  sudo systemctl enable --now legion-control-daemon.service"
echo "  systemctl status legion-control-daemon.service"
echo
echo "Re-run this script after rebuilding the binary; it overwrites $inst_bin"
