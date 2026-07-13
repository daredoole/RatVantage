# Changelog

## 0.1.1 - 2026-07-12

- Relicense current development and future releases from MIT to GPL-3.0-or-later.
- Preserve the already-published `v0.1.0` release under its original MIT license.

## 0.1.0 - 2026-07-12

- Working Rust workspace with probe, common model, daemon, GTK UI, tray helper, and test-support crates.
- Polkit-gated daemon owns privileged hardware writes; UI and tray remain unprivileged.
- Runtime capability discovery replaces hardcoded hardware paths.
- GTK dashboard and StatusNotifier tray expose status, diagnostics, guarded quick actions, and recovery/drift feedback.
- Reversible write paths are gated behind explicit daemon flags and covered by validators, tests, and rollback or reset behavior.
- Higher-risk fan curve and GPU runtime paths remain plan-only or probe-only until validation evidence is sufficient.
- Fixture, private-bus, GTK smoke, tray smoke, packaging, D-Bus contract, and CI workflows are present.
- Platform and Fedora power modes stay synchronized, and full profiles restore the CPU scaling maximum after firmware mode changes.
- The GTK overview uses a simpler verified-state dashboard and separates everyday controls from advanced tools.
- Public desktop, tray, AppStream, polkit, service, and RPM display names consistently use RatVantage.
