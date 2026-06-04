#!/usr/bin/env bash
# One-time dev setup: install a root-owned helper plus a narrow sudoers rule so
# scripts/update-dev-install.sh --daemon can refresh the system daemon without a
# password prompt.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(readlink -f "$repo_root")"
current_user="${SUDO_USER:-$(id -un)}"

usage() {
  cat <<EOF
Usage: sudo scripts/install-dev-passwordless-updater.sh

Installs:
  /usr/local/sbin/ratvantage-dev-update-daemon
  /etc/sudoers.d/ratvantage-dev-update-daemon-$current_user

This is intentionally dev-only. It lets $current_user replace and restart the
root RatVantage daemon from this worktree without a sudo password:
  $repo_root/scripts/update-dev-install.sh --daemon
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$(id -u)" -ne 0 ]]; then
  echo "run once with sudo: sudo $0" >&2
  exit 2
fi

if [[ -z "$current_user" || "$current_user" == "root" ]]; then
  echo "could not determine the non-root user for sudoers" >&2
  exit 2
fi

helper=/usr/local/sbin/ratvantage-dev-update-daemon
openrgb_setup_helper=/usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access
sudoers=/etc/sudoers.d/ratvantage-dev-update-daemon-$current_user

install -d -m0755 /usr/local/sbin
cat >"$helper" <<EOF
#!/usr/bin/env bash
set -euo pipefail
# ratvantage-dev-update-daemon-capability: repo-driven-daemon-args-v1

allowed_repo="$repo_root"
repo_root="\${1:-}"
if [[ -z "\$repo_root" ]]; then
  echo "usage: ratvantage-dev-update-daemon $repo_root" >&2
  exit 2
fi
repo_root="\$(readlink -f "\$repo_root")"
if [[ "\$repo_root" != "\$allowed_repo" ]]; then
  echo "refusing repo outside allowed dev worktree: \$repo_root" >&2
  exit 2
fi

if rpm -qa | grep -E '^legion-control' >/dev/null 2>&1; then
  echo "refusing: a legion-control* RPM is installed. Remove it before dev install." >&2
  exit 2
fi

bin_src="\$repo_root/target/release/legion-control-daemon"
if [[ ! -f "\$bin_src" || ! -x "\$bin_src" ]]; then
  echo "daemon binary must exist and be executable: \$bin_src" >&2
  exit 2
fi

install -D -m0644 "\$repo_root/data/dbus/org.ratvantage.LegionControl1.conf" \\
  /etc/dbus-1/system.d/org.ratvantage.LegionControl1.conf
install -D -m0644 "\$repo_root/data/polkit/org.ratvantage.LegionControl1.policy" \\
  /usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy
install -D -m0755 "\$repo_root/scripts/setup-keyboard-rgb-openrgb-access.sh" \\
  /usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access
if [[ -x "\$repo_root/scripts/install-dev-polkit-local-user-rule.sh" ]]; then
  "\$repo_root/scripts/install-dev-polkit-local-user-rule.sh" "$current_user" >/dev/null
fi

inst_bin=/usr/local/libexec/ratvantage/legion-control-daemon
install -d -m0755 /usr/local/libexec/ratvantage /etc/legion-control /etc/dbus-1/system-services
install -m0755 "\$bin_src" "\$inst_bin"

unit_file=/etc/systemd/system/legion-control-daemon.service
dbus_svc=/etc/dbus-1/system-services/org.ratvantage.LegionControl1.service
daemon_args_file="\$repo_root/scripts/dev-daemon-args.sh"
if [[ ! -x "\$daemon_args_file" ]]; then
  echo "daemon args helper must exist and be executable: \$daemon_args_file" >&2
  exit 2
fi
mapfile -t daemon_args < <("\$daemon_args_file")

exec_start="\$inst_bin"
for arg in "\${daemon_args[@]}"; do
  exec_start+=" \$arg"
done

cat >"\$unit_file" <<UNIT
[Unit]
Description=Legion Control hardware daemon (RatVantage dev install)
Documentation=https://github.com/daredoole/RatVantage
After=dbus.service multi-user.target

[Service]
Type=dbus
BusName=org.ratvantage.LegionControl1
ExecStart=\$exec_start
Restart=on-failure
StateDirectory=legion-control
ReadWritePaths=/sys/firmware/acpi /sys/class/power_supply /sys/class/leds /sys/class/hwmon /sys/class/firmware-attributes /sys/class/drm /sys/bus/pci/devices /sys/devices /var/lib/legion-control /etc/legion-control /sys/bus/platform/drivers

[Install]
WantedBy=multi-user.target
UNIT

cat >"\$dbus_svc" <<DBUS
[D-BUS Service]
Name=org.ratvantage.LegionControl1
Exec=\$exec_start
User=root
SystemdService=legion-control-daemon.service
DBUS

systemctl daemon-reload
busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig >/dev/null || true
systemctl reload polkit.service 2>/dev/null || true
systemctl enable --now legion-control-daemon.service >/dev/null
systemctl restart legion-control-daemon.service
echo "daemon=updated"
EOF
chmod 0755 "$helper"
chown root:root "$helper"

cat >"$sudoers" <<EOF
$current_user ALL=(root) NOPASSWD: $helper $repo_root
$current_user ALL=(root) NOPASSWD: $openrgb_setup_helper --user $current_user
EOF
chmod 0440 "$sudoers"
chown root:root "$sudoers"

if ! visudo -cf "$sudoers" >/dev/null; then
  rm -f "$sudoers"
  echo "sudoers validation failed; removed $sudoers" >&2
  exit 1
fi

echo "Installed passwordless RatVantage dev updater for $current_user."
echo "Future daemon refresh:"
echo "  $repo_root/scripts/update-dev-install.sh --daemon"
