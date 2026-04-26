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

## Compatibility PR Bundle

For outside contributors or one-off hardware submissions, prefer the wrapper:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

The bundle contains:

- `fixture/` with the narrow read-only sysfs snapshot.
- `probe.json` from `cargo run -p legion-probe -- --json --sysfs-root <fixture>`.
- `compat-report.json` with structured machine and capability summary data.
- `compat-report.md` with a reviewer-friendly markdown summary.
- `pull-request-body.md` with ready-to-paste PR text.

Recommended contributor flow:

1. Run `scripts/capture-compat-report.sh --output compat/<machine-label>` on the target Legion laptop.
2. Review `compat/<machine-label>/fixture/fixture-manifest.txt`.
3. Remove serial numbers or user-identifying values if any appear in the bundle.
4. Copy the bundle into the repo, usually under `tests/fixtures/sysfs-<product-name>-<note>/`.
5. Paste `pull-request-body.md` into a PR, or use `.github/PULL_REQUEST_TEMPLATE/hardware-compatibility.md` as the checklist.
6. Run `./scripts/ci-local.sh` before pushing the fixture PR.

The wrapper still uses the same read-only capture paths as `scripts/capture-sysfs-fixture.sh`.
It does not add new hardware access, write paths, or broad sysfs scraping.

## Safety Rules

- Do not capture or commit arbitrary `/sys` trees.
- Do not add write-only controls to fixtures.
- Do not depend on stable `hwmonN` numbering.
- Treat missing paths as normal unsupported hardware.
