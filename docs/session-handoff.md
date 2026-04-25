# Session Handoff

## Current state

- Repository: `https://github.com/daredoole/RatVantage`
- Visibility: private for now.
- Branch: `main`
- Latest known milestone: read-only pre-alpha scaffold.
- Rust toolchain: pinned stable in `rust-toolchain.toml`; local stable installed because GTK stack requires rustc 1.92+.

## Implemented

- Workspace crates: `legion-common`, `legion-probe`, `legion-daemon`, `legion-ui`, `ratvantage-test-support`.
- Probe fixture coverage for confirmed 82WM-style sysfs paths.
- Read-only D-Bus daemon methods:
  - `GetHardwareSummary`
  - `GetCapabilities`
  - `RefreshCapabilities`
  - `GetTelemetry`
  - `GetRawProbeReport`
- UI `--status` command and optional GTK4/libadwaita shell behind `gtk-ui`.
- Private-bus contract tests and shared test support.
- Local CI and GitHub CI.

## Commands

```bash
./scripts/install-dev-deps-fedora.sh
./scripts/ci-local.sh
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
cargo run -p legion-control-daemon -- --dry-run
cargo run -p legion-control-ui --features gtk-ui
```

## CI policy

Do not turn GitHub CI off completely yet. Use local CI before pushing, then keep GitHub CI as the clean-checkout and remote-runner guard. If CI minutes become a real problem while private, reduce triggers before disabling it.

## Next tasks

1. Add headless GTK smoke test for the optional shell.
2. Add packaging assets for Fedora/systemd/D-Bus/polkit/desktop/metainfo/RPM.
3. Expand probe fixtures from real hardware reports.
4. Draft but do not enable write-method contracts.

## Safety constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writer.
- No hardcoded `hwmonN`.
- No hardware writes until validators, polkit policy, rollback behavior, and manual validation exist.
