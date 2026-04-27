# Repository Guidelines

This file is read automatically by Codex CLI, Claude Code, Cursor, and other AI coding agents.
For the single-page quick-start used by Claude Code specifically, see [CLAUDE.md](CLAUDE.md).

## Project Status

RatVantage is a **working Rust implementation** — not a planning document. The Cargo workspace
compiles, all tests pass, and CI is green. Do not treat planning docs in `docs/` as the source of
truth for what exists; read the code.

## Crate Map

| Crate | Path | Purpose |
|---|---|---|
| `legion-common` | `crates/legion-common/` | Shared types, capability models, serde schemas |
| `legion-probe` | `crates/legion-probe/` | Read-only sysfs/hwmon discovery CLI |
| `legion-control-daemon` | `crates/legion-daemon/` | Root daemon, D-Bus service, polkit gate |
| `legion-control-ui` | `crates/legion-ui/` | GTK4/libadwaita dashboard (feature `gtk-ui`) |
| `legion-control-tray` | `crates/legion-tray/` | StatusNotifier tray helper |
| `ratvantage-test-support` | `crates/test-support/` | Shared test fixtures and helpers |

## Build and Test Commands

```bash
cargo build --workspace                         # build everything
cargo test --workspace                          # all unit + integration tests
cargo test -p legion-control-tray               # tray crate only (fast, no system deps)
cargo clippy --all-targets --all-features -- -D warnings   # must be clean before merging
cargo fmt --all                                 # format (CI checks with --check)
./scripts/ci-local.sh                           # full local CI mirror — run before pushing
```

When adding or changing code, always run `cargo fmt --all` before committing.
CI runs `cargo fmt --all --check` and will fail on any diff.

## Development Without Hardware

All tests run against fixture sysfs trees — no real Legion hardware required:

```bash
# Probe against the confirmed 82WM fixture
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed

# Start a private-bus daemon + UI/tray session from fixture
scripts/run-local-session-app.sh --frontend status --sysfs-root tests/fixtures/sysfs-82wm-confirmed
scripts/run-local-session-app.sh --frontend menu-check
scripts/run-local-session-app.sh --frontend tray
scripts/run-local-session-app.sh --frontend ui --gsk-renderer cairo

# Tray menu diagnostic (no D-Bus daemon needed)
cargo run -p legion-control-tray -- --menu-check --bus-address <address>
```

## Live hardware (execute bundles)

Wire system D-Bus + polkit once: `scripts/install-dev-system-integration.sh`. Then
either install a dev unit: `scripts/install-dev-systemd-ratvantage.sh` (see
`--help`), or run `sudo ./target/release/legion-control-daemon` with the matching
`--enable-*-write` flags. Per-control capture commands:
[docs/live-validation-evidence-runbook.md](docs/live-validation-evidence-runbook.md).
Harness details and `control_id` table: [docs/live-write-validation.md](docs/live-write-validation.md).
Fan/GPU execution policy: [docs/fan-gpu-execution-policy.md](docs/fan-gpu-execution-policy.md).

## Coding Style

- Rust edition 2021, `rustfmt` defaults, Clippy clean (`-D warnings`).
- `snake_case` for modules/functions/fields, `PascalCase` for types/enum variants.
- Crate names: lowercase hyphenated (`legion-common`, `legion-control-tray`).
- Workspace dependencies in root `Cargo.toml`; crates use `dependency.workspace = true`.
- No `unwrap()` in library or daemon code — use `anyhow::Result` and `?`.
- Do not add `#[allow(dead_code)]` or `#[allow(unused_imports)]` to paper over unused code;
  remove the dead code instead.
- Prefer small, focused commits. Imperative subject line ≤ 72 chars, e.g.
  `Fix tray menu separator ordering` or `Add battery health row to status section`.

## Testing Guidelines

- **Never write to real `/sys`** in tests.
- Use fixtures under `tests/fixtures/sysfs-82wm-confirmed/` for sysfs layout coverage.
- Cover: platform profile parsing, battery charge type parsing, hwmon detection without
  hardcoded `hwmonN`, missing-path handling.
- Integration tests that need D-Bus use a private session bus from `ratvantage-test-support`.
- GTK smoke tests run headless under Xvfb: `xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell`.

## Pull Request Guidelines

- PR description must list: what changed, which tests cover it, any safety impact.
- Screenshots required for UI/tray visual changes.
- `./scripts/ci-local.sh` must pass locally before opening a PR.
- CI (`cargo fmt --check`, `cargo test`, `cargo clippy -D warnings`, smoke scripts) must be
  green before merging.

## Safety — Non-Negotiable

These rules are not optional. An agent that violates them should stop and ask:

1. **The GUI never runs as root.** All privileged hardware writes go through the polkit-gated
   daemon over D-Bus.
2. **No raw sysfs writes** outside the daemon. No `fs::write` to `/sys` from UI or tray code.
3. **No hardcoded `hwmonN`** paths. Use dynamic discovery via the capability registry.
4. **No raw WMI/EC writes**, overclocking controls, or firmware flashing.
5. **No Lenovo/Legion trademarks or logos** in assets or UI copy.
6. **Default to read-only.** Write paths require explicit daemon flags, validators, and
   rollback-on-readback-mismatch. Add write support only after all three exist.

## Agentic Session Workflow

For long or multi-session tasks:

- Start from `docs/session-handoff.md` — it records the latest commits, current milestone,
  next roadmap slice, and exact validation commands.
- After completing work, update `docs/session-handoff.md` with the new state before ending
  the session so the next agent/developer can pick up cleanly.
- Run `./scripts/ci-local.sh` as the final check before committing.
- Commit formatting fixes separately from logic changes when possible.
