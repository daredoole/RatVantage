# Write Contracts

`legion_common::WRITE_METHOD_CONTRACTS` is the source of truth for write-surface
metadata. Some reversible contracts are now implemented behind daemon policy
flags and polkit authorization, while higher-risk surfaces remain draft-only.

The active daemon exposes the following read-only methods:

- `GetHardwareSummary`
- `GetCapabilities`
- `RefreshCapabilities`
- `GetTelemetry`
- `GetRawProbeReport`
- `PlanPlatformProfileWrite`
- `PlanBatteryChargeTypeWrite`
- `PlanLedStateWrite`
- `PlanIdeapadToggleWrite`
- `PlanGpuModeWrite`
- `PlanFirmwareAttributeWrite`
- `PlanCpuBoostWrite`
- `PlanCurveOptimizerAllCoreWrite`
- `PlanConservationModeWrite`
- `PlanAmdGpuDpmForceLevelWrite`
- `PlanFanPresetWrite`
- `PlanRestoreAutoFanWrite`
- `GetHardwareProfiles`
- `GetHardwareProfileTriggers`
- `GetHardwareProfileApplyPreview`
- `GetHardwareProfileTriggerApplyPreview`
- `GetLastHardwareProfileApply`

And the following gated write methods:

- `SetPlatformProfile`
- `SetBatteryChargeType`
- `SetLedState`
- `SetIdeapadToggle`
- `SetGpuMode`
- `SetCpuGovernor`
- `SetCpuEpp`
- `SetFirmwareAttribute`
- `SetCpuBoost`
- `SetConservationMode`
- `SetAmdGpuDpmForceLevel`
- `SetCurveOptimizerAllCore`
- `ApplyHardwareProfile`
- `ApplyHardwareProfileTrigger`

All of those execution paths still remain disabled by default unless the daemon
is started with the matching write-enable flags.

## Implemented Gated Writes

| Method | Capability | polkit action | Request | Safety scope |
|---|---|---|---|---|
| `SetPlatformProfile` | `platform_profile` | `org.ratvantage.LegionControl1.set-platform-profile` | `{"profile":"string"}` | exact runtime choice validation, read-back, rollback |
| `SetBatteryChargeType` | `battery_charge_type` | `org.ratvantage.LegionControl1.set-battery-charge-type` | `{"charge_type":"string"}` | exact runtime choice validation, read-back, rollback |
| `SetLedState` | `leds` | `org.ratvantage.LegionControl1.set-led-state` | `{"led_id":"platform::ylogo","enabled":"bool"}` | currently restricted to `platform::ylogo`, binary LED only |
| `SetIdeapadToggle` | `ideapad_toggles` | `org.ratvantage.LegionControl1.set-ideapad-toggle` | `{"toggle_id":"fn_lock|camera_power|usb_charging|fan_mode","enabled":"bool"}` | allowlisted ideapad toggles only; `fn_lock` also requires paired `platform::fnlock` LED corroboration |
| `SetGpuMode` | `gpu` | `org.ratvantage.LegionControl1.set-gpu-mode` | `{"mode":"integrated|hybrid|nvidia"}` | EnvyControl daemon execution, reboot-pending state recorded after command success |
| `SetCpuGovernor` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-governor` | `{"governor":"string"}` | exact runtime choice validation, per-policy opt-in |
| `SetCpuEpp` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-epp` | `{"epp":"string"}` | exact runtime choice validation, per-policy opt-in |
| `SetFirmwareAttribute` | `firmware_attributes` | `org.ratvantage.LegionControl1.set-firmware-attribute` | `{"attribute_id":"ppt_pl1_spl|ppt_pl2_sppt|ppt_pl3_fppt","value":"integer"}` | 82WM PPT allowlist, scalar min/max/increment validation, read-back, rollback |
| `SetCpuBoost` | `cpu_power` | `org.ratvantage.LegionControl1.set-cpu-boost` | `{"boost":"0|1"}` | binary validator, read-back, rollback |
| `SetConservationMode` | `ideapad_toggles` | `org.ratvantage.LegionControl1.set-conservation-mode` | `{"enabled":"0|1"}` | dedicated conservation-mode write path through ideapad toggle validation |
| `SetAmdGpuDpmForceLevel` | `amd_gpu_power_dpm` | `org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level` | `{"level":"auto|low"}` | exact driver choice validation, read-back, rollback |
| `SetCurveOptimizerAllCore` | `curve_optimizer_all_core` | `org.ratvantage.LegionControl1.set-curve-optimizer` | `{"offset":"0|-1..-30"}` | experimental RyzenAdj backend, write-only without `ryzen_smu`, explicit reset-to-zero path |
| `ApplyHardwareProfile` | `hardware_profiles` | `org.ratvantage.LegionControl1.apply-hardware-profile` | `{"profile_id":"string"}` | validates stored profile first, requires `--enable-hardware-profile-apply`, executes existing gated actions in preview order, stops on first non-applied action, records last run |
| `ApplyHardwareProfileTrigger` | `hardware_profile_triggers` | `org.ratvantage.LegionControl1.apply-hardware-profile` | `{"trigger_id":"ac_connected|ac_disconnected|resume|platform_profile_changed|manual"}` | resolves a persisted trigger mapping to a stored profile, then uses the same profile apply gate and execution path |

## Disabled Drafts

| Method | Capability | Future polkit action | Request | Risk |
|---|---|---|---|---|
| `ApplyFanPreset` | `fan_curves` | `org.ratvantage.LegionControl1.apply-fan-preset` | `{"preset_id":"string"}` | experimental write |
| `RestoreAutoFan` | `fan_curves` | `org.ratvantage.LegionControl1.restore-auto-fan` | `{}` | experimental write |

## Required Gates Before Enabling

- Keep new D-Bus write methods absent until validators and rollback/read-back behavior exist.
- Authorize each write through polkit inside the daemon, never through GUI sudo.
- Validate requested values against choices read at runtime.
- Use `validate_platform_profile_choice` and
  `validate_battery_charge_type_choice` before any future sysfs write.
- Use `validate_led_state_request` and `validate_ideapad_toggle_request`
  before any future LED or ideapad-toggle write expansion.
- Use `validate_gpu_mode_choice` before any EnvyControl GPU mode write.
- Use `validate_fan_preset_choice` before any future fan curve preset write.
- Use `plan_platform_profile_write`, `plan_battery_charge_type_write`,
  `plan_led_state_write`, `plan_ideapad_toggle_write`, `plan_gpu_mode_write`,
  `plan_fan_preset_write`, and `plan_restore_auto_fan_write` for
  validator-backed dry-run plans before any future daemon write implementation.
- Read back the changed sysfs value after each write.
- Store previous values before writing.
- Restore previous values on read-back failure when still safe and listed.
- Add private-bus contract tests for each enabled method.
- Add fixture coverage for success, unsupported, invalid choice, and rollback paths.

## Dry-Run Planning

Dry-run planning is pure shared logic in `legion-common`. It returns the future
method name, capability ID, polkit action, target path/tool, previous value,
requested value, read-back requirement, rollback value, rollback instructions,
reboot requirement, safety notes, and ordered execution step IDs.

The plan functions do not write sysfs. The daemon exposes them as read-only
D-Bus planning methods so clients can preview validation, polkit action, target
path, rollback value, and execution steps before any write method is attempted.
`legion-control-ui --plan-platform-profile <profile>` and
`legion-control-ui --plan-battery-charge-type <charge_type>` print sysfs-backed
plans as JSON for CLI inspection. `legion-control-ui --plan-led-state
<led_id=on|off>` and `legion-control-ui --plan-ideapad-toggle
<toggle_id=on|off>` do the same for the currently enabled reversible LED and
ideapad-toggle surfaces. `legion-control-ui --plan-gpu-mode <mode>` prints the
EnvyControl GPU mode plan, marks the future change as reboot required, and
includes rollback guidance for the previous mode.
`legion-control-ui --plan-fan-preset <preset_id>` prints the packaged fan preset
plan only when the preset schema is valid and the detected fan curve exposes a
complete 10-point writable shape.
`legion-control-ui --plan-restore-auto-fan` prints the future restore/default
fan-control plan when a fan curve capability is detected.
Higher-risk draft write methods should remain outside the zbus `#[interface]`
implementation until write support is deliberately enabled.

## Out Of Scope

- Raw WMI calls.
- Raw EC writes.
- Arbitrary sysfs writes.
- Overclocking controls.
- Raw firmware attribute writes outside the explicit PPT allowlist and validator.
