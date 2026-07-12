# Hardware Compatibility Intake Plan

## Goal

Make adding another Lenovo Legion model safe and routine:

1. A user captures a narrow, read-only compatibility bundle.
2. RatVantage validates and sanitizes it.
3. Automation derives a stable device identity and capability summary.
4. A pull request adds a reviewed fixture and machine manifest.
5. CI verifies the fixture and regenerates the public compatibility tables.

The catalog must never imply that every control works merely because the laptop
boots RatVantage. Device confidence and per-control evidence are separate.

## Source of Truth

Add one reviewed manifest per product type:

```text
data/compatibility/devices/
  82wm.toml
  83df.toml
```

Suggested schema:

```toml
schema_version = 1
device_id = "82wm"
vendor = "LENOVO"
product_name = "82WM"
marketing_names = ["Legion Pro 5 16ARX8"]
status = "confirmed"
fixture = "tests/fixtures/sysfs-82wm-confirmed"
report_count = 1
first_verified = "2026-05-30"
last_verified = "2026-07-12"
fedora_versions = ["43"]
kernel_families = ["6.15"]

[capabilities.platform_profile]
status = "confirmed_read"
evidence = ["fixture"]

[capabilities.cpu_power]
status = "confirmed_write"
evidence = ["fixture", "live-readback", "rollback"]

[capabilities.fan_curves]
status = "plan_only"
reason = "No accepted live apply and rollback evidence"
```

The manifest contains only normalized conclusions and repository-relative
evidence references. Raw personal data never belongs in it.

## Status Model

### Device status

- **reported** — bundle received but not yet accepted.
- **testing** — sanitized fixture passes probe and UI/CLI smoke tests.
- **confirmed** — maintainer-reviewed fixture, real-hardware attestation, and all
  required read-only release tests pass.
- **regressed** — previously confirmed device has an unresolved compatibility
  regression.
- **unsupported** — known not to expose the minimum safe Linux interfaces.

### Capability status

- **detected** — probe sees the interface; no behavior claim.
- **confirmed_read** — captured values parse correctly and render safely.
- **confirmed_write** — model-specific live evidence proves authorization,
  validation, readback, and rollback/reset.
- **plan_only** — RatVantage can explain the operation but cannot execute it.
- **unsupported** — no approved stable Linux interface.

A device may be confirmed while individual capabilities remain plan-only or
unsupported.

## Contributor Flow

Keep the existing entry point:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

Extend it to produce a deterministic `submission.json` containing:

- capture schema version;
- DMI product type and display name;
- Fedora and kernel versions;
- normalized capability IDs and choices;
- hashes for every captured fixture file;
- explicit confirmation that capture was read-only;
- sanitizer version and result.

Two submission routes:

1. **Contributor PR** — the capture command prepares a ready-to-commit directory
   plus the existing generated PR body.
2. **Assisted issue** — a GitHub issue form accepts the small report and an
   attached bundle. A maintainer imports it locally; GitHub must never execute
   an untrusted archive from an issue.

## Maintainer Importer

Add:

```bash
scripts/import-compat-submission.sh <bundle>
```

The importer must:

1. Extract into a temporary directory.
2. Reject absolute paths, parent traversal, symlinks, device nodes, oversized
   files, unexpected paths, and unexpected file counts.
3. Run the public-release privacy patterns plus fixture-specific deny rules for
   serials, UUIDs, MAC addresses, usernames, and home paths.
4. Validate `submission.json` and `compat-report.json` schemas.
5. Re-run `legion-probe` against the fixture.
6. Require generated output to match the submitted summary.
7. Choose the canonical product-type key without trusting a user-supplied path.
8. Create or update the device manifest without automatically promoting
   capability status.
9. Regenerate the compatibility documentation.

The importer prepares files; a maintainer still reviews and commits them.

## Pull Request Contract

Enhance the hardware compatibility PR template with:

- product type and marketing name;
- new device versus additional report;
- requested device status;
- capability differences from the closest existing fixture;
- kernel/Fedora versions;
- privacy validation result;
- fixture hash result;
- manual observations;
- explicit statement that no write support is being promoted;
- separate links for any write-validation evidence.

Recommended labels:

- `hardware-report`
- `device:new`
- `device:additional-evidence`
- `compat:testing`
- `compat:confirmed-candidate`
- `needs-sanitization`
- `needs-hardware-review`

## CI Workflow

Add a path-filtered `hardware-compatibility.yml` workflow for changes under
`data/compatibility/` or `tests/fixtures/sysfs-*/`.

CI performs only read-only operations:

1. Validate every device manifest against a versioned schema.
2. Verify unique normalized device IDs and repository-relative paths.
3. Scan changed fixture files for privacy leaks and forbidden file types.
4. Confirm fixture contents use the capture allowlist.
5. Re-run `legion-probe --json` for every changed fixture.
6. Compare normalized probe output with the manifest.
7. Run focused probe, daemon private-bus, CLI, tray, and GTK smoke tests against
   the changed fixture.
8. Ensure no write capability was promoted without referenced live evidence.
9. Regenerate README/catalog output and fail on a dirty diff.
10. Upload reviewer summaries, not raw unreviewed user archives.

Use the ordinary `pull_request` event for untrusted fixture code. Do not use
`pull_request_target` to check out or execute contributor content. If automatic
labeling is added, keep it in a metadata-only workflow with minimal
`pull-requests: write` permission.

## Generated Documentation

Add:

- `docs/hardware-compatibility.md` — full generated catalog and per-capability
  matrix.
- A concise generated section in `README.md`.

README table:

| Device | Product type | Status | Reports | Confirmed controls | Testing / limitations | Last verified |
|---|---|---:|---:|---|---|---|
| Legion Pro 5 16ARX8 | 82WM | Confirmed | 1 | Platform, battery, CPU, LEDs | Fan apply plan-only | 2026-07-12 |

The generator should place markers around the README section:

```markdown
<!-- compatibility-table:start -->
...
<!-- compatibility-table:end -->
```

`scripts/generate-compatibility-docs.py --check` fails if committed output is
stale. Without `--check`, it updates both README and the detailed catalog.

## Promotion Rules

### Testing to confirmed device

Require:

- sanitized bundle;
- fixture and manifest merged;
- deterministic probe reproduction;
- CLI and GTK fixture smoke;
- maintainer review;
- contributor confirmation that displayed identity and readbacks match the
  physical machine.

### Read to write capability

Require separately for each product type and control:

- explicit daemon flag and polkit action;
- input validator;
- successful live write and readback;
- rollback/reset evidence;
- negative/mismatch test;
- documented kernel/driver surface;
- maintainer approval of the evidence bundle.

One confirmed model must never automatically promote writes on a similar model.
Shared surfaces can reduce review work, but the new model starts at detected or
confirmed-read until its own evidence is accepted.

## Implementation Slices

1. **Catalog foundation**
   - Add manifest schema, 82WM manifest, generator, and generated README table.
2. **Validation**
   - Add manifest/privacy/fixture validator and tests for malicious archives and
     stale generated output.
3. **Submission UX**
   - Add deterministic `submission.json`, importer, issue form, and improved PR
     template.
4. **CI automation**
   - Add path-filtered compatibility workflow, artifact summaries, and labels.
5. **Multi-device proof**
   - Add a second fixture through the exact public workflow before calling the
     process complete.

## Definition of Done

- A non-developer can create a safe bundle with one command.
- A maintainer can turn the bundle into a reviewable PR with one command.
- CI rejects unsafe, inconsistent, or undocumented submissions.
- README support tables are generated from reviewed manifests.
- Support is visible per device and per capability.
- No untrusted bundle gains write execution or privileged CI access.
