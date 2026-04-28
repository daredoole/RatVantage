# Session handoff

**Token discipline:** read **[AGENTS.md](../AGENTS.md)** for crate map, build/test/lint, safety rules, and PR workflow. This file stays **short** (next tasks + prompt + safety). Long completed-slice log, full implemented inventory, and extended CLI list live in **[session-handoff-archive.md](session-handoff-archive.md)** — open only when you need that depth.

## Repo snapshot

- Repository: `https://github.com/daredoole/RatVantage` (private for now).
- Branch: `main`.
- Before editing: `git status --short --branch` and `git log --oneline -5`.
- Toolchain: `rust-toolchain.toml`; GTK needs a recent stable rustc (see toolchain file).

## Milestone (compact)

Pre-alpha: Rust workspace builds with CI green; polkit-gated daemon + GTK + StatusNotifier tray; reversible writes for platform profile, battery charge type, ylogo LED, ideapad toggles (`fn_lock`, `camera_power`, `usb_charging`); fan and GPU surfaces remain **plan / preview only** (see [fan-gpu-execution-policy.md](fan-gpu-execution-policy.md)). GTK UI refactored into modular subpages. Live execute evidence: [live-validation-evidence-runbook.md](live-validation-evidence-runbook.md), [live-write-validation.md](live-write-validation.md).

## Next tasks

1. Run the live write-validation harness in execute mode on supported Legion hardware, **one control at a time**, before broadening the live write surface again.
2. If the KDE Wayland/NVIDIA GTK black-window issue recurs, treat it as a frontend/compositor bug: keep tray/CLI validation via `scripts/run-local-session-app.sh` while isolating the renderer path.
3. Keep tray autostart disabled until GNOME-with-extension smoke exists; KDE smoke is not the blocker.
4. GTK Fans manual curve work stays **after** planning controls have fixture/live evidence; fan preset execution stays disabled until policy and evidence gates are met.
5. Keep `docs/feature-roadmap.md` / `docs/implementation-plan.md` aligned when scope changes.
6. Do not enable higher-risk hardware mutation until validators, polkit, rollback, and manual validation exist.

## When you finish a slice

- Run `./scripts/ci-local.sh`, commit, **update this file** (Next tasks / milestone if needed).
- Optional audit trail: append one line under **Completed slice log** in [session-handoff-archive.md](session-handoff-archive.md).

## New session prompt

Start with:

```text
Read AGENTS.md and docs/session-handoff.md first. Open docs/session-handoff-archive.md only if you need the long completed-slice log or full command inventory. Act as orchestrator: inspect git status, pick the next roadmap slice, delegate bounded work when useful, validate with ./scripts/ci-local.sh, update docs, commit. Do not change safety constraints.
```

## Safety constraints

- No raw WMI calls.
- No raw EC writes.
- No arbitrary sysfs writer.
- No hardcoded `hwmonN`.
- No hardware writes until validators, polkit policy, rollback behavior, and manual validation exist.
