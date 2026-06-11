# Security Policy

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report them privately via [GitHub's private vulnerability reporting](https://github.com/neeraip/hydra/security/advisories/new). Include:

- A description of the vulnerability and its potential impact
- Steps to reproduce or a minimal proof-of-concept
- Any suggested mitigations you have in mind

You can expect an acknowledgement within **72 hours** and a resolution or status update within **14 days**.

## Supported Versions

Only the latest release on GitHub receives security fixes. Older versions are not patched.

## Scope

Hydra is a local simulation library and CLI tool — it does not run as a networked service and holds no user credentials or sensitive data. The most relevant attack surface is **malicious `.inp` input files** that could trigger parser panics, out-of-memory conditions, or unsafe behaviour in the solver. Reports in this category are especially welcome.
