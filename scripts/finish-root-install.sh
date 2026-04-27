#!/usr/bin/env bash
# Run once on your machine (interactive sudo) after `cargo build --release`.
# Installs the systemd daemon; tray/UI use ~/.local/bin from install-user-session.sh.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
daemon="$repo_root/target/release/legion-control-daemon"
if [[ ! -x "$daemon" ]]; then
  echo "missing $daemon — run: cargo build --release -p legion-control-daemon" >&2
  exit 2
fi
if [[ "$(id -u)" -ne 0 ]]; then
  echo "run with sudo: sudo $0" >&2
  exit 2
fi
"$repo_root/scripts/install-dev-systemd-ratvantage.sh" "$daemon" -- --enable-platform-profile-write
systemctl daemon-reload
systemctl enable --now legion-control-daemon.service
systemctl status legion-control-daemon.service --no-pager
