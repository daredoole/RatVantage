#!/usr/bin/env bash
# Dev-only convenience: allow one active local user to perform RatVantage
# hardware write actions without repeated polkit password prompts.
set -euo pipefail

target_user="${1:-${SUDO_USER:-${USER:-}}}"
rule_path="/etc/polkit-1/rules.d/49-ratvantage-dev-local-user.rules"

if [[ -z "$target_user" ]]; then
  echo "usage: sudo $0 <username>" >&2
  exit 2
fi

if [[ "$(id -u)" -ne 0 ]]; then
  echo "run with sudo: sudo $0 $target_user" >&2
  exit 2
fi

install -d -m0755 /etc/polkit-1/rules.d
cat >"$rule_path" <<EOF
// RatVantage dev convenience rule.
// Allows only the named active local user to perform RatVantage hardware writes.
polkit.addRule(function(action, subject) {
  var actions = [
    "org.ratvantage.LegionControl1.set-platform-profile",
    "org.ratvantage.LegionControl1.set-battery-charge-type",
    "org.ratvantage.LegionControl1.set-led-state",
    "org.ratvantage.LegionControl1.set-keyboard-rgb",
    "org.ratvantage.LegionControl1.set-ideapad-toggle",
    "org.ratvantage.LegionControl1.set-gpu-mode",
    "org.ratvantage.LegionControl1.set-cpu-governor",
    "org.ratvantage.LegionControl1.set-cpu-epp",
    "org.ratvantage.LegionControl1.set-firmware-attribute",
    "org.ratvantage.LegionControl1.set-cpu-boost",
    "org.ratvantage.LegionControl1.set-conservation-mode",
    "org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level",
    "org.ratvantage.LegionControl1.set-curve-optimizer",
    "org.ratvantage.LegionControl1.setup-openrgb-access",
    "org.ratvantage.LegionControl1.apply-hardware-profile"
  ];

  if (actions.indexOf(action.id) >= 0 &&
      subject.local &&
      subject.active &&
      subject.user == "$target_user") {
    return polkit.Result.YES;
  }
});
EOF

chmod 0644 "$rule_path"
echo "Installed $rule_path for user: $target_user"
echo "Remove it with: sudo rm $rule_path"
