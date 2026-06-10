# Release Checklist

Use this checklist before tagging or publishing a RatVantage package.

## Required Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-targets --all-features`
- [ ] `scripts/qa-gui.sh`
- [ ] `scripts/ci-local.sh`
- [ ] `scripts/validate-release-packaging.sh`
- [ ] Review `target/qa-report/review.md`
- [ ] Confirm visual, semantic UI, and D-Bus baselines are unchanged or intentionally reviewed.

## Packaging Safety

- [ ] GUI and tray launchers do not use `sudo`, `pkexec`, or root services.
- [ ] Packaged systemd unit starts only `legion-control-daemon`.
- [ ] Packaged systemd and D-Bus service files do not enable `--enable-*-write` flags by default.
- [ ] Polkit policy validates and every write action requires `auth_admin_keep`.
- [ ] D-Bus service name, systemd `BusName`, and daemon interface name match.
- [ ] Staged install smoke passes without modifying the host system.
- [ ] Uninstall/stale-service cleanup behavior is reviewed for the target package manager.

## User Experience

- [ ] Unsupported-hardware first run remains read-only and explains unavailable controls.
- [ ] Diagnostics identify probed and unavailable surfaces.
- [ ] README/install instructions match the package being released.
- [ ] OpenRGB helper instructions are present only for already implemented helper paths.
- [ ] Known release risks are recorded.

## Release Mechanics

- [ ] Version numbers are updated where applicable.
- [ ] Changelog entry is complete.
- [ ] RPM spec parses with `rpmspec -P` when RPM tooling is available.
- [ ] Package contents are inspected before publication.
- [ ] Tag and release notes mention hardware-write safety defaults.
