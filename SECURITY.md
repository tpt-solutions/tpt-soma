# Security policy

tpt-soma handles data that can identify people, so security reports are taken
seriously.

## Reporting a vulnerability

Do not open a public issue for a security problem.

Report it privately to: **[security contact: to be set by the maintainers]**

Include what you found, the steps to reproduce it, and the commit or version
affected. You will get an acknowledgement, and a fix will be scheduled before
any public disclosure.

## Scope

In scope: the capability token scheme, route authorisation, the audit ledger and
its hash chain, differential-privacy budget accounting, ingestion parsers, and
the frontend's handling of uploaded data.

The threat model is in [docs/security/threat-model.md](docs/security/threat-model.md).
Open findings listed there are known and tracked.

## Supported versions

Pre-release. Only the `master` branch receives fixes.
