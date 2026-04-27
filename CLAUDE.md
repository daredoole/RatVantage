# RatVantage

Fedora-native Lenovo Legion hardware control — daemon + GTK4 dashboard + StatusNotifier tray.
Rust workspace. The GUI never runs as root; all hardware writes go through a polkit-gated daemon.

## Crates

- `legion-common` — shared types only, no side effects
- `legion-probe` — read-only sysfs/hwmon discovery CLI
- `legion-control-daemon` — root daemon, D-Bus service (`org.ratvantage.LegionControl1`), polkit gate
- `legion-control-ui` — GTK4 dashboard (feature `gtk-ui`) + CLI status/diagnostics client
- `legion-control-tray` — StatusNotifier tray helper
- `ratvantage-test-support` — shared D-Bus fixture helpers for integration tests

## Key Commands

```bash
cargo test --workspace                          # run all tests (no hardware needed)
cargo test -p legion-control-tray               # tray only — fast, no system deps
cargo fmt --all                                 # format before committing (CI checks --check)
cargo clippy --all-targets --all-features -- -D warnings   # must be clean
./scripts/ci-local.sh                           # full local CI — run before pushing
```

## Testing Without Hardware

Tests use fixture sysfs trees, not real hardware:

```bash
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed
scripts/run-local-session-app.sh --frontend menu-check   # tray menu via private bus
scripts/run-local-session-app.sh --frontend status       # daemon status via private bus
```

Primary fixture: `tests/fixtures/sysfs-82wm-confirmed/`

## Live hardware (execute bundles)

`scripts/install-dev-system-integration.sh`, then `scripts/install-dev-systemd-ratvantage.sh`
or a foreground daemon; step-by-step: [docs/live-validation-evidence-runbook.md](docs/live-validation-evidence-runbook.md).
Policy: [docs/fan-gpu-execution-policy.md](docs/fan-gpu-execution-policy.md).

## Non-Negotiable Safety Rules

- GUI never runs as root
- All sysfs writes go through the polkit-gated daemon — nowhere else
- No hardcoded `hwmonN` paths — use dynamic discovery
- No raw WMI/EC writes or overclocking controls
- Default to read-only; write paths need validators + rollback before enabling

## Before You Commit

1. `cargo fmt --all` — CI will reject any formatting diff
2. `cargo test --workspace` — all tests green
3. `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings

## Detailed Docs

- `AGENTS.md` — full coding style, PR rules, agentic workflow
- `docs/session-handoff.md` — current implementation state and next milestone
- `docs/architecture.md` — D-Bus API, process boundaries, daemon design
- `docs/feature-roadmap.md` — what's done, what's next
