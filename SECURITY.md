# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in zerobox, please report it responsibly.

**Do not open a public issue.** Instead, email security concerns to the maintainers via [GitHub private vulnerability reporting](https://github.com/afshinm/zerobox/security/advisories/new).

We will acknowledge your report within 48 hours and provide a timeline for a fix.

## Scope

zerobox relies on OS-level sandboxing mechanisms:

- **macOS**: Apple's Seatbelt (`sandbox-exec`)
- **Linux**: Bubblewrap + Seccomp + Linux namespaces
- **Windows**: Restricted tokens + ACLs + Windows Firewall

The sandboxing crates are vendored from [OpenAI Codex](https://github.com/openai/codex) (`codex-rs`). Vulnerabilities in the upstream sandboxing implementation should also be reported to OpenAI.

## Known Limitations

- GUI applications (Chrome, Firefox) cannot be sandboxed due to macOS WindowServer requirements.
- On Ubuntu 24.04+, unprivileged user namespaces are disabled by default via AppArmor. Users must enable them for Linux sandboxing to work.
- The `--allow-read` flag with Minimal platform defaults exposes system paths like `/etc` on macOS.
