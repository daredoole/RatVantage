# Contributing

Thanks for helping make RatVantage safer and more useful on Linux.

## Development Setup

RatVantage is a Rust workspace. Fedora is the primary development target.

```bash
rustup toolchain install stable
./scripts/install-dev-deps-fedora.sh
cargo build --workspace
cargo test --workspace
```

For local UI/tray work without installing a system daemon, use the private session launcher:

```bash
scripts/run-local-session-app.sh --frontend status
scripts/run-local-session-app.sh --frontend menu-check
scripts/run-local-session-app.sh --frontend tray
scripts/run-local-session-app.sh --frontend ui --gsk-renderer cairo
```

## Required Checks

Run these before opening a pull request:

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
scripts/audit-public-release.sh
```

Run `./scripts/ci-local.sh` before larger changes or release work.

## Safety Rules

- Do not add raw sysfs write APIs outside the daemon.
- Do not hardcode `hwmonN` paths.
- Do not add raw WMI calls, raw EC writes, firmware flashing, or overclocking shortcuts.
- Do not expose unsupported controls in UI/tray shortcuts.
- Write support must include validators, explicit daemon flags, polkit gating, rollback or reset behavior, and tests.
- Tests must never write to real `/sys`; use fixtures under `tests/fixtures/`.

## Hardware Reports

Compatibility reports are valuable, but they must be sanitized before submission.

Remove:

- Serial numbers, UUIDs, machine IDs, MAC addresses, and account names.
- Personal home paths and local filesystem layouts.
- Private logs, sensitive device evidence, or unrelated system information.

Prefer the supported bundle workflow:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

Review generated files before attaching them to an issue or pull request.

## Pull Requests

Describe:

- What changed.
- Which tests cover it.
- Any safety impact.
- Whether hardware evidence is fixture-only, plan-only, or live validated.

UI and tray visual changes should include screenshots when practical.
