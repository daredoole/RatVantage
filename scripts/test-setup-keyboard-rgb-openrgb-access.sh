#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/setup-keyboard-rgb-openrgb-access.sh"

help_output="$("$script" --help)"
grep -q "setup-keyboard-rgb-openrgb-access.sh" <<<"$help_output"
grep -q "After a real run, log out and back in" <<<"$help_output"

dry_run_output="$("$script" --dry-run --user "$(id -un)")"
grep -Eq "(dry_run: usermod -aG i2c|already in the i2c group)" <<<"$dry_run_output"
grep -q "/etc/modules-load.d/ratvantage-openrgb-i2c.conf" <<<"$dry_run_output"
grep -q "/etc/udev/rules.d/60-ratvantage-openrgb-i2c.rules" <<<"$dry_run_output"
grep -q "Log out and back in" <<<"$dry_run_output"

echo "setup-keyboard-rgb-openrgb-access tests passed"
