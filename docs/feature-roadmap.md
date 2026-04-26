# Feature Roadmap

## Completed scaffold

Current pre-alpha code provides the safe read-only base:

- Runtime probe for hardware summary, capabilities, telemetry, and raw probe report.
- Root-capable daemon shape with read-only D-Bus APIs plus gated reversible execution for `SetPlatformProfile` and `SetBatteryChargeType`.
- UI status client and optional GTK4/libadwaita shell with read-only Status, Profiles, Battery, Fans, Appearance, and Diagnostics tabs.
- Read-only tray/status helper with a state-driven menu derived from detected profile choices, battery charge choices, packaged preset labels, and pending runtime state.
- Read-only StatusNotifier tray backend with dashboard, refresh, quit, and `--menu-check` diagnostics for the runtime-derived menu.
- StatusNotifier dashboard launch forwards custom D-Bus addresses for private/session-bus workflows.
- Tray tooltip reports platform profile, fan RPM, and available/missing capabilities.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke coverage for the optional shell.
- Fedora packaging assets for systemd, D-Bus, polkit, desktop metadata, AppStream metadata, and RPM spec.
- Packaging metadata validation in local and GitHub CI.
- Read-only sysfs fixture capture workflow for adding real hardware reports.
- Read-only compatibility bundle workflow for external hardware submissions, including generated summary files and PR body text.
- StatusNotifier tray smoke script and manual checklist.
- KDE StatusNotifier smoke report workflow with reusable report bundles.
- Read-only tray desktop diagnostics via `legion-control-tray --desktop-check`.
- Disabled draft write-method contracts for platform profile, battery charge type, GPU mode, and fan presets.
- Pure validators for platform profile, battery charge type, EnvyControl GPU mode, and packaged fan preset choices.
- Validator-backed dry-run planning for platform profile, battery charge type, GPU mode, and fan presets.
- Gated platform-profile and battery charge type write execution paths with rollback-on-readback-mismatch tests and matching UI CLI entry points. [implemented, still disabled by default unless daemon write flags are enabled]
- Daemon-side Rust adapters for dry-run planning, plus gated `SetPlatformProfile` and `SetBatteryChargeType` execution while other D-Bus write methods remain absent.
- Runtime-captured 82WM fixture coverage, including bracketed battery `charge_types` current-value parsing.
- Current 82WM read-only validation evidence for DMI, platform profiles, charge choices, sensors, LEDs, firmware toggles, and EnvyControl.
- Read-only battery telemetry for capacity, status, and health where `BAT0` exposes it.
- Read-only EnvyControl GPU mode query when `envycontrol --query` is available.
- Read-only UI `--overview` summary for platform profile, battery mode, fan RPM, temperatures, GPU mode, battery telemetry, LED brightness, and firmware toggle values.
- UI overview includes app-state-only GPU pending-reboot status and saved fan curve snapshots when recorded by the daemon.
- Tray/status output and GTK pages surface app-state-only GPU pending-reboot state and saved fan curve snapshots.
- Read-only UI `--diagnostics` JSON bundle with hardware summary, compact counts, kernel version, detected sysfs paths, recent daemon log excerpts, and raw probe report.
- Diagnostics/export parity for durable app state, so CLI `--diagnostics` output and GTK Diagnostics Copy JSON include `gpu_mode_pending` and `last_known_good_fan_curve`.
- Diagnostics include `platform_profile_choices` and `charge_types` source paths.
- UI dry-run plan previews for platform profile, battery charge type, GPU mode, fan preset, and fan restore/default writes, plus gated `--set-platform-profile` and `--set-battery-charge-type` execution output.
- UI status output includes per-capability status and risk labels.
- Read-only GTK Profiles, Battery, and Fans tabs show platform profile choices, battery charge choices, fan telemetry, fan curve paths, packaged preset IDs, sysfs source paths, and battery telemetry from the diagnostics bundle.
- Read-only GTK Appearance tab shows LED brightness nodes and firmware toggle values from the diagnostics bundle.
- Read-only GTK diagnostics tab for the same hardware/debug bundle, including Copy JSON.
- Packaged read-only fan preset TOML assets with CI schema validation and runtime dry-run planning.
- Fixture tests, private-bus integration tests, clippy/fmt/test local CI, and GitHub CI.

## Next immediate work

- Keep tray autostart disabled; GNOME AppIndicator extension path is still untested.
- Collect more captured fixtures through the compatibility bundle workflow when additional supported Legion machines are available.
- If no new hardware reports are available, continue with read-only UI/tray polish; KDE-specific smoke/reporting is now in place, while GNOME validation remains blocked.
- Keep progress docs current after each completed roadmap slice.
- Keep GitHub CI as remote guard; run `./scripts/ci-local.sh` before pushing to reduce failed CI minutes.

## MVP

Goal: safe, useful daily controls using only confirmed interfaces and conservative wrappers.

### Core app

- Runtime capability probe.
- Root system daemon with D-Bus API.
- polkit-gated write actions.
- GTK4/libadwaita dashboard.
- Optional tray/status process.
- JSON probe report export.
- Journald logging.
- Last-known-good state storage.

### Overview

- Current platform profile. [implemented in `--overview`]
- Current battery charge type. [implemented in `--overview`]
- Fan 1 / fan 2 RPM. [implemented in `--overview`]
- Temperature telemetry with sensor labels where available. [implemented in `--overview`]
- GPU mode from EnvyControl if installed. [implemented in `--overview` as read-only query]
- Basic battery capacity/status/health where exposed. [implemented in `--overview` for `BAT0` telemetry]
- LED brightness and firmware toggle values where exposed. [implemented in `--overview` as read-only data]

### Profiles

- Show exact values from `/sys/firmware/acpi/platform_profile_choices`. [implemented as read-only GTK page]
- Surface detected profile choices in the tray menu without enabling writes. [implemented]
- Allow gated platform-profile execution with read-back and rollback. [implemented in daemon/UI CLI, disabled by default unless the daemon write flag is enabled]
- Allow setting only listed profiles.
- Do not expose `custom` or `max-power` unless listed.
- Re-read fan curve and telemetry after profile changes.

### Battery

- Show `Fast`, `Standard`, `Long_Life` from `/sys/class/power_supply/BAT0/charge_types`. [implemented as read-only GTK page]
- Surface current charge type, detected charge choices, and battery telemetry in the tray menu without enabling writes. [implemented]
- Allow gated battery charge type execution with read-back and rollback. [implemented in daemon/UI CLI, disabled by default unless the daemon write flag is enabled]
- Allow setting exact charge type values.
- Show explanatory labels without claiming exact thresholds.

Suggested labels:

| Kernel value | UI label | Notes |
|---|---|---|
| `Fast` | Fast charge | Higher battery stress; not for always-on use. |
| `Standard` | Standard charge | Normal full-charge behavior. |
| `Long_Life` | Long life / conservation | Battery longevity mode; exact threshold is firmware-defined. |

### Fan presets

- Read detected fan curve capability.
- Provide packaged presets:
  - Quiet office [implemented as TOML asset and dry-run input]
  - Balanced daily [implemented as TOML asset and dry-run input]
  - Gaming [implemented as TOML asset and dry-run input]
  - Max safe [implemented as TOML asset and dry-run input]
- Surface packaged preset labels in the tray menu without enabling writes. [implemented]
- Apply full validated curve only. [implemented as dry-run validation only]
- Restore safe/default action. [implemented as dry-run planning only]
- Store last-known-good fan curve. [implemented as app-state-only capture]

### GPU

- Read `envycontrol --query` when available. [implemented]
- Offer guided switch to `integrated`, `hybrid`, or `nvidia` only if EnvyControl is installed and daemon validation passes. [implemented as dry-run planning only]
- Mark changes as pending reboot. [implemented in dry-run plan JSON]
- Provide clear rollback instructions. [implemented in dry-run plan JSON]

### Appearance

- Y-logo LED toggle if `/sys/class/leds/platform::ylogo/brightness` exists. [implemented as read-only GTK Appearance and `--overview` data]
- Fn-lock LED display only if `/sys/class/leds/platform::fnlock/brightness` exists. [implemented as read-only GTK Appearance and `--overview` data]

## Version 0.2

Goal: make the confirmed controls more complete and user-friendly.

### Fan curve editor

- 10-point visual editor.
- Read current curve when trustworthy.
- Warn if readback is incomplete, zeroed, or inconsistent.
- Validate full curve before applying.
- Export/import fan presets as TOML.
- Assign fan preset per platform profile.
- Re-apply selected fan preset after resume if enabled.

### Tray polish

- Better tooltip with fan RPM and profile. [implemented]
- State-driven read-only tray menu with current profile, battery telemetry, detected profile/charge choices, packaged preset labels, capability summaries, and pending app state. [implemented]
- `legion-control-tray --menu-check` diagnostics for the same runtime-derived menu. [implemented]
- Pending reboot indicator for GPU mode. [implemented as app-state-only daemon state plus overview, tray, and GTK output]
- Fallback behavior when GNOME AppIndicator extension is missing. [implemented as read-only guidance]

### Functional keyboard/peripheral probes

Expose only if present:

- functional Fn-lock via VPC2004 `fn_lock`; [implemented as read-only GTK Appearance and `--overview` data]
- always-on USB charging via VPC2004 `usb_charging`; [implemented as read-only GTK Appearance and `--overview` data when exposed]
- IO-port LED if a stable LED node exists.

### Diagnostics

- Hardware summary page. [implemented in CLI and GTK diagnostics surfaces]
- Raw capability registry viewer. [implemented in CLI and GTK diagnostics surfaces]
- CLI debug bundle via `legion-control-ui --diagnostics`. [implemented]
- Tray desktop/session diagnostics via `legion-control-tray --desktop-check`. [implemented]
- Compact diagnostics summary with capability, sensor, fan curve, and detected-path counts. [implemented]
- Copy debug bundle:
  - DMI model fields;
  - kernel version;
  - detected sysfs paths;
  - durable app-state fields `gpu_mode_pending` and `last_known_good_fan_curve`;
  - capability JSON; [implemented in CLI and GTK diagnostics surfaces]
  - recent daemon log excerpt. [implemented as best-effort `journalctl` read]
- Copy diagnostics JSON from the GTK viewer. [implemented]

## Version 0.3

Goal: add optional controls that are useful but not core to Legion thermal management.

### Peripherals

Expose only when probed and with warnings:

- camera power via VPC2004 `camera_power`;
- touchpad hardware toggle via VPC2004 `touchpad`;
- legacy conservation mode as compatibility diagnostic if `charge_types` is absent;
- backlight readout or `brightnessctl` wrapper only if users ask for it.

### Desktop integration

- Better PowerProfiles D-Bus owner detection.
- Optional sync policy between Lenovo platform profile and generic desktop power profile.
- Notifications for profile/fan reset after resume.
- KDE-specific tray behavior testing. [implemented as report-capable smoke workflow with recorded KDE Wayland evidence]
- GNOME-specific AppIndicator extension detection and smoke testing.

### Preset automation, local only

- Apply preset on AC plug/unplug.
- Apply quiet preset on battery.
- Apply gaming preset when selected process is running.
- All automation rules must show a clear log and be easy to disable.

## Advanced / experimental

These belong behind an Advanced page, disabled by default.

### Firmware PPT controls

Only expose if all conditions are true:

- `/sys/class/firmware-attributes/lenovo-wmi-other/attributes/` exists;
- `ppt_pl1_spl`, `ppt_pl2_sppt`, or `ppt_pl3_fppt` directories exist;
- each attribute has `current_value`, `min_value`, `max_value`, `scalar_increment`, and `type`;
- values can be read back;
- `custom` platform profile exists, or the docs for the current driver prove the value is effective in the current profile;
- polkit confirmation is required.

UI rules:

- PL1/SPL may be a slider with exact bounds.
- PL2/SPPT may be a slider with warning.
- PL3/FPPT should require explicit confirmation every time.
- Always show default/min/max/current.
- Always provide restore-default action.

### Fan target RPM

Expose only as a debug/advanced control if `fanX_target` exists. Prefer fan curves and presets for normal users.

### Display overdrive

Only expose if a stable path or maintained wrapper exists and read-back confirms state. Do not call raw WMI methods.

### DKMS-only features

Optional adapter only:

- rapid charge;
- win-key lock;
- fan full-speed/dust-cleaning;
- extra LEDs.

Do not require the out-of-tree module for the app to build, install, or run.

## Not planned unless proven safe

These should remain out of scope until a stable ABI, clear bounds, read-back verification, and rollback path exist.

- Raw WMI method calls.
- Raw EC memory writes.
- Arbitrary sysfs writer.
- CPU overclocking.
- GPU overclocking.
- Native keyboard RGB EC/HID payload writer.
- Runtime MUX/G-Sync toggles through raw WMI.
- `max-power` profile if not listed in `platform_profile_choices`.
- Any feature that cannot be hidden cleanly when missing.
