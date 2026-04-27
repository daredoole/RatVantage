# Session handoff — archive

Frozen **long-form** material: completed-slice log, full implemented inventory, extended commands, execute-mode notes, CI policy, and working-process detail.

For **current** next tasks and the short session prompt, read [session-handoff.md](session-handoff.md). Canonical repo rules: [AGENTS.md](../AGENTS.md).

---

## Current task

- Completed slice: the GTK shell now has a dedicated GPU tab that shows the current EnvyControl mode, previews dry-run GPU switch plans with rollback guidance, and records or clears the pending-reboot app state without enabling direct GPU-mode execution.
- Completed slice: the GPU tab is wired into `--gtk-page gpu`, headless GTK coverage, the screenshot-smoke workflow, and the progress docs so the implemented shell matches the documented page layout.
- Completed slice: the GTK Fans tab now exposes packaged preset selection with daemon-backed dry-run previews for `ApplyFanPreset` and `RestoreAutoFan`, plus a capture control for the durable last-known-good fan curve snapshot (still planning-only; fan preset execution remains disabled in the dashboard).
- Completed slice: read-only live fan curve sysfs readings via `GetLiveFanCurveReadings`, CLI `--fan-curve-live`, and GTK Fans "Refresh live readings" (no app-state mutation).
- Completed slice: default GTK smoke capture includes the `gpu` page alongside `fans`; `scripts/run-local-session-app.sh` documents `--gtk-page gpu`.
- Completed slice: GTK Fans manual scratchpad TOML exchange (`ratvantage_fan_scratchpad_v1` encode/decode in `legion-common`, clipboard copy, multiline editor import for scratchpad or packaged preset TOML when hwmon pair count matches); still read-only (no daemon apply).
- Completed slice: read-only live vs last-known-good fan curve comparison (`format_fan_curve_live_vs_saved` in `legion-common`, GTK Fans “Compare live to saved” with monospace report); still no writes.
- Completed slice: durable per-platform-profile fan preset map in daemon state (`GetFanPresetProfileMap` / setters), diagnostics JSON parity, GTK Fans “Fan preset per platform profile” section with per-choice save and clear-all.
- Completed slice: `fan_preset_reapply_after_resume` policy in daemon state with `GetFanPresetReapplyAfterResume` / `SetFanPresetReapplyAfterResume`, GTK Fans switch, and a system-bus logind `PrepareForSleep` background observer on the production daemon that refreshes the probe after resume and prints a dry-run fan preset plan when a mapping exists (still no fan sysfs execution).
- Completed slice: `fan_curve_snapshot_chart_pairs` in `legion-common` plus GTK Fans “Curve shape (read-only preview)” PWM bars keyed off the saved last-known-good snapshot (v0.2 editor groundwork only).
- Completed slice: GTK Fans read-only Cairo drawing for temperature→PWM polyline from the same saved snapshot (still not an interactive editor).
- Completed slice: GTK Fans manual scratchpad includes an interactive temp→PWM chart (drag nearest point to edit row entries), entry `connect_changed` redraw wiring, TOML import chart refresh, and `repopulate_manual_fan_scratchpad_rows` optional chart hook — still scratchpad-only (no daemon fan sysfs writes).
- Completed slice: the same temp→PWM Cairo chart (scratchpad and read-only LKG preview) draws light horizontal grid lines, left PWM scale ticks (0 / 128 / 255), bottom min/max raw-temperature labels, a centered axis title, plus an expanded scratchpad hint describing horizontal vs vertical axes.
- Completed slice: Fans manual scratchpad chart supports click-to-select, orange highlight on the selected point, arrow-key nudging (Shift = coarse temp/PWM steps), focusable chart with tooltip, first-row parse error text when the curve cannot plot, and selection cleared whenever scratchpad rows are repopulated.
- Completed slice: scratchpad temp/pwm `Entry` rows use `EventControllerFocus::enter` so keyboard/tab focus on a row syncs the chart selection highlight with the same `ScratchpadChartSelection` used for click and arrow nudges.
- Completed slice: GTK Fans scratchpad **Preview sysfs targets**: `format_manual_fan_scratchpad_sysfs_preview` in `legion-common` builds read-only per-path text from parsed rows after the same monotonic validation as **Validate pairs**; parse failures reuse scratchpad chart error copy; no D-Bus or polkit from this control.
- Completed slice: **Preview sysfs targets** also mirrors a short outcome line into the scratchpad status label (parse block, validation failure, or success with pair count).
- Completed slice: **Copy sysfs preview** toolbar button copies the monospace preview pane to the clipboard (skips the initial placeholder); status line confirms or explains when there is nothing to copy.
- Validation for the latest slices passed with `cargo fmt --all`, `xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell`, and `./scripts/ci-local.sh`.
- Current user-visible GTK surface now includes `Status`, `Profiles`, `Battery`, `GPU`, `Fans`, `Appearance`, and `Diagnostics`.
- Direct GPU-mode execution is still disabled in the dashboard; the GTK GPU tab is planning-only and app-state-only, matching the daemon safety policy.
- Completed slice: Fans manual scratchpad action row tooltips (load live/saved, clear, validate, preview, copy preview, copy JSON/TOML) for hover and screen-reader context; `./scripts/validate-packaging.sh` re-checked clean.
- Completed slice: Fans manual scratchpad actions split into two horizontal rows (load/saved/clear/validate vs preview/copy-preview/JSON/TOML) inside one vertical strip for narrow-window readability.
- Completed slice: GTK screenshot smoke in `./scripts/ci-local.sh` and GitHub Actions now includes **`gpu`** and **`fans`** (`--pages status,battery,gpu,fans`); `docs/gtk-smoke-testing.md` notes the CI subset.
- Completed slice: tray/menu and notification copy polish (clearer status rows, tooltip telemetry labels, desktop notification titles/bodies, write-result wording); `ApplyFanPreset` / `RestoreAutoFan` remain gated.
- Completed slice: `legion-common` helpers `format_gpu_mode_pending_summary` and `format_fan_curve_snapshot_summary` shared by `--overview`, tray status lines, GTK pending-reboot rows, and runtime refresh notices (same human-readable text everywhere).
- Completed slice: `--overview` now prints `fan_preset_by_platform_profile` (sorted `profile=preset_id` pairs, or `none`) and `fan_preset_reapply_after_resume`, matching durable daemon state already exposed in `--diagnostics` JSON.
- Completed slice: write-validation harness captures **fan preset** and **restore-to-auto** dry-run plan artifacts (`--plan-fan-preset`, `--plan-restore-auto-fan`); `--execute` remains whitelisted to reversible non-fan controls only.
- Completed slice: `tests/fixtures/sysfs-82wm-confirmed` hwmon fan curve now exposes **pwm1_auto_point1..10** temp/pwm nodes so packaged **balanced-daily** dry-run planning succeeds on the default fixture (daemon dbus contract + validation harness report show `planned` for fan preset).
- Completed slice: write-validation harness also captures **GPU mode** dry-run plans (`--plan-gpu-mode`) when diagnostics show EnvyControl with a switchable current mode; execute mode still never touches GPU or fan sysfs.
- Completed slice: README documents live **EnvyControl GPU probe** verification (`envycontrol --query` + `legion-probe --json`) and clarifies fixture `sysfs-root` runs intentionally omit GPU probing.
- Completed slice: read-only **PowerProfiles** session D-Bus probe (`org.freedesktop.UPower.PowerProfiles` name owner + `ActiveProfile`) in `legion-probe` when `sysfs_root` is `/`; raw probe JSON, `power_profiles` capability row, `--overview` line `desktop_power_profiles=`, and GTK Profiles **Desktop PowerProfiles** card; fixtures keep `power_profiles` null and the capability **missing** so CI counts stay stable.
- Completed slice: write-validation harness supports **`--execute-only <control_id>`** (narrow apply+revert per PR bundle), stderr nudge when `--execute` is used without it, `execute-skipped-filter` status in JSON, and expanded **live-write-validation** docs (daemon flag table, fan execution gate, KDE-first GNOME deferral).
- Completed slice: **`scripts/archive-validation-bundle.sh`** zips a bundle next to its directory (stable paths); **`--execute`** path now **preflights** `before/diagnostics.json` so ServiceUnknown / non-JSON fails immediately; root **`/*.zip`** gitignored for local evidence archives.
- Completed slice: **`docs/live-validation-evidence-runbook.md`** (per-family live capture commands), **`scripts/install-dev-systemd-ratvantage.sh`** (dev binary under `/usr/local` + systemd + D-Bus activation; refuses if a `legion-control*` RPM is installed), and **`docs/fan-gpu-execution-policy.md`** (explicit fan/GPU execution deferral).
- Completed slice: README, **AGENTS.md**, **CLAUDE.md**, and **`.cursor/rules/ratvantage.mdc`** now point operators at the runbook + fan/GPU policy; **live-write-validation** git-checkout flow documents Option A (dev systemd) vs Option B (foreground); runbook **Notes** match the script’s **`^legion-control`** RPM guard and clarify reload/restart.
- Completed slice: **Agent doc dedupe** — `CLAUDE.md` and `.cursor/rules/ratvantage.mdc` are thin stubs linking to **`AGENTS.md`**; **`docs/session-handoff.md`** is lean for “read first,” with long completed-slice log + inventory + CLI list moved to **`docs/session-handoff-archive.md`**.
- Next recommended roadmap slice: on supported Legion hardware, follow the evidence runbook and attach narrow execute bundles; fan/GPU execution remains policy-gated until maintainers ship the corresponding daemon surface.
- If the KDE Wayland/NVIDIA black-window bug returns, treat it as a compositor/frontend issue and keep the private-session launcher plus `--gdk-backend x11` fallback available while continuing tray/CLI validation.

## Implemented

- Workspace crates: `legion-common`, `legion-probe`, `legion-daemon`, `legion-ui`, `legion-tray`, `ratvantage-test-support`.
- Probe fixture coverage for confirmed and runtime-captured 82WM-style sysfs paths.
- Current 82WM read-only validation evidence recorded in `docs/implementation-plan.md`.
- Bracketed battery `charge_types` parsing, including inferred current value when `charge_type` is absent.
- Read-only `BAT0` telemetry for capacity percent, charging status, and health string when exposed.
- Read-only EnvyControl GPU mode query when `envycontrol --query` is available; fixture-backed runs keep GPU capability missing for deterministic tests.
- EnvyControl GPU mode dry-run planning for `integrated`, `hybrid`, and `nvidia`, with write execution disabled plus reboot-required and rollback metadata in the plan.
- Fan restore/default dry-run planning through `PlanRestoreAutoFanWrite` and `--plan-restore-auto-fan`; write execution remains disabled.
- Durable app-state-only GPU pending-reboot tracking via `GetGpuModePending`, `SetGpuModePending`, `ClearGpuModePending`, and the UI `--gpu-mode-pending`, `--set-gpu-mode-pending`, `--clear-gpu-mode-pending` commands.
- Durable app-state-only last-known-good fan curve capture via `GetLastKnownGoodFanCurve`, `CaptureLastKnownGoodFanCurve`, and the UI `--last-known-good-fan-curve`, `--capture-last-known-good-fan-curve` commands.
- Read-only live fan curve sysfs readings via `GetLiveFanCurveReadings` and `legion-control-ui --fan-curve-live` (GTK Fans tab includes a refresh control).
- Tray/status output, UI `--overview`, and GTK Status/Fans pages surface the durable GPU pending and saved fan curve state.
- UI `--overview` command for platform profile, battery charge type, fan RPM, temperatures, GPU mode, durable app state (GPU pending, saved fan curve, per-profile fan preset map, resume re-apply flag), battery telemetry, LED brightness, and firmware toggle values.
- UI `--diagnostics` command for a read-only JSON debug bundle containing hardware summary, compact capability/sensor/fan/path counts, kernel version, detected sysfs paths, durable app-state fields `gpu_mode_pending` and `last_known_good_fan_curve`, recent daemon log excerpts, and raw probe report.
- UI `--set-platform-profile`, `--set-battery-charge-type`, `--set-led-state`, and `--set-ideapad-toggle` commands for gated reversible execution results over D-Bus.
- Platform profile and battery charge type models include both current-value paths and choice-list paths for diagnostics.
- UI status output includes per-capability status and risk labels.
- Optional GTK Profiles, Battery, and Appearance tabs render the diagnostics bundle data and expose gated quick-apply controls with inline write-result feedback where the write surface is currently allowed.
- Optional GTK Fans tab renders fan telemetry, fan curve paths, last-known-good snapshot status, packaged preset selection with dry-run plan previews for fan preset and restore-to-auto flows, capture for the durable last-known-good curve, read-only live sysfs curve readings with a per-point `ActionRow` list after refresh, a matching read-only saved last-known-good point list with daemon refresh, a raw multiline live dump, a read-only live-vs-saved sysfs diff report, a manual scratchpad with monotonic validation plus JSON and lossless TOML (`ratvantage_fan_scratchpad_v1`) export and editor import (including packaged preset TOML when point counts match) and an interactive temp→PWM chart tied to the same rows (drag, click-select, row entry focus syncs highlight, arrow-key nudge with Shift coarse steps, invalid-state chart messaging, Preview sysfs targets read-only buffer plus Copy sysfs preview, two-row scratchpad action strip), per-platform-profile fan preset mapping rows plus a resume re-apply policy switch, still with no `ApplyFanPreset` / `RestoreAutoFan` execution in the dashboard.
- Optional GTK Appearance tab renders LED brightness and firmware toggle values and now exposes gated quick-apply controls for ylogo LED, restricted `fn_lock`, and dashboard-confirmed `camera_power` plus `usb_charging`.
- Optional GTK diagnostics tab for the same read-only hardware/debug bundle, with compact counts and Copy JSON parity for durable app-state fields.
- Packaged read-only fan preset TOML assets in `data/presets/`, validated by `scripts/validate-packaging.sh`, installed by the RPM spec, and validated at runtime for dry-run fan preset planning.
- Read-mostly D-Bus daemon methods plus gated reversible writes:
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
  - `PlanFanPresetWrite`
  - `GetFanPresetProfileMap` / `SetFanPresetProfileMapEntry` / `RemoveFanPresetProfileMapEntry` / `ClearFanPresetProfileMap`
  - `GetFanPresetReapplyAfterResume` / `SetFanPresetReapplyAfterResume`
  - `SetPlatformProfile`
  - `SetBatteryChargeType`
  - `SetLedState`
  - `SetIdeapadToggle`
- UI `--status`, `--plan-platform-profile`, `--set-platform-profile`, `--plan-battery-charge-type`, `--set-battery-charge-type`, `--plan-led-state`, `--set-led-state`, `--plan-ideapad-toggle`, `--set-ideapad-toggle`, `--plan-gpu-mode`, and `--plan-fan-preset` commands, plus optional GTK4/libadwaita shell behind `gtk-ui`.
- Read-only `legion-control-tray --status` summary output.
- `legion-control-tray --menu-check` diagnostics for the runtime-derived tray menu, including reversible quick-action entries.
- `legion-control-tray` StatusNotifier backend with dashboard, refresh, quit, periodic/resume reloads, informational runtime menu rows, reversible quick actions for platform profile, battery charge type, ylogo LED, and restricted `fn_lock`, plus dashboard-routed guidance for `camera_power`.
- Tray menu shows detected platform profile choices, battery charge choices, LED state, ideapad toggle state, packaged fan preset labels, capability summaries, pending app state, quick actions for non-current reversible choices, and warning rows for dashboard-confirmed controls.
- StatusNotifier tray dashboard launch forwards `--bus-address` when the tray runs against a private/session bus.
- Tray tooltip reports current platform profile, fan RPM, and available/missing capability counts.
- StatusNotifier tray smoke script and manual checklist; autostart is still disabled.
- KDE Plasma Wayland StatusNotifier smoke passed with fixture daemon: registration, screenshot capture, tooltip properties, runtime menu export, refresh, and quit were verified.
- KDE Plasma Wayland StatusNotifier smoke can now emit reusable report bundles with watcher counts, raw item properties, tray status/tooltip output, `tray-menu-check.txt`, and environment metadata under `target/smoke/`.
- `legion-control-tray --desktop-check` reports desktop/session state, watcher availability, and autostart gating without requiring the tray to stay running.
- GNOME AppIndicator extension path is intentionally untested for now: GNOME Shell and the extension are installed, but the active graphical session is KDE Wayland. Keep tray autostart disabled.
- Tray startup emits GNOME AppIndicator/KStatusNotifier extension guidance when the desktop session reports GNOME.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke test for the optional shell, run through Xvfb in local and GitHub CI.
- Private-bus contract tests and shared test support.
- Fedora packaging assets for systemd, D-Bus, polkit, desktop metadata, AppStream metadata, and RPM spec.
- Packaging metadata validation script wired into local and GitHub CI.
- Read-only sysfs fixture capture workflow, validated against the existing 82WM fixture in local CI.
- Read-only compatibility bundle workflow via `scripts/capture-compat-report.sh`, validated against the existing 82WM fixture in local and GitHub CI.
- Live write-validation harness via `scripts/capture-write-validation-report.sh`, validated against the existing 82WM fixture in local and GitHub CI.
- Local private-session frontend launcher via `scripts/run-local-session-app.sh`, validated in local smoke runs for `status` and `menu-check` against the existing 82WM fixture.
- Hardware compatibility PR template in `.github/PULL_REQUEST_TEMPLATE/hardware-compatibility.md`.
- Disabled draft write-method contracts for GPU mode and fan presets, plus gated platform-profile, battery charge type, ylogo LED, and limited ideapad-toggle execution paths for `fn_lock` and `camera_power`.
- Pure validators for platform profile, battery charge type, ylogo LED state, limited ideapad toggle writes, EnvyControl GPU mode, and packaged fan preset choices; the reversible platform/battery/LED/ideapad-toggle writes remain disabled by default unless the daemon write flags are enabled.
- Validator-backed dry-run planning for platform profile, battery charge type, ylogo LED state, limited ideapad toggle writes, GPU mode, and fan presets; `SetPlatformProfile`, `SetBatteryChargeType`, `SetLedState`, and `SetIdeapadToggle` are exposed over D-Bus with policy/auth gates and rollback handling.
- Daemon-side Rust adapters for dry-run planning, tested directly and through private-bus contract tests.
- Local CI and GitHub CI.
- `docs/implementation-plan.md` intentionally has both layouts:
  - `Current scaffold` shows what exists today.
  - `Target layout` preserves the fuller planned architecture with `data/`, `packaging/`, `xtask/`, presets, desktop metadata, and tray work.

## Commands

```bash
./scripts/install-dev-deps-fedora.sh
./scripts/ci-local.sh
./scripts/validate-packaging.sh
scripts/capture-sysfs-fixture.sh --output tests/fixtures/sysfs-<model>-<note>
scripts/capture-compat-report.sh --output compat/<machine-label>
scripts/capture-write-validation-report.sh --output target/validation/<machine-label>-plan
scripts/capture-write-validation-report.sh --output target/validation/<machine-label>-live --execute --system-bus
scripts/run-local-session-app.sh --frontend status
scripts/run-local-session-app.sh --frontend menu-check
scripts/run-local-session-app.sh --frontend tray
scripts/run-local-session-app.sh --frontend ui --gsk-renderer cairo
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --enable-platform-profile-write --enable-battery-charge-type-write --enable-led-state-write --enable-ideapad-toggle-write --enable-camera-power-write
cargo run -p legion-control-ui --features gtk-ui
cargo run -p legion-control-ui -- --overview --bus-address <dbus-address>
cargo run -p legion-control-ui -- --diagnostics --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-led-state platform::ylogo=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-led-state platform::ylogo=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-ideapad-toggle fn_lock=on --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-ideapad-toggle fn_lock=on --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-ideapad-toggle camera_power=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-ideapad-toggle camera_power=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-ideapad-toggle usb_charging=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-ideapad-toggle usb_charging=off --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-gpu-mode hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-fan-preset balanced-daily --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-restore-auto-fan --bus-address <dbus-address>
cargo run -p legion-control-ui -- --gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-gpu-mode-pending hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --clear-gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-ui -- --capture-last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-ui -- --fan-curve-live --bus-address <dbus-address>
cargo run -p legion-control-tray -- --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
cargo run -p legion-control-tray -- --desktop-check
cargo run -p legion-control-tray -- --menu-check --bus-address <dbus-address>
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 15 --report-dir target/smoke/statusnotifier-<desktop>-<date>
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-runtime-capture
```

### Execute-mode harness evidence (for broadening writes)

**Plan-only** runs (`scripts/capture-write-validation-report.sh --output …` without `--execute`) never perform hardware writes; they are what CI and fixtures use.

**Evidence** for enabling additional live write paths means capturing a **reviewed bundle** from a real Legion machine where the **installed root daemon** has the **specific** write flag(s) turned on for the control under test, then:

1. Run once per control family, for example:

   ```bash
   scripts/capture-write-validation-report.sh \
     --output target/validation/<machine>-<control>-live \
     --execute \
     --execute-only <control_id> \
     --system-bus
   ```

   Use `control_id` values from `docs/live-write-validation.md` (for example
   `platform_profile`, `battery_charge_type`, `platform::ylogo`, `fn_lock`,
   `camera_power`, `usb_charging`). Omitting `--execute-only` still works but
   emits a stderr reminder: prefer one family per bundle for reviewable PR
   evidence.

2. Inspect `validation-report.md`, `validation-report.json`, CLI transcripts, and diagnostics inside that directory; keep the tree as the artifact you reference in PRs or release notes.

3. Prefer **one** write family per capture (matching daemon flags), confirm read-back/rollback behavior, then disable flags again if you are done testing.

If you cannot use the system bus, pass `--bus-address` to a session daemon that was started with the same write flags (advanced). Do not enable high-risk fan or GPU execution flags until narrower captures exist and maintainers agree.

## CI policy

Do not turn GitHub CI off completely yet. Use local CI before pushing, then keep GitHub CI as the clean-checkout and remote-runner guard. If CI minutes become a real problem while private, reduce triggers before disabling it.

## Working process

- Read `AGENTS.md`, **`docs/session-handoff.md`**, and current `git status` before editing (open **`docs/session-handoff-archive.md`** only when you need the long log or full command list).
- Act as orchestrator for the session: form the slice plan, delegate independent audits or bounded implementation work to agents when useful, keep the critical path local, then integrate and review agent results.
- Prefer multiple agents when there are independent questions or disjoint write scopes, such as roadmap selection, fixture audit, UI tests, packaging/docs review, or separate crate changes.
- When spawning worker agents, state exact file ownership, remind them that others may edit the repo, and require changed file paths in their final answer.
- Keep responses under 500 words when possible and put long artifacts in files.
- Follow context-mode routing from `AGENTS.md`: no `curl`, no `wget`, no inline HTTP fetches, and route large command/search/file-analysis output through context-mode.
- Use `rg`/`rg --files` for local search, and use parallel reads where useful.
- Use `apply_patch` for manual file edits.
- Treat each roadmap slice as one implementation unit.
- Validate with focused checks plus `./scripts/ci-local.sh` before committing.
- Update `README.md`, `docs/feature-roadmap.md`, `docs/implementation-plan.md`, and **`docs/session-handoff.md`** when progress or next tasks change (append to **`docs/session-handoff-archive.md`** only if you want a long audit line).
- Commit each completed slice separately with a short imperative message.
- Do not end a slice half-finished: implement, validate, update progress docs, and commit.
