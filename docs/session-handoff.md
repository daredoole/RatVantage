# Session Handoff

This file is public-safe maintainer context for continuing RatVantage work. Keep detailed private notes, local machine state, and raw validation evidence outside the repository.

## Current Focus

- Keep public beta docs accurate and sanitized.
- Continue evidence-first hardware work through validators, polkit gates, rollback or reset behavior, fixture tests, and live validation bundles.
- Keep fan curve execution and unpromoted GPU runtime switching plan-only until the required Linux surfaces and recovery evidence exist.
- Desktop `power-saver` can now route through a `desktop_power_profile` automation rule to a validated hardware profile; execution still requires the normal daemon write flags and read-back paths.
- Wi-Fi power save is now detected as `wireless_power` and has a gated `SetWifiPowerSave` daemon write path for profile actions; live use requires rebuilding/restarting the daemon with `--enable-wifi-power-save-write`.
- Fn+Q/platform-profile changes can now route through a `platform_profile_router` automation rule so low-power, balanced, performance, and max-power each restore full CPU/GPU/Wi-Fi tuning instead of only changing firmware `platform_profile`.
- User-facing platform-profile changes from the GTK UI, tray, and CLI synchronize the matching desktop power profile (`low-power` → `power-saver`, `balanced` → `balanced`, performance modes → `performance`) with verified read-back and rollback. Re-selecting the active mode applies its enabled full-profile router mapping immediately, so stale CPU/GPU tuning is repaired even without a firmware transition. Custom firmware profiles intentionally leave the desktop profile unchanged.
- Cross-mode changes now move firmware first, then the desktop profile, and restore the previous firmware mode if the desktop transition fails. This avoids `amd_pstate` boost rejection when leaving low/custom mode while preserving transactional read-back behavior. Same-mode repair still updates the desktop profile before applying the mapped full profile.
- CPU max-frequency caps are first-class hardware profile actions (`cpu_max_khz`) with `SetCpuMaxFrequency`, per-policy read-back, rollback, polkit policy, and dev daemon flag `--enable-cpu-max-frequency-write`. Balanced/performance/max templates now use `cpu_max_restore`, which re-probes `cpuinfo_max_freq` after the platform-profile write instead of capturing the current mode's ceiling. Live regression testing started Performance at `3601000` kHz and restored all policies to the reported `5385280` kHz maximum.
- OpenRGB live probing is lazy: normal daemon/tray refresh avoids `openrgb --list-devices`; detailed OpenRGB probing is used only for OpenRGB-specific planning or SDK write paths so OpenRGB remains available without background probe load.
- Lenovo WMI PPT attributes (`ppt_pl1_spl`, `ppt_pl2_sppt`, `ppt_pl3_fppt`) are exposed on the live 82WM through `/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes`. Generic `/sys/firmware/acpi/platform_profile=custom` still returns `EINVAL`, but the kernel's Gamezone handler accepts custom through `/sys/class/platform-profile/platform-profile-0/profile`. RatVantage now records that driver-specific custom path during probe and uses it only for `PrepareCustomThermalMode`, allowing guarded PPT writes. Saver templates add PPT 50/60/70 only when that handler and all three PPT attributes are detected.
- Keep compatibility-bundle and release-audit workflows current.
- Public-release validation now permits only generic CI container homes (`/home/ratvantage`, `/github/home`) while still rejecting personal home paths, and the rule carries inline regression assertions.
- GTK screenshot QA retries transient black X11 captures, exits cleanly if the window disappears, and has reviewed baselines for the simplified verified-state dashboard. All blocking GTK behavior, nonblank, visual, semantic safety, and D-Bus gates pass on the release candidate.
- `anyhow` is pinned to `>=1.0.103`; `cargo audit` is clean after resolving RUSTSEC-2026-0190.

## Start A Session

1. Read [AGENTS.md](../AGENTS.md).
2. Check `git status --short --branch`.
3. Pick a bounded roadmap or bugfix slice.
4. Preserve unrelated worktree changes.
5. Validate with focused tests, then broader CI when practical.

## Finish A Slice

- Run `cargo fmt --all` for code changes.
- Run focused tests for the changed area.
- Run `scripts/audit-public-release.sh` for release/docs changes.
- Run `./scripts/ci-local.sh` before publishing or opening a release PR.
- Update public docs when behavior, safety scope, or supported controls change.

## Safety Constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writers.
- No hardcoded `hwmonN`.
- No GUI or tray execution as root.
- No hardware write path without validators, polkit policy, explicit daemon flag, rollback or reset behavior, and tests.
