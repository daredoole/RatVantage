# Fixture Capture

Use fixtures to expand probe coverage without writing to real hardware. Capture
only read-only sysfs files that `legion-probe` already understands.

## Capture From Hardware

Run on the target laptop:

```bash
scripts/capture-sysfs-fixture.sh --output tests/fixtures/sysfs-<model>-<note>
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-<model>-<note>
```

Recommended fixture names:

- `sysfs-82wm-confirmed`
- `sysfs-<product-name>-minimal`
- `sysfs-<product-name>-unsupported`

Before committing a new fixture:

- Review `fixture-manifest.txt`.
- Remove serial numbers or user-identifying values if any appear.
- Keep only read-only probe inputs under `sys/`.
- Add or update focused probe tests for the new behavior.
- Run `./scripts/ci-local.sh`.

## Safety Rules

- Do not capture or commit arbitrary `/sys` trees.
- Do not add write-only controls to fixtures.
- Do not depend on stable `hwmonN` numbering.
- Treat missing paths as normal unsupported hardware.
