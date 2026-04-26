#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-sysfs-fixture.sh --output <fixture-dir> [--sysfs-root <root>]

Copies a narrow, read-only sysfs snapshot for RatVantage probe fixtures.
The output directory receives paths under sys/ matching the probe's supported
read-only inputs. No writes are made to the source sysfs tree.
EOF
}

sysfs_root="/"
output=""

while (($#)); do
  case "$1" in
    --sysfs-root)
      sysfs_root="${2:?missing value for --sysfs-root}"
      shift 2
      ;;
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
    --help|-h)
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

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

if [[ "$output" == "/" ]]; then
  echo "refusing to write fixture output to /" >&2
  exit 2
fi

sysfs_root="${sysfs_root%/}"
[[ -z "$sysfs_root" ]] && sysfs_root="/"

mkdir -p "$output"
manifest="$output/fixture-manifest.txt"
: >"$manifest"

copy_one() {
  local rel="$1"
  local src
  if [[ "$sysfs_root" == "/" ]]; then
    src="/$rel"
  else
    src="$sysfs_root/$rel"
  fi

  [[ -r "$src" ]] || return 0

  local dst="$output/$rel"
  mkdir -p "$(dirname "$dst")"
  timeout 2s head -c 65536 "$src" >"$dst" 2>/dev/null || {
    rm -f "$dst"
    echo "skipped unreadable: $rel" >>"$manifest"
    return 0
  }
  echo "captured: $rel" >>"$manifest"
}

copy_glob() {
  local pattern="$1"
  local root_prefix
  if [[ "$sysfs_root" == "/" ]]; then
    root_prefix="/"
  else
    root_prefix="$sysfs_root/"
  fi

  shopt -s nullglob
  local path
  for path in "$root_prefix"$pattern; do
    [[ -e "$path" ]] || continue
    copy_one "${path#"$root_prefix"}"
  done
  shopt -u nullglob
}

copy_one "sys/class/dmi/id/sys_vendor"
copy_one "sys/class/dmi/id/product_name"
copy_one "sys/class/dmi/id/product_version"
copy_one "sys/class/dmi/id/product_sku"

copy_one "sys/firmware/acpi/platform_profile"
copy_one "sys/firmware/acpi/platform_profile_choices"

copy_glob "sys/class/power_supply/*/charge_type"
copy_glob "sys/class/power_supply/*/charge_types"

copy_glob "sys/class/hwmon/*/name"
copy_glob "sys/class/hwmon/*/fan*_input"
copy_glob "sys/class/hwmon/*/fan*_label"
copy_glob "sys/class/hwmon/*/temp*_input"
copy_glob "sys/class/hwmon/*/temp*_label"
copy_glob "sys/class/hwmon/*/pwm*_auto_point*_pwm"
copy_glob "sys/class/hwmon/*/pwm*_auto_point*_temp"

copy_glob "sys/class/leds/*/brightness"
copy_glob "sys/class/leds/*/max_brightness"

copy_glob "sys/class/firmware-attributes/*/attributes/*/current_value"
copy_glob "sys/class/firmware-attributes/*/attributes/*/display_name"
copy_glob "sys/class/firmware-attributes/*/attributes/*/type"
copy_glob "sys/class/firmware-attributes/*/attributes/*/possible_values"

copy_glob "sys/bus/platform/drivers/ideapad_acpi/*/camera_power"
copy_glob "sys/bus/platform/drivers/ideapad_acpi/*/conservation_mode"
copy_glob "sys/bus/platform/drivers/ideapad_acpi/*/fn_lock"
copy_glob "sys/bus/platform/drivers/ideapad_acpi/*/touchpad"
copy_glob "sys/bus/platform/drivers/ideapad_acpi/*/usb_charging"

captured_count="$(grep -c '^captured:' "$manifest" || true)"
echo "captured $captured_count files into $output"
