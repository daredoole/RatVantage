#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
args_script="$repo_root/scripts/dev-daemon-args.sh"
install_script="$repo_root/scripts/install-dev-passwordless-updater.sh"
update_script="$repo_root/scripts/update-dev-install.sh"

[[ -x "$args_script" ]]

args="$("$args_script")"
grep -q -- "--enable-openrgb-access-setup" <<<"$args"
grep -q -- "--enable-hardware-profile-apply" <<<"$args"
grep -q -- "--enable-automation-observer" <<<"$args"
grep -q -- "--enable-keyboard-rgb-write" <<<"$args"
grep -q -- "--openrgb-sdk-helper" <<<"$args"
grep -q -- "$HOME/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper" <<<"$args"

override_args="$(RATVANTAGE_DEV_USER_HOME=/tmp/ratvantage-dev-user-home "$args_script")"
grep -q -- "/tmp/ratvantage-dev-user-home/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper" <<<"$override_args"

grep -q "ratvantage-dev-update-daemon-capability: repo-driven-daemon-args-v1" "$install_script"
grep -q 'scripts/dev-daemon-args.sh' "$install_script"
grep -q 'mapfile -t daemon_args' "$install_script"

grep -q "ratvantage-dev-update-daemon-capability: repo-driven-daemon-args-v1" "$update_script"
grep -q 'scripts/dev-daemon-args.sh' "$update_script"
grep -q '"${daemon_args\[@\]}"' "$update_script"

echo "dev-daemon-args tests passed"
