# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Full rules:** [AGENTS.md](AGENTS.md) — canonical source. **Current tasks:** [docs/session-handoff.md](docs/session-handoff.md). **Architecture / roadmap:** [docs/architecture.md](docs/architecture.md), [docs/feature-roadmap.md](docs/feature-roadmap.md).

---

## Build & Test

```bash
cargo build --workspace
cargo test --workspace
cargo test -p legion-control-tray               # fast, no system deps
xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell  # GTK headless
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all                                 # run before every commit
./scripts/ci-local.sh                           # full local CI — run before pushing
```

Minimum rustc: **1.92** (required by gtk-rs). Dev deps (GTK4, libadwaita, dbus-daemon, xvfb-run, etc.): `scripts/install-dev-deps-fedora.sh`.

---

## Architecture

Five-process split: **probe → daemon → UI / tray / CLI**.

| Crate | Binary / lib | Role |
|---|---|---|
| `legion-common` | lib | Shared types, capability models, serde schemas |
| `legion-probe` | `legion-probe` | Read-only sysfs/hwmon discovery; outputs JSON |
| `legion-daemon` | `legion-control-daemon` | Root daemon, D-Bus service, polkit gate for all writes |
| `legion-ui` | `legion-control-ui` | GTK4/libadwaita dashboard (`--features gtk-ui`) |
| `legion-tray` | `legion-control-tray` | KDE StatusNotifier tray helper |
| `test-support` | lib | Shared fixtures, private-bus helpers for integration tests |

**Data flow:** UI/tray call D-Bus methods on the daemon → daemon runs polkit auth → daemon writes sysfs → daemon reads back to verify (rollback on mismatch). UI and tray never write `/sys` directly.

**Capability registry** (`legion-common`): all hardware paths are dynamically discovered at probe time — no hardcoded `hwmonN` indices anywhere.

**Write guard:** every write path requires a validator, an explicit daemon `--enable-*-write` flag, and rollback-on-readback-mismatch. New write support must not be added until all three exist.

---

## Development Without Hardware

All tests run against fixture sysfs trees in `tests/fixtures/sysfs-82wm-confirmed/` — no real hardware needed.

```bash
# Probe against confirmed fixture
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed

# Private-bus daemon + frontend session from fixture
scripts/run-local-session-app.sh --frontend status --sysfs-root tests/fixtures/sysfs-82wm-confirmed
scripts/run-local-session-app.sh --frontend tray
scripts/run-local-session-app.sh --frontend ui --gsk-renderer cairo
scripts/run-local-session-app.sh --frontend menu-check
```

---

## Live Hardware (one-time setup)

```bash
scripts/install-dev-system-integration.sh   # wire system D-Bus + polkit (once)
scripts/install-dev-systemd-ratvantage.sh   # install dev systemd unit (see --help)
# or: sudo ./target/release/legion-control-daemon --enable-*-write
```

Per-control capture runbook: [docs/live-validation-evidence-runbook.md](docs/live-validation-evidence-runbook.md). Write execution policy: [docs/fan-gpu-execution-policy.md](docs/fan-gpu-execution-policy.md).

---

## Safety — Non-Negotiable

1. **GUI never runs as root.** All privileged writes go through the polkit-gated daemon over D-Bus.
2. **No raw sysfs writes** from UI or tray (`fs::write` to `/sys` is forbidden outside daemon).
3. **No hardcoded `hwmonN`** paths — use dynamic capability registry.
4. **No raw WMI/EC writes**, overclocking controls, or firmware flashing.
5. **No Lenovo/Legion trademarks or logos** in assets or UI copy.
6. **Default read-only.** Write paths need validator + daemon flag + rollback test before enabling.

If unsure whether a change violates these, stop and ask.

---

## Coding Style

- Rust edition 2021, `rustfmt` defaults, Clippy clean (`-D warnings`).
- `snake_case` modules/functions/fields; `PascalCase` types/enum variants.
- Workspace deps declared in root `Cargo.toml`; crates use `dependency.workspace = true`.
- No `unwrap()` in library or daemon code — use `anyhow::Result` and `?`.
- No `#[allow(dead_code)]` or `#[allow(unused_imports)]` — remove dead code instead.
- Commits: imperative subject ≤ 72 chars (e.g. `Fix tray menu separator ordering`).

---

## Testing

- Never write to real `/sys` in tests.
- Sysfs layout tests use `tests/fixtures/sysfs-82wm-confirmed/`.
- D-Bus integration tests use private session bus from `ratvantage-test-support`.
- GTK smoke: `xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell`.

---

## PR Rules

- Description must list: what changed, which tests cover it, any safety impact.
- Screenshots required for UI/tray visual changes.
- `./scripts/ci-local.sh` must pass locally before opening a PR.
