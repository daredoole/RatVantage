# Changelog

## Unreleased Beta

- Working Rust workspace with probe, common model, daemon, GTK UI, tray helper, and test-support crates.
- Polkit-gated daemon owns privileged hardware writes; UI and tray remain unprivileged.
- Runtime capability discovery replaces hardcoded hardware paths.
- GTK dashboard and StatusNotifier tray expose status, diagnostics, guarded quick actions, and recovery/drift feedback.
- Reversible write paths are gated behind explicit daemon flags and covered by validators, tests, and rollback or reset behavior.
- Higher-risk fan curve and GPU runtime paths remain plan-only or probe-only until validation evidence is sufficient.
- Fixture, private-bus, GTK smoke, tray smoke, packaging, D-Bus contract, and CI workflows are present.
