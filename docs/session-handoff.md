# Session Handoff

## Current state

- Repository: `https://github.com/daredoole/RatVantage`
- Visibility: private for now.
- Branch: `main`
- Worktree at handoff: clean after `5318bb8`; branch ahead of `origin/main` by 59 commits.
- Global Codex config: GitHub MCP is disabled, not removed, in `/home/darrian/.codex/config.toml`. New sessions should not rely on GitHub MCP unless the user explicitly re-enables it.
- Latest local commits:
  - `HEAD` (`Surface fan state in overview`; run `git log --oneline -1` for the exact hash)
  - `187d039` (`Surface state in tray and GTK`)
  - `29fc945` (`Add fan curve state capture`)
  - `b72ff7e` (`Add GPU pending state tracking`)
  - `7ccf2c0` (`Add fan restore dry-run planning`)
  - `654cc6d` (`Record 82WM validation evidence`)
  - `8a28656` (`Add GPU rollback plan guidance`)
  - `ad4766b` (`Add appearance data to overview`)
  - `717fda9` (`Add GTK appearance page`)
  - `4341c1d` (`Add tray desktop guidance`)
  - `3dde848` (`Add disabled tray battery entries`)
  - `ab128c4` (`Add disabled tray fan preset entries`)
  - `c61cef8` (`Polish tray tooltip details`)
- Latest known milestone: read-only pre-alpha scaffold with GTK smoke coverage, hardened packaging metadata, disabled write planning, runtime/current 82WM fixture and validation evidence, diagnostics log excerpts and compact summary counts, packaged fan preset assets with dry-run planning, fan restore/default dry-run planning, app-state-only GPU pending-reboot tracking, app-state-only last-known-good fan curve capture, overview/tray/GTK state visibility, diagnostics/export parity for `gpu_mode_pending` and `last_known_good_fan_curve`, read-only StatusNotifier tray backend, tray dashboard bus-address forwarding, tray tooltip profile/fan/count details, disabled quick fan preset and battery charge tray entries, GNOME tray extension guidance, KDE StatusNotifier tooltip/menu/quit smoke evidence, report-capable KDE tray smoke bundles under `target/smoke/`, documented GNOME untested path, read-only battery overview telemetry, read-only EnvyControl GPU query, UI status/overview/diagnostics/dry-run output with LED brightness and firmware toggle values, GPU dry-run planning with reboot-required messaging and rollback guidance, diagnostics choice-source paths, per-capability status labels, GTK read-only Status, Profiles, Battery, Fans, Appearance, and Diagnostics tabs, and a compatibility bundle/PR intake workflow for outside Legion hardware submissions.
- Rust toolchain: pinned stable in `rust-toolchain.toml`; local stable installed because GTK stack requires rustc 1.92+.

## Current task

- Completed slice: KDE tray smoke is now report-capable and repeatable from the existing `scripts/smoke-statusnotifier-tray.sh` workflow.
- The smoke script accepts `--report-dir` and writes:
  - `environment.txt`
  - `watcher-counts.txt`
  - `watcher-protocol.txt`
  - `watcher-items.txt`
  - `tray-status.txt`
  - `tray-tooltip.txt`
  - `item-properties.txt`
  - `smoke-report.md`
- Recorded local KDE Wayland report bundle path: `target/smoke/statusnotifier-kde-wayland-2026-04-25`.
- This remains a read-only slice. No hardware writes, no new mutation paths, and no safety-constraint changes were introduced.
- Next recommended task from the updated roadmap: keep autostart disabled until GNOME-with-extension smoke exists; otherwise continue with read-only tray/UI polish and wait for outside hardware submissions through the compatibility bundle flow.

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
- Tray/status output, UI `--overview`, and GTK Status/Fans pages surface the durable GPU pending and saved fan curve state.
- UI `--overview` command for platform profile, battery charge type, fan RPM, temperatures, GPU mode, durable app state, battery telemetry, LED brightness, and firmware toggle values.
- UI `--diagnostics` command for a read-only JSON debug bundle containing hardware summary, compact capability/sensor/fan/path counts, kernel version, detected sysfs paths, durable app-state fields `gpu_mode_pending` and `last_known_good_fan_curve`, recent daemon log excerpts, and raw probe report.
- Platform profile and battery charge type models include both current-value paths and choice-list paths for diagnostics.
- UI status output includes per-capability status and risk labels.
- Optional GTK read-only Profiles and Battery tabs render the same diagnostics bundle data without write controls.
- Optional GTK read-only Fans tab renders fan telemetry, fan curve paths, and packaged preset IDs without write controls.
- Optional GTK read-only Appearance tab renders LED brightness and firmware toggle values without write controls.
- Optional GTK diagnostics tab for the same read-only hardware/debug bundle, with compact counts and Copy JSON parity for durable app-state fields.
- Packaged read-only fan preset TOML assets in `data/presets/`, validated by `scripts/validate-packaging.sh`, installed by the RPM spec, and validated at runtime for dry-run fan preset planning.
- Read-only D-Bus daemon methods:
  - `GetHardwareSummary`
  - `GetCapabilities`
  - `RefreshCapabilities`
  - `GetTelemetry`
  - `GetRawProbeReport`
  - `PlanPlatformProfileWrite`
  - `PlanBatteryChargeTypeWrite`
  - `PlanGpuModeWrite`
  - `PlanFanPresetWrite`
- UI `--status`, `--plan-platform-profile`, `--plan-battery-charge-type`, `--plan-gpu-mode`, and `--plan-fan-preset` commands, plus optional GTK4/libadwaita shell behind `gtk-ui`.
- Read-only `legion-control-tray --status` scaffold.
- Read-only `legion-control-tray` StatusNotifier backend with dashboard, refresh, quit, and disabled write actions.
- Tray menu shows packaged fan preset entries, all disabled while write methods remain absent.
- Tray menu shows common battery charge type entries, all disabled while write methods remain absent.
- StatusNotifier tray dashboard launch forwards `--bus-address` when the tray runs against a private/session bus.
- Tray tooltip reports current platform profile, fan RPM, and available/missing capability counts.
- StatusNotifier tray smoke script and manual checklist; autostart is still disabled.
- KDE Plasma Wayland StatusNotifier smoke passed with fixture daemon: registration, screenshot capture, tooltip properties, read-only menu export, refresh, quit, and disabled write actions were verified.
- KDE Plasma Wayland StatusNotifier smoke can now emit reusable report bundles with watcher counts, raw item properties, tray status/tooltip output, and environment metadata under `target/smoke/`.
- GNOME AppIndicator extension path is intentionally untested for now: GNOME Shell and the extension are installed, but the active graphical session is KDE Wayland. Keep tray autostart disabled.
- Tray startup emits GNOME AppIndicator/KStatusNotifier extension guidance when the desktop session reports GNOME.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke test for the optional shell, run through Xvfb in local and GitHub CI.
- Private-bus contract tests and shared test support.
- Fedora packaging assets for systemd, D-Bus, polkit, desktop metadata, AppStream metadata, and RPM spec.
- Packaging metadata validation script wired into local and GitHub CI.
- Read-only sysfs fixture capture workflow, validated against the existing 82WM fixture in local CI.
- Read-only compatibility bundle workflow via `scripts/capture-compat-report.sh`, validated against the existing 82WM fixture in local and GitHub CI.
- Hardware compatibility PR template in `.github/PULL_REQUEST_TEMPLATE/hardware-compatibility.md`.
- Disabled draft write-method contracts for platform profile, battery charge type, GPU mode, and fan presets.
- Pure validators for platform profile, battery charge type, EnvyControl GPU mode, and packaged fan preset choices; no write methods are enabled.
- Validator-backed dry-run planning for platform profile, battery charge type, GPU mode, and fan presets; read-only D-Bus planning methods are exposed, but no write methods are enabled.
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
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-ui --features gtk-ui
cargo run -p legion-control-ui -- --overview --bus-address <dbus-address>
cargo run -p legion-control-ui -- --diagnostics --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-gpu-mode hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-fan-preset balanced-daily --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-restore-auto-fan --bus-address <dbus-address>
cargo run -p legion-control-ui -- --gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-gpu-mode-pending hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --clear-gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-ui -- --capture-last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-tray -- --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
scripts/smoke-statusnotifier-tray.sh --bus-address "$DBUS_SESSION_BUS_ADDRESS" --hold-seconds 15 --report-dir target/smoke/statusnotifier-<desktop>-<date>
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-runtime-capture
```

## CI policy

Do not turn GitHub CI off completely yet. Use local CI before pushing, then keep GitHub CI as the clean-checkout and remote-runner guard. If CI minutes become a real problem while private, reduce triggers before disabling it.

## Next tasks

1. Collect additional captured fixtures through the compatibility bundle workflow when outside Legion submissions become available.
2. Keep tray autostart disabled until GNOME-with-extension smoke exists; KDE smoke is no longer the blocker.
3. If no new hardware reports are available, continue with read-only KDE/UI tray polish.
4. Keep progress docs current after each completed roadmap slice.
5. Keep all hardware mutation disabled until the safety checklist below is satisfied.

## Working process

- Read `AGENTS.md`, this handoff, and current `git status` before editing.
- Act as orchestrator for the session: form the slice plan, delegate independent audits or bounded implementation work to agents when useful, keep the critical path local, then integrate and review agent results.
- Prefer multiple agents when there are independent questions or disjoint write scopes, such as roadmap selection, fixture audit, UI tests, packaging/docs review, or separate crate changes.
- When spawning worker agents, state exact file ownership, remind them that others may edit the repo, and require changed file paths in their final answer.
- Keep responses under 500 words when possible and put long artifacts in files.
- Follow context-mode routing from `AGENTS.md`: no `curl`, no `wget`, no inline HTTP fetches, and route large command/search/file-analysis output through context-mode.
- Use `rg`/`rg --files` for local search, and use parallel reads where useful.
- Use `apply_patch` for manual file edits.
- Treat each roadmap slice as one implementation unit.
- Validate with focused checks plus `./scripts/ci-local.sh` before committing.
- Update `README.md`, `docs/feature-roadmap.md`, `docs/implementation-plan.md`, and this handoff when progress or next tasks change.
- Commit each completed slice separately with a short imperative message.
- Do not end a slice half-finished: implement, validate, update progress docs, and commit.

## New session prompt

Start with:

```text
Read AGENTS.md and docs/session-handoff.md first. Act as the orchestrator: inspect current git status, identify the next roadmap slice, spawn agents for independent audits or bounded work when useful, keep the critical path local, integrate results, validate, update docs, and commit. Continue from the latest committed state without changing safety constraints. Do not stop at planning unless blocked.
```

## Safety constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writer.
- No hardcoded `hwmonN`.
- No hardware writes until validators, polkit policy, rollback behavior, and manual validation exist.
