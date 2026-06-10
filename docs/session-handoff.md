# Session Handoff

This file is public-safe maintainer context for continuing RatVantage work. Keep detailed private notes, local machine state, and raw validation evidence outside the repository.

## Current Focus

- Keep public beta docs accurate and sanitized.
- Continue evidence-first hardware work through validators, polkit gates, rollback or reset behavior, fixture tests, and live validation bundles.
- Keep fan curve execution and unpromoted GPU runtime switching plan-only until the required Linux surfaces and recovery evidence exist.
- Keep compatibility-bundle and release-audit workflows current.

## Start A Session

1. Read [AGENTS.md](../AGENTS.md).
2. Check `git status --short --branch`.
3. Pick a bounded roadmap or bugfix slice.
4. Preserve unrelated worktree changes.
5. Validate with focused tests, then broader CI when practical.

## Finish A Slice

- Run `cargo fmt --all` for code changes.
- Run focused tests for the changed area.
- Run `scripts/audit-public-release.sh` for release/docs changes.
- Run `./scripts/ci-local.sh` before publishing or opening a release PR.
- Update public docs when behavior, safety scope, or supported controls change.

## Safety Constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writers.
- No hardcoded `hwmonN`.
- No GUI or tray execution as root.
- No hardware write path without validators, polkit policy, explicit daemon flag, rollback or reset behavior, and tests.
