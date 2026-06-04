#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

stale_helper="$tmp/ratvantage-dev-update-daemon"
setup_helper="$tmp/ratvantage-setup-keyboard-rgb-openrgb-access"
fake_sudo="$tmp/sudo"
sudo_log="$tmp/sudo.log"
stdout="$tmp/stdout"
stderr="$tmp/stderr"

cat >"$stale_helper" <<'SH'
#!/usr/bin/env bash
echo "stale helper should not run" >&2
exit 99
SH
chmod 0755 "$stale_helper"

cat >"$fake_sudo" <<SH
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$sudo_log"
exit 1
SH
chmod 0755 "$fake_sudo"

set +e
RATVANTAGE_UPDATE_DEV_SKIP_DAEMON_BUILD=1 \
RATVANTAGE_DEV_UPDATE_HELPER="$stale_helper" \
RATVANTAGE_OPENRGB_SETUP_HELPER="$setup_helper" \
RATVANTAGE_SUDO_BIN="$fake_sudo" \
  "$repo_root/scripts/update-dev-install.sh" --daemon --no-restart-tray \
  >"$stdout" 2>"$stderr" </dev/null
status=$?
set -e

if [[ "$status" -eq 0 ]]; then
  echo "expected stale passwordless updater path to fail without interactive sudo" >&2
  exit 1
fi

grep -q "Passwordless daemon updater is installed but stale" "$stderr"
grep -q "$setup_helper" "$stderr"
grep -q "No interactive sudo is available" "$stderr"
grep -q -- "-n true" "$sudo_log"
if grep -q "Installing user tray/dashboard" "$stdout"; then
  echo "stale daemon helper path should stop before user install" >&2
  exit 1
fi

echo "update-dev-install passwordless stale-helper test passed"
