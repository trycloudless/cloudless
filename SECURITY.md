# Security Policy

CloudLess is a client-side encrypted backup engine: files are hashed, compressed, and encrypted on-device before upload, and the server is designed to never see plaintext data or paths. The full threat model, trust boundaries, and security invariants are documented in [`.agent/rules/threat-model.md`](.agent/rules/threat-model.md) — please read it before reporting, so you can reference the specific invariant your finding affects.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately by emailing **security@trycloudless.io** with:

- A description of the vulnerability and its potential impact
- Steps to reproduce, including affected version/commit
- Any proof-of-concept code or logs (redact any real user data)

If your finding involves cryptography (key derivation, encryption ordering, nonce reuse, etc.), please point to the specific invariant in `.agent/rules/threat-model.md` that you believe is violated — it speeds up triage significantly.

### What to expect

*(These are our current targets while the project is early-stage — if we miss them, please follow up.)*

- Acknowledgement of your report within **3 business days**
- An initial assessment (severity, whether it's accepted) within **7 business days**
- A fix timeline communicated once severity is confirmed — critical issues (e.g. plaintext exposure, authentication bypass, cross-device data leakage) are prioritized immediately

We'll credit reporters in the fix notes/changelog unless you'd prefer to stay anonymous.

## Scope

**In scope:**
- The client apps (Tauri desktop, Leptos UI) and their encryption, chunking, and storage-adapter code
- The `api_server` HTTP API and its authentication, authorization, and data-handling logic
- The `cloudless_core` client library
- The self-hosted Docker Compose deployment configuration in `deploy/self-hosted/`

**Out of scope:**
- Vulnerabilities in third-party dependencies — please report those upstream (we'll still appreciate a heads-up)
- The security of a self-hoster's own infrastructure, TLS termination, reverse proxy, or OS-level hardening — CloudLess ships the app; operators are responsible for their deployment environment
- Social engineering, physical attacks, or denial-of-service against trycloudless.io's hosted infrastructure
- Issues that require an already-compromised device or an already-known plaintext password (see [Non-Goals](.agent/rules/threat-model.md#non-goals) in the threat model)

## Safe Harbor

We support responsible disclosure. If you make a good-faith effort to:

- Avoid privacy violations, data destruction, and service disruption during your research
- Only interact with test accounts/data you control (do not access, modify, or exfiltrate other users' data)
- Give us a reasonable time to remediate before any public disclosure

...then we will not pursue legal action against you for that research.

## Supported Versions

CloudLess is pre-1.0 and does not yet maintain multiple parallel release branches. Security fixes are applied to the latest code on the default branch; self-hosted operators should track releases and update promptly once a release process is in place.
