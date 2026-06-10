# Security Policy

## Supported Versions

Security fixes target the current `main` branch and any published beta release tags.

## Reporting

For vulnerabilities, unsafe hardware-write behavior, privilege-boundary issues, or leak reports, use GitHub Security Advisories if they are enabled for the repository. If advisories are not available, open a minimal issue asking for a private security contact without posting exploit details or sensitive logs publicly.

Do not publish:

- Working privilege-escalation details before a fix is available.
- Serial numbers, machine IDs, MAC addresses, private paths, or account names.
- Raw hardware captures containing sensitive or user-identifying values.

## Scope

Relevant reports include:

- GUI or tray paths that can trigger privileged writes directly.
- Daemon methods missing validation, authorization, rollback, or read-back gates.
- Raw sysfs, WMI, EC, or firmware writes outside the intended daemon policy.
- Unsafe defaults that enable hardware mutation without explicit flags.
- Repository content that leaks personal data or sensitive device evidence.

General hardware compatibility requests should use normal issues or the hardware compatibility PR template.
