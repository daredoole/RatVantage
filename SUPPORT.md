# Support

RatVantage is beta software. Fedora source builds and the checked-in fixture workflow are the primary supported paths.

## Before Filing an Issue

Run:

```bash
cargo run -p legion-probe -- --json
cargo run -p legion-control-ui -- --overview
cargo run -p legion-control-ui -- --diagnostics
```

For hardware compatibility reports, prefer:

```bash
scripts/capture-compat-report.sh --output compat/<machine-label>
```

Review generated files before sharing them.

## What To Include

- Fedora version and kernel version.
- RatVantage commit or release tag.
- Whether the issue is probe-only, daemon, GTK UI, tray, packaging, or write-validation related.
- Sanitized command output or a sanitized compatibility bundle.

Do not include serial numbers, machine IDs, MAC addresses, account names, personal paths, private logs, or unrelated system information.
