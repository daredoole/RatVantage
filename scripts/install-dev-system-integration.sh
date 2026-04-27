#!/usr/bin/env bash
# One-time install of system D-Bus + polkit files from the repo so a locally
# built legion-control-daemon can own org.ratvantage.LegionControl1 on the
# system bus. For a dev systemd unit + D-Bus activation, see
# scripts/install-dev-systemd-ratvantage.sh (or use an RPM / foreground binary;
# docs/live-write-validation.md).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: sudo scripts/install-dev-system-integration.sh

Copies (from this repo):
  data/dbus/org.ratvantage.LegionControl1.conf -> /etc/dbus-1/system.d/
  data/polkit/org.ratvantage.LegionControl1.policy -> /usr/share/polkit-1/actions/

Then reloads D-Bus config and polkit so writes take effect.

After this, either:

  sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-platform-profile-write
  sudo systemctl daemon-reload
  sudo systemctl enable --now legion-control-daemon.service

or run the daemon in the foreground (example — platform profile writes only):

  sudo mkdir -p /var/lib/legion-control
  sudo ./target/release/legion-control-daemon --enable-platform-profile-write

Build the binary first:
  cargo build --release -p legion-control-daemon
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$(id -u)" -ne 0 ]]; then
  echo "run with sudo: sudo $0" >&2
  exit 2
fi

install -D -m 0644 "$repo_root/data/dbus/org.ratvantage.LegionControl1.conf" \
  /etc/dbus-1/system.d/org.ratvantage.LegionControl1.conf

install -D -m 0644 "$repo_root/data/polkit/org.ratvantage.LegionControl1.policy" \
  /usr/share/polkit-1/actions/org.ratvantage.LegionControl1.policy

if busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig 2>/dev/null; then
  echo "Reloaded D-Bus system configuration."
else
  echo "warning: could not reload D-Bus via busctl; try: sudo systemctl reload dbus-broker" >&2
  echo "  or reboot if the daemon still cannot claim org.ratvantage.LegionControl1." >&2
fi

if systemctl reload polkit.service 2>/dev/null; then
  echo "Reloaded polkit."
else
  echo "note: polkit reload skipped (service name may differ); new policy should load on next auth or reboot." >&2
fi

echo
echo "Installed. Next:"
echo "  cargo build --release -p legion-control-daemon"
echo "  sudo ./scripts/install-dev-systemd-ratvantage.sh ./target/release/legion-control-daemon -- --enable-platform-profile-write"
echo "  sudo systemctl daemon-reload && sudo systemctl enable --now legion-control-daemon.service"
echo "  # or foreground: sudo mkdir -p /var/lib/legion-control && sudo ./target/release/legion-control-daemon --enable-platform-profile-write"
echo "  # other terminal:"
echo "  cargo run -q -p legion-control-ui -- --diagnostics | head"
