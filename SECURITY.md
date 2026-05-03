# Security Policy

## Supported Versions

Only the latest published version on [crates.io](https://crates.io/crates/cargo-chronoscope) receives security fixes.

| Version | Supported |
|---------|-----------|
| latest  | ✅        |
| older   | ❌        |

## Reporting a Vulnerability

**Please do not file a public GitHub issue for security vulnerabilities.**

Use GitHub's private vulnerability reporting instead:

- https://github.com/ymw0407/cargo-chronoscope/security/advisories/new

You can expect:

- An acknowledgement within **72 hours**.
- A status update within **7 days**.
- A fix released within **30 days** for confirmed, in-scope issues, or a written explanation of why a longer timeline is required.

## Scope

In scope:

- The `cargo-chronoscope` crate published on crates.io.
- The composite GitHub Action shipped from this repository.
- Any pre-built binary attached to a GitHub Release in this repository.

Out of scope:

- Vulnerabilities in third-party dependencies (please report those upstream; we will pick up the fix once it is published).
- Issues that require an attacker to already control the developer machine running `cargo-chronoscope`.
