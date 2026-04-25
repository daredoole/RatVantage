# RatVantage for Fedora

> Working product name in older docs: **Legion Control**

A Fedora-native dashboard and tray/status tool for Lenovo Legion laptop power, fan, battery, GPU, and lighting features through safe Linux interfaces.

This project targets the **Lenovo Legion Pro 5 16ARX8, product 82WM** first. Runtime probing decides what is shown; the app must not assume every Legion exposes the same paths.

## Current Status

Pre-alpha implementation scaffold exists:

- Rust workspace with shared models, read-only probe, read-only daemon, UI client, and test support crates.
- Probe fixture coverage for confirmed and runtime-captured 82WM-style sysfs paths.
- Private D-Bus contract tests for read-only daemon methods.
- UI `--status` model and optional GTK4/libadwaita shell behind `gtk-ui`.
- Read-only tray/status helper scaffold.
- Disabled tray autostart packaging placeholder.
- Runtime-captured 82WM fixture coverage, including bracketed battery `charge_types` current-value parsing.
- Headless GTK smoke coverage for the optional shell.
- Fedora packaging metadata and validation for systemd, D-Bus, polkit, desktop, AppStream, and RPM assets.
- Read-only sysfs fixture capture workflow for adding more real hardware reports.
- Disabled write-method contract drafts for platform profile and battery charge type.
- Pure validators for platform profile and battery charge type choices.
- Validator-backed dry-run planning for platform profile and battery charge type.
- Daemon-side dry-run planning adapters that are not exposed over D-Bus.
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
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-ui -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --status --bus-address <dbus-address>
cargo run -p legion-control-tray -- --tooltip --bus-address <dbus-address>
cargo run -p legion-control-ui --features gtk-ui
```

To collect a read-only fixture from another Legion machine, use
`scripts/capture-sysfs-fixture.sh`. See [docs/fixture-capture.md](docs/fixture-capture.md).

Keep GitHub CI enabled as the clean-checkout and remote-runner guard. Local CI prevents wasted failed pushes; GitHub CI catches missing packages, toolchain drift, and workflow breakage.

## Roadmap Summary

Completed scaffold:

- Read-only probe and capability model.
- Read-only daemon D-Bus methods.
- UI status client and optional GTK shell.
- Read-only tray/status helper scaffold.
- Disabled tray autostart packaging placeholder.
- Headless GTK smoke coverage.
- Fedora packaging metadata and validation.
- Read-only fixture capture workflow.
- Runtime-captured 82WM fixture coverage.
- Disabled write-method contract drafts.
- Pure platform profile and battery charge type validators.
- Pure dry-run planning for future platform profile and battery charge type writes.
- Non-D-Bus daemon planning adapters.
- Fixture, private-bus, unit, and contract tests.
- Local and GitHub CI.

Next:

- Real tray backend selection and AppIndicator/StatusNotifier implementation.
- Real tray backend selection and AppIndicator/StatusNotifier implementation.

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
