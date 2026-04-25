# Session Handoff

## Current state

- Repository: `https://github.com/daredoole/RatVantage`
- Visibility: private for now.
- Branch: `main`
- Latest local commits:
  - `29b174f` (`Refresh runtime fixture progress docs`)
  - `f0d7b14` (`Add runtime 82WM probe fixture`)
  - `431f457` (`Clarify disabled tray autostart docs`)
  - `d10599e` (`Package disabled tray autostart placeholder`)
  - `e0ff36c` (`Add read-only tray scaffold`)
  - `22c0820` (`Add daemon dry-run planning adapters`)
- Latest known milestone: read-only pre-alpha scaffold with GTK smoke coverage, hardened packaging metadata, disabled write planning, runtime 82WM fixture coverage, and a read-only StatusNotifier tray backend.
- Rust toolchain: pinned stable in `rust-toolchain.toml`; local stable installed because GTK stack requires rustc 1.92+.

## Implemented

- Workspace crates: `legion-common`, `legion-probe`, `legion-daemon`, `legion-ui`, `legion-tray`, `ratvantage-test-support`.
- Probe fixture coverage for confirmed and runtime-captured 82WM-style sysfs paths.
- Bracketed battery `charge_types` parsing, including inferred current value when `charge_type` is absent.
- Read-only D-Bus daemon methods:
  - `GetHardwareSummary`
  - `GetCapabilities`
  - `RefreshCapabilities`
  - `GetTelemetry`
  - `GetRawProbeReport`
- UI `--status` command and optional GTK4/libadwaita shell behind `gtk-ui`.
- Read-only `legion-control-tray --status` scaffold.
- Read-only `legion-control-tray` StatusNotifier backend with dashboard, refresh, quit, and disabled write actions.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke test for the optional shell, run through Xvfb in local and GitHub CI.
- Private-bus contract tests and shared test support.
- Fedora packaging assets for systemd, D-Bus, polkit, desktop metadata, AppStream metadata, and RPM spec.
- Packaging metadata validation script wired into local and GitHub CI.
- Read-only sysfs fixture capture workflow, validated against the existing 82WM fixture in local CI.
- Disabled draft write-method contracts for platform profile and battery charge type.
- Pure validators for platform profile and battery charge type choices; no write methods are enabled.
- Validator-backed dry-run planning for platform profile and battery charge type; still no D-Bus write methods.
- Daemon-side Rust adapters for dry-run planning, tested directly while D-Bus introspection remains read-only.
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
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-ui --features gtk-ui
cargo run -p legion-control-tray -- --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-runtime-capture
```

## CI policy

Do not turn GitHub CI off completely yet. Use local CI before pushing, then keep GitHub CI as the clean-checkout and remote-runner guard. If CI minutes become a real problem while private, reduce triggers before disabling it.

## Next tasks

1. Desktop smoke test the StatusNotifier tray backend before enabling autostart.
2. Add more captured fixtures when additional supported Legion machines are available.

## Working process

- Treat each roadmap slice as one implementation unit.
- Validate with focused checks plus `./scripts/ci-local.sh` before committing.
- Update `README.md`, `docs/feature-roadmap.md`, `docs/implementation-plan.md`, and this handoff when progress or next tasks change.
- Commit each completed slice separately with a short imperative message.
- Use parallel agents for bounded audits or implementation slices when their work can run independently.

## New session prompt

Start with:

```text
Read AGENTS.md and docs/session-handoff.md first. Then inspect current git status and continue from the next task without changing safety constraints.
```

## Safety constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writer.
- No hardcoded `hwmonN`.
- No hardware writes until validators, polkit policy, rollback behavior, and manual validation exist.
