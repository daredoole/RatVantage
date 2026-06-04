#!/usr/bin/env bash
# One-time setup for OpenRGB-style keyboard RGB device-node access.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: sudo scripts/setup-keyboard-rgb-openrgb-access.sh [options]

Adds a user to the i2c group when needed, loads i2c-dev at boot, and installs
a udev rule that keeps /dev/i2c-* nodes group-writable for OpenRGB-compatible
access.

Options:
  --user <user>       User to add to the i2c group. Default: SUDO_USER.
  --dry-run           Print intended actions without changing the system.
  -h, --help          Show this help.

After a real run, log out and back in before expecting new group membership.
EOF
}

target_user="${SUDO_USER:-}"
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user)
      target_user="${2:?missing value for --user}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
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

if [[ -z "$target_user" || "$target_user" == "root" ]]; then
  echo "could not determine a non-root target user; pass --user <user>" >&2
  exit 2
fi

if [[ "$dry_run" -eq 0 && "$(id -u)" -ne 0 ]]; then
  echo "run once with sudo: sudo $0 --user $target_user" >&2
  exit 2
fi

run() {
  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry_run:'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

write_file() {
  local path="$1"
  local mode="$2"
  local content="$3"
  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry_run: install -m%s %s\n' "$mode" "$path"
    printf '%s\n' "$content"
  else
    install -D -m"$mode" /dev/null "$path"
    printf '%s\n' "$content" >"$path"
  fi
}

if ! id "$target_user" >/dev/null 2>&1; then
  echo "target user does not exist: $target_user" >&2
  exit 2
fi

if ! getent group i2c >/dev/null 2>&1; then
  run groupadd --system i2c
fi

if id -nG "$target_user" | tr ' ' '\n' | grep -qx i2c; then
  echo "$target_user is already in the i2c group."
else
  run usermod -aG i2c "$target_user"
fi
run modprobe i2c-dev

write_file \
  /etc/modules-load.d/ratvantage-openrgb-i2c.conf \
  0644 \
  "i2c-dev"

write_file \
  /etc/udev/rules.d/60-ratvantage-openrgb-i2c.rules \
  0644 \
  'KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"'

run udevadm control --reload-rules
run udevadm trigger --subsystem-match=i2c-dev

echo "Installed OpenRGB keyboard RGB access setup for $target_user."
echo "Log out and back in, then run:"
target_home="$(getent passwd "$target_user" | cut -d: -f6 || true)"
if [[ -n "$target_home" && -x "$target_home/.local/bin/ratvantage-check-keyboard-rgb-openrgb" ]]; then
  echo "  $target_home/.local/bin/ratvantage-check-keyboard-rgb-openrgb --output target/validation/keyboard-rgb-openrgb-readiness"
else
  echo "  scripts/check-keyboard-rgb-openrgb.sh --output target/validation/keyboard-rgb-openrgb-readiness"
fi
