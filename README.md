# Legion Control for Fedora

> Project name placeholder: **Legion Control**

A Fedora-native dashboard and tray/status tool for controlling Lenovo Legion laptop power, fan, battery, GPU, and lighting features through safe Linux interfaces.

This project is designed first for the **Lenovo Legion Pro 5 16ARX8, product 82WM**, with a strict probe-driven hardware model. It does not assume every Legion exposes the same paths.

## Current status

Early planning / pre-alpha.

The documentation package defines the architecture, safety model, packaging plan, and first implementation tasks. Code should start with read-only probing before any write path is implemented.

## Supported hardware

Initial target:

- Lenovo Legion Pro 5 16ARX8
- Product type: 82WM
- Fedora 43
- Modern Linux kernel with Lenovo platform profile / WMI support

Expected confirmed controls on the target machine:

- platform profile through `/sys/firmware/acpi/platform_profile`;
- battery charge type through `/sys/class/power_supply/BAT0/charge_types`;
- fan and temperature telemetry through hwmon;
- 10-point fan curve controls through Legion hwmon nodes;
- Y-logo LED through LED sysfs;
- NVIDIA mode query/switch flow through EnvyControl when installed.

Other Lenovo Legion models may work only where the runtime probe finds compatible controls.

## Features

Planned MVP:

- Fedora-native GTK4/libadwaita dashboard.
- Optional tray/status menu.
- Root system daemon with D-Bus API.
- polkit authorization for hardware writes.
- Runtime hardware capability probing.
- Platform profile switching.
- Battery charge mode switching.
- Fan RPM and temperature overview.
- Safe fan presets.
- Y-logo LED toggle.
- EnvyControl GPU mode flow with reboot-required banner.
- Probe report for debugging.

Planned later:

- Manual fan curve editor.
- Per-profile fan presets.
- Peripheral toggles when VPC2004 paths exist.
- Advanced firmware PPT controls when `lenovo-wmi-other` attributes are actually present.

## Safety warning

This project controls real hardware behavior.

Fan curves, firmware power limits, GPU switching, and battery charging modes can affect thermals, stability, battery wear, and boot behavior. The app intentionally avoids raw WMI calls, raw EC writes, arbitrary sysfs writes, and overclocking controls.

The GUI must never run as root. Hardware writes go through a narrow, validated, polkit-gated daemon API.

## Install from source

Placeholder for development builds:

```bash
git clone https://github.com/OWNER/legion-control.git
cd legion-control
cargo build --workspace
```

Runtime installation is not defined yet. The intended release format is a Fedora RPM with separate daemon and UI subpackages.

## Development workflow

Start read-only:

```bash
cargo run -p legion-probe -- --json
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-ui
```

Write support should be added only after:

- the probe layer can identify all required paths;
- validators exist;
- polkit actions exist;
- rollback behavior is implemented;
- manual validation has been completed on the target machine.

## Screenshots

Screenshots placeholder:

```text
docs/assets/screenshots/overview.png
docs/assets/screenshots/fan-curves.png
docs/assets/screenshots/gpu-mode.png
```

## Roadmap summary

MVP:

- D-Bus daemon.
- GTK dashboard.
- Hardware probe report.
- Platform profile, battery charge type, fan telemetry, fan presets, Y-logo LED, GPU mode workflow.

Version 0.2:

- Fan curve editor.
- Better tray integration.
- Functional Fn-lock / USB charging probes if present.
- Debug bundle export.

Version 0.3:

- Camera/touchpad/USB peripheral toggles if present.
- PowerProfiles D-Bus conflict handling.
- Local automation rules.

Advanced:

- PPT/SPL/SPPT/FPPT firmware attributes only when probed and validated.

Not planned:

- Raw WMI methods.
- Raw EC writes.
- Arbitrary sysfs writer.
- CPU/GPU overclocking.
- Native keyboard RGB payload writer.

## Contributing

Useful contributions:

- probe reports from Lenovo Legion machines;
- Fedora packaging fixes;
- GTK/libadwaita UI work;
- safe Rust hardware adapter code;
- tests using fake sysfs layouts;
- documentation for recovery and validation.

Contribution rules:

- Do not add a raw sysfs write API.
- Do not hardcode `hwmonN`.
- Do not add a feature without a probe path and safe fallback.
- Do not add raw WMI/EC writes.
- Keep unsupported controls hidden, not half-enabled.

## License

License placeholder: `GPL-3.0-or-later` recommended for the app and daemon.

Confirm final licensing before importing third-party code or icons.
