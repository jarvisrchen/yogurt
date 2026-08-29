# Security Policy

## Supported versions

Yogurt is pre-1.0.
Only the latest tagged release receives security fixes.
There is no long-term support branch and no backporting to older tags.

## Reporting a vulnerability

Please do not open a public GitHub issue for security vulnerabilities.

Use GitHub's private vulnerability reporting instead:
[github.com/jarvisrchen/yogurt/security/advisories/new](https://github.com/jarvisrchen/yogurt/security/advisories/new).
This opens a private draft security advisory visible only to the maintainer
until a fix is ready.

If you cannot use GitHub's advisory flow, email the maintainer address listed
on the [GitHub profile](https://github.com/jarvisrchen) instead of filing a
public issue.

Include what you found, how to reproduce it, and the potential impact.
A best-effort acknowledgment and fix timeline will follow; there is no
formal SLA at this stage of the project.

## Threat model

Yogurt runs as a single local process bound to `127.0.0.1` and gates its
HTTP and WebSocket surface behind a random per-install session token, so
anything short of local code execution or local disk access on your Mac
cannot reach it.
Provider API keys are stored in the macOS Keychain, never in plaintext
config or in the database, and the process sends no telemetry of any kind.
The trust boundary is the same as any other localhost-bound desktop tool:
whoever can reach `localhost:7878` on this machine can read your notes and
transcripts, so do not port-forward it or run it as a shared/multi-user
service.
See [docs/ARCHITECTURE.md section 9, Trust boundaries](docs/ARCHITECTURE.md#9-trust-boundaries)
for the full enforcement map.
