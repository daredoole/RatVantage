# RatVantage for Fedora

RatVantage is a Fedora-native dashboard and tray/status tool for Lenovo Legion laptop power, fan, battery, GPU, and lighting controls through safe Linux interfaces.

This beta targets the **Lenovo Legion Pro 5 16ARX8, product type 82WM** first. Runtime probing decides which controls are shown; RatVantage must not assume every Legion laptop exposes the same kernel, firmware, or userspace surfaces.

RatVantage is not affiliated with or endorsed by Lenovo.

## Beta Status

The repository is a working Rust implementation with CI coverage, fixture-backed tests, a polkit-gated daemon, GTK4/libadwaita UI, and StatusNotifier tray helper.

Currently supported or implemented:

- Read-only probe and capability registry for platform profiles, battery modes, hwmon fan/temperature telemetry, LEDs, firmware attributes, OpenRGB status, EnvyControl GPU state, CPU controls, and packaged fan presets.
- Privileged D-Bus daemon for all hardware writes; the GUI and tray never run as root.
- GTK dashboard with Overview, Power, Battery, GPU, Fans, Devices, Automations, Settings, and Diagnostics views.
- Tray/status helper with runtime-derived status, capability summaries, refresh, diagnostics, and guarded quick actions.
- Explicitly gated reversible writes for validated controls such as platform profile, battery charge type, selected ideapad toggles, LED/RGB paths where backend evidence exists, CPU power controls, AMD GPU DPM, and hardware-profile automation.
- Plan-only or probe-only surfaces for higher-risk areas, including real fan curve execution and unpromoted GPU runtime switching.
- Fixture, private-bus, GTK smoke, tray smoke, D-Bus contract, packaging, and local CI checks.

Writes are disabled by default. A privileged daemon must be started with the matching `--enable-*-write` flags before any write path can execute.

## Supported Hardware

Initial confirmed target:

- Lenovo Legion Pro 5 16ARX8
- Product type: 82WM
- Fedora 43
- Modern Linux kernel with platform profile, power supply, hwmon, Lenovo WMI/firmware-attribute, and optional EnvyControl/OpenRGB support

Other Legion systems may work if their Linux-exposed surfaces match the probed capability model. Compatibility reports are welcome; do not include serial numbers, machine IDs, MAC addresses, private account names, or raw captures with personal data.

## Safety Model

RatVantage controls real hardware behavior. Fan curves, firmware power limits, GPU switching, CPU tuning, and battery charging modes can affect thermals, stability, battery wear, and boot behavior.

Safety rules:

- The GUI and tray never run as root.
- All privileged writes go through the daemon over D-Bus with polkit authorization.
- No raw WMI calls, raw EC writes, arbitrary sysfs writers, firmware flashing, or overclocking shortcuts.
- No hardcoded `hwmonN` paths; controls come from runtime capability discovery.
- Write support requires validators, explicit daemon flags, read-back where possible, rollback/reset behavior, tests, and live validation evidence.

## Install From Source

```bash
git clone https://github.com/daredoole/RatVantage.git
cd RatVantage
rustup toolchain install stable
./scripts/install-dev-deps-fedora.sh
./scripts/ci-local.sh
./scripts/validate-release-packaging.sh
```

RPM packaging assets exist, but beta distribution is still source-first unless a release package is published.

## Install Model

RatVantage uses a split install model:

- The GTK dashboard and tray run as the logged-in user.
- The daemon is the only privileged component and owns hardware writes over D-Bus.
- Polkit authorizes privileged write requests.
- Packaged service metadata is read-only by default and must not include `--enable-*-write` flags.

For development refreshes on a trusted local machine, use the dev installer scripts documented in
`scripts/update-dev-install.sh --help`. These scripts can update the user-session GUI/tray and,
when explicitly requested, the dev system daemon. Do not run the GUI or tray as root.

Before packaging or release, run:

```bash
scripts/validate-release-packaging.sh
scripts/smoke-install-staged.sh
```

The staged smoke check installs packaging metadata into a temporary root only. It does not modify
the host system, start services, require Legion hardware, or perform hardware writes.

## Local Development

Run the full local CI mirror before opening a PR:

```bash
./scripts/ci-local.sh
```

Useful fixture commands:

```bash
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --session --sysfs-root tests/fixtures/sysfs-82wm-confirmed
scripts/run-local-session-app.sh --frontend status
scripts/run-local-session-app.sh --frontend menu-check
scripts/run-local-session-app.sh --frontend tray
scripts/run-local-session-app.sh --frontend ui --gsk-renderer cairo
```

Useful live-system checks:

```bash
cargo run -p legion-probe -- --json
cargo run -p legion-control-ui -- --overview
cargo run -p legion-control-ui -- --diagnostics
cargo run -p legion-control-tray -- --desktop-check
```

Collect a compatibility bundle for a hardware report:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

Capture write-validation evidence only after reading the live validation docs and enabling the correct daemon flags:

```bash
scripts/capture-write-validation-report.sh --output target/validation/<machine-label>-plan
scripts/capture-write-validation-report.sh --output target/validation/<machine-label>-live --execute --system-bus
```

## Documentation

- [Architecture](docs/architecture.md)
- [Safety model](docs/safety-model.md)
- [Hardware control matrix](docs/hardware-control-matrix.md)
- [Write contracts](docs/write-contracts.md)
- [Live write validation](docs/live-write-validation.md)
- [Fixture capture](docs/fixture-capture.md)
- [Fedora packaging](docs/fedora-packaging.md)
- [Release packaging inventory](docs/release-packaging-inventory.md)
- [Release checklist](docs/release-checklist.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)

## Contributing

Useful contributions include compatibility bundles from additional Legion machines, Fedora packaging fixes, GTK/libadwaita UI work, safe Rust hardware adapter code, and tests using fake sysfs layouts.

Before contributing, read [CONTRIBUTING.md](CONTRIBUTING.md). Hardware reports must remove serial numbers, user-identifying values, private paths, and sensitive device evidence before submission.

## License

RatVantage is licensed under the MIT License. See [LICENSE](LICENSE).
