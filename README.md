# RatVantage for Fedora

> Working product name in older docs: **Legion Control**

A Fedora-native dashboard and tray/status tool for Lenovo Legion laptop power, fan, battery, GPU, and lighting features through safe Linux interfaces.

This project targets the **Lenovo Legion Pro 5 16ARX8, product 82WM** first. Runtime probing decides what is shown; the app must not assume every Legion exposes the same paths.

## Current Status

Pre-alpha implementation scaffold exists:

- Rust workspace with shared models, read-only probe, read-only daemon, UI client, and test support crates.
- Probe fixture coverage for confirmed and runtime-captured 82WM-style sysfs paths.
- Private D-Bus contract tests for read-only daemon methods.
- UI `--status`, `--overview`, and `--diagnostics` commands plus optional GTK4/libadwaita shell with a read-only diagnostics tab behind `gtk-ui`.
- Read-only tray/status helper scaffold.
- Tray dashboard launch forwards custom D-Bus addresses for private/session-bus smoke workflows.
- Tray status separates available and missing capabilities in tooltips.
- Disabled tray autostart packaging placeholder.
- Runtime-captured 82WM fixture coverage, including bracketed battery `charge_types` current-value parsing.
- Headless GTK smoke coverage for the optional shell.
- Fedora packaging metadata and validation for systemd, D-Bus, polkit, desktop, AppStream, and RPM assets.
- Read-only sysfs fixture capture workflow for adding more real hardware reports.
- Packaged read-only fan preset TOML assets with CI schema validation.
- Disabled write-method contract drafts for platform profile and battery charge type.
- Pure validators for platform profile and battery charge type choices.
- Validator-backed dry-run planning for platform profile and battery charge type.
- Read-only D-Bus dry-run planning for platform profile and battery charge type.
- Local CI script and GitHub Actions CI.

No hardware write path exists yet. Write support must wait for validators, polkit policy, rollback behavior, and manual target-machine validation.

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

Useful read-only commands:

```bash
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-runtime-capture
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-ui -- --status --bus-address <dbus-address>
cargo run -p legion-control-ui -- --overview --bus-address <dbus-address>
cargo run -p legion-control-ui -- --diagnostics --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-platform-profile performance --bus-address <dbus-address>
cargo run -p legion-control-ui -- --plan-battery-charge-type Conservation --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
cargo run -p legion-control-ui --features gtk-ui
scripts/smoke-statusnotifier-tray.sh --hold-seconds 15
```

To collect a read-only fixture from another Legion machine, use
`scripts/capture-sysfs-fixture.sh`. See [docs/fixture-capture.md](docs/fixture-capture.md).

Keep GitHub CI enabled as the clean-checkout and remote-runner guard. Local CI prevents wasted failed pushes; GitHub CI catches missing packages, toolchain drift, and workflow breakage.

## Roadmap Summary

Completed scaffold:

- Read-only probe and capability model.
- Read-only daemon D-Bus methods.
- UI status, overview, diagnostics clients, and optional GTK shell with diagnostics tab.
- Read-only tray/status helper scaffold.
- Read-only StatusNotifier tray backend with disabled write actions.
- StatusNotifier dashboard launch keeps `--bus-address` when the tray uses a private bus.
- Tray tooltip reports available and missing capability counts separately.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke coverage.
- Fedora packaging metadata and validation.
- Read-only fixture capture workflow.
- Runtime-captured 82WM fixture coverage.
- Packaged fan preset TOML assets.
- Disabled write-method contract drafts.
- Pure platform profile and battery charge type validators.
- Pure dry-run planning for future platform profile and battery charge type writes.
- Read-only daemon planning methods over D-Bus; no write methods are exposed.
- UI CLI previews for platform profile and battery charge type dry-run plans.
- Read-only diagnostics JSON bundle with hardware summary, kernel version, detected sysfs paths, recent daemon log excerpts, and raw probe report.
- Diagnostics include choice file paths for platform profiles and battery charge types.
- Read-only GTK diagnostics tab with the same debug bundle and a Copy JSON action.
- Fixture, private-bus, unit, and contract tests.
- Local and GitHub CI.

Next:

- Desktop smoke testing for the StatusNotifier tray backend before enabling autostart.
- Additional captured fixtures when more supported Legion machines are available.

See [docs/feature-roadmap.md](docs/feature-roadmap.md) and [docs/implementation-plan.md](docs/implementation-plan.md).

## Contributing

Useful contributions:

- Probe reports from Lenovo Legion machines.
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
