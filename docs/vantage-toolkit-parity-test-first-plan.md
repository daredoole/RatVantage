# Vantage / Toolkit Parity Test-First Plan

This plan targets high-value Lenovo Vantage / Lenovo Legion Toolkit parity gaps that RatVantage does not fully cover yet. Fn+Q platform-profile behavior is not a target here: RatVantage already reads/writes platform profiles through the daemon, refreshes after profile changes, and treats platform profile as the authoritative Linux equivalent.

## Ground Rules

- Tests and evidence come before execution features.
- No raw WMI calls, raw EC writes, arbitrary sysfs writers, hardcoded `hwmonN`, firmware flashing, or branding/logo reuse.
- Every write path needs: capability probe, validator, dry-run plan, polkit gate, explicit daemon flag, rollback or reset story, fixture/private-bus tests, and live evidence bundle.
- Unsafe or model-private behavior stays read-only or plan-only until kernel/sysfs/HID evidence proves the path.
- UI exposes unsupported controls as absent or clearly disabled with the reason; it must not imply a capability exists because Windows software has it.

## Test Lanes

1. Fixture probe tests: synthetic sysfs/HID reports prove detection and absence behavior.
2. Pure validator tests: reject unknown IDs, out-of-range values, invalid color/effect names, and conflicting profile actions.
3. Dry-run contract tests: plans include method, capability ID, polkit action, requested value, rollback/reset instructions, reboot requirement, and safety notes.
4. Private-bus daemon tests: fake backends simulate success, policy block, auth block, read-back mismatch, rollback, missing device, and transport failure.
5. CLI/GTK/tray tests: controls render only when probed, disabled states explain why, and successful writes refresh runtime state.
6. Live evidence tests: one feature family at a time, captured through the validation harness before promotion.

## Track A: Keyboard RGB

Goal: true keyboard RGB zones/colors/effects, not just `platform::kbd_backlight` brightness.

Test first:
- Add a `KeyboardRgbCapability` fixture format with zones, supported colors/effects, current mode, and backend identity. [implemented as fixture metadata]
- Add parser/validator tests for RGB hex colors, named effects, speed/brightness ranges, zone IDs, and unsupported devices.
- Add dry-run tests for `PlanKeyboardRgbWrite`. [implemented]
- Add fake backend private-bus tests for apply, read-back mismatch, rollback failure, and reset-to-previous-mode. [implemented]
- Add HID candidate fixture tests for ITE `048D:C985` / `048D:C103` devices and non-candidate absence behavior. [implemented]
- Add read-only HID report-descriptor metadata tests for descriptor length, Report ID extraction, and report kind/size summaries. [implemented]
- Add repeatable RGB evidence bundle tests that capture candidate metadata and descriptor hashes without opening `/dev/hidraw`. [implemented in `scripts/capture-keyboard-rgb-evidence.sh`]
- Add protocol research classification tests that fingerprint ITE candidates without claiming write support. [implemented in `scripts/test-capture-keyboard-rgb-evidence.sh`]
- Add bundle comparison tests that cluster protocol signatures and keep backend-readiness blockers explicit. [implemented in `scripts/test-compare-keyboard-rgb-evidence.sh`]
- Add tray/menu tests that surface RGB readiness without exposing unsafe write actions. [implemented: tray menu shows HID research candidates and an RGB evidence dashboard route while no backend is ready; when OpenRGB SDK/native backend readiness is proven, tray menu exposes guarded `set_keyboard_rgb` presets for Static dim, Breathing dim, Rainbow wave, and Spectrum cycle through the daemon `SetKeyboardRgb` path]
- Add operator-observed hotkey/effect notes to evidence bundles. [implemented: captures references like `Fn+Space` cycling RGB modes and visible `breathing` effect without treating them as daemon read-back]
- Add overview/status tests that surface RGB readiness from the normal CLI/dashboard data path. [implemented: `keyboard_rgb_status` reports backend readiness or research-only candidates]
- Add OpenRGB readiness evidence before attempting a RatVantage backend. [implemented: `keyboard_rgb_openrgb` in the probe report plus `scripts/check-keyboard-rgb-openrgb.sh` record OpenRGB device detection, user/group membership, device-node owner/group/access details, setup recommendation, and SDK helper/server/snapshot readiness; `backend_ready=true` only when RatVantage has SDK snapshot support plus read-back/rollback coverage]
- Add OpenRGB bridge command dry-run tests for mode/color/brightness validation before any execution path. [implemented: `PlanOpenRgbKeyboardRgbBridge` validates the detected OpenRGB device/modes/LED order and returns a non-executing command preview with read-back/reset blockers]
- Add explicit setup guidance for missing OpenRGB Linux device access. [implemented: `scripts/setup-keyboard-rgb-openrgb-access.sh` installs/runs as `/usr/local/sbin/ratvantage-setup-keyboard-rgb-openrgb-access` for idempotent one-time `i2c` group, `i2c-dev`, and udev setup; GTK shows the setup row whenever `i2c-dev`, `i2c` rw, or `hidraw` rw checks are incomplete and names the missing piece; `PlanOpenRgbAccessSetup` / gated `SetupOpenRgbAccess` let GTK and CLI (`--plan-openrgb-access-setup`, `--setup-openrgb-access`) ask the root daemon through polkit instead of requiring a pasted terminal sudo command; user-session installs also copy a local setup/check wrapper so the GUI command uses the passwordless root helper when available and falls back to interactive sudo]
- Add OpenRGB bridge evidence bundle tests for save/apply/mode-read-back/restore behavior. [implemented: `scripts/capture-keyboard-rgb-openrgb-bridge-evidence.sh` is dry-run by default, supports explicit `--execute`, saves before/after profiles, records mode read-back and restore status, parses OpenRGB profile strings, scans saved profiles for requested RGB/BGR byte triplets, cleans stale profile artifacts before every capture, and only allows `backend_ready_evidence=true` when mode, restore, profile save, and color-byte evidence all pass; user install exposes this as `ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence`; live CLI execute evidence on this machine remains negative because OpenRGB exits zero but read-back stays `Direct` and saved profile bytes do not prove requested colors]
- Add an OpenRGB bridge promotion reviewer. [implemented: `scripts/review-keyboard-rgb-openrgb-bridge-evidence.sh --require-promotable <bundle>` rejects dry-run or incomplete execute bundles until every backend-readiness gate is proven; user install exposes this as `ratvantage-review-keyboard-rgb-openrgb-bridge-evidence`]
- Add a stable OpenRGB bridge evidence status command. [implemented: `scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh` summarizes OpenRGB readiness preflight, dry-run/execute bundle state, and next action; user install exposes this as `ratvantage-keyboard-rgb-openrgb-bridge-status`]

Implementation slices:
- Research backend: Linux HID/sysfs first; only consider Windows Toolkit protocol as reference material.
- Probe backend read-only. [candidate-only HID evidence implemented; no support claim]
- Capture report-descriptor byte count, Report IDs, and input/output/feature byte lengths for candidate HID devices. [implemented]
- Capture descriptor hashes/hex into operator evidence bundles for cross-machine protocol comparison. [implemented; read-only sysfs metadata only]
- Record operator-observed firmware hotkey/effect state alongside sysfs evidence. [implemented; `Fn+Space`/`breathing` can be recorded as observation-only evidence]
- Capture OpenRGB as a reference backend when installed. [implemented in core probe/overview/dashboard/tray readiness surfaces; live dogfood detects `Lenovo 5 2023` / `Lenovo 4-Zone device` with `Direct`, `Breathing`, `Rainbow Wave`, and `Spectrum Cycle` modes]
- Expose OpenRGB bridge dry-run planning through daemon/client/CLI while keeping writes disabled. [implemented as `--plan-openrgb-keyboard-rgb <json>`; no Set method, no policy flag, no execution]
- Capture OpenRGB bridge mode/profile restore evidence before any daemon execution path. [implemented as evidence harnesses; OpenRGB CLI apply is negative even with lowercase mode, autoconnect, device index selection, and Direct-color requests, but operator-triggered SDK write evidence using `RGBCONTROLLER_UPDATEMODE` plus `RGBCONTROLLER_UPDATELEDS` now proves mode/color write and restore through SDK read-back; backend promotion now has the real SDK helper path, dev daemon args, RPM helper packaging, and live system-daemon `SetKeyboardRgb` dogfood through the OpenRGB SDK fallback]
- Review OpenRGB bridge execute evidence with the promotion gate before adding daemon execution. [implemented reviewer; dry-run bundles are accepted for inspection but fail `--require-promotable`]
- Classify captured ITE candidates into protocol research families and emit stable protocol signatures/matrix rows for cross-machine comparison. [implemented; still read-only and `backend_ready=false`]
- Compare one or more evidence bundles into protocol-signature clusters with promotion blockers. [implemented in `scripts/compare-keyboard-rgb-evidence.sh`]
- Surface candidate/readiness state in user-visible overview, dashboard, and tray paths without enabling RGB writes until backend evidence is ready. [implemented; tray write presets appear only after OpenRGB SDK/native backend readiness is proven]
- Surface copyable OpenRGB bridge evidence commands in the dashboard. [implemented in GTK Appearance; status and dry-run commands are non-mutating, execute command remains operator-triggered]
- Add a staged GTK RGB editor before execution promotion. [implemented in GTK Appearance: effect picker uses detected OpenRGB/native modes, per-zone `#RRGGBB` entries use detected OpenRGB LED labels, brightness/speed sliders build `KeyboardRgbWriteRequest`, Copy request JSON is local, Preview plan calls the read-only daemon planning method and uses the SDK plan when `backend_ready=true`, Check status captures read-only OpenRGB readiness then runs the readiness/SDK-aware bridge evidence status helper inline, Check SDK captures read-only OpenRGB SDK controller evidence, Start server runs/copies the user-session OpenRGB SDK server helper, Capture dry-run runs the non-mutating OpenRGB evidence helper, Review execute runs the read-only promotion reviewer, and Apply RGB remains disabled while `backend_ready=false`]
- Add daemon/CLI dry-run only. [implemented]
- Add gated fake execution backend. [implemented for tests only]
- Add guarded live execution only after live read-back works. [implemented for OpenRGB SDK-backed daemon `SetKeyboardRgb` and tray RGB presets after live mode/color read-back evidence]
- Add OpenRGB SDK read-back if profile color-byte evidence is insufficient across models. [implemented as read/write evidence gates: `scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh` starts/connects to the OpenRGB SDK server, negotiates protocol with the client version, waits for controller enumeration, skips async device-list packets, parses controller active mode/LED/color data into JSON/Markdown, has immediate/delayed fake SDK server regression coverage, is installed as `ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence`, is captured by compatibility bundles, GTK Appearance exposes a `Check SDK` read-only evidence row, and bridge execute/reviewer/status consume SDK before/after/restored snapshots; `scripts/capture-keyboard-rgb-openrgb-sdk-write-evidence.sh --execute` is operator-triggered, sends `RGBCONTROLLER_UPDATEMODE` plus `RGBCONTROLLER_UPDATELEDS`, proves requested mode/colors by SDK read-back, restores before mode/colors, has fake SDK server coverage, is installed as `ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence`, and is exposed in GTK Appearance as a copyable operator command. Current live SDK read-back and SDK mode/color write evidence are positive, while OpenRGB CLI apply remains negative. Daemon planning now exposes `PlanOpenRgbKeyboardRgbSdkWrite`, OpenRGB-only hardware-profile RGB preview falls back to `SetOpenRgbKeyboardRgbSdk`, direct `SetKeyboardRgb` falls back to that SDK writer when native keyboard RGB is absent, daemon tests cover SDK fallback apply/read-back mismatch rollback/rollback-failure plus a fake external-helper process boundary, and the daemon has opt-in `--enable-keyboard-rgb-write` / `--openrgb-sdk-helper <path>` flags. `scripts/openrgb-keyboard-rgb-sdk-helper.sh` is implemented and installed as `ratvantage-openrgb-keyboard-rgb-sdk-helper`; it connects to an already-running SDK server and supports `snapshot`, `write`, and `restore`. Dev/user-session installs now also expose `ratvantage-capture-keyboard-rgb-evidence` and `ratvantage-compare-keyboard-rgb-evidence`; RPM packaging has a `legion-control-helpers` subpackage for the OpenRGB/readiness/evidence/compatibility scripts with no setuid helper. Dev daemon args now enable keyboard RGB writes with the installed helper path, the user-session SDK server strategy works live, probe/common/daemon reports expose SDK snapshot readiness, live probe reports `backend_ready=true`, and live system-daemon `--set-keyboard-rgb` applied Breathing / `#333333` through `SetOpenRgbKeyboardRgbSdk`.]
- Tray presets and Apply RGB execution after execution evidence. [implemented: GTK Apply RGB and tray presets use daemon `SetKeyboardRgb`; tray offers Static dim, Breathing dim, Rainbow wave, and Spectrum cycle when backend readiness is proven]

Stop conditions:
- Backend requires raw EC/WMI writes.
- No read-back/reset path exists.
- Protocol varies by model without detectable identity.

## Track B: Custom Thermal / God Mode Unlock

Goal: make Lenovo custom mode usable for fan curves and firmware attributes when the kernel exposes the needed surfaces.

Test first:
- Add validator tests for `custom` platform profile: only allowed when listed in `platform_profile_choices`.
- Add dry-run tests that PPT/fan curve writes require custom mode or explain why not. [implemented: common + daemon fixture tests annotate required/satisfied/unavailable custom thermal prerequisite states]
- Add private-bus tests for a multi-step plan: switch to custom, apply setting, read back, rollback previous profile/value. [implemented as read-only sequence previews for firmware PPT, fan preset, and restore-auto-fan; execution remains future work]
- Add negative tests for firmware `EBUSY` and fan mode read-back mismatch.

Implementation slices:
- Model custom-mode dependency explicitly in write plans. [implemented for firmware PPT, fan preset, and restore-auto-fan dry-run plans]
- Add a `PrepareCustomThermalMode` plan-only method if needed. [implemented as read-only D-Bus/client/CLI planning; normal `SetPlatformProfile custom` remains blocked]
- Add custom-thermal sequence previews for dependent PPT/fan plans. [implemented as read-only D-Bus/client/CLI planning with reverse rollback order]
- Re-test PPT writes after explicit custom-mode activation.
- Promote only controls that pass live apply/revert evidence.

Stop conditions:
- Firmware attributes stay `EBUSY`.
- Custom mode is not listed.
- Fan curve nodes are absent or write-only.

## Track C: CPU/GPU Power Limits And Dynamic Boost

Goal: expose useful Lenovo power-limit tuning while keeping risk bounded.

Test first:
- Extend firmware attribute fixtures with `default_value`, `min_value`, `max_value`, `scalar_increment`, profile page, and busy state. [partially implemented: default/type/min/max/increment metadata is fixture-tested; busy state remains future work]
- Validate SPL/SPPT/FPPT and any GPU limit/dynamic boost values against firmware metadata.
- Add hardware-profile conflict tests so CPU/GPU limits do not combine with unsupported fan mode or missing custom mode.
- Add reset-to-default dry-run and execution tests. [dry-run reset planning implemented when firmware exposes `default_value`; execution remains future work]

Implementation slices:
- Keep dashboard-only, never tray.
- Add presets first: Conservative, Balanced custom, Performance custom, Reset defaults. [implemented as read-only custom-thermal sequence previews for PL1/PL2/PL3; execution remains future work]
- Add reset-to-default plans before presets. [implemented for individual PPT firmware attributes with default metadata]
- Add manual numeric controls only after preset evidence.
- Record last applied values and reset state in daemon diagnostics.

Stop conditions:
- No firmware metadata.
- No default/reset value.
- No stable read-back.

## Track D: Hotkey And State Integration

Goal: support Vantage/Toolkit-like behavior around hotkeys and state drift without duplicating Fn+Q itself.

Test first:
- Add tests for detecting external profile changes and producing notifications.
- Add automation tests for profile-changed triggers. [implemented for daemon-owned observer baseline/change/apply behavior]
- Add state tests for RGB/profile/fan/power drift after resume. [partially implemented: resume observer tests prove a mapped `resume` hardware-profile trigger reapplies a profile through existing daemon gates; GPU reboot-completion observer tests prove pending GPU mode is cleared and a mapped `gpu_mode_reboot_completed` trigger applies through existing daemon gates after the probed mode matches; diagnostics now compare the last completed hardware-profile apply against current read-only probe state for comparable actions including OpenRGB SDK mode/color drift; diagnostics also compare live fan curve readings against the saved last-known-good fan snapshot, runtime refresh notices report fan curve drift, and tray notifications surface new hardware-profile plus fan-curve drift]

Implementation slices:
- Add profile-changed automation trigger beyond notification. [implemented: opt-in automation observer detects external `platform_profile` changes, records bounded recent change history, applies a mapped `platform_profile_changed` hardware-profile trigger through existing daemon gates, and GTK Automations includes an `Fn+Q tuning repair starter` that seeds a CPU-tuning repair profile plus the trigger mapping]
- Add per-profile appearance/power preset reapply after resume. [partially implemented: hardware profiles can store and preview validated keyboard RGB requests through either the promoted native backend or the dry-run OpenRGB bridge, GTK summaries show compact `rgb=<effect> <color>` intent, a mapped `resume` trigger now reapplies the selected hardware profile after logind resume through existing daemon policy/read-back gates, and GTK Automations includes a `Resume balanced profile starter` that seeds the resume trigger mapping; RGB execution still remains blocked until promotion evidence exists]
- Add user-visible history for external changes. [implemented: daemon state stores recent external platform-profile changes, D-Bus exposes `GetRecentPlatformProfileChanges`, `legion-control-ui --recent-platform-profile-changes` prints the list directly, diagnostics JSON/support bundles include the list, and GTK Automations shows recent Fn+Q/firmware-side profile changes]
- Add optional hotkey listener only if a safe desktop-level shortcut path exists.

Stop conditions:
- Requires intercepting firmware hotkeys below the desktop/session layer.

## Track E: GPU Switching / Advanced Optimus

Goal: improve GPU switching beyond reboot-required EnvyControl where Linux exposes a safe path.

Test first:
- Add capability fixtures for switch type: reboot-required, session-restart-required, runtime mux, unknown. [implemented in the shared GPU capability schema with `GpuSwitchType`; EnvyControl probes classify as `reboot_required`, older JSON defaults to `unknown`, and overview/tray status/tray tooltip/tray menu/GPU page/plan output surface the classification]
- Add dry-run tests that reboot/session requirements are explicit. [implemented: `reboot_required`/legacy-unknown EnvyControl remains the only planned execution path; structured `gpu_switching` diagnostics now report provider, current mode, switch type, execution model, blockers, evidence, and next action; `session_restart_required` and `runtime_mux` classifications are validation-blocked until a dedicated backend and live display recovery evidence exist]
- Add fake-backend tests for pending state, rollback instructions, and post-reboot validation.

Implementation slices:
- Keep current EnvyControl path as baseline.
- Detect runtime-switch-capable paths read-only. [partially implemented: diagnostics and GTK now expose runtime/session classifications and blockers, but no kernel/driver runtime path has been promoted]
- Add plan-only runtime switch if a kernel/driver path is found.
- Promote only with live display recovery evidence.

Stop conditions:
- Runtime switch can blank the display without automatic recovery.
- Backend cannot report current mux/GPU state reliably.

## Track F: RatVantage-Only High-Value Features

These are features Lenovo tools do not emphasize and are safer to differentiate on Linux:

- Evidence-first compatibility bundles for user-submitted hardware reports. [implemented: `scripts/capture-compatibility-bundle.sh` captures read-only overview, diagnostics, automation diagnostics, reset diagnostics, probe JSON, OpenRGB readiness, OpenRGB bridge evidence status, and OpenRGB SDK read-back evidence into a single support directory; bundle metadata includes compact `high_value_recovery` reset summaries, `high_value_drift` hardware-profile/fan-curve drift summaries, `high_value_gpu_switching` reboot/session/runtime classification summaries, and `high_value_automation` profile/rule/run/change summaries; bundles also generate `compatibility-bundle-pr-body.md` for hardware report submissions; user-session install exposes `ratvantage-capture-compatibility-bundle`; GTK Diagnostics surfaces a copyable bundle command]
- Hardware-profile automation with AC/battery/profile/resume/GPU-reboot-completion triggers. [implemented for AC/battery routing as daemon-owned `ac_profile_router` rules, battery capacity threshold routing as daemon-owned `battery_profile_threshold` rules, fast-charge threshold routing as daemon-owned `fast_charge_until_threshold` rules, mapped `resume`, `platform_profile_changed`, and `gpu_mode_reboot_completed` hardware-profile triggers through the opt-in observer, GTK custom authoring for AC-router, battery-threshold, and fast-charge rules, and GTK starters for AC routing, AC CPU performance/efficiency routing, battery quiet mode, battery threshold recovery, integrated GPU on battery, resume repair, GPU reboot-completion repair, and Fn+Q/platform-profile-change tuning repair; broader mixed preset/profile composition remains]
- Advanced CPU backend setup/read-back visibility. [implemented for RyzenAdj and `ryzen_smu`: daemon detection reports command availability, module/sysfs/read-back state, CLI exposes `--ryzen-backend-status` and `--ryzen-smu-setup`, Profiles shows backend status, and Diagnostics exposes copyable read-only commands; RatVantage still does not install or load kernel modules automatically]
- Per-profile safe presets that combine platform profile, battery mode, CPU EPP/governor/boost, GPU DPM, and RGB. [implemented for GTK Balanced Daily Mixed, Quiet Battery Mixed, and Performance AC Mixed starters that save daemon-owned profiles combining platform profile, battery charge type, CPU governor/EPP/boost, AMD GPU DPM, staged RGB, fan-preset mappings, and an AC-connected trigger for the performance preset; RGB execution remains blocked until the RGB backend is promoted]
- Post-write drift detection and notification. [partially implemented: diagnostics and `--automation-diagnostics` derive a read-only `hardware_profile_drift` report from the last completed hardware-profile apply and current probe state, compare OpenRGB SDK keyboard mode/colors when SDK snapshot data exists, include recent external platform-profile changes, derive read-only `fan_curve_drift` by comparing live fan curve readings against the saved last-known-good snapshot, GTK Diagnostics shows both drift summaries, runtime refresh notices report live fan curve drift, and StatusNotifier sends de-duplicated desktop notifications when new profile or fan-curve drift appears]
- One-click reset plans for every risky tuning family. [partially implemented: `legion-control-ui --reset-diagnostics` emits a read-only reset snapshot for Curve Optimizer reset-to-zero, firmware PPT `reset-defaults`, OpenRGB SDK keyboard RGB current-mode/color recovery planning, restore-auto-fan, custom-thermal restore-auto-fan planning, GPU-mode pending recovery guidance with the clear command, and GPU switching recovery guidance keyed to reboot/session/runtime/unknown switch classifications; GTK Diagnostics surfaces a copyable reset diagnostics command; execution remains on the existing gated per-family paths]

Test first:
- Add read-only compatibility bundle tests that fake UI/probe/OpenRGB commands and verify generated metadata, logs, and safety markers. [implemented in `scripts/test-capture-compatibility-bundle.sh`]
- Add profile preview tests for every new action before execution exists.
- Add automation dry-run tests before observer execution.
- Add diagnostics JSON parity tests for every persisted state field.

## Recommended Order

1. Keyboard RGB read-only capability and validators.
2. Keyboard RGB dry-run/fake backend.
3. Custom thermal prerequisite modeling for PPT/fan writes.
4. PPT retry/custom-mode evidence slice.
5. RatVantage-only profile presets combining already-safe controls.
6. Dynamic boost/power-limit presets if firmware metadata and reset paths are reliable.
7. Runtime GPU switching research only after the above stabilizes.

## References

- Linux kernel `lenovo-wmi-gamezone`: https://www.kernel.org/doc/html/latest/wmi/devices/lenovo-wmi-gamezone.html
- Linux kernel `lenovo-wmi-other`: https://www.kernel.org/doc/html/latest/wmi/devices/lenovo-wmi-other.html
- Lenovo Legion Toolkit README: https://github.com/BartoszCichecki/LenovoLegionToolkit
- Lenovo Vantage for Gaming: https://www.lenovo.com/us/en/software/vantageforgaming
- Lenovo Legion Space support page: https://support.lenovo.com/us/en/solutions/ht517560
