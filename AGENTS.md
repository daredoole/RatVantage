# Repository Guidelines

## Project Structure & Module Organization

This repository is currently documentation-first and pre-alpha. The active planning materials live in `README.md`, `docs/`, and `prompts/`, especially `prompts/codex-build-kickoff.md`. The intended Rust workspace should use:

- `crates/legion-common/` for shared types, capability models, and serialization.
- `crates/legion-probe/` for read-only hardware/sysfs discovery.
- `crates/legion-daemon/` for privileged daemon behavior and D-Bus boundaries.
- `crates/legion-ui/` for the desktop UI.
- `data/` for systemd, D-Bus, polkit, desktop, metainfo, icon, and preset files.
- `packaging/rpm/` for Fedora RPM packaging.
- `tests/fixtures/` for fake sysfs trees and hardware fixtures.

Keep design notes in `docs/` and reusable implementation prompts in `prompts/`.

## Build, Test, and Development Commands

Use Cargo once the workspace is initialized:

- `cargo run -p legion-probe -- --json` runs read-only capability detection.
- `cargo run -p legion-control-daemon -- --dry-run` starts the daemon without writes.
- `cargo run -p legion-control-ui` launches the UI.
- `cargo test` runs unit and fixture-based tests.
- `cargo fmt` formats Rust code.
- `cargo clippy --all-targets --all-features` checks lint issues across crates.

## Coding Style & Naming Conventions

Format Rust with `rustfmt` and keep Clippy warnings actionable. Use `snake_case` for modules, functions, and fields; `PascalCase` for types and enum variants; and lowercase hyphenated crate names such as `legion-common`. Prefer typed structs with stable `serde` field names for external JSON or D-Bus-facing data.

## Testing Guidelines

Tests must not write to real `/sys`. Model hardware state with fixtures under `tests/fixtures/sysfs-82wm-confirmed/`. Cover platform profile parsing, battery charge type parsing, hwmon detection without hardcoded `hwmonN`, missing-path handling, and firmware attribute detection only when metadata exists.

## Commit & Pull Request Guidelines

No established git history is available yet. Use short imperative commits, for example `Add probe fixture parser` or `Document daemon dry-run flow`. Pull requests should describe the safety impact, list validation commands run, link related issues when available, and include screenshots for UI changes.

## Safety & Configuration Notes

Default to probe-first, read-only behavior. Add write support only after validators, polkit actions, rollback behavior, and manual validation exist. Do not claim official Lenovo affiliation or use Lenovo/Legion logos.
