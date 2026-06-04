# 82WM driver surface read/write GUI plan

Goal: promote the live 82WM driver surfaces from read-only/planning into validated daemon writes with GTK/tray UX, without violating RatVantage's safety model.

## Implementation status from this pass

Code-level support now exists for the highest-value exposed surfaces:

- Firmware PPT writes: daemon dry-run/write methods, allowlisted scalar validators, polkit action, CLI, GTK controls, fixture coverage, readback, and rollback.
- Conservation mode and CPU boost: daemon dry-run/write methods, boolean validators, opt-in daemon flags, polkit actions, CLI, GTK controls, readback, and rollback.
- CPU governor and EPP: daemon dry-run/write methods, choice validators, opt-in daemon flags, polkit actions, CLI, GTK controls, readback, rollback, and live evidence gates.
- Fan mode: fixture coverage and GUI exposure through the existing ideapad toggle write path, still requiring live `0 -> 1 -> 0` evidence before support is promoted in docs.
- AMD GPU DPM force level: daemon dry-run/write methods, opt-in flag, polkit action, CLI, GTK control, fixture coverage, readback, and rollback to the previous force level.
- EnvyControl GPU mode: daemon-only `SetGpuMode`, explicit `--enable-gpu-mode-write`, polkit gate, fake-binary private-bus coverage, GTK/CLI execution, and reboot-pending state recording after command success.
- RyzenAdj all-core Curve Optimizer: experimental daemon-only write path, explicit `--enable-curve-optimizer-write`, polkit gate, signed-offset validator, u32 encoding, GTK controls hidden behind an explicit Advanced CPU Tuning gate, explicit reset-to-zero button, CLI controls, fake-backend D-Bus coverage, and persisted write-only state.

Live evidence captured so far:

- CPU boost: accepted execute evidence in `target/validation/82wm-live-cpu_boost`; write to `0` read back successfully and revert to `1` read back successfully.
- CPU governor: accepted execute evidence in `target/validation/82wm-live-cpu_governor`; write and revert both read back successfully.
- CPU EPP: accepted execute evidence in `target/validation/82wm-live-cpu_epp`; write and revert both read back successfully.
- Conservation mode: accepted execute evidence in `target/validation/82wm-live-conservation_mode`; write to `0` and revert to `1` both read back successfully after the daemon was restarted with `--enable-conservation-mode-write`.
- AMD GPU DPM force level: accepted execute evidence in `target/validation/82wm-live-amd_gpu_dpm_force_level`; write to `low` and revert to `auto` both read back successfully.
- RyzenAdj all-core Curve Optimizer: accepted execute evidence in `target/validation/82wm-live-curve_optimizer_all_core`; `-20` apply produced write-only state and reset-to-zero produced write-only state through the daemon.
- EnvyControl GPU mode: accepted execute evidence in `target/validation/82wm-live-gpu_mode`; switching from `integrated` to `hybrid` completed and recorded reboot-required pending state.
- Hardware profile apply: accepted execute evidence in `target/validation/82wm-live-hardware_profile` using the seeded `validation_cpu_driver` profile with CPU governor, EPP, and boost action results.
- Hardware profile trigger apply: accepted execute evidence in `target/validation/82wm-live-hardware_profile_trigger` using the seeded `manual=validation_cpu_driver` trigger with CPU governor, EPP, and boost action results.
- Keyboard RGB: daemon/OpenRGB SDK execution works locally, and the standard live
  evidence matrix now requires a dedicated `target/validation/82wm-live-keyboard_rgb`
  bundle before this plan is complete.
- Firmware PPT limits: live execution reached the Lenovo WMI firmware attribute files, but all three writes returned `Device or resource busy (os error 16)`:
  - `target/validation/82wm-live-ppt_pl1_spl`
  - `target/validation/82wm-live-ppt_pl2_sppt`
  - `target/validation/82wm-live-ppt_pl3_fppt`
  Keep PPT controls as gated/experimental until the busy firmware state is understood or a retry/reboot-safe protocol is validated.
- Fan mode: live execution in `target/validation/82wm-live-fan_mode` did not pass promotion; requesting `1` read back `0`, and the daemon restored the previous `0` value. Keep fan mode unpromoted until the driver exposes a writable behavior or a different validated control path exists.
- Evidence gate: `scripts/verify-82wm-live-evidence.sh --root target/validation` now requires the 14-control matrix, with PPT and fan mode accepted only as negative evidence and keyboard RGB requiring a passing apply/revert bundle.

Still not promotable as full read/write support:

- PPT write support is not promotable yet because live writes return `EBUSY` from the firmware attribute driver.
- Fan mode is not promotable yet because live writes read back unchanged.
- Execute evidence and stability soak for RyzenAdj Curve Optimizer values; current read-back status remains write-only until a `ryzen_smu` backend exists.
- The broader automation/profile engine from Phase 7; current daemon persistence now includes saved hardware profiles, trigger-to-profile mappings, and last apply result, but automatic OS event observers remain gated on live write evidence.
- Hardware profiles now have a daemon-owned store, dry-run apply preview, manual daemon apply, CLI/GTK apply controls, tray visibility for last apply result, and per-action result recording for supported controls.
- Live validation tooling now plans the expanded control surface from fixture or live diagnostics and can execute individual advanced controls only when `--execute-only <control_id>` names that exact family. Current advanced control IDs include `cpu_governor`, `cpu_epp`, `cpu_boost`, `conservation_mode`, `firmware_attribute:ppt_pl1_spl`, `firmware_attribute:ppt_pl2_sppt`, `firmware_attribute:ppt_pl3_fppt`, `amd_gpu_dpm_force_level`, `keyboard_rgb`, `gpu_mode`, `curve_optimizer_all_core`, `hardware_profile`, and `hardware_profile_trigger`. The harness can also seed a narrow daemon hardware profile and trigger mapping before capture so profile evidence is reproducible.
- Local CI now asserts the fixture write-validation bundle contains clean dry-run plans for the advanced CPU/PPT/GPU controls and separately seeds `validation_cpu_driver` plus a trigger mapping to prove both `hardware_profile` and `hardware_profile_trigger` previews contain `SetCpuGovernor`, `SetCpuEpp`, and `SetCpuBoost`.
- Bundle review now supports explicit gates such as `--require-mode execute` and `--require-control cpu_boost=pass`, so live evidence can be checked by command instead of manual inspection alone; `scripts/test-review-write-validation-bundle.sh` now regresses the pass, wrong-mode, wrong-status, and missing-control cases.
- The full live verifier now treats PPT and fan mode as required negative execute evidence on this host: PPT rows must show Lenovo WMI `EBUSY`, and fan mode must show read-back unchanged with daemon restore, rather than pretending these controls are promotable writes.
- GTK fixture coverage now proves the 82WM PPT controls render as numeric spin rows with current value, range, step metadata, and no duplicate attribute rows.
- GTK copy now calls out that `conservation_mode` may mirror the battery charge-type `Long_Life`/Conservation mode, and AMD GPU DPM writes may affect display/GPU stability with `auto` as the restore path.
- GTK AMD GPU DPM execution now has an explicit confirmation checkbox: `Apply force level` starts disabled until the operator confirms the display/GPU stability risk.
- GTK GPU mode execution now has an explicit confirmation checkbox: `Switch mode` starts disabled until the operator confirms they reviewed recovery guidance and reboot risk.
- GTK GPU DPM detail now labels SCLK/MCLK as read-only and explicitly says manual clock writes are not exposed; DPM force level remains the supported GPU power control.
- GTK fan-mode controls now render the raw ideapad toggle as `Auto (0)` / `Full speed (1)`, keep it separate from fan curves, and have fixture coverage for the current 82WM `fan_mode=0` state.
- `scripts/verify-82wm-live-evidence.sh` now checks the full advanced-control evidence set from `data/validation/82wm-live-evidence-requirements.tsv` across all captured bundles and fails until every required execute-mode result is present with live metadata (`sysfs_root=/`, `target_bus_mode=system` or `custom-address`), the matching `execute_only` value, plus apply/revert payloads where rollback is expected. For Curve Optimizer, it also requires the operator checklist to mention the CO control, reset, and stability evidence.
- The live verifier now also validates the dry-run plan method, polkit action, and readback requirement for each evidence row (`SetFirmwareAttribute`, `SetCpuBoost`, `SetOpenRgbKeyboardRgbSdk` / `SetKeyboardRgb`, `SetCurveOptimizerAllCore`, `SetGpuMode`, etc.) and requires seeded hardware-profile/profile-trigger previews to include `SetCpuGovernor`, `SetCpuEpp`, `SetCpuBoost`, plus their matching authorization and readback metadata.
- For `fan_mode`, the live verifier now also requires `operator-checklist.md` with the observed Auto -> Full speed -> Auto sequence and thermal/fan behavior notes, because no portable fan RPM telemetry exists on this host.
- For `gpu_mode`, the live verifier now requires `operator-checklist.md` with EnvyControl command success, reboot guidance, and recovery-path notes, because this is one-way evidence rather than an automatic apply/revert control.
- Fan curves and fan RPM sensors remain blocked by the current kernel/driver surface on this host.

## Non-negotiable shape

Every write feature follows the same path:

1. Probe support and current value.
2. Add a typed validator.
3. Add dry-run plan output.
4. Add daemon writer behind an explicit `--enable-*` flag.
5. Add polkit action and authorization.
6. Read back after write.
7. Roll back on readback mismatch when rollback is meaningful.
8. Add fixture tests, private-bus D-Bus tests, GTK smoke tests.
9. Capture live execute evidence on this 82WM.
10. Only then present it as a normal GUI control.

## Deliverable map and completion audit

This plan covers the nine requested deliverables as follows:

1. Current architecture summary: RatVantage stays split across read-only discovery in `legion-probe`, shared capability/write-plan schemas in `legion-common`, all privileged mutation in the polkit-gated `legion-control-daemon`, and GTK/tray/CLI frontends in `legion-ui` / `legion-tray`. The GUI never writes hardware paths directly.
2. Provider/backend placement: sysfs-backed CPU, battery, PPT, fan, and AMD GPU DPM writers live in the daemon write surface; Curve Optimizer is a separate RyzenAdj-backed daemon provider; future `ryzen_smu` and `amdctl` providers must stay separate from the amd-pstate and PPT groups.
3. Daemon/polkit placement: every new write has a dry-run D-Bus method, an execute D-Bus method, an explicit daemon `--enable-*` flag, and a matching action in `data/polkit/org.ratvantage.LegionControl1.policy`.
4. GTK placement: stable driver behavior controls live with Profiles/Battery/GPU/Fans as appropriate; risky CPU tuning stays behind the Advanced CPU Tuning switch in Profiles and defaults hidden.
5. Data model/state changes: write plans/results remain typed in `legion-common`; hardware profiles persist requested CPU driver behavior and PPT/GPU controls; Curve Optimizer persists last requested signed offset, encoded value, backend, timestamp, stdout/stderr summary, and `write_only` readback status.
6. Safety and rollback: sysfs-style controls require validators, previous-value capture, readback, and rollback on mismatch. Curve Optimizer is reset-to-zero rather than true rollback until a readback-capable backend exists. GPU mode is one-way/reboot-pending evidence and requires operator recovery notes.
7. Tests: required coverage includes unit validators, fixture probe coverage, private-bus D-Bus contract tests, fake backend command tests, GTK smoke tests, validation-bundle review tests, and live-evidence verifier regressions.
8. Implementation checklist: the phase sections below are the ordered checklist, with PPT first, then conservation/CPU boost, fan mode, AMD DPM, EnvyControl, GPU clocks, automations/settings, and Advanced CPU Tuning.
9. Risks and deferrals: risky or unsupported controls remain explicitly deferred, including fan curves without `pwm*_auto_point*`, fan RPM telemetry without `fan*_input`, raw AMD GPU clock writes, per-core CO, iGPU CO, boot-time undervolt application, and P-state VID undervolt promotion.

The current repository-side work is not enough to declare full support. The remaining completion gate is live 82WM execute evidence under `target/validation`, reviewed by:

```bash
scripts/verify-82wm-live-evidence.sh --root target/validation
```

The default evidence matrix is `data/validation/82wm-live-evidence-requirements.tsv`. It currently requires these bundles before the plan is complete:

- `firmware_attribute:ppt_pl1_spl`
- `firmware_attribute:ppt_pl2_sppt`
- `firmware_attribute:ppt_pl3_fppt`
- `conservation_mode`
- `cpu_governor`
- `cpu_epp`
- `cpu_boost`
- `fan_mode`
- `amd_gpu_dpm_force_level`
- `keyboard_rgb`
- `curve_optimizer_all_core`
- `gpu_mode`
- `hardware_profile`
- `hardware_profile_trigger`

Each live bundle should be captured one control at a time with the system daemon already running with the matching enable flag:

```bash
scripts/capture-write-validation-report.sh \
  --output target/validation/82wm-live-<output_slug> \
  --execute \
  --execute-only <control_id> \
  --system-bus
```

Review each bundle with an explicit gate before accepting it:

```bash
scripts/review-write-validation-bundle.sh \
  --require-mode execute \
  --require-control <control_id>=<expected_status> \
  target/validation/82wm-live-<output_slug>
```

## Phase 1: Firmware PPT power limits

Scope:

- `ppt_pl1_spl`
- `ppt_pl2_sppt`
- `ppt_pl3_fppt`

Implementation:

- Add a firmware scalar write contract in `legion-common`.
- Allowlist only these three IDs for 82WM initially.
- Validate integer, min, max, and scalar increment from firmware metadata.
- Add daemon methods:
  - `PlanFirmwareAttributeWrite(attribute_id, value)`
  - `SetFirmwareAttribute(attribute_id, value)`
- Add explicit flag: `--enable-firmware-attribute-write`.
- Add polkit action for firmware attribute writes.
- Add GTK Profiles controls as numeric sliders/spin rows in the existing firmware power limit group.
- Add CLI flags for planning and execution.
- Add live validation for one PPT attribute at a time.

Done when:

- Out-of-range writes are rejected.
- Mismatched readback restores the previous value.
- GTK shows current, range, step, pending/apply/error state.
- Execute evidence exists for all three attributes on this device.

## Phase 2: Battery conservation mode and CPU boost

Scope:

- `/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode`
- `/sys/devices/system/cpu/cpufreq/boost`

Implementation:

- Add boolean validators using `0`/`1` only for boost and conservation mode.
- Add choice validators for CPU governor and EPP using the runtime `available_governors` / `available_epp` lists.
- Add dry-run and write methods, or reuse a generic sysfs control only if it remains allowlisted per control.
- Add separate flags:
  - `--enable-conservation-mode-write`
  - `--enable-cpu-governor-write`
  - `--enable-cpu-epp-write`
  - `--enable-cpu-boost-write`
- Put conservation mode in Battery, with copy explaining relation to `Long_Life`.
- Put CPU governor, EPP, and boost in Profiles under CPU Frequency Scaling.
- Add rollback/readback tests.

Done when:

- Conservation mode and battery charge type do not fight each other silently.
- CPU governor and EPP changes are visible immediately after refresh and revert to their captured original values.
- CPU boost changes are visible immediately after refresh.

## Phase 3: Fan mode promotion

Scope:

- `fan_mode`, currently `0`, with apparent `Auto (0)` / `Full speed (1)` UX.

Implementation:

- Confirm accepted values on live hardware.
- Fixture coverage exists for the `fan_mode=0` 82WM state and the GTK `Auto (0)` / `Full speed (1)` controls.
- Add live execute bundle.
- Move docs/handoff from "needs validation" to supported.
- Keep this separate from fan curves.

Done when:

- `0 -> 1 -> 0` execute evidence is captured.
- Thermal/fan behavior is documented.
- GUI shows this as a supported control, not an experimental one.

## Phase 4: AMD GPU DPM force controls

Scope:

- `power_dpm_force_performance_level`
- Possibly `power_dpm_state` only if still meaningful on this kernel/driver.

Implementation:

- Start with conservative allowlist: likely `auto` and `low` only.
- Do not expose manual clock states in this phase.
- Add daemon plan/write methods and GPU page control.
- Require readback and clear "may affect display/GPU stability" confirmation. GTK now enforces this with a checkbox before the apply button is enabled.

Done when:

- Writes survive refresh.
- Invalid driver values are rejected before sysfs write.
- GUI can restore `auto`.

## Phase 5: EnvyControl GPU mode execution

Scope:

- `integrated`
- `hybrid`
- `nvidia`

Implementation:

- Keep pending-reboot state.
- Add actual daemon execution by invoking EnvyControl from the root daemon, not from GTK/tray.
- Require explicit `--enable-gpu-mode-write`.
- Add preflight checks for installed EnvyControl, current mode, requested mode, and reboot requirement.
- GUI flow: plan -> confirm -> execute -> record pending reboot.

Done when:

- Private-bus tests cover command construction with a fake EnvyControl binary.
- Live evidence confirms mode switch and recovery instructions.

## Phase 6: AMD GPU clocks

Scope:

- `pp_dpm_sclk`
- `pp_dpm_mclk`

Decision:

Do not target full manual clock writes until DPM force-level controls are stable. Manual GPU clocks can hang the display or destabilize the system, and this device already has safer performance controls through platform profile, PPT limits, CPU EPP, and DPM force level.

If implemented later:

- Hide behind an "expert" flag and GUI disclosure.
- Require last-known-good capture.
- Require restore-to-auto flow.
- Never apply on startup automatically.

## Phase 7: Automations and settings persistence

Implementation:

- Add a daemon-owned profile store, not GTK-only state.
- Persist per-profile desired values for supported controls only.
- Add an apply engine with dry-run preview.
- Triggers:
  - AC plugged/unplugged
  - platform profile changed
  - resume from sleep
  - app launch/manual apply
- Actions:
  - platform profile
  - CPU governor/EPP/boost
  - PPT limits
  - conservation mode
  - fan mode only after Phase 3
  - AMD DPM only after Phase 4

Done when:

- Settings survive daemon restart.
- Failed automation actions are visible in diagnostics/tray.
- No unsupported control is silently applied.

Current status:

- Daemon-owned hardware profiles are persisted in `DaemonState`.
- Saved profile actions are restricted to supported typed fields and unknown JSON fields are rejected.
- Dry-run apply preview returns the same validator-backed write plans used by direct controls.
- Manual `ApplyHardwareProfile` executes saved actions in preview order, stops on the first non-applied action, and records the last run in daemon state for diagnostics.
- CLI and GTK controls can list, preview, apply, and inspect saved profile apply results.
- Tray status/menu surfaces the last hardware profile apply result, including the first stopped action and daemon message.
- Hardware profile trigger mappings are persisted for `ac_connected`, `ac_disconnected`, `resume`, `platform_profile_changed`, and `manual`; trigger preview resolves the mapped profile to validator-backed plans, and `ApplyHardwareProfileTrigger` runs through the same gated apply path.
- GTK Automations can assign saved profiles to supported triggers, clear mappings, and run mapped triggers on demand.
- The full 82WM evidence verifier now requires hardware-profile and trigger evidence to include per-action results for `cpu_governor`, `cpu_epp`, and `cpu_boost`, proving the profile path covers the complete CPU driver-behavior group rather than only one CPU toggle; regression coverage mutates both manual-profile and trigger-profile bundles to keep that gate enforced.
- Automatic OS event observers for those triggers are still deferred until the live write-evidence gates above are satisfied.

## Phase 8: Advanced CPU tuning

Scope:

- RyzenAdj all-core Curve Optimizer through `--set-coall=<u32>`.
- Future `ryzen_smu` provider for read-back/write validation.
- Future `amdctl` P-state VID undervolt provider, kept separate from Curve Optimizer.

Implementation:

- Keep amd-pstate governor/EPP/boost, Curve Optimizer, P-state VID, and PPT power limits as separate groups.
- Keep Curve Optimizer controls hidden behind an explicit Advanced CPU Tuning switch in GTK; the switch defaults off and only reveals controls for the current UI session.
- Require explicit daemon policy: `--enable-curve-optimizer-write`.
- Add polkit action: `org.ratvantage.LegionControl1.set-curve-optimizer`.
- Validate signed all-core offset as integer `-30..=0` initially.
- Encode negative offsets as `4294967296 + offset`; `0` stays `0`.
- Execute `/usr/local/bin/ryzenadj --set-coall=<encoded>` only from the daemon.
- Capture stdout/stderr and require `Successfully set coall`.
- Persist requested offset, encoded value, backend, timestamp, and `write_only` readback status.
- Provide reset by applying offset `0`, including a dedicated GTK reset-to-zero button that uses the same daemon/polkit path.
- Prefer `/sys/kernel/ryzen_smu_drv` for future read-back/write validation when available.

Done when:

- Fake-backend private-bus tests cover command construction, required success-marker parsing, missing-marker failure, policy block, and state persistence.
- GTK Advanced CPU Tuning shows write-only warning, instability warning, all-core offset control, and reset path.
- Live evidence confirms apply/reset behavior and documents recovery/stability checks.
- `scripts/capture-write-validation-report.sh --execute --execute-only curve_optimizer_all_core` captures the apply result, daemon `GetLastCurveOptimizerAllCore` write-only state after apply, reset-to-zero result, daemon last-write state after reset, and operator stability checklist; the full live verifier now requires those state artifacts.

## Not fully implementable from current live surface

These cannot become full read/write GUI features on this device unless the kernel/driver exposes them or RatVantage gains another trusted backend:

- Fan curves: no live `pwm*_auto_point*` files were present.
- Fan RPM sensors: no live `fan*_input` files were present.

Keep the existing read-only/planning UI for these, and show "not exposed by current driver" instead of implying a missing RatVantage feature.

## Recommended order

1. Firmware PPT limits.
2. Conservation mode and CPU boost.
3. Fan mode validation.
4. AMD GPU DPM force level.
5. EnvyControl execution.
6. Curve Optimizer apply/reset evidence.
7. Hardware profile apply evidence.
8. Automations/settings persistence.
9. Manual AMD GPU clocks only if still needed.

This order maximizes useful tuning first while keeping the highest-risk graphics and thermal paths behind evidence gates.
