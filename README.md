# RatVantage for Fedora

> Working product name in older docs: **Legion Control**

A Fedora-native dashboard and tray/status tool for Lenovo Legion laptop power, fan, battery, GPU, and lighting features through safe Linux interfaces.

This project targets the **Lenovo Legion Pro 5 16ARX8, product 82WM** first. Runtime probing decides what is shown; the app must not assume every Legion exposes the same paths.

## Current Status

Pre-alpha implementation scaffold exists:

- Rust workspace with shared models, read-only probe, read-only daemon, UI client, and test support crates.
- Probe fixture coverage for confirmed and runtime-captured 82WM-style sysfs paths.
- Private D-Bus contract tests for read-only daemon methods.
- UI `--status`, `--overview`, and `--diagnostics` commands plus optional GTK4/libadwaita shell with Status, Profiles, Battery, Fans, Appearance, and Diagnostics tabs behind `gtk-ui`; the Profiles and Battery tabs now expose gated quick-apply controls with inline execution feedback.
- Tray/status helper with a state-driven menu derived from detected profile choices, battery charge choices, packaged fan presets, pending runtime state, and gated quick actions for reversible profile and charge-type writes.
- Tray dashboard launch forwards custom D-Bus addresses for private/session-bus smoke workflows.
- Tray status separates available and missing capabilities in tooltips.
- KDE StatusNotifier smoke can emit a reusable report bundle with environment, watcher, and tray summary data.
- `legion-control-tray --desktop-check` reports desktop/session state, watcher availability, and autostart gating for read-only tray diagnostics.
- `legion-control-tray --menu-check` prints the same derived tray menu, including quick-action entries, so private-bus tests and smoke bundles can verify the exact runtime menu content.
- UI status output includes per-capability status and risk labels.
- Disabled tray autostart packaging placeholder.
- Runtime-captured 82WM fixture coverage, including bracketed battery `charge_types` current-value parsing.
- Headless GTK smoke coverage for the optional shell.
- Fedora packaging metadata and validation for systemd, D-Bus, polkit, desktop, AppStream, and RPM assets.
- Read-only sysfs fixture capture workflow for adding more real hardware reports.
- Read-only compatibility bundle workflow for outside Legion contributors, including generated probe summaries and PR body text.
- Read-only validation evidence for the current 82WM target is recorded in `docs/implementation-plan.md`.
- Packaged read-only fan preset TOML assets with CI schema validation and dry-run planning.
- Disabled write-method contract drafts for platform profile, battery charge type, GPU mode, fan presets, and fan restore/default.
- Pure validators for platform profile, battery charge type, EnvyControl GPU mode, and packaged fan preset choices.
- Validator-backed dry-run planning for platform profile, battery charge type, GPU mode, fan presets, and fan restore/default.
- Gated platform-profile and battery charge type execution paths with rollback-on-readback-mismatch coverage; the daemon now uses real `pkcheck` caller authorization and still blocks execution by default unless the write flags are enabled.
- Read-only D-Bus dry-run planning for GPU mode, fan presets, and fan restore/default.
- GPU dry-run plans include reboot-required metadata and rollback guidance; execution remains disabled.
- App-state-only GPU pending-reboot tracking in `/var/lib/legion-control/state.toml`; no hardware writes are performed.
- App-state-only last-known-good fan curve capture in the same TOML state file; no fan writes are performed.
- Tray/status output and GTK pages surface pending GPU reboot state and saved fan curve snapshots.
- Read-only diagnostics/export surfaces now include the same durable app-state fields, including `gpu_mode_pending` and `last_known_good_fan_curve`.
- Local CI script and GitHub Actions CI.

Only reversible platform-profile and battery charge-type execution paths exist so far, and both remain disabled by default unless the daemon is started with their explicit enable flags. Higher-risk write support must still wait for validators, rollback behavior, and manual target-machine validation.

For continuation work, start from [docs/session-handoff.md](docs/session-handoff.md). It records the latest commits, next roadmap slice, safety constraints, validation commands, and the expected orchestrator/agent workflow for new Codex sessions.

## Supported Hardware

Initial target:

- Lenovo Legion Pro 5 16ARX8
- Product type: 82WM
- Fedora 43
- Modern Linux kernel with Lenovo platform profile / WMI support

Expected confirmed controls include platform profile, battery charge type, hwmon fan/temperature telemetry, Legion fan curve nodes, Y-logo LED, and EnvyControl GPU mode when installed.

## Safety Warning

This project controls real hardware behavior. Fan curves, firmware power limits, GPU switching, and battery charging modes can affect thermals, stability, battery wear, and boot behavior.

The GUI must never run as root. Hardware writes will go through a narrow, validated, polkit-gated daemon API. Raw WMI calls, raw EC writes, arbitrary sysfs writes, and overclocking controls stay out of scope.

## Install From Source

```bash
git clone https://github.com/daredoole/RatVantage.git
cd RatVantage
rustup toolchain install stable
./scripts/install-dev-deps-fedora.sh
./scripts/ci-local.sh
./scripts/validate-packaging.sh
```

RPM packaging assets now exist, but release installation is not supported yet. Intended release format is Fedora RPMs with separate daemon and UI packages.

## Development Workflow

Run local CI before pushing:

```bash
./scripts/ci-local.sh
```

Useful commands:

```bash
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-runtime-capture
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --enable-platform-profile-write --enable-battery-charge-type-write
cargo run -p legion-control-ui -- --status --bus-address <dbus-address>
cargo run -p legion-control-ui -- --overview --bus-address <dbus-address>
cargo run -p legion-control-ui -- --diagnostics --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-gpu-mode hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-fan-preset balanced-daily --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-restore-auto-fan --bus-address <dbus-address>
cargo run -p legion-control-ui -- --gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --set-gpu-mode-pending hybrid --bus-address <dbus-address>
cargo run -p legion-control-ui -- --clear-gpu-mode-pending --bus-address <dbus-address>
cargo run -p legion-control-ui -- --last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-ui -- --capture-last-known-good-fan-curve --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
cargo run -p legion-control-tray -- --desktop-check
cargo run -p legion-control-tray -- --menu-check --bus-address <dbus-address>
cargo run -p legion-control-ui --features gtk-ui
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
```

To collect a read-only fixture from another Legion machine, use
`scripts/capture-sysfs-fixture.sh`. See [docs/fixture-capture.md](docs/fixture-capture.md).

To prepare a contributor-ready compatibility bundle with fixture, probe JSON,
summary markdown/JSON, and a pasteable PR body, use:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

Keep GitHub CI enabled as the clean-checkout and remote-runner guard. Local CI prevents wasted failed pushes; GitHub CI catches missing packages, toolchain drift, and workflow breakage.

## Roadmap Summary

Completed scaffold:

- Read-only probe and capability model.
- Read-only daemon D-Bus methods.
- UI status, overview, diagnostics clients, and optional GTK shell with read-only Profiles, Battery, Fans, Appearance, and Diagnostics tabs.
- Read-only tray/status helper with a state-driven menu derived from runtime profile choices, battery choices, packaged presets, and pending app state.
- Read-only StatusNotifier tray backend with dashboard, refresh, quit, and menu diagnostics that match the runtime-derived menu.
- StatusNotifier dashboard launch keeps `--bus-address` when the tray uses a private bus.
- Tray tooltip reports platform profile, fan RPM, and available/missing capability counts.
- Tray menu and `--menu-check` output expose current profile, charge type, battery telemetry, packaged preset labels, capability summaries, and pending app state without enabling writes.
- KDE StatusNotifier smoke report workflow with recorded local evidence.
- Read-only tray desktop diagnostics via `legion-control-tray --desktop-check`.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke coverage.
- Fedora packaging metadata and validation.
- Read-only fixture capture workflow.
- Read-only compatibility bundle workflow for fixture PR intake.
- Runtime-captured 82WM fixture coverage.
- Packaged fan preset TOML assets with runtime dry-run validation.
- Disabled write-method contract drafts.
- Pure platform profile, battery charge type, and EnvyControl GPU mode validators.
- Pure dry-run planning for GPU mode and fan preset writes, plus validated platform-profile and battery charge type execution paths with rollback tests.
- Daemon planning methods over D-Bus plus gated `SetPlatformProfile` and `SetBatteryChargeType` execution; higher-risk write methods are still absent.
- UI CLI previews for platform profile, battery charge type, GPU mode, and fan preset dry-run plans, plus `--set-platform-profile` and `--set-battery-charge-type` execution output.
- Read-only diagnostics JSON bundle with hardware summary, compact counts, kernel version, detected sysfs paths, recent daemon log excerpts, and raw probe report.
- Diagnostics/export parity for durable app state, so CLI `--diagnostics` output and GTK Diagnostics Copy JSON both include `gpu_mode_pending` and `last_known_good_fan_curve`.
- Read-only overview output includes durable GPU pending-reboot and saved fan curve state, plus LED brightness and firmware toggle values when exposed.
- Diagnostics include choice file paths for platform profiles and battery charge types.
- Read-only GTK Fans tab for fan telemetry, detected fan curve paths, and packaged preset IDs.
- Read-only GTK diagnostics tab with the same debug bundle and a Copy JSON action.
- Fixture, private-bus, unit, and contract tests.
- Local and GitHub CI.

Next:

- GNOME-with-extension smoke for the StatusNotifier tray backend before enabling autostart.
- External compatibility PRs from more supported Legion machines through the bundle workflow.
- Additional read-only tray/UI polish while waiting on more hardware reports.

See [docs/feature-roadmap.md](docs/feature-roadmap.md) and [docs/implementation-plan.md](docs/implementation-plan.md).

## Contributing

Useful contributions:

- Probe reports from Lenovo Legion machines.
- Compatibility bundles captured with `scripts/capture-compat-report.sh`.
- Fedora packaging fixes.
- GTK/libadwaita UI work.
- Safe Rust hardware adapter code.
- Tests using fake sysfs layouts.

Contribution rules:

- Do not add a raw sysfs write API.
- Do not hardcode `hwmonN`.
- Do not expose unsupported controls.
- Do not add raw WMI/EC writes.

## License

License placeholder: `GPL-3.0-or-later` recommended. Confirm final licensing before importing third-party code or icons.
