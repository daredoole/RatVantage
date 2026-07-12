#!/usr/bin/env bash
# Print the broad RatVantage dev daemon flag set, one argument per line.
set -euo pipefail

keyboard_rgb_sdk_helper="${RATVANTAGE_OPENRGB_SDK_HELPER:-}"
if [[ -z "$keyboard_rgb_sdk_helper" ]]; then
  dev_user_home="${RATVANTAGE_DEV_USER_HOME:-}"
  if [[ -z "$dev_user_home" && -n "${SUDO_USER:-}" && "${SUDO_USER:-}" != "root" ]]; then
    dev_user_home="$(getent passwd "$SUDO_USER" | cut -d: -f6 || true)"
  fi
  if [[ -z "$dev_user_home" ]]; then
    dev_user_home="$HOME"
  fi
  keyboard_rgb_sdk_helper="$dev_user_home/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper"
fi

cat <<'EOF'
--enable-platform-profile-write
--enable-battery-charge-type-write
--enable-led-state-write
--enable-keyboard-rgb-write
--enable-ideapad-toggle-write
--enable-camera-power-write
--enable-usb-charging-write
--enable-fan-mode-write
--enable-gpu-mode-write
--enable-cpu-governor-write
--enable-cpu-epp-write
--enable-cpu-max-frequency-write
--enable-firmware-attribute-write
--enable-cpu-boost-write
--enable-conservation-mode-write
--enable-amd-gpu-dpm-write
--enable-wifi-power-save-write
--enable-curve-optimizer-write
--enable-openrgb-access-setup
--enable-hardware-profile-apply
--enable-automation-observer
EOF
printf '%s\n' --openrgb-sdk-helper "$keyboard_rgb_sdk_helper"
